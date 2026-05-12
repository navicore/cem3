//! Built-in word signatures for Seq
//!
//! Defines the stack effects for all runtime built-in operations.
//!
//! The signatures and docs are split across per-category sub-modules under
//! `builtins/`, each exposing an `add_signatures(&mut sigs)` and an
//! `add_docs(&mut docs)` helper. This file composes them into the public
//! `builtin_signatures()` and `builtin_docs()` maps.

use crate::types::Effect;
use std::collections::HashMap;
use std::sync::LazyLock;

mod adt;
mod arith;
mod callable;
mod concurrency;
mod diagnostics;
mod dns;
mod float;
mod fs;
mod http;
mod io;
mod list;
mod macros;
mod map;
mod os;
mod stack;
mod tcp;
mod text;
mod udp;

#[cfg(test)]
mod tests;

type AddSigsFn = fn(&mut HashMap<String, Effect>);
type AddDocsFn = fn(&mut HashMap<&'static str, &'static str>);

/// Single source of truth for the built-in category sub-modules.
///
/// Adding a new sub-module requires exactly one row here; the signature map,
/// the doc map, and `builtin_categories()` all iterate over this list, so
/// they cannot drift apart.
const CATEGORIES: &[(&str, AddSigsFn, AddDocsFn)] = &[
    ("io", io::add_signatures, io::add_docs),
    ("fs", fs::add_signatures, fs::add_docs),
    ("arith", arith::add_signatures, arith::add_docs),
    ("stack", stack::add_signatures, stack::add_docs),
    (
        "concurrency",
        concurrency::add_signatures,
        concurrency::add_docs,
    ),
    ("callable", callable::add_signatures, callable::add_docs),
    ("tcp", tcp::add_signatures, tcp::add_docs),
    ("udp", udp::add_signatures, udp::add_docs),
    ("dns", dns::add_signatures, dns::add_docs),
    ("http", http::add_signatures, http::add_docs),
    ("os", os::add_signatures, os::add_docs),
    ("text", text::add_signatures, text::add_docs),
    ("adt", adt::add_signatures, adt::add_docs),
    ("list", list::add_signatures, list::add_docs),
    ("map", map::add_signatures, map::add_docs),
    ("float", float::add_signatures, float::add_docs),
    (
        "diagnostics",
        diagnostics::add_signatures,
        diagnostics::add_docs,
    ),
];

/// Get the stack-effect signature for a built-in word.
pub fn builtin_signature(name: &str) -> Option<Effect> {
    BUILTIN_SIGNATURES.get(name).cloned()
}

/// Build the full map of built-in word signatures.
///
/// Clones the cached map so callers that wanted ownership (e.g. tests,
/// `TypeChecker::register_external_words`) keep working unchanged.
pub fn builtin_signatures() -> HashMap<String, Effect> {
    BUILTIN_SIGNATURES.clone()
}

static BUILTIN_SIGNATURES: LazyLock<HashMap<String, Effect>> = LazyLock::new(|| {
    let mut sigs = HashMap::new();
    for (_, add_sigs, _) in CATEGORIES {
        add_sigs(&mut sigs);
    }
    sigs
});

/// Get documentation for a built-in word.
pub fn builtin_doc(name: &str) -> Option<&'static str> {
    BUILTIN_DOCS.get(name).copied()
}

/// Built-in words grouped by their category sub-module, in registration order.
///
/// Each entry is `(category_name, sorted_word_names)`. Useful for clients
/// that want to render a categorised reference (e.g. a quick-help screen).
pub fn builtin_categories() -> Vec<(&'static str, Vec<String>)> {
    CATEGORIES
        .iter()
        .map(|(name, add_sigs, _)| {
            let mut sigs = HashMap::new();
            add_sigs(&mut sigs);
            let mut words: Vec<String> = sigs.into_keys().collect();
            words.sort();
            (*name, words)
        })
        .collect()
}

/// Get all built-in word documentation (cached with LazyLock for performance).
pub fn builtin_docs() -> &'static HashMap<&'static str, &'static str> {
    &BUILTIN_DOCS
}

static BUILTIN_DOCS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut docs = HashMap::new();
    for (_, _, add_docs) in CATEGORIES {
        add_docs(&mut docs);
    }
    docs
});
