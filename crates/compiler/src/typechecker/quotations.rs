//! Quotation inference, capture analysis, and spawn-stack adjustment.

use crate::ast::Statement;
use crate::capture_analysis::{calculate_captures, extract_concrete_types};
use crate::types::{Effect, SideEffect, StackType, Type};
use crate::unification::{Subst, unify_types};

use super::TypeChecker;

impl TypeChecker {
    /// Infer the type of a quotation literal: run body inference in its own
    /// aux scope, then resolve it to a `Quotation` or (auto-captured)
    /// `Closure` and push it onto the stack.
    pub(super) fn infer_quotation(
        &self,
        id: usize,
        body: &[Statement],
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Save and clear expected type so nested quotations don't inherit it.
        // The expected type applies only to THIS quotation, not inner ones.
        let expected_for_this_quotation = self.expected_quotation_type.borrow().clone();
        *self.expected_quotation_type.borrow_mut() = None;

        // Save enclosing aux stack and enter quotation scope (Issue #350, #393).
        // Quotations are compiled as separate LLVM functions; each gets its own
        // aux slot table. The save/restore here means the enclosing word's aux
        // state is undisturbed by the quotation, and the quotation's aux usage
        // is tracked independently in `quotation_aux_depths` (Issue #393).
        let saved_aux = self.current_aux_stack.borrow().clone();
        *self.current_aux_stack.borrow_mut() = StackType::Empty;
        self.quotation_id_stack.borrow_mut().push(id);

        // Run the body inference and balance check inside an immediately-invoked
        // closure so we can restore scope state on every exit path — including
        // errors. Without this, an error in body inference or the balance check
        // would leave the typechecker with a corrupt scope stack and a polluted
        // aux stack, which matters for callers that inspect errors and continue.
        let body_result: Result<Effect, String> = (|| {
            // Infer the effect of the quotation body.
            //
            // If we have an expected quotation type from a combinator's signature
            // (e.g., list.fold expects [..b Acc T -- ..b Acc]), seed the body
            // inference with that input stack. Without this, the body inference
            // starts from a polymorphic row variable, and operations like >aux
            // can't pop because they don't know the type. Issue #393.
            let body_effect = if let Some(expected) = &expected_for_this_quotation {
                let expected_effect = match expected {
                    Type::Quotation(eff) => Some((**eff).clone()),
                    Type::Closure { effect, .. } => Some((**effect).clone()),
                    _ => None,
                };
                if let Some(eff) = expected_effect {
                    // Freshen to avoid row-variable name clashes with the
                    // enclosing scope.
                    let fresh = self.freshen_effect(&eff);
                    let (result, subst, effects) =
                        self.infer_statements_from(body, &fresh.inputs, false)?;
                    let normalized_start = subst.apply_stack(&fresh.inputs);
                    let normalized_result = subst.apply_stack(&result);
                    Effect::with_effects(normalized_start, normalized_result, effects)
                } else {
                    self.infer_statements(body)?
                }
            } else {
                self.infer_statements(body)?
            };

            // Verify quotation's aux stack is balanced (Issue #350).
            // Lexical scoping: every >aux inside the quotation must have a
            // matching aux> inside the same quotation.
            let quot_aux = self.current_aux_stack.borrow().clone();
            if quot_aux != StackType::Empty {
                return Err(format!(
                    "Quotation has unbalanced aux stack.\n\
                     Remaining aux stack: {}\n\
                     Every >aux must be matched by a corresponding aux> within the quotation.",
                    quot_aux
                ));
            }

            Ok(body_effect)
        })();

        // Always restore scope state, regardless of whether the body inference
        // succeeded or failed.
        *self.current_aux_stack.borrow_mut() = saved_aux;
        self.quotation_id_stack.borrow_mut().pop();
        *self.expected_quotation_type.borrow_mut() = expected_for_this_quotation.clone();

        let body_effect = body_result?;

        // Perform capture analysis
        let quot_type = self.analyze_captures(&body_effect, &current_stack)?;

        // If this is a closure, we need to pop the captured values from the stack
        // and correct the capture types from the caller's actual stack.
        let result_stack = match &quot_type {
            Type::Quotation(_) => {
                // Stateless - no captures. Record in type map for codegen.
                self.quotation_types
                    .borrow_mut()
                    .insert(id, quot_type.clone());
                current_stack.push(quot_type)
            }
            Type::Closure {
                captures, effect, ..
            } => {
                // The capture COUNT comes from analyze_captures; the capture
                // TYPES come from the caller's actual stack (see
                // pop_actual_capture_types for why we don't unify here).
                let (stack, actual_captures) =
                    self.pop_actual_capture_types(captures.len(), &current_stack)?;

                // Rebuild the closure type with the actual capture types.
                let corrected_quot_type = Type::Closure {
                    effect: effect.clone(),
                    captures: actual_captures,
                };

                // Update the type map so codegen sees the corrected types.
                self.quotation_types
                    .borrow_mut()
                    .insert(id, corrected_quot_type.clone());

                stack.push(corrected_quot_type)
            }
            _ => unreachable!("analyze_captures only returns Quotation or Closure"),
        };

        // Quotations don't propagate effects - they capture them in the quotation type
        // The effect annotation on the quotation type (e.g., [ ..a -- ..b | Yield Int ])
        // indicates what effects the quotation may produce when called
        Ok((result_stack, Subst::empty(), vec![]))
    }

