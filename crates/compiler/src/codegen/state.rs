//! CodeGen State and Core Types
//!
//! This module contains the CodeGen struct definition and core types
//! used across the code generation modules.

use crate::ast::UnionDef;
use crate::ffi::FfiBindings;
use crate::types::Type;
use std::collections::HashMap;

use super::specialization::SpecSignature;

/// Sentinel value for unreachable predecessors in phi nodes.
/// Used when a branch ends with a tail call (which emits ret directly).
pub(super) const UNREACHABLE_PREDECESSOR: &str = "unreachable";

/// Maximum number of values to keep in virtual registers (Issue #189).
/// Values beyond this are spilled to memory.
///
/// Tuned for common patterns:
/// - Binary ops need 2 values (`a b i.+`)
/// - Dup patterns need 3 values (`a dup i.* b i.+`)
/// - Complex expressions may use 4 (`a b i.+ c d i.* i.-`)
///
/// Larger values increase register pressure with diminishing returns,
/// as most operations trigger spills (control flow, function calls, etc.).
pub(super) const MAX_VIRTUAL_STACK: usize = 4;

/// Tracks whether a statement is in tail position.
///
/// A statement is in tail position when its result is directly returned
/// from the function without further processing. For tail calls, we can
/// use LLVM's `musttail` to guarantee tail call optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TailPosition {
    /// This is the last operation before return - can use musttail
    Tail,
    /// More operations follow - use regular call
    NonTail,
}

/// Result of generating code for an if-statement branch.
pub(super) struct BranchResult {
    /// The stack variable after executing the branch
    pub stack_var: String,
    /// Whether the branch emitted a tail call (and thus a ret)
    pub emitted_tail_call: bool,
    /// The predecessor block label for the phi node (or UNREACHABLE_PREDECESSOR)
    pub predecessor: String,
}

/// Mangle a Seq word name into a valid LLVM IR identifier.
///
/// LLVM IR identifiers can contain: letters, digits, underscores, dollars, periods.
/// Seq words can contain: letters, digits, hyphens, question marks, arrows, etc.
///
/// We escape special characters using underscore-based encoding:
/// - `-` (hyphen) -> `_` (hyphens not valid in LLVM IR identifiers)
/// - `?` -> `_Q_` (question)
/// - `>` -> `_GT_` (greater than, for ->)
/// - `<` -> `_LT_` (less than)
/// - `!` -> `_BANG_`
/// - `*` -> `_STAR_`
/// - `/` -> `_SLASH_`
/// - `+` -> `_PLUS_`
/// - `=` -> `_EQ_`
/// - `.` -> `_DOT_`
pub(super) fn mangle_name(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        match c {
            '?' => result.push_str("_Q_"),
            '>' => result.push_str("_GT_"),
            '<' => result.push_str("_LT_"),
            '!' => result.push_str("_BANG_"),
            '*' => result.push_str("_STAR_"),
            '/' => result.push_str("_SLASH_"),
            '+' => result.push_str("_PLUS_"),
            '=' => result.push_str("_EQ_"),
            // Hyphens converted to underscores (hyphens not valid in LLVM IR)
            '-' => result.push('_'),
            // Keep these as-is (valid in LLVM IR)
            '_' | '.' | '$' => result.push(c),
            // Alphanumeric kept as-is
            c if c.is_alphanumeric() => result.push(c),
            // Any other character gets hex-encoded
            _ => result.push_str(&format!("_x{:02X}_", c as u32)),
        }
    }
    result
}

/// Result of generating a quotation: wrapper and impl function names
/// For closures, both names are the same (no TCO support yet)
pub(super) struct QuotationFunctions {
    /// C-convention wrapper function (for runtime calls)
    pub wrapper: String,
    /// tailcc implementation function (for TCO via musttail)
    pub impl_: String,
}

/// A value held in an LLVM virtual register instead of memory (Issue #189).
///
/// This optimization keeps recently-pushed values in SSA variables,
/// avoiding memory stores/loads for common patterns like `2 3 i.+`.
/// Values are spilled to memory at control flow points and function calls.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Float and Bool variants for Phase 2
pub(super) enum VirtualValue {
    /// Integer value in an SSA variable (i64)
    Int {
        ssa_var: String,
        #[allow(dead_code)] // Used for constant folding in Phase 2
        value: i64,
    },
    /// Float value in an SSA variable (double)
    Float { ssa_var: String },
    /// Boolean value in an SSA variable (i64: 0 or 1)
    Bool { ssa_var: String },
}

#[allow(dead_code)] // ssa_var method used in spill_virtual_stack
impl VirtualValue {
    /// Get the SSA variable name
    pub fn ssa_var(&self) -> &str {
        match self {
            VirtualValue::Int { ssa_var, .. } => ssa_var,
            VirtualValue::Float { ssa_var } => ssa_var,
            VirtualValue::Bool { ssa_var } => ssa_var,
        }
    }

