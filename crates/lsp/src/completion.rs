//! LSP completion logic. Detects cursor context from the line prefix and
//! builds ranked `CompletionItem`s for local/included words, builtins,
//! keywords, stdlib module names, and stack-effect types.

use crate::includes::{IncludedWord, LocalWord};
use seqc::builtins::builtin_signatures;
use seqc::lint::{Severity, known_lint_ids};
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat, MarkupContent, MarkupKind,
};

/// Standard library modules available via `include std:module`
const STDLIB_MODULES: &[(&str, &str)] = &[
    ("imath", "Integer math functions (abs, min, max, clamp)"),
    (
        "fmath",
        "Float math functions (abs, min, max, clamp, floor, ceil)",
    ),
    ("json", "JSON parsing and serialization"),
    ("yaml", "YAML parsing and serialization"),
    ("http", "HTTP request/response utilities"),
    ("stack-utils", "Stack manipulation utilities"),
    ("result", "Result/Option pattern helpers"),
    ("map", "Map utilities and helpers"),
    ("list", "List utilities (list-of, lv)"),
    ("son", "SON serialization (list-of, lv, son.dump)"),
    ("signal", "Unix signal handling"),
    (
        "zipper",
        "Functional list zipper for O(1) cursor navigation",
    ),
];

/// Context for completion requests.
pub(crate) struct CompletionContext<'a> {
    /// The current line text up to the cursor
    pub(crate) line_prefix: &'a str,
    /// Words from included modules
    pub(crate) included_words: &'a [IncludedWord],
    /// Words defined in the current document
    pub(crate) local_words: &'a [LocalWord],
}

/// Completion context type - determines what completions to show
#[derive(Debug, PartialEq)]
enum ContextType {
    /// Inside a string literal - no completions
    InString,
    /// Inside a comment - no completions
    InComment,
    /// Inside the parentheses of a `# seq:allow(...)` annotation
    LintAllow,
    /// After "include " - show modules
    IncludeModule,
    /// After "include std:" - show stdlib modules
    IncludeStdModule,
    /// Inside stack effect declaration ( ... ) - show types
    InStackEffect,
    /// After ":" at start of word definition - no completions (user typing word name)
    WordDefName,
    /// Normal code context - show words, builtins, keywords
    Code,
}

/// Get completion items based on context.
pub(crate) fn get_completions(context: Option<CompletionContext<'_>>) -> Vec<CompletionItem> {
    let Some(ctx) = context else {
        return get_builtin_completions();
    };

    let context_type = detect_context(ctx.line_prefix);

    match context_type {
        ContextType::InString | ContextType::InComment | ContextType::WordDefName => {
            // No completions in these contexts
            Vec::new()
        }
        ContextType::LintAllow => get_lint_allow_completions(ctx.line_prefix),
        ContextType::IncludeModule => get_include_module_completions(ctx.line_prefix),
        ContextType::IncludeStdModule => get_include_std_completions(ctx.line_prefix),
        ContextType::InStackEffect => get_type_completions(),
        ContextType::Code => get_code_completions(ctx.included_words, ctx.local_words),
    }
}

