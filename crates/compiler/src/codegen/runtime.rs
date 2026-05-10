//! Runtime function declarations for LLVM IR.
//!
//! The full set of `declare` statements and Seq-word → C-symbol mappings is
//! split across the sibling `runtime/` sub-modules by category. Each
//! sub-module exposes two slices — `DECLS` and `SYMBOLS` — and this file
//! concatenates them into the public `RUNTIME_DECLARATIONS` and
//! `BUILTIN_SYMBOLS` statics used by the rest of codegen.
//!
//! Adding a new runtime entry point is a two-line edit to the appropriate
//! sub-module: append a `RuntimeDecl { decl, category }` and, if the entry
//! point is callable from Seq, append a `(seq-word, c-symbol)` pair.

mod adt;
mod args_exit;
mod arith;
mod callable;
mod closure;
mod collections;
mod concurrency;
mod float;
mod fs;
mod http;
mod misc;
mod os;
mod stack;
mod stdio;
mod tcp;
mod test_time;
mod text;
mod udp;

use super::error::CodeGenError;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

/// A runtime function declaration for LLVM IR.
pub struct RuntimeDecl {
    /// LLVM declaration string (e.g., "declare ptr @patch_seq_add(ptr)")
    pub decl: &'static str,
    /// Optional category comment (e.g., "; Stack operations")
    pub category: Option<&'static str>,
}

/// All runtime function declarations, assembled in IR-emission order.
pub static RUNTIME_DECLARATIONS: LazyLock<Vec<&'static RuntimeDecl>> = LazyLock::new(|| {
    let slices: &[&[RuntimeDecl]] = &[
        stdio::DECLS,
        arith::DECLS,
        stack::DECLS,
        callable::DECLS,
        closure::DECLS,
        concurrency::DECLS,
        args_exit::DECLS,
        fs::DECLS,
        collections::DECLS,
        tcp::DECLS,
        udp::DECLS,
        http::DECLS,
        os::DECLS,
        text::DECLS,
        adt::DECLS,
        float::DECLS,
        test_time::DECLS,
        misc::DECLS,
    ];
    slices.iter().flat_map(|s| s.iter()).collect()
});

/// Mapping from Seq word names to their C runtime symbol names.
/// This centralizes all the name transformations in one place:
/// - Symbolic operators (=, <, >) map to descriptive names (eq, lt, gt)
/// - Hyphens become underscores for C compatibility
/// - Special characters get escaped (?, +, ->)
/// - Reserved words get suffixes (drop -> drop_op)
pub static BUILTIN_SYMBOLS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let slices: &[&[(&str, &str)]] = &[
        stdio::SYMBOLS,
        args_exit::SYMBOLS,
        arith::SYMBOLS,
        stack::SYMBOLS,
        concurrency::SYMBOLS,
        callable::SYMBOLS,
        closure::SYMBOLS,
        tcp::SYMBOLS,
        udp::SYMBOLS,
        http::SYMBOLS,
        os::SYMBOLS,
        text::SYMBOLS,
        misc::SYMBOLS,
        adt::SYMBOLS,
        fs::SYMBOLS,
        collections::SYMBOLS,
        float::SYMBOLS,
        test_time::SYMBOLS,
    ];
    slices.iter().flat_map(|s| s.iter().copied()).collect()
});

/// Emit all runtime function declarations to the IR string.
pub fn emit_runtime_decls(ir: &mut String) -> Result<(), CodeGenError> {
    for decl in RUNTIME_DECLARATIONS.iter() {
        if let Some(cat) = decl.category {
            writeln!(ir, "{}", cat)?;
        }
        writeln!(ir, "{}", decl.decl)?;
    }
    writeln!(ir)?;
    Ok(())
}
