//! Abstract Syntax Tree for cem3
//!
//! Minimal AST sufficient for hello-world and basic programs.
//! Will be extended as we add more language features.

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub words: Vec<WordDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordDef {
    pub name: String,
    pub body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Integer literal: pushes value onto stack
    IntLiteral(i64),

    /// Boolean literal: pushes true/false onto stack
    BoolLiteral(bool),

    /// String literal: pushes string onto stack
    StringLiteral(String),

    /// Word call: calls another word or built-in
    WordCall(String),

    /// Conditional: if/else/then
    ///
    /// Pops an integer from the stack (0 = zero, non-zero = non-zero)
    /// and executes the appropriate branch
    If {
        /// Statements to execute when condition is non-zero (the 'then' clause)
        then_branch: Vec<Statement>,
        /// Optional statements to execute when condition is zero (the 'else' clause)
        else_branch: Option<Vec<Statement>>,
    },
}

impl Program {
    pub fn new() -> Self {
        Program { words: Vec::new() }
    }

    pub fn find_word(&self, name: &str) -> Option<&WordDef> {
        self.words.iter().find(|w| w.name == name)
    }

    /// Validate that all word calls reference either a defined word or a built-in
    pub fn validate_word_calls(&self) -> Result<(), String> {
        // List of known runtime built-ins
        // IMPORTANT: Keep this in sync with codegen.rs WordCall matching
        let builtins = [
            // I/O operations
            "write_line",
            "read_line",
            "int->string",
            // Arithmetic operations
            "add",
            "subtract",
            "multiply",
            "divide",
            // Comparison operations (return 0 or 1)
            "=",
            "<",
            ">",
            "<=",
            ">=",
            "<>",
            // Stack operations (simple - no parameters)
            "dup",
            "drop",
            "swap",
            "over",
            "rot",
            "nip",
            "tuck",
            // Note: pick is omitted - requires parameter support in AST
            // Concurrency operations
            "make-channel",
            "send",
            "receive",
            "close-channel",
            "yield",
            // Note: spawn omitted - requires quotation support in AST
        ];

        for word in &self.words {
            self.validate_statements(&word.body, &word.name, &builtins)?;
        }

        Ok(())
    }

    /// Helper to validate word calls in a list of statements (recursively)
    fn validate_statements(
        &self,
        statements: &[Statement],
        word_name: &str,
        builtins: &[&str],
    ) -> Result<(), String> {
        for statement in statements {
            match statement {
                Statement::WordCall(name) => {
                    // Check if it's a built-in
                    if builtins.contains(&name.as_str()) {
                        continue;
                    }
                    // Check if it's a user-defined word
                    if self.find_word(name).is_some() {
                        continue;
                    }
                    // Undefined word!
                    return Err(format!(
                        "Undefined word '{}' called in word '{}'. \
                         Did you forget to define it or misspell a built-in?",
                        name, word_name
                    ));
                }
                Statement::If {
                    then_branch,
                    else_branch,
                } => {
                    // Recursively validate both branches
                    self.validate_statements(then_branch, word_name, builtins)?;
                    if let Some(eb) = else_branch {
                        self.validate_statements(eb, word_name, builtins)?;
                    }
                }
                _ => {} // Literals don't need validation
            }
        }
        Ok(())
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_builtin_words() {
        let program = Program {
            words: vec![WordDef {
                name: "main".to_string(),
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("write_line".to_string()),
                ],
            }],
        };

        // Should succeed - add and write_line are built-ins
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_user_defined_words() {
        let program = Program {
            words: vec![
                WordDef {
                    name: "helper".to_string(),
                    body: vec![Statement::IntLiteral(42)],
                },
                WordDef {
                    name: "main".to_string(),
                    body: vec![Statement::WordCall("helper".to_string())],
                },
            ],
        };

        // Should succeed - helper is defined
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_undefined_word() {
        let program = Program {
            words: vec![WordDef {
                name: "main".to_string(),
                body: vec![Statement::WordCall("undefined_word".to_string())],
            }],
        };

        // Should fail - undefined_word is not a built-in or user-defined word
        let result = program.validate_word_calls();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("undefined_word"));
        assert!(error.contains("main"));
    }

    #[test]
    fn test_validate_misspelled_builtin() {
        let program = Program {
            words: vec![WordDef {
                name: "main".to_string(),
                body: vec![Statement::WordCall("wrte_line".to_string())], // typo
            }],
        };

        // Should fail with helpful message
        let result = program.validate_word_calls();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("wrte_line"));
        assert!(error.contains("misspell"));
    }
}
