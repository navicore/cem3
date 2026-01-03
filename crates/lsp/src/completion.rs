use crate::includes::{IncludedWord, LocalWord};
use seqc::builtins::builtin_signatures;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, MarkupContent, MarkupKind,
};

/// Standard library modules available via `include std:module`
const STDLIB_MODULES: &[(&str, &str)] = &[
    ("json", "JSON parsing and serialization"),
    ("yaml", "YAML parsing and serialization"),
    ("http", "HTTP request/response utilities"),
    ("math", "Mathematical functions"),
    ("stack-utils", "Stack manipulation utilities"),
];

/// Context for completion requests.
pub struct CompletionContext<'a> {
    /// The current line text up to the cursor
    pub line_prefix: &'a str,
    /// Words from included modules
    pub included_words: &'a [IncludedWord],
    /// Words defined in the current document
    pub local_words: &'a [LocalWord],
}

/// Completion context type - determines what completions to show
#[derive(Debug, PartialEq)]
enum ContextType {
    /// Inside a string literal - no completions
    InString,
    /// Inside a comment - no completions
    InComment,
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
pub fn get_completions(context: Option<CompletionContext<'_>>) -> Vec<CompletionItem> {
    let Some(ctx) = context else {
        return get_builtin_completions();
    };

    let context_type = detect_context(ctx.line_prefix);

    match context_type {
        ContextType::InString | ContextType::InComment | ContextType::WordDefName => {
            // No completions in these contexts
            Vec::new()
        }
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
    if let Some(_partial) = trimmed.strip_prefix("include std:") {
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
    if line_prefix.contains('#') {
        // Find if cursor is after the #
        if let Some(hash_pos) = line_prefix.rfind('#') {
            // Check if this # is inside a string
            let before_hash = &line_prefix[..hash_pos];
            if !is_in_string(before_hash) {
                return ContextType::InComment;
            }
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
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("```seq\ninclude {}\n```\n\n{}", full_name, desc),
                })),
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
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```seq\ninclude std:{}\n```\n\n{}", name, desc),
            })),
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

/// Get completions for normal code context
fn get_code_completions(
    included_words: &[IncludedWord],
    local_words: &[LocalWord],
) -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add local words first (highest priority)
    for word in local_words {
        let detail = word
            .effect
            .as_ref()
            .map(format_effect)
            .unwrap_or_else(|| "( ? )".to_string());

        items.push(CompletionItem {
            label: word.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail.clone()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "```seq\n: {} {}\n```\n\n*Defined in this file*",
                    word.name, detail
                ),
            })),
            sort_text: Some(format!("0_{}", word.name)), // Sort local words first
            ..Default::default()
        });
    }

    // Add included words
    for word in included_words {
        let detail = word
            .effect
            .as_ref()
            .map(format_effect)
            .unwrap_or_else(|| "( ? )".to_string());

        items.push(CompletionItem {
            label: word.name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(detail.clone()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "```seq\n: {} {}\n```\n\n*From {}*",
                    word.name, detail, word.source
                ),
            })),
            sort_text: Some(format!("1_{}", word.name)), // Sort included words second
            ..Default::default()
        });
    }

    // Add builtins
    items.extend(get_builtin_completions());

    items
}

/// Get builtin completions (used when no context available)
fn get_builtin_completions() -> Vec<CompletionItem> {
    let mut items = Vec::new();

    // Add all builtins with their signatures
    for (name, effect) in builtin_signatures() {
        let signature = format_effect(&effect);
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(signature.clone()),
            documentation: Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```seq\n{} {}\n```\n\n*Built-in*", name, signature),
            })),
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
        (
            "while",
            "( condition-quot body-quot -- )",
            "Loop while condition is true",
        ),
        (
            "until",
            "( body-quot condition-quot -- )",
            "Loop until condition is true",
        ),
        ("times", "( quot n -- )", "Execute quotation n times"),
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
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some(sig.to_string()),
            documentation: Some(Documentation::String(desc.to_string())),
            sort_text: Some(format!("2_{}", name)),
            ..Default::default()
        });
    }

    items
}

/// Format a stack effect for display.
pub fn format_effect(effect: &seqc::Effect) -> String {
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
fn format_type(ty: &seqc::Type) -> String {
    use seqc::Type;

    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::String => "String".to_string(),
        Type::Symbol => "Symbol".to_string(),
        Type::Var(name) => name.clone(),
        Type::Union(name) => name.clone(),
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
    fn test_is_in_string() {
        assert!(!is_in_string("hello"));
        assert!(is_in_string("\"hello"));
        assert!(!is_in_string("\"hello\""));
        assert!(is_in_string("\"hello\" \"world"));
        assert!(!is_in_string("\"hello\" \"world\""));
    }
}