    /// Get the discriminant for this value type
    pub fn discriminant(&self) -> i64 {
        match self {
            VirtualValue::Int { .. } => 0,
            VirtualValue::Float { .. } => 1,
            VirtualValue::Bool { .. } => 2,
        }
    }
}

pub struct CodeGen {
    pub(super) output: String,
    pub(super) string_globals: String,
    pub(super) temp_counter: usize,
    pub(super) string_counter: usize,
    pub(super) block_counter: usize, // For generating unique block labels
    pub(super) quot_counter: usize,  // For generating unique quotation function names
    pub(super) string_constants: HashMap<String, String>, // string content -> global name
    pub(super) quotation_functions: String, // Accumulates generated quotation functions
    pub(super) type_map: HashMap<usize, Type>, // Maps quotation ID to inferred type (from typechecker)
    pub(super) external_builtins: HashMap<String, String>, // seq_name -> symbol (for external builtins)
    pub(super) inside_closure: bool, // Track if we're generating code inside a closure (disables TCO)
    pub(super) inside_main: bool, // Track if we're generating code for main (uses C convention, no musttail)
    pub(super) inside_quotation: bool, // Track if we're generating code for a quotation (uses C convention, no musttail)
    pub(super) unions: Vec<UnionDef>,  // Union type definitions for pattern matching
    pub(super) ffi_bindings: FfiBindings, // FFI function bindings
    pub(super) ffi_wrapper_code: String, // Generated FFI wrapper functions
    /// Pure inline test mode: bypasses scheduler, returns top of stack as exit code.
    /// Used for testing pure integer programs without FFI dependencies.
    pub(super) pure_inline_test: bool,
    // Symbol interning for O(1) equality (Issue #166)
    pub(super) symbol_globals: String, // LLVM IR for static symbol globals
    pub(super) symbol_counter: usize,  // Counter for unique symbol names
    pub(super) symbol_constants: HashMap<String, String>, // symbol name -> global name (deduplication)
    /// Per-statement type info for optimization (Issue #186)
    /// Maps (word_name, statement_index) -> top-of-stack type before statement
    pub(super) statement_types: HashMap<(String, usize), Type>,
    /// Current word being compiled (for statement type lookup)
    pub(super) current_word_name: Option<String>,
    /// Current statement index within the word (for statement type lookup)
    pub(super) current_stmt_index: usize,
    /// Nesting depth for type lookup - only depth 0 can use type info
    /// Nested contexts (if/else, loops) increment this to disable lookups
    pub(super) codegen_depth: usize,
    /// True if the previous statement was a trivially-copyable literal (Issue #195)
    /// Used to optimize `dup` after literal push (e.g., `42 dup`)
    pub(super) prev_stmt_is_trivial_literal: bool,
    /// If previous statement was IntLiteral, stores its value (Issue #192)
    /// Used to optimize `roll`/`pick` with constant N (e.g., `2 roll` -> rot)
    pub(super) prev_stmt_int_value: Option<i64>,
    /// Virtual register stack for top N values (Issue #189)
    /// Values here are in SSA variables, not yet written to memory.
    /// The memory stack pointer tracks where memory ends; virtual values are "above" it.
    pub(super) virtual_stack: Vec<VirtualValue>,
    /// Specialized word signatures for register-based codegen
    /// Maps word name -> specialized signature
    pub(super) specialized_words: HashMap<String, SpecSignature>,
}

impl Default for CodeGen {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen {
            output: String::new(),
            string_globals: String::new(),
            temp_counter: 0,
            string_counter: 0,
            block_counter: 0,
            inside_closure: false,
            inside_main: false,
            inside_quotation: false,
            quot_counter: 0,
            string_constants: HashMap::new(),
            quotation_functions: String::new(),
            type_map: HashMap::new(),
            external_builtins: HashMap::new(),
            unions: Vec::new(),
            ffi_bindings: FfiBindings::new(),
            ffi_wrapper_code: String::new(),
            pure_inline_test: false,
            symbol_globals: String::new(),
            symbol_counter: 0,
            symbol_constants: HashMap::new(),
            statement_types: HashMap::new(),
            current_word_name: None,
            current_stmt_index: 0,
            codegen_depth: 0,
            prev_stmt_is_trivial_literal: false,
            prev_stmt_int_value: None,
            virtual_stack: Vec::new(),
            specialized_words: HashMap::new(),
        }
    }

    /// Create a CodeGen for pure inline testing.
    /// Bypasses the scheduler, returning top of stack as exit code.
    /// Only supports operations that are fully inlined (integers, arithmetic, stack ops).
    #[allow(dead_code)]
    pub fn new_pure_inline_test() -> Self {
        let mut cg = Self::new();
        cg.pure_inline_test = true;
        cg
    }
}
