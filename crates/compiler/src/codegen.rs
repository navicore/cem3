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

use crate::ast::{Program, Statement, WordDef};
use crate::config::CompilerConfig;
use crate::types::Type;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

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
        ("write_line", "patch_seq_write_line"),
        ("read_line", "patch_seq_read_line"),
        ("read_line+", "patch_seq_read_line_plus"),
        ("int->string", "patch_seq_int_to_string"),
        // Command-line arguments
        ("arg-count", "patch_seq_arg_count"),
        ("arg", "patch_seq_arg_at"),
        // Arithmetic
        ("add", "patch_seq_add"),
        ("subtract", "patch_seq_subtract"),
        ("multiply", "patch_seq_multiply"),
        ("divide", "patch_seq_divide"),
        // Comparison (symbolic -> named)
        ("=", "patch_seq_eq"),
        ("<", "patch_seq_lt"),
        (">", "patch_seq_gt"),
        ("<=", "patch_seq_lte"),
        (">=", "patch_seq_gte"),
        ("<>", "patch_seq_neq"),
        // Boolean
        ("and", "patch_seq_and"),
        ("or", "patch_seq_or"),
        ("not", "patch_seq_not"),
        // Stack operations
        ("dup", "patch_seq_dup"),
        ("swap", "patch_seq_swap"),
        ("over", "patch_seq_over"),
        ("rot", "patch_seq_rot"),
        ("nip", "patch_seq_nip"),
        ("tuck", "patch_seq_tuck"),
        ("drop", "patch_seq_drop_op"),
        ("pick", "patch_seq_pick_op"),
        ("roll", "patch_seq_roll"),
        // Concurrency
        ("make-channel", "patch_seq_make_channel"),
        ("send", "patch_seq_chan_send"),
        ("send-safe", "patch_seq_chan_send_safe"),
        ("receive", "patch_seq_chan_receive"),
        ("receive-safe", "patch_seq_chan_receive_safe"),
        ("close-channel", "patch_seq_close_channel"),
        ("yield", "patch_seq_yield_strand"),
        // Quotation operations
        ("call", "patch_seq_call"),
        ("times", "patch_seq_times"),
        ("while", "patch_seq_while_loop"),
        ("until", "patch_seq_until_loop"),
        ("forever", "patch_seq_forever"),
        ("spawn", "patch_seq_spawn"),
        ("cond", "patch_seq_cond"),
        // TCP operations
        ("tcp-listen", "patch_seq_tcp_listen"),
        ("tcp-accept", "patch_seq_tcp_accept"),
        ("tcp-read", "patch_seq_tcp_read"),
        ("tcp-write", "patch_seq_tcp_write"),
        ("tcp-close", "patch_seq_tcp_close"),
        // String operations
        ("string-concat", "patch_seq_string_concat"),
        ("string-length", "patch_seq_string_length"),
        ("string-byte-length", "patch_seq_string_byte_length"),
        ("string-char-at", "patch_seq_string_char_at"),
        ("string-substring", "patch_seq_string_substring"),
        ("char->string", "patch_seq_char_to_string"),
        ("string-find", "patch_seq_string_find"),
        ("string-split", "patch_seq_string_split"),
        ("string-contains", "patch_seq_string_contains"),
        ("string-starts-with", "patch_seq_string_starts_with"),
        ("string-empty", "patch_seq_string_empty"),
        ("string-trim", "patch_seq_string_trim"),
        ("string-chomp", "patch_seq_string_chomp"),
        ("string-to-upper", "patch_seq_string_to_upper"),
        ("string-to-lower", "patch_seq_string_to_lower"),
        ("string-equal", "patch_seq_string_equal"),
        ("json-escape", "patch_seq_json_escape"),
        ("string->int", "patch_seq_string_to_int"),
        // File operations
        ("file-slurp", "patch_seq_file_slurp"),
        ("file-slurp-safe", "patch_seq_file_slurp_safe"),
        ("file-exists?", "patch_seq_file_exists"),
        // List operations
        ("list-map", "patch_seq_list_map"),
        ("list-filter", "patch_seq_list_filter"),
        ("list-fold", "patch_seq_list_fold"),
        ("list-each", "patch_seq_list_each"),
        ("list-length", "patch_seq_list_length"),
        ("list-empty?", "patch_seq_list_empty"),
        // Map operations
        ("make-map", "patch_seq_make_map"),
        ("map-get", "patch_seq_map_get"),
        ("map-get-safe", "patch_seq_map_get_safe"),
        ("map-set", "patch_seq_map_set"),
        ("map-has?", "patch_seq_map_has"),
        ("map-remove", "patch_seq_map_remove"),
        ("map-keys", "patch_seq_map_keys"),
        ("map-values", "patch_seq_map_values"),
        ("map-size", "patch_seq_map_size"),
        ("map-empty?", "patch_seq_map_empty"),
        // Variant operations
        ("variant-field-count", "patch_seq_variant_field_count"),
        ("variant-tag", "patch_seq_variant_tag"),
        ("variant-field-at", "patch_seq_variant_field_at"),
        ("variant-append", "patch_seq_variant_append"),
        ("variant-last", "patch_seq_variant_last"),
        ("variant-init", "patch_seq_variant_init"),
        ("make-variant", "patch_seq_make_variant"),
        ("make-variant-0", "patch_seq_make_variant_0"),
        ("make-variant-1", "patch_seq_make_variant_1"),
        ("make-variant-2", "patch_seq_make_variant_2"),
        ("make-variant-3", "patch_seq_make_variant_3"),
        ("make-variant-4", "patch_seq_make_variant_4"),
        // Float arithmetic
        ("f.add", "patch_seq_f_add"),
        ("f.subtract", "patch_seq_f_subtract"),
        ("f.multiply", "patch_seq_f_multiply"),
        ("f.divide", "patch_seq_f_divide"),
        // Float comparison (symbolic -> named)
        ("f.=", "patch_seq_f_eq"),
        ("f.<", "patch_seq_f_lt"),
        ("f.>", "patch_seq_f_gt"),
        ("f.<=", "patch_seq_f_lte"),
        ("f.>=", "patch_seq_f_gte"),
        ("f.<>", "patch_seq_f_neq"),
        // Float type conversions
        ("int->float", "patch_seq_int_to_float"),
        ("float->int", "patch_seq_float_to_int"),
        ("float->string", "patch_seq_float_to_string"),
        ("string->float", "patch_seq_string_to_float"),
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
        }
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
    fn get_quotation_type(&self, id: usize) -> Result<&Type, String> {
        self.type_map.get(&id).ok_or_else(|| {
            format!(
                "CodeGen: no type information for quotation ID {}. This is a compiler bug.",
                id
            )
        })
    }

    /// Escape a string for LLVM IR string literals
    fn escape_llvm_string(s: &str) -> String {
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
                        write!(&mut result, r"\{:02X}", byte).unwrap();
                    }
                }
            }
        }
        result
    }

    /// Get or create a global string constant
    fn get_string_global(&mut self, s: &str) -> String {
        if let Some(global_name) = self.string_constants.get(s) {
            return global_name.clone();
        }

        let global_name = format!("@.str.{}", self.string_counter);
        self.string_counter += 1;

        let escaped = Self::escape_llvm_string(s);
        let len = s.len() + 1; // +1 for null terminator

        writeln!(
            &mut self.string_globals,
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            global_name, len, escaped
        )
        .unwrap();

        self.string_constants
            .insert(s.to_string(), global_name.clone());
        global_name
    }

    /// Generate LLVM IR for entire program
    pub fn codegen_program(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
    ) -> Result<String, String> {
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
    ) -> Result<String, String> {
        // Store type map for use during code generation
        self.type_map = type_map;

        // Build external builtins map from config
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        // Verify we have a main word
        if program.find_word("main").is_none() {
            return Err("No main word defined".to_string());
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
        writeln!(&mut ir, "; ModuleID = 'main'").unwrap();
        writeln!(&mut ir, "target triple = \"{}\"", get_target_triple()).unwrap();
        writeln!(&mut ir).unwrap();

        // Opaque Value type (Rust enum)
        writeln!(&mut ir, "; Opaque Value type (Rust enum)").unwrap();
        writeln!(&mut ir, "%Value = type opaque").unwrap();
        writeln!(&mut ir).unwrap();

        // String constants
        if !self.string_globals.is_empty() {
            ir.push_str(&self.string_globals);
            writeln!(&mut ir).unwrap();
        }

        // Runtime function declarations
        writeln!(&mut ir, "; Runtime function declarations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_push_int(ptr, i64)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_push_string(ptr, ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_write_line(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_read_line(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_read_line_plus(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_int_to_string(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_add(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_subtract(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_multiply(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_divide(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_eq(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_lt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_gt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_lte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_gte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_neq(ptr)").unwrap();
        writeln!(&mut ir, "; Boolean operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_and(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_or(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_not(ptr)").unwrap();
        writeln!(&mut ir, "; Stack operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_dup(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_drop_op(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_swap(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_over(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_rot(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_nip(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tuck(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_pick_op(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_roll(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_push_value(ptr, %Value)").unwrap();
        writeln!(&mut ir, "; Quotation operations").unwrap();
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_push_quotation(ptr, i64, i64)"
        )
        .unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_call(ptr)").unwrap();
        // Phase 2 TCO helpers for quotation calls
        writeln!(&mut ir, "declare i64 @patch_seq_peek_is_quotation(ptr)").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_peek_quotation_fn_ptr(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_times(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_while_loop(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_until_loop(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_forever(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_spawn(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_cond(ptr)").unwrap();
        writeln!(&mut ir, "; Closure operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_create_env(i32)").unwrap();
        writeln!(&mut ir, "declare void @patch_seq_env_set(ptr, i32, %Value)").unwrap();
        writeln!(&mut ir, "declare %Value @patch_seq_env_get(ptr, i64, i32)").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_env_get_int(ptr, i64, i32)").unwrap();
        writeln!(
            &mut ir,
            "declare i64 @patch_seq_env_get_bool(ptr, i64, i32)"
        )
        .unwrap();
        writeln!(
            &mut ir,
            "declare double @patch_seq_env_get_float(ptr, i64, i32)"
        )
        .unwrap();
        writeln!(
            &mut ir,
            "declare i64 @patch_seq_env_get_quotation(ptr, i64, i32)"
        )
        .unwrap();
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_env_get_string(ptr, i64, i32)"
        )
        .unwrap();
        writeln!(&mut ir, "declare %Value @patch_seq_make_closure(i64, ptr)").unwrap();
        writeln!(
            &mut ir,
            "declare ptr @patch_seq_push_closure(ptr, i64, i32)"
        )
        .unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_push_seqstring(ptr, ptr)").unwrap();
        writeln!(&mut ir, "; Concurrency operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_channel(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_chan_send(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_chan_send_safe(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_chan_receive(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_chan_receive_safe(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_close_channel(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_yield_strand(ptr)").unwrap();
        writeln!(&mut ir, "; Scheduler operations").unwrap();
        writeln!(&mut ir, "declare void @patch_seq_scheduler_init()").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_scheduler_run()").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_strand_spawn(ptr, ptr)").unwrap();
        writeln!(&mut ir, "; Command-line argument operations").unwrap();
        writeln!(&mut ir, "declare void @patch_seq_args_init(i32, ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_arg_count(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_arg_at(ptr)").unwrap();
        writeln!(&mut ir, "; File operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_file_slurp(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_file_slurp_safe(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_file_exists(ptr)").unwrap();
        writeln!(&mut ir, "; List operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_map(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_filter(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_fold(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_each(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_length(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_list_empty(ptr)").unwrap();
        writeln!(&mut ir, "; Map operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_map(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_get(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_get_safe(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_set(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_has(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_remove(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_keys(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_values(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_size(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_map_empty(ptr)").unwrap();
        writeln!(&mut ir, "; TCP operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_listen(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_accept(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_read(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_write(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_close(ptr)").unwrap();
        writeln!(&mut ir, "; String operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_concat(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_length(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_byte_length(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_char_at(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_substring(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_char_to_string(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_find(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_split(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_contains(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_starts_with(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_empty(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_trim(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_chomp(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_upper(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_lower(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_equal(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_json_escape(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_int(ptr)").unwrap();
        writeln!(&mut ir, "; Variant operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_field_count(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_tag(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_field_at(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_append(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_last(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_variant_init(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_0(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_1(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_2(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_3(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_make_variant_4(ptr)").unwrap();
        writeln!(&mut ir, "; Float operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_push_float(ptr, double)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_add(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_subtract(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_multiply(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_divide(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_eq(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_lt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_gt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_lte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_gte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_f_neq(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_int_to_float(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_float_to_int(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_float_to_string(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_float(ptr)").unwrap();
        writeln!(&mut ir, "; Helpers for conditionals").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_peek_int_value(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_pop_stack(ptr)").unwrap();
        writeln!(&mut ir).unwrap();

        // External builtin declarations (from config)
        if !self.external_builtins.is_empty() {
            writeln!(&mut ir, "; External builtin declarations").unwrap();
            for symbol in self.external_builtins.values() {
                // All external builtins follow the standard stack convention: ptr -> ptr
                writeln!(&mut ir, "declare ptr @{}(ptr)", symbol).unwrap();
            }
            writeln!(&mut ir).unwrap();
        }

        // Quotation functions (generated from quotation literals)
        if !self.quotation_functions.is_empty() {
            writeln!(&mut ir, "; Quotation functions").unwrap();
            ir.push_str(&self.quotation_functions);
            writeln!(&mut ir).unwrap();
        }

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Generate code for a word definition
    fn codegen_word(&mut self, word: &WordDef) -> Result<(), String> {
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
            )
            .unwrap();
        } else {
            writeln!(
                &mut self.output,
                "define tailcc ptr @{}(ptr %stack) {{",
                function_name
            )
            .unwrap();
        }
        writeln!(&mut self.output, "entry:").unwrap();

        let mut stack_var = "stack".to_string();
        let body_len = word.body.len();

        // Generate code for each statement
        // The last statement is in tail position
        for (i, statement) in word.body.iter().enumerate() {
            let position = if i == body_len - 1 {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };
            stack_var = self.codegen_statement(&stack_var, statement, position)?;
        }

        // Only emit ret if the last statement wasn't a tail call
        // (tail calls emit their own ret)
        if word.body.is_empty()
            || !self.will_emit_tail_call(word.body.last().unwrap(), TailPosition::Tail)
        {
            writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
        }
        writeln!(&mut self.output, "}}").unwrap();
        writeln!(&mut self.output).unwrap();

        self.inside_main = false;
        Ok(())
    }

    /// Generate a quotation function
    /// Returns wrapper and impl function names for TCO support
    fn codegen_quotation(
        &mut self,
        body: &[Statement],
        quot_type: &Type,
    ) -> Result<QuotationFunctions, String> {
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
                )
                .unwrap();
                writeln!(&mut self.output, "entry:").unwrap();

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
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
                }
                writeln!(&mut self.output, "}}").unwrap();
                writeln!(&mut self.output).unwrap();

                // Now generate the wrapper function with C convention
                // This is a thin wrapper that just calls the impl
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack) {{",
                    wrapper_name
                )
                .unwrap();
                writeln!(&mut self.output, "entry:").unwrap();
                writeln!(
                    &mut self.output,
                    "  %result = call tailcc ptr @{}(ptr %stack)",
                    impl_name
                )
                .unwrap();
                writeln!(&mut self.output, "  ret ptr %result").unwrap();
                writeln!(&mut self.output, "}}").unwrap();
                writeln!(&mut self.output).unwrap();

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
                )
                .unwrap();
                writeln!(&mut self.output, "entry:").unwrap();

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
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
                }
                writeln!(&mut self.output, "}}").unwrap();
                writeln!(&mut self.output).unwrap();

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
            _ => Err(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            )),
        }
    }

    /// Check if a name refers to a runtime builtin (not a user-defined word).
    fn is_runtime_builtin(&self, name: &str) -> bool {
        BUILTIN_SYMBOLS.contains_key(name) || self.external_builtins.contains_key(name)
    }

    /// Emit code to push a captured value onto the stack.
    /// Returns the new stack variable name, or an error for unsupported types.
    fn emit_capture_push(
        &mut self,
        capture_type: &Type,
        index: usize,
        stack_var: &str,
    ) -> Result<String, String> {
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
            Type::String => (
                "patch_seq_env_get_string",
                "ptr",
                "patch_seq_push_seqstring",
                "ptr",
            ),
            Type::Quotation(_) => (
                "patch_seq_env_get_quotation",
                "i64",
                "patch_seq_push_quotation",
                "i64",
            ),
            Type::Closure { .. } => {
                return Err("Closure captures are not yet supported. \
                     Closures capturing other closures require additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                    .to_string());
            }
            Type::Var(name) if name.starts_with("Variant") => {
                return Err("Variant captures are not yet supported. \
                     Capturing Variants in closures requires additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                    .to_string());
            }
            _ => {
                return Err(format!(
                    "Unsupported capture type: {:?}. \
                     Supported capture types: Int, Bool, Float, String, Quotation.",
                    capture_type
                ));
            }
        };

        // Get value from environment
        let value_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call {} @{}(ptr %env_data, i64 %env_len, i32 {})",
            value_var, getter_type, getter, index
        )
        .unwrap();

        // Push value onto stack
        let new_stack_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @{}(ptr %{}, {} %{})",
            new_stack_var, pusher, stack_var, pusher_type, value_var
        )
        .unwrap();

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
    ) -> Result<BranchResult, String> {
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
            writeln!(&mut self.output, "  br label %{}", pred).unwrap();
            writeln!(&mut self.output, "{}:", pred).unwrap();
            writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();
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
            Statement::WordCall(name) => {
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
    ) -> Result<String, String> {
        // Check if top of stack is a quotation
        let is_quot_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_is_quotation(ptr %{})",
            is_quot_var, stack_var
        )
        .unwrap();

        // Compare to 1 (true = quotation)
        let cmp_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 1",
            cmp_var, is_quot_var
        )
        .unwrap();

        // Create labels for branching
        let quot_block = self.fresh_block("call_quotation");
        let closure_block = self.fresh_block("call_closure");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_var, quot_block, closure_block
        )
        .unwrap();

        // Quotation path: extract fn_ptr and musttail call
        writeln!(&mut self.output, "{}:", quot_block).unwrap();
        let fn_ptr_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_quotation_fn_ptr(ptr %{})",
            fn_ptr_var, stack_var
        )
        .unwrap();

        // Pop the quotation from the stack
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            popped_stack, stack_var
        )
        .unwrap();

        // Convert i64 fn_ptr to function pointer type
        let fn_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = inttoptr i64 %{} to ptr",
            fn_var, fn_ptr_var
        )
        .unwrap();

        // Tail call the quotation's impl function using musttail + tailcc
        // This is guaranteed TCO: caller is tailcc, quotation impl is tailcc
        let quot_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = musttail call tailcc ptr %{}(ptr %{})",
            quot_result, fn_var, popped_stack
        )
        .unwrap();
        writeln!(&mut self.output, "  ret ptr %{}", quot_result).unwrap();

        // Closure path: fall back to regular patch_seq_call
        // Use a fresh temp to ensure proper SSA numbering (must be >= quotation branch temps)
        writeln!(&mut self.output, "{}:", closure_block).unwrap();
        let closure_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_call(ptr %{})",
            closure_result, stack_var
        )
        .unwrap();
        writeln!(&mut self.output, "  ret ptr %{}", closure_result).unwrap();

        // Return a dummy value - both branches emit ret, so this won't be used
        Ok(closure_result)
    }

    // =========================================================================
    // Statement Code Generation Helpers
    // =========================================================================

    /// Generate code for an integer literal: ( -- n )
    fn codegen_int_literal(&mut self, stack_var: &str, n: i64) -> Result<String, String> {
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
            result_var, stack_var, n
        )
        .unwrap();
        Ok(result_var)
    }

    /// Generate code for a float literal: ( -- f )
    ///
    /// Uses LLVM's hexadecimal floating point format for exact representation.
    /// Handles special values (NaN, Infinity) explicitly.
    fn codegen_float_literal(&mut self, stack_var: &str, f: f64) -> Result<String, String> {
        let result_var = self.fresh_temp();
        // Format float to ensure LLVM recognizes it as a double literal
        let float_str = if f.is_nan() {
            "0x7FF8000000000000".to_string() // NaN
        } else if f.is_infinite() {
            if f.is_sign_positive() {
                "0x7FF0000000000000".to_string() // +Infinity
            } else {
                "0xFFF0000000000000".to_string() // -Infinity
            }
        } else {
            // Use LLVM's hexadecimal floating point format for exact representation
            format!("0x{:016X}", f.to_bits())
        };
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_float(ptr %{}, double {})",
            result_var, stack_var, float_str
        )
        .unwrap();
        Ok(result_var)
    }

    /// Generate code for a boolean literal: ( -- b )
    fn codegen_bool_literal(&mut self, stack_var: &str, b: bool) -> Result<String, String> {
        let result_var = self.fresh_temp();
        let val = if b { 1 } else { 0 };
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
            result_var, stack_var, val
        )
        .unwrap();
        Ok(result_var)
    }

    /// Generate code for a string literal: ( -- s )
    fn codegen_string_literal(&mut self, stack_var: &str, s: &str) -> Result<String, String> {
        let global = self.get_string_global(s);
        let ptr_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr inbounds [{} x i8], ptr {}, i32 0, i32 0",
            ptr_temp,
            s.len() + 1,
            global
        )
        .unwrap();
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_string(ptr %{}, ptr %{})",
            result_var, stack_var, ptr_temp
        )
        .unwrap();
        Ok(result_var)
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
    ) -> Result<String, String> {
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
            writeln!(
                &mut self.output,
                "  %{} = musttail call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )
            .unwrap();
            writeln!(&mut self.output, "  ret ptr %{}", result_var).unwrap();
        } else if is_seq_word {
            // Non-tail call to user-defined word: must use tailcc calling convention
            writeln!(
                &mut self.output,
                "  %{} = call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )
            .unwrap();
        } else {
            // Call to builtin (C calling convention)
            writeln!(
                &mut self.output,
                "  %{} = call ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )
            .unwrap();
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
    ) -> Result<String, String> {
        // Peek the condition value, then pop to free the stack node
        let cond_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
            cond_temp, stack_var
        )
        .unwrap();

        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            popped_stack, stack_var
        )
        .unwrap();

        // Compare with 0 (0 = false, non-zero = true)
        let cmp_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp_temp, cond_temp
        )
        .unwrap();

        // Generate unique block labels
        let then_block = self.fresh_block("if_then");
        let else_block = self.fresh_block("if_else");
        let merge_block = self.fresh_block("if_merge");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_temp, then_block, else_block
        )
        .unwrap();

        // Then branch
        writeln!(&mut self.output, "{}:", then_block).unwrap();
        let then_result = self.codegen_branch(
            then_branch,
            &popped_stack,
            position,
            &merge_block,
            "if_then",
        )?;

        // Else branch
        writeln!(&mut self.output, "{}:", else_block).unwrap();
        let else_result = if let Some(eb) = else_branch {
            self.codegen_branch(eb, &popped_stack, position, &merge_block, "if_else")?
        } else {
            // No else clause - emit landing block with unchanged stack
            let else_pred = self.fresh_block("if_else_end");
            writeln!(&mut self.output, "  br label %{}", else_pred).unwrap();
            writeln!(&mut self.output, "{}:", else_pred).unwrap();
            writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();
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
        writeln!(&mut self.output, "{}:", merge_block).unwrap();
        let result_var = self.fresh_temp();

        if then_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, else_result.stack_var, else_result.predecessor
            )
            .unwrap();
        } else if else_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, then_result.stack_var, then_result.predecessor
            )
            .unwrap();
        } else {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{} ]",
                result_var,
                then_result.stack_var,
                then_result.predecessor,
                else_result.stack_var,
                else_result.predecessor
            )
            .unwrap();
        }

        Ok(result_var)
    }

    /// Generate code for pushing a quotation or closure onto the stack
    fn codegen_quotation_push(
        &mut self,
        stack_var: &str,
        id: usize,
        body: &[Statement],
    ) -> Result<String, String> {
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
                )
                .unwrap();

                let impl_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    impl_ptr_var, fns.impl_
                )
                .unwrap();

                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_quotation(ptr %{}, i64 %{}, i64 %{})",
                    result_var, stack_var, wrapper_ptr_var, impl_ptr_var
                )
                .unwrap();
                Ok(result_var)
            }
            Type::Closure { captures, .. } => {
                // For closures, just use the single function pointer (no TCO yet)
                let fn_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    fn_ptr_var, fns.wrapper
                )
                .unwrap();

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
                )
                .unwrap();
                Ok(result_var)
            }
            _ => Err(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            )),
        }
    }

    // =========================================================================
    // Main Statement Dispatcher
    // =========================================================================

    /// Generate code for a single statement
    ///
    /// The `position` parameter indicates whether this statement is in tail position.
    /// For tail calls, we emit `musttail call` followed by `ret` to guarantee TCO.
    fn codegen_statement(
        &mut self,
        stack_var: &str,
        statement: &Statement,
        position: TailPosition,
    ) -> Result<String, String> {
        match statement {
            Statement::IntLiteral(n) => self.codegen_int_literal(stack_var, *n),
            Statement::FloatLiteral(f) => self.codegen_float_literal(stack_var, *f),
            Statement::BoolLiteral(b) => self.codegen_bool_literal(stack_var, *b),
            Statement::StringLiteral(s) => self.codegen_string_literal(stack_var, s),
            Statement::WordCall(name) => self.codegen_word_call(stack_var, name, position),
            Statement::If {
                then_branch,
                else_branch,
            } => self.codegen_if_statement(stack_var, then_branch, else_branch.as_ref(), position),
            Statement::Quotation { id, body } => self.codegen_quotation_push(stack_var, *id, body),
        }
    }

    /// Generate main function that calls user's main word
    fn codegen_main(&mut self) -> Result<(), String> {
        writeln!(
            &mut self.output,
            "define i32 @main(i32 %argc, ptr %argv) {{"
        )
        .unwrap();
        writeln!(&mut self.output, "entry:").unwrap();

        // Initialize command-line arguments (before scheduler so args are available)
        writeln!(
            &mut self.output,
            "  call void @patch_seq_args_init(i32 %argc, ptr %argv)"
        )
        .unwrap();

        // Initialize scheduler
        writeln!(&mut self.output, "  call void @patch_seq_scheduler_init()").unwrap();

        // Spawn user's main function as the first strand
        // This ensures all code runs in coroutine context for non-blocking I/O
        writeln!(
            &mut self.output,
            "  %0 = call i64 @patch_seq_strand_spawn(ptr @seq_main, ptr null)"
        )
        .unwrap();

        // Wait for all spawned strands to complete (including main)
        writeln!(
            &mut self.output,
            "  %1 = call ptr @patch_seq_scheduler_run()"
        )
        .unwrap();

        writeln!(&mut self.output, "  ret i32 0").unwrap();
        writeln!(&mut self.output, "}}").unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Program, Statement, WordDef};

    #[test]
    fn test_codegen_hello_world() {
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("Hello, World!".to_string()),
                    Statement::WordCall("write_line".to_string()),
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
    fn test_codegen_arithmetic() {
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall("add".to_string()),
                ],
                source: None,
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        assert!(ir.contains("call ptr @patch_seq_push_int(ptr %stack, i64 2)"));
        assert!(ir.contains("call ptr @patch_seq_push_int"));
        assert!(ir.contains("call ptr @patch_seq_add"));
    }

    #[test]
    fn test_escape_llvm_string() {
        assert_eq!(CodeGen::escape_llvm_string("hello"), "hello");
        assert_eq!(CodeGen::escape_llvm_string("a\nb"), r"a\0Ab");
        assert_eq!(CodeGen::escape_llvm_string("a\tb"), r"a\09b");
        assert_eq!(CodeGen::escape_llvm_string("a\"b"), r"a\22b");
    }

    #[test]
    fn test_external_builtins_declared() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::WordCall("my-external-op".to_string()),
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
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall("actor-self".to_string()),
                    Statement::WordCall("journal-append".to_string()),
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
}
