//! LLVM IR Code Generation via Text
//!
//! Generates LLVM IR as text (.ll files) and invokes clang to produce executables.
//! This approach is simpler and more portable than using FFI bindings (inkwell).
//!
//! # Code Generation Strategy
//!
//! Stack is threaded through all operations as a pointer:
//! 1. Start with null stack pointer
//! 2. Each operation takes stack, returns new stack
//! 3. Final stack is ignored (should be null for well-typed programs)
//!
//! # Runtime Function Declarations
//!
//! All runtime functions follow the pattern:
//! - `define ptr @name(ptr %stack) { ... }` for stack operations
//! - `define ptr @push_int(ptr %stack, i64 %value) { ... }` for literals
//! - Stack type is represented as `ptr` (opaque pointer in modern LLVM)

use crate::ast::{MatchArm, Pattern, Program, Statement, UnionDef, WordDef};
use crate::config::CompilerConfig;
use crate::ffi::{FfiBindings, FfiType, Ownership, PassMode};
use crate::types::Type;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

/// Error type for code generation operations.
///
/// This allows proper error propagation using `?` for both logical errors
/// (invalid programs, missing definitions) and formatting errors (write failures).
#[derive(Debug)]
pub enum CodeGenError {
    /// A logical error in code generation (e.g., missing word definition)
    Logic(String),
    /// A formatting error when writing IR
    Format(std::fmt::Error),
}

impl std::fmt::Display for CodeGenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodeGenError::Logic(s) => write!(f, "{}", s),
            CodeGenError::Format(e) => write!(f, "IR generation error: {}", e),
        }
    }
}

impl std::error::Error for CodeGenError {}

impl From<String> for CodeGenError {
    fn from(s: String) -> Self {
        CodeGenError::Logic(s)
    }
}

impl From<std::fmt::Error> for CodeGenError {
    fn from(e: std::fmt::Error) -> Self {
        CodeGenError::Format(e)
    }
}

/// Sentinel value for unreachable predecessors in phi nodes.
/// Used when a branch ends with a tail call (which emits ret directly).
const UNREACHABLE_PREDECESSOR: &str = "unreachable";

/// Mapping from Seq word names to their C runtime symbol names.
/// This centralizes all the name transformations in one place:
/// - Symbolic operators (=, <, >) map to descriptive names (eq, lt, gt)
/// - Hyphens become underscores for C compatibility
/// - Special characters get escaped (?, +, ->)
/// - Reserved words get suffixes (drop -> drop_op)
static BUILTIN_SYMBOLS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        // I/O operations
        ("io.write", "patch_seq_write"),
        ("io.write-line", "patch_seq_write_line"),
        ("io.read-line", "patch_seq_read_line"),
        ("io.read-line+", "patch_seq_read_line_plus"),
        ("io.read-n", "patch_seq_read_n"),
        ("int->string", "patch_seq_int_to_string"),
        ("symbol->string", "patch_seq_symbol_to_string"),
        ("string->symbol", "patch_seq_string_to_symbol"),
        // Command-line arguments
        ("args.count", "patch_seq_arg_count"),
        ("args.at", "patch_seq_arg_at"),
        // Integer Arithmetic
        ("i.add", "patch_seq_add"),
        ("i.subtract", "patch_seq_subtract"),
        ("i.multiply", "patch_seq_multiply"),
        ("i.divide", "patch_seq_divide"),
        // Terse integer arithmetic aliases
        ("i.+", "patch_seq_add"),
        ("i.-", "patch_seq_subtract"),
        ("i.*", "patch_seq_multiply"),
        ("i./", "patch_seq_divide"),
        // Note: i.% (modulo) is fully inlined, no runtime function needed
        // Integer comparison (symbol form)
        ("i.=", "patch_seq_eq"),
        ("i.<", "patch_seq_lt"),
        ("i.>", "patch_seq_gt"),
        ("i.<=", "patch_seq_lte"),
        ("i.>=", "patch_seq_gte"),
        ("i.<>", "patch_seq_neq"),
        // Integer comparison (verbose form)
        ("i.eq", "patch_seq_eq"),
        ("i.lt", "patch_seq_lt"),
        ("i.gt", "patch_seq_gt"),
        ("i.lte", "patch_seq_lte"),
        ("i.gte", "patch_seq_gte"),
        ("i.neq", "patch_seq_neq"),
        // Boolean
        ("and", "patch_seq_and"),
        ("or", "patch_seq_or"),
        ("not", "patch_seq_not"),
        // Bitwise
        ("band", "patch_seq_band"),
        ("bor", "patch_seq_bor"),
        ("bxor", "patch_seq_bxor"),
        ("bnot", "patch_seq_bnot"),
        ("shl", "patch_seq_shl"),
        ("shr", "patch_seq_shr"),
        ("popcount", "patch_seq_popcount"),
        ("clz", "patch_seq_clz"),
        ("ctz", "patch_seq_ctz"),
        ("int-bits", "patch_seq_int_bits"),
        // Stack operations
        ("dup", "patch_seq_dup"),
        ("swap", "patch_seq_swap"),
        ("over", "patch_seq_over"),
        ("rot", "patch_seq_rot"),
        ("nip", "patch_seq_nip"),
        ("tuck", "patch_seq_tuck"),
        ("2dup", "patch_seq_2dup"),
        ("3drop", "patch_seq_3drop"),
        ("drop", "patch_seq_drop_op"),
        ("pick", "patch_seq_pick_op"),
        ("roll", "patch_seq_roll"),
        // Channel operations (errors are values, not crashes)
        ("chan.make", "patch_seq_make_channel"),
        ("chan.send", "patch_seq_chan_send"),
        ("chan.receive", "patch_seq_chan_receive"),
        ("chan.close", "patch_seq_close_channel"),
        ("chan.yield", "patch_seq_yield_strand"),
        // Quotation operations
        ("call", "patch_seq_call"),
        ("times", "patch_seq_times"),
        ("while", "patch_seq_while_loop"),
        ("until", "patch_seq_until_loop"),
        ("strand.spawn", "patch_seq_spawn"),
        ("strand.weave", "patch_seq_weave"),
        ("strand.resume", "patch_seq_resume"),
        ("strand.weave-cancel", "patch_seq_weave_cancel"),
        ("yield", "patch_seq_yield"),
        ("cond", "patch_seq_cond"),
        // TCP operations
        ("tcp.listen", "patch_seq_tcp_listen"),
        ("tcp.accept", "patch_seq_tcp_accept"),
        ("tcp.read", "patch_seq_tcp_read"),
        ("tcp.write", "patch_seq_tcp_write"),
        ("tcp.close", "patch_seq_tcp_close"),
        // OS operations
        ("os.getenv", "patch_seq_getenv"),
        ("os.home-dir", "patch_seq_home_dir"),
        ("os.current-dir", "patch_seq_current_dir"),
        ("os.path-exists", "patch_seq_path_exists"),
        ("os.path-is-file", "patch_seq_path_is_file"),
        ("os.path-is-dir", "patch_seq_path_is_dir"),
        ("os.path-join", "patch_seq_path_join"),
        ("os.path-parent", "patch_seq_path_parent"),
        ("os.path-filename", "patch_seq_path_filename"),
        ("os.exit", "patch_seq_exit"),
        ("os.name", "patch_seq_os_name"),
        ("os.arch", "patch_seq_os_arch"),
        // String operations
        ("string.concat", "patch_seq_string_concat"),
        ("string.length", "patch_seq_string_length"),
        ("string.byte-length", "patch_seq_string_byte_length"),
        ("string.char-at", "patch_seq_string_char_at"),
        ("string.substring", "patch_seq_string_substring"),
        ("char->string", "patch_seq_char_to_string"),
        ("string.find", "patch_seq_string_find"),
        ("string.split", "patch_seq_string_split"),
        ("string.contains", "patch_seq_string_contains"),
        ("string.starts-with", "patch_seq_string_starts_with"),
        ("string.empty?", "patch_seq_string_empty"),
        ("string.trim", "patch_seq_string_trim"),
        ("string.chomp", "patch_seq_string_chomp"),
        ("string.to-upper", "patch_seq_string_to_upper"),
        ("string.to-lower", "patch_seq_string_to_lower"),
        ("string.equal?", "patch_seq_string_equal"),
        ("string.json-escape", "patch_seq_json_escape"),
        ("string->int", "patch_seq_string_to_int"),
        // Symbol operations
        ("symbol.=", "patch_seq_symbol_equal"),
        // File operations
        ("file.slurp", "patch_seq_file_slurp"),
        ("file.exists?", "patch_seq_file_exists"),
        ("file.for-each-line+", "patch_seq_file_for_each_line_plus"),
        // List operations
        ("list.map", "patch_seq_list_map"),
        ("list.filter", "patch_seq_list_filter"),
        ("list.fold", "patch_seq_list_fold"),
        ("list.each", "patch_seq_list_each"),
        ("list.length", "patch_seq_list_length"),
        ("list.empty?", "patch_seq_list_empty"),
        // Map operations
        ("map.make", "patch_seq_make_map"),
        ("map.get", "patch_seq_map_get"),
        ("map.set", "patch_seq_map_set"),
        ("map.has?", "patch_seq_map_has"),
        ("map.remove", "patch_seq_map_remove"),
        ("map.keys", "patch_seq_map_keys"),
        ("map.values", "patch_seq_map_values"),
        ("map.size", "patch_seq_map_size"),
        ("map.empty?", "patch_seq_map_empty"),
        // Variant operations
        ("variant.field-count", "patch_seq_variant_field_count"),
        ("variant.tag", "patch_seq_variant_tag"),
        ("variant.field-at", "patch_seq_variant_field_at"),
        ("variant.append", "patch_seq_variant_append"),
        ("variant.last", "patch_seq_variant_last"),
        ("variant.init", "patch_seq_variant_init"),
        ("variant.make-0", "patch_seq_make_variant_0"),
        ("variant.make-1", "patch_seq_make_variant_1"),
        ("variant.make-2", "patch_seq_make_variant_2"),
        ("variant.make-3", "patch_seq_make_variant_3"),
        ("variant.make-4", "patch_seq_make_variant_4"),
        // wrap-N aliases for dynamic variant construction (SON)
        ("wrap-0", "patch_seq_make_variant_0"),
        ("wrap-1", "patch_seq_make_variant_1"),
        ("wrap-2", "patch_seq_make_variant_2"),
        ("wrap-3", "patch_seq_make_variant_3"),
        ("wrap-4", "patch_seq_make_variant_4"),
        // Float arithmetic
        ("f.add", "patch_seq_f_add"),
        ("f.subtract", "patch_seq_f_subtract"),
        ("f.multiply", "patch_seq_f_multiply"),
        ("f.divide", "patch_seq_f_divide"),
        // Terse float arithmetic aliases
        ("f.+", "patch_seq_f_add"),
        ("f.-", "patch_seq_f_subtract"),
        ("f.*", "patch_seq_f_multiply"),
        ("f./", "patch_seq_f_divide"),
        // Float comparison (symbol form)
        ("f.=", "patch_seq_f_eq"),
        ("f.<", "patch_seq_f_lt"),
        ("f.>", "patch_seq_f_gt"),
        ("f.<=", "patch_seq_f_lte"),
        ("f.>=", "patch_seq_f_gte"),
        ("f.<>", "patch_seq_f_neq"),
        // Float comparison (verbose form)
        ("f.eq", "patch_seq_f_eq"),
        ("f.lt", "patch_seq_f_lt"),
        ("f.gt", "patch_seq_f_gt"),
        ("f.lte", "patch_seq_f_lte"),
        ("f.gte", "patch_seq_f_gte"),
        ("f.neq", "patch_seq_f_neq"),
        // Float type conversions
        ("int->float", "patch_seq_int_to_float"),
        ("float->int", "patch_seq_float_to_int"),
        ("float->string", "patch_seq_float_to_string"),
        ("string->float", "patch_seq_string_to_float"),
        // Test framework operations
        ("test.init", "patch_seq_test_init"),
        ("test.finish", "patch_seq_test_finish"),
        ("test.has-failures", "patch_seq_test_has_failures"),
        ("test.assert", "patch_seq_test_assert"),
        ("test.assert-not", "patch_seq_test_assert_not"),
        ("test.assert-eq", "patch_seq_test_assert_eq"),
        ("test.assert-eq-str", "patch_seq_test_assert_eq_str"),
        ("test.fail", "patch_seq_test_fail"),
        ("test.pass-count", "patch_seq_test_pass_count"),
        ("test.fail-count", "patch_seq_test_fail_count"),
        // Time operations
        ("time.now", "patch_seq_time_now"),
        ("time.nanos", "patch_seq_time_nanos"),
        ("time.sleep-ms", "patch_seq_time_sleep_ms"),
        // Stack introspection
        ("stack.dump", "patch_seq_stack_dump"),
    ])
});

/// Tracks whether a statement is in tail position.
///
/// A statement is in tail position when its result is directly returned
/// from the function without further processing. For tail calls, we can
/// use LLVM's `musttail` to guarantee tail call optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TailPosition {
    /// This is the last operation before return - can use musttail
    Tail,
    /// More operations follow - use regular call
    NonTail,
}

