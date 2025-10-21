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
    string_constants: HashMap<String, String>, // string content -> global name
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen {
            output: String::new(),
            string_globals: String::new(),
            temp_counter: 0,
            string_counter: 0,
            string_constants: HashMap::new(),
        }
    }

    /// Generate a fresh temporary variable name
    fn fresh_temp(&mut self) -> String {
        let name = format!("{}", self.temp_counter);
        self.temp_counter += 1;
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

        self.string_constants.insert(s.to_string(), global_name.clone());
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
        writeln!(&mut ir, "declare ptr @add(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @subtract(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @multiply(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @divide(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @dup(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @drop_op(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @swap(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @over(ptr)").unwrap();
        writeln!(&mut ir, "declare ptr @rot(ptr)").unwrap();
        writeln!(&mut ir).unwrap();

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Generate code for a word definition
    fn codegen_word(&mut self, word: &WordDef) -> Result<(), String> {
        // Prefix word names with "cem_" to avoid conflicts with C symbols
        let function_name = format!("cem_{}", word.name);
        writeln!(&mut self.output, "define ptr @{}(ptr %stack) {{", function_name).unwrap();
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
    fn codegen_statement(&mut self, stack_var: &str, statement: &Statement) -> Result<String, String> {
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
                // Check if it's a runtime built-in, otherwise it's a user word
                let function_name = match name.as_str() {
                    "write_line" | "read_line" | "add" | "subtract" | "multiply" | "divide" |
                    "dup" | "swap" | "over" | "rot" => name.to_string(),
                    "drop" => "drop_op".to_string(), // drop is reserved in LLVM
                    _ => format!("cem_{}", name), // User-defined word
                };
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @{}(ptr %{})",
                    result_var, function_name, stack_var
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
                body: vec![
                    Statement::StringLiteral("Hello, World!".to_string()),
                    Statement::WordCall("write_line".to_string()),
                ],
            }],
        };

        let ir = codegen.codegen_program(&program).unwrap();

        assert!(ir.contains("define i32 @main()"));
        assert!(ir.contains("define ptr @main(ptr %stack)"));
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
