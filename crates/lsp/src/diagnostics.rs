use crate::includes::IncludedWord;
use seqc::{Parser, TypeChecker, lint};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, DiagnosticSeverity, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};
use tracing::{debug, warn};

/// Check a document for parse and type errors, returning LSP diagnostics.
///
/// This version doesn't know about included words - use `check_document_with_includes`
/// for include-aware diagnostics.
#[cfg(test)]
pub fn check_document(source: &str) -> Vec<Diagnostic> {
    check_document_with_includes(source, &[], None)
}

/// Check a document for parse and type errors, with knowledge of included words.
///
/// The `included_words` parameter should contain all words available from
/// included modules, with their effects if known.
///
/// The `file_path` parameter is used for lint diagnostics to identify the source file.
pub fn check_document_with_includes(
    source: &str,
    included_words: &[IncludedWord],
    file_path: Option<&Path>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Phase 1: Parse
    let mut parser = Parser::new(source);
    let mut program = match parser.parse() {
        Ok(prog) => prog,
        Err(err) => {
            debug!("Parse error: {}", err);
            diagnostics.push(error_to_diagnostic(&err, source));
            return diagnostics;
        }
    };

    // Phase 1.5: Generate ADT constructors (Make-VariantName words)
    if let Err(err) = program.generate_constructors() {
        debug!("Constructor generation error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, source));
        return diagnostics;
    }

    // Extract names for word call validation
    let included_word_names: Vec<&str> = included_words.iter().map(|w| w.name.as_str()).collect();

    // Phase 2: Validate word calls (check for undefined words)
    // This catches references to words that don't exist as either
    // user-defined words or builtins.
    // We pass included word names so they aren't flagged as undefined.
    if let Err(err) = program.validate_word_calls_with_externals(&included_word_names) {
        debug!("Validation error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, source));
        // Continue to type checking - may find additional errors
    }

    // Phase 3: Type check
    // Register external words with the typechecker so it knows about included words
    let mut typechecker = TypeChecker::new();

    // Build list of external words with their effects (or None for placeholder)
    // TODO: When effect is None, a maximally polymorphic placeholder (..a -- ..b) is used.
    // This may allow type-incorrect code to pass the typechecker. Consider:
    // - Emitting a warning when a word has no effect signature
    // - Requiring all exported words to have effects
    // - Tracking which words used placeholders and showing them in diagnostics
    let external_words: Vec<(&str, Option<&seqc::Effect>)> = included_words
        .iter()
        .map(|w| (w.name.as_str(), w.effect.as_ref()))
        .collect();
    typechecker.register_external_words(&external_words);

    if let Err(err) = typechecker.check_program(&program) {
        debug!("Type error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, source));
    }

    // Phase 4: Lint checks
    // Run lint checks and add any warnings/hints
    let lint_file_path = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("source.seq"));
    match lint::Linter::with_defaults() {
        Ok(linter) => {
            let lint_diagnostics = linter.lint_program(&program, &lint_file_path);
            for lint_diag in lint_diagnostics {
                diagnostics.push(lint_to_diagnostic(&lint_diag, source));
            }
        }
        Err(e) => {
            warn!("Failed to create linter: {}", e);
        }
    }

    diagnostics
}

/// Get code actions for lint diagnostics that overlap with the given range.
///
/// This re-runs the linter to find applicable fixes for the requested range.
pub fn get_code_actions(
    source: &str,
    range: Range,
    uri: &Url,
    file_path: Option<&Path>,
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // Parse the source
    let mut parser = Parser::new(source);
    let program = match parser.parse() {
        Ok(prog) => prog,
        Err(_) => return actions, // No actions if parse fails
    };

    // Run linter
    let lint_file_path = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("source.seq"));

    let linter = match lint::Linter::with_defaults() {
        Ok(l) => l,
        Err(_) => return actions,
    };

    let lint_diagnostics = linter.lint_program(&program, &lint_file_path);

    // Find lint diagnostics that overlap with the requested range
    for lint_diag in &lint_diagnostics {
        let diag_range = make_lint_range(lint_diag, source);

        // Check if ranges overlap
        if ranges_overlap(&diag_range, &range) {
            // Only create actions for diagnostics that have a fix
            if let Some(action) = lint_to_code_action(lint_diag, source, uri, &diag_range) {
                actions.push(action);
            }
        }
    }

    actions
}

/// Check if two ranges overlap (or if a point is inside a range)
fn ranges_overlap(a: &Range, b: &Range) -> bool {
    // Special case: if b is a zero-width cursor position, check if it's inside a
    if b.start == b.end {
        let cursor_line = b.start.line;
        let cursor_char = b.start.character;

        // Cursor is inside range a if:
        // - cursor line is within a's line range
        // - if on start line, cursor char >= start char
        // - if on end line, cursor char <= end char
        if cursor_line < a.start.line || cursor_line > a.end.line {
            return false;
        }
        if cursor_line == a.start.line && cursor_char < a.start.character {
            return false;
        }
        if cursor_line == a.end.line && cursor_char > a.end.character {
            return false;
        }
        return true;
    }

    // General case: ranges overlap if neither is entirely before the other
    !(a.end.line < b.start.line
        || (a.end.line == b.start.line && a.end.character <= b.start.character)
        || b.end.line < a.start.line
        || (b.end.line == a.start.line && b.end.character <= a.start.character))
}

/// Create an LSP Range from a lint diagnostic
fn make_lint_range(lint_diag: &lint::LintDiagnostic, source: &str) -> Range {
    let line = lint_diag.line as u32;

    let (start_char, end_char) = match (lint_diag.start_column, lint_diag.end_column) {
        (Some(start), Some(end)) => (start as u32, end as u32),
        (Some(start), None) => {
            let line_length = source
                .lines()
                .nth(lint_diag.line)
                .map(|l| l.len() as u32)
                .unwrap_or(0);
            (start as u32, line_length)
        }
        _ => {
            let line_length = source
                .lines()
                .nth(lint_diag.line)
                .map(|l| l.len() as u32)
                .unwrap_or(0);
            (0, line_length)
        }
    };

    Range {
        start: Position {
            line,
            character: start_char,
        },
        end: Position {
            line,
            character: end_char,
        },
    }
}

/// Convert a lint diagnostic to a CodeAction if it has a fix
fn lint_to_code_action(
    lint_diag: &lint::LintDiagnostic,
    _source: &str,
    uri: &Url,
    range: &Range,
) -> Option<CodeAction> {
    // Create the title based on whether there's a replacement or removal
    let title = if lint_diag.replacement.is_empty() {
        format!("Remove redundant code ({})", lint_diag.id)
    } else {
        format!("Replace with `{}`", lint_diag.replacement)
    };

    // Create the text edit
    let new_text = lint_diag.replacement.clone();

    let edit = TextEdit {
        range: *range,
        new_text,
    };

    // Create workspace edit
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };

    Some(CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
}

/// Convert a lint diagnostic to an LSP diagnostic.
fn lint_to_diagnostic(lint_diag: &lint::LintDiagnostic, source: &str) -> Diagnostic {
    let line = lint_diag.line as u32;

    // Debug: log what we're receiving from the linter
    tracing::debug!(
        "lint_to_diagnostic: id={} line={} start_col={:?} end_col={:?}",
        lint_diag.id,
        lint_diag.line,
        lint_diag.start_column,
        lint_diag.end_column
    );

    let severity = match lint_diag.severity {
        lint::Severity::Error => DiagnosticSeverity::ERROR,
        lint::Severity::Warning => DiagnosticSeverity::WARNING,
        lint::Severity::Hint => DiagnosticSeverity::HINT,
    };

    let message = if lint_diag.replacement.is_empty() {
        lint_diag.message.clone()
    } else {
        format!(
            "{} (use `{}` instead)",
            lint_diag.message, lint_diag.replacement
        )
    };

    // Use precise column info if available, otherwise fall back to whole line
    let (start_char, end_char) = match (lint_diag.start_column, lint_diag.end_column) {
        (Some(start), Some(end)) => (start as u32, end as u32),
        (Some(start), None) => {
            // Have start but no end - highlight to end of line
            let line_length = source
                .lines()
                .nth(lint_diag.line)
                .map(|l| l.len() as u32)
                .unwrap_or(0);
            (start as u32, line_length)
        }
        _ => {
            // No column info - highlight whole line
            let line_length = source
                .lines()
                .nth(lint_diag.line)
                .map(|l| l.len() as u32)
                .unwrap_or(0);
            (0, line_length)
        }
    };

    Diagnostic {
        range: Range {
            start: Position {
                line,
                character: start_char,
            },
            end: Position {
                line,
                character: end_char,
            },
        },
        severity: Some(severity),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            lint_diag.id.clone(),
        )),
        code_description: None,
        source: Some("seq-lint".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Convert a compiler error string to an LSP diagnostic.
///
/// The compiler currently returns errors as strings without structured position
/// information. We attempt to extract line numbers from the error message,
/// falling back to line 0 if not found.
fn error_to_diagnostic(error: &str, source: &str) -> Diagnostic {
    let (line, message) = extract_line_info(error, source);

    // Calculate actual line length for proper highlighting
    let line_length = source
        .lines()
        .nth(line)
        .map(|l| l.len() as u32)
        .unwrap_or(0);

    Diagnostic {
        range: Range {
            start: Position {
                line: line as u32,
                character: 0,
            },
            end: Position {
                line: line as u32,
                character: line_length,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("seq".to_string()),
        message: message.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Try to extract line number information from an error message.
///
/// Current compiler error formats we try to handle:
/// - "at line N: ..."
/// - "line N: ..."
/// - "Unknown word: 'foo'" (search for 'foo' in source)
/// - "Undefined word 'foo' called in word 'bar'" (search for 'foo' in source)
///
/// Returns (line_number, cleaned_message)
fn extract_line_info<'a>(error: &'a str, source: &str) -> (usize, &'a str) {
    // Try "at line N" pattern
    if let Some(idx) = error.find("at line ") {
        let after = &error[idx + 8..];
        if let Some(end) = after.find(|c: char| !c.is_ascii_digit())
            && let Ok(line) = after[..end].parse::<usize>()
        {
            return (line.saturating_sub(1), error); // LSP uses 0-based lines
        }
    }

    // Try "line N:" pattern
    if let Some(idx) = error.find("line ") {
        let after = &error[idx + 5..];
        if let Some(end) = after.find(|c: char| !c.is_ascii_digit())
            && let Ok(line) = after[..end].parse::<usize>()
        {
            return (line.saturating_sub(1), error);
        }
    }

    // Try to find unknown word in source (old format)
    if let Some(rest) = error.strip_prefix("Unknown word: '")
        && let Some(end) = rest.find('\'')
        && let Some(line) = find_word_line(source, &rest[..end])
    {
        return (line, error);
    }

    // Try to find undefined word in source (new format from validate_word_calls)
    // Format: "Undefined word 'foo' called in word 'bar'"
    if let Some(rest) = error.strip_prefix("Undefined word '")
        && let Some(end) = rest.find('\'')
        && let Some(line) = find_word_line(source, &rest[..end])
    {
        return (line, error);
    }

    // Fallback: report on line 0
    warn!("Could not extract line info from error: {}", error);
    (0, error)
}

/// Find the line number where a word appears in the source.
///
/// Seq words can contain special characters like `-`, `>`, `?`, etc.
/// We need to match whole words accounting for these characters.
fn find_word_line(source: &str, word: &str) -> Option<usize> {
    for (line_num, line) in source.lines().enumerate() {
        if !line.contains(word) {
            continue;
        }

        // Skip comment lines
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }

        // Check each potential word position
        // Seq words are separated by whitespace, so we can use that
        for token in trimmed.split_whitespace() {
            if token == word {
                return Some(line_num);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error() {
        let source = ": foo 1 2 +";
        let diagnostics = check_document(source);
        // Should error on missing semicolon
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_type_error() {
        let source = ": foo ( -- Int ) \"hello\" ;";
        let diagnostics = check_document(source);
        // Should error on stack effect mismatch
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_undefined_word() {
        let source = ": main ( -- Int ) undefined-word 0 ;";
        let diagnostics = check_document(source);
        // Should error on undefined word
        assert!(!diagnostics.is_empty(), "Expected diagnostics but got none");
        assert!(
            diagnostics[0].message.contains("Undefined word"),
            "Expected 'Undefined word' in message, got: {}",
            diagnostics[0].message
        );
    }

    #[test]
    fn test_valid_program() {
        let source = ": main ( -- Int ) 0 ;";
        let diagnostics = check_document(source);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_find_word_with_special_chars() {
        let source = "string->float\nfile-exists?\nsome-word";
        assert_eq!(find_word_line(source, "string->float"), Some(0));
        assert_eq!(find_word_line(source, "file-exists?"), Some(1));
        assert_eq!(find_word_line(source, "some-word"), Some(2));
    }

    #[test]
    fn test_find_word_skips_comments() {
        let source = "# string->float comment\nstring->float";
        assert_eq!(find_word_line(source, "string->float"), Some(1));
    }

    #[test]
    fn test_builtin_make_variant_recognized() {
        // make-variant-2 should be recognized as a builtin, not flagged as unknown
        let source = ": main ( -- ) 1 2 3 make-variant-2 drop ;";
        let diagnostics = check_document(source);
        // Should have no "Undefined word" errors for make-variant-2
        for d in &diagnostics {
            assert!(
                !d.message.contains("make-variant-2"),
                "make-variant-2 should be recognized as builtin, got: {}",
                d.message
            );
        }
    }

    #[test]
    fn test_adt_constructor_recognized() {
        // Make-Circle should be generated from the union definition
        let source = r#"
union Shape { Circle { radius: Int } Rectangle { width: Int, height: Int } }

: main ( -- Int )
  5 Make-Circle
  drop
  0
;
"#;
        let diagnostics = check_document(source);
        // Should have no errors - Make-Circle is a valid constructor
        for d in &diagnostics {
            assert!(
                !d.message.contains("Make-Circle"),
                "Make-Circle should be recognized as ADT constructor, got: {}",
                d.message
            );
        }
        assert!(
            diagnostics.is_empty(),
            "Expected no diagnostics, got: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_lint_swap_drop() {
        // swap drop should trigger a lint hint suggesting nip
        let source = ": main ( -- Int ) 1 2 swap drop ;";
        let diagnostics = check_document(source);
        // Should have a lint hint for prefer-nip
        let lint_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.source.as_deref() == Some("seq-lint"))
            .collect();
        assert!(
            !lint_diags.is_empty(),
            "Expected lint diagnostic for swap drop"
        );
        assert!(
            lint_diags[0].message.contains("nip"),
            "Expected nip suggestion, got: {}",
            lint_diags[0].message
        );
        assert_eq!(lint_diags[0].severity, Some(DiagnosticSeverity::HINT));
    }

    #[test]
    fn test_lint_redundant_swap_swap() {
        // swap swap should trigger a lint warning
        let source = ": main ( -- Int ) 1 2 swap swap drop ;";
        let diagnostics = check_document(source);
        // Should have a lint warning for redundant-swap-swap
        let lint_diags: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.source.as_deref() == Some("seq-lint"))
            .collect();
        assert!(
            lint_diags.iter().any(|d| d.message.contains("cancel out")),
            "Expected swap swap warning, got: {:?}",
            lint_diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}