/// Result of generating code for an if-statement branch.
struct BranchResult {
    /// The stack variable after executing the branch
    stack_var: String,
    /// Whether the branch emitted a tail call (and thus a ret)
    emitted_tail_call: bool,
    /// The predecessor block label for the phi node (or UNREACHABLE_PREDECESSOR)
    predecessor: String,
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
fn mangle_name(name: &str) -> String {
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
struct QuotationFunctions {
    /// C-convention wrapper function (for runtime calls)
    wrapper: String,
    /// tailcc implementation function (for TCO via musttail)
    impl_: String,
}

pub struct CodeGen {
    output: String,
    string_globals: String,
    temp_counter: usize,
    string_counter: usize,
    block_counter: usize, // For generating unique block labels
    quot_counter: usize,  // For generating unique quotation function names
    string_constants: HashMap<String, String>, // string content -> global name
    quotation_functions: String, // Accumulates generated quotation functions
    type_map: HashMap<usize, Type>, // Maps quotation ID to inferred type (from typechecker)
    external_builtins: HashMap<String, String>, // seq_name -> symbol (for external builtins)
    inside_closure: bool, // Track if we're generating code inside a closure (disables TCO)
    inside_main: bool, // Track if we're generating code for main (uses C convention, no musttail)
    inside_quotation: bool, // Track if we're generating code for a quotation (uses C convention, no musttail)
    unions: Vec<UnionDef>,  // Union type definitions for pattern matching
    ffi_bindings: FfiBindings, // FFI function bindings
    ffi_wrapper_code: String, // Generated FFI wrapper functions
    /// Pure inline test mode: bypasses scheduler, returns top of stack as exit code.
    /// Used for testing pure integer programs without FFI dependencies.
    pure_inline_test: bool,
    // Symbol interning for O(1) equality (Issue #166)
    symbol_globals: String, // LLVM IR for static symbol globals
    symbol_counter: usize,  // Counter for unique symbol names
    symbol_constants: HashMap<String, String>, // symbol name -> global name (deduplication)
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

    /// Generate a fresh temporary variable name
    fn fresh_temp(&mut self) -> String {
        let name = format!("{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    /// Generate a fresh block label
    fn fresh_block(&mut self, prefix: &str) -> String {
        let name = format!("{}{}", prefix, self.block_counter);
        self.block_counter += 1;
        name
    }

    /// Get the next quotation type (consumes it in DFS traversal order)
    /// Get the inferred type for a quotation by its ID
    fn get_quotation_type(&self, id: usize) -> Result<&Type, CodeGenError> {
        self.type_map.get(&id).ok_or_else(|| {
            CodeGenError::Logic(format!(
                "CodeGen: no type information for quotation ID {}. This is a compiler bug.",
                id
            ))
        })
    }

    /// Find variant info by name across all unions
    ///
    /// Returns (tag_index, field_count) for the variant
    /// Returns (tag_index, field_count, field_names)
    fn find_variant_info(
        &self,
        variant_name: &str,
    ) -> Result<(usize, usize, Vec<String>), CodeGenError> {
        for union_def in &self.unions {
            for (tag_idx, variant) in union_def.variants.iter().enumerate() {
                if variant.name == variant_name {
                    let field_names: Vec<String> =
                        variant.fields.iter().map(|f| f.name.clone()).collect();
                    return Ok((tag_idx, variant.fields.len(), field_names));
                }
            }
        }
        Err(CodeGenError::Logic(format!(
            "Unknown variant '{}' in match pattern. No union defines this variant.",
            variant_name
        )))
    }

    /// Find the union that contains a given variant
    ///
    /// Returns the UnionDef reference if found
    fn find_union_for_variant(&self, variant_name: &str) -> Option<&UnionDef> {
        for union_def in &self.unions {
            for variant in &union_def.variants {
                if variant.name == variant_name {
                    return Some(union_def);
                }
            }
        }
        None
    }

    /// Check if a match expression is exhaustive for its union type
    ///
    /// Returns Ok(()) if exhaustive, Err with missing variants if not
    fn check_match_exhaustiveness(&self, arms: &[MatchArm]) -> Result<(), (String, Vec<String>)> {
        if arms.is_empty() {
            return Ok(()); // Empty match is degenerate, skip check
        }

        // Get the first variant name to find the union
        let first_variant = match &arms[0].pattern {
            Pattern::Variant(name) => name.as_str(),
            Pattern::VariantWithBindings { name, .. } => name.as_str(),
        };

        // Find the union this variant belongs to
        let union_def = match self.find_union_for_variant(first_variant) {
            Some(u) => u,
            None => return Ok(()), // Unknown variant, let find_variant_info handle error
        };

        // Collect all variant names in the match arms
        let matched_variants: std::collections::HashSet<&str> = arms
            .iter()
            .map(|arm| match &arm.pattern {
                Pattern::Variant(name) => name.as_str(),
                Pattern::VariantWithBindings { name, .. } => name.as_str(),
            })
            .collect();

        // Check if all union variants are covered
        let missing: Vec<String> = union_def
            .variants
            .iter()
            .filter(|v| !matched_variants.contains(v.name.as_str()))
            .map(|v| v.name.clone())
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err((union_def.name.clone(), missing))
        }
    }

    /// Escape a string for LLVM IR string literals
    fn escape_llvm_string(s: &str) -> Result<String, std::fmt::Error> {
        let mut result = String::new();
        for ch in s.chars() {
            match ch {
                ' '..='!' | '#'..='[' | ']'..='~' => result.push(ch),
                '\\' => result.push_str(r"\\"),
                '"' => result.push_str(r#"\22"#),
                '\n' => result.push_str(r"\0A"),
                '\r' => result.push_str(r"\0D"),
                '\t' => result.push_str(r"\09"),
                _ => {
                    // Non-printable: use hex escape
                    for byte in ch.to_string().as_bytes() {
                        write!(&mut result, r"\{:02X}", byte)?;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Get or create a global string constant
    fn get_string_global(&mut self, s: &str) -> Result<String, CodeGenError> {
        if let Some(global_name) = self.string_constants.get(s) {
            return Ok(global_name.clone());
        }

        let global_name = format!("@.str.{}", self.string_counter);
        self.string_counter += 1;

        let escaped = Self::escape_llvm_string(s)?;
        let len = s.len() + 1; // +1 for null terminator

        writeln!(
            &mut self.string_globals,
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            global_name, len, escaped
        )?;

        self.string_constants
            .insert(s.to_string(), global_name.clone());
        Ok(global_name)
    }

    /// Get or create a global interned symbol constant (Issue #166)
    ///
    /// Creates a static SeqString structure with capacity=0 to mark it as interned.
    /// This enables O(1) symbol equality via pointer comparison.
    fn get_symbol_global(&mut self, symbol_name: &str) -> Result<String, CodeGenError> {
        // Deduplicate: return existing global if we've seen this symbol
        if let Some(global_name) = self.symbol_constants.get(symbol_name) {
            return Ok(global_name.clone());
        }

        // Get or create the underlying string data
        let str_global = self.get_string_global(symbol_name)?;

        // Create the SeqString structure global
        let sym_global = format!("@.sym.{}", self.symbol_counter);
        self.symbol_counter += 1;

        // SeqString layout: { ptr, i64 len, i64 capacity, i8 global }
        // capacity=0 marks this as an interned symbol (never freed)
        // global=1 marks it as static data
        writeln!(
            &mut self.symbol_globals,
            "{} = private unnamed_addr constant {{ ptr, i64, i64, i8 }} {{ ptr {}, i64 {}, i64 0, i8 1 }}",
            sym_global,
            str_global,
            symbol_name.len()
        )?;

        self.symbol_constants
            .insert(symbol_name.to_string(), sym_global.clone());
        Ok(sym_global)
    }

    /// Generate LLVM IR for entire program
    pub fn codegen_program(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
    ) -> Result<String, CodeGenError> {
        self.codegen_program_with_config(program, type_map, &CompilerConfig::default())
    }

    /// Generate LLVM IR for entire program with custom configuration
    ///
    /// This allows external projects to extend the compiler with additional
    /// builtins that will be declared and callable from Seq code.
    pub fn codegen_program_with_config(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        config: &CompilerConfig,
    ) -> Result<String, CodeGenError> {
        // Store type map for use during code generation
        self.type_map = type_map;

        // Store union definitions for pattern matching
        self.unions = program.unions.clone();

        // Build external builtins map from config
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        // Verify we have a main word
        if program.find_word("main").is_none() {
            return Err(CodeGenError::Logic("No main word defined".to_string()));
        }

        // Generate all user-defined words
        for word in &program.words {
            self.codegen_word(word)?;
        }

        // Generate main function
        self.codegen_main()?;

        // Assemble final IR
        let mut ir = String::new();

        // Target and type declarations
        writeln!(&mut ir, "; ModuleID = 'main'")?;
        writeln!(&mut ir, "target triple = \"{}\"", get_target_triple())?;
        writeln!(&mut ir)?;

        // Value type (Rust enum with #[repr(C)], 40 bytes: discriminant + largest variant payload)
        // We define concrete size so LLVM can pass by value (required for Alpine/musl)
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

        // Runtime function declarations
        writeln!(&mut ir, "; Runtime function declarations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_int(ptr, i64)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_string(ptr, ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_symbol(ptr, ptr)")?;
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_push_interned_symbol(ptr, ptr)"
        )?;
        writeln!(&mut ir, "declare ptr @patch_seq_write(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_write_line(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_read_line(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_read_line_plus(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_read_n(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_int_to_string(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_symbol_to_string(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_symbol(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_add(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_subtract(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_multiply(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_divide(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_eq(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_lt(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_gt(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_lte(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_gte(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_neq(ptr)")?;
        writeln!(&mut ir, "; Boolean operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_and(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_or(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_not(ptr)")?;
        writeln!(&mut ir, "; Bitwise operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_band(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_bor(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_bxor(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_bnot(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_shl(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_shr(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_popcount(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_clz(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_ctz(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_int_bits(ptr)")?;
        // LLVM intrinsics for bit counting (used by inline codegen)
        writeln!(&mut ir, "declare i64 @llvm.ctpop.i64(i64)")?;
        writeln!(&mut ir, "declare i64 @llvm.ctlz.i64(i64, i1)")?;
        writeln!(&mut ir, "declare i64 @llvm.cttz.i64(i64, i1)")?;
        // LLVM intrinsic for memmove (used by roll)
        writeln!(
            &mut ir,
            "declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)"
        )?;
        // LLVM intrinsic for trap (used by division-by-zero check)
        writeln!(&mut ir, "declare void @llvm.trap() noreturn nounwind")?;
        writeln!(&mut ir, "; Stack operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_dup(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_drop_op(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_swap(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_over(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_rot(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_nip(ptr)")?;
        writeln!(&mut ir, "declare void @patch_seq_clone_value(ptr, ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tuck(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_2dup(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_3drop(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_pick_op(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_roll(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_value(ptr, %Value)")?;
        writeln!(&mut ir, "; Quotation operations")?;
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_push_quotation(ptr, i64, i64)"
        )?;
        writeln!(&mut ir, "declare ptr @patch_seq_call(ptr)")?;
        // Phase 2 TCO helpers for quotation calls
        writeln!(&mut ir, "declare i64 @patch_seq_peek_is_quotation(ptr)")?;
        writeln!(&mut ir, "declare i64 @patch_seq_peek_quotation_fn_ptr(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_times(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_while_loop(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_until_loop(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_spawn(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_weave(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_resume(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_weave_cancel(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_yield(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_cond(ptr)")?;
        writeln!(&mut ir, "; Closure operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_create_env(i32)")?;
        writeln!(&mut ir, "declare void @patch_seq_env_set(ptr, i32, %Value)")?;
        writeln!(&mut ir, "declare %Value @patch_seq_env_get(ptr, i64, i32)")?;
        writeln!(&mut ir, "declare i64 @patch_seq_env_get_int(ptr, i64, i32)")?;
        writeln!(
            &mut ir,
            "declare i64 @patch_seq_env_get_bool(ptr, i64, i32)"
        )?;
        writeln!(
            &mut ir,
            "declare double @patch_seq_env_get_float(ptr, i64, i32)"
        )?;
        writeln!(
            &mut ir,
            "declare i64 @patch_seq_env_get_quotation(ptr, i64, i32)"
        )?;
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_env_get_string(ptr, i64, i32)"
        )?;
        // Combined get+push for strings to avoid returning SeqString by value through FFI
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_env_push_string(ptr, ptr, i64, i32)"
        )?;
        writeln!(&mut ir, "declare %Value @patch_seq_make_closure(i64, ptr)")?;
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_push_closure(ptr, i64, i32)"
        )?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_seqstring(ptr, ptr)")?;
        writeln!(&mut ir, "; Concurrency operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_channel(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_chan_send(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_chan_receive(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_close_channel(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_yield_strand(ptr)")?;
        writeln!(&mut ir, "declare void @patch_seq_maybe_yield()")?;
        writeln!(&mut ir, "; Scheduler operations")?;
        writeln!(&mut ir, "declare void @patch_seq_scheduler_init()")?;
        writeln!(&mut ir, "declare ptr @patch_seq_scheduler_run()")?;
        writeln!(&mut ir, "declare i64 @patch_seq_strand_spawn(ptr, ptr)")?;
        writeln!(&mut ir, "; Command-line argument operations")?;
        writeln!(&mut ir, "declare void @patch_seq_args_init(i32, ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_arg_count(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_arg_at(ptr)")?;
        writeln!(&mut ir, "; File operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_file_slurp(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_file_exists(ptr)")?;
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_file_for_each_line_plus(ptr)"
        )?;
        writeln!(&mut ir, "; List operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_map(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_filter(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_fold(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_each(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_length(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_list_empty(ptr)")?;
        writeln!(&mut ir, "; Map operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_map(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_get(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_set(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_has(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_remove(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_keys(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_values(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_size(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_map_empty(ptr)")?;
        writeln!(&mut ir, "; TCP operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_listen(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_accept(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_read(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_write(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_close(ptr)")?;
        writeln!(&mut ir, "; OS operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_getenv(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_home_dir(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_current_dir(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_exists(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_is_file(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_is_dir(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_join(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_parent(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_path_filename(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_exit(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_os_name(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_os_arch(ptr)")?;
        writeln!(&mut ir, "; String operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_concat(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_length(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_byte_length(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_char_at(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_substring(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_char_to_string(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_find(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_split(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_contains(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_starts_with(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_empty(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_trim(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_chomp(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_upper(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_lower(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_equal(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_json_escape(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_int(ptr)")?;
        writeln!(&mut ir, "; Symbol operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_symbol_equal(ptr)")?;
        writeln!(&mut ir, "; Variant operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_field_count(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_tag(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_field_at(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_append(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_last(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_variant_init(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_0(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_1(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_2(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_3(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_4(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_unpack_variant(ptr, i64)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_symbol_eq_cstr(ptr, ptr)")?;
        writeln!(&mut ir, "; Float operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_push_float(ptr, double)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_add(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_subtract(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_multiply(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_divide(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_eq(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_lt(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_gt(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_lte(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_gte(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_f_neq(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_int_to_float(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_float_to_int(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_float_to_string(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_float(ptr)")?;
        writeln!(&mut ir, "; Test framework operations")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_init(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_finish(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_has_failures(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_assert(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_assert_not(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_assert_eq(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_assert_eq_str(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_fail(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_pass_count(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_test_fail_count(ptr)")?;
        // Time operations
        writeln!(&mut ir, "declare ptr @patch_seq_time_now(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_time_nanos(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_time_sleep_ms(ptr)")?;
        // Stack introspection (for REPL)
        writeln!(&mut ir, "declare ptr @patch_seq_stack_dump(ptr)")?;
        writeln!(&mut ir, "; Helpers for conditionals")?;
        writeln!(&mut ir, "declare i64 @patch_seq_peek_int_value(ptr)")?;
        writeln!(&mut ir, "declare i1 @patch_seq_peek_bool_value(ptr)")?;
        writeln!(&mut ir, "declare ptr @patch_seq_pop_stack(ptr)")?;
        writeln!(&mut ir)?;

        // Tagged stack runtime declarations
        writeln!(&mut ir, "; Tagged stack operations")?;
        writeln!(&mut ir, "declare ptr @seq_stack_new_default()")?;
        writeln!(&mut ir, "declare void @seq_stack_free(ptr)")?;
        writeln!(&mut ir, "declare ptr @seq_stack_base(ptr)")?;
        writeln!(&mut ir, "declare i64 @seq_stack_sp(ptr)")?;
        writeln!(&mut ir, "declare void @seq_stack_set_sp(ptr, i64)")?;
        writeln!(&mut ir, "declare void @seq_stack_grow(ptr, i64)")?;
        writeln!(&mut ir, "declare void @patch_seq_set_stack_base(ptr)")?;
        writeln!(&mut ir)?;

        // External builtin declarations (from config)
        if !self.external_builtins.is_empty() {
            writeln!(&mut ir, "; External builtin declarations")?;
            for symbol in self.external_builtins.values() {
                // All external builtins follow the standard stack convention: ptr -> ptr
                writeln!(&mut ir, "declare ptr @{}(ptr)", symbol)?;
            }
            writeln!(&mut ir)?;
        }

        // Quotation functions (generated from quotation literals)
        if !self.quotation_functions.is_empty() {
            writeln!(&mut ir, "; Quotation functions")?;
            ir.push_str(&self.quotation_functions);
            writeln!(&mut ir)?;
        }

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Generate LLVM IR for entire program with FFI support
    ///
    /// This is the main entry point for compiling programs that use FFI.
    pub fn codegen_program_with_ffi(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        config: &CompilerConfig,
        ffi_bindings: &FfiBindings,
    ) -> Result<String, CodeGenError> {
        // Store FFI bindings
        self.ffi_bindings = ffi_bindings.clone();

        // Generate FFI wrapper functions
        self.generate_ffi_wrappers()?;

        // Store type map for use during code generation
        self.type_map = type_map;

        // Store union definitions for pattern matching
        self.unions = program.unions.clone();

        // Build external builtins map from config
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        // Verify we have a main word
        if program.find_word("main").is_none() {
            return Err(CodeGenError::Logic("No main word defined".to_string()));
        }

        // Generate all user-defined words
        for word in &program.words {
            self.codegen_word(word)?;
        }

        // Generate main function
        self.codegen_main()?;

        // Assemble final IR
        let mut ir = String::new();

        // Target and type declarations
        writeln!(&mut ir, "; ModuleID = 'main'")?;
        writeln!(&mut ir, "target triple = \"{}\"", get_target_triple())?;
        writeln!(&mut ir)?;

        // Value type (Rust enum with #[repr(C)], 40 bytes: discriminant + largest variant payload)
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

        // Runtime function declarations (same as codegen_program_with_config)
        self.emit_runtime_declarations(&mut ir)?;

        // FFI C function declarations
        if !self.ffi_bindings.functions.is_empty() {
            writeln!(&mut ir, "; FFI C function declarations")?;
            writeln!(&mut ir, "declare ptr @malloc(i64)")?;
            writeln!(&mut ir, "declare void @free(ptr)")?;
            writeln!(&mut ir, "declare i64 @strlen(ptr)")?;
            writeln!(&mut ir, "declare ptr @memcpy(ptr, ptr, i64)")?;
            // Declare FFI string helpers from runtime
            writeln!(
                &mut ir,
                "declare ptr @patch_seq_string_to_cstring(ptr, ptr)"
            )?;
            writeln!(
                &mut ir,
                "declare ptr @patch_seq_cstring_to_string(ptr, ptr)"
            )?;
            for func in self.ffi_bindings.functions.values() {
                let c_ret_type = ffi_return_type(&func.return_spec);
                let c_args = ffi_c_args(&func.args);
                writeln!(
                    &mut ir,
                    "declare {} @{}({})",
                    c_ret_type, func.c_name, c_args
                )?;
            }
            writeln!(&mut ir)?;
        }

        // External builtin declarations (from config)
        if !self.external_builtins.is_empty() {
            writeln!(&mut ir, "; External builtin declarations")?;
            for symbol in self.external_builtins.values() {
                writeln!(&mut ir, "declare ptr @{}(ptr)", symbol)?;
            }
            writeln!(&mut ir)?;
        }

        // FFI wrapper functions
        if !self.ffi_wrapper_code.is_empty() {
            writeln!(&mut ir, "; FFI wrapper functions")?;
            ir.push_str(&self.ffi_wrapper_code);
            writeln!(&mut ir)?;
        }

        // Quotation functions
        if !self.quotation_functions.is_empty() {
            writeln!(&mut ir, "; Quotation functions")?;
            ir.push_str(&self.quotation_functions);
            writeln!(&mut ir)?;
        }

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Emit string and symbol global constants
    fn emit_string_and_symbol_globals(&self, ir: &mut String) -> Result<(), CodeGenError> {
        // String constants
        if !self.string_globals.is_empty() {
            ir.push_str(&self.string_globals);
            writeln!(ir)?;
        }

        // Symbol constants (interned symbols for O(1) equality)
        if !self.symbol_globals.is_empty() {
            ir.push_str(&self.symbol_globals);
            writeln!(ir)?;
        }
        Ok(())
    }

    /// Emit runtime function declarations
    fn emit_runtime_declarations(&self, ir: &mut String) -> Result<(), CodeGenError> {
        writeln!(ir, "; Runtime function declarations")?;
        writeln!(ir, "declare ptr @patch_seq_push_int(ptr, i64)")?;
        writeln!(ir, "declare ptr @patch_seq_push_string(ptr, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_push_symbol(ptr, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_push_interned_symbol(ptr, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_write(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_write_line(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_read_line(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_read_line_plus(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_read_n(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_int_to_string(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_symbol_to_string(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_to_symbol(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_add(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_subtract(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_multiply(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_divide(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_eq(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_lt(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_gt(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_lte(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_gte(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_neq(ptr)")?;
        writeln!(ir, "; Boolean operations")?;
        writeln!(ir, "declare ptr @patch_seq_and(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_or(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_not(ptr)")?;
        writeln!(ir, "; Bitwise operations")?;
        writeln!(ir, "declare ptr @patch_seq_band(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_bor(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_bxor(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_bnot(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_shl(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_shr(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_popcount(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_clz(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_ctz(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_int_bits(ptr)")?;
        // LLVM intrinsics for bit counting (used by inline codegen)
        writeln!(ir, "declare i64 @llvm.ctpop.i64(i64)")?;
        writeln!(ir, "declare i64 @llvm.ctlz.i64(i64, i1)")?;
        writeln!(ir, "declare i64 @llvm.cttz.i64(i64, i1)")?;
        // LLVM intrinsic for memmove (used by roll)
        writeln!(
            ir,
            "declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)"
        )?;
        // LLVM intrinsic for trap (used by division-by-zero check)
        writeln!(ir, "declare void @llvm.trap() noreturn nounwind")?;
        writeln!(ir, "; Stack operations")?;
        writeln!(ir, "declare ptr @patch_seq_dup(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_drop_op(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_swap(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_over(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_rot(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_nip(ptr)")?;
        writeln!(ir, "declare void @patch_seq_clone_value(ptr, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_tuck(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_2dup(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_3drop(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_pick_op(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_roll(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_push_value(ptr, %Value)")?;
        writeln!(ir, "; Quotation operations")?;
        writeln!(ir, "declare ptr @patch_seq_push_quotation(ptr, i64, i64)")?;
        writeln!(ir, "declare ptr @patch_seq_call(ptr)")?;
        writeln!(ir, "declare i64 @patch_seq_peek_is_quotation(ptr)")?;
        writeln!(ir, "declare i64 @patch_seq_peek_quotation_fn_ptr(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_times(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_while_loop(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_until_loop(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_spawn(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_weave(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_resume(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_weave_cancel(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_yield(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_cond(ptr)")?;
        writeln!(ir, "; Closure operations")?;
        writeln!(ir, "declare ptr @patch_seq_create_env(i32)")?;
        writeln!(ir, "declare void @patch_seq_env_set(ptr, i32, %Value)")?;
        writeln!(ir, "declare %Value @patch_seq_env_get(ptr, i64, i32)")?;
        writeln!(ir, "declare i64 @patch_seq_env_get_int(ptr, i64, i32)")?;
        writeln!(ir, "declare i64 @patch_seq_env_get_bool(ptr, i64, i32)")?;
        writeln!(ir, "declare double @patch_seq_env_get_float(ptr, i64, i32)")?;
        writeln!(
            ir,
            "declare i64 @patch_seq_env_get_quotation(ptr, i64, i32)"
        )?;
        writeln!(ir, "declare ptr @patch_seq_env_get_string(ptr, i64, i32)")?;
        writeln!(
            ir,
            "declare ptr @patch_seq_env_push_string(ptr, ptr, i64, i32)"
        )?;
        writeln!(ir, "declare %Value @patch_seq_make_closure(i64, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_push_closure(ptr, i64, i32)")?;
        writeln!(ir, "declare ptr @patch_seq_push_seqstring(ptr, ptr)")?;
        writeln!(ir, "; Concurrency operations")?;
        writeln!(ir, "declare ptr @patch_seq_make_channel(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_chan_send(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_chan_receive(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_close_channel(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_yield_strand(ptr)")?;
        writeln!(ir, "declare void @patch_seq_maybe_yield()")?;
        writeln!(ir, "; Scheduler operations")?;
        writeln!(ir, "declare void @patch_seq_scheduler_init()")?;
        writeln!(ir, "declare ptr @patch_seq_scheduler_run()")?;
        writeln!(ir, "declare i64 @patch_seq_strand_spawn(ptr, ptr)")?;
        writeln!(ir, "; Command-line argument operations")?;
        writeln!(ir, "declare void @patch_seq_args_init(i32, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_arg_count(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_arg_at(ptr)")?;
        writeln!(ir, "; File operations")?;
        writeln!(ir, "declare ptr @patch_seq_file_slurp(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_file_exists(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_file_for_each_line_plus(ptr)")?;
        writeln!(ir, "; List operations")?;
        writeln!(ir, "declare ptr @patch_seq_list_map(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_list_filter(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_list_fold(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_list_each(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_list_length(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_list_empty(ptr)")?;
        writeln!(ir, "; Map operations")?;
        writeln!(ir, "declare ptr @patch_seq_make_map(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_get(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_set(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_has(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_remove(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_keys(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_values(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_size(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_map_empty(ptr)")?;
        writeln!(ir, "; TCP operations")?;
        writeln!(ir, "declare ptr @patch_seq_tcp_listen(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_tcp_accept(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_tcp_read(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_tcp_write(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_tcp_close(ptr)")?;
        writeln!(ir, "; OS operations")?;
        writeln!(ir, "declare ptr @patch_seq_getenv(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_home_dir(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_current_dir(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_exists(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_is_file(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_is_dir(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_join(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_parent(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_path_filename(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_exit(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_os_name(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_os_arch(ptr)")?;
        writeln!(ir, "; String operations")?;
        writeln!(ir, "declare ptr @patch_seq_string_concat(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_length(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_byte_length(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_char_at(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_substring(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_char_to_string(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_find(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_split(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_contains(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_starts_with(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_empty(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_trim(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_chomp(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_to_upper(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_to_lower(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_equal(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_json_escape(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_to_int(ptr)")?;
        writeln!(ir, "; Symbol operations")?;
        writeln!(ir, "declare ptr @patch_seq_symbol_equal(ptr)")?;
        writeln!(ir, "; Variant operations")?;
        writeln!(ir, "declare ptr @patch_seq_variant_field_count(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_variant_tag(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_variant_field_at(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_variant_append(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_variant_last(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_variant_init(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_make_variant_0(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_make_variant_1(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_make_variant_2(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_make_variant_3(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_make_variant_4(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_unpack_variant(ptr, i64)")?;
        writeln!(ir, "declare ptr @patch_seq_symbol_eq_cstr(ptr, ptr)")?;
        writeln!(ir, "; Float operations")?;
        writeln!(ir, "declare ptr @patch_seq_push_float(ptr, double)")?;
        writeln!(ir, "declare ptr @patch_seq_f_add(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_subtract(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_multiply(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_divide(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_eq(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_lt(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_gt(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_lte(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_gte(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_f_neq(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_int_to_float(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_float_to_int(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_float_to_string(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_string_to_float(ptr)")?;
        writeln!(ir, "; Test framework operations")?;
        writeln!(ir, "declare ptr @patch_seq_test_init(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_finish(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_has_failures(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_assert(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_assert_not(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_assert_eq(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_assert_eq_str(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_fail(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_pass_count(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_test_fail_count(ptr)")?;
        // Time operations
        writeln!(ir, "declare ptr @patch_seq_time_now(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_time_nanos(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_time_sleep_ms(ptr)")?;
        // Stack introspection (for REPL)
        writeln!(ir, "declare ptr @patch_seq_stack_dump(ptr)")?;
        writeln!(ir, "; Helpers for conditionals")?;
        writeln!(ir, "declare i64 @patch_seq_peek_int_value(ptr)")?;
        writeln!(ir, "declare i1 @patch_seq_peek_bool_value(ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_pop_stack(ptr)")?;
        writeln!(ir)?;

        // Tagged stack runtime declarations
        writeln!(ir, "; Tagged stack operations")?;
        writeln!(ir, "declare ptr @seq_stack_new_default()")?;
        writeln!(ir, "declare void @seq_stack_free(ptr)")?;
        writeln!(ir, "declare ptr @seq_stack_base(ptr)")?;
        writeln!(ir, "declare i64 @seq_stack_sp(ptr)")?;
        writeln!(ir, "declare void @seq_stack_set_sp(ptr, i64)")?;
        writeln!(ir, "declare void @seq_stack_grow(ptr, i64)")?;
        writeln!(ir, "declare void @patch_seq_set_stack_base(ptr)")?;
        writeln!(ir)?;

        Ok(())
    }

    /// Generate FFI wrapper functions
    fn generate_ffi_wrappers(&mut self) -> Result<(), CodeGenError> {
        // Collect functions to avoid borrowing self.ffi_bindings while mutating self
        let funcs: Vec<_> = self.ffi_bindings.functions.values().cloned().collect();
        for func in funcs {
            self.generate_ffi_wrapper(&func)?;
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // FFI Wrapper Helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Allocate storage for a by_ref out parameter
    fn write_ffi_by_ref_alloca(
        &mut self,
        i: usize,
        ffi_type: &FfiType,
    ) -> Result<String, CodeGenError> {
        let alloca_var = format!("out_param_{}", i);
        let llvm_type = match ffi_type {
            FfiType::Ptr => "ptr",
            FfiType::Int => "i64",
            _ => {
                return Err(CodeGenError::Logic(format!(
                    "Unsupported type {:?} for by_ref parameter",
                    ffi_type
                )));
            }
        };
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = alloca {}",
            alloca_var, llvm_type
        )?;
        Ok(alloca_var)
    }

    /// Pop an FFI argument from the stack and return (c_arg_string, optional_cstr_var_to_free)
    fn write_ffi_pop_arg(
        &mut self,
        i: usize,
        arg: &crate::ffi::FfiArg,
        stack_var: &mut String,
    ) -> Result<(String, Option<String>), CodeGenError> {
        // Handle fixed value arguments
        if let Some(ref value) = arg.value {
            return match value.as_str() {
                "null" | "NULL" => Ok(("ptr null".to_string(), None)),
                _ => value
                    .parse::<i64>()
                    .map(|int_val| (format!("i64 {}", int_val), None))
                    .map_err(|e| {
                        CodeGenError::Logic(format!(
                            "Invalid fixed value '{}' for argument {}: {}. \
                         Expected 'null' or a 64-bit integer.",
                            value, i, e
                        ))
                    }),
            };
        }

        match (&arg.arg_type, &arg.pass) {
            (_, PassMode::ByRef) => {
                // by_ref args don't pop from stack - just reference the alloca
                Ok((format!("ptr %out_param_{}", i), None))
            }
            (FfiType::String, PassMode::CString) => self.write_ffi_pop_cstring(i, stack_var),
            (FfiType::Int, _) => self.write_ffi_pop_int(i, stack_var).map(|s| (s, None)),
            (FfiType::Ptr, PassMode::Ptr) => {
                self.write_ffi_pop_ptr(i, stack_var).map(|s| (s, None))
            }
            _ => Err(CodeGenError::Logic(format!(
                "Unsupported FFI argument type {:?} with pass mode {:?}",
                arg.arg_type, arg.pass
            ))),
        }
    }

    /// Pop a C string argument from the stack - returns (c_arg, cstr_var_to_free)
    fn write_ffi_pop_cstring(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<(String, Option<String>), CodeGenError> {
        let cstr_var = format!("cstr_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_string_to_cstring(ptr %{}, ptr null)",
            cstr_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok((format!("ptr %{}", cstr_var), Some(cstr_var)))
    }

    /// Pop an integer argument from the stack
    fn write_ffi_pop_int(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<String, CodeGenError> {
        let int_var = format!("int_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
            int_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok(format!("i64 %{}", int_var))
    }

    /// Pop a pointer argument from the stack
    fn write_ffi_pop_ptr(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<String, CodeGenError> {
        let int_var = format!("ptr_int_{}", i);
        let ptr_var = format!("ptr_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
            int_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = inttoptr i64 %{} to ptr",
            ptr_var, int_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok(format!("ptr %{}", ptr_var))
    }

    /// Push a by_ref out parameter result onto the stack
    fn write_ffi_push_by_ref_result(
        &mut self,
        alloca_var: &str,
        ffi_type: &FfiType,
        stack_var: &mut String,
    ) -> Result<(), CodeGenError> {
        let new_stack = format!("stack_after_byref_{}", alloca_var);
        match ffi_type {
            FfiType::Ptr => {
                let loaded_var = format!("{}_val", alloca_var);
                let int_var = format!("{}_int", alloca_var);
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = load ptr, ptr %{}",
                    loaded_var, alloca_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = ptrtoint ptr %{} to i64",
                    int_var, loaded_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                    new_stack, stack_var, int_var
                )?;
            }
            FfiType::Int => {
                let loaded_var = format!("{}_val", alloca_var);
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = load i64, ptr %{}",
                    loaded_var, alloca_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                    new_stack, stack_var, loaded_var
                )?;
            }
            _ => return Ok(()), // Other types not supported for by_ref
        }
        *stack_var = new_stack;
        Ok(())
    }

    /// Handle FFI return value - string type (with NULL check)
    fn write_ffi_return_string(
        &mut self,
        stack_var: &str,
        caller_frees: bool,
    ) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %is_null = icmp eq ptr %c_result, null"
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  br i1 %is_null, label %null_case, label %valid_case"
        )?;

        // NULL case - push empty string
        writeln!(&mut self.ffi_wrapper_code, "null_case:")?;
        let empty_str = self.get_string_global("")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %stack_null = call ptr @patch_seq_push_string(ptr %{}, ptr {})",
            stack_var, empty_str
        )?;
        writeln!(&mut self.ffi_wrapper_code, "  br label %done")?;

        // Valid case - convert C string to Seq string
        writeln!(&mut self.ffi_wrapper_code, "valid_case:")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %stack_with_result = call ptr @patch_seq_cstring_to_string(ptr %{}, ptr %c_result)",
            stack_var
        )?;
        if caller_frees {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call void @free(ptr %c_result)"
            )?;
        }
        writeln!(&mut self.ffi_wrapper_code, "  br label %done")?;

        // Join paths
        writeln!(&mut self.ffi_wrapper_code, "done:")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %final_stack = phi ptr [ %stack_null, %null_case ], [ %stack_with_result, %valid_case ]"
        )?;
        writeln!(&mut self.ffi_wrapper_code, "  ret ptr %final_stack")?;
        Ok(())
    }

    /// Handle FFI return value - simple types (Int, Ptr, Void)
    fn write_ffi_return_simple(
        &mut self,
        return_type: &FfiType,
        stack_var: &str,
    ) -> Result<(), CodeGenError> {
        match return_type {
            FfiType::Int => {
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %stack_with_result = call ptr @patch_seq_push_int(ptr %{}, i64 %c_result)",
                    stack_var
                )?;
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %stack_with_result")?;
            }
            FfiType::Void => {
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %{}", stack_var)?;
            }
            FfiType::Ptr => {
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %ptr_as_int = ptrtoint ptr %c_result to i64"
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %stack_with_result = call ptr @patch_seq_push_int(ptr %{}, i64 %ptr_as_int)",
                    stack_var
                )?;
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %stack_with_result")?;
            }
            FfiType::String => {
                // String is handled by write_ffi_return_string
                unreachable!("String return should use write_ffi_return_string");
            }
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Main FFI Wrapper Generator
    // ─────────────────────────────────────────────────────────────────────────

    /// Generate a single FFI wrapper function
    ///
    /// The wrapper:
    /// 1. Pops arguments from the Seq stack
    /// 2. Converts Seq types to C types
    /// 3. Calls the C function
    /// 4. Converts result back to Seq type
    /// 5. Pushes result onto Seq stack
    /// 6. Frees memory if needed (caller_frees)
    fn generate_ffi_wrapper(
        &mut self,
        func: &crate::ffi::FfiFunctionInfo,
    ) -> Result<(), CodeGenError> {
        let wrapper_name = format!("seq_ffi_{}", mangle_name(&func.seq_name));

        writeln!(
            &mut self.ffi_wrapper_code,
            "define ptr @{}(ptr %stack) {{",
            wrapper_name
        )?;
        writeln!(&mut self.ffi_wrapper_code, "entry:")?;

        let mut stack_var = "stack".to_string();
        let mut c_args: Vec<String> = Vec::new();
        let mut c_string_vars: Vec<String> = Vec::new();
        let mut by_ref_vars: Vec<(String, FfiType)> = Vec::new();

        // First pass: allocate storage for by_ref out parameters
        for (i, arg) in func.args.iter().enumerate() {
            if arg.pass == PassMode::ByRef {
                let alloca_var = self.write_ffi_by_ref_alloca(i, &arg.arg_type)?;
                by_ref_vars.push((alloca_var, arg.arg_type.clone()));
            }
        }

        // Second pass: pop arguments from stack (in reverse order - last arg on top)
        for (i, arg) in func.args.iter().enumerate().rev() {
            let (c_arg, cstr_var) = self.write_ffi_pop_arg(i, arg, &mut stack_var)?;
            c_args.push(c_arg);
            if let Some(var) = cstr_var {
                c_string_vars.push(var);
            }
        }

        // Reverse args back to correct order for C call
        c_args.reverse();

        // Generate the C function call
        let c_ret_type = ffi_return_type(&func.return_spec);
        let c_args_str = c_args.join(", ");
        let has_return = func
            .return_spec
            .as_ref()
            .is_some_and(|r| r.return_type != FfiType::Void);

        if has_return {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  %c_result = call {} @{}({})",
                c_ret_type, func.c_name, c_args_str
            )?;
        } else {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call {} @{}({})",
                c_ret_type, func.c_name, c_args_str
            )?;
        }

        // Free C strings we allocated for arguments
        for cstr_var in &c_string_vars {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call void @free(ptr %{})",
                cstr_var
            )?;
        }

        // Push by_ref out parameter values onto stack
        for (alloca_var, ffi_type) in &by_ref_vars {
            self.write_ffi_push_by_ref_result(alloca_var, ffi_type, &mut stack_var)?;
        }

        // Handle return value
        if let Some(ref return_spec) = func.return_spec {
            if return_spec.return_type == FfiType::String {
                self.write_ffi_return_string(
                    &stack_var,
                    return_spec.ownership == Ownership::CallerFrees,
                )?;
            } else {
                self.write_ffi_return_simple(&return_spec.return_type, &stack_var)?;
            }
        } else {
            writeln!(&mut self.ffi_wrapper_code, "  ret ptr %{}", stack_var)?;
        }

        writeln!(&mut self.ffi_wrapper_code, "}}")?;
        writeln!(&mut self.ffi_wrapper_code)?;

        Ok(())
    }

    /// Generate code for a word definition
    fn codegen_word(&mut self, word: &WordDef) -> Result<(), CodeGenError> {
        // Prefix word names with "seq_" to avoid conflicts with C symbols
        // Also mangle special characters that aren't valid in LLVM IR identifiers
        let function_name = format!("seq_{}", mangle_name(&word.name));

        // main uses C calling convention since it's called from the runtime via function pointer.
        // All other words use tailcc for guaranteed tail call optimization.
        // This is fine because recursive main would be terrible design anyway.
        let is_main = word.name == "main";
        self.inside_main = is_main;

        if is_main {
            writeln!(
                &mut self.output,
                "define ptr @{}(ptr %stack) {{",
                function_name
            )?;
        } else {
            writeln!(
                &mut self.output,
                "define tailcc ptr @{}(ptr %stack) {{",
                function_name
            )?;
        }
        writeln!(&mut self.output, "entry:")?;

        // For main (non-pure-inline): allocate the tagged stack and get base pointer
        // In pure_inline_test mode, main() allocates the stack, so seq_main just uses %stack
        let mut stack_var = if is_main && !self.pure_inline_test {
            // Allocate tagged stack
            writeln!(
                &mut self.output,
                "  %tagged_stack = call ptr @seq_stack_new_default()"
            )?;
            // Get base pointer - this is our initial "stack" (SP points to first free slot)
            writeln!(
                &mut self.output,
                "  %stack_base = call ptr @seq_stack_base(ptr %tagged_stack)"
            )?;
            // Set thread-local stack base for clone_stack (needed by spawn)
            writeln!(
                &mut self.output,
                "  call void @patch_seq_set_stack_base(ptr %stack_base)"
            )?;
            "stack_base".to_string()
        } else {
            "stack".to_string()
        };

        // Generate code for all statements with pattern detection for inline loops
        stack_var = self.codegen_statements(&word.body, &stack_var, true)?;

        // Only emit ret if the last statement wasn't a tail call
        // (tail calls emit their own ret)
        if word.body.is_empty()
            || !self.will_emit_tail_call(word.body.last().unwrap(), TailPosition::Tail)
        {
            if is_main && !self.pure_inline_test {
                // Normal mode: free the stack before returning
                writeln!(
                    &mut self.output,
                    "  call void @seq_stack_free(ptr %tagged_stack)"
                )?;
                // Return null since we've freed the stack
                writeln!(&mut self.output, "  ret ptr null")?;
            } else {
                // Return the final stack pointer (used by main to read result)
                writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
            }
        }
        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        self.inside_main = false;
        Ok(())
    }

    /// Generate a quotation function
    /// Returns wrapper and impl function names for TCO support
    fn codegen_quotation(
        &mut self,
        body: &[Statement],
        quot_type: &Type,
    ) -> Result<QuotationFunctions, CodeGenError> {
        // Generate unique function names
        let base_name = format!("seq_quot_{}", self.quot_counter);
        self.quot_counter += 1;

        // Save current output and switch to quotation_functions
        let saved_output = std::mem::take(&mut self.output);

        // Generate function signature based on type
        match quot_type {
            Type::Quotation(_) => {
                // Stateless quotation: generate both wrapper (C) and impl (tailcc)
                let wrapper_name = base_name.clone();
                let impl_name = format!("{}_impl", base_name);

                // First, generate the impl function with tailcc convention
                // This is the actual function body that can be tail-called
                writeln!(
                    &mut self.output,
                    "define tailcc ptr @{}(ptr %stack) {{",
                    impl_name
                )?;
                writeln!(&mut self.output, "entry:")?;

                let mut stack_var = "stack".to_string();
                let body_len = body.len();

                // Generate code for each statement in the quotation body
                // Last statement is in tail position (can use musttail)
                for (i, statement) in body.iter().enumerate() {
                    let position = if i == body_len - 1 {
                        TailPosition::Tail
                    } else {
                        TailPosition::NonTail
                    };
                    stack_var = self.codegen_statement(&stack_var, statement, position)?;
                }

                // Only emit ret if the last statement wasn't a tail call
                if body.is_empty()
                    || !self.will_emit_tail_call(body.last().unwrap(), TailPosition::Tail)
                {
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
                }
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Now generate the wrapper function with C convention
                // This is a thin wrapper that just calls the impl
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack) {{",
                    wrapper_name
                )?;
                writeln!(&mut self.output, "entry:")?;
                writeln!(
                    &mut self.output,
                    "  %result = call tailcc ptr @{}(ptr %stack)",
                    impl_name
                )?;
                writeln!(&mut self.output, "  ret ptr %result")?;
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Move generated functions to quotation_functions
                self.quotation_functions.push_str(&self.output);

                // Restore original output
                self.output = saved_output;

                Ok(QuotationFunctions {
                    wrapper: wrapper_name,
                    impl_: impl_name,
                })
            }
            Type::Closure { captures, .. } => {
                // Closure: fn(Stack, *const Value, usize) -> Stack
                // Note: Closures don't use tailcc yet (Phase 3 work)
                // Mark that we're inside a closure to disable tail calls
                self.inside_closure = true;
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack, ptr %env_data, i64 %env_len) {{",
                    base_name
                )?;
                writeln!(&mut self.output, "entry:")?;

                // Push captured values onto the stack before executing body
                // Captures are stored bottom-to-top, so push them in order
                let mut stack_var = "stack".to_string();
                for (index, capture_type) in captures.iter().enumerate() {
                    stack_var = self.emit_capture_push(capture_type, index, &stack_var)?;
                }

                // Generate code for each statement in the quotation body
                // Last statement is in tail position
                let body_len = body.len();
                for (i, statement) in body.iter().enumerate() {
                    let position = if i == body_len - 1 {
                        TailPosition::Tail
                    } else {
                        TailPosition::NonTail
                    };
                    stack_var = self.codegen_statement(&stack_var, statement, position)?;
                }

                // Only emit ret if the last statement wasn't a tail call
                if body.is_empty()
                    || !self.will_emit_tail_call(body.last().unwrap(), TailPosition::Tail)
                {
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
                }
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Move generated function to quotation_functions
                self.quotation_functions.push_str(&self.output);

                // Restore original output and reset closure flag
                self.output = saved_output;
                self.inside_closure = false;

                // For closures, both wrapper and impl are the same (no TCO yet)
                Ok(QuotationFunctions {
                    wrapper: base_name.clone(),
                    impl_: base_name,
                })
            }
            _ => Err(CodeGenError::Logic(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            ))),
        }
    }

    /// Check if a name refers to a runtime builtin (not a user-defined word).
    fn is_runtime_builtin(&self, name: &str) -> bool {
        BUILTIN_SYMBOLS.contains_key(name)
            || self.external_builtins.contains_key(name)
            || self.ffi_bindings.is_ffi_function(name)
    }

    /// Emit code to push a captured value onto the stack.
    /// Returns the new stack variable name, or an error for unsupported types.
    fn emit_capture_push(
        &mut self,
        capture_type: &Type,
        index: usize,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // String captures use a combined get+push function to avoid returning
        // SeqString by value through FFI (causes crashes on Linux due to calling convention)
        if matches!(capture_type, Type::String) {
            let new_stack_var = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_env_push_string(ptr %{}, ptr %env_data, i64 %env_len, i32 {})",
                new_stack_var, stack_var, index
            )?;
            return Ok(new_stack_var);
        }

        // Each capture type needs: (getter_fn, getter_llvm_type, pusher_fn, pusher_llvm_type)
        let (getter, getter_type, pusher, pusher_type) = match capture_type {
            Type::Int => ("patch_seq_env_get_int", "i64", "patch_seq_push_int", "i64"),
            Type::Bool => ("patch_seq_env_get_bool", "i64", "patch_seq_push_int", "i64"),
            Type::Float => (
                "patch_seq_env_get_float",
                "double",
                "patch_seq_push_float",
                "double",
            ),
            Type::String => unreachable!("String handled above"),
            Type::Quotation(_) => (
                "patch_seq_env_get_quotation",
                "i64",
                "patch_seq_push_quotation",
                "i64",
            ),
            Type::Closure { .. } => {
                return Err(CodeGenError::Logic(
                    "Closure captures are not yet supported. \
                     Closures capturing other closures require additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                        .to_string(),
                ));
            }
            Type::Var(name) if name.starts_with("Variant") => {
                return Err(CodeGenError::Logic(
                    "Variant captures are not yet supported. \
                     Capturing Variants in closures requires additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                        .to_string(),
                ));
            }
            _ => {
                return Err(CodeGenError::Logic(format!(
                    "Unsupported capture type: {:?}. \
                     Supported capture types: Int, Bool, Float, String, Quotation.",
                    capture_type
                )));
            }
        };

        // Get value from environment
        let value_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call {} @{}(ptr %env_data, i64 %env_len, i32 {})",
            value_var, getter_type, getter, index
        )?;

        // Push value onto stack
        let new_stack_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @{}(ptr %{}, {} %{})",
            new_stack_var, pusher, stack_var, pusher_type, value_var
        )?;

        Ok(new_stack_var)
    }

    /// Generate code for a single branch of an if statement.
    ///
    /// Returns the final stack variable, whether a tail call was emitted,
    /// and the predecessor block label for the phi node.
    fn codegen_branch(
        &mut self,
        statements: &[Statement],
        initial_stack: &str,
        position: TailPosition,
        merge_block: &str,
        block_prefix: &str,
    ) -> Result<BranchResult, CodeGenError> {
        let mut stack_var = initial_stack.to_string();
        let len = statements.len();
        let mut emitted_tail_call = false;

        for (i, stmt) in statements.iter().enumerate() {
            let stmt_position = if i == len - 1 {
                position // Last statement inherits our tail position
            } else {
                TailPosition::NonTail
            };
            if i == len - 1 {
                emitted_tail_call = self.will_emit_tail_call(stmt, stmt_position);
            }
            stack_var = self.codegen_statement(&stack_var, stmt, stmt_position)?;
        }

        // Only emit landing block if no tail call was emitted
        let predecessor = if emitted_tail_call {
            UNREACHABLE_PREDECESSOR.to_string()
        } else {
            let pred = self.fresh_block(&format!("{}_end", block_prefix));
            writeln!(&mut self.output, "  br label %{}", pred)?;
            writeln!(&mut self.output, "{}:", pred)?;
            writeln!(&mut self.output, "  br label %{}", merge_block)?;
            pred
        };

        Ok(BranchResult {
            stack_var,
            emitted_tail_call,
            predecessor,
        })
    }

    /// Check if a statement in tail position would emit a terminator (ret).
    ///
    /// Returns true for:
    /// - User-defined word calls (emit `musttail` + `ret`)
    /// - `call` word (Phase 2 TCO for quotations)
    /// - If statements where BOTH branches emit terminators
    ///
    /// Returns false if inside a closure (closures can't use `musttail` due to
    /// signature mismatch - they have 3 params vs 1 for regular functions).
    /// Also returns false if inside main or quotation (they use C convention, can't musttail to tailcc).
    fn will_emit_tail_call(&self, statement: &Statement, position: TailPosition) -> bool {
        if position != TailPosition::Tail
            || self.inside_closure
            || self.inside_main
            || self.inside_quotation
        {
            return false;
        }
        match statement {
            Statement::WordCall { name, .. } => {
                // Phase 2 TCO: `call` is now TCO-eligible (it emits its own ret)
                if name == "call" {
                    return true;
                }
                !self.is_runtime_builtin(name)
            }
            Statement::If {
                then_branch,
                else_branch,
            } => {
                // An if statement emits a terminator (no merge block) if BOTH branches
                // end with terminators. Empty branches don't terminate.
                let then_terminates = then_branch
                    .last()
                    .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail));
                let else_terminates = else_branch
                    .as_ref()
                    .and_then(|eb| eb.last())
                    .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail));
                then_terminates && else_terminates
            }
            _ => false,
        }
    }

    /// Generate code for a tail call to a quotation (Phase 2 TCO).
    ///
    /// This is called when `call` is in tail position. We generate inline dispatch:
    /// 1. Check if top of stack is a Quotation (not Closure)
    /// 2. If Quotation: pop, extract fn_ptr, musttail call it
    /// 3. If Closure: call regular patch_seq_call (no TCO for closures yet)
    ///
    /// The function always emits a `ret`, so no merge block is needed.
    fn codegen_tail_call_quotation(
        &mut self,
        stack_var: &str,
        _result_var: &str,
    ) -> Result<String, CodeGenError> {
        // Check if top of stack is a quotation
        let is_quot_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_is_quotation(ptr %{})",
            is_quot_var, stack_var
        )?;

        // Compare to 1 (true = quotation)
        let cmp_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 1",
            cmp_var, is_quot_var
        )?;

        // Create labels for branching
        let quot_block = self.fresh_block("call_quotation");
        let closure_block = self.fresh_block("call_closure");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_var, quot_block, closure_block
        )?;