/// Detect what context the cursor is in based on the line prefix
fn detect_context(line_prefix: &str) -> ContextType {
    let trimmed = line_prefix.trim_start();

    // Check for include contexts first (most specific)
    if trimmed.starts_with("include std:") {
        return ContextType::IncludeStdModule;
    }
    if trimmed.starts_with("include ") {
        return ContextType::IncludeModule;
    }

    // Check if we're inside a string (odd number of unescaped quotes)
    if is_in_string(line_prefix) {
        return ContextType::InString;
    }

    // Check for comment (anything after #)
    if let Some(hash_pos) = line_prefix.rfind('#') {
        let before_hash = &line_prefix[..hash_pos];
        if !is_in_string(before_hash) {
            // Inside a comment — but if the cursor sits between the open
            // paren of `seq:allow(` and the (missing) close paren, offer
            // lint IDs. The annotation form is parsed in
            // crates/compiler/src/parser/cursor.rs.
            if is_in_seq_allow(&line_prefix[hash_pos..]) {
                return ContextType::LintAllow;
            }
            return ContextType::InComment;
        }
    }

    // Check for word definition name (: followed by space, cursor right after)
    // Pattern: ": name" where we're typing the name
    if let Some(after_colon) = trimmed.strip_prefix(':') {
        let after_colon = after_colon.trim_start();
        // If there's no space after the word name, we're still typing it
        if !after_colon.contains(' ') && !after_colon.contains('(') {
            return ContextType::WordDefName;
        }
    }

    // Check for stack effect context - inside ( ... )
    // Count unmatched opening parens, ignoring those inside strings
    let unmatched_parens = count_unmatched_parens(line_prefix);
    if unmatched_parens > 0 {
        return ContextType::InStackEffect;
    }

    ContextType::Code
}

/// Count unmatched opening parentheses, ignoring those inside strings
fn count_unmatched_parens(text: &str) -> i32 {
    let mut in_string = false;
    let mut count = 0;

    for c in text.chars() {
        match c {
            '"' => in_string = !in_string,
            '(' if !in_string => count += 1,
            ')' if !in_string => count -= 1,
            _ => {}
        }
    }

    count
}

/// `comment` is the slice from `#` to the cursor. Returns true if the
/// cursor is somewhere the LSP should offer `seq:allow(...)` help.
///
/// Two cases trigger:
/// 1. Cursor is *inside* the parens of `seq:allow(...)` (open, not yet
///    closed) — we'll offer lint IDs.
/// 2. Cursor sits on a non-empty *prefix* of `seq:allow(` (e.g. `# s`,
///    `# seq:`, `# seq:allow`) — we'll offer the marker snippet so one
///    completion scaffolds the whole annotation. `# seq:` is specific
///    enough that this doesn't compete with regular comment prose.
///
/// Whitespace between `#` and `seq:allow` is tolerated to match the
/// parser, which folds it away (`parser/cursor.rs`).
fn is_in_seq_allow(comment: &str) -> bool {
    let after_hash = comment.strip_prefix('#').unwrap_or(comment).trim_start();

    // Case 1: inside the parens.
    if let Some(after_marker) = after_hash.strip_prefix("seq:allow(") {
        return !after_marker.contains(')');
    }

    // Case 2: on a non-empty prefix of `seq:allow(`. We deliberately
    // exclude the empty prefix — opening a bare comment `# ` shouldn't
    // pop the lint UI.
    !after_hash.is_empty() && "seq:allow(".starts_with(after_hash)
}

/// Wrap Markdown text as completion-item `Documentation`.
fn markdown_doc(value: String) -> Documentation {
    Documentation::MarkupContent(MarkupContent {
        kind: MarkupKind::Markdown,
        value,
    })
}

/// Build completions for a `seq:allow` context. Two shapes:
///
/// - **Pre-paren** (`# seq:`, `# seq:allow`, etc.): a single snippet
///   item that scaffolds `seq:allow(<cursor>)`. The user accepts it, the
///   cursor lands inside the parens, and the next request hits the
///   in-paren branch below.
/// - **In-paren** (`# seq:allow(<partial>`): one item per known lint ID,
///   prefix-filtered by what's been typed after the `(`.
fn get_lint_allow_completions(line_prefix: &str) -> Vec<CompletionItem> {
    // Distinguish pre-paren from in-paren by whether `seq:allow(` has
    // appeared yet on the line.
    if !line_prefix.contains("seq:allow(") {
        return vec![lint_allow_snippet_item()];
    }

    let partial = line_prefix
        .rfind('(')
        .map(|i| &line_prefix[i + 1..])
        .unwrap_or("");

    known_lint_ids()
        .into_iter()
        .filter(|lint| lint.id.starts_with(partial))
        .map(|lint| {
            let severity = match lint.severity {
                Severity::Error => "error",
                Severity::Warning => "warning",
                Severity::Hint => "hint",
            };
            CompletionItem {
                label: lint.id.clone(),
                kind: Some(CompletionItemKind::ENUM_MEMBER),
                detail: Some(severity.to_string()),
                documentation: Some(markdown_doc(format!(
                    "**{}** *({})*\n\n{}",
                    lint.id, severity, lint.message
                ))),
                ..Default::default()
            }
        })
        .collect()
}

