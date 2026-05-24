//! Word-call inference: infer_word_call, aux push/pop, call, arithmetic sugar.

use crate::types::{SideEffect, StackType, Type};
use crate::unification::Subst;

use super::TypeChecker;

/// Arithmetic-sugar operators that `resolve_arithmetic_sugar` lowers to
/// concrete `i.`/`f.`/`string.` ops based on the operand types.
const SUGAR_OPS: &[&str] = &["+", "-", "*", "/", "%", "=", "<", ">", "<=", ">=", "<>"];

impl TypeChecker {
    pub(super) fn infer_word_call(
        &self,
        name: &str,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Arithmetic sugar resolution: resolve +, -, *, / etc. to concrete ops
        // based on the types currently on the stack.
        if SUGAR_OPS.contains(&name) {
            if let Some(resolved) = self.resolve_arithmetic_sugar(name, &current_stack) {
                // Record the resolution for codegen, keyed by source location (line, column)
                if let Some(s) = span {
                    self.resolved_sugar
                        .borrow_mut()
                        .insert((s.line, s.column), resolved.clone());
                }
                // Proceed as if the user wrote the resolved name
                return self.infer_word_call(&resolved, span, current_stack);
            }
            // Sugar op but types don't match — give a helpful error.
            // When `span` is set, emit `at line N col M: ` so the LSP can
            // pinpoint the operator even on lines with multiple sugar tokens.
            let position_prefix = match span {
                Some(s) => format!("at line {} col {}: ", s.line + 1, s.column + 1),
                None => self.line_prefix(),
            };
            let (top_desc, second_desc) = {
                let top = current_stack.clone().pop().map(|(_, t)| format!("{}", t));
                let second = current_stack
                    .clone()
                    .pop()
                    .and_then(|(r, _)| r.pop().map(|(_, t)| format!("{}", t)));
                (
                    top.unwrap_or_else(|| "empty".to_string()),
                    second.unwrap_or_else(|| "empty".to_string()),
                )
            };
            let (type_options, suggestion) = match name {
                "+" => (
                    "Int+Int, Float+Float, or String+String",
                    "Use `i.+`, `f.+`, or `string.concat`.",
                ),
                "=" => (
                    "Int+Int, Float+Float, or String+String (equality)",
                    "Use `i.=`, `f.=`, or `string.equal?`.",
                ),
                "%" => (
                    "Int+Int only — float modulo is not supported",
                    "Use `i.%` for integer modulo.",
                ),
                _ => (
                    "Int+Int or Float+Float",
                    "Use the `i.` or `f.` prefixed variant.",
                ),
            };
            // When both operands are out of scope (top_desc/second_desc
            // both "empty") the most likely cause is sugar appearing
            // inside a quotation body, where the stack is empty from the
            // resolver's perspective. Lead with the typed-form suggestion
            // rather than the generic "requires matching types" wording.
            if top_desc == "empty" && second_desc == "empty" {
                return Err(format!(
                    "{}`{}` can't resolve here — operand types not in scope \
                     (this commonly happens inside a quotation body, where \
                     the body's stack is empty from the resolver's view). \
                     {}",
                    position_prefix, name, suggestion,
                ));
            }
            return Err(format!(
                "{}`{}` requires matching types ({}), got ({}, {}). {}",
                position_prefix, name, type_options, second_desc, top_desc, suggestion,
            ));
        }

        // Special handling for aux stack operations (Issue #350)
        if name == ">aux" {
            return self.infer_to_aux(span, current_stack);
        }
        if name == "aux>" {
            return self.infer_from_aux(span, current_stack);
        }

        // Special handling for `call`: extract and apply the quotation's actual effect
        // This ensures stack pollution through quotations is caught (Issue #228)
        if name == "call" {
            return self.infer_call(span, current_stack);
        }

        // Special handling for dataflow combinators
        if name == "dip" {
            return self.infer_dip(span, current_stack);
        }
        if name == "keep" {
            return self.infer_keep(span, current_stack);
        }
        if name == "bi" {
            return self.infer_bi(span, current_stack);
        }
        if name == "if" {
            return self.infer_if_combinator(span, current_stack);
        }

        // Look up word's effect
        let effect = self
            .lookup_word_effect(name)
            .ok_or_else(|| format!("Unknown word: '{}'", name))?;

        // Freshen the effect to avoid variable name clashes
        let fresh_effect = self.freshen_effect(&effect);

        // Special handling for strand.spawn: auto-convert Quotation to Closure if needed
        let adjusted_stack = if name == "strand.spawn" {
            self.adjust_stack_for_spawn(current_stack, &fresh_effect)?
        } else {
            current_stack
        };

        // Apply the freshened effect to current stack
        let (result_stack, subst) = self.apply_effect(&fresh_effect, adjusted_stack, name, span)?;

        // Propagate side effects from the called word
        // Note: strand.weave "handles" Yield effects (consumes them from the quotation)
        // strand.spawn requires pure quotations (checked separately)
        let propagated_effects = fresh_effect.effects.clone();

        Ok((result_stack, subst, propagated_effects))
    }

