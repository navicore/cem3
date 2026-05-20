//! Lint Engine for Seq
//!
//! A clippy-inspired lint tool that detects common patterns and suggests improvements.
//! Phase 1: Syntactic pattern matching on word sequences.
//!
//! # Architecture
//!
//! - `LintConfig` - Parsed lint rules from TOML
//! - `Pattern` - Compiled pattern for matching
//! - `Linter` - Walks AST and finds matches
//! - `LintDiagnostic` - Output format compatible with LSP
//!
//! # Known Limitations (Phase 1)
//!
//! - **No quotation boundary awareness**: Patterns match across statement boundaries
//!   within a word body. Patterns like `[ drop` would incorrectly match `[` followed
//!   by `drop` anywhere, not just at quotation start. Such patterns should be avoided
//!   until Phase 2 adds quotation-aware matching.

mod linter;
mod types;

#[cfg(test)]
mod tests;

pub use linter::Linter;
pub use types::{
    CompiledPattern, DEFAULT_LINTS, KnownLint, LintConfig, LintDiagnostic, LintRule,
    MAX_NESTING_DEPTH, PatternElement, Severity, format_diagnostics, known_lint_ids,
};