/// The single pre-paren snippet item. Accepting it inserts
/// `seq:allow(<cursor>)` so the next keystroke triggers the in-paren
/// per-ID list.
fn lint_allow_snippet_item() -> CompletionItem {
    CompletionItem {
        label: "seq:allow(...)".to_string(),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some("suppress a specific lint for the next word".to_string()),
        documentation: Some(markdown_doc(
            "Annotate the next word definition to suppress a specific lint.\n\n\
                    ```seq\n# seq:allow(lint-id)\n: my-word ( -- ) ... ;\n```"
                .to_string(),
        )),
        insert_text: Some("seq:allow($0)".to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        // filter_text lets editors that match the typed prefix against
        // the *filter text* (rather than the label) keep this item
        // visible after the user types `seq:` or further.
        filter_text: Some("seq:allow".to_string()),
        ..Default::default()
    }
}

/// Check if cursor position is inside a string literal
fn is_in_string(text: &str) -> bool {
    let mut in_string = false;

    for c in text.chars() {
        if c == '"' {
            in_string = !in_string;
        }
        // Note: Seq doesn't currently support escape sequences in strings
    }

    in_string
}

/// Get completions for "include " context
fn get_include_module_completions(line_prefix: &str) -> Vec<CompletionItem> {
    let trimmed = line_prefix.trim_start();
    let partial = trimmed.strip_prefix("include ").unwrap_or("");

    let mut items = Vec::new();

    // Suggest std: prefix if it matches
    if "std:".starts_with(partial) || partial.is_empty() {
        items.push(CompletionItem {
            label: "std:".to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some("Standard library".to_string()),
            documentation: Some(Documentation::String(
                "Include a module from the standard library".to_string(),
            )),
            ..Default::default()
        });
    }

    // Also suggest full std:module completions
    for (name, desc) in STDLIB_MODULES {
        let full_name = format!("std:{}", name);
        if full_name.starts_with(partial) {
            items.push(CompletionItem {
                label: full_name.clone(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some(desc.to_string()),
                documentation: Some(markdown_doc(format!(
                    "```seq\ninclude {}\n```\n\n{}",
                    full_name, desc
                ))),
                ..Default::default()
            });
        }
    }

    items
}

/// Get completions for "include std:" context
fn get_include_std_completions(line_prefix: &str) -> Vec<CompletionItem> {
    let trimmed = line_prefix.trim_start();
    let partial = trimmed.strip_prefix("include std:").unwrap_or("");

    STDLIB_MODULES
        .iter()
        .filter(|(name, _)| name.starts_with(partial))
        .map(|(name, desc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(desc.to_string()),
            documentation: Some(markdown_doc(format!(
                "```seq\ninclude std:{}\n```\n\n{}",
                name, desc
            ))),
            ..Default::default()
        })
        .collect()
}

/// Get type completions for stack effect declarations
fn get_type_completions() -> Vec<CompletionItem> {
    let types = [
        ("Int", "64-bit signed integer"),
        ("Float", "64-bit floating point"),
        ("Bool", "Boolean (true/false)"),
        ("String", "UTF-8 string"),
        ("--", "Stack effect separator"),
    ];

    types
        .iter()
        .map(|(name, desc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            detail: Some(desc.to_string()),
            ..Default::default()
        })
        .collect()
}

/// Build a FUNCTION-kind CompletionItem for a user-visible word.
///
/// `source_trailer` is the trailing italicized markdown line
/// (e.g. `*Defined in this file*` or `*From utils*`).
fn make_word_completion(
    name: &str,
    effect: Option<&seqc::Effect>,
    source_trailer: &str,
    sort_prefix: &str,
) -> CompletionItem {
    let detail = effect
        .map(format_effect)
        .unwrap_or_else(|| "( ? )".to_string());
    let doc_value = format!("```seq\n: {} {}\n```\n\n{}", name, detail, source_trailer);
    CompletionItem {
        label: name.to_string(),
        // OPERATOR — not FUNCTION. Seq is concatenative: words consume
        // the stack and have no parenthesised argument list. Many editors
        // (nvim-cmp, VS Code) auto-insert `()` on confirm when the kind
        // is FUNCTION/METHOD; OPERATOR keeps the inserted text bare.
        kind: Some(CompletionItemKind::OPERATOR),
        detail: Some(detail),
        documentation: Some(markdown_doc(doc_value)),
        sort_text: Some(format!("{}{}", sort_prefix, name)),
        ..Default::default()
    }
}

/// Get completions for normal code context
fn get_code_completions(
    included_words: &[IncludedWord],
    local_words: &[LocalWord],
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add local words first (highest priority)
    for word in local_words {
        items.push(make_word_completion(
            &word.name,
            word.effect.as_ref(),
            "*Defined in this file*",
            "0_",
        ));
    }

    for word in included_words {
        let trailer = format!("*From {}*", word.source);
        items.push(make_word_completion(
            &word.name,
            word.effect.as_ref(),
            &trailer,
            "1_",
        ));
    }

    items.extend(get_builtin_completions());

    items
}

/// Get builtin completions (used when no context available)
fn get_builtin_completions() -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add all builtins with their signatures
    for (name, effect) in builtin_signatures() {
        let signature = format_effect(&effect);
        let doc_value = format!("```seq\n{} {}\n```\n\n*Built-in*", name, signature);
        items.push(CompletionItem {
            label: name.clone(),
            // OPERATOR — see make_word_completion for rationale.
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some(signature),
            documentation: Some(markdown_doc(doc_value)),
            sort_text: Some(format!("2_{}", name)), // Sort builtins after local/included
            ..Default::default()
        });
    }

    // Add keywords
    for keyword in &["if", "else", "then", "include", "true", "false"] {
        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            sort_text: Some(format!("3_{}", keyword)), // Sort keywords last
            ..Default::default()
        });
    }

    // Add control flow builtins with descriptions
    let control_flow = [
        ("call", "( quot -- ... )", "Execute a quotation"),
        (
            "spawn",
            "( quot -- strand-id )",
            "Spawn quotation as new strand",
        ),
    ];

    for (name, sig, desc) in control_flow {
        // Skip if already added from builtin_signatures
        if items.iter().any(|i| i.label == name) {
            continue;
        }
        items.push(CompletionItem {
            label: name.to_string(),
            // OPERATOR — see make_word_completion for rationale.
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some(sig.to_string()),
            documentation: Some(Documentation::String(desc.to_string())),
            sort_text: Some(format!("2_{}", name)),
            ..Default::default()
        });
    }

    items
}

