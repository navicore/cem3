//! Parse, type-check, and lint orchestration for the LSP, plus conversion
//! from compiler/lint results into LSP diagnostics and code actions.

use crate::includes::IncludeResolution;
use seqc::ast::{Program, QuotationSpan, Statement};
use seqc::lint::known_lint_ids;
use seqc::types::Type;
use seqc::{Parser, TypeChecker, lint};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, Diagnostic, DiagnosticSeverity, Position, Range, TextEdit, Url,
    WorkspaceEdit,
};
use tracing::{debug, warn};

/// Information about a quotation for LSP hover support
#[derive(Debug, Clone)]
pub(crate) struct QuotationInfo {
    /// The quotation's source span
    pub(crate) span: QuotationSpan,
    /// The inferred type (Quotation or Closure with effect)
    pub(crate) inferred_type: Type,
}

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

/// Resolve the path used for lint/error-flag diagnostics; defaults to `source.seq`
/// when the document has no on-disk path yet (e.g. untitled buffer).
fn default_lint_path(file_path: Option<&Path>) -> PathBuf {
    file_path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("source.seq"))
}

/// Build a `WorkspaceEdit` that changes a single range in a single file.
fn single_file_workspace_edit(uri: &Url, range: Range, new_text: String) -> WorkspaceEdit {
    let edit = TextEdit { range, new_text };
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);
    WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    }
}

