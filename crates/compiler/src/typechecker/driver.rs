//! Top-level driver: check_program / check_word / statement iteration.

use crate::ast::{Program, Statement, WordDef};
use crate::types::{
    Effect, SideEffect, StackType, Type, UnionTypeInfo, VariantFieldInfo, VariantInfo,
};
use crate::unification::{Subst, unify_stacks};

use super::{TypeChecker, format_line_prefix, validate_main_effect};

impl TypeChecker {
    pub fn check_program(&mut self, program: &Program) -> Result<(), String> {
        // First pass: register all union definitions
        for union_def in &program.unions {
            let variants = union_def
                .variants
                .iter()
                .map(|v| VariantInfo {
                    name: v.name.clone(),
                    fields: v
                        .fields
                        .iter()
                        .map(|f| VariantFieldInfo {
                            name: f.name.clone(),
                            field_type: self.parse_type_name(&f.type_name),
                        })
                        .collect(),
                })
                .collect();

            self.unions.insert(
                union_def.name.clone(),
                UnionTypeInfo {
                    name: union_def.name.clone(),
                    variants,
                },
            );
        }

        // Validate field types in unions reference known types
        self.validate_union_field_types(program)?;

        // Second pass: collect all word signatures
        // All words must have explicit stack effect declarations (v2.0 requirement)
        for word in &program.words {
            if let Some(effect) = &word.effect {
                // RFC #345: Validate that all types in the effect are known
                // This catches cases where an uppercase identifier was parsed as a type variable
                // but should have been a union type (e.g., from an include)
                self.validate_effect_types(effect, &word.name)?;
                self.env.insert(word.name.clone(), effect.clone());
            } else {
                return Err(format!(
                    "Word '{}' is missing a stack effect declaration.\n\
                     All words must declare their stack effect, e.g.: : {} ( -- ) ... ;",
                    word.name, word.name
                ));
            }
        }

        // Validate main's signature (Issue #355).
        // Only `( -- )` and `( -- Int )` are allowed.
        if let Some(main_effect) = self.env.get("main") {
            validate_main_effect(main_effect)?;
        }

        // Third pass: type check each word body
        for word in &program.words {
            self.check_word(word)?;
        }

        Ok(())
    }

    /// Type check a word definition
    pub(super) fn check_word(&self, word: &WordDef) -> Result<(), String> {
        // Track current word for detecting recursive tail calls (divergent branches)
        let line = word.source.as_ref().map(|s| s.start_line);
        *self.current_word.borrow_mut() = Some((word.name.clone(), line));

        // Reset aux stack for this word (Issue #350)
        *self.current_aux_stack.borrow_mut() = StackType::Empty;

        // All words must have declared effects (enforced in check_program)
        let declared_effect = word.effect.as_ref().expect("word must have effect");

        // Check if the word's output type is a quotation or closure
        // If so, store it as the expected type for capture inference
        if let Some((_rest, top_type)) = declared_effect.outputs.clone().pop()
            && matches!(top_type, Type::Quotation(_) | Type::Closure { .. })
        {
            *self.expected_quotation_type.borrow_mut() = Some(top_type);
        }

        // Infer the result stack and effects starting from declared input
        let (result_stack, _subst, inferred_effects) =
            self.infer_statements_from(&word.body, &declared_effect.inputs, true)?;

        // Clear expected type after checking
        *self.expected_quotation_type.borrow_mut() = None;

        // Verify result matches declared output
        let line_info = line.map(format_line_prefix).unwrap_or_default();
        unify_stacks(&declared_effect.outputs, &result_stack).map_err(|e| {
            format!(
                "{}Word '{}': declared output stack ({}) doesn't match inferred ({}): {}",
                line_info, word.name, declared_effect.outputs, result_stack, e
            )
        })?;

        // Verify computational effects match (bidirectional)
        // 1. Check that each inferred effect has a matching declared effect (by kind)
        // Type variables in effects are matched by kind (Yield matches Yield)
        for inferred in &inferred_effects {
            if !self.effect_matches_any(inferred, &declared_effect.effects) {
                return Err(format!(
                    "{}Word '{}': body produces effect '{}' but no matching effect is declared.\n\
                     Hint: Add '| Yield <type>' to the word's stack effect declaration.",
                    line_info, word.name, inferred
                ));
            }
        }

        // 2. Check that each declared effect is actually produced (effect soundness)
        // This prevents declaring effects that don't occur
        for declared in &declared_effect.effects {
            if !self.effect_matches_any(declared, &inferred_effects) {
                return Err(format!(
                    "{}Word '{}': declares effect '{}' but body doesn't produce it.\n\
                     Hint: Remove the effect declaration or ensure the body uses yield.",
                    line_info, word.name, declared
                ));
            }
        }

        // Verify aux stack is empty at word boundary (Issue #350)
        let aux_stack = self.current_aux_stack.borrow().clone();
        if aux_stack != StackType::Empty {
            return Err(format!(
                "{}Word '{}': aux stack is not empty at word return.\n\
                 Remaining aux stack: {}\n\
                 Every >aux must be matched by a corresponding aux> before the word returns.",
                line_info, word.name, aux_stack
            ));
        }

        // Clear current word
        *self.current_word.borrow_mut() = None;

        Ok(())
    }

