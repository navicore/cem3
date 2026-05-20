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

use crate::ast::Span;
use serde::Deserialize;
use std::path::PathBuf;

/// Embedded default lint rules
pub static DEFAULT_LINTS: &str = include_str!("../lints.toml");

/// Maximum if/else nesting depth before warning (structural lint)
/// 4 levels deep is the threshold - beyond this, consider `cond` or helper words
pub const MAX_NESTING_DEPTH: usize = 4;

/// Severity level for lint diagnostics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Hint,
}

/// A single lint rule from configuration
#[derive(Debug, Clone, Deserialize)]
pub struct LintRule {
    /// Unique identifier for the lint
    pub id: String,
    /// Pattern to match (space-separated words, $X for wildcards)
    pub pattern: String,
    /// Suggested replacement (empty string means "remove")
    #[serde(default)]
    pub replacement: String,
    /// Human-readable message
    pub message: String,
    /// Severity level
    #[serde(default = "default_severity")]
    pub severity: Severity,
}

fn default_severity() -> Severity {
    Severity::Warning
}

/// Lint configuration containing all rules
#[derive(Debug, Clone, Deserialize)]
pub struct LintConfig {
    #[serde(rename = "lint")]
    pub rules: Vec<LintRule>,
}

impl LintConfig {
    /// Parse lint configuration from TOML string
    pub fn from_toml(toml_str: &str) -> Result<Self, String> {
        toml::from_str(toml_str).map_err(|e| format!("Failed to parse lint config: {}", e))
    }

    /// Load default embedded lint configuration
    pub fn default_config() -> Result<Self, String> {
        Self::from_toml(DEFAULT_LINTS)
    }

    /// Merge another config into this one (user overrides)
    pub fn merge(&mut self, other: LintConfig) {
        // User rules override defaults with same id
        for rule in other.rules {
            if let Some(existing) = self.rules.iter_mut().find(|r| r.id == rule.id) {
                *existing = rule;
            } else {
                self.rules.push(rule);
            }
        }
    }
}

/// One lint ID known to the compiler, with metadata for editor surfaces.
///
/// Returned from [`known_lint_ids`] as the single source of truth for
/// what `# seq:allow(<id>)` accepts. Adding a new lint — whether to
/// `lints.toml` or as a hard-coded analyzer that honors `allowed_lints` —
/// must flow through this function.
#[derive(Debug, Clone)]
pub struct KnownLint {
    pub id: String,
    pub message: String,
    pub severity: Severity,
}

/// Every lint ID that `# seq:allow(<id>)` actually suppresses.
///
/// Suppression points today:
/// - `Linter::lint_word` filters all TOML-pattern diagnostics plus
///   `deep-nesting` by `word.allowed_lints` (`lint/linter.rs`).
/// - `ErrorFlagAnalyzer::analyze_program` skips words allowing
///   `unchecked-error-flag` (`error_flag_lint/analyzer.rs`).
///
/// Lint IDs that exist but ignore `allowed_lints` (`unreachable-chan-yield`,
/// `resource-leak-*`, `resource-branch-inconsistent`) are intentionally
/// omitted — listing them would let users write annotations that
/// silently do nothing.
pub fn known_lint_ids() -> Vec<KnownLint> {
    let mut out = Vec::new();

    if let Ok(config) = LintConfig::default_config() {
        for rule in config.rules {
            out.push(KnownLint {
                id: rule.id,
                message: rule.message,
                severity: rule.severity,
            });
        }
    }

    out.push(KnownLint {
        id: "deep-nesting".to_string(),
        message: format!(
            "deeply nested if/else ({}+ levels) - consider `cond` or extracting to helper words",
            MAX_NESTING_DEPTH
        ),
        severity: Severity::Hint,
    });

    out.push(KnownLint {
        id: "unchecked-error-flag".to_string(),
        message: "operation returns a Bool success flag that is dropped without being checked"
            .to_string(),
        severity: Severity::Warning,
    });

    out
}

/// A compiled pattern for efficient matching
#[derive(Debug, Clone)]
pub struct CompiledPattern {
    /// The original rule
    pub rule: LintRule,
    /// Pattern elements (words or wildcards)
    pub elements: Vec<PatternElement>,
}

/// Element in a compiled pattern
#[derive(Debug, Clone, PartialEq)]
pub enum PatternElement {
    /// Exact word match
    Word(String),
    /// Single-word wildcard ($X, $Y, etc.)
    SingleWildcard(String),
    /// Multi-word wildcard ($...)
    MultiWildcard,
}

impl CompiledPattern {
    /// Compile a pattern string into elements
    pub fn compile(rule: LintRule) -> Result<Self, String> {
        let mut elements = Vec::new();
        let mut multi_wildcard_count = 0;

        for token in rule.pattern.split_whitespace() {
            if token == "$..." {
                multi_wildcard_count += 1;
                elements.push(PatternElement::MultiWildcard);
            } else if token.starts_with('$') {
                elements.push(PatternElement::SingleWildcard(token.to_string()));
            } else {
                elements.push(PatternElement::Word(token.to_string()));
            }
        }

        if elements.is_empty() {
            return Err(format!("Empty pattern in lint rule '{}'", rule.id));
        }

        // Validate: at most one multi-wildcard per pattern to avoid
        // exponential backtracking complexity
        if multi_wildcard_count > 1 {
            return Err(format!(
                "Pattern in lint rule '{}' has {} multi-wildcards ($...), but at most 1 is allowed",
                rule.id, multi_wildcard_count
            ));
        }

        Ok(CompiledPattern { rule, elements })
    }
}

/// A lint diagnostic (match found)
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    /// Lint rule ID
    pub id: String,
    /// Human-readable message
    pub message: String,
    /// Severity level
    pub severity: Severity,
    /// Suggested replacement
    pub replacement: String,
    /// File where the match was found
    pub file: PathBuf,
    /// Start line number (0-indexed)
    pub line: usize,
    /// End line number (0-indexed), for multi-line matches
    pub end_line: Option<usize>,
    /// Start column (0-indexed), if available from source spans
    pub start_column: Option<usize>,
    /// End column (0-indexed, exclusive), if available from source spans
    pub end_column: Option<usize>,
    /// Word name where the match was found
    pub word_name: String,
    /// Start index in the word body
    pub start_index: usize,
    /// End index in the word body (exclusive)
    pub end_index: usize,
}

/// Word call info extracted from a statement, including optional span
#[derive(Debug, Clone)]
pub(super) struct WordInfo<'a> {
    pub(super) name: &'a str,
    pub(super) span: Option<&'a Span>,
}

pub fn format_diagnostics(diagnostics: &[LintDiagnostic]) -> String {
    let mut output = String::new();
    for d in diagnostics {
        let severity_str = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Hint => "hint",
        };
        // Include column info in output if available
        let location = match d.start_column {
            Some(col) => format!("{}:{}:{}", d.file.display(), d.line + 1, col + 1),
            None => format!("{}:{}", d.file.display(), d.line + 1),
        };
        output.push_str(&format!(
            "{}: {} [{}]: {}\n",
            location, severity_str, d.id, d.message
        ));
        if !d.replacement.is_empty() {
            output.push_str(&format!("  suggestion: replace with `{}`\n", d.replacement));
        } else if d.replacement.is_empty() && d.message.contains("no effect") {
            output.push_str("  suggestion: remove this code\n");
        }
    }
    output
}