/// Build an LSP `Diagnostic`, defaulting the optional fields to `None`.
fn make_diagnostic(
    range: Range,
    severity: DiagnosticSeverity,
    code: &str,
    source: &str,
    message: String,
) -> Diagnostic {
    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(tower_lsp::lsp_types::NumberOrString::String(
            code.to_string(),
        )),
        code_description: None,
        source: Some(source.to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Build a QUICKFIX `CodeAction` with the optional fields defaulted to `None`.
fn quickfix_action(title: String, edit: WorkspaceEdit, is_preferred: Option<bool>) -> CodeAction {
    CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: None,
        edit: Some(edit),
        command: None,
        is_preferred,
        disabled: None,
        data: None,
    }
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
pub(crate) fn check_document_with_quotations(
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
    let lint_file_path = default_lint_path(file_path);
    if let Ok(linter) = lint::Linter::with_defaults() {
        let lint_diagnostics = linter.lint_program(&program, &lint_file_path);
        for lint_diag in lint_diagnostics {
            diagnostics.push(lint_to_diagnostic(&lint_diag, &source));
        }
    }

    // Phase 4a: Flag unknown lint IDs in `# seq:allow(...)` annotations.
    diagnostics.extend(unknown_seq_allow_diagnostics(&program, &source));

    // Phase 4b: Error flag tracking (unchecked Bool from fallible operations)
    {
        let mut flag_analyzer = seqc::ErrorFlagAnalyzer::new(&lint_file_path);
        let flag_diagnostics = flag_analyzer.analyze_program(&program);
        for flag_diag in flag_diagnostics {
            diagnostics.push(lint_to_diagnostic(&flag_diag, &source));
        }
    }

    (diagnostics, quotation_info)
}

/// Get code actions for diagnostics that overlap with the given range.
///
/// Two sources contribute:
/// - lint diagnostics (re-derived by re-running the linter on the source);
/// - cached typecheck/parse diagnostics (passed in by the caller, so we
///   don't re-typecheck on every lightbulb click).
pub(crate) fn get_code_actions(
    source: &str,
    range: Range,
    uri: &Url,
    file_path: Option<&Path>,
    cached_diagnostics: &[Diagnostic],
) -> Vec<CodeAction> {
    let mut actions = Vec::new();

    // Strip shebang if present (for script mode files)
    let source = strip_shebang(source);

    // Parse the source
    let mut parser = Parser::new(&source);
    let Ok(program) = parser.parse() else {
        return actions; // No actions if parse fails
    };

    // Run linter
    let lint_file_path = default_lint_path(file_path);

    let Ok(linter) = lint::Linter::with_defaults() else {
        return actions;
    };

    let lint_diagnostics = linter.lint_program(&program, &lint_file_path);

    // Find lint diagnostics that overlap with the requested range
    for lint_diag in &lint_diagnostics {
        let diag_range = make_lint_range(lint_diag, &source);

        // Check if ranges overlap
        if ranges_overlap(&diag_range, &range) {
            // Only create actions for diagnostics that have a fix
            if let Some(action) = lint_to_code_action(lint_diag, uri, &diag_range) {
                actions.push(action);
            }
        }
    }

    // Sugar-resolution failures emit type-error diagnostics whose message
    // names the operator. When the cursor overlaps one, offer typed-form
    // rewrites (`i.+`, `f.+`, `string.concat`, …).
    for diag in cached_diagnostics {
        if !ranges_overlap(&diag.range, &range) {
            continue;
        }
        actions.extend(sugar_code_actions(diag, uri));
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
    uri: &Url,
    range: &Range,
) -> Option<CodeAction> {
    // For unchecked-* lint rules, offer "Add error check" instead of "Remove"
    if lint_diag.id.starts_with("unchecked-") {
        return unchecked_error_code_action(lint_diag, uri, range);
    }

    let title = if lint_diag.replacement.is_empty() {
        format!("Remove redundant code ({})", lint_diag.id)
    } else {
        format!("Replace with `{}`", lint_diag.replacement)
    };

    let workspace_edit = single_file_workspace_edit(uri, *range, lint_diag.replacement.clone());

    Some(quickfix_action(title, workspace_edit, Some(true)))
}

/// Generate a code action for unchecked error flag diagnostics.
///
/// Replaces `op drop` with an `if` error check skeleton:
/// ```seq
/// op [
///   # success
/// ] [
///   drop  # handle error
/// ] if
/// ```
fn unchecked_error_code_action(
    lint_diag: &lint::LintDiagnostic,
    uri: &Url,
    range: &Range,
) -> Option<CodeAction> {
    let title = "Add error check (if)".to_string();

    // The range covers "op drop" — replace just the "drop" part with the skeleton.
    // The diagnostic range starts at the operation and ends after "drop".
    // We want to replace "drop" (the last word in the range) with the skeleton.
    // Since we can't easily compute the "drop" sub-range, we replace the whole
    // matched pattern. The pattern is "op drop", so we replace with the
    // combinator skeleton.
    //
    // Extract the operation name from the diagnostic message.
    // Messages follow the pattern: "`op` returns ..."
    let op_name = lint_diag
        .message
        .strip_prefix('`')
        .and_then(|s| s.split('`').next())
        .unwrap_or("op");

    let new_text = format!(
        "{} [\n    # success\n  ] [\n    drop  # handle {} error\n  ] if",
        op_name, op_name,
    );

    let workspace_edit = single_file_workspace_edit(uri, *range, new_text);

    // not preferred — user should review the generated skeleton
    Some(quickfix_action(title, workspace_edit, Some(false)))
}

/// Typed alternatives offered for each unresolved-sugar operator.
///
/// `is_preferred` is `None` on the resulting actions because no
/// alternative is uniquely correct without type context the LSP can't
/// reproduce. The list order (most-likely first) is therefore the only
/// tiebreaker — most editors render quick-fixes in arrival order.
const SUGAR_ALTERNATIVES: &[(&str, &[&str])] = &[
    ("+", &["i.+", "f.+", "string.concat"]),
    ("-", &["i.-", "f.-"]),
    ("*", &["i.*", "f.*"]),
    ("/", &["i./", "f./"]),
    ("%", &["i.%"]),
    ("=", &["i.=", "f.=", "string.equal?"]),
    ("<", &["i.<", "f.<"]),
    (">", &["i.>", "f.>"]),
    ("<=", &["i.<=", "f.<="]),
    (">=", &["i.>=", "f.>="]),
    ("<>", &["i.<>", "f.<>"]),
];

/// If the diagnostic looks like an unresolved-sugar failure, emit a
/// `Replace `+` with `i.+`` quick-fix per typed alternative.
///
/// Position info comes from the `at line N col M:` prefix the typechecker
/// embeds in sugar errors when a span is available — using that lets us
/// pinpoint the right token even when a line has multiple sugar
/// occurrences. (Note: `character` here is a UTF-8 byte offset because
/// the server declares `PositionEncodingKind::UTF8` in its capabilities.)
fn sugar_code_actions(diag: &Diagnostic, uri: &Url) -> Vec<CodeAction> {
    let msg = &diag.message;
    if !msg.contains("can't resolve here") && !msg.contains("requires matching types") {
        return Vec::new();
    }
    let Some(op) = first_backticked(msg) else {
        return Vec::new();
    };
    let Some((_, alternatives)) = SUGAR_ALTERNATIVES.iter().find(|(o, _)| *o == op) else {
        return Vec::new();
    };
    // No `at line N col M:` prefix means the typechecker had no span (e.g.
    // a programmatically constructed call). Skip rather than risk editing
    // the wrong occurrence — the legacy `line N` fallback is intentional.
    let Some((line, col)) = parse_position_prefix(msg) else {
        return Vec::new();
    };
    let op_range = Range {
        start: Position {
            line,
            character: col,
        },
        end: Position {
            line,
            character: col + op.len() as u32,
        },
    };

    alternatives
        .iter()
        .map(|replacement| {
            let title = format!("Replace `{}` with `{}`", op, replacement);
            let edit = single_file_workspace_edit(uri, op_range, (*replacement).to_string());
            quickfix_action(title, edit, None)
        })
        .collect()
}

/// Return the contents of the first backtick-bounded span in `s`. The
/// typechecker's sugar-failure messages always lead with the operator
/// name in backticks (e.g. `` `+` can't resolve... ``).
fn first_backticked(s: &str) -> Option<&str> {
    let start = s.find('`')? + 1;
    let rest = &s[start..];
    let end = rest.find('`')?;
    Some(&rest[..end])
}

/// Parse `at line N col M:` from a typechecker error message. Returns
/// 0-indexed (line, col) suitable for an LSP `Position`.
///
/// `line`/`col` in the message are 1-indexed; `0` would mean a malformed
/// message, so `checked_sub` bails out rather than mapping to (0, 0) and
/// risking a phantom edit at the start of the file.
fn parse_position_prefix(msg: &str) -> Option<(u32, u32)> {
    let after_line = msg.strip_prefix("at line ")?;
    let line_end = after_line.find(' ')?;
    let line_1: u32 = after_line[..line_end].parse().ok()?;
    let after = &after_line[line_end + 1..];
    let after_col = after.strip_prefix("col ")?;
    let col_end = after_col.find(':')?;
    let col_1: u32 = after_col[..col_end].parse().ok()?;
    Some((line_1.checked_sub(1)?, col_1.checked_sub(1)?))
}

/// Walk `program.words` and emit a warning for every `# seq:allow(<id>)`
/// where `<id>` isn't a lint the compiler honors.
///
/// We don't track the annotation comment's own source position (the
/// parser folds it into the next word's `allowed_lints` without a span),
/// so the diagnostic is pinned to the word definition's line. That's
/// close enough for the editor to find via the lightbulb, and the
/// message names the offending ID verbatim.
fn unknown_seq_allow_diagnostics(program: &Program, source: &str) -> Vec<Diagnostic> {
    let known: HashSet<String> = known_lint_ids().into_iter().map(|l| l.id).collect();
    let mut out = Vec::new();

    for word in &program.words {
        for id in &word.allowed_lints {
            if known.contains(id) {
                continue;
            }
            let line = word
                .source
                .as_ref()
                .map(|s| s.start_line as u32)
                .unwrap_or(0);
            let line_length = source
                .lines()
                .nth(line as usize)
                .map(|l| l.len() as u32)
                .unwrap_or(0);

            let range = Range {
                start: Position { line, character: 0 },
                end: Position {
                    line,
                    character: line_length,
                },
            };
            out.push(make_diagnostic(
                range,
                DiagnosticSeverity::WARNING,
                "unknown-seq-allow",
                "seq-lint",
                format!(
                    "`# seq:allow({})` references unknown lint id `{}` (annotation has no effect)",
                    id, id
                ),
            ));
        }
    }

    out
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

    let range = make_lint_range(lint_diag, source);

    make_diagnostic(range, severity, &lint_diag.id, "seq-lint", message)
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

    let range = Range {
        start: Position {
            line: line as u32,
            character: 0,
        },
        end: Position {
            line: line as u32,
            character: line_length,
        },
    };

    make_diagnostic(
        range,
        DiagnosticSeverity::ERROR,
        "type-error",
        "seqc",
        message.to_string(),
    )
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
    fn test_unknown_seq_allow_id_warns() {
        // A misspelled lint id should produce an "unknown-seq-allow" warning.
        let source = "# seq:allow(unckecked-tcp-write)\n: main ( -- Int ) 0 ;";
        let diagnostics = check_document(source);
        let unknown: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| match &d.code {
                Some(tower_lsp::lsp_types::NumberOrString::String(s)) => s == "unknown-seq-allow",
                _ => false,
            })
            .collect();
        assert_eq!(
            unknown.len(),
            1,
            "expected one unknown-seq-allow warning, got {} diagnostics: {:?}",
            unknown.len(),
            diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
        assert_eq!(unknown[0].severity, Some(DiagnosticSeverity::WARNING));
        assert!(
            unknown[0].message.contains("unckecked-tcp-write"),
            "message should name the typo'd id, got: {}",
            unknown[0].message
        );
    }

    #[test]
    fn test_known_seq_allow_ids_dont_warn() {
        // Real IDs (from lints.toml and the hard-coded set) must not warn.
        let source = "# seq:allow(unchecked-tcp-write)\n\
                      # seq:allow(deep-nesting)\n\
                      # seq:allow(unchecked-error-flag)\n\
                      : main ( -- Int ) 0 ;";
        let diagnostics = check_document(source);
        let unknown: Vec<&Diagnostic> = diagnostics
            .iter()
            .filter(|d| match &d.code {
                Some(tower_lsp::lsp_types::NumberOrString::String(s)) => s == "unknown-seq-allow",
                _ => false,
            })
            .collect();
        assert!(
            unknown.is_empty(),
            "real lint ids should not warn, got: {:?}",
            unknown.iter().map(|d| &d.message).collect::<Vec<_>>()
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
        // if with incompatible branch types should report error on the line
        // of the trailing combinator (matches the lowered Statement::If span).
        let source = r#": test ( Bool -- Int )
  [
    42
  ] [
    "string"
  ] if
;

: main ( -- )
  true test drop
;
"#;
        let diagnostics = check_document(source);
        assert!(!diagnostics.is_empty(), "Expected type error");
        let diag = &diagnostics[0];
        assert!(
            diag.message.contains("branches have incompatible"),
            "Expected branch mismatch error, got: {}",
            diag.message
        );
        // The `if` call is on line 6 (1-indexed); its rewritten Statement::If
        // takes that span. 0-indexed for LSP -> line 5.
        assert_eq!(
            diag.range.start.line, 5,
            "Expected error on line 5 (0-indexed), got line {}. Message: {}",
            diag.range.start.line, diag.message
        );
    }

    #[test]
    fn test_diagnostic_structure() {
        // Verify the diagnostic has all required fields for neovim
        let source = r#": test ( Bool -- Int )
  [ 42 ] [ "string" ] if
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
    dup 0 i.= [
      drop "division by zero" Make-Err
    ] [
      i./ Make-Ok
    ] if
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
        // The `Make-Ok` is on line 10 (1-indexed) → line 9 in 0-indexed LSP coords.
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

    /// Pin the wording coupling between the typechecker's sugar-failure
    /// errors and `sugar_code_actions`. If the typechecker error wording
    /// drifts away from "can't resolve here" / "requires matching types"
    /// the LSP code action would silently disappear; this test breaks
    /// loudly instead.
    #[test]
    fn sugar_code_action_recognizes_known_wordings() {
        let uri: Url = "file:///fake/path.seq".parse().unwrap();

        // Quotation-body case (post-#420 wording).
        let source = "3 4 [ + ] call\n";
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: source.len() as u32,
                },
            },
            message: "at line 1 col 7: `+` can't resolve here — operand types not in scope. \
                      Use `i.+`, `f.+`, or `string.concat`."
                .to_string(),
            ..Default::default()
        };
        let actions = sugar_code_actions(&diag, &uri);
        assert_eq!(
            actions.len(),
            3,
            "expected 3 typed alternatives for `+`, got {}",
            actions.len()
        );
        assert!(actions.iter().any(|a| a.title.contains("i.+")));
        assert!(actions.iter().any(|a| a.title.contains("f.+")));
        assert!(actions.iter().any(|a| a.title.contains("string.concat")));
        // Edit range must target just the `+` operator at col 7 (1-indexed),
        // not the whole diagnostic line.
        let edit = actions[0].edit.as_ref().unwrap();
        let changes = edit.changes.as_ref().unwrap();
        let text_edit = &changes.values().next().unwrap()[0];
        assert_eq!(text_edit.range.start.character, 6);
        assert_eq!(text_edit.range.end.character, 7);

        // Type-mismatch case (legacy wording).
        let source2 = "3 \"hi\" +\n";
        let diag2 = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: source2.len() as u32,
                },
            },
            message: "at line 1 col 8: `+` requires matching types (Int+Int, Float+Float, \
                      or String+String), got (Int, String). Use `i.+`, `f.+`, or \
                      `string.concat`."
                .to_string(),
            ..Default::default()
        };
        let actions2 = sugar_code_actions(&diag2, &uri);
        assert_eq!(
            actions2.len(),
            3,
            "expected 3 alternatives in mismatch case"
        );
    }

    /// When the typechecker emits the legacy `at line N:` prefix (no
    /// column — happens when no AST span is available), we can't pinpoint
    /// the operator, so no code action fires. Better than risking a
    /// phantom edit at file start.
    #[test]
    fn sugar_code_action_no_column_prefix_yields_no_actions() {
        let uri: Url = "file:///fake/path.seq".parse().unwrap();
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 16,
                },
            },
            // Legacy `at line N:` form — no `col M:`.
            message: "at line 1: `+` can't resolve here — operand types not in scope. \
                      Use `i.+`, `f.+`, or `string.concat`."
                .to_string(),
            ..Default::default()
        };
        let actions = sugar_code_actions(&diag, &uri);
        assert!(
            actions.is_empty(),
            "no-column message should produce no code actions, got {} action(s)",
            actions.len()
        );
    }

    #[test]
    fn sugar_code_action_ignores_unrelated_diagnostic() {
        let uri: Url = "file:///fake/path.seq".parse().unwrap();
        let diag = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 10,
                },
            },
            message: "Parse error: unexpected token".to_string(),
            ..Default::default()
        };
        let actions = sugar_code_actions(&diag, &uri);
        assert!(
            actions.is_empty(),
            "non-sugar diagnostic should produce no code actions"
        );
    }
}
