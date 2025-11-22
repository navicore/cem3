//! Enhanced type checker for Seq with full type tracking
//!
//! Uses row polymorphism and unification to verify stack effects.
//! Based on cem2's type checker but simplified for Phase 8.5.

use crate::ast::{Program, Statement, WordDef};
use crate::builtins::builtin_signature;
use crate::types::{Effect, StackType, Type};
use crate::unification::{Subst, unify_stacks};
use std::collections::HashMap;

pub struct TypeChecker {
    /// Environment mapping word names to their effects
    env: HashMap<String, Effect>,
    /// Counter for generating fresh type variables
    fresh_counter: std::cell::Cell<usize>,
    /// Quotation types tracked during type checking (in DFS traversal order)
    /// Each quotation encountered gets its type stored here
    quotation_types: std::cell::RefCell<Vec<Type>>,
    /// Expected quotation/closure type (from word signature, if any)
    /// Used during type-driven capture inference
    expected_quotation_type: std::cell::RefCell<Option<Type>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
            fresh_counter: std::cell::Cell::new(0),
            quotation_types: std::cell::RefCell::new(Vec::new()),
            expected_quotation_type: std::cell::RefCell::new(None),
        }
    }

    /// Extract accumulated quotation types (in DFS traversal order)
    ///
    /// This should be called after check_program() to get the inferred types
    /// for all quotations in the program.
    pub fn take_quotation_types(&self) -> Vec<Type> {
        self.quotation_types.borrow_mut().drain(..).collect()
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
            Type::Int | Type::Bool | Type::String => ty.clone(),
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
        }
    }

    /// Type check a complete program
    pub fn check_program(&mut self, program: &Program) -> Result<(), String> {
        // First pass: collect all word signatures
        for word in &program.words {
            if let Some(effect) = &word.effect {
                self.env.insert(word.name.clone(), effect.clone());
            }
        }

        // Second pass: type check each word body
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
                    "Word '{}': declared output stack ({:?}) doesn't match inferred ({:?}): {}",
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

        for stmt in statements {
            let (new_stack, subst) = self.infer_statement(stmt, current_stack)?;
            current_stack = new_stack;
            accumulated_subst = accumulated_subst.compose(&subst);
        }

        Ok((current_stack, accumulated_subst))
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

    /// Infer the resulting stack type after a statement
    /// Takes current stack, returns (new stack, substitution) after statement
    fn infer_statement(
        &self,
        statement: &Statement,
        current_stack: StackType,
    ) -> Result<(StackType, Subst), String> {
        match statement {
            Statement::IntLiteral(_) => {
                // Push Int onto stack
                Ok((current_stack.push(Type::Int), Subst::empty()))
            }

            Statement::BoolLiteral(_) => {
                // Push Bool onto stack (currently represented as Int at runtime)
                // But we track it as Int in the type system
                Ok((current_stack.push(Type::Int), Subst::empty()))
            }

            Statement::StringLiteral(_) => {
                // Push String onto stack
                Ok((current_stack.push(Type::String), Subst::empty()))
            }

            Statement::WordCall(name) => {
                // Look up word's effect
                let effect = self
                    .lookup_word_effect(name)
                    .ok_or_else(|| format!("Unknown word: '{}'", name))?;

                // Freshen the effect to avoid variable name clashes
                let fresh_effect = self.freshen_effect(&effect);

                // Apply the freshened effect to current stack
                self.apply_effect(&fresh_effect, current_stack, name)
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                // Pop condition (must be Int/Bool)
                let (stack_after_cond, cond_type) =
                    self.pop_type(&current_stack, "if condition")?;

                // Condition must be Int (Forth-style: 0 = false, non-zero = true)
                let cond_subst = unify_stacks(
                    &StackType::singleton(Type::Int),
                    &StackType::singleton(cond_type),
                )
                .map_err(|e| format!("if condition must be Int: {}", e))?;

                let stack_after_cond = cond_subst.apply_stack(&stack_after_cond);

                // Infer then branch
                let then_effect = self.infer_statements(then_branch)?;
                let (then_result, _then_subst) =
                    self.apply_effect(&then_effect, stack_after_cond.clone(), "if then")?;

                // Infer else branch (or use stack_after_cond if no else)
                let (else_result, _else_subst) = if let Some(else_stmts) = else_branch {
                    let else_effect = self.infer_statements(else_stmts)?;
                    self.apply_effect(&else_effect, stack_after_cond, "if else")?
                } else {
                    (stack_after_cond, Subst::empty())
                };

                // Both branches must produce compatible stacks
                let branch_subst = unify_stacks(&then_result, &else_result).map_err(|e| {
                    format!(
                        "if branches have incompatible stack effects: then={:?}, else={:?}: {}",
                        then_result, else_result, e
                    )
                })?;

                // Apply branch unification to get the final result
                let result = branch_subst.apply_stack(&then_result);

                // Propagate condition substitution composed with branch substitution
                let total_subst = cond_subst.compose(&branch_subst);
                Ok((result, total_subst))
            }

            Statement::Quotation(body) => {
                // Type checking for quotations with automatic capture analysis
                //
                // A quotation is a block of deferred code.
                //
                // For stateless quotations:
                //   Example: [ 1 add ]
                //   Body effect: ( -- Int )  (pushes 1, needs Int from call site, adds)
                //   Type: Quotation([Int -- Int])
                //
                // For closures (automatic capture):
                //   Example: 5 [ add ]
                //   Body effect: ( Int Int -- Int )  (needs 2 Ints)
                //   One Int will be captured from creation site
                //   Type: Closure { effect: [Int -- Int], captures: [Int] }

                // Infer the effect of the quotation body
                let body_effect = self.infer_statements(body)?;

                // Perform capture analysis
                let quot_type = self.analyze_captures(&body_effect, &current_stack)?;

                // Record this quotation's type (for CodeGen to use later)
                self.quotation_types.borrow_mut().push(quot_type.clone());

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
                "{}: stack type mismatch. Expected {:?}, got {:?}: {}",
                operation, effect.inputs, current_stack, e
            )
        })?;

        // Apply substitution to output
        let result_stack = subst.apply_stack(&effect.outputs);

        Ok((result_stack, subst))
    }

    /// Analyze quotation captures
    ///
    /// Determines whether a quotation should be stateless (Type::Quotation)
    /// or a closure (Type::Closure) based on the expected type from the word signature.
    ///
    /// Type-driven inference:
    ///   - If expected type is Closure[effect], calculate what to capture
    ///   - If expected type is Quotation[effect], return stateless
    ///   - If no expected type, default to stateless (conservative)
    ///
    /// Example:
    ///   Signature: ( Int -- Closure[Int -- Int] )
    ///   Body: [ add ]
    ///   Body effect: ( Int Int -- Int )  [add needs 2 Ints]
    ///   Expected effect: [Int -- Int]    [call site provides 1 Int]
    ///   Result: Closure { effect: [Int -- Int], captures: [Int] }
    ///           (captures 1 Int from creation stack)
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
                let captures = self.calculate_captures(body_effect, &effect)?;
                Ok(Type::Closure { effect, captures })
            }
            Some(Type::Quotation(expected_effect)) => {
                // User declared quotation type - stateless
                Ok(Type::Quotation(expected_effect))
            }
            _ => {
                // No expected type - conservative default: stateless quotation
                Ok(Type::Quotation(Box::new(body_effect.clone())))
            }
        }
    }

    /// Calculate capture types for a closure
    ///
    /// Given:
    ///   - body_effect: what the quotation body needs (e.g., Int Int -- Int)
    ///   - call_effect: what the call site will provide (e.g., Int -- Int)
    ///
    /// Calculate:
    ///   - captures: types to capture from creation stack (e.g., [Int])
    ///
    /// Example:
    ///   Body needs: (Int Int -- Int)  [2 inputs total]
    ///   Call provides: (Int -- Int)   [1 input from call site]
    ///   Captures: 2 - 1 = 1 Int       [1 input from creation site]
    ///
    /// Capture Ordering:
    ///   Captures are returned bottom-to-top (deepest value first).
    ///   This matches how push_closure pops from the stack:
    ///   - Stack at creation: ( ...rest bottom top )
    ///   - push_closure pops top-down: pop top, pop bottom
    ///   - Stores as: env[0]=top, env[1]=bottom (reversed)
    ///   - Closure function pushes: push env[0], push env[1]
    ///   - Result: bottom is deeper, top is shallower (correct order)
    fn calculate_captures(
        &self,
        body_effect: &Effect,
        call_effect: &Effect,
    ) -> Result<Vec<Type>, String> {
        // Extract concrete types from stack types (bottom to top)
        let body_inputs = self.extract_concrete_types(&body_effect.inputs);
        let call_inputs = self.extract_concrete_types(&call_effect.inputs);

        // Validate: call site shouldn't provide MORE than body needs
        if call_inputs.len() > body_inputs.len() {
            return Err(format!(
                "Closure signature error: call site provides {} values but body only needs {}",
                call_inputs.len(),
                body_inputs.len()
            ));
        }

        // Calculate how many to capture (from bottom of stack)
        let capture_count = body_inputs.len() - call_inputs.len();

        // Captures are the first N types (bottom of stack)
        // Example: body needs [Int, String] (bottom to top), call provides [String]
        // Captures: [Int] (the bottom type)
        Ok(body_inputs[0..capture_count].to_vec())
    }

    /// Extract concrete types from a stack type (bottom to top order)
    ///
    /// Example:
    ///   Input: Cons { rest: Cons { rest: Empty, top: Int }, top: String }
    ///   Output: [Int, String]  (bottom to top)
    ///
    /// Row variables are not supported - this is only for concrete stacks
    fn extract_concrete_types(&self, stack: &StackType) -> Vec<Type> {
        let mut types = Vec::new();
        let mut current = stack.clone();

        // Pop types from top to bottom
        while let Some((rest, top)) = current.pop() {
            types.push(top);
            current = rest;
        }

        // Reverse to get bottom-to-top order
        types.reverse();
        types
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::IntLiteral(42)],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_simple_operation() {
        // : test ( Int Int -- Int ) add ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::WordCall("add".to_string())],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_type_mismatch() {
        // : test ( String -- ) write_line ;  with body: 42
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::String),
                    StackType::Empty,
                )),
                body: vec![
                    Statement::IntLiteral(42), // Pushes Int, not String!
                    Statement::WordCall("write_line".to_string()),
                ],
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
            words: vec![WordDef {
                name: "my-dup".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::Empty.push(Type::Int).push(Type::Int),
                )),
                body: vec![Statement::WordCall("dup".to_string())],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::String),
                )),
                body: vec![
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![Statement::StringLiteral("greater".to_string())],
                        else_branch: Some(vec![Statement::StringLiteral(
                            "not greater".to_string(),
                        )]),
                    },
                ],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(1),
                    Statement::If {
                        then_branch: vec![Statement::IntLiteral(42)],
                        else_branch: Some(vec![Statement::StringLiteral("string".to_string())]),
                    },
                ],
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
        // : main ( -- ) 42 helper write_line ;
        let program = Program {
            words: vec![
                WordDef {
                    name: "helper".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::String),
                    )),
                    body: vec![Statement::WordCall("int->string".to_string())],
                },
                WordDef {
                    name: "main".to_string(),
                    effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                    body: vec![
                        Statement::IntLiteral(42),
                        Statement::WordCall("helper".to_string()),
                        Statement::WordCall("write_line".to_string()),
                    ],
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
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("multiply".to_string()),
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_write_line_type_error() {
        // : test ( Int -- ) write_line ;  // ERROR: write_line expects String
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::Empty,
                )),
                body: vec![Statement::WordCall("write_line".to_string())],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![Statement::WordCall("drop".to_string())],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::singleton(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::WordCall("add".to_string())],
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
        //   make-channel  # ( -- Int )
        //   42 swap       # ( Int Int -- Int Int )
        //   send          # ( Int Int -- )
        // ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![
                    Statement::WordCall("make-channel".to_string()),
                    Statement::IntLiteral(42),
                    Statement::WordCall("swap".to_string()),
                    Statement::WordCall("send".to_string()),
                ],
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
                    Statement::WordCall("rot".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_empty_program() {
        // Program with no words should be valid
        let program = Program { words: vec![] };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_word_without_effect_declaration() {
        // : helper 42 ;  // No effect declaration
        let program = Program {
            words: vec![WordDef {
                name: "helper".to_string(),
                effect: None,
                body: vec![Statement::IntLiteral(42)],
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
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![
                            Statement::WordCall(">".to_string()),
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
                            Statement::WordCall("drop".to_string()),
                            Statement::WordCall("drop".to_string()),
                            Statement::StringLiteral("first false".to_string()),
                        ]),
                    },
                ],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![Statement::IntLiteral(100)],
                        else_branch: None, // No else - should leave stack unchanged
                    },
                ],
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
        // : helper2 ( String -- ) write_line ;
        // : main ( -- ) 42 helper1 helper2 ;
        let program = Program {
            words: vec![
                WordDef {
                    name: "helper1".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::String),
                    )),
                    body: vec![Statement::WordCall("int->string".to_string())],
                },
                WordDef {
                    name: "helper2".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::String),
                        StackType::Empty,
                    )),
                    body: vec![Statement::WordCall("write_line".to_string())],
                },
                WordDef {
                    name: "main".to_string(),
                    effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                    body: vec![
                        Statement::IntLiteral(42),
                        Statement::WordCall("helper1".to_string()),
                        Statement::WordCall("helper2".to_string()),
                    ],
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
                    Statement::WordCall("over".to_string()),
                    Statement::WordCall("nip".to_string()),
                    Statement::WordCall("tuck".to_string()),
                ],
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
        //     write_line        # ( String -- )
        //   else
        //     write_line
        //   then ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::WordCall("int->string".to_string()),
                    Statement::IntLiteral(100),
                    Statement::IntLiteral(200),
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![Statement::WordCall("write_line".to_string())],
                        else_branch: Some(vec![Statement::WordCall("write_line".to_string())]),
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_string_literal() {
        // : test ( -- String ) "hello" ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::String),
                )),
                body: vec![Statement::StringLiteral("hello".to_string())],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_bool_literal() {
        // : test ( -- Int ) true ;
        // Booleans are represented as Int in the type system
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::BoolLiteral(true)],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_type_error_in_nested_conditional() {
        // : test ( Int Int -- ? )
        //   > if
        //     42 write_line   # ERROR: write_line expects String, got Int
        //   else
        //     "ok" write_line
        //   then ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(10),
                    Statement::IntLiteral(20),
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(42),
                            Statement::WordCall("write_line".to_string()),
                        ],
                        else_branch: Some(vec![
                            Statement::StringLiteral("ok".to_string()),
                            Statement::WordCall("write_line".to_string()),
                        ]),
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Type mismatch"));
    }

    #[test]
    fn test_read_line_operation() {
        // : test ( -- String ) read_line ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::String),
                )),
                body: vec![Statement::WordCall("read_line".to_string())],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_comparison_operations() {
        // Test all comparison operators
        // : test ( Int Int -- Int Int Int Int Int Int )
        //   2dup = 2dup < 2dup > 2dup <= 2dup >= 2dup <> ;
        // Simplified: just test that comparisons work
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty.push(Type::Int).push(Type::Int),
                    StackType::singleton(Type::Int),
                )),
                body: vec![Statement::WordCall("<=".to_string())],
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
            words: vec![
                WordDef {
                    name: "is-even".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall("dup".to_string()),
                        Statement::IntLiteral(0),
                        Statement::WordCall("=".to_string()),
                        Statement::If {
                            then_branch: vec![
                                Statement::WordCall("drop".to_string()),
                                Statement::IntLiteral(1),
                            ],
                            else_branch: Some(vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall("subtract".to_string()),
                                Statement::WordCall("is-odd".to_string()),
                            ]),
                        },
                    ],
                },
                WordDef {
                    name: "is-odd".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall("dup".to_string()),
                        Statement::IntLiteral(0),
                        Statement::WordCall("=".to_string()),
                        Statement::If {
                            then_branch: vec![
                                Statement::WordCall("drop".to_string()),
                                Statement::IntLiteral(0),
                            ],
                            else_branch: Some(vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall("subtract".to_string()),
                                Statement::WordCall("is-even".to_string()),
                            ]),
                        },
                    ],
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
            words: vec![
                WordDef {
                    name: "apply-twice".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall("dup".to_string()),
                        Statement::WordCall("add".to_string()),
                    ],
                },
                WordDef {
                    name: "quad".to_string(),
                    effect: Some(Effect::new(
                        StackType::singleton(Type::Int),
                        StackType::singleton(Type::Int),
                    )),
                    body: vec![
                        Statement::WordCall("apply-twice".to_string()),
                        Statement::WordCall("apply-twice".to_string()),
                    ],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(stack_type, StackType::singleton(Type::Int))),
                body: vec![
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("add".to_string()),
                ],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("input".to_string()).push(Type::Int),
                        StackType::RowVar("input".to_string()).push(Type::Int),
                    )))),
                )),
                body: vec![Statement::Quotation(vec![
                    Statement::IntLiteral(1),
                    Statement::WordCall("add".to_string()),
                ])],
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(Type::Quotation(Box::new(Effect::new(
                        StackType::RowVar("input".to_string()),
                        StackType::RowVar("input".to_string()),
                    )))),
                )),
                body: vec![Statement::Quotation(vec![])],
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
    //     let program = Program {
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
    //                 Statement::WordCall("write_line".to_string()),
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
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty,
                    StackType::singleton(outer_quot_type),
                )),
                body: vec![Statement::Quotation(vec![Statement::Quotation(vec![
                    Statement::IntLiteral(1),
                    Statement::WordCall("add".to_string()),
                ])])],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }
}