    /// Infer the resulting stack type from a sequence of statements
    /// starting from a given input stack
    /// Returns (final_stack, substitution, accumulated_effects)
    ///
    /// `capture_stmt_types`: If true, capture statement type info for codegen optimization.
    /// Should only be true for top-level word bodies, not for nested branches/loops.
    pub(super) fn infer_statements_from(
        &self,
        statements: &[Statement],
        start_stack: &StackType,
        capture_stmt_types: bool,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        let mut current_stack = start_stack.clone();
        let mut accumulated_subst = Subst::empty();
        let mut accumulated_effects: Vec<SideEffect> = Vec::new();
        let mut skip_next = false;

        for (i, stmt) in statements.iter().enumerate() {
            // Skip this statement if we already handled it (e.g., pick/roll after literal)
            if skip_next {
                skip_next = false;
                continue;
            }

            // Special case: IntLiteral followed by pick or roll
            // Handle them as a fused operation with correct type semantics
            if let Statement::IntLiteral(n) = stmt
                && let Some(Statement::WordCall {
                    name: next_word, ..
                }) = statements.get(i + 1)
            {
                if next_word == "pick" {
                    let (new_stack, subst) = self.handle_literal_pick(*n, current_stack.clone())?;
                    current_stack = new_stack;
                    accumulated_subst = accumulated_subst.compose(&subst);
                    skip_next = true; // Skip the "pick" word
                    continue;
                } else if next_word == "roll" {
                    let (new_stack, subst) = self.handle_literal_roll(*n, current_stack.clone())?;
                    current_stack = new_stack;
                    accumulated_subst = accumulated_subst.compose(&subst);
                    skip_next = true; // Skip the "roll" word
                    continue;
                }
            }

            // Look ahead: if this is a quotation followed by a word that expects specific quotation type,
            // set the expected type before checking the quotation
            let saved_expected_type = if matches!(stmt, Statement::Quotation { .. }) {
                // Save the current expected type
                let saved = self.expected_quotation_type.borrow().clone();

                // Try to set expected type based on lookahead
                if let Some(Statement::WordCall {
                    name: next_word, ..
                }) = statements.get(i + 1)
                {
                    // Check if the next word expects a specific quotation type
                    if let Some(next_effect) = self.lookup_word_effect(next_word) {
                        // Extract the quotation type expected by the next word
                        // For operations like spawn: ( ..a Quotation(-- ) -- ..a Int )
                        if let Some((_rest, quot_type)) = next_effect.inputs.clone().pop()
                            && matches!(quot_type, Type::Quotation(_))
                        {
                            *self.expected_quotation_type.borrow_mut() = Some(quot_type);
                        }
                    }
                }
                Some(saved)
            } else {
                None
            };

            // Capture statement type info for codegen optimization (Issue #186)
            // Record the top-of-stack type BEFORE this statement for operations like dup
            // Only capture for top-level word bodies, not nested branches/loops
            if capture_stmt_types && let Some((word_name, _)) = self.current_word.borrow().as_ref()
            {
                self.capture_statement_type(word_name, i, &current_stack);
            }

            let (new_stack, subst, effects) = self.infer_statement(stmt, current_stack)?;
            current_stack = new_stack;
            accumulated_subst = accumulated_subst.compose(&subst);

            // Accumulate side effects from this statement
            for effect in effects {
                if !accumulated_effects.contains(&effect) {
                    accumulated_effects.push(effect);
                }
            }

            // Restore expected type after checking quotation
            if let Some(saved) = saved_expected_type {
                *self.expected_quotation_type.borrow_mut() = saved;
            }
        }

        Ok((current_stack, accumulated_subst, accumulated_effects))
    }

    /// Infer a statement sequence's full `Effect`, starting from a fresh
    /// row-polymorphic `..input` and normalizing both ends by the accumulated
    /// substitution. Used for quotation bodies.
    pub(super) fn infer_statements(&self, statements: &[Statement]) -> Result<Effect, String> {
        let start = StackType::RowVar("input".to_string());
        // Don't capture statement types for quotation bodies - only top-level word bodies
        let (result, subst, effects) = self.infer_statements_from(statements, &start, false)?;

        // Apply the accumulated substitution to both start and result
        // This ensures row variables are consistently named
        let normalized_start = subst.apply_stack(&start);
        let normalized_result = subst.apply_stack(&result);

        Ok(Effect::with_effects(
            normalized_start,
            normalized_result,
            effects,
        ))
    }

    /// Infer the resulting stack, substitution, and effects of a single statement.
    pub(super) fn infer_statement(
        &self,
        statement: &Statement,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        match statement {
            Statement::IntLiteral(_) => Ok((current_stack.push(Type::Int), Subst::empty(), vec![])),
            Statement::BoolLiteral(_) => {
                Ok((current_stack.push(Type::Bool), Subst::empty(), vec![]))
            }
            Statement::StringLiteral(_) => {
                Ok((current_stack.push(Type::String), Subst::empty(), vec![]))
            }
            Statement::FloatLiteral(_) => {
                Ok((current_stack.push(Type::Float), Subst::empty(), vec![]))
            }
            Statement::Symbol(_) => Ok((current_stack.push(Type::Symbol), Subst::empty(), vec![])),
            Statement::Match { arms, span } => self.infer_match(arms, span, current_stack),
            Statement::WordCall { name, span } => self.infer_word_call(name, span, current_stack),
            Statement::If {
                then_branch,
                else_branch,
                span,
            } => self.infer_if(then_branch, else_branch, span, current_stack),
            Statement::Quotation { id, body, .. } => self.infer_quotation(*id, body, current_stack),
        }
    }

    /// Whether `inferred` matches any of `declared` by kind (Yield matches Yield).
    pub(super) fn effect_matches_any(
        &self,
        inferred: &SideEffect,
        declared: &[SideEffect],
    ) -> bool {
        declared.iter().any(|decl| match (inferred, decl) {
            (SideEffect::Yield(_), SideEffect::Yield(_)) => true,
        })
    }
}
