//! Dataflow combinators: dip, keep, bi, if.

use crate::types::{SideEffect, StackType, Type};
use crate::unification::{Subst, unify_stacks};

use super::TypeChecker;

impl TypeChecker {
    /// Infer the stack effect of `dip`: ( ..a x quot -- ..b x )
    ///
    /// Pop the quotation and the value below it, run the quotation on
    /// the rest of the stack, then restore the value on top.
    pub(super) fn infer_dip(
        &self,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let line_prefix = self.line_prefix();

        // Pop the quotation
        let (stack_after_quot, quot_type) = current_stack.clone().pop().ok_or_else(|| {
            format!(
                "{}dip: stack underflow - expected quotation on stack",
                line_prefix
            )
        })?;

        let quot_effect = match self.extract_quotation_effect(
            &quot_type,
            "dip",
            "on top of stack",
            current_stack,
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        if quot_effect.has_yield() {
            return Err("dip: quotation must not have Yield effects.\n\
                 Use strand.weave for quotations that yield."
                .to_string());
        }

        // Pop the preserved value (below the quotation).
        //
        // Polymorphic pop: when the stack below the quotation is a bare row
        // variable (e.g., this `dip` is the first operation in a quotation
        // body whose incoming stack is unconstrained), refine the row var
        // by introducing a fresh type for the preserved slot. The
        // substitution is composed with the quotation-application subst so
        // the surrounding context sees the refinement. Issue #471.
        let (rest_stack, preserved_type, pop_subst) = self.polymorphic_pop(
            stack_after_quot,
            "dip: expected a value below the quotation",
        )?;

        // Freshen and apply the quotation's effect to the stack below the preserved value
        let fresh_effect = self.freshen_effect(&quot_effect);
        let (result_stack, apply_subst) =
            self.apply_effect(&fresh_effect, rest_stack, "dip (quotation)", span)?;

        // Combined substitution: the row-var refinement from the preserved
        // pop, then any unification from applying the quotation's effect.
        let subst = pop_subst.compose(&apply_subst);

        // Push the preserved value back on top, applying substitution in case
        // preserved_type contains type variables resolved during unification
        let resolved_preserved = subst.apply_type(&preserved_type);
        let result_stack = result_stack.push(resolved_preserved);

        let propagated_effects = fresh_effect.effects.clone();
        Ok((result_stack, subst, propagated_effects))
    }

    /// Infer the stack effect of `keep`: ( ..a x quot -- ..b x )
    ///
    /// Run the quotation on the value (quotation receives x), then
    /// restore the original value on top. Like `over >aux call aux>`.
    pub(super) fn infer_keep(
        &self,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let line_prefix = self.line_prefix();

        // Pop the quotation
        let (stack_after_quot, quot_type) = current_stack.clone().pop().ok_or_else(|| {
            format!(
                "{}keep: stack underflow - expected quotation on stack",
                line_prefix
            )
        })?;

        let quot_effect = match self.extract_quotation_effect(
            &quot_type,
            "keep",
            "on top of stack",
            current_stack,
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        if quot_effect.has_yield() {
            return Err("keep: quotation must not have Yield effects.\n\
                 Use strand.weave for quotations that yield."
                .to_string());
        }

        // Peek at the preserved value type. We don't consume it from the
        // stack the quotation sees (keep passes the value to the quotation
        // AND preserves it), but we need its type to push back at the end.
        //
        // Polymorphic peek: if the stack below the quotation is a bare row
        // variable (e.g., `keep` is the first operation in a quotation body
        // with unconstrained input), refine the row var by introducing a
        // fresh preserved-value type. The refinement substitution applies
        // to the stack used by the quotation. Issue #471.
        let (_rest_stack, preserved_type, peek_subst) = self.polymorphic_pop(
            stack_after_quot.clone(),
            "keep: expected a value below the quotation",
        )?;
        let quot_input_stack = peek_subst.apply_stack(&stack_after_quot);

        // The quotation receives x on the stack (stack_after_quot still has x on top).
        // Apply the quotation's effect to the stack INCLUDING x.
        let fresh_effect = self.freshen_effect(&quot_effect);
        let (result_stack, apply_subst) =
            self.apply_effect(&fresh_effect, quot_input_stack, "keep (quotation)", span)?;

        let subst = peek_subst.compose(&apply_subst);

        // Push the preserved value back on top, applying substitution in case
        // preserved_type contains type variables resolved during unification
        let resolved_preserved = subst.apply_type(&preserved_type);
        let result_stack = result_stack.push(resolved_preserved);

        let propagated_effects = fresh_effect.effects.clone();
        Ok((result_stack, subst, propagated_effects))
    }

    /// Infer the stack effect of `bi`: ( ..a x quot1 quot2 -- ..c )
    ///
    /// Apply two quotations to the same value. First quotation receives x,
    /// then second quotation receives x on top of the first quotation's results.
    pub(super) fn infer_bi(
        &self,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let line_prefix = self.line_prefix();

        // Pop quot2 (top)
        let (stack1, quot2_type) = current_stack.clone().pop().ok_or_else(|| {
            format!(
                "{}bi: stack underflow - expected second quotation on stack",
                line_prefix
            )
        })?;

        // Pop quot1
        let (stack2, quot1_type) = stack1.clone().pop().ok_or_else(|| {
            format!(
                "{}bi: stack underflow - expected first quotation on stack",
                line_prefix
            )
        })?;

        let quot1_effect = match self.extract_quotation_effect(
            &quot1_type,
            "bi",
            "as first quotation",
            current_stack.clone(),
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        let quot2_effect = match self.extract_quotation_effect(
            &quot2_type,
            "bi",
            "as second quotation",
            current_stack,
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        if quot1_effect.has_yield() || quot2_effect.has_yield() {
            return Err("bi: quotations must not have Yield effects.\n\
                 Use strand.weave for quotations that yield."
                .to_string());
        }

        // stack2 has x on top (the value both quotations operate on).
        // Peek at x's type for the second application. Polymorphic: refine a
        // bare row variable below the two quotations so `bi` inside a
        // polymorphic body type-checks. Issue #471.
        let (_rest, preserved_type, peek_subst) =
            self.polymorphic_pop(stack2.clone(), "bi: expected a value below the quotations")?;
        let quot1_input_stack = peek_subst.apply_stack(&stack2);

        // Apply quot1 to stack including x
        let fresh_effect1 = self.freshen_effect(&quot1_effect);
        let (after_quot1, apply_subst1) = self.apply_effect(
            &fresh_effect1,
            quot1_input_stack,
            "bi (first quotation)",
            span,
        )?;
        let subst1 = peek_subst.compose(&apply_subst1);

        // Push x again for quot2, applying subst1 in case preserved_type
        // contains type variables that were resolved during quot1's unification
        let resolved_preserved = subst1.apply_type(&preserved_type);
        let with_x = after_quot1.push(resolved_preserved);

        // Apply quot2
        let fresh_effect2 = self.freshen_effect(&quot2_effect);
        let (result_stack, subst2) =
            self.apply_effect(&fresh_effect2, with_x, "bi (second quotation)", span)?;

        let subst = subst1.compose(&subst2);

        let mut effects = fresh_effect1.effects.clone();
        for e in fresh_effect2.effects.clone() {
            if !effects.contains(&e) {
                effects.push(e);
            }
        }

        Ok((result_stack, subst, effects))
    }

    /// Infer the stack effect of `if`: ( ..a Bool [..a -- ..b] [..a -- ..b] -- ..b )
    ///
    /// Branch on a Bool, invoking one of two quotations whose effects must
    /// be identical. Mirrors the semantics of `if/else/then` but as a
    /// combinator — the quotations are first-class values.
    pub(super) fn infer_if_combinator(
        &self,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let line_prefix = self.line_prefix();

        // Pop else-quotation (top), then-quotation (below), condition (below that).
        let (stack1, else_quot_type) = current_stack.clone().pop().ok_or_else(|| {
            format!(
                "{}if: stack underflow - expected else-quotation on stack",
                line_prefix
            )
        })?;
        let (stack2, then_quot_type) = stack1.clone().pop().ok_or_else(|| {
            format!(
                "{}if: stack underflow - expected then-quotation below else-quotation",
                line_prefix
            )
        })?;
        // Polymorphic pop of the condition: a bare row variable below the
        // two quotations gets refined to "row + Bool" once the cond-unify
        // constrains the fresh type. This lets `if` work inside a
        // quotation body whose incoming stack is polymorphic. Issue #471.
        let (stack_after_cond, cond_type, cond_pop_subst) =
            self.polymorphic_pop(stack2, "if: expected Bool below the two quotations")?;

        // Condition must be Bool.
        let cond_unify_subst = unify_stacks(
            &StackType::singleton(Type::Bool),
            &StackType::singleton(cond_type),
        )
        .map_err(|e| format!("{}if: condition must be Bool: {}", line_prefix, e))?;
        let cond_subst = cond_pop_subst.compose(&cond_unify_subst);
        let stack_after_cond = cond_subst.apply_stack(&stack_after_cond);

        // Extract both quotation effects, mirroring the dip/keep/bi pattern:
        // if either quotation is a still-unresolved type variable, fall back
        // to the placeholder builtin signature so the caller gets *some*
        // shape rather than a hard failure.
        let then_effect = match self.extract_quotation_effect(
            &then_quot_type,
            "if",
            "as then-branch",
            current_stack.clone(),
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        let else_effect = match self.extract_quotation_effect(
            &else_quot_type,
            "if",
            "as else-branch",
            current_stack,
            span,
            &line_prefix,
        )? {
            QuotEffect::Concrete(effect) => effect,
            QuotEffect::Fallback(result_stack, subst) => {
                return Ok((result_stack, subst, vec![]));
            }
        };

        // Yield-bearing branches are allowed (matches `Statement::If`
        // semantics). For literal-quotation calls the AST normalization
        // pass rewrites the triple to `Statement::If` before this code
        // runs; for the dynamic-dispatch path the runtime invokes the
        // chosen quotation through the same machinery used everywhere
        // else, and yields propagate naturally.

        // Apply each branch independently to the post-condition stack, then
        // unify the two result stacks — matching `Statement::If` semantics.
        let fresh_then = self.freshen_effect(&then_effect);
        let (then_result, then_subst) = self.apply_effect(
            &fresh_then,
            stack_after_cond.clone(),
            "if (then branch)",
            span,
        )?;

        let fresh_else = self.freshen_effect(&else_effect);
        let else_input = then_subst.apply_stack(&stack_after_cond);
        let (else_result, else_subst) =
            self.apply_effect(&fresh_else, else_input, "if (else branch)", span)?;

        let then_result_after_else = else_subst.apply_stack(&then_result);
        let if_line = span.as_ref().map(|s| s.line + 1).unwrap_or(0);
        let merge_subst = unify_stacks(&then_result_after_else, &else_result).map_err(|e| {
            if if_line > 0 {
                format!(
                    "at line {}: if branches have incompatible stack effects:\n\
                     \x20 then branch produces: {}\n\
                     \x20 else branch produces: {}\n\
                     \x20 Both quotations of `if` must produce the same stack shape.\n\
                     \x20 Error: {}",
                    if_line, then_result_after_else, else_result, e
                )
            } else {
                format!(
                    "if branches have incompatible stack effects:\n\
                     \x20 then branch produces: {}\n\
                     \x20 else branch produces: {}\n\
                     \x20 Error: {}",
                    then_result_after_else, else_result, e
                )
            }
        })?;

        let result_stack = merge_subst.apply_stack(&else_result);
        let subst = cond_subst
            .compose(&then_subst)
            .compose(&else_subst)
            .compose(&merge_subst);

        let mut effects = fresh_then.effects.clone();
        for e in fresh_else.effects.clone() {
            if !effects.contains(&e) {
                effects.push(e);
            }
        }

        Ok((result_stack, subst, effects))
    }

    /// Resolve a quotation slot's effect for a combinator
    /// (`dip`/`keep`/`bi`/`if`), applying a uniform Var fallback:
    ///   - Concrete `Quotation` or `Closure` types yield their effect.
    ///   - Unresolved `Var` types fall back to the placeholder builtin
    ///     signature so the surrounding word still has *some* inferred
    ///     shape.
    ///   - Anything else is a type error pinpointing the offending slot.
    ///
    /// `word_name` is the combinator (`"dip"`, `"bi"`, `"if"`, …) — used
    /// both for the placeholder lookup and the error prefix.
    /// `slot_label` is the slot-specific phrase appended to the type
    /// error, e.g. `"on top of stack"` or `"as first quotation"`.
    fn extract_quotation_effect(
        &self,
        quot_type: &Type,
        word_name: &str,
        slot_label: &str,
        current_stack: StackType,
        span: &Option<crate::ast::Span>,
        line_prefix: &str,
    ) -> Result<QuotEffect, String> {
        match quot_type {
            Type::Quotation(effect) => Ok(QuotEffect::Concrete((**effect).clone())),
            Type::Closure { effect, .. } => Ok(QuotEffect::Concrete((**effect).clone())),
            Type::Var(_) => {
                let effect = self
                    .lookup_word_effect(word_name)
                    .ok_or_else(|| format!("Unknown word: '{}'", word_name))?;
                let fresh_effect = self.freshen_effect(&effect);
                let (result_stack, subst) =
                    self.apply_effect(&fresh_effect, current_stack, word_name, span)?;
                Ok(QuotEffect::Fallback(result_stack, subst))
            }
            other => Err(format!(
                "{}{}: expected quotation or closure {}, got {}",
                line_prefix, word_name, slot_label, other
            )),
        }
    }
}

/// Result of resolving a quotation slot's effect for a combinator —
/// either a concrete effect to feed into the surrounding inference, or
/// a caller-must-return placeholder result for the unresolved-Var case.
enum QuotEffect {
    Concrete(crate::types::Effect),
    Fallback(StackType, Subst),
}