        // Quotation path: extract fn_ptr and musttail call
        writeln!(&mut self.output, "{}:", quot_block)?;
        let fn_ptr_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_quotation_fn_ptr(ptr %{})",
            fn_ptr_var, stack_var
        )?;

        // Pop the quotation from the stack
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            popped_stack, stack_var
        )?;

        // Convert i64 fn_ptr to function pointer type
        let fn_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = inttoptr i64 %{} to ptr",
            fn_var, fn_ptr_var
        )?;

        // Tail call the quotation's impl function using musttail + tailcc
        // This is guaranteed TCO: caller is tailcc, quotation impl is tailcc
        // Yield check before tail call to prevent starvation in tight loops
        writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
        let quot_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = musttail call tailcc ptr %{}(ptr %{})",
            quot_result, fn_var, popped_stack
        )?;
        writeln!(&mut self.output, "  ret ptr %{}", quot_result)?;

        // Closure path: fall back to regular patch_seq_call
        // Use a fresh temp to ensure proper SSA numbering (must be >= quotation branch temps)
        //
        // Note: No yield check here because closures use regular calls (not musttail),
        // so recursive closures will eventually hit stack limits. The yield safety valve
        // is specifically for unbounded TCO loops which can run infinitely.
        writeln!(&mut self.output, "{}:", closure_block)?;
        let closure_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_call(ptr %{})",
            closure_result, stack_var
        )?;
        writeln!(&mut self.output, "  ret ptr %{}", closure_result)?;

        // Return a dummy value - both branches emit ret, so this won't be used
        Ok(closure_result)
    }

    // =========================================================================
    // Statement Code Generation Helpers
    // =========================================================================

    /// Generate code for an integer literal: ( -- n )
    fn codegen_int_literal(&mut self, stack_var: &str, n: i64) -> Result<String, CodeGenError> {
        // Inline push: Write Value directly to stack
        // Value layout with #[repr(C)]: slot0=discriminant, slot1=value
        // stack_var points to where the next value should go

        // Store discriminant 0 (Int) at slot0
        writeln!(&mut self.output, "  store i64 0, ptr %{}", stack_var)?;

        // Get pointer to slot1 (offset 8 bytes = 1 i64)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, stack_var
        )?;

        // Store value at slot1
        writeln!(&mut self.output, "  store i64 {}, ptr %{}", n, slot1_ptr)?;

        // Return pointer to next Value slot
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a float literal: ( -- f )
    ///
    /// Uses LLVM's hexadecimal floating point format for exact representation.
    /// Handles special values (NaN, Infinity) explicitly.
    fn codegen_float_literal(&mut self, stack_var: &str, f: f64) -> Result<String, CodeGenError> {
        // Format float bits as hex for LLVM
        let float_bits = f.to_bits();

        // Inline push: Write Value directly to stack
        // Value layout with #[repr(C)]: slot0=discriminant, slot1=value
        // Float discriminant = 1 (Int=0, Float=1, Bool=2)

        // Store discriminant 1 (Float) at slot0
        writeln!(&mut self.output, "  store i64 1, ptr %{}", stack_var)?;

        // Get pointer to slot1 (offset 8 bytes = 1 i64)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, stack_var
        )?;

        // Store float bits as i64 at slot1
        writeln!(
            &mut self.output,
            "  store i64 {}, ptr %{}",
            float_bits, slot1_ptr
        )?;

        // Return pointer to next Value slot
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a boolean literal: ( -- b )
    fn codegen_bool_literal(&mut self, stack_var: &str, b: bool) -> Result<String, CodeGenError> {
        let val = if b { 1 } else { 0 };

        // Inline push: Write Value directly to stack
        // Value layout with #[repr(C)]: slot0=discriminant, slot1=value
        // Bool discriminant = 2 (Int=0, Float=1, Bool=2)

        // Store discriminant 2 (Bool) at slot0
        writeln!(&mut self.output, "  store i64 2, ptr %{}", stack_var)?;

        // Get pointer to slot1 (offset 8 bytes = 1 i64)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, stack_var
        )?;

        // Store value at slot1 (1 for true, 0 for false)
        writeln!(&mut self.output, "  store i64 {}, ptr %{}", val, slot1_ptr)?;

        // Return pointer to next Value slot
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a string literal: ( -- s )
    fn codegen_string_literal(&mut self, stack_var: &str, s: &str) -> Result<String, CodeGenError> {
        let global = self.get_string_global(s)?;
        let ptr_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr inbounds [{} x i8], ptr {}, i32 0, i32 0",
            ptr_temp,
            s.len() + 1,
            global
        )?;
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_string(ptr %{}, ptr %{})",
            result_var, stack_var, ptr_temp
        )?;
        Ok(result_var)
    }

    /// Generate code for a symbol literal: ( -- sym )
    fn codegen_symbol_literal(&mut self, stack_var: &str, s: &str) -> Result<String, CodeGenError> {
        // Get interned symbol global (static SeqString structure)
        let sym_global = self.get_symbol_global(s)?;

        // Push the interned symbol - passes pointer to static SeqString structure
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_interned_symbol(ptr %{}, ptr {})",
            result_var, stack_var, sym_global
        )?;
        Ok(result_var)
    }

    /// Try to generate inline code for a tagged stack operation.
    /// Returns Some(result_var) if the operation was inlined, None otherwise.
    fn try_codegen_inline_op(
        &mut self,
        stack_var: &str,
        name: &str,
    ) -> Result<Option<String>, CodeGenError> {
        match name {
            // drop: ( a -- )
            // Must call runtime to properly drop heap values
            "drop" => {
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // dup: ( a -- a a )
            // Uses patch_seq_clone_value to properly clone heap values
            "dup" => {
                let top_ptr = self.fresh_temp();
                let result_var = self.fresh_temp();

                // Get pointer to top value (sp - 1)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;
                // Clone the value from top_ptr to stack_var (current SP)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    top_ptr, stack_var
                )?;
                // Increment SP
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // swap: ( a b -- b a )
            "swap" => {
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                // Get pointers to a and b
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Load full Values (40 bytes each)
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                // Store swapped
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_b
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // over: ( a b -- a b a )
            // Uses patch_seq_clone_value to properly clone heap values
            "over" => {
                let ptr_a = self.fresh_temp();
                let result_var = self.fresh_temp();

                // Get pointer to a (sp - 2)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Clone the value from ptr_a to stack_var (current SP)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Increment SP
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // i.add / i.+: ( a b -- a+b )
            "i.add" | "i.+" => self.codegen_inline_binary_op(stack_var, "add", "sub"),

            // i.subtract / i.-: ( a b -- a-b )
            "i.subtract" | "i.-" => self.codegen_inline_binary_op(stack_var, "sub", "add"),

            // i.multiply / i.*: ( a b -- a*b )
            "i.multiply" | "i.*" => {
                // Values are in slot1 of each Value (slot0 is discriminant 0)
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Get slot1 pointers (offset 8 bytes)
                let slot1_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_a, ptr_a
                )?;
                let slot1_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_b, ptr_b
                )?;

                // Load values from slot1
                let val_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_a, slot1_a
                )?;
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_b, slot1_b
                )?;

                // Multiply
                let product = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = mul i64 %{}, %{}",
                    product, val_a, val_b
                )?;

                // Store result at slot1 (discriminant 0 already at slot0)
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    product, slot1_a
                )?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // i.divide / i./: ( a b -- a/b )
            // Matches runtime behavior: panic on zero, wrapping for i64::MIN/-1
            "i.divide" | "i./" => {
                // Values are in slot1 of each Value (slot0 is discriminant 0)
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Get slot1 pointers (offset 8 bytes)
                let slot1_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_a, ptr_a
                )?;
                let slot1_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_b, ptr_b
                )?;

                // Load values from slot1
                let val_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_a, slot1_a
                )?;
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_b, slot1_b
                )?;

                // Check for division by zero
                let is_zero = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp eq i64 %{}, 0",
                    is_zero, val_b
                )?;

                // Check for overflow case: i64::MIN / -1
                let is_min = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp eq i64 %{}, -9223372036854775808",
                    is_min, val_a
                )?;
                let is_neg_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp eq i64 %{}, -1",
                    is_neg_one, val_b
                )?;
                let is_overflow = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i1 %{}, %{}",
                    is_overflow, is_min, is_neg_one
                )?;

                // Use safe divisor: if zero use 1, if overflow case use 1
                let safe_divisor = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = select i1 %{}, i64 1, i64 %{}",
                    safe_divisor, is_zero, val_b
                )?;
                let final_divisor = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = select i1 %{}, i64 1, i64 %{}",
                    final_divisor, is_overflow, safe_divisor
                )?;

                // Divide (signed) with safe divisor
                let quotient = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sdiv i64 %{}, %{}",
                    quotient, val_a, final_divisor
                )?;

                // For overflow case: result should be i64::MIN (wrapping behavior)
                // For zero case: we'll trap below, but use 0 as placeholder
                let safe_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = select i1 %{}, i64 -9223372036854775808, i64 %{}",
                    safe_result, is_overflow, quotient
                )?;

                // Trap on division by zero (call llvm.trap)
                let ok_label = self.fresh_block("div_ok");
                let trap_label = self.fresh_block("div_trap");
                writeln!(
                    &mut self.output,
                    "  br i1 %{}, label %{}, label %{}",
                    is_zero, trap_label, ok_label
                )?;
                writeln!(&mut self.output, "{}:", trap_label)?;
                writeln!(&mut self.output, "  call void @llvm.trap()")?;
                writeln!(&mut self.output, "  unreachable")?;
                writeln!(&mut self.output, "{}:", ok_label)?;

                // Store result at slot1 (discriminant 0 already at slot0)
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    safe_result, slot1_a
                )?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // i.%: ( a b -- a%b ) - integer modulo/remainder
            "i.%" => {
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                let slot1_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_a, ptr_a
                )?;
                let slot1_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_b, ptr_b
                )?;

                let val_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_a, slot1_a
                )?;
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_b, slot1_b
                )?;

                // Check for division by zero
                let is_zero = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp eq i64 %{}, 0",
                    is_zero, val_b
                )?;

                let ok_label = self.fresh_block("mod_ok");
                let trap_label = self.fresh_block("mod_trap");
                writeln!(
                    &mut self.output,
                    "  br i1 %{}, label %{}, label %{}",
                    is_zero, trap_label, ok_label
                )?;
                writeln!(&mut self.output, "{}:", trap_label)?;
                writeln!(&mut self.output, "  call void @llvm.trap()")?;
                writeln!(&mut self.output, "  unreachable")?;
                writeln!(&mut self.output, "{}:", ok_label)?;

                // Signed remainder
                let remainder = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = srem i64 %{}, %{}",
                    remainder, val_a, val_b
                )?;

                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    remainder, slot1_a
                )?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // Integer comparison operations - result is tagged bool (0=false, 1=true)
            "i.=" | "i.eq" => self.codegen_inline_comparison(stack_var, "eq"),
            "i.<>" | "i.neq" => self.codegen_inline_comparison(stack_var, "ne"),
            "i.<" | "i.lt" => self.codegen_inline_comparison(stack_var, "slt"),
            "i.>" | "i.gt" => self.codegen_inline_comparison(stack_var, "sgt"),
            "i.<=" | "i.lte" => self.codegen_inline_comparison(stack_var, "sle"),
            "i.>=" | "i.gte" => self.codegen_inline_comparison(stack_var, "sge"),

            // Float arithmetic operations
            // Values are stored as f64 bits in slot1, discriminant 1 (Float)
            "f.add" | "f.+" => self.codegen_inline_float_binary_op(stack_var, "fadd"),
            "f.subtract" | "f.-" => self.codegen_inline_float_binary_op(stack_var, "fsub"),
            "f.multiply" | "f.*" => self.codegen_inline_float_binary_op(stack_var, "fmul"),
            "f.divide" | "f./" => self.codegen_inline_float_binary_op(stack_var, "fdiv"),

            // Float comparison operations - result is tagged bool
            "f.=" | "f.eq" => self.codegen_inline_float_comparison(stack_var, "oeq"),
            "f.<>" | "f.neq" => self.codegen_inline_float_comparison(stack_var, "one"),
            "f.<" | "f.lt" => self.codegen_inline_float_comparison(stack_var, "olt"),
            "f.>" | "f.gt" => self.codegen_inline_float_comparison(stack_var, "ogt"),
            "f.<=" | "f.lte" => self.codegen_inline_float_comparison(stack_var, "ole"),
            "f.>=" | "f.gte" => self.codegen_inline_float_comparison(stack_var, "oge"),

            // Boolean operations - values are in slot1, discriminant 2 (Bool)
            // and: ( a b -- a&&b )
            "and" => {
                // Get pointers to Value slots
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Get slot1 pointers (values at offset 8)
                let slot1_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_a, ptr_a
                )?;
                let slot1_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_b, ptr_b
                )?;

                // Load values from slot1
                let val_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_a, slot1_a
                )?;
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_b, slot1_b
                )?;

                // AND the values and convert to 0 or 1
                let and_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i64 %{}, %{}",
                    and_result, val_a, val_b
                )?;
                let bool_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp ne i64 %{}, 0",
                    bool_result, and_result
                )?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, bool_result
                )?;

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // or: ( a b -- a||b )
            "or" => {
                // Get pointers to Value slots
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Get slot1 pointers (values at offset 8)
                let slot1_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_a, ptr_a
                )?;
                let slot1_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_b, ptr_b
                )?;

                // Load values from slot1
                let val_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_a, slot1_a
                )?;
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val_b, slot1_b
                )?;

                // OR the values and convert to 0 or 1
                let or_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = or i64 %{}, %{}",
                    or_result, val_a, val_b
                )?;
                let bool_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp ne i64 %{}, 0",
                    bool_result, or_result
                )?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, bool_result
                )?;

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // not: ( a -- !a )
            "not" => {
                // Get pointer to top Value
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;

                // Get pointer to slot1 (value at offset 8)
                let slot1_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_ptr, top_ptr
                )?;

                // Load value from slot1
                let val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val, slot1_ptr
                )?;

                // not: if val == 0, result is 1; else result is 0
                let is_zero = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = icmp eq i64 %{}, 0", is_zero, val)?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, is_zero
                )?;

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", top_ptr)?;
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    zext, slot1_ptr
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // Bitwise operations - operate on Int values (discriminant 0)
            // band: ( a b -- a&b ) bitwise AND
            "band" => self.codegen_inline_int_bitwise_binary(stack_var, "and"),

            // bor: ( a b -- a|b ) bitwise OR
            "bor" => self.codegen_inline_int_bitwise_binary(stack_var, "or"),

            // bxor: ( a b -- a^b ) bitwise XOR
            "bxor" => self.codegen_inline_int_bitwise_binary(stack_var, "xor"),

            // shl: ( a b -- a<<b ) shift left, returns 0 for shift >= 64 or negative
            "shl" => self.codegen_inline_shift(stack_var, true),

            // shr: ( a b -- a>>b ) logical shift right, returns 0 for shift >= 64 or negative
            "shr" => self.codegen_inline_shift(stack_var, false),

            // bnot: ( a -- ~a ) bitwise NOT
            "bnot" => {
                // Get pointer to top Value
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;

                // Get pointer to slot1 (value at offset 8)
                let slot1_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    slot1_ptr, top_ptr
                )?;

                // Load value from slot1
                let val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    val, slot1_ptr
                )?;

                // Bitwise NOT: XOR with -1 (all 1s)
                let not_result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, -1", not_result, val)?;

                // Store result (discriminant stays 0 for Int)
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    not_result, slot1_ptr
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // popcount: ( a -- count ) count number of 1 bits
            "popcount" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctpop.i64"),

            // clz: ( a -- count ) count leading zeros
            "clz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctlz.i64"),

            // ctz: ( a -- count ) count trailing zeros
            "ctz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.cttz.i64"),

            // More stack operations
            // rot: ( a b c -- b c a )
            "rot" => {
                let ptr_c = self.fresh_temp();
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_c, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -3",
                    ptr_a, stack_var
                )?;

                // Load full Values (40 bytes each)
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_c, ptr_c
                )?;

                // Rotate: a goes to top, b goes to a's position, c goes to b's position
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_c, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_c
                )?;

                Ok(Some(stack_var.to_string()))
            }

            // nip: ( a b -- b )
            // Must call runtime to properly drop the removed value
            "nip" => {
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_nip(ptr %{})",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // tuck: ( a b -- b a b )
            // Uses patch_seq_clone_value to properly clone heap values
            "tuck" => {
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let result_var = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Load full Values
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                // Clone b to the new top position
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, stack_var
                )?;

                // Result: b a b (a's slot gets b, b's slot gets a, new slot gets b_clone)
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_b
                )?;

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // 2dup: ( a b -- a b a b )
            // Uses patch_seq_clone_value to properly clone heap values
            "2dup" => {
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let new_ptr = self.fresh_temp();
                let result_var = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Clone a to stack_var
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Clone b to stack_var + 1
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    new_ptr, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, new_ptr
                )?;

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 2",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // 3drop: ( a b c -- )
            // Must call runtime to properly drop heap values
            "3drop" => {
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_3drop(ptr %{})",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // pick: ( ... xn ... x1 x0 n -- ... xn ... x1 x0 xn )
            // Copy the nth item (0-indexed from below n) to top
            // Uses patch_seq_clone_value to properly clone heap values
            "pick" => {
                // Get pointer to n (top of stack)
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load n from slot1
                let n_slot1 = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    n_slot1, n_ptr
                )?;
                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    n_val, n_slot1
                )?;

                // Calculate offset: -(n + 2) from stack_var
                // After popping n, x0 is at -1, x1 at -2, xn at -(n+1)
                // But we're indexing from stack_var, so xn is at -(n+2)
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 2", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to the item to copy
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                    src_ptr, stack_var, neg_offset
                )?;

                // Clone the value from src_ptr to n_ptr (replacing n with the picked value)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    src_ptr, n_ptr
                )?;

                // SP unchanged (we replaced n with the picked value)
                Ok(Some(stack_var.to_string()))
            }

            // roll: ( ... xn xn-1 ... x1 x0 n -- ... xn-1 ... x1 x0 xn )
            // Move the nth item to top, shifting others down
            "roll" => {
                // Get pointer to n (top of stack)
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load n from slot1
                let n_slot1 = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    n_slot1, n_ptr
                )?;
                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    n_val, n_slot1
                )?;

                // Pop n first - new SP is stack_var - 1
                let popped_sp = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    popped_sp, stack_var
                )?;

                // Calculate offset to the item to roll: -(n + 1) from popped_sp
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 1", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to the item to roll (xn)
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                    src_ptr, popped_sp, neg_offset
                )?;

                // Load the value to roll
                let rolled_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    rolled_val, src_ptr
                )?;

                // Use memmove to shift items down (from src+1 to src, n items)
                // memmove(dest, src, size) - dest is src_ptr, src is src_ptr+1
                let src_plus_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    src_plus_one, src_ptr
                )?;

                // Size in bytes = n * 40 (sizeof %Value)
                let size_bytes = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = mul i64 %{}, 40",
                    size_bytes, n_val
                )?;

                // Call memmove
                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 %{}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top (popped_sp - 1, which is where x0 was)
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    rolled_val, top_ptr
                )?;

                // SP = popped_sp (we removed n, rolled doesn't change count)
                Ok(Some(popped_sp))
            }

            // Not an inline-able operation
            _ => Ok(None),
        }
    }

    /// Generate inline code for comparison operations.
    /// Returns Value::Bool (discriminant 2 at slot0, 0/1 at slot1).
    fn codegen_inline_comparison(
        &mut self,
        stack_var: &str,
        icmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values are at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Compare
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp {} i64 %{}, %{}",
            cmp_result, icmp_op, val_a, val_b
        )?;

        // Convert i1 to i64
        let zext = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            zext, cmp_result
        )?;

        // Store result as Value::Bool (discriminant 2 at slot0, 0/1 at slot1)
        writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
        writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for binary arithmetic (add/subtract).
    /// Values are stored in slot1 of each Value (slot0 is discriminant 0 for Int).
    fn codegen_inline_binary_op(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
        _adjust_op: &str, // No longer needed, kept for compatibility
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get pointers to slot1 (actual value, offset 8 bytes from Value start)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load actual values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Perform the operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result: discriminant 0 at slot0, result at slot1
        // ptr_a already has discriminant 0 from the original push, so we only need to update slot1
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for float binary operations (f.add, f.subtract, etc.)
    /// Values are stored as f64 bits in slot1, discriminant 1 (Float).
    fn codegen_inline_float_binary_op(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1 as i64 (raw bits)
        let bits_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_a, slot1_a
        )?;
        let bits_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_b, slot1_b
        )?;

        // Bitcast to double
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_a, bits_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_b, bits_b
        )?;

        // Perform the float operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} double %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Bitcast result back to i64
        let result_bits = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast double %{} to i64",
            result_bits, op_result
        )?;

        // Store result at slot1 (discriminant 1 already at slot0)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result_bits, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for float comparison operations.
    /// Returns Value::Bool (discriminant 2 at slot0, 0/1 at slot1).
    fn codegen_inline_float_comparison(
        &mut self,
        stack_var: &str,
        fcmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1 as i64 (raw bits)
        let bits_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_a, slot1_a
        )?;
        let bits_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_b, slot1_b
        )?;

        // Bitcast to double
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_a, bits_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_b, bits_b
        )?;

        // Compare using fcmp
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = fcmp {} double %{}, %{}",
            cmp_result, fcmp_op, val_a, val_b
        )?;

        // Convert i1 to i64
        let zext = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            zext, cmp_result
        )?;

        // Store result as Value::Bool (discriminant 2 at slot0, 0/1 at slot1)
        writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
        writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for integer bitwise binary operations.
    /// Returns tagged int (discriminant 0).
    fn codegen_inline_int_bitwise_binary(
        &mut self,
        stack_var: &str,
        llvm_op: &str, // "and", "or", "xor"
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Perform the bitwise operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result (discriminant stays 0 for Int, just update slot1)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for shift operations with proper edge case handling.
    /// Matches runtime behavior: returns 0 for negative shift or shift >= 64.
    /// For shr, uses logical (not arithmetic) shift to match runtime.
    fn codegen_inline_shift(
        &mut self,
        stack_var: &str,
        is_left: bool, // true for shl, false for shr
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointers to Value slots (b = shift count, a = value to shift)
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1 (val_a = value to shift, val_b = shift count)
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Check if shift count is negative
        let is_neg = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp slt i64 %{}, 0",
            is_neg, val_b
        )?;

        // Check if shift count >= 64
        let is_overflow = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, 64",
            is_overflow, val_b
        )?;

        // Combine: is_invalid = is_neg OR is_overflow
        let is_invalid = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i1 %{}, %{}",
            is_invalid, is_neg, is_overflow
        )?;

        // Use a safe shift count (clamped to 0 if invalid) to avoid LLVM UB
        let safe_count = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            safe_count, is_invalid, val_b
        )?;

        // Perform the shift operation with safe count
        let shift_result = self.fresh_temp();
        let op = if is_left { "shl" } else { "lshr" };
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            shift_result, op, val_a, safe_count
        )?;

        // Select final result: 0 if invalid, otherwise shift_result
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            op_result, is_invalid, shift_result
        )?;

        // Store result (discriminant stays 0 for Int, just update slot1)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for integer unary intrinsic operations.
    /// Used for popcount, clz, ctz which use LLVM intrinsics.
    fn codegen_inline_int_unary_intrinsic(
        &mut self,
        stack_var: &str,
        intrinsic: &str, // "llvm.ctpop.i64", "llvm.ctlz.i64", "llvm.cttz.i64"
    ) -> Result<Option<String>, CodeGenError> {
        // Get pointer to top Value
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;

        // Get pointer to slot1 (value at offset 8)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;

        // Load value from slot1
        let val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val, slot1_ptr
        )?;

        // Call the intrinsic
        let result = self.fresh_temp();
        if intrinsic == "llvm.ctpop.i64" {
            writeln!(
                &mut self.output,
                "  %{} = call i64 @{}(i64 %{})",
                result, intrinsic, val
            )?;
        } else {
            // clz and ctz have a second parameter: is_poison_on_zero (false)
            writeln!(
                &mut self.output,
                "  %{} = call i64 @{}(i64 %{}, i1 false)",
                result, intrinsic, val
            )?;
        }

        // Store result (discriminant stays 0 for Int)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result, slot1_ptr
        )?;

        // SP unchanged
        Ok(Some(stack_var.to_string()))
    }

    /// Generate inline code for `while` loop: [cond] [body] while
    ///
    /// LLVM structure:
    /// ```text
    /// while_cond:
    ///   <execute cond_body>
    ///   %cond = load condition from stack
    ///   %sp = pop condition
    ///   br i1 %cond, label %while_body, label %while_end
    /// while_body:
    ///   <execute loop_body>
    ///   br label %while_cond
    /// while_end:
    ///   ...
    /// ```
    fn codegen_inline_while(
        &mut self,
        stack_var: &str,
        cond_body: &[Statement],
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        let cond_block = self.fresh_block("while_cond");
        let body_block = self.fresh_block("while_body");
        let end_block = self.fresh_block("while_end");

        // Use named variables for phi nodes to avoid SSA ordering issues
        let loop_stack_phi = format!("{}_stack", cond_block);
        let loop_stack_next = format!("{}_stack_next", cond_block);

        // Jump to condition check
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for stack pointer at loop entry
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}, %{}_end ]",
            loop_stack_phi, stack_var, loop_stack_next, body_block
        )?;

        // Execute condition body
        let cond_stack = self.codegen_statements(cond_body, &loop_stack_phi, false)?;

        // Inline peek and pop for condition
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, cond_stack
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let cond_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            cond_val, slot1_ptr
        )?;
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_stack, cond_stack
        )?;

        // Branch on condition
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cond_bool, cond_val
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &popped_stack, false)?;

        // Create landing block for phi node
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;

        // Store result for phi and loop back
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(popped_stack)
    }

    /// Generate inline code for `until` loop: [cond] [body] until
    ///
    /// Like while but executes body first, then checks condition.
    /// Continues until condition is TRUE (opposite of while).
    fn codegen_inline_until(
        &mut self,
        stack_var: &str,
        cond_body: &[Statement],
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        let body_block = self.fresh_block("until_body");
        let cond_block = self.fresh_block("until_cond");
        let end_block = self.fresh_block("until_end");

        // Use named variables for phi nodes to avoid SSA ordering issues
        let loop_stack_phi = format!("{}_stack", body_block);
        let loop_stack_next = format!("{}_stack_next", body_block);

        // Jump to body (do-while style)
        writeln!(&mut self.output, "  br label %{}", body_block)?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;

        // Phi for stack pointer at loop entry
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}, %{}_end ]",
            loop_stack_phi, stack_var, loop_stack_next, cond_block
        )?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack_phi, false)?;

        // Jump to condition
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Execute condition body
        let cond_stack = self.codegen_statements(cond_body, &body_end_stack, false)?;

        // Inline peek and pop for condition
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, cond_stack
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let cond_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            cond_val, slot1_ptr
        )?;
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_stack, cond_stack
        )?;

        // Create landing block for phi
        let cond_end_block = format!("{}_end", cond_block);
        writeln!(&mut self.output, "  br label %{}", cond_end_block)?;
        writeln!(&mut self.output, "{}:", cond_end_block)?;

        // Store result for phi
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, popped_stack
        )?;

        // Branch: if condition is TRUE, exit; if FALSE, continue loop
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cond_bool, cond_val
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, end_block, body_block
        )?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(popped_stack)
    }

    /// Generate inline code for `times` loop: n [body] times
    ///
    /// Pops count from stack, executes body that many times.
    #[allow(dead_code)] // Reserved for future dynamic count support
    fn codegen_inline_times(
        &mut self,
        stack_var: &str,
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        let cond_block = self.fresh_block("times_cond");
        let body_block = self.fresh_block("times_body");
        let end_block = self.fresh_block("times_end");

        // Pop count from stack (it was pushed before the quotation)
        // Actually, the quotation is at top, count is below it
        // But in our pattern, we detected [body] times, so count is already on stack
        // We need to pop the count that's on the stack
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let count_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            count_val, slot1_ptr
        )?;
        let init_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            init_stack, stack_var
        )?;

        // Jump to condition
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for counter and stack
        let counter = self.fresh_temp();
        let loop_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = phi i64 [ %{}, %entry ], [ %{}_next, %{}_end ]",
            counter, count_val, counter, body_block
        )?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}_body_end, %{}_end ]",
            loop_stack, init_stack, body_block, body_block
        )?;

        // Check if counter > 0
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sgt i64 %{}, 0",
            cond_bool, counter
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack, false)?;

        // Create landing block
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;

        // Decrement counter and store for phi
        writeln!(
            &mut self.output,
            "  %{}_next = sub i64 %{}, 1",
            counter, counter
        )?;
        writeln!(
            &mut self.output,
            "  %{}_body_end = getelementptr i8, ptr %{}, i64 0",
            body_block, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(loop_stack)
    }

    /// Generate inline code for `times` loop with literal count: [body] n times
    ///
    /// The count is known at compile time, so we don't need to pop it from stack.
    fn codegen_inline_times_literal(
        &mut self,
        stack_var: &str,
        loop_body: &[Statement],
        count: i64,
    ) -> Result<String, CodeGenError> {
        // If count is 0 or negative, skip the loop entirely
        if count <= 0 {
            return Ok(stack_var.to_string());
        }

        let cond_block = self.fresh_block("times_cond");
        let body_block = self.fresh_block("times_body");
        let end_block = self.fresh_block("times_end");

        // Use named variables for phi nodes to avoid SSA ordering issues
        let counter_phi = format!("{}_counter", cond_block);
        let counter_next = format!("{}_counter_next", cond_block);
        let loop_stack_phi = format!("{}_stack", cond_block);
        let loop_stack_next = format!("{}_stack_next", cond_block);

        // Jump to condition
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for counter and stack
        writeln!(
            &mut self.output,
            "  %{} = phi i64 [ {}, %entry ], [ %{}, %{}_end ]",
            counter_phi, count, counter_next, body_block
        )?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}, %{}_end ]",
            loop_stack_phi, stack_var, loop_stack_next, body_block
        )?;

        // Check if counter > 0
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sgt i64 %{}, 0",
            cond_bool, counter_phi
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack_phi, false)?;

        // Create landing block
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;

        // Decrement counter and create stack alias for phi
        writeln!(
            &mut self.output,
            "  %{} = sub i64 %{}, 1",
            counter_next, counter_phi
        )?;
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(loop_stack_phi)
    }

    /// Generate code for a word call
    ///
    /// Handles builtin functions, external builtins, and user-defined words.
    /// Emits tail calls when appropriate.
    fn codegen_word_call(
        &mut self,
        stack_var: &str,
        name: &str,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Inline operations for common stack/arithmetic ops
        if let Some(result) = self.try_codegen_inline_op(stack_var, name)? {
            return Ok(result);
        }

        let result_var = self.fresh_temp();

        // Phase 2 TCO: Special handling for `call` in tail position
        // Not available in main/quotation (C convention can't musttail to tailcc)
        if name == "call"
            && position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
        {
            return self.codegen_tail_call_quotation(stack_var, &result_var);
        }

        // Map source-level word names to runtime function names
        let (function_name, is_seq_word) = if let Some(&symbol) = BUILTIN_SYMBOLS.get(name) {
            (symbol.to_string(), false)
        } else if let Some(symbol) = self.external_builtins.get(name) {
            (symbol.clone(), false)
        } else if self.ffi_bindings.is_ffi_function(name) {
            // FFI wrapper function
            (format!("seq_ffi_{}", mangle_name(name)), false)
        } else {
            (format!("seq_{}", mangle_name(name)), true)
        };

        // Emit tail call for user-defined words in tail position
        // Not available in main/quotation (C convention can't musttail to tailcc)
        let can_tail_call = position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
            && is_seq_word;
        if can_tail_call {
            // Yield check before tail call to prevent starvation in tight loops
            writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
            writeln!(
                &mut self.output,
                "  %{} = musttail call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
            writeln!(&mut self.output, "  ret ptr %{}", result_var)?;
        } else if is_seq_word {
            // Non-tail call to user-defined word: must use tailcc calling convention
            writeln!(
                &mut self.output,
                "  %{} = call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
        } else {
            // Call to builtin (C calling convention)
            writeln!(
                &mut self.output,
                "  %{} = call ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
        }
        Ok(result_var)
    }

    /// Generate code for an if statement with optional else branch
    ///
    /// Handles phi node merging for branches with different control flow.
    fn codegen_if_statement(
        &mut self,
        stack_var: &str,
        then_branch: &[Statement],
        else_branch: Option<&Vec<Statement>>,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Peek the condition value, then pop (inline)
        // Get pointer to top Value (at SP-1)
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;

        // Get pointer to slot1 (value at offset 8)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;

        // Load condition value from slot1
        let cond_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            cond_temp, slot1_ptr
        )?;

        // Pop: SP = SP - 1
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_stack, stack_var
        )?;

        // Compare with 0 (0 = false, non-zero = true)
        let cmp_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp_temp, cond_temp
        )?;

        // Generate unique block labels
        let then_block = self.fresh_block("if_then");
        let else_block = self.fresh_block("if_else");
        let merge_block = self.fresh_block("if_merge");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_temp, then_block, else_block
        )?;

        // Then branch
        writeln!(&mut self.output, "{}:", then_block)?;
        let then_result = self.codegen_branch(
            then_branch,
            &popped_stack,
            position,
            &merge_block,
            "if_then",
        )?;

        // Else branch
        writeln!(&mut self.output, "{}:", else_block)?;
        let else_result = if let Some(eb) = else_branch {
            self.codegen_branch(eb, &popped_stack, position, &merge_block, "if_else")?
        } else {
            // No else clause - emit landing block with unchanged stack
            let else_pred = self.fresh_block("if_else_end");
            writeln!(&mut self.output, "  br label %{}", else_pred)?;
            writeln!(&mut self.output, "{}:", else_pred)?;
            writeln!(&mut self.output, "  br label %{}", merge_block)?;
            BranchResult {
                stack_var: popped_stack.clone(),
                emitted_tail_call: false,
                predecessor: else_pred,
            }
        };

        // If both branches emitted tail calls, no merge needed
        if then_result.emitted_tail_call && else_result.emitted_tail_call {
            return Ok(then_result.stack_var);
        }

        // Merge block with phi node
        writeln!(&mut self.output, "{}:", merge_block)?;
        let result_var = self.fresh_temp();

        if then_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, else_result.stack_var, else_result.predecessor
            )?;
        } else if else_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, then_result.stack_var, then_result.predecessor
            )?;
        } else {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{} ]",
                result_var,
                then_result.stack_var,
                then_result.predecessor,
                else_result.stack_var,
                else_result.predecessor
            )?;
        }

        Ok(result_var)
    }

    /// Generate code for a match expression (pattern matching on union types)
    ///
    /// Match expressions use symbol-based tags (for SON support):
    /// 1. Get the variant's tag as a Symbol
    /// 2. Compare with each arm's variant name using string comparison
    /// 3. Jump to the matching arm using cascading if-else
    /// 4. In each arm, unpack the variant's fields onto the stack
    /// 5. Execute the arm's body
    /// 6. Merge control flow at the end
    fn codegen_match_statement(
        &mut self,
        stack_var: &str,
        arms: &[MatchArm],
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Step 0: Check exhaustiveness
        if let Err((union_name, missing)) = self.check_match_exhaustiveness(arms) {
            return Err(CodeGenError::Logic(format!(
                "Non-exhaustive match on union '{}'. Missing variants: {}",
                union_name,
                missing.join(", ")
            )));
        }

        // Step 1: Duplicate the variant so we can get the tag without consuming it
        let dup_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_dup(ptr %{})",
            dup_stack, stack_var
        )?;

        // Step 2: Call variant-tag on the duplicate to get the tag as Symbol
        let tagged_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_variant_tag(ptr %{})",
            tagged_stack, dup_stack
        )?;

        // Now tagged_stack has the symbol tag on top, original variant below

        // Step 3: Prepare for cascading if-else pattern matching
        let default_block = self.fresh_block("match_unreachable");
        let merge_block = self.fresh_block("match_merge");

        // Collect arm info: (variant_name, block_name, field_count, field_names)
        let mut arm_info: Vec<(String, String, usize, Vec<String>)> = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let block = self.fresh_block(&format!("match_arm_{}", i));
            let variant_name = match &arm.pattern {
                Pattern::Variant(name) => name.clone(),
                Pattern::VariantWithBindings { name, .. } => name.clone(),
            };
            let (_tag, field_count, field_names) = self.find_variant_info(&variant_name)?;
            arm_info.push((variant_name, block, field_count, field_names));
        }

        // Step 4: Generate cascading if-else for each arm
        // We need to preserve the stack with symbol on top for each comparison
        let mut current_tag_stack = tagged_stack.clone();
        for (i, (variant_name, arm_block, _, _)) in arm_info.iter().enumerate() {
            let is_last = i == arm_info.len() - 1;
            let next_check = if is_last {
                default_block.clone()
            } else {
                self.fresh_block(&format!("match_check_{}", i + 1))
            };

            // For all but last arm: dup the tag, compare, branch
            // For last arm: just compare (tag will be consumed)
            let compare_stack = if !is_last {
                let dup = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_dup(ptr %{})",
                    dup, current_tag_stack
                )?;
                dup
            } else {
                current_tag_stack.clone()
            };

            // Create string constant for variant name
            let str_const = self.get_string_global(variant_name)?;

            // Compare symbol with C string
            let cmp_stack = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_symbol_eq_cstr(ptr %{}, ptr {})",
                cmp_stack, compare_stack, str_const
            )?;

            // Peek the bool result
            let cmp_val = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call i1 @patch_seq_peek_bool_value(ptr %{})",
                cmp_val, cmp_stack
            )?;

            // Pop the bool and update stack for next iteration
            let popped = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
                popped, cmp_stack
            )?;

            // Branch: if true goto arm, else continue checking
            writeln!(
                &mut self.output,
                "  br i1 %{}, label %{}, label %{}",
                cmp_val, arm_block, next_check
            )?;

            // Start next check block (unless this was the last arm)
            if !is_last {
                writeln!(&mut self.output, "{}:", next_check)?;
                // Update current_tag_stack to the popped version for next iteration
                current_tag_stack = popped;
            }
        }

        // Step 5: Generate unreachable default block (should never reach for exhaustive match)
        writeln!(&mut self.output, "{}:", default_block)?;
        writeln!(&mut self.output, "  unreachable")?;

        // Step 6: Generate each match arm
        // We use the original stack_var which still has the variant
        let mut arm_results: Vec<BranchResult> = Vec::new();
        for (i, (arm, (_variant_name, block, field_count, field_names))) in
            arms.iter().zip(arm_info.iter()).enumerate()
        {
            writeln!(&mut self.output, "{}:", block)?;

            // Extract fields based on pattern type
            let unpacked_stack = match &arm.pattern {
                Pattern::Variant(_) => {
                    // Stack-based: unpack all fields in declaration order
                    let result = self.fresh_temp();
                    writeln!(
                        &mut self.output,
                        "  %{} = call ptr @patch_seq_unpack_variant(ptr %{}, i64 {})",
                        result, stack_var, field_count
                    )?;
                    result
                }
                Pattern::VariantWithBindings { bindings, .. } => {
                    // Named bindings: extract only bound fields using variant_field_at
                    // variant_field_at expects: ( variant index -- field_value )
                    //
                    // Algorithm for bindings [a, b, c]:
                    // - For each binding except last: dup, push idx, field_at, swap
                    // - For last binding: push idx, field_at
                    // This leaves fields in binding order: ( a b c )

                    if bindings.is_empty() {
                        // No bindings: just drop the variant
                        let drop_stack = self.fresh_temp();
                        writeln!(
                            &mut self.output,
                            "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                            drop_stack, stack_var
                        )?;
                        drop_stack
                    } else {
                        let mut current_stack = stack_var.to_string();
                        let is_last = |idx: usize| idx == bindings.len() - 1;

                        for (bind_idx, binding) in bindings.iter().enumerate() {
                            // Find the field index for this binding
                            let field_idx = field_names
                                .iter()
                                .position(|f| f == binding)
                                .expect("binding validation should have caught unknown field");

                            if !is_last(bind_idx) {
                                // Not the last binding: dup, push idx, field_at, swap
                                let dup_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_dup(ptr %{})",
                                    dup_stack, current_stack
                                )?;

                                let idx_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
                                    idx_stack, dup_stack, field_idx
                                )?;

                                let field_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_variant_field_at(ptr %{})",
                                    field_stack, idx_stack
                                )?;

                                // Swap to get variant back on top: ( field variant )
                                let swap_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_swap(ptr %{})",
                                    swap_stack, field_stack
                                )?;

                                current_stack = swap_stack;
                            } else {
                                // Last binding: push idx, field_at
                                let idx_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
                                    idx_stack, current_stack, field_idx
                                )?;

                                let field_stack = self.fresh_temp();
                                writeln!(
                                    &mut self.output,
                                    "  %{} = call ptr @patch_seq_variant_field_at(ptr %{})",
                                    field_stack, idx_stack
                                )?;

                                current_stack = field_stack;
                            }
                        }

                        current_stack
                    }
                }
            };

            // Generate the arm body
            let result = self.codegen_branch(
                &arm.body,
                &unpacked_stack,
                position,
                &merge_block,
                &format!("match_arm_{}", i),
            )?;
            arm_results.push(result);
        }

        // Step 7: Generate merge block with phi node
        // Check if all arms emitted tail calls
        let all_tail_calls = arm_results.iter().all(|r| r.emitted_tail_call);
        if all_tail_calls {
            // All branches tail-called, no merge needed
            // Return any stack var (won't be used)
            return Ok(arm_results[0].stack_var.clone());
        }

        writeln!(&mut self.output, "{}:", merge_block)?;
        let result_var = self.fresh_temp();

        // Build phi node from non-tail-call branches
        let phi_entries: Vec<_> = arm_results
            .iter()
            .filter(|r| !r.emitted_tail_call)
            .map(|r| format!("[ %{}, %{} ]", r.stack_var, r.predecessor))
            .collect();

        if phi_entries.is_empty() {
            // Shouldn't happen if not all_tail_calls
            return Err(CodeGenError::Logic(
                "Match codegen: unexpected empty phi".to_string(),
            ));
        }

        writeln!(
            &mut self.output,
            "  %{} = phi ptr {}",
            result_var,
            phi_entries.join(", ")
        )?;

        Ok(result_var)
    }

    /// Generate code for pushing a quotation or closure onto the stack
    fn codegen_quotation_push(
        &mut self,
        stack_var: &str,
        id: usize,
        body: &[Statement],
    ) -> Result<String, CodeGenError> {
        let quot_type = self.get_quotation_type(id)?.clone();
        let fns = self.codegen_quotation(body, &quot_type)?;

        match quot_type {
            Type::Quotation(_) => {
                // Get both wrapper and impl function pointers as i64
                let wrapper_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    wrapper_ptr_var, fns.wrapper
                )?;

                let impl_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    impl_ptr_var, fns.impl_
                )?;

                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_quotation(ptr %{}, i64 %{}, i64 %{})",
                    result_var, stack_var, wrapper_ptr_var, impl_ptr_var
                )?;
                Ok(result_var)
            }
            Type::Closure { captures, .. } => {
                // For closures, just use the single function pointer (no TCO yet)
                let fn_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    fn_ptr_var, fns.wrapper
                )?;

                let capture_count = i32::try_from(captures.len()).map_err(|_| {
                    format!(
                        "Closure has too many captures ({}) - maximum is {}",
                        captures.len(),
                        i32::MAX
                    )
                })?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_closure(ptr %{}, i64 %{}, i32 {})",
                    result_var, stack_var, fn_ptr_var, capture_count
                )?;
                Ok(result_var)
            }
            _ => Err(CodeGenError::Logic(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            ))),
        }
    }

    // =========================================================================
    // Main Statement Dispatcher
    // =========================================================================

    /// Generate code for a sequence of statements with pattern detection.
    ///
    /// Detects patterns like `[cond] [body] while` and emits inline loops
    /// instead of quotation push + FFI call.
    ///
    /// Returns the final stack variable name.
    fn codegen_statements(
        &mut self,
        statements: &[Statement],
        initial_stack_var: &str,
        last_is_tail: bool,
    ) -> Result<String, CodeGenError> {
        let mut stack_var = initial_stack_var.to_string();
        let len = statements.len();
        let mut i = 0;

        while i < len {
            let is_last = i == len - 1;
            let position = if is_last && last_is_tail {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };

            // Pattern: [cond] [body] while  or  [body] [cond] until
            // Stack order: first quotation pushed is below second
            // For while: condition is pushed first, body second → [cond] [body] while
            // For until: body is pushed first, condition second → [body] [cond] until
            if i + 2 < len
                && let (
                    Statement::Quotation {
                        body: first_quot, ..
                    },
                    Statement::Quotation {
                        body: second_quot, ..
                    },
                    Statement::WordCall { name, .. },
                ) = (&statements[i], &statements[i + 1], &statements[i + 2])
            {
                if name == "while" {
                    // while: [cond] [body] - first is cond, second is body
                    stack_var = self.codegen_inline_while(&stack_var, first_quot, second_quot)?;
                    i += 3;
                    continue;
                }
                if name == "until" {
                    // until: [body] [cond] - first is body, second is cond
                    stack_var = self.codegen_inline_until(&stack_var, second_quot, first_quot)?;
                    i += 3;
                    continue;
                }
            }

            // Pattern: [body] count times
            // Stack order: quotation pushed first, then count, then times called
            // Statement pattern: Quotation, IntLiteral, WordCall("times")
            if i + 2 < len
                && let (
                    Statement::Quotation {
                        body: loop_body, ..
                    },
                    Statement::IntLiteral(count),
                    Statement::WordCall { name, .. },
                ) = (&statements[i], &statements[i + 1], &statements[i + 2])
                && name == "times"
            {
                stack_var = self.codegen_inline_times_literal(&stack_var, loop_body, *count)?;
                i += 3;
                continue;
            }

            // Regular statement processing
            stack_var = self.codegen_statement(&stack_var, &statements[i], position)?;
            i += 1;
        }

        Ok(stack_var)
    }

    /// Generate code for a single statement
    ///
    /// The `position` parameter indicates whether this statement is in tail position.
    /// For tail calls, we emit `musttail call` followed by `ret` to guarantee TCO.
    fn codegen_statement(
        &mut self,
        stack_var: &str,
        statement: &Statement,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        match statement {
            Statement::IntLiteral(n) => self.codegen_int_literal(stack_var, *n),
            Statement::FloatLiteral(f) => self.codegen_float_literal(stack_var, *f),
            Statement::BoolLiteral(b) => self.codegen_bool_literal(stack_var, *b),
            Statement::StringLiteral(s) => self.codegen_string_literal(stack_var, s),
            Statement::Symbol(s) => self.codegen_symbol_literal(stack_var, s),
            Statement::WordCall { name, .. } => self.codegen_word_call(stack_var, name, position),
            Statement::If {
                then_branch,
                else_branch,
            } => self.codegen_if_statement(stack_var, then_branch, else_branch.as_ref(), position),
            Statement::Quotation { id, body } => self.codegen_quotation_push(stack_var, *id, body),
            Statement::Match { arms } => self.codegen_match_statement(stack_var, arms, position),
        }
    }

    /// Generate main function that calls user's main word
    fn codegen_main(&mut self) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.output,
            "define i32 @main(i32 %argc, ptr %argv) {{"
        )?;
        writeln!(&mut self.output, "entry:")?;

        if self.pure_inline_test {
            // Pure inline test mode: no scheduler, just run the code directly
            // and return the top of stack as exit code.
            //
            // This mode is for testing pure integer programs that use only
            // inlined operations (push_int, arithmetic, stack ops).

            // Allocate tagged stack
            writeln!(
                &mut self.output,
                "  %tagged_stack = call ptr @seq_stack_new_default()"
            )?;
            writeln!(
                &mut self.output,
                "  %stack_base = call ptr @seq_stack_base(ptr %tagged_stack)"
            )?;

            // Call seq_main which returns the final stack pointer
            writeln!(
                &mut self.output,
                "  %final_sp = call ptr @seq_main(ptr %stack_base)"
            )?;

            // Read top of stack value (at sp - 1, slot1 contains the int value)
            writeln!(
                &mut self.output,
                "  %top_ptr = getelementptr %Value, ptr %final_sp, i64 -1"
            )?;
            writeln!(
                &mut self.output,
                "  %val_ptr = getelementptr i64, ptr %top_ptr, i64 1"
            )?;
            writeln!(&mut self.output, "  %result = load i64, ptr %val_ptr")?;

            // Free the stack
            writeln!(
                &mut self.output,
                "  call void @seq_stack_free(ptr %tagged_stack)"
            )?;

            // Return result as exit code (truncate to i32)
            writeln!(&mut self.output, "  %exit_code = trunc i64 %result to i32")?;
            writeln!(&mut self.output, "  ret i32 %exit_code")?;
        } else {
            // Normal mode: use scheduler for concurrency support

            // Initialize command-line arguments (before scheduler so args are available)
            writeln!(
                &mut self.output,
                "  call void @patch_seq_args_init(i32 %argc, ptr %argv)"
            )?;

            // Initialize scheduler
            writeln!(&mut self.output, "  call void @patch_seq_scheduler_init()")?;

            // Spawn user's main function as the first strand
            // This ensures all code runs in coroutine context for non-blocking I/O
            writeln!(
                &mut self.output,
                "  %0 = call i64 @patch_seq_strand_spawn(ptr @seq_main, ptr null)"
            )?;

            // Wait for all spawned strands to complete (including main)
            writeln!(
                &mut self.output,
                "  %1 = call ptr @patch_seq_scheduler_run()"
            )?;

            writeln!(&mut self.output, "  ret i32 0")?;
        }
        writeln!(&mut self.output, "}}")?;

        Ok(())
    }
}

