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
}

impl Program {
    pub fn new() -> Self {
        Program { words: Vec::new() }
    }

    pub fn find_word(&self, name: &str) -> Option<&WordDef> {
        self.words.iter().find(|w| w.name == name)
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}
