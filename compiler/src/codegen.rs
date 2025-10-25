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
use std::collections::HashMap;
use std::fmt::Write as _;

pub struct CodeGen {
    output: String,
    string_globals: String,
    temp_counter: usize,
    string_counter: usize,
    block_counter: usize, // For generating unique block labels
    string_constants: HashMap<String, String>, // string content -> global name
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen {
            output: String::new(),
            string_globals: String::new(),
            temp_counter: 0,
            string_counter: 0,
            block_counter: 0,
            string_constants: HashMap::new(),
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
    pub fn codegen_program(&mut self, program: &Program) -> Result<String, String> {
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

        // String constants
        if !self.string_globals.is_empty() {
            ir.push_str(&self.string_globals);
            writeln!(&mut ir).unwrap();
        }

        // Runtime function declarations
        writeln!(&mut ir, "; Runtime function declarations").unwrap();
        writeln!(&mut ir, "declare ptr @push_int(ptr, i64)").unwrap();
        writeln!(&mut ir, "declare ptr @push_string(ptr, ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @write_line(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @read_line(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @int_to_string(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @add(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @subtract(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @multiply(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @divide(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @eq(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @lt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @gt(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @lte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @gte(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @neq(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @dup(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @drop_op(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @swap(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @over(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @rot(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @nip(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @tuck(ptr)").unwrap();
        writeln!(&mut ir, "; Concurrency operations").unwrap();
        writeln!(&mut ir, "declare ptr @make_channel(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @send(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @receive(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @close_channel(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @yield_strand(ptr)").unwrap();
        writeln!(&mut ir, "; Helpers for conditionals").unwrap();
        writeln!(&mut ir, "declare i64 @peek_int_value(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @pop_stack(ptr)").unwrap();
        writeln!(&mut ir).unwrap();

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Generate code for a word definition
    fn codegen_word(&mut self, word: &WordDef) -> Result<(), String> {
        // Prefix word names with "cem_" to avoid conflicts with C symbols
        let function_name = format!("cem_{}", word.name);
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
                    "  %{} = call ptr @push_int(ptr %{}, i64 {})",
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
                    "  %{} = call ptr @push_int(ptr %{}, i64 {})",
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
                    "  %{} = call ptr @push_string(ptr %{}, ptr %{})",
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
                // - User words get 'cem_' prefix to avoid C symbol conflicts
                let function_name = match name.as_str() {
                    // I/O operations
                    "write_line" | "read_line" => name.to_string(),
                    "int->string" => "int_to_string".to_string(),
                    // Arithmetic operations
                    "add" | "subtract" | "multiply" | "divide" => name.to_string(),
                    // Comparison operations (symbolic → named)
                    // These return Int (0 or 1) for Forth-style boolean semantics
                    "=" => "eq".to_string(),
                    "<" => "lt".to_string(),
                    ">" => "gt".to_string(),
                    "<=" => "lte".to_string(),
                    ">=" => "gte".to_string(),
                    "<>" => "neq".to_string(),
                    // Stack operations (simple - no parameters)
                    "dup" | "swap" | "over" | "rot" | "nip" | "tuck" => name.to_string(),
                    "drop" => "drop_op".to_string(), // 'drop' is reserved in LLVM IR
                    // Concurrency operations (hyphen → underscore for C compatibility)
                    "make-channel" => "make_channel".to_string(),
                    "send" => "send".to_string(),
                    "receive" => "receive".to_string(),
                    "close-channel" => "close_channel".to_string(),
                    "yield" => "yield_strand".to_string(),
                    // User-defined word (prefix to avoid C symbol conflicts)
                    _ => format!("cem_{}", name),
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
                    "  %{} = call i64 @peek_int_value(ptr %{})",
                    cond_temp, stack_var
                )
                .unwrap();

                // Pop the condition from the stack (frees the node)
                let popped_stack = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @pop_stack(ptr %{})",
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
        }
    }

    /// Generate main function that calls user's main word
    fn codegen_main(&mut self) -> Result<(), String> {
        writeln!(&mut self.output, "define i32 @main() {{").unwrap();
        writeln!(&mut self.output, "entry:").unwrap();
        writeln!(&mut self.output, "  %0 = call ptr @cem_main(ptr null)").unwrap();
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

        let ir = codegen.codegen_program(&program).unwrap();

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("define ptr @cem_main(ptr %stack)"));
        assert!(ir.contains("call ptr @push_string"));
        assert!(ir.contains("call ptr @write_line"));
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

        let ir = codegen.codegen_program(&program).unwrap();

        assert!(ir.contains("call ptr @push_int(ptr %stack, i64 2)"));
        assert!(ir.contains("call ptr @push_int"));
        assert!(ir.contains("call ptr @add"));
    }

    #[test]
    fn test_escape_llvm_string() {
        assert_eq!(CodeGen::escape_llvm_string("hello"), "hello");
        assert_eq!(CodeGen::escape_llvm_string("a\nb"), r"a\0Ab");
        assert_eq!(CodeGen::escape_llvm_string("a\tb"), r"a\09b");
        assert_eq!(CodeGen::escape_llvm_string("a\"b"), r"a\22b");
    }
}