    /// Handle >aux: pop from main stack, push onto scope-local aux stack
    /// (Issue #350, Issue #393).
    ///
    /// In word-body scope, depth is tracked per word in `aux_max_depths`.
    /// In quotation-body scope, depth is tracked per quotation ID in
    /// `quotation_aux_depths`. Each quotation gets its own slot table at
    /// codegen time.
    pub(super) fn infer_to_aux(
        &self,
        _span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let (rest, top_type) = self.pop_type(&current_stack, ">aux")?;

        // Push onto aux stack
        let mut aux = self.current_aux_stack.borrow_mut();
        *aux = aux.clone().push(top_type);

        // Track max depth for codegen alloca sizing.
        // If we're inside a quotation, key the depth by quotation ID.
        // Otherwise, key by the enclosing word name.
        let depth = Self::stack_depth(&aux);
        let quot_stack = self.quotation_id_stack.borrow();
        if let Some(&quot_id) = quot_stack.last() {
            let mut depths = self.quotation_aux_depths.borrow_mut();
            let entry = depths.entry(quot_id).or_insert(0);
            if depth > *entry {
                *entry = depth;
            }
        } else if let Some((word_name, _)) = self.current_word.borrow().as_ref() {
            let mut depths = self.aux_max_depths.borrow_mut();
            let entry = depths.entry(word_name.clone()).or_insert(0);
            if depth > *entry {
                *entry = depth;
            }
        }

        Ok((rest, Subst::empty(), vec![]))
    }

    /// Handle aux>: pop from aux stack, push onto main stack (Issue #350, #393).
    pub(super) fn infer_from_aux(
        &self,
        _span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let mut aux = self.current_aux_stack.borrow_mut();
        match aux.clone().pop() {
            Some((rest, top_type)) => {
                *aux = rest;
                Ok((current_stack.push(top_type), Subst::empty(), vec![]))
            }
            None => {
                let line_info = self.line_prefix();
                Err(format!(
                    "{}aux>: aux stack is empty. Every aux> must be paired with a preceding >aux.",
                    line_info
                ))
            }
        }
    }

    /// Special handling for `call` to properly propagate quotation effects (Issue #228)
    ///
    /// The generic `call` signature `( ..a Q -- ..b )` has independent row variables,
    /// which doesn't constrain the output based on the quotation's actual effect.
    /// This function extracts the quotation's effect and applies it properly.
    pub(super) fn infer_call(
        &self,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Pop the quotation from the stack
        let line_prefix = self.line_prefix();
        let (remaining_stack, quot_type) = current_stack.clone().pop().ok_or_else(|| {
            format!(
                "{}call: stack underflow - expected quotation on stack",
                line_prefix
            )
        })?;

        // Extract the quotation's effect
        let quot_effect = match &quot_type {
            Type::Quotation(effect) => (**effect).clone(),
            Type::Closure { effect, .. } => (**effect).clone(),
            Type::Var(_) => {
                // Type variable - fall back to polymorphic behavior
                // This happens when the quotation type isn't known yet
                let effect = self
                    .lookup_word_effect("call")
                    .ok_or_else(|| "Unknown word: 'call'".to_string())?;
                let fresh_effect = self.freshen_effect(&effect);
                let (result_stack, subst) =
                    self.apply_effect(&fresh_effect, current_stack, "call", span)?;
                return Ok((result_stack, subst, vec![]));
            }
            _ => {
                return Err(format!(
                    "call: expected quotation or closure on stack, got {}",
                    quot_type
                ));
            }
        };

        // Check for Yield effects - quotations with Yield must use strand.weave
        if quot_effect.has_yield() {
            return Err("Cannot call quotation with Yield effect directly.\n\
                 Quotations that yield values must be wrapped with `strand.weave`.\n\
                 Example: `[ yielding-code ] strand.weave` instead of `[ yielding-code ] call`"
                .to_string());
        }

        // Freshen the quotation's effect to avoid variable clashes
        let fresh_effect = self.freshen_effect(&quot_effect);

        // Apply the quotation's effect to the remaining stack
        let (result_stack, subst) =
            self.apply_effect(&fresh_effect, remaining_stack, "call", span)?;

        // Propagate side effects from the quotation
        let propagated_effects = fresh_effect.effects.clone();

        Ok((result_stack, subst, propagated_effects))
    }

    /// Resolve arithmetic sugar operators to concrete operations based on
    /// the types on the stack. Returns `None` if the name is not a sugar op.
    pub(super) fn resolve_arithmetic_sugar(&self, name: &str, stack: &StackType) -> Option<String> {
        // Only handle known sugar operators
        if !SUGAR_OPS.contains(&name) {
            return None;
        }

        // Peek at the top two types on the stack
        let (rest, top) = stack.clone().pop()?;
        let (_, second) = rest.pop()?;

        // Integer sugar lowers to `i.<op>`, float to `f.<op>` (no float modulo).
        // String supports only `+` (concat) and `=` (equality).
        match (&second, &top) {
            (Type::Int, Type::Int) => Some(format!("i.{}", name)),
            (Type::Float, Type::Float) if name != "%" => Some(format!("f.{}", name)),
            (Type::String, Type::String) => match name {
                "+" => Some("string.concat".to_string()),
                "=" => Some("string.equal?".to_string()),
                _ => None,
            },
            // No match — not a sugar op for these types (falls through to
            // normal lookup, which fails with "Unknown word: '+'", giving the
            // user a clear error that they need explicit types).
            _ => None,
        }
    }
}
