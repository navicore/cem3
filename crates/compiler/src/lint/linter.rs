//! The `Linter` walks the AST, matches compiled patterns against word-call
//! sequences, and emits `LintDiagnostic` entries. Also houses the if/else
//! nesting-depth check.

use std::path::Path;

use crate::ast::{Program, Statement, WordDef};

use super::types::{
    CompiledPattern, LintConfig, LintDiagnostic, MAX_NESTING_DEPTH, PatternElement, Severity,
    WordInfo,
};

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

        self.lint_statement_list(
            &word.body,
            word,
            file,
            fallback_line,
            &mut local_diagnostics,
        );

        // Check for deeply nested if/else chains (top-level word body only)
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
                span: _,
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
            Statement::Match { arms, span: _ } => {
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

    /// Run all patterns against `statements`, then recurse into any
    /// nested forms (quotations, if branches, match arms) within them.
    /// Single source of truth for the "scan + descend" loop used by both
    /// the top-level word body and every nested body.
    fn lint_statement_list(
        &self,
        statements: &[Statement],
        word: &WordDef,
        file: &Path,
        fallback_line: usize,
        diagnostics: &mut Vec<LintDiagnostic>,
    ) {
        let word_infos = self.extract_word_sequence(statements);
        for pattern in &self.patterns {
            self.find_matches(&word_infos, pattern, word, file, fallback_line, diagnostics);
        }
        self.lint_nested(statements, word, file, diagnostics);
    }

    /// Walk `statements`, descending into any nested body (quotation,
    /// if branch, match arm) by handing it back to `lint_statement_list`.
    /// The combined recursion does extract → patterns → recurse on every
    /// reachable body in the AST.
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
                    self.lint_statement_list(body, word, file, fallback_line, diagnostics);
                }
                Statement::If {
                    then_branch,
                    else_branch,
                    span: _,
                } => {
                    self.lint_statement_list(then_branch, word, file, fallback_line, diagnostics);
                    if let Some(else_stmts) = else_branch {
                        self.lint_statement_list(
                            else_stmts,
                            word,
                            file,
                            fallback_line,
                            diagnostics,
                        );
                    }
                }
                Statement::Match { arms, span: _ } => {
                    for arm in arms {
                        self.lint_statement_list(&arm.body, word, file, fallback_line, diagnostics);
                    }
                }
                _ => {}
            }
        }
    }
}
