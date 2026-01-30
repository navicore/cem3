use crate::includes::IncludeResolution;
use seqc::ast::{Program, QuotationSpan, Statement};
use seqc::types::Type;
use seqc::{Parser, TypeChecker, lint};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, DiagnosticSeverity, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};
use tracing::{debug, warn};

/// Strip shebang line from source if present.
///
/// Replaces the first line with a comment if it starts with `#!`
/// so that line numbers in error messages remain correct.
fn strip_shebang(source: &str) -> Cow<'_, str> {
    if source.starts_with("#!") {
        // Replace shebang with comment of same length to preserve line numbers
        if let Some(newline_pos) = source.find('\n') {
            let mut result = String::with_capacity(source.len());
            result.push('#');
            result.push_str(&" ".repeat(newline_pos - 1));
            result.push_str(&source[newline_pos..]);
            Cow::Owned(result)
        } else {
            // Single line file with just shebang
            Cow::Borrowed("#")
        }
    } else {
        Cow::Borrowed(source)
    }
}

/// Information about a quotation for LSP hover support
#[derive(Debug, Clone)]
pub struct QuotationInfo {
    /// The quotation's source span
    pub span: QuotationSpan,
    /// The inferred type (Quotation or Closure with effect)
    pub inferred_type: Type,
}

/// Collect all quotation spans from a program
fn collect_quotation_spans(program: &Program) -> HashMap<usize, QuotationSpan> {
    let mut spans = HashMap::new();
    for word in &program.words {
        collect_quotations_from_statements(&word.body, &mut spans);
    }
    spans
}

fn collect_quotations_from_statements(
    stmts: &[Statement],
    spans: &mut HashMap<usize, QuotationSpan>,
) {
    for stmt in stmts {
        match stmt {
            Statement::Quotation { id, body, span } => {
                if let Some(s) = span {
                    spans.insert(*id, s.clone());
                }
                collect_quotations_from_statements(body, spans);
            }
            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => {
                collect_quotations_from_statements(then_branch, spans);
                if let Some(else_stmts) = else_branch {
                    collect_quotations_from_statements(else_stmts, spans);
                }
            }
            Statement::Match { arms, span: _ } => {
                for arm in arms {
                    collect_quotations_from_statements(&arm.body, spans);
                }
            }
            _ => {}
        }
    }
}

/// Check a document for parse and type errors, returning LSP diagnostics.
///
/// This version doesn't know about included words - use `check_document_with_quotations`
/// for include-aware diagnostics.
#[cfg(test)]
pub fn check_document(source: &str) -> Vec<Diagnostic> {
    let (diagnostics, _quotations) =
        check_document_with_quotations(source, &IncludeResolution::default(), None);
    diagnostics
}

