//! Control-flow inference: if/match, divergent-branch detection.

use crate::ast::Statement;
use crate::types::{SideEffect, StackType, Type, VariantInfo};
use crate::unification::{Subst, unify_stacks};

use super::TypeChecker;

impl TypeChecker {
    pub(super) fn infer_match(
        &self,
        arms: &[crate::ast::MatchArm],
        match_span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        if arms.is_empty() {
            return Err("match expression must have at least one arm".to_string());
        }

        // Pop the matched value from the stack
        let (stack_after_match, _matched_type) =
            self.pop_type(&current_stack, "match expression")?;

        // Track all arm results for unification
        let mut arm_results: Vec<StackType> = Vec::new();
        let mut combined_subst = Subst::empty();
        let mut merged_effects: Vec<SideEffect> = Vec::new();

        // Save aux stack before match arms (Issue #350)
        let aux_before_match = self.current_aux_stack.borrow().clone();
        let mut aux_after_arms: Vec<StackType> = Vec::new();

        for arm in arms {
            // Restore aux stack before each arm (Issue #350)
            *self.current_aux_stack.borrow_mut() = aux_before_match.clone();

            // Get variant name from pattern
            let variant_name = match &arm.pattern {
                crate::ast::Pattern::Variant(name) => name.as_str(),
                crate::ast::Pattern::VariantWithBindings { name, .. } => name.as_str(),
            };

            // Look up variant info
            let (_union_name, variant_info) = self
                .find_variant(variant_name)
                .ok_or_else(|| format!("Unknown variant '{}' in match pattern", variant_name))?;

            // Push fields onto the stack based on pattern type
            let arm_stack = self.push_variant_fields(
                &stack_after_match,
                &arm.pattern,
                variant_info,
                variant_name,
            )?;

            // Type check the arm body directly from the actual stack
            // Don't capture statement types for match arms - only top-level word bodies
            let (arm_result, arm_subst, arm_effects) =
                self.infer_statements_from(&arm.body, &arm_stack, false)?;

            combined_subst = combined_subst.compose(&arm_subst);
            arm_results.push(arm_result);
            aux_after_arms.push(self.current_aux_stack.borrow().clone());

            // Merge effects from this arm
            for effect in arm_effects {
                if !merged_effects.contains(&effect) {
                    merged_effects.push(effect);
                }
            }
        }

        // Verify all arms produce the same aux stack (Issue #350)
        if aux_after_arms.len() > 1 {
            let first_aux = &aux_after_arms[0];
            for (i, arm_aux) in aux_after_arms.iter().enumerate().skip(1) {
                if arm_aux != first_aux {
                    let match_line = span_line(match_span);
                    return Err(format!(
                        "at line {}: match arms have incompatible aux stack effects:\n\
                         \x20 arm 0 aux: {}\n\
                         \x20 arm {} aux: {}\n\
                         \x20 All match arms must leave the aux stack in the same state.",
                        match_line, first_aux, i, arm_aux
                    ));
                }
            }
        }
        // Set aux to the first arm's result (all are verified equal)
        if let Some(aux) = aux_after_arms.into_iter().next() {
            *self.current_aux_stack.borrow_mut() = aux;
        }

        // Unify all arm results to ensure they're compatible
        let mut final_result = arm_results[0].clone();
        for (i, arm_result) in arm_results.iter().enumerate().skip(1) {
            // Get line info for error reporting
            let match_line = span_line(match_span);
            let arm0_line = span_line(&arms[0].span);
            let arm_i_line = span_line(&arms[i].span);

            let arm_subst = unify_stacks(&final_result, arm_result).map_err(|e| {
                if match_line > 0 && arm0_line > 0 && arm_i_line > 0 {
                    format!(
                        "at line {}: match arms have incompatible stack effects:\n\
                         \x20 arm 0 (line {}) produces: {}\n\
                         \x20 arm {} (line {}) produces: {}\n\
                         \x20 All match arms must produce the same stack shape.\n\
                         \x20 Error: {}",
                        match_line, arm0_line, final_result, i, arm_i_line, arm_result, e
                    )
                } else {
                    format!(
                        "match arms have incompatible stack effects:\n\
                         \x20 arm 0 produces: {}\n\
                         \x20 arm {} produces: {}\n\
                         \x20 All match arms must produce the same stack shape.\n\
                         \x20 Error: {}",
                        final_result, i, arm_result, e
                    )
                }
            })?;
            combined_subst = combined_subst.compose(&arm_subst);
            final_result = arm_subst.apply_stack(&final_result);
        }

        Ok((final_result, combined_subst, merged_effects))
    }

