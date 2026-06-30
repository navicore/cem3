//! CodeGen State and Core Types
//!
//! This module contains the CodeGen struct definition and core types
//! used across the code generation modules.

use crate::ast::UnionDef;
use crate::ffi::FfiBindings;
use crate::types::Type;
use std::collections::HashMap;
use std::path::PathBuf;

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

/// Snapshot of the enclosing function's mutable codegen state while a nested
/// quotation or closure is being generated. Returned by
/// `enter_quotation_scope` and consumed by `exit_quotation_scope`, which
/// commits the nested IR to `quotation_functions` and restores these fields.
pub(super) struct QuotationScope {
    pub output: String,
    pub virtual_stack: Vec<VirtualValue>,
    pub word_name: Option<String>,
    pub aux_slots: Vec<String>,
    pub aux_sp: usize,
    /// Snapshot of the enclosing word's DISubprogram. Cleared while a
    /// quotation body is being emitted (the quotation lives in its own
    /// LLVM function with no subprogram, so any `!dbg` attached inside
    /// would be unverifiable), and restored when the scope exits.
    pub dbg_subprogram_id: Option<usize>,
}

/// A value held in an LLVM virtual register instead of memory (Issue #189).
///
/// This optimization keeps recently-pushed values in SSA variables,
/// avoiding memory stores/loads for common patterns like `2 3 i.+`.
/// Values are spilled to memory at control flow points and function calls.
#[derive(Clone, Debug)]
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

