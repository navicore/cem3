//! Enhanced type checker for cem3 with full type tracking
//!
//! Uses row polymorphism and unification to verify stack effects.
//! Based on cem2's type checker but simplified for Phase 8.5.

use crate::ast::{Program, Statement, WordDef};
use crate::builtins::builtin_signature;
use crate::types::{Effect, StackType, Type};
use crate::unification::unify_stacks;
use std::collections::HashMap;

pub struct TypeChecker {
    /// Environment mapping word names to their effects
    env: HashMap<String, Effect>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            env: HashMap::new(),
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
            // Infer the result stack starting from declared input
            let result_stack = self.infer_statements_from(&word.body, &declared_effect.inputs)?;

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
    ) -> Result<StackType, String> {
        let mut current_stack = start_stack.clone();

        for stmt in statements {
            current_stack = self.infer_statement(stmt, current_stack)?;
        }

        Ok(current_stack)
    }

    /// Infer the stack effect of a sequence of statements (for compatibility)
    fn infer_statements(&self, statements: &[Statement]) -> Result<Effect, String> {
        let start = StackType::RowVar("input".to_string());
        let result = self.infer_statements_from(statements, &start)?;

        Ok(Effect::new(start, result))
    }

    /// Infer the resulting stack type after a statement
    /// Takes current stack, returns new stack after statement
    fn infer_statement(
        &self,
        statement: &Statement,
        current_stack: StackType,
    ) -> Result<StackType, String> {
        match statement {
            Statement::IntLiteral(_) => {
                // Push Int onto stack
                Ok(current_stack.push(Type::Int))
            }

            Statement::BoolLiteral(_) => {
                // Push Bool onto stack (currently represented as Int at runtime)
                // But we track it as Int in the type system
                Ok(current_stack.push(Type::Int))
            }

            Statement::StringLiteral(_) => {
                // Push String onto stack
                Ok(current_stack.push(Type::String))
            }

            Statement::WordCall(name) => {
                // Look up word's effect
                let effect = self
                    .lookup_word_effect(name)
                    .ok_or_else(|| format!("Unknown word: '{}'", name))?;

                // Apply the effect to current stack
                self.apply_effect(&effect, current_stack, name)
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                // Pop condition (must be Int/Bool)
                let (stack_after_cond, cond_type) = self.pop_type(current_stack, "if condition")?;

                // Condition must be Int (Forth-style: 0 = false, non-zero = true)
                let subst = unify_stacks(
                    &StackType::singleton(Type::Int),
                    &StackType::singleton(cond_type),
                )
                .map_err(|e| format!("if condition must be Int: {}", e))?;

                let stack_after_cond = subst.apply_stack(&stack_after_cond);

                // Infer then branch
                let then_effect = self.infer_statements(then_branch)?;
                let then_result =
                    self.apply_effect(&then_effect, stack_after_cond.clone(), "if then")?;

                // Infer else branch (or use stack_after_cond if no else)
                let else_result = if let Some(else_stmts) = else_branch {
                    let else_effect = self.infer_statements(else_stmts)?;
                    self.apply_effect(&else_effect, stack_after_cond, "if else")?
                } else {
                    stack_after_cond
                };

                // Both branches must produce compatible stacks
                unify_stacks(&then_result, &else_result).map_err(|e| {
                    format!(
                        "if branches have incompatible stack effects: then={:?}, else={:?}: {}",
                        then_result, else_result, e
                    )
                })?;

                Ok(then_result)
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
    fn apply_effect(
        &self,
        effect: &Effect,
        current_stack: StackType,
        operation: &str,
    ) -> Result<StackType, String> {
        // Unify current stack with effect's input
        let subst = unify_stacks(&effect.inputs, &current_stack).map_err(|e| {
            format!(
                "{}: stack type mismatch. Expected {:?}, got {:?}: {}",
                operation, effect.inputs, current_stack, e
            )
        })?;

        // Apply substitution to output
        let result_stack = subst.apply_stack(&effect.outputs);

        Ok(result_stack)
    }

    /// Pop a type from a stack type, returning (rest, top)
    fn pop_type(&self, stack: StackType, context: &str) -> Result<(StackType, Type), String> {
        match stack {
            StackType::Cons { rest, top } => Ok((*rest, top)),
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
        // : test ( Int Int Int -- String )
        //   > if
        //     > if "both true" else "first true" then
        //   else
        //     "first false"
        //   then ;
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                effect: Some(Effect::new(
                    StackType::Empty
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
                                then_branch: vec![Statement::StringLiteral("both true".to_string())],
                                else_branch: Some(vec![Statement::StringLiteral(
                                    "first true".to_string(),
                                )]),
                            },
                        ],
                        else_branch: Some(vec![Statement::StringLiteral("first false".to_string())]),
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
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
}
