//! LLVM IR Code Generation
//!
//! This module generates LLVM IR as text (.ll files) for Seq programs.
//! The code generation is split into focused submodules for maintainability.
//!
//! # Key Concepts
//!
//! ## Value Representation
//!
//! All Seq values use the `%Value` type, an 8-byte tagged pointer (i64).
//! Int and Bool are encoded inline; heap types (Float, String, Variant, etc.)
//! are stored as Arc<Value> pointers.
//!
//! ## Calling Conventions
//!
//! - **User-defined words**: Use `tailcc` (tail call convention) to enable TCO.
//!   Each word has two functions: a C-convention wrapper (`seq_word_*`) for
//!   external calls and a `tailcc` implementation (`seq_word_*_impl`) for
//!   internal calls that can use `musttail`.
//!
//! - **Runtime functions**: Use C convention (`ccc`). Declared in `runtime.rs`.
//!
//! - **Quotations**: Use C convention. Quotations are first-class functions that
//!   capture their environment. They have wrapper/impl pairs but currently don't
//!   support TCO due to closure complexity.
//!
//! ## Virtual Stack Optimization
//!
//! The top N values (default 4) are kept in SSA virtual registers instead of
//! memory. This avoids store/load overhead for common patterns like `2 3 i.+`.
//! Values are "spilled" to the memory stack at control flow points (if/else,
//! loops) and function calls. See `virtual_stack.rs` and `VirtualValue` in
//! `state.rs`.
//!
//! ## Tail Call Optimization (TCO)
//!
//! Word calls in tail position use LLVM's `musttail` for guaranteed TCO.
//! A call is in tail position when it's the last operation before return.
//! TCO is disabled in these contexts:
//! - Inside `main` (uses C convention for entry point)
//! - Inside quotations (closure semantics require stack frames)
//! - Inside closures that capture variables
//!
//! ## Quotations and Closures
//!
//! Quotations (`[ ... ]`) compile to function pointers pushed onto the stack.
//! - **Pure quotations**: No captured variables, just a function pointer.
//! - **Closures**: Capture variables from enclosing scope. The runtime allocates
//!   a closure struct containing the function pointer and captured values.
//!
//! Each quotation generates a wrapper function (C convention, for `call` builtin)
//! and an impl function. Closure captures are analyzed at compile time by
//! `capture_analysis.rs`.
//!
//! # Module Structure
//!
//! - `state.rs`: Core types (CodeGen, VirtualValue, TailPosition)
//! - `program.rs`: Main entry points (codegen_program*)
//! - `words.rs`: Word and quotation code generation
//! - `statements.rs`: Statement dispatch and main function
//! - `inline/`: Inline operation code generation (no runtime calls)
//!   - `dispatch.rs`: Main inline dispatch logic
//!   - `ops.rs`: Individual inline operations
//!   - `shuffle.rs`: Stack shuffle + pick/roll helpers
//! - `control_flow.rs`: If/else, match statements
//! - `virtual_stack.rs`: Virtual register optimization
//! - `types.rs`: Type helpers and exhaustiveness checking
//! - `globals.rs`: String and symbol constants
//! - `runtime/`: Runtime function declarations (split by category)
//! - `specialization/`: Register-based specialization for primitive-typed words
//! - `ffi_wrappers.rs`: FFI wrapper generation
//! - `platform.rs`: Platform detection
//! - `error.rs`: Error types
//! - `tests.rs`: End-to-end codegen tests (pipeline-level)

// Submodules
mod bitwise_i63;
mod control_flow;
mod debug_info;
mod error;
mod ffi_wrappers;
mod globals;
mod inline;
mod layout;
mod platform;
mod program;
mod runtime;
mod specialization;
mod state;
mod statements;
mod types;
mod virtual_stack;
mod words;

// Public re-exports
pub use error::CodeGenError;
pub use platform::{ffi_c_args, ffi_return_type, get_target_triple};
pub use runtime::{BUILTIN_SYMBOLS, RUNTIME_DECLARATIONS, emit_runtime_decls};
pub use state::CodeGen;

// Internal re-exports for submodules
use state::{
    BranchResult, MAX_VIRTUAL_STACK, QuotationFunctions, QuotationScope, TailPosition,
    UNREACHABLE_PREDECESSOR, VirtualValue, mangle_name,
};

#[cfg(test)]
mod tests;
