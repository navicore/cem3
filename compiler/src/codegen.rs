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
use crate::types::Type;
use std::collections::HashMap;
use std::fmt::Write as _;

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
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen {
            output: String::new(),
            string_globals: String::new(),
            temp_counter: 0,
            string_counter: 0,
            block_counter: 0,
            quot_counter: 0,
            string_constants: HashMap::new(),
            quotation_functions: String::new(),
            type_map: HashMap::new(),
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
        // Store type map for use during code generation
        self.type_map = type_map;

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
        writeln!(&mut ir, "declare ptr @patch_seq_chan_receive(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_close_channel(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_yield_strand(ptr)").unwrap();
        writeln!(&mut ir, "; Scheduler operations").unwrap();
        writeln!(&mut ir, "declare void @patch_seq_scheduler_init()").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_scheduler_run()").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_strand_spawn(ptr, ptr)").unwrap();
        writeln!(&mut ir, "; TCP operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_listen(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_accept(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_read(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_write(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_tcp_close(ptr)").unwrap();
        writeln!(&mut ir, "; String operations").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_concat(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_length(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_split(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_contains(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_starts_with(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_empty(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_trim(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_upper(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_string_to_lower(ptr)").unwrap();
        writeln!(&mut ir, "; Helpers for conditionals").unwrap();
        writeln!(&mut ir, "declare i64 @patch_seq_peek_int_value(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @patch_seq_pop_stack(ptr)").unwrap();
        writeln!(&mut ir).unwrap();

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
        let function_name = format!("seq_{}", word.name);
        writeln!(
            &mut self.output,
            "define ptr @{}(ptr %stack) {{",
            function_name
        )
        .unwrap();
        writeln!(&mut self.output, "entry:").unwrap();

        let mut stack_var = "stack".to_string();

        // Generate code for each statement
        for statement in &word.body {
            stack_var = self.codegen_statement(&stack_var, statement)?;
        }

        writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
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
        match quot_type {
            Type::Quotation(_) => {
                // Stateless quotation: fn(Stack) -> Stack
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack) {{",
                    function_name
                )
                .unwrap();
            }
            Type::Closure { captures, .. } => {
                // Closure: fn(Stack, *const Value, usize) -> Stack
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
                        _ => {
                            // TODO: Implement type-specific getters for Bool and other types
                            // Each type needs:
                            //   - Runtime: env_get_<type> in closures.rs
                            //   - CodeGen: Match arm here to call the right getter
                            return Err(format!(
                                "CodeGen: Only Int and String captures are currently supported, got {:?}. \
                                 Other types require implementing env_get_<type> functions.",
                                capture_type
                            ));
                        }
                    }
                }

                // Generate code for each statement in the quotation body
                for statement in body {
                    stack_var = self.codegen_statement(&stack_var, statement)?;
                }

                writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
                writeln!(&mut self.output, "}}").unwrap();
                writeln!(&mut self.output).unwrap();

                // Move generated function to quotation_functions
                self.quotation_functions.push_str(&self.output);

                // Restore original output
                self.output = saved_output;

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

        // Generate code for each statement in the quotation body
        for statement in body {
            stack_var = self.codegen_statement(&stack_var, statement)?;
        }

        writeln!(&mut self.output, "  ret ptr %{}", stack_var).unwrap();
        writeln!(&mut self.output, "}}").unwrap();
        writeln!(&mut self.output).unwrap();

        // Move generated function to quotation_functions
        self.quotation_functions.push_str(&self.output);

        // Restore original output
        self.output = saved_output;

        Ok(function_name)
    }

    /// Generate code for a single statement
    fn codegen_statement(
        &mut self,
        stack_var: &str,
        statement: &Statement,
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
                let function_name = match name.as_str() {
                    // I/O operations
                    "write_line" | "read_line" => format!("patch_seq_{}", name),
                    "int->string" => "patch_seq_int_to_string".to_string(),
                    // Arithmetic operations
                    "add" | "subtract" | "multiply" | "divide" => format!("patch_seq_{}", name),
                    // Comparison operations (symbolic → named)
                    // These return Int (0 or 1) for Forth-style boolean semantics
                    "=" => "patch_seq_eq".to_string(),
                    "<" => "patch_seq_lt".to_string(),
                    ">" => "patch_seq_gt".to_string(),
                    "<=" => "patch_seq_lte".to_string(),
                    ">=" => "patch_seq_gte".to_string(),
                    "<>" => "patch_seq_neq".to_string(),
                    // Boolean operations
                    "and" | "or" | "not" => format!("patch_seq_{}", name),
                    // Stack operations (simple - no parameters)
                    "dup" | "swap" | "over" | "rot" | "nip" | "tuck" => {
                        format!("patch_seq_{}", name)
                    }
                    "drop" => "patch_seq_drop_op".to_string(), // 'drop' is reserved in LLVM IR
                    "pick" => "patch_seq_pick_op".to_string(), // pick takes Int parameter from stack
                    // Concurrency operations (hyphen → underscore for C compatibility)
                    "make-channel" => "patch_seq_make_channel".to_string(),
                    "send" => "patch_seq_chan_send".to_string(),
                    "receive" => "patch_seq_chan_receive".to_string(),
                    "close-channel" => "patch_seq_close_channel".to_string(),
                    "yield" => "patch_seq_yield_strand".to_string(),
                    // Quotation operations
                    "call" => "patch_seq_call".to_string(),
                    "times" => "patch_seq_times".to_string(),
                    "while" => "patch_seq_while_loop".to_string(),
                    "until" => "patch_seq_until_loop".to_string(),
                    "forever" => "patch_seq_forever".to_string(),
                    "spawn" => "patch_seq_spawn".to_string(),
                    "cond" => "patch_seq_cond".to_string(),
                    // TCP operations (hyphen → underscore for C compatibility)
                    "tcp-listen" => "patch_seq_tcp_listen".to_string(),
                    "tcp-accept" => "patch_seq_tcp_accept".to_string(),
                    "tcp-read" => "patch_seq_tcp_read".to_string(),
                    "tcp-write" => "patch_seq_tcp_write".to_string(),
                    "tcp-close" => "patch_seq_tcp_close".to_string(),
                    // String operations (hyphen → underscore for C compatibility)
                    "string-concat" => "patch_seq_string_concat".to_string(),
                    "string-length" => "patch_seq_string_length".to_string(),
                    "string-split" => "patch_seq_string_split".to_string(),
                    "string-contains" => "patch_seq_string_contains".to_string(),
                    "string-starts-with" => "patch_seq_string_starts_with".to_string(),
                    "string-empty" => "patch_seq_string_empty".to_string(),
                    "string-trim" => "patch_seq_string_trim".to_string(),
                    "string-to-upper" => "patch_seq_string_to_upper".to_string(),
                    "string-to-lower" => "patch_seq_string_to_lower".to_string(),
                    // User-defined word (prefix to avoid C symbol conflicts)
                    _ => format!("seq_{}", name),
                };
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @{}(ptr %{})",
                    result_var, function_name, stack_var
                )
                .unwrap();
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
                writeln!(&mut self.output, "{}:", then_block).unwrap();
                let mut then_stack = popped_stack.clone();
                for stmt in then_branch {
                    then_stack = self.codegen_statement(&then_stack, stmt)?;
                }
                // Create landing block for phi node predecessor tracking.
                // This is CRITICAL for nested conditionals: if then_branch contains
                // another if statement, the actual control flow predecessor is the
                // inner if's merge block, not then_block. The landing block ensures
                // the phi node always references the correct immediate predecessor.
                let then_predecessor = self.fresh_block("if_then_end");
                writeln!(&mut self.output, "  br label %{}", then_predecessor).unwrap();
                writeln!(&mut self.output, "{}:", then_predecessor).unwrap();
                writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();

                // Else branch (executed when condition is zero)
                writeln!(&mut self.output, "{}:", else_block).unwrap();
                let else_stack = if let Some(eb) = else_branch {
                    let mut es = popped_stack.clone();
                    for stmt in eb {
                        es = self.codegen_statement(&es, stmt)?;
                    }
                    es
                } else {
                    // No else clause - stack unchanged
                    popped_stack.clone()
                };
                // Landing block for else branch (same reasoning as then_branch)
                let else_predecessor = self.fresh_block("if_else_end");
                writeln!(&mut self.output, "  br label %{}", else_predecessor).unwrap();
                writeln!(&mut self.output, "{}:", else_predecessor).unwrap();
                writeln!(&mut self.output, "  br label %{}", merge_block).unwrap();

                // Merge block - phi node to merge stack pointers from both paths
                writeln!(&mut self.output, "{}:", merge_block).unwrap();
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{} ]",
                    result_var, then_stack, then_predecessor, else_stack, else_predecessor
                )
                .unwrap();

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
        writeln!(&mut self.output, "define i32 @main() {{").unwrap();
        writeln!(&mut self.output, "entry:").unwrap();

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
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("Hello, World!".to_string()),
                    Statement::WordCall("write_line".to_string()),
                ],
            }],
        };

        let ir = codegen.codegen_program(&program, HashMap::new()).unwrap();

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("define ptr @seq_main(ptr %stack)"));
        assert!(ir.contains("call ptr @patch_seq_push_string"));
        assert!(ir.contains("call ptr @patch_seq_write_line"));
        assert!(ir.contains("\"Hello, World!\\00\""));
    }

    #[test]
    fn test_codegen_arithmetic() {
        let mut codegen = CodeGen::new();

        let program = Program {
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall("add".to_string()),
                ],
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
}
