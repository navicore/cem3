//! Code generation error types.

/// Error type for code generation operations.
///
/// This allows proper error propagation using `?` for both logical errors
/// (invalid programs, missing definitions) and formatting errors (write failures).
#[derive(Debug)]
pub enum CodeGenError {
    /// A logical error in code generation (e.g., missing word definition)
    Logic(String),
    /// A formatting error when writing IR
    Format(std::fmt::Error),
}

impl std::fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeGenError::Logic(s) => write!(f, "{}", s),
            CodeGenError::Format(e) => write!(f, "IR generation error: {}", e),
        }
    }
}

impl std::error::Error for CodeGenError {}

impl From<String> for CodeGenError {
    fn from(s: String) -> Self {
        CodeGenError::Logic(s)
    }
}

impl From<std::fmt::Error> for CodeGenError {
    fn from(e: std::fmt::Error) -> Self {
        CodeGenError::Format(e)
    }
}