    /// Push variant fields onto the stack based on the match pattern
    pub(super) fn push_variant_fields(
        &self,
        stack: &StackType,
        pattern: &crate::ast::Pattern,
        variant_info: &VariantInfo,
        variant_name: &str,
    ) -> Result<StackType, String> {
        let mut arm_stack = stack.clone();
        match pattern {
            crate::ast::Pattern::Variant(_) => {
                // Stack-based: push all fields in declaration order
                for field in &variant_info.fields {
                    arm_stack = arm_stack.push(field.field_type.clone());
                }
            }
            crate::ast::Pattern::VariantWithBindings { bindings, .. } => {
                // Named bindings: validate and push only bound fields
                for binding in bindings {
                    let field = variant_info
                        .fields
                        .iter()
                        .find(|f| &f.name == binding)
                        .ok_or_else(|| {
                            let available: Vec<_> = variant_info
                                .fields
                                .iter()
                                .map(|f| f.name.as_str())
                                .collect();
                            format!(
                                "Unknown field '{}' in pattern for variant '{}'.\n\
                                 Available fields: {}",
                                binding,
                                variant_name,
                                available.join(", ")
                            )
                        })?;
                    arm_stack = arm_stack.push(field.field_type.clone());
                }
            }
        }
        Ok(arm_stack)
    }

    /// Check if a branch ends with a recursive tail call to the current word
    /// or to a mutually recursive word.
    ///
    /// Such branches are "divergent" - they never return to the if/else,
    /// so their stack effect shouldn't constrain the other branch.
    ///
    /// # Detection Capabilities
    ///
    /// - Direct recursion: word calls itself
    /// - Mutual recursion: word calls another word in the same SCC (when call graph is available)
    ///
    /// # Limitations
    ///
    /// This detection does NOT detect:
    /// - Calls to known non-returning functions (panic, exit, infinite loops)
    /// - Nested control flow with tail calls (if ... if ... recurse then then)
    ///
    /// These patterns will still require branch unification. Future enhancements
    /// could track known non-returning functions or support explicit divergence
    /// annotations (similar to Rust's `!` type).
    pub(super) fn is_divergent_branch(&self, statements: &[Statement]) -> bool {
        let Some((current_word_name, _)) = self.current_word.borrow().as_ref().cloned() else {
            return false;
        };
        let Some(Statement::WordCall { name, .. }) = statements.last() else {
            return false;
        };

        // Direct recursion: word calls itself
        if name == &current_word_name {
            return true;
        }

        // Mutual recursion: word calls another word in the same SCC
        if let Some(ref graph) = self.call_graph
            && graph.are_mutually_recursive(&current_word_name, name)
        {
            return true;
        }

        false
    }

