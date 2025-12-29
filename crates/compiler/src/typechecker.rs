//! Enhanced type checker for Seq with full type tracking
//!
//! Uses row polymorphism and unification to verify stack effects.
//! Based on cem2's type checker but simplified for Phase 8.5.

use crate::ast::{Program, Statement, WordDef};
use crate::builtins::builtin_signature;
use crate::capture_analysis::calculate_captures;
use crate::types::{Effect, StackType, Type, UnionTypeInfo, VariantFieldInfo, VariantInfo};
use crate::unification::{Subst, unify_stacks};
use std::collections::HashMap;

pub struct TypeChecker {
    /// Environment mapping word names to their effects
    env: HashMap<String, Effect>,
    /// Union type registry - maps union names to their type information
    /// Contains variant names and field types for each union
    unions: HashMap<String, UnionTypeInfo>,
    /// Counter for generating fresh type variables
    fresh_counter: std::cell::Cell<usize>,
    /// Quotation types tracked during type checking
    /// Maps quotation ID (from AST) to inferred type (Quotation or Closure)
    /// This type map is used by codegen to generate appropriate code
    quotation_types: std::cell::RefCell<HashMap<usize, Type>>,
    /// Expected quotation/closure type (from word signature, if any)
    /// Used during type-driven capture inference
    expected_quotation_type: std::cell::RefCell<Option<Type>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            unions: HashMap::new(),
            fresh_counter: std::cell::Cell::new(0),
            quotation_types: std::cell::RefCell::new(HashMap::new()),
            expected_quotation_type: std::cell::RefCell::new(None),
        }
    }

    /// Look up a union type by name
    pub fn get_union(&self, name: &str) -> Option<&UnionTypeInfo> {
        self.unions.get(name)
    }

    /// Get all registered union types
    pub fn get_unions(&self) -> &HashMap<String, UnionTypeInfo> {
        &self.unions
    }

    /// Find variant info by name across all unions
    ///
    /// Returns (union_name, variant_info) for the variant
    fn find_variant(&self, variant_name: &str) -> Option<(&str, &VariantInfo)> {
        for (union_name, union_info) in &self.unions {
            for variant in &union_info.variants {
                if variant.name == variant_name {
                    return Some((union_name.as_str(), variant));
                }
            }
        }
        None
    }

    /// Register external word effects (e.g., from included modules).
    ///
    /// Words with `Some(effect)` get their actual signature.
    /// Words with `None` get a maximally polymorphic placeholder `( ..a -- ..b )`.
    pub fn register_external_words(&mut self, words: &[(&str, Option<&Effect>)]) {
        for (name, effect) in words {
            if let Some(eff) = effect {
                self.env.insert(name.to_string(), (*eff).clone());
            } else {
                // Maximally polymorphic placeholder
                let placeholder = Effect::new(
                    StackType::RowVar("ext_in".to_string()),
                    StackType::RowVar("ext_out".to_string()),
                );
                self.env.insert(name.to_string(), placeholder);
            }
        }
    }

    /// Register external union type names (e.g., from included modules).
    ///
    /// This allows field types in union definitions to reference types from includes.
    /// We only register the name as a valid type; we don't need full variant info
    /// since the actual union definition lives in the included file.
    pub fn register_external_unions(&mut self, union_names: &[&str]) {
        for name in union_names {
            // Insert a placeholder union with no variants
            // This makes is_valid_type_name() return true for this type
            self.unions.insert(
                name.to_string(),
                UnionTypeInfo {
                    name: name.to_string(),
                    variants: vec![],
                },
            );
        }
    }

    /// Extract the type map (quotation ID -> inferred type)
    ///
    /// This should be called after check_program() to get the inferred types
    /// for all quotations in the program. The map is used by codegen to generate
    /// appropriate code for Quotations vs Closures.
    pub fn take_quotation_types(&self) -> HashMap<usize, Type> {
        self.quotation_types.replace(HashMap::new())
    }

    /// Generate a fresh variable name
    fn fresh_var(&self, prefix: &str) -> String {
        let n = self.fresh_counter.get();
        self.fresh_counter.set(n + 1);
        format!("{}${}", prefix, n)
    }

    /// Freshen all type and row variables in an effect
    fn freshen_effect(&self, effect: &Effect) -> Effect {
        let mut type_map = HashMap::new();
        let mut row_map = HashMap::new();

        let fresh_inputs = self.freshen_stack(&effect.inputs, &mut type_map, &mut row_map);
        let fresh_outputs = self.freshen_stack(&effect.outputs, &mut type_map, &mut row_map);

        Effect::new(fresh_inputs, fresh_outputs)
    }

    fn freshen_stack(
        &self,
        stack: &StackType,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> StackType {
        match stack {
            StackType::Empty => StackType::Empty,
            StackType::RowVar(name) => {
                let fresh_name = row_map
                    .entry(name.clone())
                    .or_insert_with(|| self.fresh_var(name));
                StackType::RowVar(fresh_name.clone())
            }
            StackType::Cons { rest, top } => {
                let fresh_rest = self.freshen_stack(rest, type_map, row_map);
                let fresh_top = self.freshen_type(top, type_map, row_map);
                StackType::Cons {
                    rest: Box::new(fresh_rest),
                    top: fresh_top,
                }
            }
        }
    }

    fn freshen_type(
        &self,
        ty: &Type,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> Type {
        match ty {
            Type::Int | Type::Float | Type::Bool | Type::String => ty.clone(),
            Type::Var(name) => {
                let fresh_name = type_map
                    .entry(name.clone())
                    .or_insert_with(|| self.fresh_var(name));
                Type::Var(fresh_name.clone())
            }
            Type::Quotation(effect) => {
                let fresh_inputs = self.freshen_stack(&effect.inputs, type_map, row_map);
                let fresh_outputs = self.freshen_stack(&effect.outputs, type_map, row_map);
                Type::Quotation(Box::new(Effect::new(fresh_inputs, fresh_outputs)))
            }
            Type::Closure { effect, captures } => {
                let fresh_inputs = self.freshen_stack(&effect.inputs, type_map, row_map);
                let fresh_outputs = self.freshen_stack(&effect.outputs, type_map, row_map);
                let fresh_captures = captures
                    .iter()
                    .map(|t| self.freshen_type(t, type_map, row_map))
                    .collect();
                Type::Closure {
                    effect: Box::new(Effect::new(fresh_inputs, fresh_outputs)),
                    captures: fresh_captures,
                }
            }
            // Union types are concrete named types - no freshening needed
            Type::Union(name) => Type::Union(name.clone()),
        }
    }

    /// Parse a type name string into a Type
    ///
    /// Supports: Int, Float, Bool, String, and union types
    fn parse_type_name(&self, name: &str) -> Type {
        match name {
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "String" => Type::String,
            // Any other name is assumed to be a union type reference
            other => Type::Union(other.to_string()),
        }
    }

    /// Check if a type name is a known valid type
    ///
    /// Returns true for built-in types (Int, Float, Bool, String) and
    /// registered union type names
    fn is_valid_type_name(&self, name: &str) -> bool {
        matches!(name, "Int" | "Float" | "Bool" | "String") || self.unions.contains_key(name)
    }

    /// Validate that all field types in union definitions reference known types
    ///
    /// Note: Field count validation happens earlier in generate_constructors()
    fn validate_union_field_types(&self, program: &Program) -> Result<(), String> {
        for union_def in &program.unions {
            for variant in &union_def.variants {
                for field in &variant.fields {
                    if !self.is_valid_type_name(&field.type_name) {
                        return Err(format!(
                            "Unknown type '{}' in field '{}' of variant '{}' in union '{}'. \
                             Valid types are: Int, Float, Bool, String, or a defined union name.",
                            field.type_name, field.name, variant.name, union_def.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Type check a complete program
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
        // For words without explicit effects, use a maximally polymorphic placeholder
        // This allows calls to work, and actual type safety comes from checking the body
        for word in &program.words {
            if let Some(effect) = &word.effect {
                self.env.insert(word.name.clone(), effect.clone());
            } else {
                // Use placeholder effect: ( ..input -- ..output )
                // This is maximally polymorphic and allows any usage
                let placeholder = Effect::new(
                    StackType::RowVar("input".to_string()),
                    StackType::RowVar("output".to_string()),
                );
                self.env.insert(word.name.clone(), placeholder);
            }
        }

        // Third pass: type check each word body
        for word in &program.words {
            self.check_word(word)?;
        }

        Ok(())
    }

    /// Type check a word definition
    fn check_word(&self, word: &WordDef) -> Result<(), String> {
        // If word has declared effect, verify body matches it
        if let Some(declared_effect) = &word.effect {
            // Check if the word's output type is a quotation or closure
            // If so, store it as the expected type for capture inference
            if let Some((_rest, top_type)) = declared_effect.outputs.clone().pop()
                && matches!(top_type, Type::Quotation(_) | Type::Closure { .. })
            {
                *self.expected_quotation_type.borrow_mut() = Some(top_type);
            }

            // Infer the result stack starting from declared input
            let (result_stack, _subst) =
                self.infer_statements_from(&word.body, &declared_effect.inputs)?;

            // Clear expected type after checking
            *self.expected_quotation_type.borrow_mut() = None;

            // Verify result matches declared output
            unify_stacks(&declared_effect.outputs, &result_stack).map_err(|e| {
                format!(
                    "Word '{}': declared output stack ({}) doesn't match inferred ({}): {}",
                    word.name, declared_effect.outputs, result_stack, e
                )
            })?;
        } else {
            // No declared effect - just verify body is well-typed
            // Start from polymorphic input
            self.infer_statements_from(&word.body, &StackType::RowVar("input".to_string()))?;
        }

        Ok(())
    }

    /// Infer the resulting stack type from a sequence of statements
    /// starting from a given input stack
    fn infer_statements_from(
        &self,
        statements: &[Statement],
        start_stack: &StackType,
    ) -> Result<(StackType, Subst), String> {
        let mut current_stack = start_stack.clone();
        let mut accumulated_subst = Subst::empty();
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

            let (new_stack, subst) = self.infer_statement(stmt, current_stack)?;
            current_stack = new_stack;
            accumulated_subst = accumulated_subst.compose(&subst);

            // Restore expected type after checking quotation
            if let Some(saved) = saved_expected_type {
                *self.expected_quotation_type.borrow_mut() = saved;
            }
        }

        Ok((current_stack, accumulated_subst))
    }

    /// Handle `n pick` where n is a literal integer
    ///
    /// pick(n) copies the value at position n to the top of the stack.
    /// Position 0 is the top, 1 is below top, etc.
    ///
    /// Example: `2 pick` on stack ( A B C ) produces ( A B C A )
    /// - Position 0: C (top)
    /// - Position 1: B
    /// - Position 2: A
    /// - Result: copy A to top
    fn handle_literal_pick(
        &self,
        n: i64,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        if n < 0 {
            return Err(format!("pick: index must be non-negative, got {}", n));
        }

        // Get the type at position n
        let type_at_n = self.get_type_at_position(&current_stack, n as usize, "pick")?;

        // Push a copy of that type onto the stack
        Ok((current_stack.push(type_at_n), Subst::empty()))
    }

    /// Handle `n roll` where n is a literal integer
    ///
    /// roll(n) moves the value at position n to the top of the stack,
    /// shifting all items above it down by one position.
    ///
    /// Example: `2 roll` on stack ( A B C ) produces ( B C A )
    /// - Position 0: C (top)
    /// - Position 1: B
    /// - Position 2: A
    /// - Result: move A to top, B and C shift down
    fn handle_literal_roll(
        &self,
        n: i64,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        if n < 0 {
            return Err(format!("roll: index must be non-negative, got {}", n));
        }

        // For roll, we need to:
        // 1. Extract the type at position n
        // 2. Remove it from that position
        // 3. Push it on top
        self.rotate_type_to_top(current_stack, n as usize)
    }

    /// Get the type at position n in the stack (0 = top)
    fn get_type_at_position(&self, stack: &StackType, n: usize, op: &str) -> Result<Type, String> {
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
                        "{}: stack underflow - position {} requested but stack has only {} concrete items",
                        op, n, pos
                    ));
                }
            }
        }
    }

    /// Remove the type at position n and push it on top (for roll)
    fn rotate_type_to_top(&self, stack: StackType, n: usize) -> Result<(StackType, Subst), String> {
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
                        "roll: stack underflow - position {} requested but stack has only {} items",
                        n, pos
                    ));
                }
            }
        }
    }

    /// Infer the stack effect of a sequence of statements
    /// Returns an Effect with both inputs and outputs normalized by applying discovered substitutions
    fn infer_statements(&self, statements: &[Statement]) -> Result<Effect, String> {
        let start = StackType::RowVar("input".to_string());
        let (result, subst) = self.infer_statements_from(statements, &start)?;

        // Apply the accumulated substitution to both start and result
        // This ensures row variables are consistently named
        let normalized_start = subst.apply_stack(&start);
        let normalized_result = subst.apply_stack(&result);

        Ok(Effect::new(normalized_start, normalized_result))
    }

    /// Infer the stack effect of a match expression
    fn infer_match(
        &self,
        arms: &[crate::ast::MatchArm],
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        if arms.is_empty() {
            return Err("match expression must have at least one arm".to_string());
        }

        // Pop the matched value from the stack
        let (stack_after_match, _matched_type) =
            self.pop_type(&current_stack, "match expression")?;

        // Track all arm results for unification
        let mut arm_results: Vec<StackType> = Vec::new();
        let mut combined_subst = Subst::empty();

        for arm in arms {
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
            let (arm_result, arm_subst) = self.infer_statements_from(&arm.body, &arm_stack)?;

            combined_subst = combined_subst.compose(&arm_subst);
            arm_results.push(arm_result);
        }

        // Unify all arm results to ensure they're compatible
        let mut final_result = arm_results[0].clone();
        for (i, arm_result) in arm_results.iter().enumerate().skip(1) {
            let arm_subst = unify_stacks(&final_result, arm_result).map_err(|e| {
                format!(
                    "match arms have incompatible stack effects:\n\
                     \x20 arm 0 produces: {}\n\
                     \x20 arm {} produces: {}\n\
                     \x20 All match arms must produce the same stack shape.\n\
                     \x20 Error: {}",
                    final_result, i, arm_result, e
                )
            })?;
            combined_subst = combined_subst.compose(&arm_subst);
            final_result = arm_subst.apply_stack(&final_result);
        }

        Ok((final_result, combined_subst))
    }

    /// Push variant fields onto the stack based on the match pattern
    fn push_variant_fields(
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

    /// Infer the stack effect of an if/else expression
    fn infer_if(
        &self,
        then_branch: &[Statement],
        else_branch: &Option<Vec<Statement>>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        // Pop condition (must be Bool)
        let (stack_after_cond, cond_type) = self.pop_type(&current_stack, "if condition")?;

        // Condition must be Bool
        let cond_subst = unify_stacks(
            &StackType::singleton(Type::Bool),
            &StackType::singleton(cond_type),
        )
        .map_err(|e| format!("if condition must be Bool: {}", e))?;

        let stack_after_cond = cond_subst.apply_stack(&stack_after_cond);

        // Infer branches directly from the actual stack
        let (then_result, then_subst) =
            self.infer_statements_from(then_branch, &stack_after_cond)?;

        // Infer else branch (or use stack_after_cond if no else)
        let (else_result, else_subst) = if let Some(else_stmts) = else_branch {
            self.infer_statements_from(else_stmts, &stack_after_cond)?
        } else {
            (stack_after_cond, Subst::empty())
        };

        // Both branches must produce compatible stacks
        let branch_subst = unify_stacks(&then_result, &else_result).map_err(|e| {
            format!(
                "if/else branches have incompatible stack effects:\n\
                 \x20 then branch produces: {}\n\
                 \x20 else branch produces: {}\n\
                 \x20 Both branches of an if/else must produce the same stack shape.\n\
                 \x20 Hint: Make sure both branches push/pop the same number of values.\n\
                 \x20 Error: {}",
                then_result, else_result, e
            )
        })?;

        // Apply branch unification to get the final result
        let result = branch_subst.apply_stack(&then_result);

        // Propagate all substitutions
        let total_subst = cond_subst
            .compose(&then_subst)
            .compose(&else_subst)
            .compose(&branch_subst);
        Ok((result, total_subst))
    }

    /// Infer the stack effect of a quotation
    fn infer_quotation(
        &self,
        id: usize,
        body: &[Statement],
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        // Infer the effect of the quotation body
        let body_effect = self.infer_statements(body)?;

        // Perform capture analysis
        let quot_type = self.analyze_captures(&body_effect, &current_stack)?;

        // Record this quotation's type in the type map (for CodeGen to use later)
        self.quotation_types
            .borrow_mut()
            .insert(id, quot_type.clone());

        // If this is a closure, we need to pop the captured values from the stack
        let result_stack = match &quot_type {
            Type::Quotation(_) => {
                // Stateless - no captures, just push quotation onto stack
                current_stack.push(quot_type)
            }
            Type::Closure { captures, .. } => {
                // Pop captured values from stack, then push closure
                let mut stack = current_stack.clone();
                for _ in 0..captures.len() {
                    let (new_stack, _value) = self.pop_type(&stack, "closure capture")?;
                    stack = new_stack;
                }
                stack.push(quot_type)
            }
            _ => unreachable!("analyze_captures only returns Quotation or Closure"),
        };

        Ok((result_stack, Subst::empty()))
    }

    /// Infer the stack effect of a word call
    fn infer_word_call(
        &self,
        name: &str,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        // Look up word's effect
        let effect = self
            .lookup_word_effect(name)
            .ok_or_else(|| format!("Unknown word: '{}'", name))?;

        // Freshen the effect to avoid variable name clashes
        let fresh_effect = self.freshen_effect(&effect);

        // Special handling for spawn: auto-convert Quotation to Closure if needed
        let adjusted_stack = if name == "spawn" {
            self.adjust_stack_for_spawn(current_stack, &fresh_effect)?
        } else {
            current_stack
        };

        // Apply the freshened effect to current stack
        self.apply_effect(&fresh_effect, adjusted_stack, name)
    }

    /// Infer the resulting stack type after a statement
    /// Takes current stack, returns (new stack, substitution) after statement
    fn infer_statement(
        &self,
        statement: &Statement,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        match statement {
            Statement::IntLiteral(_) => Ok((current_stack.push(Type::Int), Subst::empty())),
            Statement::BoolLiteral(_) => Ok((current_stack.push(Type::Bool), Subst::empty())),
            Statement::StringLiteral(_) => Ok((current_stack.push(Type::String), Subst::empty())),
            Statement::FloatLiteral(_) => Ok((current_stack.push(Type::Float), Subst::empty())),
            Statement::Match { arms } => self.infer_match(arms, current_stack),
            Statement::WordCall { name, .. } => self.infer_word_call(name, current_stack),
            Statement::If {
                then_branch,
                else_branch,
            } => self.infer_if(then_branch, else_branch, current_stack),
            Statement::Quotation { id, body } => self.infer_quotation(*id, body, current_stack),
        }
    }

    /// Look up the effect of a word (built-in or user-defined)
    fn lookup_word_effect(&self, name: &str) -> Option<Effect> {
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
    fn apply_effect(
        &self,
        effect: &Effect,
        current_stack: StackType,
        operation: &str,
    ) -> Result<(StackType, Subst), String> {
        // Unify current stack with effect's input
        let subst = unify_stacks(&effect.inputs, &current_stack).map_err(|e| {
            format!(
                "{}: stack type mismatch. Expected {}, got {}: {}",
                operation, effect.inputs, current_stack, e
            )
        })?;

        // Apply substitution to output
        let result_stack = subst.apply_stack(&effect.outputs);

        Ok((result_stack, subst))
    }

    /// Adjust stack for spawn operation by converting Quotation to Closure if needed
    ///
    /// spawn expects Quotation(Empty -- Empty), but if we have Quotation(T... -- U...)
    /// with non-empty inputs, we auto-convert it to a Closure that captures those inputs.
    fn adjust_stack_for_spawn(
        &self,
        current_stack: StackType,
        spawn_effect: &Effect,
    ) -> Result<StackType, String> {
        // spawn expects: ( ..a Quotation(Empty -- Empty) -- ..a Int )
        // Extract the expected quotation type from spawn's effect
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
                                "spawn: not enough values on stack to capture. Need {} values",
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
    fn analyze_captures(
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
                // User declared quotation type - check if we need to auto-create closure
                // Auto-create closure only when:
                // 1. Expected effect has empty inputs (like spawn's ( -- ))
                // 2. Body effect has non-empty inputs (needs values to execute)

                let expected_is_empty = matches!(expected_effect.inputs, StackType::Empty);
                let body_needs_inputs = !matches!(body_effect.inputs, StackType::Empty);

                if expected_is_empty && body_needs_inputs {
                    // Body needs inputs but expected provides none
                    // Auto-create closure to capture the inputs
                    let captures = calculate_captures(body_effect, &expected_effect)?;
                    Ok(Type::Closure {
                        effect: expected_effect,
                        captures,
                    })
                } else {
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

    /// Pop a type from a stack type, returning (rest, top)
    fn pop_type(&self, stack: &StackType, context: &str) -> Result<(StackType, Type), String> {
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

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_literal() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::IntLiteral(42)],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_simple_operation() {
        // : test ( Int Int -- Int ) add ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::WordCall {
                    name: "i.add".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_type_mismatch() {
        // : test ( String -- ) io.write-line ;  with body: 42
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::String),
                    StackType::Empty,
                )),
                body: vec![
                    Statement::IntLiteral(42), // Pushes Int, not String!
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_polymorphic_dup() {
        // : my-dup ( Int -- Int Int ) dup ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "my-dup".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::Empty.push(Type::Int).push(Type::Int),
                )),
                body: vec![Statement::WordCall {
                    name: "dup".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_conditional_branches() {
        // : test ( Int Int -- String )
        //   > if "greater" else "not greater" then ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::String),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::StringLiteral("greater".to_string())],
                        else_branch: Some(vec![Statement::StringLiteral(
                            "not greater".to_string(),
                        )]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_mismatched_branches() {
        // : test ( Int -- ? )
        //   if 42 else "string" then ;  // ERROR: incompatible types
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::BoolLiteral(true),
                    Statement::If {
                        then_branch: vec![Statement::IntLiteral(42)],
                        else_branch: Some(vec![Statement::StringLiteral("string".to_string())]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("incompatible"));
    }

    #[test]
    fn test_user_defined_word_call() {
        // : helper ( Int -- String ) int->string ;
        // : main ( -- ) 42 helper io.write-line ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "helper".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::String),
                    )),
                    body: vec![Statement::WordCall {
                        name: "int->string".to_string(),
                        span: None,
                    }],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                    body: vec![
                        Statement::IntLiteral(42),
                        Statement::WordCall {
                            name: "helper".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "io.write-line".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
            ],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_arithmetic_chain() {
        // : test ( Int Int Int -- Int )
        //   add multiply ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.multiply".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_write_line_type_error() {
        // : test ( Int -- ) io.write-line ;  // ERROR: io.write-line expects String
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::Empty,
                )),
                body: vec![Statement::WordCall {
                    name: "io.write-line".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_stack_underflow_drop() {
        // : test ( -- ) drop ;  // ERROR: can't drop from empty stack
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![Statement::WordCall {
                    name: "drop".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn test_stack_underflow_add() {
        // : test ( Int -- Int ) add ;  // ERROR: add needs 2 values
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::WordCall {
                    name: "i.add".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));
    }

    #[test]
    fn test_csp_operations() {
        // : test ( -- )
        //   chan.make     # ( -- Int )
        //   42 swap       # ( Int Int -- Int Int )
        //   chan.send     # ( Int Int -- Bool )
        //   drop          # ( Bool -- )
        // ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![
                    Statement::WordCall {
                        name: "chan.make".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(42),
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "chan.send".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_complex_stack_shuffling() {
        // : test ( Int Int Int -- Int )
        //   rot add add ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "rot".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_empty_program() {
        // Program with no words should be valid
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_word_without_effect_declaration() {
        // : helper 42 ;  // No effect declaration
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "helper".to_string(),
                effect: None,
                body: vec![Statement::IntLiteral(42)],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_nested_conditionals() {
        // : test ( Int Int Int Int -- String )
        //   > if
        //     > if "both true" else "first true" then
        //   else
        //     drop drop "first false"
        //   then ;
        // Note: Needs 4 Ints total (2 for each > comparison)
        // Else branch must drop unused Ints to match then branch's stack effect
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                    StackType::singleton(Type::String),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            Statement::WordCall {
                                name: "i.>".to_string(),
                                span: None,
                            },
                            Statement::If {
                                then_branch: vec![Statement::StringLiteral(
                                    "both true".to_string(),
                                )],
                                else_branch: Some(vec![Statement::StringLiteral(
                                    "first true".to_string(),
                                )]),
                            },
                        ],
                        else_branch: Some(vec![
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::StringLiteral("first false".to_string()),
                        ]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        match checker.check_program(&program) {
            Ok(_) => {}
            Err(e) => panic!("Type check failed: {}", e),
        }
    }

    #[test]
    fn test_conditional_without_else() {
        // : test ( Int Int -- Int )
        //   > if 100 then ;
        // Both branches must leave same stack
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::IntLiteral(100)],
                        else_branch: None, // No else - should leave stack unchanged
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        // This should fail because then pushes Int but else leaves stack empty
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_word_chain() {
        // : helper1 ( Int -- String ) int->string ;
        // : helper2 ( String -- ) io.write-line ;
        // : main ( -- ) 42 helper1 helper2 ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "helper1".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::String),
                    )),
                    body: vec![Statement::WordCall {
                        name: "int->string".to_string(),
                        span: None,
                    }],
                    source: None,
                },
                WordDef {
                    name: "helper2".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::String),
                        StackType::Empty,
                    )),
                    body: vec![Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    }],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                    body: vec![
                        Statement::IntLiteral(42),
                        Statement::WordCall {
                            name: "helper1".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "helper2".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
            ],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_all_stack_ops() {
        // : test ( Int Int Int -- Int Int Int Int )
        //   over nip tuck ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "over".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "nip".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "tuck".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_mixed_types_complex() {
        // : test ( -- )
        //   42 int->string      # ( -- String )
        //   100 200 >           # ( String -- String Int )
        //   if                  # ( String -- String )
        //     io.write-line     # ( String -- )
        //   else
        //     io.write-line
        //   then ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::WordCall {
                        name: "int->string".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(100),
                    Statement::IntLiteral(200),
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::WordCall {
                            name: "io.write-line".to_string(),
                            span: None,
                        }],
                        else_branch: Some(vec![Statement::WordCall {
                            name: "io.write-line".to_string(),
                            span: None,
                        }]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_string_literal() {
        // : test ( -- String ) "hello" ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::String),
                )),
                body: vec![Statement::StringLiteral("hello".to_string())],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_bool_literal() {
        // : test ( -- Bool ) true ;
        // Booleans are now properly typed as Bool
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Bool),
                )),
                body: vec![Statement::BoolLiteral(true)],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_type_error_in_nested_conditional() {
        // : test ( Int Int -- ? )
        //   > if
        //     42 io.write-line   # ERROR: io.write-line expects String, got Int
        //   else
        //     "ok" io.write-line
        //   then ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(10),
                    Statement::IntLiteral(20),
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(42),
                            Statement::WordCall {
                                name: "io.write-line".to_string(),
                                span: None,
                            },
                        ],
                        else_branch: Some(vec![
                            Statement::StringLiteral("ok".to_string()),
                            Statement::WordCall {
                                name: "io.write-line".to_string(),
                                span: None,
                            },
                        ]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_read_line_operation() {
        // : test ( -- String ) io.read-line ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::String),
                )),
                body: vec![Statement::WordCall {
                    name: "io.read-line".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_comparison_operations() {
        // Test all comparison operators
        // : test ( Int Int -- Bool )
        //   i.<= ;
        // Simplified: just test that comparisons work and return Bool
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Bool),
                )),
                body: vec![Statement::WordCall {
                    name: "i.<=".to_string(),
                    span: None,
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_recursive_word_definitions() {
        // Test mutually recursive words (type checking only, no runtime)
        // : is-even ( Int -- Int ) dup 0 = if drop 1 else 1 subtract is-odd then ;
        // : is-odd ( Int -- Int ) dup 0 = if drop 0 else 1 subtract is-even then ;
        //
        // Note: This tests that the checker can handle words that reference each other
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "is-even".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::IntLiteral(0),
                        Statement::WordCall {
                            name: "i.=".to_string(),
                            span: None,
                        },
                        Statement::If {
                            then_branch: vec![
                                Statement::WordCall {
                                    name: "drop".to_string(),
                                    span: None,
                                },
                                Statement::IntLiteral(1),
                            ],
                            else_branch: Some(vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall {
                                    name: "i.subtract".to_string(),
                                    span: None,
                                },
                                Statement::WordCall {
                                    name: "is-odd".to_string(),
                                    span: None,
                                },
                            ]),
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "is-odd".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::IntLiteral(0),
                        Statement::WordCall {
                            name: "i.=".to_string(),
                            span: None,
                        },
                        Statement::If {
                            then_branch: vec![
                                Statement::WordCall {
                                    name: "drop".to_string(),
                                    span: None,
                                },
                                Statement::IntLiteral(0),
                            ],
                            else_branch: Some(vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall {
                                    name: "i.subtract".to_string(),
                                    span: None,
                                },
                                Statement::WordCall {
                                    name: "is-even".to_string(),
                                    span: None,
                                },
                            ]),
                        },
                    ],
                    source: None,
                },
            ],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_word_calling_word_with_row_polymorphism() {
        // Test that row variables unify correctly through word calls
        // : apply-twice ( Int -- Int ) dup add ;
        // : quad ( Int -- Int ) apply-twice apply-twice ;
        // Should work: both use row polymorphism correctly
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "apply-twice".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "i.add".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "quad".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall {
                            name: "apply-twice".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "apply-twice".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
            ],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_deep_stack_types() {
        // Test with many values on stack (10+ items)
        // : test ( Int Int Int Int Int Int Int Int Int Int -- Int )
        //   add add add add add add add add add ;
        let mut stack_type = StackType::Empty;
        for _ in 0..10 {
            stack_type = stack_type.push(Type::Int);
        }

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(stack_type, StackType::singleton(Type::Int))),
                body: vec![
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_simple_quotation() {
        // : test ( -- Quot )
        //   [ 1 add ] ;
        // Quotation type should be [ ..input Int -- ..input Int ] (row polymorphic)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("input".to_string()).push(Type::Int),
                        StackType::RowVar("input".to_string()).push(Type::Int),
                    )))),
                )),
                body: vec![Statement::Quotation {
                    id: 0,
                    body: vec![
                        Statement::IntLiteral(1),
                        Statement::WordCall {
                            name: "i.add".to_string(),
                            span: None,
                        },
                    ],
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        match checker.check_program(&program) {
            Ok(_) => {}
            Err(e) => panic!("Type check failed: {}", e),
        }
    }

    #[test]
    fn test_empty_quotation() {
        // : test ( -- Quot )
        //   [ ] ;
        // Empty quotation has effect ( ..input -- ..input ) (preserves stack)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("input".to_string()),
                        StackType::RowVar("input".to_string()),
                    )))),
                )),
                body: vec![Statement::Quotation {
                    id: 1,
                    body: vec![],
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    // TODO: Re-enable once write_line is properly row-polymorphic
    // #[test]
    // fn test_quotation_with_string() {
    //     // : test ( -- Quot )
    //     //   [ "hello" write_line ] ;
    //     let program = Program { includes: vec![],
    //         words: vec![WordDef {
    //             name: "test".to_string(),
    //             effect: Some(Effect::new(
    //                 StackType::Empty,
    //                 StackType::singleton(Type::Quotation(Box::new(Effect::new(
    //                     StackType::RowVar("input".to_string()),
    //                     StackType::RowVar("input".to_string()),
    //                 )))),
    //             )),
    //             body: vec![Statement::Quotation(vec![
    //                 Statement::StringLiteral("hello".to_string()),
    //                 Statement::WordCall { name: "write_line".to_string(), span: None },
    //             ])],
    //         }],
    //     };
    //
    //     let mut checker = TypeChecker::new();
    //     assert!(checker.check_program(&program).is_ok());
    // }

    #[test]
    fn test_nested_quotation() {
        // : test ( -- Quot )
        //   [ [ 1 add ] ] ;
        // Outer quotation contains inner quotation (both row-polymorphic)
        let inner_quot_type = Type::Quotation(Box::new(Effect::new(
            StackType::RowVar("input".to_string()).push(Type::Int),
            StackType::RowVar("input".to_string()).push(Type::Int),
        )));

        let outer_quot_type = Type::Quotation(Box::new(Effect::new(
            StackType::RowVar("input".to_string()),
            StackType::RowVar("input".to_string()).push(inner_quot_type.clone()),
        )));

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(outer_quot_type),
                )),
                body: vec![Statement::Quotation {
                    id: 2,
                    body: vec![Statement::Quotation {
                        id: 3,
                        body: vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.add".to_string(),
                                span: None,
                            },
                        ],
                    }],
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_invalid_field_type_error() {
        use crate::ast::{UnionDef, UnionField, UnionVariant};

        let program = Program {
            includes: vec![],
            unions: vec![UnionDef {
                name: "Message".to_string(),
                variants: vec![UnionVariant {
                    name: "Get".to_string(),
                    fields: vec![UnionField {
                        name: "chan".to_string(),
                        type_name: "InvalidType".to_string(),
                    }],
                    source: None,
                }],
                source: None,
            }],
            words: vec![],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Unknown type 'InvalidType'"));
        assert!(err.contains("chan"));
        assert!(err.contains("Get"));
        assert!(err.contains("Message"));
    }

    #[test]
    fn test_roll_inside_conditional_with_concrete_stack() {
        // Bug #93: n roll inside if/else should work when stack has enough concrete items
        // : test ( Int Int Int Int -- Int Int Int Int )
        //   dup 0 > if
        //     3 roll    # Works: 4 concrete items available
        //   else
        //     rot rot   # Alternative that also works
        //   then ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(0),
                    Statement::WordCall {
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(3),
                            Statement::WordCall {
                                name: "roll".to_string(),
                                span: None,
                            },
                        ],
                        else_branch: Some(vec![
                            Statement::WordCall {
                                name: "rot".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "rot".to_string(),
                                span: None,
                            },
                        ]),
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        // This should now work because both branches have 4 concrete items
        match checker.check_program(&program) {
            Ok(_) => {}
            Err(e) => panic!("Type check failed: {}", e),
        }
    }

    #[test]
    fn test_roll_inside_match_arm_with_concrete_stack() {
        // Similar to bug #93 but for match arms: n roll inside match should work
        // when stack has enough concrete items from the match context
        use crate::ast::{MatchArm, Pattern, UnionDef, UnionVariant};

        // Define a simple union: union Result = Ok | Err
        let union_def = UnionDef {
            name: "Result".to_string(),
            variants: vec![
                UnionVariant {
                    name: "Ok".to_string(),
                    fields: vec![],
                    source: None,
                },
                UnionVariant {
                    name: "Err".to_string(),
                    fields: vec![],
                    source: None,
                },
            ],
            source: None,
        };

        // : test ( Int Int Int Int Result -- Int Int Int Int )
        //   match
        //     Ok => 3 roll
        //     Err => rot rot
        //   end ;
        let program = Program {
            includes: vec![],
            unions: vec![union_def],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Union("Result".to_string())),
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int)
                        .push(Type::Int),
                )),
                body: vec![Statement::Match {
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Variant("Ok".to_string()),
                            body: vec![
                                Statement::IntLiteral(3),
                                Statement::WordCall {
                                    name: "roll".to_string(),
                                    span: None,
                                },
                            ],
                        },
                        MatchArm {
                            pattern: Pattern::Variant("Err".to_string()),
                            body: vec![
                                Statement::WordCall {
                                    name: "rot".to_string(),
                                    span: None,
                                },
                                Statement::WordCall {
                                    name: "rot".to_string(),
                                    span: None,
                                },
                            ],
                        },
                    ],
                }],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        match checker.check_program(&program) {
            Ok(_) => {}
            Err(e) => panic!("Type check failed: {}", e),
        }
    }

    #[test]
    fn test_roll_with_row_polymorphic_input() {
        // roll reaching into row variable should work (needed for stdlib)
        // : test ( ..a Int Int Int -- ..a Int Int Int ??? )
        //   3 roll ;   # Reaches into ..a, generates fresh type
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None, // No declared effect - polymorphic inference
                body: vec![
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "roll".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        // This should succeed - roll into row variable is allowed for polymorphic words
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_pick_with_row_polymorphic_input() {
        // pick reaching into row variable should work (needed for stdlib)
        // : test ( ..a Int Int -- ..a Int Int ??? )
        //   2 pick ;   # Reaches into ..a, generates fresh type
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None, // No declared effect - polymorphic inference
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::WordCall {
                        name: "pick".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let mut checker = TypeChecker::new();
        // This should succeed - pick into row variable is allowed for polymorphic words
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_valid_union_reference_in_field() {
        use crate::ast::{UnionDef, UnionField, UnionVariant};

        let program = Program {
            includes: vec![],
            unions: vec![
                UnionDef {
                    name: "Inner".to_string(),
                    variants: vec![UnionVariant {
                        name: "Val".to_string(),
                        fields: vec![UnionField {
                            name: "x".to_string(),
                            type_name: "Int".to_string(),
                        }],
                        source: None,
                    }],
                    source: None,
                },
                UnionDef {
                    name: "Outer".to_string(),
                    variants: vec![UnionVariant {
                        name: "Wrap".to_string(),
                        fields: vec![UnionField {
                            name: "inner".to_string(),
                            type_name: "Inner".to_string(), // Reference to other union
                        }],
                        source: None,
                    }],
                    source: None,
                },
            ],
            words: vec![],
        };

        let mut checker = TypeChecker::new();
        assert!(
            checker.check_program(&program).is_ok(),
            "Union reference in field should be valid"
        );
    }
}