    /// Pop `count` capture values off the caller's `stack`, returning the
    /// remaining stack and the popped types bottom-to-top (matching
    /// `calculate_captures`' convention).
    ///
    /// The types come from the caller's actual stack, NOT the body's
    /// inference: the body may have constrained a type variable to Int/Float
    /// via its operations (e.g. `i.+`) even when the runtime value is a
    /// Variant. `unify_types(Var, Int)` would succeed and propagate the wrong
    /// type to codegen, which would then emit `env_get_int` for a Variant — a
    /// runtime crash. Using the caller's real types ensures the correct getter.
    fn pop_actual_capture_types(
        &self,
        count: usize,
        stack: &StackType,
    ) -> Result<(StackType, Vec<Type>), String> {
        let mut stack = stack.clone();
        let mut actual_captures: Vec<Type> = Vec::new();
        for _ in 0..count {
            let (new_stack, actual_type) = self.pop_type(&stack, "closure capture")?;
            actual_captures.push(actual_type);
            stack = new_stack;
        }
        // Pop order is top-down; reverse to bottom-to-top.
        actual_captures.reverse();
        Ok((stack, actual_captures))
    }

    /// Convert a top-of-stack `Quotation` with non-empty inputs into a
    /// `Closure` for `strand.spawn` (which expects `( -- )`), capturing the
    /// values below it from the caller's stack.
    pub(super) fn adjust_stack_for_spawn(
        &self,
        current_stack: StackType,
        spawn_effect: &Effect,
    ) -> Result<StackType, String> {
        // strand.spawn expects: ( ..a Quotation(Empty -- Empty) -- ..a Int )
        // Extract the expected quotation type from strand.spawn's effect
        let expected_quot_type = match &spawn_effect.inputs {
            StackType::Cons { top, rest: _ } => {
                if !matches!(top, Type::Quotation(_)) {
                    return Ok(current_stack); // Not a quotation, don't adjust
                }
                top
            }
            _ => return Ok(current_stack),
        };

        // Check what's actually on the stack
        let (rest_stack, actual_type) = match &current_stack {
            StackType::Cons { rest, top } => (rest.as_ref().clone(), top),
            _ => return Ok(current_stack), // Empty stack, nothing to adjust
        };

        // If top of stack is a Quotation with non-empty inputs, convert to Closure
        if let Type::Quotation(actual_effect) = actual_type {
            // Check if quotation needs inputs
            if !matches!(actual_effect.inputs, StackType::Empty) {
                // Extract expected effect from spawn's signature
                let expected_effect = match expected_quot_type {
                    Type::Quotation(eff) => eff.as_ref(),
                    _ => return Ok(current_stack),
                };

                // Calculate what needs to be captured
                let captures = calculate_captures(actual_effect, expected_effect)?;

                // Create a Closure type
                let closure_type = Type::Closure {
                    effect: Box::new(expected_effect.clone()),
                    captures: captures.clone(),
                };

                // Pop the captured values from the stack
                // The values to capture are BELOW the quotation on the stack
                let mut adjusted_stack = rest_stack;
                for _ in &captures {
                    adjusted_stack = match adjusted_stack {
                        StackType::Cons { rest, .. } => rest.as_ref().clone(),
                        _ => {
                            return Err(format!(
                                "strand.spawn: not enough values on stack to capture. Need {} values",
                                captures.len()
                            ));
                        }
                    };
                }

                // Push the Closure onto the adjusted stack
                return Ok(adjusted_stack.push(closure_type));
            }
        }

        Ok(current_stack)
    }