/// Format a stack effect for display.
pub(crate) fn format_effect(effect: &seqc::Effect) -> String {
    format!(
        "( {} -- {} )",
        format_stack(&effect.inputs),
        format_stack(&effect.outputs)
    )
}

/// Format a stack type for display.
fn format_stack(stack: &seqc::StackType) -> String {
    use seqc::StackType;

    match stack {
        StackType::Empty => String::new(),
        StackType::RowVar(name) => format!("..{}", name),
        StackType::Cons { rest, top } => {
            let rest_str = format_stack(rest);
            let top_str = format_type(top);
            if rest_str.is_empty() {
                top_str
            } else {
                format!("{} {}", rest_str, top_str)
            }
        }
    }
}

/// Format a type for display.
pub(crate) fn format_type(ty: &seqc::Type) -> String {
    use seqc::Type;

    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::String => "String".to_string(),
        Type::Symbol => "Symbol".to_string(),
        Type::Channel => "Channel".to_string(),
        Type::Socket => "Socket".to_string(),
        Type::Var(name) => name.clone(),
        Type::Union(name) => name.clone(),
        Type::Variant => "Variant".to_string(),
        Type::Quotation(effect) => format!("[ {} ]", format_effect(effect)),
        Type::Closure { effect, .. } => format!("{{ {} }}", format_effect(effect)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_context_code() {
        assert_eq!(detect_context("  dup"), ContextType::Code);
        assert_eq!(detect_context("1 2 +"), ContextType::Code);
    }

    #[test]
    fn test_detect_context_include() {
        assert_eq!(detect_context("include "), ContextType::IncludeModule);
        assert_eq!(
            detect_context("include std:"),
            ContextType::IncludeStdModule
        );
        assert_eq!(
            detect_context("include std:js"),
            ContextType::IncludeStdModule
        );
    }

    #[test]
    fn test_detect_context_string() {
        assert_eq!(detect_context("\"hello"), ContextType::InString);
        assert_eq!(detect_context("\"hello\" "), ContextType::Code);
        assert_eq!(detect_context("\"hello\" \"world"), ContextType::InString);
    }

    #[test]
    fn test_detect_context_comment() {
        assert_eq!(detect_context("# comment"), ContextType::InComment);
        assert_eq!(detect_context("dup # comment"), ContextType::InComment);
        // Hash inside string is not a comment
        assert_eq!(detect_context("\"#hashtag\""), ContextType::Code);
    }

    #[test]
    fn test_detect_context_lint_allow_inside_parens() {
        // Cursor inside the parens — LintAllow context.
        assert_eq!(
            detect_context("# seq:allow("),
            ContextType::LintAllow,
            "after the open paren"
        );
        assert_eq!(
            detect_context("# seq:allow(unc"),
            ContextType::LintAllow,
            "partial id typed"
        );
        // No space between # and seq:allow — parser accepts it, LSP must too.
        assert_eq!(
            detect_context("#seq:allow("),
            ContextType::LintAllow,
            "no space after hash"
        );
        // Closing paren ends the LintAllow window — back to comment.
        assert_eq!(
            detect_context("# seq:allow(unchecked-tcp-write) "),
            ContextType::InComment,
            "after the close paren"
        );
        // Plain comments stay InComment.
        assert_eq!(
            detect_context("# seq:allow is great"),
            ContextType::InComment,
            "no parens at all"
        );
    }

    #[test]
    fn test_detect_context_lint_allow_marker_prefix() {
        // Typing the marker itself — should also fire so the editor pops
        // the snippet completion.
        for prefix in &[
            "# s",
            "# se",
            "# seq",
            "# seq:",
            "# seq:a",
            "# seq:allow",
            "#seq:",
        ] {
            assert_eq!(
                detect_context(prefix),
                ContextType::LintAllow,
                "expected LintAllow at {:?}",
                prefix
            );
        }
        // Bare comment (no marker text yet) must NOT fire — we don't
        // want a popup the moment someone opens a normal comment.
        assert_eq!(detect_context("# "), ContextType::InComment);
        assert_eq!(detect_context("#"), ContextType::InComment);
        // Comments with unrelated text stay InComment.
        assert_eq!(detect_context("# todo: fix this"), ContextType::InComment);
        assert_eq!(detect_context("# fixme"), ContextType::InComment);
    }

    #[test]
    fn test_lint_allow_completions_pre_paren_offers_snippet() {
        // Before the user types `(`, we offer one snippet item that
        // scaffolds the whole `seq:allow(...)` form.
        let items = get_lint_allow_completions("# seq:");
        assert_eq!(items.len(), 1, "expected single snippet item");
        let item = &items[0];
        assert_eq!(item.kind, Some(CompletionItemKind::SNIPPET));
        assert_eq!(item.insert_text.as_deref(), Some("seq:allow($0)"));
        assert_eq!(item.insert_text_format, Some(InsertTextFormat::SNIPPET));
    }

    #[test]
    fn test_lint_allow_completions_include_known_ids() {
        let items = get_lint_allow_completions("# seq:allow(");
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(
            labels.contains(&"unchecked-tcp-write"),
            "expected unchecked-tcp-write in {:?}",
            labels
        );
        assert!(
            labels.contains(&"deep-nesting"),
            "expected hard-coded deep-nesting in {:?}",
            labels
        );
        assert!(
            labels.contains(&"unchecked-error-flag"),
            "expected hard-coded unchecked-error-flag in {:?}",
            labels
        );
    }

    #[test]
    fn test_lint_allow_completions_filter_by_prefix() {
        let items = get_lint_allow_completions("# seq:allow(unchecked-tcp");
        // Every item must start with the typed prefix.
        for item in &items {
            assert!(
                item.label.starts_with("unchecked-tcp"),
                "{} doesn't match prefix",
                item.label
            );
        }
        assert!(!items.is_empty(), "expected at least one unchecked-tcp-*");
    }

    #[test]
    fn test_detect_context_word_def() {
        assert_eq!(detect_context(": my-word"), ContextType::WordDefName);
        assert_eq!(detect_context(": my-word ("), ContextType::InStackEffect);
        assert_eq!(
            detect_context(": my-word ( Int"),
            ContextType::InStackEffect
        );
    }

    #[test]
    fn test_detect_context_stack_effect() {
        assert_eq!(detect_context("( Int"), ContextType::InStackEffect);
        assert_eq!(detect_context("( Int -- "), ContextType::InStackEffect);
        assert_eq!(detect_context("( Int -- Int )"), ContextType::Code);
        // Parens inside strings should be ignored
        assert_eq!(detect_context("\"(\" dup"), ContextType::Code);
        assert_eq!(detect_context("\")\" dup"), ContextType::Code);
    }

    #[test]
    fn test_detect_context_dotted_prefix() {
        // Typing "int." or "f." should remain in Code context so completions stay open
        assert_eq!(detect_context("int."), ContextType::Code);
        assert_eq!(detect_context("f."), ContextType::Code);
        assert_eq!(detect_context("list.m"), ContextType::Code);
        assert_eq!(detect_context("  map.get"), ContextType::Code);
    }

    #[test]
    fn test_completions_include_dotted_builtins() {
        let items = get_builtin_completions();
        // Verify that dotted builtins like int.add, f.add, etc. are present
        let has_dotted = items.iter().any(|item| item.label.contains('.'));
        assert!(
            has_dotted,
            "Builtin completions should include dotted names like int.add"
        );
    }

    #[test]
    fn test_word_completion_kind_is_operator() {
        // Regression: word completions must use OPERATOR (not FUNCTION /
        // METHOD) so editors don't auto-insert `()` on confirm. Seq is
        // concatenative — words never take parenthesised arguments.
        for item in get_builtin_completions() {
            if item.kind == Some(CompletionItemKind::KEYWORD) {
                continue; // if/else/then/include/true/false — fine as KEYWORD
            }
            assert_eq!(
                item.kind,
                Some(CompletionItemKind::OPERATOR),
                "completion item {:?} should be OPERATOR, got {:?}",
                item.label,
                item.kind,
            );
        }
    }

    #[test]
    fn test_is_in_string() {
        assert!(!is_in_string("hello"));
        assert!(is_in_string("\"hello"));
        assert!(!is_in_string("\"hello\""));
        assert!(is_in_string("\"hello\" \"world"));
        assert!(!is_in_string("\"hello\" \"world\""));
    }
}
