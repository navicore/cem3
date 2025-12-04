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
        writeln!(&mut ir, "declare ptr @patch_seq_push_quotation(ptr, i64)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_call(ptr)").unwrap();
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
        // Use tailcc calling convention for guaranteed tail call optimization
        writeln!(
            &mut self.output,
            "define tailcc ptr @{}(ptr %stack) {{",
            function_name
        )
        .unwrap();
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

        Ok(())
    }

    /// Generate a quotation function
    /// Returns the function name
    fn codegen_quotation(
        &mut self,
        body: &[Statement],
        quot_type: &Type,
    ) -> Result<String, String> {
        // Generate unique function name
        let function_name = format!("seq_quot_{}", self.quot_counter);
        self.quot_counter += 1;

        // Save current output and switch to quotation_functions
        let saved_output = std::mem::take(&mut self.output);

        // Generate function signature based on type
        // Use tailcc for quotations to enable tail call optimization
        match quot_type {
            Type::Quotation(_) => {
                // Stateless quotation: fn(Stack) -> Stack
                writeln!(
                    &mut self.output,
                    "define tailcc ptr @{}(ptr %stack) {{",
                    function_name
                )
                .unwrap();
            }
            Type::Closure { captures, .. } => {
                // Closure: fn(Stack, *const Value, usize) -> Stack
                // Note: Closures don't use tailcc yet (Phase 3 work)
                // Mark that we're inside a closure to disable tail calls
                self.inside_closure = true;
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack, ptr %env_data, i64 %env_len) {{",
                    function_name
                )
                .unwrap();
                writeln!(&mut self.output, "entry:").unwrap();

                // Push captured values onto the stack before executing body
                // Captures are stored bottom-to-top, so push them in order
                let mut stack_var = "stack".to_string();
                for (index, capture_type) in captures.iter().enumerate() {
                    // Use type-specific getters to avoid passing large Value enum through FFI
                    match capture_type {
                        Type::Int => {
                            let int_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call i64 @patch_seq_env_get_int(ptr %env_data, i64 %env_len, i32 {})",
                                int_var, index
                            )
                            .unwrap();
                            let new_stack_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                                new_stack_var, stack_var, int_var
                            )
                            .unwrap();
                            stack_var = new_stack_var;
                        }
                        Type::String => {
                            let string_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_env_get_string(ptr %env_data, i64 %env_len, i32 {})",
                                string_var, index
                            )
                            .unwrap();
                            let new_stack_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_push_seqstring(ptr %{}, ptr %{})",
                                new_stack_var, stack_var, string_var
                            )
                            .unwrap();
                            stack_var = new_stack_var;
                        }
                        Type::Bool => {
                            // Bool is stored as i64 (0 or 1)
                            let bool_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call i64 @patch_seq_env_get_bool(ptr %env_data, i64 %env_len, i32 {})",
                                bool_var, index
                            )
                            .unwrap();
                            let new_stack_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                                new_stack_var, stack_var, bool_var
                            )
                            .unwrap();
                            stack_var = new_stack_var;
                        }
                        Type::Float => {
                            let float_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call double @patch_seq_env_get_float(ptr %env_data, i64 %env_len, i32 {})",
                                float_var, index
                            )
                            .unwrap();
                            let new_stack_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_push_float(ptr %{}, double %{})",
                                new_stack_var, stack_var, float_var
                            )
                            .unwrap();
                            stack_var = new_stack_var;
                        }
                        Type::Quotation(_effect) => {
                            // Quotation is just a function pointer (i64)
                            let fn_ptr_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call i64 @patch_seq_env_get_quotation(ptr %env_data, i64 %env_len, i32 {})",
                                fn_ptr_var, index
                            )
                            .unwrap();
                            let new_stack_var = self.fresh_temp();
                            writeln!(
                                &mut self.output,
                                "  %{} = call ptr @patch_seq_push_quotation(ptr %{}, i64 %{})",
                                new_stack_var, stack_var, fn_ptr_var
                            )
                            .unwrap();
                            stack_var = new_stack_var;
                        }
                        Type::Closure { .. } => {
                            return Err(
                                "Closure captures are not yet supported. \
                                 Closures capturing other closures require additional implementation. \
                                 Supported capture types: Int, Bool, Float, String, Quotation."
                                    .to_string(),
                            );
                        }
                        Type::Var(name) if name.starts_with("Variant") => {
                            return Err(
                                "Variant captures are not yet supported. \
                                 Capturing Variants in closures requires additional implementation. \
                                 Supported capture types: Int, Bool, Float, String, Quotation."
                                    .to_string(),
                            );
                        }
                        _ => {
                            // Unknown type - provide helpful error
                            return Err(format!(
                                "Unsupported capture type: {:?}. \
                                 Supported capture types: Int, Bool, Float, String, Quotation.",
                                capture_type
                            ));
                        }
                    }
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

                return Ok(function_name);
            }
            _ => {
                return Err(format!(
                    "CodeGen: expected Quotation or Closure type, got {:?}",
                    quot_type
                ));
            }
        }

        writeln!(&mut self.output, "entry:").unwrap();

        let mut stack_var = "stack".to_string();
        let body_len = body.len();

        // Generate code for each statement in the quotation body
        // Last statement is in tail position
        for (i, statement) in body.iter().enumerate() {
            let position = if i == body_len - 1 {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };
            stack_var = self.codegen_statement(&stack_var, statement, position)?;
        }

        // Only emit ret if the last statement wasn't a tail call
        if body.is_empty() || !self.will_emit_tail_call(body.last().unwrap(), TailPosition::Tail) {
            writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
        }
        writeln!(&mut self.output, "}}").unwrap();
        writeln!(&mut self.output).unwrap();

        // Move generated function to quotation_functions
        self.quotation_functions.push_str(&self.output);

        // Restore original output
        self.output = saved_output;

        Ok(function_name)
    }

    /// Check if a statement in tail position would emit a terminator (ret)
    /// This is true for:
    /// - User-defined word calls (emit musttail + ret)
    /// - If statements where BOTH branches emit terminators
    ///   Returns false if inside a closure (closures can't use `musttail` due to signature mismatch)
    fn will_emit_tail_call(&self, statement: &Statement, position: TailPosition) -> bool {
        if position != TailPosition::Tail {
            return false;
        }
        // Closures can't use musttail because their signature differs from regular functions
        if self.inside_closure {
            return false;
        }
        match statement {
            Statement::WordCall(name) => {
                // Check if it's a user-defined word (not a runtime builtin)
                !matches!(
                    name.as_str(),
                    "write_line"
                        | "read_line"
                        | "read_line+"
                        | "int->string"
                        | "arg-count"
                        | "arg"
                        | "add"
                        | "subtract"
                        | "multiply"
                        | "divide"
                        | "="
                        | "<"
                        | ">"
                        | "<="
                        | ">="
                        | "<>"
                        | "and"
                        | "or"
                        | "not"
                        | "dup"
                        | "swap"
                        | "over"
                        | "rot"
                        | "nip"
                        | "tuck"
                        | "drop"
                        | "pick"
                        | "roll"
                        | "make-channel"
                        | "send"
                        | "send-safe"
                        | "receive"
                        | "receive-safe"
                        | "close-channel"
                        | "yield"
                        | "call"
                        | "times"
                        | "while"
                        | "until"
                        | "forever"
                        | "spawn"
                        | "cond"
                        | "tcp-listen"
                        | "tcp-accept"
                        | "tcp-read"
                        | "tcp-write"
                        | "tcp-close"
                        | "string-concat"
                        | "string-length"
                        | "string-byte-length"
                        | "string-char-at"
                        | "string-substring"
                        | "char->string"
                        | "string-find"
                        | "string-split"
                        | "string-contains"
                        | "string-starts-with"
                        | "string-empty"
                        | "string-trim"
                        | "string-chomp"
                        | "string-to-upper"
                        | "string-to-lower"
                        | "string-equal"
                        | "json-escape"
                        | "string->int"
                        | "file-slurp"
                        | "file-slurp-safe"
                        | "file-exists?"
                        | "list-map"
                        | "list-filter"
                        | "list-fold"
                        | "list-each"
                        | "list-length"
                        | "list-empty?"
                        | "make-map"
                        | "map-get"
                        | "map-get-safe"
                        | "map-set"
                        | "map-has?"
                        | "map-remove"
                        | "map-keys"
                        | "map-values"
                        | "map-size"
                        | "map-empty?"
                        | "variant-field-count"
                        | "variant-tag"
                        | "variant-field-at"
                        | "variant-append"
                        | "variant-last"
                        | "variant-init"
                        | "make-variant"
                        | "make-variant-0"
                        | "make-variant-1"
                        | "make-variant-2"
                        | "make-variant-3"
                        | "make-variant-4"
                        | "f.add"
                        | "f.subtract"
                        | "f.multiply"
                        | "f.divide"
                        | "f.="
                        | "f.<"
                        | "f.>"
                        | "f.<="
                        | "f.>="
                        | "f.<>"
                        | "int->float"
                        | "float->int"
                        | "float->string"
                        | "string->float"
                ) && !self.external_builtins.contains_key(name)
            }
            Statement::If {
                then_branch,
                else_branch,
            } => {
                // An if statement emits a terminator (no merge block) if BOTH branches
                // end with terminators. We check the last statement of each branch.
                let then_terminates = then_branch
                    .last()
                    .map(|s| self.will_emit_tail_call(s, TailPosition::Tail))
                    .unwrap_or(false);
                let else_terminates = else_branch
                    .as_ref()
                    .and_then(|eb| eb.last())
                    .map(|s| self.will_emit_tail_call(s, TailPosition::Tail))
                    .unwrap_or(false);
                then_terminates && else_terminates
            }
            _ => false,
        }
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
    ) -> Result<String, String> {
        match statement {
            Statement::IntLiteral(n) => {
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
                    result_var, stack_var, n
                )
                .unwrap();
                Ok(result_var)
            }

            Statement::FloatLiteral(f) => {
                let result_var = self.fresh_temp();
                // Format float to ensure LLVM recognizes it as a double literal
                // Use hex representation for precise and always-valid format
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
                    let bits = f.to_bits();
                    format!("0x{:016X}", bits)
                };
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_float(ptr %{}, double {})",
                    result_var, stack_var, float_str
                )
                .unwrap();
                Ok(result_var)
            }

            Statement::BoolLiteral(b) => {
                let result_var = self.fresh_temp();
                let val = if *b { 1 } else { 0 };
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
                    result_var, stack_var, val
                )
                .unwrap();
                Ok(result_var)
            }

            Statement::StringLiteral(s) => {
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

            Statement::WordCall(name) => {
                let result_var = self.fresh_temp();
                // Map source-level word names to runtime function names
                // Most built-ins use their source name directly, but some need mapping:
                // - Symbolic operators (=, <, >) map to names (eq, lt, gt)
                // - 'drop' maps to 'drop_op' (drop is LLVM reserved)
                // - User words get 'seq_' prefix to avoid C symbol conflicts
                let (function_name, is_seq_word) = match name.as_str() {
                    // I/O operations
                    "write_line" | "read_line" => (format!("patch_seq_{}", name), false),
                    "read_line+" => ("patch_seq_read_line_plus".to_string(), false),
                    "int->string" => ("patch_seq_int_to_string".to_string(), false),
                    // Command-line argument operations
                    "arg-count" => ("patch_seq_arg_count".to_string(), false),
                    "arg" => ("patch_seq_arg_at".to_string(), false),
                    // Arithmetic operations
                    "add" | "subtract" | "multiply" | "divide" => {
                        (format!("patch_seq_{}", name), false)
                    }
                    // Comparison operations (symbolic → named)
                    // These return Int (0 or 1) for Forth-style boolean semantics
                    "=" => ("patch_seq_eq".to_string(), false),
                    "<" => ("patch_seq_lt".to_string(), false),
                    ">" => ("patch_seq_gt".to_string(), false),
                    "<=" => ("patch_seq_lte".to_string(), false),
                    ">=" => ("patch_seq_gte".to_string(), false),
                    "<>" => ("patch_seq_neq".to_string(), false),
                    // Boolean operations
                    "and" | "or" | "not" => (format!("patch_seq_{}", name), false),
                    // Stack operations (simple - no parameters)
                    "dup" | "swap" | "over" | "rot" | "nip" | "tuck" => {
                        (format!("patch_seq_{}", name), false)
                    }
                    "drop" => ("patch_seq_drop_op".to_string(), false),
                    "pick" => ("patch_seq_pick_op".to_string(), false),
                    "roll" => ("patch_seq_roll".to_string(), false),
                    // Concurrency operations (hyphen → underscore for C compatibility)
                    "make-channel" => ("patch_seq_make_channel".to_string(), false),
                    "send" => ("patch_seq_chan_send".to_string(), false),
                    "send-safe" => ("patch_seq_chan_send_safe".to_string(), false),
                    "receive" => ("patch_seq_chan_receive".to_string(), false),
                    "receive-safe" => ("patch_seq_chan_receive_safe".to_string(), false),
                    "close-channel" => ("patch_seq_close_channel".to_string(), false),
                    "yield" => ("patch_seq_yield_strand".to_string(), false),
                    // Quotation operations
                    "call" => ("patch_seq_call".to_string(), false),
                    "times" => ("patch_seq_times".to_string(), false),
                    "while" => ("patch_seq_while_loop".to_string(), false),
                    "until" => ("patch_seq_until_loop".to_string(), false),
                    "forever" => ("patch_seq_forever".to_string(), false),
                    "spawn" => ("patch_seq_spawn".to_string(), false),
                    "cond" => ("patch_seq_cond".to_string(), false),
                    // TCP operations (hyphen → underscore for C compatibility)
                    "tcp-listen" => ("patch_seq_tcp_listen".to_string(), false),
                    "tcp-accept" => ("patch_seq_tcp_accept".to_string(), false),
                    "tcp-read" => ("patch_seq_tcp_read".to_string(), false),
                    "tcp-write" => ("patch_seq_tcp_write".to_string(), false),
                    "tcp-close" => ("patch_seq_tcp_close".to_string(), false),
                    // String operations (hyphen → underscore for C compatibility)
                    "string-concat" => ("patch_seq_string_concat".to_string(), false),
                    "string-length" => ("patch_seq_string_length".to_string(), false),
                    "string-byte-length" => ("patch_seq_string_byte_length".to_string(), false),
                    "string-char-at" => ("patch_seq_string_char_at".to_string(), false),
                    "string-substring" => ("patch_seq_string_substring".to_string(), false),
                    "char->string" => ("patch_seq_char_to_string".to_string(), false),
                    "string-find" => ("patch_seq_string_find".to_string(), false),
                    "string-split" => ("patch_seq_string_split".to_string(), false),
                    "string-contains" => ("patch_seq_string_contains".to_string(), false),
                    "string-starts-with" => ("patch_seq_string_starts_with".to_string(), false),
                    "string-empty" => ("patch_seq_string_empty".to_string(), false),
                    "string-trim" => ("patch_seq_string_trim".to_string(), false),
                    "string-chomp" => ("patch_seq_string_chomp".to_string(), false),
                    "string-to-upper" => ("patch_seq_string_to_upper".to_string(), false),
                    "string-to-lower" => ("patch_seq_string_to_lower".to_string(), false),
                    "string-equal" => ("patch_seq_string_equal".to_string(), false),
                    "json-escape" => ("patch_seq_json_escape".to_string(), false),
                    "string->int" => ("patch_seq_string_to_int".to_string(), false),
                    // File operations (hyphen → underscore for C compatibility)
                    "file-slurp" => ("patch_seq_file_slurp".to_string(), false),
                    "file-slurp-safe" => ("patch_seq_file_slurp_safe".to_string(), false),
                    "file-exists?" => ("patch_seq_file_exists".to_string(), false),
                    // List operations (hyphen → underscore for C compatibility)
                    "list-map" => ("patch_seq_list_map".to_string(), false),
                    "list-filter" => ("patch_seq_list_filter".to_string(), false),
                    "list-fold" => ("patch_seq_list_fold".to_string(), false),
                    "list-each" => ("patch_seq_list_each".to_string(), false),
                    "list-length" => ("patch_seq_list_length".to_string(), false),
                    "list-empty?" => ("patch_seq_list_empty".to_string(), false),
                    // Map operations (hyphen → underscore for C compatibility)
                    "make-map" => ("patch_seq_make_map".to_string(), false),
                    "map-get" => ("patch_seq_map_get".to_string(), false),
                    "map-get-safe" => ("patch_seq_map_get_safe".to_string(), false),
                    "map-set" => ("patch_seq_map_set".to_string(), false),
                    "map-has?" => ("patch_seq_map_has".to_string(), false),
                    "map-remove" => ("patch_seq_map_remove".to_string(), false),
                    "map-keys" => ("patch_seq_map_keys".to_string(), false),
                    "map-values" => ("patch_seq_map_values".to_string(), false),
                    "map-size" => ("patch_seq_map_size".to_string(), false),
                    "map-empty?" => ("patch_seq_map_empty".to_string(), false),
                    // Variant operations (hyphen → underscore for C compatibility)
                    "variant-field-count" => ("patch_seq_variant_field_count".to_string(), false),
                    "variant-tag" => ("patch_seq_variant_tag".to_string(), false),
                    "variant-field-at" => ("patch_seq_variant_field_at".to_string(), false),
                    "variant-append" => ("patch_seq_variant_append".to_string(), false),
                    "variant-last" => ("patch_seq_variant_last".to_string(), false),
                    "variant-init" => ("patch_seq_variant_init".to_string(), false),
                    "make-variant" => ("patch_seq_make_variant".to_string(), false),
                    "make-variant-0" => ("patch_seq_make_variant_0".to_string(), false),
                    "make-variant-1" => ("patch_seq_make_variant_1".to_string(), false),
                    "make-variant-2" => ("patch_seq_make_variant_2".to_string(), false),
                    "make-variant-3" => ("patch_seq_make_variant_3".to_string(), false),
                    "make-variant-4" => ("patch_seq_make_variant_4".to_string(), false),
                    // Float arithmetic operations (dot notation → underscore)
                    "f.add" => ("patch_seq_f_add".to_string(), false),
                    "f.subtract" => ("patch_seq_f_subtract".to_string(), false),
                    "f.multiply" => ("patch_seq_f_multiply".to_string(), false),
                    "f.divide" => ("patch_seq_f_divide".to_string(), false),
                    // Float comparison operations (symbolic → named)
                    "f.=" => ("patch_seq_f_eq".to_string(), false),
                    "f.<" => ("patch_seq_f_lt".to_string(), false),
                    "f.>" => ("patch_seq_f_gt".to_string(), false),
                    "f.<=" => ("patch_seq_f_lte".to_string(), false),
                    "f.>=" => ("patch_seq_f_gte".to_string(), false),
                    "f.<>" => ("patch_seq_f_neq".to_string(), false),
                    // Float type conversions
                    "int->float" => ("patch_seq_int_to_float".to_string(), false),
                    "float->int" => ("patch_seq_float_to_int".to_string(), false),
                    "float->string" => ("patch_seq_float_to_string".to_string(), false),
                    "string->float" => ("patch_seq_string_to_float".to_string(), false),
                    // Check for external builtins (from config)
                    // Then fall through to user-defined words
                    _ => {
                        if let Some(symbol) = self.external_builtins.get(name) {
                            // External builtin from config
                            (symbol.clone(), false)
                        } else {
                            // User-defined word (prefix to avoid C symbol conflicts)
                            // Also mangle special characters for LLVM IR compatibility
                            (format!("seq_{}", mangle_name(name)), true)
                        }
                    }
                };

                // For tail position calls to user-defined words (seq_* functions),
                // emit musttail call with tailcc convention
                // Note: Closures can't use musttail because their signature differs
                if position == TailPosition::Tail && is_seq_word && !self.inside_closure {
                    writeln!(
                        &mut self.output,
                        "  %{} = musttail call tailcc ptr @{}(ptr %{})",
                        result_var, function_name, stack_var
                    )
                    .unwrap();
                    writeln!(&mut self.output, "  ret ptr %{}", result_var).unwrap();
                } else {
                    // Regular call (non-tail, runtime function, or inside closure)
                    writeln!(
                        &mut self.output,
                        "  %{} = call ptr @{}(ptr %{})",
                        result_var, function_name, stack_var
                    )
                    .unwrap();
                }
                Ok(result_var)
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                // NOTE: Stack effect validation is performed by the type checker (see typechecker.rs).
                // Both branches must produce the same stack depth, which is validated before
                // we reach codegen. This ensures the phi node merges compatible stack pointers.

                // Peek the condition value first (doesn't modify stack)
                // Then pop separately to properly free the stack node
                // (prevents memory leak while allowing us to use the value for branching)
                let cond_temp = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
                    cond_temp, stack_var
                )
                .unwrap();

                // Pop the condition from the stack (frees the node)
                let popped_stack = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
                    popped_stack, stack_var
                )
                .unwrap();

                // Compare with 0 (0 = zero, non-zero = non-zero)
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

                // Conditional branch
                writeln!(
                    &mut self.output,
                    "  br i1 %{}, label %{}, label %{}",
                    cmp_temp, then_block, else_block
                )
                .unwrap();

                // Then branch (executed when condition is non-zero)
                // The last statement in the branch inherits our tail position
                writeln!(&mut self.output, "{}:", then_block).unwrap();
                let mut then_stack = popped_stack.clone();
                let then_len = then_branch.len();
                let mut then_emitted_tail_call = false;
                for (i, stmt) in then_branch.iter().enumerate() {
                    let stmt_position = if i == then_len - 1 {
                        position // Last statement inherits our tail position
                    } else {
                        TailPosition::NonTail
                    };
                    // Check if this is the last statement and will emit a tail call
                    if i == then_len - 1 {
                        then_emitted_tail_call = self.will_emit_tail_call(stmt, stmt_position);
                    }
                    then_stack = self.codegen_statement(&then_stack, stmt, stmt_position)?;
                }

                // Only emit landing block if no tail call was emitted
                // (tail calls emit their own ret, so we can't branch after)
                let then_predecessor = if then_emitted_tail_call {
                    // No landing block needed - the tail call already returned
                    "unreachable".to_string()
                } else {
                    // Create landing block for phi node predecessor tracking.
                    let then_pred = self.fresh_block("if_then_end");
                    writeln!(&mut self.output, "  br label %{}", then_pred).unwrap();
                    writeln!(&mut self.output, "{}:", then_pred).unwrap();
                    writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();
                    then_pred
                };

                // Else branch (executed when condition is zero)
                // The last statement in the branch inherits our tail position
                writeln!(&mut self.output, "{}:", else_block).unwrap();
                let mut else_emitted_tail_call = false;
                let else_stack = if let Some(eb) = else_branch {
                    let mut es = popped_stack.clone();
                    let else_len = eb.len();
                    for (i, stmt) in eb.iter().enumerate() {
                        let stmt_position = if i == else_len - 1 {
                            position // Last statement inherits our tail position
                        } else {
                            TailPosition::NonTail
                        };
                        // Check if this is the last statement and will emit a tail call
                        if i == else_len - 1 {
                            else_emitted_tail_call = self.will_emit_tail_call(stmt, stmt_position);
                        }
                        es = self.codegen_statement(&es, stmt, stmt_position)?;
                    }
                    es
                } else {
                    // No else clause - stack unchanged
                    popped_stack.clone()
                };

                // Only emit landing block if no tail call was emitted
                let else_predecessor = if else_emitted_tail_call {
                    "unreachable".to_string()
                } else {
                    let else_pred = self.fresh_block("if_else_end");
                    writeln!(&mut self.output, "  br label %{}", else_pred).unwrap();
                    writeln!(&mut self.output, "{}:", else_pred).unwrap();
                    writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();
                    else_pred
                };

                // If both branches emitted tail calls, we don't need a merge block
                // The function has already returned from both paths
                if then_emitted_tail_call && else_emitted_tail_call {
                    // Both branches returned via tail call, no merge needed
                    // Return a dummy value (won't be used)
                    return Ok(then_stack);
                }

                // Merge block - phi node to merge stack pointers from both paths
                writeln!(&mut self.output, "{}:", merge_block).unwrap();
                let result_var = self.fresh_temp();

                // Build phi node based on which branches reach the merge block
                if then_emitted_tail_call {
                    // Only else branch reaches merge
                    writeln!(
                        &mut self.output,
                        "  %{} = phi ptr [ %{}, %{} ]",
                        result_var, else_stack, else_predecessor
                    )
                    .unwrap();
                } else if else_emitted_tail_call {
                    // Only then branch reaches merge
                    writeln!(
                        &mut self.output,
                        "  %{} = phi ptr [ %{}, %{} ]",
                        result_var, then_stack, then_predecessor
                    )
                    .unwrap();
                } else {
                    // Both branches reach merge
                    writeln!(
                        &mut self.output,
                        "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{} ]",
                        result_var, then_stack, then_predecessor, else_stack, else_predecessor
                    )
                    .unwrap();
                }

                Ok(result_var)
            }

            Statement::Quotation { id, body } => {
                // Get the inferred type for this quotation using its ID
                let quot_type = self.get_quotation_type(*id)?.clone();

                // Generate a function for the quotation body
                let fn_name = self.codegen_quotation(body, &quot_type)?;

                // Get function pointer as usize
                let fn_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    fn_ptr_var, fn_name
                )
                .unwrap();

                // Generate code based on quotation type
                match quot_type {
                    Type::Quotation(_effect) => {
                        // Stateless quotation - use push_quotation
                        let result_var = self.fresh_temp();
                        writeln!(
                            &mut self.output,
                            "  %{} = call ptr @patch_seq_push_quotation(ptr %{}, i64 %{})",
                            result_var, stack_var, fn_ptr_var
                        )
                        .unwrap();
                        Ok(result_var)
                    }
                    Type::Closure {
                        effect: _effect,
                        captures,
                    } => {
                        // Closure with captures - use push_closure
                        let capture_count = captures.len() as i32;
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
        assert!(ir.contains("define tailcc ptr @seq_main(ptr %stack)"));
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
