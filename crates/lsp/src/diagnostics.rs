use crate::includes::IncludedWord;
use seqc::{Parser, TypeChecker, lint};
use std::path::PathBuf;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};
use tracing::{debug, warn};

/// Check a document for parse and type errors, returning LSP diagnostics.
///
/// This version doesn't know about included words - use `check_document_with_includes`
/// for include-aware diagnostics.
#[cfg(test)]
pub fn check_document(source: &str) -> Vec<Diagnostic> {
    check_document_with_includes(source, &[])
}

/// Check a document for parse and type errors, with knowledge of included words.
///
/// The `included_words` parameter should contain all words available from
/// included modules, with their effects if known.
pub fn check_document_with_includes(
    source: &str,
    included_words: &[IncludedWord],
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
    if let Ok(linter) = lint::Linter::with_defaults() {
        let lint_diagnostics = linter.lint_program(&program, &PathBuf::from("source.seq"));
        for lint_diag in lint_diagnostics {
            diagnostics.push(lint_to_diagnostic(&lint_diag, source));
        }
    }

    diagnostics
}

/// Convert a lint diagnostic to an LSP diagnostic.
fn lint_to_diagnostic(lint_diag: &lint::LintDiagnostic, source: &str) -> Diagnostic {
    let line = lint_diag.line as u32;

    // Calculate line length for highlighting
    let line_length = source
        .lines()
        .nth(lint_diag.line)
        .map(|l| l.len() as u32)
        .unwrap_or(0);

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

    Diagnostic {
        range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: line_length,
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
            lint_diags
                .iter()
                .any(|d| d.message.contains("swaps cancel")),
            "Expected swap swap warning, got: {:?}",
            lint_diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
}