    /// Analyze quotation captures
    ///
    /// Determines whether a quotation should be stateless (Type::Quotation)
    /// or a closure (Type::Closure) based on the expected type from the word signature.
    ///
    /// Type-driven inference with automatic closure creation:
    ///   - If expected type is Closure[effect], calculate what to capture
    ///   - If expected type is Quotation[effect]:
    ///     - If body needs more inputs than expected effect, auto-create Closure
    ///     - Otherwise return stateless Quotation
    ///   - If no expected type, default to stateless (conservative)
    ///
    /// Example 1 (auto-create closure):
    ///   Expected: Quotation[-- ]          [spawn expects ( -- )]
    ///   Body: [ handle-connection ]       [needs ( Int -- )]
    ///   Body effect: ( Int -- )           [needs 1 Int]
    ///   Expected effect: ( -- )           [provides 0 inputs]
    ///   Result: Closure { effect: ( -- ), captures: [Int] }
    ///
    /// Example 2 (explicit closure):
    ///   Signature: ( Int -- Closure[Int -- Int] )
    ///   Body: [ add ]
    ///   Body effect: ( Int Int -- Int )  [add needs 2 Ints]
    ///   Expected effect: [Int -- Int]    [call site provides 1 Int]
    ///   Result: Closure { effect: [Int -- Int], captures: [Int] }
    pub(super) fn analyze_captures(
        &self,
        body_effect: &Effect,
        _current_stack: &StackType,
    ) -> Result<Type, String> {
        // Check if there's an expected type from the word signature
        let expected = self.expected_quotation_type.borrow().clone();

        match expected {
            Some(Type::Closure { effect, .. }) => {
                // User declared closure type - calculate captures
                let captures = calculate_captures(body_effect, &effect)?;
                Ok(Type::Closure { effect, captures })
            }
            Some(Type::Quotation(expected_effect)) => {
                // Check if we need to auto-create a closure by comparing the
                // body's concrete input count against what the combinator provides.
                let body_inputs = extract_concrete_types(&body_effect.inputs);
                let expected_inputs = extract_concrete_types(&expected_effect.inputs);

                // Auto-capture triggers when the body needs more concrete inputs
                // than the expected provides. Three branches:
                // (a) Expected is empty (strand.spawn): body needs any inputs → capture all.
                // (b) Expected has concrete inputs (list.fold): body has MORE → capture excess.
                // (c) Expected has ONLY a row variable and no concrete inputs
                //     (strand.weave): don't capture, fall through to unification.
                let expected_is_empty = matches!(expected_effect.inputs, StackType::Empty);
                let should_capture = if expected_is_empty {
                    !body_inputs.is_empty()
                } else if !expected_inputs.is_empty() {
                    body_inputs.len() > expected_inputs.len()
                } else {
                    false // row-variable-only expected — don't capture, unify instead
                };

                if should_capture {
                    // Body needs more inputs than the combinator provides.
                    // The excess (bottommost) become captures; the topmost must
                    // align with what the combinator provides.
                    //
                    // Example: list.fold expects ( ..b Acc T -- ..b Acc ).
                    // Body inferred as ( ..b X Acc T -- ..b Acc ).
                    // expected_inputs = [Acc, T], body_inputs = [X, Acc, T].
                    // Captures = [X]. Topmost 2 of body must match expected's 2.
                    //
                    // Issue #395: this extends the empty-input auto-capture
                    // (used by strand.spawn) to the non-empty case.
                    let captures = calculate_captures(body_effect, &expected_effect)?;
                    Ok(Type::Closure {
                        effect: expected_effect,
                        captures,
                    })
                } else {
                    // Body has same or fewer inputs — standard unification path.
                    // This catches:
                    // - Stack pollution: body pushes values when expected is stack-neutral
                    // - Stack underflow: body consumes values when expected is stack-neutral
                    // - Wrong return type: body returns Int when Bool expected
                    let body_quot = Type::Quotation(Box::new(body_effect.clone()));
                    let expected_quot = Type::Quotation(expected_effect.clone());
                    unify_types(&body_quot, &expected_quot).map_err(|e| {
                        format!(
                            "quotation effect mismatch: expected {}, got {}: {}",
                            expected_effect, body_effect, e
                        )
                    })?;

                    // Body is compatible with expected effect - stateless quotation
                    Ok(Type::Quotation(expected_effect))
                }
            }
            _ => {
                // No expected type - conservative default: stateless quotation
                Ok(Type::Quotation(Box::new(body_effect.clone())))
            }
        }
    }
}