    /// Infer the stack effect of an if/else expression
    pub(super) fn infer_if(
        &self,
        then_branch: &[Statement],
        else_branch: &Option<Vec<Statement>>,
        if_span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Pop condition (must be Bool). Polymorphic pop allows the cond to
        // come from a row variable when this `if` sits inside a quotation
        // body with unconstrained input — the fresh type gets unified to
        // Bool below. Issue #471.
        let (stack_after_cond, cond_type, cond_pop_subst) =
            self.polymorphic_pop(current_stack, "if condition")?;

        // Condition must be Bool
        let cond_unify_subst = unify_stacks(
            &StackType::singleton(Type::Bool),
            &StackType::singleton(cond_type),
        )
        .map_err(|e| format!("if condition must be Bool: {}", e))?;

        let cond_subst = cond_pop_subst.compose(&cond_unify_subst);
        let stack_after_cond = cond_subst.apply_stack(&stack_after_cond);

        // Check for divergent branches (recursive tail calls)
        let then_diverges = self.is_divergent_branch(then_branch);
        let else_diverges = else_branch
            .as_ref()
            .map(|stmts| self.is_divergent_branch(stmts))
            .unwrap_or(false);

        // Save aux stack before branching (Issue #350)
        let aux_before_branches = self.current_aux_stack.borrow().clone();

        // Infer branches directly from the actual stack
        // Don't capture statement types for if branches - only top-level word bodies
        let (then_result, then_subst, then_effects) =
            self.infer_statements_from(then_branch, &stack_after_cond, false)?;
        let aux_after_then = self.current_aux_stack.borrow().clone();

        // Restore aux stack before checking else branch (Issue #350)
        *self.current_aux_stack.borrow_mut() = aux_before_branches.clone();

        // Infer else branch (or use stack_after_cond if no else)
        let (else_result, else_subst, else_effects) = if let Some(else_stmts) = else_branch {
            self.infer_statements_from(else_stmts, &stack_after_cond, false)?
        } else {
            (stack_after_cond.clone(), Subst::empty(), vec![])
        };
        let aux_after_else = self.current_aux_stack.borrow().clone();

        // Verify aux stacks match between branches (Issue #350)
        // Skip check if one branch diverges (never returns)
        if !then_diverges && !else_diverges && aux_after_then != aux_after_else {
            let if_line = span_line(if_span);
            return Err(format!(
                "at line {}: if/else branches have incompatible aux stack effects:\n\
                 \x20 then branch aux: {}\n\
                 \x20 else branch aux: {}\n\
                 \x20 Both branches must leave the aux stack in the same state.",
                if_line, aux_after_then, aux_after_else
            ));
        }

        // Set aux to the non-divergent branch's result (or then if neither diverges)
        if then_diverges && !else_diverges {
            *self.current_aux_stack.borrow_mut() = aux_after_else;
        } else {
            *self.current_aux_stack.borrow_mut() = aux_after_then;
        }

        // Merge effects from both branches (if either yields, the whole if yields)
        let mut merged_effects = then_effects;
        for effect in else_effects {
            if !merged_effects.contains(&effect) {
                merged_effects.push(effect);
            }
        }

        // Handle divergent branches: if one branch diverges (never returns),
        // use the other branch's stack type without requiring unification.
        // This supports patterns like:
        //   chan.receive not if drop store-loop then
        // where the then branch recurses and the else branch continues.
        let (result, branch_subst) = if then_diverges && !else_diverges {
            // Then branch diverges, use else branch's type
            (else_result, Subst::empty())
        } else if else_diverges && !then_diverges {
            // Else branch diverges, use then branch's type
            (then_result, Subst::empty())
        } else {
            // Both branches must produce compatible stacks (normal case)
            let if_line = span_line(if_span);
            let branch_subst = unify_stacks(&then_result, &else_result).map_err(|e| {
                if if_line > 0 {
                    format!(
                        "at line {}: if/else branches have incompatible stack effects:\n\
                         \x20 then branch produces: {}\n\
                         \x20 else branch produces: {}\n\
                         \x20 Both branches of an if/else must produce the same stack shape.\n\
                         \x20 Hint: Make sure both branches push/pop the same number of values.\n\
                         \x20 Error: {}",
                        if_line, then_result, else_result, e
                    )
                } else {
                    format!(
                        "if/else branches have incompatible stack effects:\n\
                         \x20 then branch produces: {}\n\
                         \x20 else branch produces: {}\n\
                         \x20 Both branches of an if/else must produce the same stack shape.\n\
                         \x20 Hint: Make sure both branches push/pop the same number of values.\n\
                         \x20 Error: {}",
                        then_result, else_result, e
                    )
                }
            })?;
            (branch_subst.apply_stack(&then_result), branch_subst)
        };

        // Propagate all substitutions
        let total_subst = cond_subst
            .compose(&then_subst)
            .compose(&else_subst)
            .compose(&branch_subst);
        Ok((result, total_subst, merged_effects))
    }
}

/// Resolve a span to a 1-based line number for error messages, or 0 if absent.
fn span_line(span: &Option<crate::ast::Span>) -> usize {
    span.as_ref().map_or(0, |s| s.line + 1)
}
