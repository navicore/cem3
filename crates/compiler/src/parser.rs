//! Simple parser for Seq syntax
//!
//! Syntax:
//! ```text
//! : word-name ( stack-effect )
//!   statement1
//!   statement2
//!   ... ;
//! ```

mod cursor;
mod driver;
mod items;
mod statements;
mod token;
pub(crate) mod type_parse;

#[cfg(test)]
mod tests;

pub use token::Token;

// Private helpers brought into the parser module's scope so that sibling
// sub-modules can reference them as `super::<name>`.
use token::{annotate_error_with_line, is_float_literal, tokenize, unescape_string};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Counter for assigning unique IDs to quotations
    /// Used by the typechecker to track inferred types
    next_quotation_id: usize,
    /// Pending lint annotations collected from `# seq:allow(lint-id)` comments
    pending_allowed_lints: Vec<String>,
    /// Known union type names - used to distinguish union types from type variables
    /// RFC #345: Union types in stack effects must be recognized as concrete types
    known_unions: std::collections::HashSet<String>,
}
