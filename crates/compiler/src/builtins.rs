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
mod float;
mod fs;
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
    io::add_signatures(&mut sigs);
    fs::add_signatures(&mut sigs);
    arith::add_signatures(&mut sigs);
    stack::add_signatures(&mut sigs);
    concurrency::add_signatures(&mut sigs);
    callable::add_signatures(&mut sigs);
    tcp::add_signatures(&mut sigs);
    udp::add_signatures(&mut sigs);
    os::add_signatures(&mut sigs);
    text::add_signatures(&mut sigs);
    adt::add_signatures(&mut sigs);
    list::add_signatures(&mut sigs);
    map::add_signatures(&mut sigs);
    float::add_signatures(&mut sigs);
    diagnostics::add_signatures(&mut sigs);
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
    type AddFn = fn(&mut HashMap<String, Effect>);
    let categories: &[(&'static str, AddFn)] = &[
        ("io", io::add_signatures),
        ("fs", fs::add_signatures),
        ("arith", arith::add_signatures),
        ("stack", stack::add_signatures),
        ("concurrency", concurrency::add_signatures),
        ("callable", callable::add_signatures),
        ("tcp", tcp::add_signatures),
        ("udp", udp::add_signatures),
        ("os", os::add_signatures),
        ("text", text::add_signatures),
        ("adt", adt::add_signatures),
        ("list", list::add_signatures),
        ("map", map::add_signatures),
        ("float", float::add_signatures),
        ("diagnostics", diagnostics::add_signatures),
    ];
    categories
        .iter()
        .map(|(name, add_fn)| {
            let mut sigs = HashMap::new();
            add_fn(&mut sigs);
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
    io::add_docs(&mut docs);
    fs::add_docs(&mut docs);
    arith::add_docs(&mut docs);
    stack::add_docs(&mut docs);
    concurrency::add_docs(&mut docs);
    callable::add_docs(&mut docs);
    tcp::add_docs(&mut docs);
    udp::add_docs(&mut docs);
    os::add_docs(&mut docs);
    text::add_docs(&mut docs);
    adt::add_docs(&mut docs);
    list::add_docs(&mut docs);
    map::add_docs(&mut docs);
    float::add_docs(&mut docs);
    diagnostics::add_docs(&mut docs);
    docs
});
