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

use crate::ast::{Program, Span, Statement, WordDef};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Embedded default lint rules
pub static DEFAULT_LINTS: &str = include_str!("lints.toml");

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

impl Severity {
    /// Convert to LSP DiagnosticSeverity number
    pub fn to_lsp_severity(&self) -> u32 {
        match self {
            Severity::Error => 1,
            Severity::Warning => 2,
            Severity::Hint => 4,
        }
    }
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
struct WordInfo<'a> {
    name: &'a str,
    span: Option<&'a Span>,
}

/// The linter engine
pub struct Linter {
    patterns: Vec<CompiledPattern>,
}

impl Linter {
    /// Create a new linter with the given configuration
    pub fn new(config: &LintConfig) -> Result<Self, String> {
        let mut patterns = Vec::new();
        for rule in &config.rules {
            patterns.push(CompiledPattern::compile(rule.clone())?);
        }
        Ok(Linter { patterns })
    }

    /// Create a linter with default configuration
    pub fn with_defaults() -> Result<Self, String> {
        let config = LintConfig::default_config()?;
        Self::new(&config)
    }

    /// Lint a program and return all diagnostics
    pub fn lint_program(&self, program: &Program, file: &Path) -> Vec<LintDiagnostic> {
        let mut diagnostics = Vec::new();

        for word in &program.words {
            self.lint_word(word, file, &mut diagnostics);
        }

        diagnostics
    }

    /// Lint a single word definition
    fn lint_word(&self, word: &WordDef, file: &Path, diagnostics: &mut Vec<LintDiagnostic>) {
        let fallback_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);

        // Collect diagnostics locally first, then filter by allowed_lints
        let mut local_diagnostics = Vec::new();

        // Extract word sequence from the body (with span info)
        let word_infos = self.extract_word_sequence(&word.body);

        // Try each pattern
        for pattern in &self.patterns {
            self.find_matches(
                &word_infos,
                pattern,
                word,
                file,
                fallback_line,
                &mut local_diagnostics,
            );
        }

        // Check for deeply nested if/else chains
        let max_depth = Self::max_if_nesting_depth(&word.body);
        if max_depth >= MAX_NESTING_DEPTH {
            local_diagnostics.push(LintDiagnostic {
                id: "deep-nesting".to_string(),
                message: format!(
                    "deeply nested if/else ({} levels) - consider using `cond` or extracting to helper words",
                    max_depth
                ),
                severity: Severity::Hint,
                replacement: String::new(),
                file: file.to_path_buf(),
                line: fallback_line,
                end_line: None,
                start_column: None,
                end_column: None,
                word_name: word.name.clone(),
                start_index: 0,
                end_index: 0,
            });
        }

        // Recursively lint nested structures (quotations, if branches)
        self.lint_nested(&word.body, word, file, &mut local_diagnostics);

