//! Abstract Syntax Tree for Seq
//!
//! Minimal AST sufficient for hello-world and basic programs.
//! Will be extended as we add more language features.

use crate::types::Effect;
use std::path::PathBuf;

/// Source location for error reporting and tooling
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub file: PathBuf,
    /// Start line (0-indexed for LSP compatibility)
    pub start_line: usize,
    /// End line (0-indexed, inclusive)
    pub end_line: usize,
}

impl SourceLocation {
    /// Create a new source location with just a single line (for backward compatibility)
    pub fn new(file: PathBuf, line: usize) -> Self {
        SourceLocation {
            file,
            start_line: line,
            end_line: line,
        }
    }

    /// Create a source location spanning multiple lines
    pub fn span(file: PathBuf, start_line: usize, end_line: usize) -> Self {
        debug_assert!(
            start_line <= end_line,
            "SourceLocation: start_line ({}) must be <= end_line ({})",
            start_line,
            end_line
        );
        SourceLocation {
            file,
            start_line,
            end_line,
        }
    }

    /// Get the line number (for backward compatibility, returns start_line)
    pub fn line(&self) -> usize {
        self.start_line
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start_line == self.end_line {
            write!(f, "{}:{}", self.file.display(), self.start_line + 1)
        } else {
            write!(
                f,
                "{}:{}-{}",
                self.file.display(),
                self.start_line + 1,
                self.end_line + 1
            )
        }
    }
}

/// Include statement
#[derive(Debug, Clone, PartialEq)]
pub enum Include {
    /// Standard library include: `include std:http`
    Std(String),
    /// Relative path include: `include "my-utils"`
    Relative(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub includes: Vec<Include>,
    pub words: Vec<WordDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordDef {
    pub name: String,
    /// Optional stack effect declaration
    /// Example: ( ..a Int -- ..a Bool )
    pub effect: Option<Effect>,
    pub body: Vec<Statement>,
    /// Source location for error reporting (collision detection)
    pub source: Option<SourceLocation>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Integer literal: pushes value onto stack
    IntLiteral(i64),

    /// Floating-point literal: pushes IEEE 754 double onto stack
    FloatLiteral(f64),

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

    /// Quotation: [ ... ]
    ///
    /// A block of deferred code (quotation/lambda)
    /// Quotations are first-class values that can be pushed onto the stack
    /// and executed later with combinators like `call`, `times`, or `while`
    ///
    /// The id field is used by the typechecker to track the inferred type
    /// (Quotation vs Closure) for this quotation. The id is assigned during parsing.
    Quotation { id: usize, body: Vec<Statement> },
}

impl Program {
    pub fn new() -> Self {
        Program {
            includes: Vec::new(),
            words: Vec::new(),
        }
    }

    pub fn find_word(&self, name: &str) -> Option<&WordDef> {
        self.words.iter().find(|w| w.name == name)
    }

    /// Validate that all word calls reference either a defined word or a built-in
    pub fn validate_word_calls(&self) -> Result<(), String> {
        self.validate_word_calls_with_externals(&[])
    }

    /// Validate that all word calls reference a defined word, built-in, or external word.
    ///
    /// The `external_words` parameter should contain names of words available from
    /// external sources (e.g., included modules) that should be considered valid.
    pub fn validate_word_calls_with_externals(
        &self,
        external_words: &[&str],
    ) -> Result<(), String> {
        // List of known runtime built-ins
        // IMPORTANT: Keep this in sync with codegen.rs WordCall matching
        let builtins = [
            // I/O operations
            "write_line",
            "read_line",
            "int->string",
            // Command-line arguments
            "arg-count",
            "arg",
            // File operations
            "file-slurp",
            "file-exists?",
            // String operations
            "string-concat",
            "string-length",
            "string-byte-length",
            "string-char-at",
            "string-substring",
            "char->string",
            "string-find",
            "string-split",
            "string-contains",
            "string-starts-with",
            "string-empty",
            "string-trim",
            "string-to-upper",
            "string-to-lower",
            "string-equal",
            "json-escape",
            // Variant operations
            "variant-field-count",
            "variant-tag",
            "variant-field-at",
            "variant-append",
            "variant-last",
            "variant-init",
            "make-variant",
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
            "pick",
            "roll",
            // Boolean operations
            "and",
            "or",
            "not",
            // Concurrency operations
            "make-channel",
            "send",
            "receive",
            "close-channel",
            "yield",
            // Quotation operations
            "call",
            "times",
            "while",
            "until",
            "forever",
            "spawn",
            "cond",
            // TCP operations
            "tcp-listen",
            "tcp-accept",
            "tcp-read",
            "tcp-write",
            "tcp-close",
            // Float arithmetic operations
            "f.add",
            "f.subtract",
            "f.multiply",
            "f.divide",
            // Float comparison operations
            "f.=",
            "f.<",
            "f.>",
            "f.<=",
            "f.>=",
            "f.<>",
            // Type conversions
            "int->float",
            "float->int",
            "float->string",
            "string->float",
        ];

        for word in &self.words {
            self.validate_statements(&word.body, &word.name, &builtins, external_words)?;
        }

        Ok(())
    }

    /// Helper to validate word calls in a list of statements (recursively)
    fn validate_statements(
        &self,
        statements: &[Statement],
        word_name: &str,
        builtins: &[&str],
        external_words: &[&str],
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
                    // Check if it's an external word (from includes)
                    if external_words.contains(&name.as_str()) {
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
                    self.validate_statements(then_branch, word_name, builtins, external_words)?;
                    if let Some(eb) = else_branch {
                        self.validate_statements(eb, word_name, builtins, external_words)?;
                    }
                }
                Statement::Quotation { body, .. } => {
                    // Recursively validate quotation body
                    self.validate_statements(body, word_name, builtins, external_words)?;
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
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall("add".to_string()),
                    Statement::WordCall("write_line".to_string()),
                ],
                source: None,
            }],
        };

        // Should succeed - add and write_line are built-ins
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_user_defined_words() {
        let program = Program {
            includes: vec![],
            words: vec![
                WordDef {
                    name: "helper".to_string(),
                    effect: None,
                    body: vec![Statement::IntLiteral(42)],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall("helper".to_string())],
                    source: None,
                },
            ],
        };

        // Should succeed - helper is defined
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_undefined_word() {
        let program = Program {
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall("undefined_word".to_string())],
                source: None,
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
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall("wrte_line".to_string())], // typo
                source: None,
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
