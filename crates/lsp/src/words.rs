//! Handler for the `seq/listWords` custom LSP request.
//!
//! Returns every built-in and stdlib word the language exposes, grouped by
//! category, with stack-effect signatures and (where available) documentation.
//! Editor plugins use this to render a "pocket guide" of the language without
//! having to mirror the word list themselves.

use seqc::parser::Parser;
use seqc::stdlib_embed;
use serde::Serialize;

use crate::completion::format_effect;

/// Top-level response: built-in words and stdlib words, each split into groups.
#[derive(Debug, Serialize)]
pub(crate) struct ListWordsResponse {
    pub builtins: Vec<WordGroup>,
    pub stdlib: Vec<WordGroup>,
}

/// A named group of words (e.g. a builtin category or a stdlib module).
#[derive(Debug, Serialize)]
pub(crate) struct WordGroup {
    pub name: String,
    pub words: Vec<WordInfo>,
}

#[derive(Debug, Serialize)]
pub(crate) struct WordInfo {
    pub name: String,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// Build the full word listing.
pub(crate) fn build() -> ListWordsResponse {
    ListWordsResponse {
        builtins: collect_builtins(),
        stdlib: collect_stdlib(),
    }
}

fn collect_builtins() -> Vec<WordGroup> {
    seqc::builtins::builtin_categories()
        .into_iter()
        .map(|(category, names)| {
            let words = names
                .into_iter()
                .map(|name| {
                    let signature = seqc::builtins::builtin_signature(&name)
                        .as_ref()
                        .map(format_effect)
                        .unwrap_or_else(|| "( ? )".to_string());
                    let doc = seqc::builtins::builtin_doc(&name)
                        .map(str::to_string)
                        .filter(|s| !s.is_empty());
                    WordInfo {
                        name,
                        signature,
                        doc,
                    }
                })
                .collect();
            WordGroup {
                name: category.to_string(),
                words,
            }
        })
        .collect()
}

fn collect_stdlib() -> Vec<WordGroup> {
    stdlib_embed::stdlib_module_names()
        .into_iter()
        .filter_map(|module| {
            let source = stdlib_embed::get_stdlib(module)?;
            let mut parser = Parser::new(source);
            let program = parser.parse().ok()?;

            let mut words: Vec<WordInfo> = program
                .words
                .iter()
                .map(|w| {
                    let signature = w
                        .effect
                        .as_ref()
                        .map(format_effect)
                        .unwrap_or_else(|| "( ? )".to_string());
                    WordInfo {
                        name: w.name.clone(),
                        signature,
                        doc: None,
                    }
                })
                .collect();
            words.sort_by(|a, b| a.name.cmp(&b.name));

            Some(WordGroup {
                name: format!("std:{}", module),
                words,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_include_known_categories_and_words() {
        let resp = build();
        let cats: Vec<&str> = resp.builtins.iter().map(|g| g.name.as_str()).collect();
        for expected in ["stack", "arith", "io", "text", "list"] {
            assert!(
                cats.contains(&expected),
                "missing builtin category {expected}"
            );
        }
        let stack = resp
            .builtins
            .iter()
            .find(|g| g.name == "stack")
            .expect("stack category present");
        let names: Vec<&str> = stack.words.iter().map(|w| w.name.as_str()).collect();
        assert!(names.contains(&"dup"), "expected dup in stack: {names:?}");
        for w in &stack.words {
            assert!(
                w.signature.starts_with('('),
                "signature should be a stack effect, got {:?}",
                w.signature
            );
        }
    }

    #[test]
    fn stdlib_groups_are_prefixed_and_nonempty() {
        let resp = build();
        let json = resp
            .stdlib
            .iter()
            .find(|g| g.name == "std:json")
            .expect("std:json group present");
        assert!(!json.words.is_empty(), "std:json has words");
        let names: Vec<&str> = json.words.iter().map(|w| w.name.as_str()).collect();
        assert!(
            names.contains(&"json-parse") || names.contains(&"json-serialize"),
            "expected json-parse/json-serialize in {names:?}"
        );
    }
}
