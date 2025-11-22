//! Seq Compiler Library
//!
//! Provides compilation from .seq source to LLVM IR and executable binaries.

pub mod ast;
pub mod builtins;
pub mod codegen;
pub mod parser;
pub mod typechecker;
pub mod types;
pub mod unification;

pub use ast::Program;
pub use codegen::CodeGen;
pub use parser::Parser;
pub use typechecker::TypeChecker;
pub use types::{Effect, StackType, Type};

use std::fs;
use std::path::Path;
use std::process::Command;

/// Compile a .seq source file to an executable
pub fn compile_file(source_path: &Path, output_path: &Path, keep_ir: bool) -> Result<(), String> {
    // Read source file
    let source = fs::read_to_string(source_path)
        .map_err(|e| format!("Failed to read source file: {}", e))?;

    // Parse
    let mut parser = Parser::new(&source);
    let program = parser.parse()?;

    // Verify we have a main word
    if program.find_word("main").is_none() {
        return Err("No main word defined".to_string());
    }

    // Validate all word calls reference defined words or built-ins
    program.validate_word_calls()?;

    // Type check (validates stack effects, especially for conditionals)
    let mut type_checker = TypeChecker::new();
    type_checker.check_program(&program)?;

    // Extract inferred quotation types (in DFS traversal order)
    let quotation_types = type_checker.take_quotation_types();

    // Generate LLVM IR with type information
    let mut codegen = CodeGen::new();
    let ir = codegen.codegen_program(&program, quotation_types)?;

    // Write IR to file
    let ir_path = output_path.with_extension("ll");
    fs::write(&ir_path, ir).map_err(|e| format!("Failed to write IR file: {}", e))?;

    // Validate runtime library exists
    let runtime_lib = Path::new("target/release/libseq_runtime.a");
    if !runtime_lib.exists() {
        return Err(format!(
            "Runtime library not found at {}. \
             Please run 'cargo build --release -p seq-runtime' first.",
            runtime_lib.display()
        ));
    }

    // Compile IR to executable using clang
    let output = Command::new("clang")
        .arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .arg("-L")
        .arg("target/release")
        .arg("-lseq_runtime")
        .output()
        .map_err(|e| format!("Failed to run clang: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Clang compilation failed:\n{}", stderr));
    }

    // Remove temporary IR file unless user wants to keep it
    if !keep_ir {
        fs::remove_file(&ir_path).ok();
    }

    Ok(())
}

/// Compile source string to LLVM IR string (for testing)
pub fn compile_to_ir(source: &str) -> Result<String, String> {
    let mut parser = Parser::new(source);
    let program = parser.parse()?;

    program.validate_word_calls()?;

    let mut type_checker = TypeChecker::new();
    type_checker.check_program(&program)?;

    let quotation_types = type_checker.take_quotation_types();

    let mut codegen = CodeGen::new();
    codegen.codegen_program(&program, quotation_types)
}