impl Default for CodeGen {
    fn default() -> Self {
        Self::new()
    }
}

/// Get the target triple for the current platform
fn get_target_triple() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "arm64-apple-macosx14.0.0"
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64")
    )))]
    {
        "unknown"
    }
}

// ============================================================================
// FFI Helper Functions
// ============================================================================

use crate::ffi::{FfiArg, FfiReturn};

/// Get the LLVM IR return type for an FFI function
fn ffi_return_type(return_spec: &Option<FfiReturn>) -> &'static str {
    match return_spec {
        Some(spec) => match spec.return_type {
            FfiType::Int => "i64",
            FfiType::String => "ptr",
            FfiType::Ptr => "ptr",
            FfiType::Void => "void",
        },
        None => "void",
    }
}

/// Get the LLVM IR argument types for an FFI function
fn ffi_c_args(args: &[FfiArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    args.iter()
        .map(|arg| match arg.arg_type {
            FfiType::Int => "i64",
            FfiType::String => "ptr",
            FfiType::Ptr => "ptr",
            FfiType::Void => "void",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Program, Statement, WordDef};

    #[test]
    fn test_codegen_hello_world() {
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("Hello, World!".to_string()),
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        assert!(ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
        // main uses C calling convention (no tailcc) since it's called from C runtime
        assert!(ir.contains("define ptr @seq_main(ptr %stack)"));
        assert!(ir.contains("call ptr @patch_seq_push_string"));
        assert!(ir.contains("call ptr @patch_seq_write_line"));
        assert!(ir.contains("\"Hello, World!\\00\""));
    }

    #[test]
    fn test_codegen_io_write() {
        // Test io.write (write without newline)
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("no newline".to_string()),
                    Statement::WordCall {
                        name: "io.write".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        assert!(ir.contains("call ptr @patch_seq_push_string"));
        assert!(ir.contains("call ptr @patch_seq_write"));
        assert!(ir.contains("\"no newline\\00\""));
    }

    #[test]
    fn test_codegen_arithmetic() {
        // Test inline tagged stack arithmetic
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        // With tagged stack, integers are pushed inline (store discriminant + value)
        assert!(ir.contains("store i64 0")); // Int discriminant
        assert!(ir.contains("store i64 2")); // Push int 2
        assert!(ir.contains("store i64 3")); // Push int 3
        // Add is inlined (load, add, store pattern)
        assert!(ir.contains("add i64"));
    }

    #[test]
    fn test_pure_inline_test_mode() {
        let mut codegen = CodeGen::new_pure_inline_test();

        // Simple program: 5 3 add (should return 8)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(5),
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        // Pure inline test mode should:
        // 1. NOT CALL the scheduler (declarations are ok, calls are not)
        assert!(!ir.contains("call void @patch_seq_scheduler_init"));
        assert!(!ir.contains("call i64 @patch_seq_strand_spawn"));

        // 2. Have main allocate tagged stack and call seq_main directly
        assert!(ir.contains("call ptr @seq_stack_new_default()"));
        assert!(ir.contains("call ptr @seq_main(ptr %stack_base)"));

        // 3. Read result from stack and return as exit code
        assert!(ir.contains("trunc i64 %result to i32"));
        assert!(ir.contains("ret i32 %exit_code"));

        // 4. Use inline push (store i64 0, not call patch_seq_push_int)
        assert!(!ir.contains("call ptr @patch_seq_push_int"));
        assert!(ir.contains("store i64 0, ptr %stack")); // Int discriminant

        // 5. Use inline add (add i64, not call patch_seq_add)
        assert!(!ir.contains("call ptr @patch_seq_add"));
        assert!(ir.contains("add i64"));
    }

    #[test]
    fn test_escape_llvm_string() {
        assert_eq!(CodeGen::escape_llvm_string("hello").unwrap(), "hello");
        assert_eq!(CodeGen::escape_llvm_string("a\nb").unwrap(), r"a\0Ab");
        assert_eq!(CodeGen::escape_llvm_string("a\tb").unwrap(), r"a\09b");
        assert_eq!(CodeGen::escape_llvm_string("a\"b").unwrap(), r"a\22b");
    }

    #[test]
    fn test_external_builtins_declared() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::WordCall {
                        name: "my-external-op".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("my-external-op", "test_runtime_my_op"));

        let ir = codegen
            .codegen_program_with_config(&program, HashMap::new(), &config)
            .unwrap();

        // Should declare the external builtin
        assert!(
            ir.contains("declare ptr @test_runtime_my_op(ptr)"),
            "IR should declare external builtin"
        );

        // Should call the external builtin
        assert!(
            ir.contains("call ptr @test_runtime_my_op"),
            "IR should call external builtin"
        );
    }

    #[test]
    fn test_multiple_external_builtins() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "actor-self".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "journal-append".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("actor-self", "seq_actors_self"))
            .with_builtin(ExternalBuiltin::new(
                "journal-append",
                "seq_actors_journal_append",
            ));

        let ir = codegen
            .codegen_program_with_config(&program, HashMap::new(), &config)
            .unwrap();

        // Should declare both external builtins
        assert!(ir.contains("declare ptr @seq_actors_self(ptr)"));
        assert!(ir.contains("declare ptr @seq_actors_journal_append(ptr)"));

        // Should call both
        assert!(ir.contains("call ptr @seq_actors_self"));
        assert!(ir.contains("call ptr @seq_actors_journal_append"));
    }

    #[test]
    fn test_external_builtins_with_library_paths() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("my-op", "runtime_my_op"))
            .with_library_path("/custom/lib")
            .with_library("myruntime");

        assert_eq!(config.external_builtins.len(), 1);
        assert_eq!(config.library_paths, vec!["/custom/lib"]);
        assert_eq!(config.libraries, vec!["myruntime"]);
    }

    #[test]
    fn test_external_builtin_full_pipeline() {
        // Test that external builtins work through the full compile pipeline
        // including parser, AST validation, type checker, and codegen
        use crate::compile_to_ir_with_config;
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let source = r#"
            : main ( -- Int )
              42 my-transform
              0
            ;
        "#;

        let config = CompilerConfig::new().with_builtin(ExternalBuiltin::new(
            "my-transform",
            "ext_runtime_transform",
        ));

        // This should succeed - the external builtin is registered
        let result = compile_to_ir_with_config(source, &config);
        assert!(
            result.is_ok(),
            "Compilation should succeed: {:?}",
            result.err()
        );

        let ir = result.unwrap();
        assert!(ir.contains("declare ptr @ext_runtime_transform(ptr)"));
        assert!(ir.contains("call ptr @ext_runtime_transform"));
    }

    #[test]
    fn test_external_builtin_without_config_fails() {
        // Test that using an external builtin without config fails validation
        use crate::compile_to_ir;

        let source = r#"
            : main ( -- Int )
              42 unknown-builtin
              0
            ;
        "#;

        // This should fail - unknown-builtin is not registered
        let result = compile_to_ir(source);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown-builtin"));
    }

    #[test]
    fn test_match_exhaustiveness_error() {
        use crate::compile_to_ir;

        let source = r#"
            union Result { Ok { value: Int } Err { msg: String } }

            : handle ( Variant -- Int )
              match
                Ok -> drop 1
                # Missing Err arm!
              end
            ;

            : main ( -- ) 42 Make-Ok handle drop ;
        "#;

        let result = compile_to_ir(source);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Non-exhaustive match"));
        assert!(err.contains("Result"));
        assert!(err.contains("Err"));
    }

    #[test]
    fn test_match_exhaustive_compiles() {
        use crate::compile_to_ir;

        let source = r#"
            union Result { Ok { value: Int } Err { msg: String } }

            : handle ( Variant -- Int )
              match
                Ok -> drop 1
                Err -> drop 0
              end
            ;

            : main ( -- ) 42 Make-Ok handle drop ;
        "#;

        let result = compile_to_ir(source);
        assert!(
            result.is_ok(),
            "Exhaustive match should compile: {:?}",
            result
        );
    }

    #[test]
    fn test_codegen_symbol() {
        // Test symbol literal codegen
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::Symbol("hello".to_string()),
                    Statement::WordCall {
                        name: "symbol->string".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        assert!(ir.contains("call ptr @patch_seq_push_interned_symbol"));
        assert!(ir.contains("call ptr @patch_seq_symbol_to_string"));
        assert!(ir.contains("\"hello\\00\""));
    }

    #[test]
    fn test_symbol_interning_dedup() {
        // Issue #166: Test that duplicate symbol literals share the same global
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    // Use :hello twice - should share the same .sym global
                    Statement::Symbol("hello".to_string()),
                    Statement::Symbol("hello".to_string()),
                    Statement::Symbol("world".to_string()), // Different symbol
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        // Should have exactly one .sym global for "hello" and one for "world"
        // Count occurrences of symbol global definitions (lines starting with @.sym)
        let sym_defs: Vec<_> = ir
            .lines()
            .filter(|l| l.trim().starts_with("@.sym."))
            .collect();

        // There should be 2 definitions: .sym.0 for "hello" and .sym.1 for "world"
        assert_eq!(
            sym_defs.len(),
            2,
            "Expected 2 symbol globals, got: {:?}",
            sym_defs
        );

        // Verify deduplication: :hello appears twice but .sym.0 is reused
        let hello_uses: usize = ir.matches("@.sym.0").count();
        assert_eq!(
            hello_uses, 3,
            "Expected 3 occurrences of .sym.0 (1 def + 2 uses)"
        );

        // The IR should contain static symbol structure with capacity=0
        assert!(
            ir.contains("i64 0, i8 1"),
            "Symbol global should have capacity=0 and global=1"
        );
    }
}