/// Check a document and return both diagnostics and quotation info for hover support.
///
/// Parses the document, type-checks it, and collects quotation
/// spans and their inferred types for LSP hover functionality.
pub fn check_document_with_quotations(
    source: &str,
    includes: &IncludeResolution,
    file_path: Option<&Path>,
) -> (Vec<Diagnostic>, Vec<QuotationInfo>) {
    let mut diagnostics = Vec::new();
    let mut quotation_info = Vec::new();

    // Strip shebang if present (for script mode files)
    let source = strip_shebang(source);

    // Phase 1: Parse
    let mut parser = Parser::new(&source);
    let mut program = match parser.parse() {
        Ok(prog) => prog,
        Err(err) => {
            debug!("Parse error: {}", err);
            diagnostics.push(error_to_diagnostic(&err, &source));
            return (diagnostics, quotation_info);
        }
    };

    // Phase 1.5: Generate ADT constructors
    if let Err(err) = program.generate_constructors() {
        debug!("Constructor generation error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, &source));
        return (diagnostics, quotation_info);
    }

    // Collect quotation spans before type checking
    let quotation_spans = collect_quotation_spans(&program);

    // Phase 2: Validate word calls
    let included_word_names: Vec<&str> = includes.words.iter().map(|w| w.name.as_str()).collect();
    if let Err(err) = program.validate_word_calls_with_externals(&included_word_names) {
        debug!("Validation error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, &source));
    }

    // Phase 3: Type check
    let mut typechecker = TypeChecker::new();
    let external_unions: Vec<&str> = includes.union_names.iter().map(|s| s.as_str()).collect();
    typechecker.register_external_unions(&external_unions);
    // Filter to words with effects (v2.0 requirement)
    let external_words: Vec<(&str, &seqc::Effect)> = includes
        .words
        .iter()
        .filter_map(|w| w.effect.as_ref().map(|e| (w.name.as_str(), e)))
        .collect();
    typechecker.register_external_words(&external_words);

    if let Err(err) = typechecker.check_program(&program) {
        debug!("Type error: {}", err);
        diagnostics.push(error_to_diagnostic(&err, &source));
    }

    // Get quotation types and combine with spans
    let quotation_types = typechecker.take_quotation_types();
    for (id, span) in quotation_spans {
        if let Some(typ) = quotation_types.get(&id) {
            quotation_info.push(QuotationInfo {
                span,
                inferred_type: typ.clone(),
            });
        }
    }

    // Phase 4: Lint checks
    let lint_file_path = file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("source.seq"));
    if let Ok(linter) = lint::Linter::with_defaults() {
        let lint_diagnostics = linter.lint_program(&program, &lint_file_path);
        for lint_diag in lint_diagnostics {
            diagnostics.push(lint_to_diagnostic(&lint_diag, &source));
        }
    }

    (diagnostics, quotation_info)
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

    // Strip shebang if present (for script mode files)
    let source = strip_shebang(source);

    // Parse the source
    let mut parser = Parser::new(&source);
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
        let diag_range = make_lint_range(lint_diag, &source);

        // Check if ranges overlap
        if ranges_overlap(&diag_range, &range) {
            // Only create actions for diagnostics that have a fix
            if let Some(action) = lint_to_code_action(lint_diag, &source, uri, &diag_range) {
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
    let start_line = lint_diag.line as u32;
    let end_line = lint_diag.end_line.map(|l| l as u32).unwrap_or(start_line);

    let start_char = lint_diag.start_column.map(|c| c as u32).unwrap_or(0);

    let end_char = match lint_diag.end_column {
        Some(end) => end as u32,
        None => {
            // Fall back to end of the end line
            let target_line = lint_diag.end_line.unwrap_or(lint_diag.line);
            source
                .lines()
                .nth(target_line)
                .map(|l| l.len() as u32)
                .unwrap_or(0)
        }
    };

    Range {
        start: Position {
            line: start_line,
            character: start_char,
        },
        end: Position {
            line: end_line,
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

    // Use make_lint_range to handle both single-line and multi-line diagnostics
    let range = make_lint_range(lint_diag, source);

    Diagnostic {
        range,
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
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            "type-error".to_string(),
        )),
        code_description: None,
        source: Some("seqc".to_string()),
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
        // variant.make-2 should be recognized as a builtin, not flagged as unknown
        // variant.make-2 expects ( field1 field2 Symbol -- V )
        let source = ": main ( -- ) 1 2 :Tag variant.make-2 drop ;";
        let diagnostics = check_document(source);
        // Should have no "Undefined word" errors for variant.make-2
        for d in &diagnostics {
            assert!(
                !d.message.contains("variant.make-2"),
                "variant.make-2 should be recognized as builtin, got: {}",
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

    #[test]
    fn test_shebang_is_tolerated() {
        // Files with shebang should parse without errors
        let source = "#!/usr/bin/env seqc\n: main ( -- Int ) 0 ;";
        let diagnostics = check_document(source);
        assert!(
            diagnostics.is_empty(),
            "Shebang should be tolerated, got: {:?}",
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_if_else_branch_mismatch_reports_correct_line() {
        // If/else with incompatible branch types should report error on the if line
        let source = r#": test ( Bool -- Int )
  if
    42
  else
    "string"
  then
;

: main ( -- )
  true test drop
;
"#;
        let diagnostics = check_document(source);
        assert!(!diagnostics.is_empty(), "Expected type error");
        let diag = &diagnostics[0];
        assert!(
            diag.message.contains("if/else branches have incompatible"),
            "Expected if/else branch mismatch error, got: {}",
            diag.message
        );
        // The 'if' is on line 2 (1-indexed), which is line 1 in 0-indexed LSP coordinates
        assert_eq!(
            diag.range.start.line, 1,
            "Expected error on line 1 (0-indexed), got line {}. Message: {}",
            diag.range.start.line, diag.message
        );
    }

    #[test]
    fn test_diagnostic_structure() {
        // Verify the diagnostic has all required fields for neovim
        let source = r#": test ( Bool -- Int )
  if 42 else "string" then
;
: main ( -- ) true test drop ;
"#;
        let diagnostics = check_document(source);
        assert!(!diagnostics.is_empty(), "Expected diagnostic");
        let diag = &diagnostics[0];

        // These fields must be present for neovim to show file-level diagnostics
        assert!(diag.severity.is_some(), "severity must be set");
        assert!(diag.source.is_some(), "source must be set");

        // Print the diagnostic for debugging
        println!("Diagnostic: {:?}", diag);
        println!("  severity: {:?}", diag.severity);
        println!("  source: {:?}", diag.source);
        println!("  code: {:?}", diag.code);
        println!("  range: {:?}", diag.range);
        println!("  message: {}", diag.message);

        // Verify JSON serialization
        let json = serde_json::to_string_pretty(diag).unwrap();
        println!("JSON:\n{}", json);
        assert!(
            json.contains("\"severity\":"),
            "JSON must contain severity field"
        );
    }

    #[test]
    fn test_stack_type_mismatch_reports_correct_line() {
        // Stack type mismatch should report error on the word call that caused it
        let source = r#"union IntResult {
  Ok { value: Int }
  Err { message: String }
}

: safe-divide ( Int Int -- IntResult )
    dup 0 i.= if
      drop "division by zero" Make-Err
    else
      i./ Make-Ok
    then
;

: main ( -- )
  10 2 safe-divide drop
;
"#;
        let diagnostics = check_document(source);
        assert!(!diagnostics.is_empty(), "Expected type error");
        let diag = &diagnostics[0];
        assert!(
            diag.message.contains("stack type mismatch") || diag.message.contains("Make-Ok"),
            "Expected stack type mismatch error, got: {}",
            diag.message
        );
        // The 'Make-Ok' is on line 10 (1-indexed), which is line 9 in 0-indexed LSP coordinates
        assert_eq!(
            diag.range.start.line, 9,
            "Expected error on line 9 (0-indexed), got line {}. Message: {}",
            diag.range.start.line, diag.message
        );
    }

    #[test]
    fn test_match_arm_mismatch_reports_correct_line() {
        // Match with incompatible arm types should report error on the match line
        let source = r#"union Message {
  Get { value: Int }
  Set { key: Int, value: Int }
}

: handle ( Message -- Int )
  match
    Get -> 42
    Set -> "string"
  end
;

: main ( -- )
  1 Make-Get handle drop
;
"#;
        let diagnostics = check_document(source);
        assert!(!diagnostics.is_empty(), "Expected type error");
        let diag = &diagnostics[0];
        assert!(
            diag.message.contains("match arms have incompatible"),
            "Expected match arms mismatch error, got: {}",
            diag.message
        );
        // The 'match' is on line 7 (1-indexed), which is line 6 in 0-indexed LSP coordinates
        assert_eq!(
            diag.range.start.line, 6,
            "Expected error on line 6 (0-indexed), got line {}. Message: {}",
            diag.range.start.line, diag.message
        );
    }
}