pub struct CodeGen {
    pub(super) output: String,
    pub(super) string_globals: String,
    pub(super) temp_counter: usize,
    pub(super) string_counter: usize,
    pub(super) block_counter: usize, // For generating unique block labels
    pub(super) quot_counter: usize,  // For generating unique quotation function names
    pub(super) string_constants: HashMap<Vec<u8>, String>, // byte payload -> global name
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
    /// Resolved arithmetic sugar: maps (line, column) -> concrete op name
    /// E.g., `+` at line 5, column 3 -> `"i.+"` if typechecker resolved it for Int operands
    pub(super) resolved_sugar: HashMap<(usize, usize), String>,
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
    /// Per-word aux stack slot counts from typechecker (Issue #350)
    /// Maps word_name -> number of %Value allocas needed
    pub(super) aux_slot_counts: HashMap<String, usize>,
    /// Per-quotation aux stack slot counts from typechecker (Issue #393)
    /// Maps quotation_id -> number of %Value allocas needed for that quotation
    pub(super) quotation_aux_slot_counts: HashMap<usize, usize>,
    /// LLVM alloca names for current word's aux slots (Issue #350)
    pub(super) current_aux_slots: Vec<String>,
    /// Compile-time index into aux slots (Issue #350)
    pub(super) current_aux_sp: usize,
    /// Whether to emit per-word atomic call counters (--instrument)
    pub(super) instrument: bool,
    /// Enable loop lowering for self-tail-recursive words (--loop-opt).
    /// See `docs/design/LOOP_LOWERING.md`.
    pub(super) loop_opt_enabled: bool,
    /// Iterations between cooperative yields inside a lowered loop.
    /// Must be a power of two (used as an AND mask). Default 1024.
    pub(super) loop_yield_cadence: u32,
    /// Call graph built from the program, used by loop lowering to classify
    /// self-recursive words. `None` disables loop lowering regardless of
    /// `loop_opt_enabled`.
    pub(super) call_graph: Option<crate::call_graph::CallGraph>,
    /// True if the user's `main` word has effect `( -- Int )`.
    /// Determines whether `seq_main` writes the top-of-stack int to the
    /// global exit code before freeing the stack. (Issue #355)
    pub(super) main_returns_int: bool,
    /// Maps word name -> sequential ID for instrumentation counters
    pub(super) word_instrument_ids: HashMap<String, usize>,
    // -------------------------------------------------------------------
    // Debug info (DWARF) — see codegen/debug_info.rs.
    //
    // When enabled, emits LLVM `!DICompileUnit`, `!DIFile`, `!DISubprogram`,
    // and per-call `!DILocation` metadata so panics in the runtime resolve
    // back to .seq source lines via the standard Rust backtrace path.
    // Zero runtime cost — pure metadata. The clang invocation must pass
    // `-g` to preserve these into the final binary's DWARF section.
    // -------------------------------------------------------------------
    /// Source file the program was compiled from (for DIFile). When
    /// `None`, debug info is disabled.
    pub(super) dbg_source: Option<PathBuf>,
    /// Accumulated DWARF metadata definitions (`!N = !DI...`). Appended to
    /// the end of the IR file alongside the module flags.
    pub(super) dbg_metadata: String,
    /// Counter for unique metadata IDs. Started at 1000 to leave headroom
    /// for any future module-level metadata that may want lower ids.
    pub(super) dbg_md_counter: usize,
    /// ID of the per-program `!DICompileUnit`. Set during program prologue
    /// when debug info is enabled.
    pub(super) dbg_cu_id: Option<usize>,
    /// ID of the shared `!DIFile` for the source file.
    pub(super) dbg_file_id: Option<usize>,
    /// ID of the shared `!DISubroutineType` reused by every subprogram —
    /// our generated functions all have the same opaque ptr-in/ptr-out
    /// signature from a debugger's point of view.
    pub(super) dbg_subroutine_type_id: Option<usize>,
    /// `!DISubprogram` ID for the function currently being emitted, if any.
    /// Call sites use this as the scope for their `!DILocation` records.
    pub(super) current_dbg_subprogram_id: Option<usize>,
    /// IDs of the two `!llvm.module.flags` records ("Dwarf Version",
    /// "Debug Info Version"). Allocated through `dbg_alloc_id` at program
    /// init so they never collide with later subprogram/location records.
    pub(super) dbg_module_flag_ids: Option<(usize, usize)>,
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
            resolved_sugar: HashMap::new(),
            current_word_name: None,
            current_stmt_index: 0,
            codegen_depth: 0,
            prev_stmt_is_trivial_literal: false,
            prev_stmt_int_value: None,
            virtual_stack: Vec::new(),
            specialized_words: HashMap::new(),
            aux_slot_counts: HashMap::new(),
            quotation_aux_slot_counts: HashMap::new(),
            current_aux_slots: Vec::new(),
            current_aux_sp: 0,
            instrument: false,
            loop_opt_enabled: false,
            loop_yield_cadence: 1024,
            call_graph: None,
            word_instrument_ids: HashMap::new(),
            main_returns_int: false,
            dbg_source: None,
            dbg_metadata: String::new(),
            dbg_md_counter: 1000,
            dbg_cu_id: None,
            dbg_file_id: None,
            dbg_subroutine_type_id: None,
            current_dbg_subprogram_id: None,
            dbg_module_flag_ids: None,
        }
    }

    /// Enable DWARF debug info generation, anchored at the given source file.
    ///
    /// Must be called before `codegen_program*`. With debug info enabled,
    /// every user-defined word gets a `!DISubprogram` and every call site
    /// with a span gets a `!DILocation` — so a runtime panic backtrace
    /// resolves the Seq frame to `.seq:line:col`. Zero runtime overhead.
    pub fn set_source_file(&mut self, path: PathBuf) {
        self.dbg_source = Some(path);
    }

    /// Create a CodeGen for pure inline testing.
    /// Bypasses the scheduler, returning top of stack as exit code.
    /// Only supports operations that are fully inlined (integers, arithmetic, stack ops).
    pub fn new_pure_inline_test() -> Self {
        let mut cg = Self::new();
        cg.pure_inline_test = true;
        cg
    }

    /// Set per-word aux stack slot counts from typechecker (Issue #350)
    pub fn set_aux_slot_counts(&mut self, counts: HashMap<String, usize>) {
        self.aux_slot_counts = counts;
    }

    /// Set per-quotation aux stack slot counts from typechecker (Issue #393)
    pub fn set_quotation_aux_slot_counts(&mut self, counts: HashMap<usize, usize>) {
        self.quotation_aux_slot_counts = counts;
    }

    /// Set resolved arithmetic sugar mappings from the typechecker
    pub fn set_resolved_sugar(&mut self, sugar: HashMap<(usize, usize), String>) {
        self.resolved_sugar = sugar;
    }

    /// Look up the resolved name for an arithmetic sugar op by source location
    pub(super) fn resolve_sugar_at(&self, line: usize, column: usize) -> Option<&str> {
        self.resolved_sugar.get(&(line, column)).map(|s| s.as_str())
    }

    /// Enable loop lowering and set the yield cadence (power of two).
    pub fn set_loop_opt(&mut self, enabled: bool, cadence: u32) {
        self.loop_opt_enabled = enabled;
        self.loop_yield_cadence = cadence;
    }

    /// Provide the program call graph for self-recursion classification.
    pub fn set_call_graph(&mut self, cg: crate::call_graph::CallGraph) {
        self.call_graph = Some(cg);
    }
}
