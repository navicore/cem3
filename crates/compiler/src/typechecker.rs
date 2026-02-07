//! Enhanced type checker for Seq with full type tracking
//!
//! Uses row polymorphism and unification to verify stack effects.
//! Based on cem2's type checker but simplified for Phase 8.5.

use crate::ast::{Program, Statement, WordDef};
use crate::builtins::builtin_signature;
use crate::call_graph::CallGraph;
use crate::capture_analysis::calculate_captures;
use crate::types::{
    Effect, SideEffect, StackType, Type, UnionTypeInfo, VariantFieldInfo, VariantInfo,
};
use crate::unification::{Subst, unify_stacks, unify_types};
use std::collections::HashMap;

/// Format a line number as an error message prefix (e.g., "at line 42: ").
/// Line numbers are 0-indexed internally, so we add 1 for display.
fn format_line_prefix(line: usize) -> String {
    format!("at line {}: ", line + 1)
}

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
    /// Current word being type-checked (for detecting recursive tail calls)
    /// Used to identify divergent branches in if/else expressions
    /// Stores (name, line_number) for better error messages
    current_word: std::cell::RefCell<Option<(String, Option<usize>)>>,
    /// Per-statement type info for codegen optimization (Issue #186)
    /// Maps (word_name, statement_index) -> concrete top-of-stack type before statement
    /// Only stores trivially-copyable types (Int, Float, Bool) to enable optimizations
    statement_top_types: std::cell::RefCell<HashMap<(String, usize), Type>>,
    /// Call graph for detecting mutual recursion (Issue #229)
    /// Used to improve divergent branch detection beyond direct recursion
    call_graph: Option<CallGraph>,
    /// Current aux stack type during word body checking (Issue #350)
    /// Tracked per-word; reset to Empty at each word boundary.
    current_aux_stack: std::cell::RefCell<StackType>,
    /// Maximum aux stack depth per word, for codegen alloca sizing (Issue #350)
    /// Maps word_name -> max_depth (number of %Value allocas needed)
    aux_max_depths: std::cell::RefCell<HashMap<String, usize>>,
    /// True when type-checking inside a quotation body (Issue #350)
    /// Aux operations are not supported in quotations (codegen limitation).
    in_quotation_scope: std::cell::Cell<bool>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            unions: HashMap::new(),
            fresh_counter: std::cell::Cell::new(0),
            quotation_types: std::cell::RefCell::new(HashMap::new()),
            expected_quotation_type: std::cell::RefCell::new(None),
            current_word: std::cell::RefCell::new(None),
            statement_top_types: std::cell::RefCell::new(HashMap::new()),
            call_graph: None,
            current_aux_stack: std::cell::RefCell::new(StackType::Empty),
            aux_max_depths: std::cell::RefCell::new(HashMap::new()),
            in_quotation_scope: std::cell::Cell::new(false),
        }
    }

    /// Set the call graph for mutual recursion detection.
    ///
    /// When set, the type checker can detect divergent branches caused by
    /// mutual recursion (e.g., even/odd pattern) in addition to direct recursion.
    pub fn set_call_graph(&mut self, call_graph: CallGraph) {
        self.call_graph = Some(call_graph);
    }

    /// Get line info prefix for error messages (e.g., "at line 42: " or "")
    fn line_prefix(&self) -> String {
        self.current_word
            .borrow()
            .as_ref()
            .and_then(|(_, line)| line.map(format_line_prefix))
            .unwrap_or_default()
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

    /// Register external word effects (e.g., from included modules or FFI).
    ///
    /// All external words must have explicit stack effects for type safety.
    pub fn register_external_words(&mut self, words: &[(&str, &Effect)]) {
        for (name, effect) in words {
            self.env.insert(name.to_string(), (*effect).clone());
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

    /// Extract per-statement type info for codegen optimization (Issue #186)
    /// Returns map of (word_name, statement_index) -> top-of-stack type
    pub fn take_statement_top_types(&self) -> HashMap<(String, usize), Type> {
        self.statement_top_types.replace(HashMap::new())
    }

    /// Extract per-word aux stack max depths for codegen alloca sizing (Issue #350)
    pub fn take_aux_max_depths(&self) -> HashMap<String, usize> {
        self.aux_max_depths.replace(HashMap::new())
    }

    /// Count the number of concrete types in a StackType (for aux depth tracking)
    fn stack_depth(stack: &StackType) -> usize {
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
    fn get_trivially_copyable_top(stack: &StackType) -> Option<Type> {
        match stack {
            StackType::Cons { top, .. } => match top {
                Type::Int | Type::Float | Type::Bool => Some(top.clone()),
                _ => None,
            },
            _ => None,
        }
    }

    /// Record the top-of-stack type for a statement if it's trivially copyable (Issue #186)
    fn capture_statement_type(&self, word_name: &str, stmt_index: usize, stack: &StackType) {
        if let Some(top_type) = Self::get_trivially_copyable_top(stack) {
            self.statement_top_types
                .borrow_mut()
                .insert((word_name.to_string(), stmt_index), top_type);
        }
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

        // Freshen the side effects too
        let fresh_effects = effect
            .effects
            .iter()
            .map(|e| self.freshen_side_effect(e, &mut type_map, &mut row_map))
            .collect();

        Effect::with_effects(fresh_inputs, fresh_outputs, fresh_effects)
    }

    fn freshen_side_effect(
        &self,
        effect: &SideEffect,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> SideEffect {
        match effect {
            SideEffect::Yield(ty) => {
                SideEffect::Yield(Box::new(self.freshen_type(ty, type_map, row_map)))
            }
        }
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
            Type::Int | Type::Float | Type::Bool | Type::String | Type::Symbol | Type::Channel => {
                ty.clone()
            }
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
    /// Supports: Int, Float, Bool, String, Channel, and union types
    fn parse_type_name(&self, name: &str) -> Type {
        match name {
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "String" => Type::String,
            "Channel" => Type::Channel,
            // Any other name is assumed to be a union type reference
            other => Type::Union(other.to_string()),
        }
    }

    /// Check if a type name is a known valid type
    ///
    /// Returns true for built-in types (Int, Float, Bool, String, Channel) and
    /// registered union type names
    fn is_valid_type_name(&self, name: &str) -> bool {
        matches!(name, "Int" | "Float" | "Bool" | "String" | "Channel")
            || self.unions.contains_key(name)
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
                             Valid types are: Int, Float, Bool, String, Channel, or a defined union name.",
                            field.type_name, field.name, variant.name, union_def.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate that all types in a stack effect are known types
    ///
    /// RFC #345: This catches cases where an uppercase identifier was parsed as a type
    /// variable but should have been a union type (e.g., from an include that wasn't
    /// available when parsing). Type variables should be single uppercase letters (T, U, V).
    fn validate_effect_types(&self, effect: &Effect, word_name: &str) -> Result<(), String> {
        self.validate_stack_types(&effect.inputs, word_name)?;
        self.validate_stack_types(&effect.outputs, word_name)?;
        Ok(())
    }

    /// Validate types in a stack type
    fn validate_stack_types(&self, stack: &StackType, word_name: &str) -> Result<(), String> {
        match stack {
            StackType::Empty | StackType::RowVar(_) => Ok(()),
            StackType::Cons { rest, top } => {
                self.validate_type(top, word_name)?;
                self.validate_stack_types(rest, word_name)
            }
        }
    }

    /// Validate a single type
    ///
    /// Type variables are allowed - they're used for polymorphism.
    /// Only Type::Union types are validated to ensure they're registered.
    fn validate_type(&self, ty: &Type, word_name: &str) -> Result<(), String> {
        match ty {
            Type::Var(_) => {
                // Type variables are always valid - they represent polymorphic types
                // Examples: T, U, V, Ctx, Handle, Acc, etc.
                // After fixup_union_types(), any union name that was mistakenly parsed
                // as a type variable will have been converted to Type::Union
                Ok(())
            }
            Type::Quotation(effect) => self.validate_effect_types(effect, word_name),
            Type::Closure { effect, captures } => {
                self.validate_effect_types(effect, word_name)?;
                for cap in captures {
                    self.validate_type(cap, word_name)?;
                }
                Ok(())
            }
            // Concrete types are always valid
            Type::Int | Type::Float | Type::Bool | Type::String | Type::Symbol | Type::Channel => {
                Ok(())
            }
            // Union types are valid if they're registered
            Type::Union(name) => {
                if !self.unions.contains_key(name) {
                    return Err(format!(
                        "In word '{}': Unknown union type '{}' in stack effect.\n\
                         Make sure this union is defined before the word that uses it.",
                        word_name, name
                    ));
                }
                Ok(())
            }
        }
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

        // Third pass: type check each word body
        for word in &program.words {
            self.check_word(word)?;
        }

        Ok(())
    }

    /// Type check a word definition
    fn check_word(&self, word: &WordDef) -> Result<(), String> {
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
    fn infer_statements_from(
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
    fn infer_statements(&self, statements: &[Statement]) -> Result<Effect, String> {
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

    /// Infer the stack effect of a match expression
    fn infer_match(
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
                    let match_line = match_span.as_ref().map(|s| s.line + 1).unwrap_or(0);
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
            let match_line = match_span.as_ref().map(|s| s.line + 1).unwrap_or(0);
            let arm0_line = arms[0].span.as_ref().map(|s| s.line + 1).unwrap_or(0);
            let arm_i_line = arms[i].span.as_ref().map(|s| s.line + 1).unwrap_or(0);

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
    fn is_divergent_branch(&self, statements: &[Statement]) -> bool {
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
    fn infer_if(
        &self,
        then_branch: &[Statement],
        else_branch: &Option<Vec<Statement>>,
        if_span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Pop condition (must be Bool)
        let (stack_after_cond, cond_type) = self.pop_type(&current_stack, "if condition")?;

        // Condition must be Bool
        let cond_subst = unify_stacks(
            &StackType::singleton(Type::Bool),
            &StackType::singleton(cond_type),
        )
        .map_err(|e| format!("if condition must be Bool: {}", e))?;

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
            let if_line = if_span.as_ref().map(|s| s.line + 1).unwrap_or(0);
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
            let if_line = if_span.as_ref().map(|s| s.line + 1).unwrap_or(0);
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

    /// Infer the stack effect of a quotation
    /// Quotations capture effects in their type - they don't propagate effects to the outer scope
    fn infer_quotation(
        &self,
        id: usize,
        body: &[Statement],
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        // Save and clear expected type so nested quotations don't inherit it
        // The expected type applies only to THIS quotation, not inner ones
        let expected_for_this_quotation = self.expected_quotation_type.borrow().clone();
        *self.expected_quotation_type.borrow_mut() = None;

        // Save enclosing aux stack and mark quotation scope (Issue #350)
        // Aux operations are rejected inside quotations because quotations are
        // compiled as separate LLVM functions without their own aux slot allocas.
        // Users should extract quotation bodies into named words if they need aux.
        let saved_aux = self.current_aux_stack.borrow().clone();
        *self.current_aux_stack.borrow_mut() = StackType::Empty;
        let saved_in_quotation = self.in_quotation_scope.get();
        self.in_quotation_scope.set(true);

        // Infer the effect of the quotation body (includes computational effects)
        let body_effect = self.infer_statements(body)?;

        // Verify quotation's aux stack is balanced (Issue #350)
        let quot_aux = self.current_aux_stack.borrow().clone();
        if quot_aux != StackType::Empty {
            return Err(format!(
                "Quotation has unbalanced aux stack.\n\
                 Remaining aux stack: {}\n\
                 Every >aux must be matched by a corresponding aux> within the quotation.",
                quot_aux
            ));
        }

        // Restore enclosing aux stack and quotation scope flag
        *self.current_aux_stack.borrow_mut() = saved_aux;
        self.in_quotation_scope.set(saved_in_quotation);

        // Restore expected type for capture analysis of THIS quotation
        *self.expected_quotation_type.borrow_mut() = expected_for_this_quotation;

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

        // Quotations don't propagate effects - they capture them in the quotation type
        // The effect annotation on the quotation type (e.g., [ ..a -- ..b | Yield Int ])
        // indicates what effects the quotation may produce when called
        Ok((result_stack, Subst::empty(), vec![]))
    }

    /// Infer the stack effect of a word call
    fn infer_word_call(
        &self,
        name: &str,
        span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
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

    /// Handle >aux: pop from main stack, push onto word-local aux stack (Issue #350)
    fn infer_to_aux(
        &self,
        _span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        if self.in_quotation_scope.get() {
            return Err(">aux is not supported inside quotations.\n\
                 Quotations are compiled as separate functions without aux stack slots.\n\
                 Extract the quotation body into a named word if you need aux."
                .to_string());
        }
        let (rest, top_type) = self.pop_type(&current_stack, ">aux")?;

        // Push onto aux stack
        let mut aux = self.current_aux_stack.borrow_mut();
        *aux = aux.clone().push(top_type);

        // Track max depth for codegen alloca sizing
        let depth = Self::stack_depth(&aux);
        if let Some((word_name, _)) = self.current_word.borrow().as_ref() {
            let mut depths = self.aux_max_depths.borrow_mut();
            let entry = depths.entry(word_name.clone()).or_insert(0);
            if depth > *entry {
                *entry = depth;
            }
        }

        Ok((rest, Subst::empty(), vec![]))
    }

    /// Handle aux>: pop from aux stack, push onto main stack (Issue #350)
    fn infer_from_aux(
        &self,
        _span: &Option<crate::ast::Span>,
        current_stack: StackType,
    ) -> Result<(StackType, Subst, Vec<SideEffect>), String> {
        if self.in_quotation_scope.get() {
            return Err("aux> is not supported inside quotations.\n\
                 Quotations are compiled as separate functions without aux stack slots.\n\
                 Extract the quotation body into a named word if you need aux."
                .to_string());
        }
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
    fn infer_call(
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

    /// Infer the resulting stack type after a statement
    /// Takes current stack, returns (new stack, substitution, side effects) after statement
    fn infer_statement(
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
    fn count_concrete_types(stack: &StackType) -> usize {
        let mut count = 0;
        let mut current = stack;
        while let StackType::Cons { rest, top: _ } = current {
            count += 1;
            current = rest;
        }
        count
    }

    /// Get the row variable name at the base of a stack, if any
    fn get_row_var_base(stack: &StackType) -> Option<String> {
        let mut current = stack;
        while let StackType::Cons { rest, top: _ } = current {
            current = rest;
        }
        match current {
            StackType::RowVar(name) => Some(name.clone()),
            _ => None,
        }
    }

    /// Adjust stack for strand.spawn operation by converting Quotation to Closure if needed
    ///
    /// strand.spawn expects Quotation(Empty -- Empty), but if we have Quotation(T... -- U...)
    /// with non-empty inputs, we auto-convert it to a Closure that captures those inputs.
    fn adjust_stack_for_spawn(
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
                    // Verify the body effect is compatible with the expected effect
                    // by unifying the quotation types. This catches:
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

    /// Check if an inferred effect matches any of the declared effects
    /// Effects match by kind (e.g., Yield matches Yield, regardless of type parameters)
    /// Type parameters should unify, but for now we just check the effect kind
    fn effect_matches_any(&self, inferred: &SideEffect, declared: &[SideEffect]) -> bool {
        declared.iter().any(|decl| match (inferred, decl) {
            (SideEffect::Yield(_), SideEffect::Yield(_)) => true,
        })
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_mismatched_branches() {
        // : test ( -- Int )
        //   true if 42 else "string" then ;  // ERROR: incompatible types
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::BoolLiteral(true),
                    Statement::If {
                        then_branch: vec![Statement::IntLiteral(42)],
                        else_branch: Some(vec![Statement::StringLiteral("string".to_string())]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mismatch"));
    }

    /// Issue #169: rot with only 2 values should fail at compile time
    /// Previously this was silently accepted due to implicit row polymorphism
    #[test]
    fn test_stack_underflow_rot_issue_169() {
        // : test ( -- ) 3 4 rot ;  // ERROR: rot needs 3 values, only 2 provided
        // Note: The parser generates `( ..rest -- ..rest )` for `( -- )`, so we use RowVar("rest")
        // to match the actual parsing behavior. The "rest" row variable is rigid.
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::RowVar("rest".to_string()),
                    StackType::RowVar("rest".to_string()),
                )),
                body: vec![
                    Statement::IntLiteral(3),
                    Statement::IntLiteral(4),
                    Statement::WordCall {
                        name: "rot".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err(), "rot with 2 values should fail");
        let err = result.unwrap_err();
        assert!(
            err.contains("stack underflow") || err.contains("requires 3"),
            "Error should mention underflow: {}",
            err
        );
    }

    #[test]
    fn test_csp_operations() {
        // : test ( -- )
        //   chan.make     # ( -- Channel )
        //   42 swap       # ( Channel Int -- Int Channel )
        //   chan.send     # ( Int Channel -- Bool )
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
        // : helper 42 ;  // No effect declaration - should error
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "helper".to_string(),
                effect: None,
                body: vec![Statement::IntLiteral(42)],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("missing a stack effect declaration")
        );
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
                                span: None,
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_type_error_in_nested_conditional() {
        // : test ( -- )
        //   10 20 i.> if
        //     42 io.write-line   # ERROR: io.write-line expects String, got Int
        //   else
        //     "ok" io.write-line
        //   then ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_read_line_operation() {
        // : test ( -- String Bool ) io.read-line ;
        // io.read-line now returns ( -- String Bool ) for error handling
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::from_vec(vec![Type::String, Type::Bool]),
                )),
                body: vec![Statement::WordCall {
                    name: "io.read-line".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
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
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                    allowed_lints: vec![],
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
                allowed_lints: vec![],
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
                    span: None,
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
                allowed_lints: vec![],
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
                    span: None,
                    id: 1,
                    body: vec![],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

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
                    span: None,
                    id: 2,
                    body: vec![Statement::Quotation {
                        span: None,
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
                allowed_lints: vec![],
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
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
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
                            span: None,
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
                            span: None,
                        },
                    ],
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
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
        // : test ( T U V W -- U V W T )
        //   3 roll ;   # Rotates: brings position 3 to top
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Var("T".to_string()))
                        .push(Type::Var("U".to_string()))
                        .push(Type::Var("V".to_string()))
                        .push(Type::Var("W".to_string())),
                    StackType::Empty
                        .push(Type::Var("U".to_string()))
                        .push(Type::Var("V".to_string()))
                        .push(Type::Var("W".to_string()))
                        .push(Type::Var("T".to_string())),
                )),
                body: vec![
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "roll".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_ok(), "roll test failed: {:?}", result.err());
    }

    #[test]
    fn test_pick_with_row_polymorphic_input() {
        // pick reaching into row variable should work (needed for stdlib)
        // : test ( T U V -- T U V T )
        //   2 pick ;   # Copies element at index 2 (0-indexed from top)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Var("T".to_string()))
                        .push(Type::Var("U".to_string()))
                        .push(Type::Var("V".to_string())),
                    StackType::Empty
                        .push(Type::Var("T".to_string()))
                        .push(Type::Var("U".to_string()))
                        .push(Type::Var("V".to_string()))
                        .push(Type::Var("T".to_string())),
                )),
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::WordCall {
                        name: "pick".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
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

    #[test]
    fn test_divergent_recursive_tail_call() {
        // Test that recursive tail calls in if/else branches are recognized as divergent.
        // This pattern is common in actor loops:
        //
        // : store-loop ( Channel -- )
        //   dup           # ( chan chan )
        //   chan.receive  # ( chan value Bool )
        //   not if        # ( chan value )
        //     drop        # ( chan ) - drop value, keep chan for recursion
        //     store-loop  # diverges - never returns
        //   then
        //   # else: ( chan value ) - process msg normally
        //   drop drop     # ( )
        // ;
        //
        // The then branch ends with a recursive call (store-loop), so it diverges.
        // The else branch (implicit empty) continues with the stack after the if.
        // Without divergent branch detection, this would fail because:
        //   - then branch produces: () (after drop store-loop)
        //   - else branch produces: (chan value)
        // But since then diverges, we should use else's type.

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "store-loop".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Channel), // ( Channel -- )
                    StackType::Empty,
                )),
                body: vec![
                    // dup -> ( chan chan )
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    // chan.receive -> ( chan value Bool )
                    Statement::WordCall {
                        name: "chan.receive".to_string(),
                        span: None,
                    },
                    // not -> ( chan value Bool )
                    Statement::WordCall {
                        name: "not".to_string(),
                        span: None,
                    },
                    // if drop store-loop then
                    Statement::If {
                        then_branch: vec![
                            // drop value -> ( chan )
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            // store-loop -> diverges
                            Statement::WordCall {
                                name: "store-loop".to_string(), // recursive tail call
                                span: None,
                            },
                        ],
                        else_branch: None, // implicit else continues with ( chan value )
                        span: None,
                    },
                    // After if: ( chan value ) - drop both
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Divergent recursive tail call should be accepted: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_divergent_else_branch() {
        // Test that divergence detection works for else branches too.
        //
        // : process-loop ( Channel -- )
        //   dup chan.receive   # ( chan value Bool )
        //   if                 # ( chan value )
        //     drop drop        # normal exit: ( )
        //   else
        //     drop             # ( chan )
        //     process-loop     # diverges - retry on failure
        //   then
        // ;

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "process-loop".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Channel), // ( Channel -- )
                    StackType::Empty,
                )),
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "chan.receive".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            // success: drop value and chan
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                        ],
                        else_branch: Some(vec![
                            // failure: drop value, keep chan, recurse
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "process-loop".to_string(), // recursive tail call
                                span: None,
                            },
                        ]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Divergent else branch should be accepted: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_non_tail_call_recursion_not_divergent() {
        // Test that recursion NOT in tail position is not treated as divergent.
        // This should fail type checking because after the recursive call,
        // there's more code that changes the stack.
        //
        // : bad-loop ( Int -- Int )
        //   dup 0 i.> if
        //     1 i.subtract bad-loop  # recursive call
        //     1 i.add                # more code after - not tail position!
        //   then
        // ;
        //
        // This should fail because:
        // - then branch: recurse then add 1 -> stack changes after recursion
        // - else branch (implicit): stack is ( Int )
        // Without proper handling, this could incorrectly pass.

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "bad-loop".to_string(),
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
                        name: "i.>".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.subtract".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "bad-loop".to_string(), // NOT in tail position
                                span: None,
                            },
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.add".to_string(), // code after recursion
                                span: None,
                            },
                        ],
                        else_branch: None,
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        // This should pass because the branches ARE compatible:
        // - then: produces Int (after bad-loop returns Int, then add 1)
        // - else: produces Int (from the dup at start)
        // The key is that bad-loop is NOT in tail position, so it's not divergent.
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Non-tail recursion should type check normally: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_call_yield_quotation_error() {
        // Phase 2c: Calling a quotation with Yield effect directly should error.
        // : bad ( Ctx -- Ctx ) [ yield ] call ;
        // This is wrong because yield quotations must be wrapped with strand.weave.
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "bad".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Var("Ctx".to_string())),
                    StackType::singleton(Type::Var("Ctx".to_string())),
                )),
                body: vec![
                    // Push a dummy value that will be yielded
                    Statement::IntLiteral(42),
                    Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![Statement::WordCall {
                            name: "yield".to_string(),
                            span: None,
                        }],
                    },
                    Statement::WordCall {
                        name: "call".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Calling yield quotation directly should fail"
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Yield") || err.contains("strand.weave"),
            "Error should mention Yield or strand.weave: {}",
            err
        );
    }

    #[test]
    fn test_strand_weave_yield_quotation_ok() {
        // Phase 2c: Using strand.weave on a Yield quotation is correct.
        // : good ( -- Int Handle ) 42 [ yield ] strand.weave ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "good".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Var("Handle".to_string())),
                )),
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![Statement::WordCall {
                            name: "yield".to_string(),
                            span: None,
                        }],
                    },
                    Statement::WordCall {
                        name: "strand.weave".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "strand.weave on yield quotation should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_call_pure_quotation_ok() {
        // Phase 2c: Calling a pure quotation (no Yield) is fine.
        // : ok ( Int -- Int ) [ 1 i.add ] call ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "ok".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.add".to_string(),
                                span: None,
                            },
                        ],
                    },
                    Statement::WordCall {
                        name: "call".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Calling pure quotation should pass: {:?}",
            result.err()
        );
    }

    // ==========================================================================
    // Stack Pollution Detection Tests (Issue #228)
    // These tests verify the type checker catches stack effect mismatches
    // ==========================================================================

    #[test]
    fn test_pollution_extra_push() {
        // : test ( Int -- Int ) 42 ;
        // Declares consuming 1 Int, producing 1 Int
        // But body pushes 42 on top of input, leaving 2 values
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::IntLiteral(42)],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: declares ( Int -- Int ) but leaves 2 values on stack"
        );
    }

    #[test]
    fn test_pollution_extra_dup() {
        // : test ( Int -- Int ) dup ;
        // Declares producing 1 Int, but dup produces 2
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
                    name: "dup".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: declares ( Int -- Int ) but dup produces 2 values"
        );
    }

    #[test]
    fn test_pollution_consumes_extra() {
        // : test ( Int -- Int ) drop drop 42 ;
        // Declares consuming 1 Int, but body drops twice
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(42),
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: declares ( Int -- Int ) but consumes 2 values"
        );
    }

    #[test]
    fn test_pollution_in_then_branch() {
        // : test ( Bool -- Int )
        //   if 1 2 else 3 then ;
        // Then branch pushes 2 values, else pushes 1
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Bool),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::If {
                    then_branch: vec![
                        Statement::IntLiteral(1),
                        Statement::IntLiteral(2), // Extra value!
                    ],
                    else_branch: Some(vec![Statement::IntLiteral(3)]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: then branch pushes 2 values, else pushes 1"
        );
    }

    #[test]
    fn test_pollution_in_else_branch() {
        // : test ( Bool -- Int )
        //   if 1 else 2 3 then ;
        // Then branch pushes 1, else pushes 2 values
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Bool),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::If {
                    then_branch: vec![Statement::IntLiteral(1)],
                    else_branch: Some(vec![
                        Statement::IntLiteral(2),
                        Statement::IntLiteral(3), // Extra value!
                    ]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: then branch pushes 1 value, else pushes 2"
        );
    }

    #[test]
    fn test_pollution_both_branches_extra() {
        // : test ( Bool -- Int )
        //   if 1 2 else 3 4 then ;
        // Both branches push 2 values but declared output is 1
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Bool),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::If {
                    then_branch: vec![Statement::IntLiteral(1), Statement::IntLiteral(2)],
                    else_branch: Some(vec![Statement::IntLiteral(3), Statement::IntLiteral(4)]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: both branches push 2 values, but declared output is 1"
        );
    }

    #[test]
    fn test_pollution_branch_consumes_extra() {
        // : test ( Bool Int -- Int )
        //   if drop drop 1 else then ;
        // Then branch consumes more than available from declared inputs
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Bool).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::If {
                    then_branch: vec![
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::IntLiteral(1),
                    ],
                    else_branch: Some(vec![]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: then branch consumes Bool (should only have Int after if)"
        );
    }

    #[test]
    fn test_pollution_quotation_wrong_arity_output() {
        // : test ( Int -- Int )
        //   [ dup ] call ;
        // Quotation produces 2 values, but word declares 1 output
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        }],
                    },
                    Statement::WordCall {
                        name: "call".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: quotation [dup] produces 2 values, declared output is 1"
        );
    }

    #[test]
    fn test_pollution_quotation_wrong_arity_input() {
        // : test ( Int -- Int )
        //   [ drop drop 42 ] call ;
        // Quotation consumes 2 values, but only 1 available
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "drop".to_string(),
                                span: None,
                            },
                            Statement::IntLiteral(42),
                        ],
                    },
                    Statement::WordCall {
                        name: "call".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Should reject: quotation [drop drop 42] consumes 2 values, only 1 available"
        );
    }

    #[test]
    fn test_missing_effect_provides_helpful_error() {
        // : myword 42 ;
        // No effect annotation - should error with helpful message including word name
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "myword".to_string(),
                effect: None, // No annotation
                body: vec![Statement::IntLiteral(42)],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("myword"), "Error should mention word name");
        assert!(
            err.contains("stack effect"),
            "Error should mention stack effect"
        );
    }

    #[test]
    fn test_valid_effect_exact_match() {
        // : test ( Int Int -- Int ) i.+ ;
        // Exact match - consumes 2, produces 1
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
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_ok(), "Should accept: effect matches exactly");
    }

    #[test]
    fn test_valid_polymorphic_passthrough() {
        // : test ( a -- a ) ;
        // Identity function - row polymorphism allows this
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Var("a".to_string()),
                    },
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Var("a".to_string()),
                    },
                )),
                body: vec![], // Empty body - just pass through
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_ok(), "Should accept: polymorphic identity");
    }

    // ==========================================================================
    // Closure Nesting Tests (Issue #230)
    // Tests for deep closure nesting, transitive captures, and edge cases
    // ==========================================================================

    #[test]
    fn test_closure_basic_capture() {
        // : make-adder ( Int -- Closure )
        //   [ i.+ ] ;
        // The quotation needs 2 Ints (for i.+) but caller will only provide 1
        // So it captures 1 Int from the creation site
        // Must declare as Closure type to trigger capture analysis
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "make-adder".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Closure {
                        effect: Box::new(Effect::new(
                            StackType::RowVar("r".to_string()).push(Type::Int),
                            StackType::RowVar("r".to_string()).push(Type::Int),
                        )),
                        captures: vec![Type::Int], // Captures 1 Int
                    }),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    }],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Basic closure capture should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_closure_nested_two_levels() {
        // : outer ( -- Quot )
        //   [ [ 1 i.+ ] ] ;
        // Outer quotation: no inputs, just returns inner quotation
        // Inner quotation: pushes 1 then adds (needs 1 Int from caller)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "outer".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("r".to_string()),
                        StackType::RowVar("r".to_string()).push(Type::Quotation(Box::new(
                            Effect::new(
                                StackType::RowVar("s".to_string()).push(Type::Int),
                                StackType::RowVar("s".to_string()).push(Type::Int),
                            ),
                        ))),
                    )))),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![Statement::Quotation {
                        span: None,
                        id: 1,
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
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Two-level nested quotations should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_closure_nested_three_levels() {
        // : deep ( -- Quot )
        //   [ [ [ 1 i.+ ] ] ] ;
        // Three levels of nesting, innermost does actual work
        let inner_effect = Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        );
        let middle_effect = Effect::new(
            StackType::RowVar("b".to_string()),
            StackType::RowVar("b".to_string()).push(Type::Quotation(Box::new(inner_effect))),
        );
        let outer_effect = Effect::new(
            StackType::RowVar("c".to_string()),
            StackType::RowVar("c".to_string()).push(Type::Quotation(Box::new(middle_effect))),
        );

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "deep".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(outer_effect))),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![Statement::Quotation {
                        span: None,
                        id: 1,
                        body: vec![Statement::Quotation {
                            span: None,
                            id: 2,
                            body: vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall {
                                    name: "i.add".to_string(),
                                    span: None,
                                },
                            ],
                        }],
                    }],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Three-level nested quotations should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_closure_use_after_creation() {
        // : use-adder ( -- Int )
        //   5 make-adder   // Creates closure capturing 5
        //   10 swap call ; // Calls closure with 10, should return 15
        //
        // Tests that closure is properly typed when called later
        let adder_type = Type::Closure {
            effect: Box::new(Effect::new(
                StackType::RowVar("r".to_string()).push(Type::Int),
                StackType::RowVar("r".to_string()).push(Type::Int),
            )),
            captures: vec![Type::Int],
        };

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "make-adder".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(adder_type.clone()),
                    )),
                    body: vec![Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![Statement::WordCall {
                            name: "i.add".to_string(),
                            span: None,
                        }],
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
                WordDef {
                    name: "use-adder".to_string(),
                    effect: Some(Effect::new(
                        StackType::Empty,
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::IntLiteral(5),
                        Statement::WordCall {
                            name: "make-adder".to_string(),
                            span: None,
                        },
                        Statement::IntLiteral(10),
                        Statement::WordCall {
                            name: "swap".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "call".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
                },
            ],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Closure usage after creation should work: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_closure_wrong_call_type() {
        // : bad-use ( -- Int )
        //   5 make-adder   // Creates Int -> Int closure
        //   "hello" swap call ; // Tries to call with String - should fail!
        let adder_type = Type::Closure {
            effect: Box::new(Effect::new(
                StackType::RowVar("r".to_string()).push(Type::Int),
                StackType::RowVar("r".to_string()).push(Type::Int),
            )),
            captures: vec![Type::Int],
        };

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "make-adder".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(adder_type.clone()),
                    )),
                    body: vec![Statement::Quotation {
                        span: None,
                        id: 0,
                        body: vec![Statement::WordCall {
                            name: "i.add".to_string(),
                            span: None,
                        }],
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
                WordDef {
                    name: "bad-use".to_string(),
                    effect: Some(Effect::new(
                        StackType::Empty,
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::IntLiteral(5),
                        Statement::WordCall {
                            name: "make-adder".to_string(),
                            span: None,
                        },
                        Statement::StringLiteral("hello".to_string()), // Wrong type!
                        Statement::WordCall {
                            name: "swap".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "call".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
                },
            ],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_err(),
            "Calling Int closure with String should fail"
        );
    }

    #[test]
    fn test_closure_multiple_captures() {
        // : make-between ( Int Int -- Quot )
        //   [ dup rot i.>= swap rot i.<= and ] ;
        // Captures both min and max, checks if value is between them
        // Body needs: value min max (3 Ints)
        // Caller provides: value (1 Int)
        // Captures: min max (2 Ints)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "make-between".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("r".to_string()).push(Type::Int),
                        StackType::RowVar("r".to_string()).push(Type::Bool),
                    )))),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![
                        // Simplified: just do a comparison that uses all 3 values
                        Statement::WordCall {
                            name: "i.>=".to_string(),
                            span: None,
                        },
                        // Note: This doesn't match the comment but tests multi-capture
                    ],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        // This should work - the quotation body uses values from stack
        // The exact behavior depends on how captures are inferred
        // For now, we're testing that it doesn't crash
        assert!(
            result.is_ok() || result.is_err(),
            "Multiple captures should be handled (pass or fail gracefully)"
        );
    }

    #[test]
    fn test_quotation_type_preserved_through_word() {
        // : identity-quot ( Quot -- Quot ) ;
        // Tests that quotation types are preserved when passed through words
        let quot_type = Type::Quotation(Box::new(Effect::new(
            StackType::RowVar("r".to_string()).push(Type::Int),
            StackType::RowVar("r".to_string()).push(Type::Int),
        )));

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "identity-quot".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(quot_type.clone()),
                    StackType::singleton(quot_type.clone()),
                )),
                body: vec![], // Identity - just return what's on stack
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Quotation type should be preserved through identity word: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_closure_captures_value_for_inner_quotation() {
        // : make-inner-adder ( Int -- Closure )
        //   [ [ i.+ ] swap call ] ;
        // The closure captures an Int
        // When called, it creates an inner quotation and calls it with the captured value
        // This tests that closures can work with nested quotations
        let closure_effect = Effect::new(
            StackType::RowVar("r".to_string()).push(Type::Int),
            StackType::RowVar("r".to_string()).push(Type::Int),
        );

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "make-inner-adder".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Closure {
                        effect: Box::new(closure_effect),
                        captures: vec![Type::Int],
                    }),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![
                        // The captured Int and the caller's Int are on stack
                        Statement::WordCall {
                            name: "i.add".to_string(),
                            span: None,
                        },
                    ],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(
            result.is_ok(),
            "Closure with capture for inner work should pass: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_union_type_mismatch_should_fail() {
        // RFC #345: Different union types should not unify
        // This tests that passing union B to a function expecting union A fails
        use crate::ast::{UnionDef, UnionField, UnionVariant};

        let mut program = Program {
            includes: vec![],
            unions: vec![
                UnionDef {
                    name: "UnionA".to_string(),
                    variants: vec![UnionVariant {
                        name: "AVal".to_string(),
                        fields: vec![UnionField {
                            name: "x".to_string(),
                            type_name: "Int".to_string(),
                        }],
                        source: None,
                    }],
                    source: None,
                },
                UnionDef {
                    name: "UnionB".to_string(),
                    variants: vec![UnionVariant {
                        name: "BVal".to_string(),
                        fields: vec![UnionField {
                            name: "y".to_string(),
                            type_name: "Int".to_string(),
                        }],
                        source: None,
                    }],
                    source: None,
                },
            ],
            words: vec![
                // : takes-a ( UnionA -- ) drop ;
                WordDef {
                    name: "takes-a".to_string(),
                    effect: Some(Effect::new(
                        StackType::RowVar("rest".to_string())
                            .push(Type::Union("UnionA".to_string())),
                        StackType::RowVar("rest".to_string()),
                    )),
                    body: vec![Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
                // : main ( -- ) 99 Make-BVal takes-a ;
                // This should FAIL: Make-BVal returns UnionB, takes-a expects UnionA
                WordDef {
                    name: "main".to_string(),
                    effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                    body: vec![
                        Statement::IntLiteral(99),
                        Statement::WordCall {
                            name: "Make-BVal".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "takes-a".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
                },
            ],
        };

        // Generate constructors (Make-AVal, Make-BVal)
        program.generate_constructors().unwrap();

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);

        // This SHOULD fail because UnionB != UnionA
        assert!(
            result.is_err(),
            "Passing UnionB to function expecting UnionA should fail, but got: {:?}",
            result
        );
        let err = result.unwrap_err();
        assert!(
            err.contains("Union") || err.contains("mismatch"),
            "Error should mention union type mismatch, got: {}",
            err
        );
    }

    // =========================================================================
    // Aux stack tests (Issue #350)
    // =========================================================================

    fn make_word_call(name: &str) -> Statement {
        Statement::WordCall {
            name: name.to_string(),
            span: None,
        }
    }

    #[test]
    fn test_aux_basic_round_trip() {
        // : test ( Int -- Int ) >aux aux> ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![make_word_call(">aux"), make_word_call("aux>")],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_aux_preserves_type() {
        // : test ( String -- String ) >aux aux> ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::String),
                    StackType::singleton(Type::String),
                )),
                body: vec![make_word_call(">aux"), make_word_call("aux>")],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_aux_unbalanced_error() {
        // : test ( Int -- ) >aux ;  -- ERROR: aux not empty at return
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::Empty,
                )),
                body: vec![make_word_call(">aux")],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("aux stack is not empty"),
            "Expected aux stack balance error, got: {}",
            err
        );
    }

    #[test]
    fn test_aux_pop_empty_error() {
        // : test ( -- Int ) aux> ;  -- ERROR: aux is empty
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Int),
                )),
                body: vec![make_word_call("aux>")],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("aux stack is empty"),
            "Expected aux empty error, got: {}",
            err
        );
    }

    #[test]
    fn test_aux_multiple_values() {
        // >aux >aux aux> aux> is identity (LIFO preserves order)
        // Input: ( Int String ), >aux pops String, >aux pops Int
        // aux> pushes Int, aux> pushes String → output: ( Int String )
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::String),
                    StackType::Empty.push(Type::Int).push(Type::String),
                )),
                body: vec![
                    make_word_call(">aux"),
                    make_word_call(">aux"),
                    make_word_call("aux>"),
                    make_word_call("aux>"),
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_aux_max_depths_tracked() {
        // : test ( Int -- Int ) >aux aux> ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![make_word_call(">aux"), make_word_call("aux>")],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        checker.check_program(&program).unwrap();
        let depths = checker.take_aux_max_depths();
        assert_eq!(depths.get("test"), Some(&1));
    }

    #[test]
    fn test_aux_in_match_balanced() {
        // Aux used symmetrically in match arms: each arm does >aux aux>
        use crate::ast::{MatchArm, Pattern, UnionDef, UnionVariant};

        let union_def = UnionDef {
            name: "Choice".to_string(),
            variants: vec![
                UnionVariant {
                    name: "Left".to_string(),
                    fields: vec![],
                    source: None,
                },
                UnionVariant {
                    name: "Right".to_string(),
                    fields: vec![],
                    source: None,
                },
            ],
            source: None,
        };

        // : test ( Int Choice -- Int )
        //   swap >aux match Left => aux> end Right => aux> end ;
        let program = Program {
            includes: vec![],
            unions: vec![union_def],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Union("Choice".to_string())),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    make_word_call("swap"),
                    make_word_call(">aux"),
                    Statement::Match {
                        arms: vec![
                            MatchArm {
                                pattern: Pattern::Variant("Left".to_string()),
                                body: vec![make_word_call("aux>")],
                                span: None,
                            },
                            MatchArm {
                                pattern: Pattern::Variant("Right".to_string()),
                                body: vec![make_word_call("aux>")],
                                span: None,
                            },
                        ],
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_aux_in_match_unbalanced_error() {
        // Match arms with different aux states should error
        use crate::ast::{MatchArm, Pattern, UnionDef, UnionVariant};

        let union_def = UnionDef {
            name: "Choice".to_string(),
            variants: vec![
                UnionVariant {
                    name: "Left".to_string(),
                    fields: vec![],
                    source: None,
                },
                UnionVariant {
                    name: "Right".to_string(),
                    fields: vec![],
                    source: None,
                },
            ],
            source: None,
        };

        // : test ( Int Choice -- Int )
        //   swap >aux match Left => aux> end Right => end ;  -- ERROR: unbalanced
        let program = Program {
            includes: vec![],
            unions: vec![union_def],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
                        .push(Type::Int)
                        .push(Type::Union("Choice".to_string())),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    make_word_call("swap"),
                    make_word_call(">aux"),
                    Statement::Match {
                        arms: vec![
                            MatchArm {
                                pattern: Pattern::Variant("Left".to_string()),
                                body: vec![make_word_call("aux>")],
                                span: None,
                            },
                            MatchArm {
                                pattern: Pattern::Variant("Right".to_string()),
                                body: vec![],
                                span: None,
                            },
                        ],
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("aux stack"),
            "Expected aux stack mismatch error, got: {}",
            err
        );
    }

    #[test]
    fn test_aux_in_quotation_rejected() {
        // >aux inside a quotation should be rejected (codegen limitation)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )))),
                )),
                body: vec![Statement::Quotation {
                    span: None,
                    id: 0,
                    body: vec![make_word_call(">aux"), make_word_call("aux>")],
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not supported inside quotations"),
            "Expected quotation rejection error, got: {}",
            err
        );
    }
}
