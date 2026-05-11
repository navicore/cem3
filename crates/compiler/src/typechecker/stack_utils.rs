//! Stack-type utilities: depth, position lookup, rotation, pop, effect application.

use std::collections::HashMap;

use crate::builtins::builtin_signature;
use crate::types::{Effect, StackType, Type};
use crate::unification::{Subst, unify_stacks};

use super::{TypeChecker, format_line_prefix};

impl TypeChecker {
    pub(super) fn stack_depth(stack: &StackType) -> usize {
        let mut depth = 0;
        let mut current = stack;
        while let StackType::Cons { rest, .. } = current {
            depth += 1;
            current = rest;
        }
        depth
    }

    /// Check if the top of the stack is a trivially-copyable type (Int, Float, Bool)
    /// These types have no heap references and can be memcpy'd in codegen.
    pub(super) fn get_trivially_copyable_top(stack: &StackType) -> Option<Type> {
        match stack {
            StackType::Cons { top, .. } => match top {
                Type::Int | Type::Float | Type::Bool => Some(top.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Record the top-of-stack type for a statement if it's trivially copyable (Issue #186)
    pub(super) fn get_type_at_position(
        &self,
        stack: &StackType,
        n: usize,
        op: &str,
    ) -> Result<Type, String> {
        let mut current = stack;
        let mut pos = 0;

        loop {
            match current {
                StackType::Cons { rest, top } => {
                    if pos == n {
                        return Ok(top.clone());
                    }
                    pos += 1;
                    current = rest;
                }
                StackType::RowVar(name) => {
                    // We've hit a row variable before reaching position n
                    // This means the type at position n is unknown statically.
                    // Generate a fresh type variable to represent it.
                    // This allows the code to type-check, with the actual type
                    // determined by unification with how the value is used.
                    //
                    // Note: This works correctly even in conditional branches because
                    // branches are now inferred from the actual stack (not abstractly),
                    // so row variables only appear when the word itself has polymorphic inputs.
                    let fresh_type = Type::Var(self.fresh_var(&format!("{}_{}", op, name)));
                    return Ok(fresh_type);
                }
                StackType::Empty => {
                    return Err(format!(
                        "{}{}: stack underflow - position {} requested but stack has only {} concrete items",
                        self.line_prefix(),
                        op,
                        n,
                        pos
                    ));
                }
            }
        }
    }

    /// Remove the type at position n and push it on top (for roll)
    pub(super) fn rotate_type_to_top(
        &self,
        stack: StackType,
        n: usize,
    ) -> Result<(StackType, Subst), String> {
        if n == 0 {
            // roll(0) is a no-op
            return Ok((stack, Subst::empty()));
        }

        // Collect all types from top to the target position
        let mut types_above: Vec<Type> = Vec::new();
        let mut current = stack;
        let mut pos = 0;

        // Pop items until we reach position n
        loop {
            match current {
                StackType::Cons { rest, top } => {
                    if pos == n {
                        // Found the target - 'top' is what we want to move to the top
                        // Rebuild the stack: rest, then types_above (reversed), then top
                        let mut result = *rest;
                        // Push types_above back in reverse order (bottom to top)
                        for ty in types_above.into_iter().rev() {
                            result = result.push(ty);
                        }
                        // Push the rotated type on top
                        result = result.push(top);
                        return Ok((result, Subst::empty()));
                    }
                    types_above.push(top);
                    pos += 1;
                    current = *rest;
                }
                StackType::RowVar(name) => {
                    // Reached a row variable before position n
                    // The type at position n is in the row variable.
                    // Generate a fresh type variable to represent the moved value.
                    //
                    // Note: This preserves stack size correctly because we're moving
                    // (not copying) a value. The row variable conceptually "loses"
                    // an item which appears on top. Since we can't express "row minus one",
                    // we generate a fresh type and trust unification to constrain it.
                    //
                    // This works correctly in conditional branches because branches are
                    // now inferred from the actual stack (not abstractly), so row variables
                    // only appear when the word itself has polymorphic inputs.
                    let fresh_type = Type::Var(self.fresh_var(&format!("roll_{}", name)));

                    // Reconstruct the stack with the rolled type on top
                    let mut result = StackType::RowVar(name.clone());
                    for ty in types_above.into_iter().rev() {
                        result = result.push(ty);
                    }
                    result = result.push(fresh_type);
                    return Ok((result, Subst::empty()));
                }
                StackType::Empty => {
                    return Err(format!(
                        "{}roll: stack underflow - position {} requested but stack has only {} items",
                        self.line_prefix(),
                        n,
                        pos
                    ));
                }
            }
        }
    }

    /// Infer the stack effect of a sequence of statements
    /// Returns an Effect with both inputs and outputs normalized by applying discovered substitutions
    /// Also includes any computational side effects (Yield, etc.)
    pub(super) fn lookup_word_effect(&self, name: &str) -> Option<Effect> {
        // First check built-ins
        if let Some(effect) = builtin_signature(name) {
            return Some(effect);
        }

        // Then check user-defined words
        self.env.get(name).cloned()
    }

    /// Apply an effect to a stack
    /// Effect: (inputs -- outputs)
    /// Current stack must match inputs, result is outputs
    /// Returns (result_stack, substitution)
    pub(super) fn apply_effect(
        &self,
        effect: &Effect,
        current_stack: StackType,
        operation: &str,
        span: &Option<crate::ast::Span>,
    ) -> Result<(StackType, Subst), String> {
        // Check for stack underflow: if the effect needs more concrete values than
        // the current stack provides, and the stack has a "rigid" row variable at its base,
        // this would be unsound (the row var could be Empty at runtime).
        // Bug #169: "phantom stack entries"
        //
        // We only check for "rigid" row variables (named "rest" from declared effects).
        // Row variables named "input" are from inference and CAN grow to discover requirements.
        let effect_concrete = Self::count_concrete_types(&effect.inputs);
        let stack_concrete = Self::count_concrete_types(&current_stack);

        if let Some(row_var_name) = Self::get_row_var_base(&current_stack) {
            // Only check "rigid" row variables (from declared effects, not inference).
            //
            // Row variable naming convention (established in parser.rs:build_stack_type):
            // - "rest": Created by the parser for declared stack effects. When a word declares
            //   `( String Int -- String )`, the parser creates `( ..rest String Int -- ..rest String )`.
            //   This "rest" is rigid because the caller guarantees exactly these concrete types.
            // - "rest$N": Freshened versions created during type checking when calling other words.
            //   These represent the callee's stack context and can grow during unification.
            // - "input": Created for words without declared effects during inference.
            //   These are flexible and grow to discover the word's actual requirements.
            //
            // Only the original "rest" (exact match) should trigger underflow checking.
            let is_rigid = row_var_name == "rest";

            if is_rigid && effect_concrete > stack_concrete {
                let word_name = self
                    .current_word
                    .borrow()
                    .as_ref()
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| "unknown".to_string());
                return Err(format!(
                    "{}In '{}': {}: stack underflow - requires {} value(s), only {} provided",
                    self.line_prefix(),
                    word_name,
                    operation,
                    effect_concrete,
                    stack_concrete
                ));
            }
        }

        // Unify current stack with effect's input
        let subst = unify_stacks(&effect.inputs, &current_stack).map_err(|e| {
            let line_info = span
                .as_ref()
                .map(|s| format_line_prefix(s.line))
                .unwrap_or_default();
            format!(
                "{}{}: stack type mismatch. Expected {}, got {}: {}",
                line_info, operation, effect.inputs, current_stack, e
            )
        })?;

        // Apply substitution to output
        let result_stack = subst.apply_stack(&effect.outputs);

        Ok((result_stack, subst))
    }

    /// Count the number of concrete (non-row-variable) types in a stack
    pub(super) fn count_concrete_types(stack: &StackType) -> usize {
        let mut count = 0;
        let mut current = stack;
        while let StackType::Cons { rest, top: _ } = current {
            count += 1;
            current = rest;
        }
        count
    }

    /// Get the row variable name at the base of a stack, if any
    pub(super) fn get_row_var_base(stack: &StackType) -> Option<String> {
        let mut current = stack;
        while let StackType::Cons { rest, top: _ } = current {
            current = rest;
        }
        match current {
            StackType::RowVar(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Pop a value from a stack, allowing row-polymorphic refinement.
    ///
    /// Four cases:
    /// - `Cons { rest, top }` — concrete pop. Returns (rest, top, empty subst).
    /// - `RowVar("rest")` — rigid row variable from a user-declared signature
    ///   (parser convention; see `apply_effect`). The user explicitly said
    ///   "no values below this," so refining it would silently accept a
    ///   genuine underflow. Returns Err.
    /// - `RowVar(other)` — flexible row variable (inferred via "input",
    ///   freshened via "$N"). Introduces a fresh type variable T and a fresh
    ///   row variable ..a', returns (RowVar(..a'), Var(T), subst
    ///   { name → Cons(RowVar(..a'), T) }). The substitution records that
    ///   the original row variable contains at least one value on top,
    ///   propagating outward through substitution composition.
    /// - `Empty` — genuine underflow, returns Err.
    ///
    /// This enables nested combinators (dip-in-dip, etc.) to type-check
    /// when the surrounding stack is row-polymorphic — issue #471 — while
    /// still rejecting underflow at rigid declaration boundaries.
    pub(super) fn polymorphic_pop(
        &self,
        stack: StackType,
        context: &str,
    ) -> Result<(StackType, Type, Subst), String> {
        match stack {
            StackType::Cons { rest, top } => Ok((*rest, top, Subst::empty())),
            StackType::RowVar(name) if name == "rest" => Err(format!(
                "{}{}: stack underflow",
                self.line_prefix(),
                context
            )),
            StackType::RowVar(name) => {
                let fresh_type = self.fresh_var("T");
                let fresh_row = self.fresh_var(&name);
                let new_rest = StackType::RowVar(fresh_row);
                let popped = Type::Var(fresh_type);
                let refined = StackType::Cons {
                    rest: Box::new(new_rest.clone()),
                    top: popped.clone(),
                };
                let mut rows = HashMap::new();
                rows.insert(name, refined);
                let subst = Subst {
                    types: HashMap::new(),
                    rows,
                };
                Ok((new_rest, popped, subst))
            }
            StackType::Empty => Err(format!(
                "{}{}: stack underflow",
                self.line_prefix(),
                context
            )),
        }
    }

    /// Adjust stack for strand.spawn operation by converting Quotation to Closure if needed
    ///
    /// strand.spawn expects Quotation(Empty -- Empty), but if we have Quotation(T... -- U...)
    /// with non-empty inputs, we auto-convert it to a Closure that captures those inputs.
    pub(super) fn pop_type(
        &self,
        stack: &StackType,
        context: &str,
    ) -> Result<(StackType, Type), String> {
        match stack {
            StackType::Cons { rest, top } => Ok(((**rest).clone(), top.clone())),
            StackType::Empty => Err(format!(
                "{}: stack underflow - expected value on stack but stack is empty",
                context
            )),
            StackType::RowVar(_) => {
                // Can't statically determine if row variable is empty
                // For now, assume it has at least one element
                // This is conservative - real implementation would track constraints
                Err(format!(
                    "{}: cannot pop from polymorphic stack without more type information",
                    context
                ))
            }
        }
    }
}
