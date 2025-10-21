//! cem3 Compiler Library
//!
//! Provides compilation from .cem source to LLVM IR and executable binaries.

pub mod ast;
pub mod codegen;
pub mod parser;

pub use ast::Program;
pub use codegen::CodeGen;
pub use parser::Parser;

use std::fs;
use std::path::Path;
use std::process::Command;

/// Compile a .cem source file to an executable
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

    // Generate LLVM IR
    let mut codegen = CodeGen::new();
    let ir = codegen.codegen_program(&program)?;

    // Write IR to file
    let ir_path = output_path.with_extension("ll");
    fs::write(&ir_path, ir).map_err(|e| format!("Failed to write IR file: {}", e))?;

    // Compile IR to executable using clang
    let output = Command::new("clang")
        .arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .arg("-L")
        .arg("target/release")
        .arg("-lcem3_runtime")
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

    let mut codegen = CodeGen::new();
    codegen.codegen_program(&program)
}
