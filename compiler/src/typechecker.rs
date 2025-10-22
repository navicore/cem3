//! Minimal type checker for cem3
//!
//! Currently focused on validating stack effects for conditional branches.
//! Based on cem2's type checker but simplified for initial implementation.
//!
//! Future work: Full bidirectional type checking with row polymorphism (see cem2)

use crate::ast::{Program, Statement, WordDef};

/// Simple stack depth tracker for validation
#[derive(Debug, Clone, PartialEq)]
struct StackDepth {
    depth: i32, // Can be negative to represent "unknown + N"
}

impl StackDepth {
    fn new() -> Self {
        StackDepth { depth: 0 }
    }

    /// Push a value onto the stack
    fn push(&self) -> Self {
        StackDepth {
            depth: self.depth + 1,
        }
    }

    /// Pop a value from the stack
    fn pop(&self) -> Result<Self, String> {
        Ok(StackDepth {
            depth: self.depth - 1,
        })
    }

    /// Check if two stack depths are compatible (same relative depth)
    fn compatible_with(&self, other: &StackDepth) -> bool {
        self.depth == other.depth
    }
}

pub struct TypeChecker;

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker
    }

    /// Type check a complete program
    pub fn check_program(&mut self, program: &Program) -> Result<(), String> {
        for word in &program.words {
            self.check_word(word)?;
        }
        Ok(())
    }

    /// Type check a word definition
    fn check_word(&self, word: &WordDef) -> Result<(), String> {
        let start_depth = StackDepth::new();
        let _final_depth = self.check_statements(&word.body, start_depth)?;
        // Note: We don't validate final depth matches declaration yet
        // (no stack effect declarations in current AST)
        Ok(())
    }

    /// Check a sequence of statements
    fn check_statements(
        &self,
        statements: &[Statement],
        mut depth: StackDepth,
    ) -> Result<StackDepth, String> {
        for stmt in statements {
            depth = self.check_statement(stmt, depth)?;
        }
        Ok(depth)
    }

    /// Check a single statement and return the resulting stack depth
    fn check_statement(
        &self,
        statement: &Statement,
        depth: StackDepth,
    ) -> Result<StackDepth, String> {
        match statement {
            Statement::IntLiteral(_) | Statement::BoolLiteral(_) | Statement::StringLiteral(_) => {
                // Literals push one value
                Ok(depth.push())
            }

            Statement::WordCall(name) => {
                // For built-ins, we know their stack effects
                self.apply_builtin_effect(name, depth)
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                // Pop the condition
                let depth_after_cond = depth.pop().map_err(|_| {
                    "if: stack underflow - condition requires 1 value on stack".to_string()
                })?;

                // Check then branch
                let then_depth = self.check_statements(then_branch, depth_after_cond.clone())?;

                // Check else branch (or use depth_after_cond if no else)
                let else_depth = if let Some(else_stmts) = else_branch {
                    self.check_statements(else_stmts, depth_after_cond)?
                } else {
                    depth_after_cond
                };

                // CRITICAL: Both branches must produce the same stack depth
                if !then_depth.compatible_with(&else_depth) {
                    return Err(format!(
                        "if branches have incompatible stack effects: \
                         then branch results in depth {}, \
                         else branch results in depth {}",
                        then_depth.depth, else_depth.depth
                    ));
                }

                Ok(then_depth)
            }
        }
    }

    /// Apply the stack effect of a built-in word
    fn apply_builtin_effect(&self, name: &str, depth: StackDepth) -> Result<StackDepth, String> {
        match name {
            // I/O operations
            "write_line" => depth.pop(), // ( str -- )
            "read_line" => Ok(depth.push()), // ( -- str )

            // Arithmetic operations ( a b -- result )
            "add" | "subtract" | "multiply" | "divide" => depth.pop()?.pop().map(|d| d.push()),

            // Comparison operations ( a b -- flag )
            "=" | "<" | ">" | "<=" | ">=" | "<>" => depth.pop()?.pop().map(|d| d.push()),

            // Stack operations
            "dup" => Ok(depth.push()),              // ( a -- a a )
            "drop" => depth.pop(),                  // ( a -- )
            "swap" => Ok(depth),                    // ( a b -- b a )
            "over" => Ok(depth.push()),             // ( a b -- a b a )
            "rot" => Ok(depth),                     // ( a b c -- b c a )
            "nip" => depth.pop(),                   // ( a b -- b )
            "tuck" => Ok(depth.push()),             // ( a b -- b a b )

            // User-defined word - we don't know its effect yet
            // In a full type system, we'd look this up
            _ => Ok(depth), // Assume net-zero for now
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
    use crate::ast::{Program, Statement, WordDef};

    #[test]
    fn test_simple_conditional() {
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                body: vec![
                    Statement::IntLiteral(5),
                    Statement::IntLiteral(3),
                    Statement::WordCall(">".to_string()),
                    Statement::If {
                        then_branch: vec![Statement::StringLiteral("yes".to_string())],
                        else_branch: Some(vec![Statement::StringLiteral("no".to_string())]),
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }

    #[test]
    fn test_mismatched_branches() {
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                body: vec![
                    Statement::IntLiteral(1),
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(1),
                            Statement::IntLiteral(2),
                        ], // Pushes 2
                        else_branch: Some(vec![Statement::IntLiteral(1)]), // Pushes 1
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        let result = checker.check_program(&program);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("incompatible stack effects"));
    }

    #[test]
    fn test_nested_conditionals() {
        let program = Program {
            words: vec![WordDef {
                name: "test".to_string(),
                body: vec![
                    Statement::IntLiteral(1),
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(2),
                            Statement::If {
                                then_branch: vec![Statement::StringLiteral("a".to_string())],
                                else_branch: Some(vec![Statement::StringLiteral("b".to_string())]),
                            },
                        ],
                        else_branch: Some(vec![Statement::StringLiteral("c".to_string())]),
                    },
                ],
            }],
        };

        let mut checker = TypeChecker::new();
        assert!(checker.check_program(&program).is_ok());
    }
}
