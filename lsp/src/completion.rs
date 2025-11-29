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
}

/// Get completion items based on context.
pub fn get_completions(context: Option<CompletionContext<'_>>) -> Vec<CompletionItem> {
    // Check if we're in an include context
    if let Some(ctx) = context
        && let Some(items) = get_include_completions(ctx.line_prefix)
    {
        return items;
    }

    get_general_completions()
}

/// Check if we're completing an include statement and return appropriate completions.
fn get_include_completions(line_prefix: &str) -> Option<Vec<CompletionItem>> {
    let trimmed = line_prefix.trim_start();

    // Check for "include std:" - complete with module names
    if let Some(partial) = trimmed.strip_prefix("include std:") {
        let items = STDLIB_MODULES
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
            .collect();
        return Some(items);
    }

    // Check for "include " - complete with std: prefix and local hints
    if let Some(partial) = trimmed.strip_prefix("include ") {
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

        return Some(items);
    }

    None
}

/// Get general completion items for all builtins and keywords.
fn get_general_completions() -> Vec<CompletionItem> {
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
                value: format!("```seq\n{} {}\n```", name, signature),
            })),
            ..Default::default()
        });
    }

    // Add keywords
    for keyword in &["if", "else", "then", "include", "true", "false"] {
        items.push(CompletionItem {
            label: keyword.to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
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
        ("forever", "( quot -- )", "Execute quotation forever"),
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
            ..Default::default()
        });
    }

    items
}

/// Format a stack effect for display.
fn format_effect(effect: &seqc::Effect) -> String {
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
        Type::Var(name) => name.clone(),
        Type::Quotation(effect) => format!("[ {} ]", format_effect(effect)),
        Type::Closure { effect, .. } => format!("{{ {} }}", format_effect(effect)),
    }
}