        // Filter out diagnostics that are allowed via # seq:allow(lint-id) annotation
        for diagnostic in local_diagnostics {
            if !word.allowed_lints.contains(&diagnostic.id) {
                diagnostics.push(diagnostic);
            }
        }
    }

    /// Calculate the maximum if/else nesting depth in a statement list
    fn max_if_nesting_depth(statements: &[Statement]) -> usize {
        let mut max_depth = 0;
        for stmt in statements {
            let depth = Self::if_nesting_depth(stmt, 0);
            if depth > max_depth {
                max_depth = depth;
            }
        }
        max_depth
    }

    /// Calculate if/else nesting depth for a single statement
    fn if_nesting_depth(stmt: &Statement, current_depth: usize) -> usize {
        match stmt {
            Statement::If {
                then_branch,
                else_branch,
            } => {
                // This if adds one level of nesting
                let new_depth = current_depth + 1;

                // Check then branch for further nesting
                let then_max = then_branch
                    .iter()
                    .map(|s| Self::if_nesting_depth(s, new_depth))
                    .max()
                    .unwrap_or(new_depth);

                // Check else branch - nested ifs in else are the classic "else if" chain
                let else_max = else_branch
                    .as_ref()
                    .map(|stmts| {
                        stmts
                            .iter()
                            .map(|s| Self::if_nesting_depth(s, new_depth))
                            .max()
                            .unwrap_or(new_depth)
                    })
                    .unwrap_or(new_depth);

                then_max.max(else_max)
            }
            Statement::Quotation { body, .. } => {
                // Quotations start fresh nesting count (they're separate code blocks)
                body.iter()
                    .map(|s| Self::if_nesting_depth(s, 0))
                    .max()
                    .unwrap_or(0)
            }
            Statement::Match { arms } => {
                // Match arms don't count as if nesting, but check for ifs inside
                arms.iter()
                    .flat_map(|arm| arm.body.iter())
                    .map(|s| Self::if_nesting_depth(s, current_depth))
                    .max()
                    .unwrap_or(current_depth)
            }
            _ => current_depth,
        }
    }

    /// Extract a flat sequence of word names with spans from statements.
    /// Non-WordCall statements (literals, quotations, etc.) are represented as
    /// a special marker `<non-word>` to prevent false pattern matches across
    /// non-consecutive word calls.
    fn extract_word_sequence<'a>(&self, statements: &'a [Statement]) -> Vec<WordInfo<'a>> {
        let mut words = Vec::new();
        for stmt in statements {
            if let Statement::WordCall { name, span } = stmt {
                words.push(WordInfo {
                    name: name.as_str(),
                    span: span.as_ref(),
                });
            } else {
                // Insert a marker for non-word statements to break up patterns.
                // This prevents false positives like matching "swap swap" when
                // there's a literal between them: "swap 0 swap"
                words.push(WordInfo {
                    name: "<non-word>",
                    span: None,
                });
            }
        }
        words
    }

    /// Find all matches of a pattern in a word sequence
    fn find_matches(
        &self,
        word_infos: &[WordInfo],
        pattern: &CompiledPattern,
        word: &WordDef,
        file: &Path,
        fallback_line: usize,
        diagnostics: &mut Vec<LintDiagnostic>,
    ) {
        if word_infos.is_empty() || pattern.elements.is_empty() {
            return;
        }

        // Sliding window match
        let mut i = 0;
        while i < word_infos.len() {
            if let Some(match_len) = Self::try_match_at(word_infos, i, &pattern.elements) {
                // Extract position info from spans if available
                let first_span = word_infos[i].span;
                let last_span = word_infos[i + match_len - 1].span;

                // Use span line if available, otherwise fall back to word definition line
                let line = first_span.map(|s| s.line).unwrap_or(fallback_line);

                // Calculate end line and column range
                let (end_line, start_column, end_column) =
                    if let (Some(first), Some(last)) = (first_span, last_span) {
                        if first.line == last.line {
                            // Same line: column range spans from first word's start to last word's end
                            (None, Some(first.column), Some(last.column + last.length))
                        } else {
                            // Multi-line match: track end line and end column
                            (
                                Some(last.line),
                                Some(first.column),
                                Some(last.column + last.length),
                            )
                        }
                    } else {
                        (None, None, None)
                    };

                diagnostics.push(LintDiagnostic {
                    id: pattern.rule.id.clone(),
                    message: pattern.rule.message.clone(),
                    severity: pattern.rule.severity,
                    replacement: pattern.rule.replacement.clone(),
                    file: file.to_path_buf(),
                    line,
                    end_line,
                    start_column,
                    end_column,
                    word_name: word.name.clone(),
                    start_index: i,
                    end_index: i + match_len,
                });
                // Skip past the match to avoid overlapping matches
                i += match_len;
            } else {
                i += 1;
            }
        }
    }

    /// Try to match pattern at position, returning match length if successful
    fn try_match_at(
        word_infos: &[WordInfo],
        start: usize,
        elements: &[PatternElement],
    ) -> Option<usize> {
        let mut word_idx = start;
        let mut elem_idx = 0;

        while elem_idx < elements.len() {
            match &elements[elem_idx] {
                PatternElement::Word(expected) => {
                    if word_idx >= word_infos.len() || word_infos[word_idx].name != expected {
                        return None;
                    }
                    word_idx += 1;
                    elem_idx += 1;
                }
                PatternElement::SingleWildcard(_) => {
                    if word_idx >= word_infos.len() {
                        return None;
                    }
                    word_idx += 1;
                    elem_idx += 1;
                }
                PatternElement::MultiWildcard => {
                    // Multi-wildcard: try all possible lengths
                    elem_idx += 1;
                    if elem_idx >= elements.len() {
                        // Wildcard at end matches rest
                        return Some(word_infos.len() - start);
                    }
                    // Try matching remaining pattern at each position
                    for try_idx in word_idx..=word_infos.len() {
                        if let Some(rest_len) =
                            Self::try_match_at(word_infos, try_idx, &elements[elem_idx..])
                        {
                            return Some(try_idx - start + rest_len);
                        }
                    }
                    return None;
                }
            }
        }

        Some(word_idx - start)
    }

    /// Recursively lint nested structures
    fn lint_nested(
        &self,
        statements: &[Statement],
        word: &WordDef,
        file: &Path,
        diagnostics: &mut Vec<LintDiagnostic>,
    ) {
        let fallback_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);

        for stmt in statements {
            match stmt {
                Statement::Quotation { body, .. } => {
                    // Lint the quotation body
                    let word_infos = self.extract_word_sequence(body);
                    for pattern in &self.patterns {
                        self.find_matches(
                            &word_infos,
                            pattern,
                            word,
                            file,
                            fallback_line,
                            diagnostics,
                        );
                    }
                    // Recurse into nested quotations
                    self.lint_nested(body, word, file, diagnostics);
                }
                Statement::If {
                    then_branch,
                    else_branch,
                } => {
                    // Lint both branches
                    let word_infos = self.extract_word_sequence(then_branch);
                    for pattern in &self.patterns {
                        self.find_matches(
                            &word_infos,
                            pattern,
                            word,
                            file,
                            fallback_line,
                            diagnostics,
                        );
                    }
                    self.lint_nested(then_branch, word, file, diagnostics);

                    if let Some(else_stmts) = else_branch {
                        let word_infos = self.extract_word_sequence(else_stmts);
                        for pattern in &self.patterns {
                            self.find_matches(
                                &word_infos,
                                pattern,
                                word,
                                file,
                                fallback_line,
                                diagnostics,
                            );
                        }
                        self.lint_nested(else_stmts, word, file, diagnostics);
                    }
                }
                Statement::Match { arms } => {
                    for arm in arms {
                        let word_infos = self.extract_word_sequence(&arm.body);
                        for pattern in &self.patterns {
                            self.find_matches(
                                &word_infos,
                                pattern,
                                word,
                                file,
                                fallback_line,
                                diagnostics,
                            );
                        }
                        self.lint_nested(&arm.body, word, file, diagnostics);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Format diagnostics for CLI output
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> LintConfig {
        LintConfig::from_toml(
            r#"
[[lint]]
id = "redundant-dup-drop"
pattern = "dup drop"
replacement = ""
message = "`dup drop` has no effect"
severity = "warning"

[[lint]]
id = "prefer-nip"
pattern = "swap drop"
replacement = "nip"
message = "prefer `nip` over `swap drop`"
severity = "hint"

[[lint]]
id = "redundant-swap-swap"
pattern = "swap swap"
replacement = ""
message = "consecutive swaps cancel out"
severity = "warning"
"#,
        )
        .unwrap()
    }

    #[test]
    fn test_parse_config() {
        let config = test_config();
        assert_eq!(config.rules.len(), 3);
        assert_eq!(config.rules[0].id, "redundant-dup-drop");
        assert_eq!(config.rules[1].severity, Severity::Hint);
    }

    #[test]
    fn test_compile_pattern() {
        let rule = LintRule {
            id: "test".to_string(),
            pattern: "swap drop".to_string(),
            replacement: "nip".to_string(),
            message: "test".to_string(),
            severity: Severity::Warning,
        };
        let compiled = CompiledPattern::compile(rule).unwrap();
        assert_eq!(compiled.elements.len(), 2);
        assert_eq!(
            compiled.elements[0],
            PatternElement::Word("swap".to_string())
        );
        assert_eq!(
            compiled.elements[1],
            PatternElement::Word("drop".to_string())
        );
    }

    #[test]
    fn test_compile_pattern_with_wildcards() {
        let rule = LintRule {
            id: "test".to_string(),
            pattern: "dup $X drop".to_string(),
            replacement: "".to_string(),
            message: "test".to_string(),
            severity: Severity::Warning,
        };
        let compiled = CompiledPattern::compile(rule).unwrap();
        assert_eq!(compiled.elements.len(), 3);
        assert_eq!(
            compiled.elements[1],
            PatternElement::SingleWildcard("$X".to_string())
        );
    }

    #[test]
    fn test_simple_match() {
        let config = test_config();
        let linter = Linter::new(&config).unwrap();

        // Create a simple program with "swap drop"
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(1),
                    Statement::IntLiteral(2),
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let diagnostics = linter.lint_program(&program, &PathBuf::from("test.seq"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id, "prefer-nip");
        assert_eq!(diagnostics[0].replacement, "nip");
    }

    #[test]
    fn test_no_false_positives() {
        let config = test_config();
        let linter = Linter::new(&config).unwrap();

        // "swap" followed by something other than "drop"
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let diagnostics = linter.lint_program(&program, &PathBuf::from("test.seq"));
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_multiple_matches() {
        let config = test_config();
        let linter = Linter::new(&config).unwrap();

        // Two instances of "swap drop"
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let diagnostics = linter.lint_program(&program, &PathBuf::from("test.seq"));
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn test_multi_wildcard_validation() {
        // Pattern with two multi-wildcards should be rejected
        let rule = LintRule {
            id: "bad-pattern".to_string(),
            pattern: "$... foo $...".to_string(),
            replacement: "".to_string(),
            message: "test".to_string(),
            severity: Severity::Warning,
        };
        let result = CompiledPattern::compile(rule);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("multi-wildcards"));
    }

    #[test]
    fn test_single_multi_wildcard_allowed() {
        // Pattern with one multi-wildcard should be accepted
        let rule = LintRule {
            id: "ok-pattern".to_string(),
            pattern: "$... foo".to_string(),
            replacement: "".to_string(),
            message: "test".to_string(),
            severity: Severity::Warning,
        };
        let result = CompiledPattern::compile(rule);
        assert!(result.is_ok());
    }

    #[test]
    fn test_literal_breaks_pattern() {
        // "swap 0 swap" should NOT match "swap swap" because the literal breaks the pattern
        let config = test_config();
        let linter = Linter::new(&config).unwrap();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(0), // This should break the pattern
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let diagnostics = linter.lint_program(&program, &PathBuf::from("test.seq"));
        // Should NOT find "swap swap" because there's a literal in between
        assert!(
            diagnostics.is_empty(),
            "Expected no matches, but got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_consecutive_swap_swap_still_matches() {
        // Actual consecutive "swap swap" should still be detected
        let config = test_config();
        let linter = Linter::new(&config).unwrap();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "test".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "swap".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        let diagnostics = linter.lint_program(&program, &PathBuf::from("test.seq"));
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].id, "redundant-swap-swap");
    }
}
