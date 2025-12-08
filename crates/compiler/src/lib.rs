//! Seq Compiler Library
//!
//! Provides compilation from .seq source to LLVM IR and executable binaries.
//!
//! # Extending the Compiler
//!
//! External projects can extend the compiler with additional builtins using
//! [`CompilerConfig`]:
//!
//! ```rust,ignore
//! use seqc::{CompilerConfig, ExternalBuiltin, compile_file_with_config};
//!
//! let config = CompilerConfig::new()
//!     .with_builtin(ExternalBuiltin::new("my-op", "my_runtime_op"));
//!
//! compile_file_with_config(source, output, false, &config)?;
//! ```

pub mod ast;
pub mod builtins;
pub mod capture_analysis;
pub mod codegen;
pub mod config;
pub mod parser;
pub mod resolver;
pub mod stdlib_embed;
pub mod typechecker;
pub mod types;
pub mod unification;

pub use ast::Program;
pub use codegen::CodeGen;
pub use config::{CompilerConfig, ExternalBuiltin};
pub use parser::Parser;
pub use resolver::{Resolver, check_collisions, check_union_collisions, find_stdlib};
pub use typechecker::TypeChecker;
pub use types::{Effect, StackType, Type};

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// Embedded runtime library (built by build.rs)
static RUNTIME_LIB: &[u8] = include_bytes!(env!("SEQ_RUNTIME_LIB_PATH"));

/// Compile a .seq source file to an executable
pub fn compile_file(source_path: &Path, output_path: &Path, keep_ir: bool) -> Result<(), String> {
    compile_file_with_config(
        source_path,
        output_path,
        keep_ir,
        &CompilerConfig::default(),
    )
}

/// Compile a .seq source file to an executable with custom configuration
///
/// This allows external projects to extend the compiler with additional
/// builtins and link against additional libraries.
pub fn compile_file_with_config(
    source_path: &Path,
    output_path: &Path,
    keep_ir: bool,
    config: &CompilerConfig,
) -> Result<(), String> {
    // Read source file
    let source = fs::read_to_string(source_path)
        .map_err(|e| format!("Failed to read source file: {}", e))?;

    // Parse
    let mut parser = Parser::new(&source);
    let program = parser.parse()?;

    // Resolve includes (if any)
    let mut program = if !program.includes.is_empty() {
        let stdlib_path = find_stdlib();
        let mut resolver = Resolver::new(stdlib_path);
        resolver.resolve(source_path, program)?
    } else {
        program
    };

    // Generate constructor words for all union types (Make-VariantName)
    // Always done here to consolidate constructor generation in one place
    program.generate_constructors()?;

    // Check for word name collisions
    check_collisions(&program.words)?;

    // Check for union name collisions across modules
    check_union_collisions(&program.unions)?;

    // Verify we have a main word
    if program.find_word("main").is_none() {
        return Err("No main word defined".to_string());
    }

    // Validate all word calls reference defined words or built-ins
    // Include external builtins from config
    let external_names = config.external_names();
    program.validate_word_calls_with_externals(&external_names)?;

    // Type check (validates stack effects, especially for conditionals)
    let mut type_checker = TypeChecker::new();

    // Register external builtins with the type checker
    if !config.external_builtins.is_empty() {
        let external_effects: Vec<(&str, Option<&types::Effect>)> = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.as_str(), b.effect.as_ref()))
            .collect();
        type_checker.register_external_words(&external_effects);
    }

    type_checker.check_program(&program)?;

    // Extract inferred quotation types (in DFS traversal order)
    let quotation_types = type_checker.take_quotation_types();

    // Generate LLVM IR with type information and external builtins
    let mut codegen = CodeGen::new();
    let ir = codegen.codegen_program_with_config(&program, quotation_types, config)?;

    // Write IR to file
    let ir_path = output_path.with_extension("ll");
    fs::write(&ir_path, ir).map_err(|e| format!("Failed to write IR file: {}", e))?;

    // Extract embedded runtime library to a temp file
    let runtime_path = std::env::temp_dir().join("libseq_runtime.a");
    {
        let mut file = fs::File::create(&runtime_path)
            .map_err(|e| format!("Failed to create runtime lib: {}", e))?;
        file.write_all(RUNTIME_LIB)
            .map_err(|e| format!("Failed to write runtime lib: {}", e))?;
    }

    // Build clang command with library paths
    let mut clang = Command::new("clang");
    clang
        .arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .arg("-L")
        .arg(runtime_path.parent().unwrap())
        .arg("-lseq_runtime");

    // Add custom library paths from config
    for lib_path in &config.library_paths {
        clang.arg("-L").arg(lib_path);
    }

    // Add custom libraries from config
    for lib in &config.libraries {
        clang.arg("-l").arg(lib);
    }

    let output = clang
        .output()
        .map_err(|e| format!("Failed to run clang: {}", e))?;

    // Clean up temp runtime lib
    fs::remove_file(&runtime_path).ok();

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
    compile_to_ir_with_config(source, &CompilerConfig::default())
}

/// Compile source string to LLVM IR string with custom configuration
pub fn compile_to_ir_with_config(source: &str, config: &CompilerConfig) -> Result<String, String> {
    let mut parser = Parser::new(source);
    let mut program = parser.parse()?;

    // Generate constructors for unions
    if !program.unions.is_empty() {
        program.generate_constructors()?;
    }

    let external_names = config.external_names();
    program.validate_word_calls_with_externals(&external_names)?;

    let mut type_checker = TypeChecker::new();

    // Register external builtins with the type checker
    if !config.external_builtins.is_empty() {
        let external_effects: Vec<(&str, Option<&types::Effect>)> = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.as_str(), b.effect.as_ref()))
            .collect();
        type_checker.register_external_words(&external_effects);
    }

    type_checker.check_program(&program)?;

    let quotation_types = type_checker.take_quotation_types();

    let mut codegen = CodeGen::new();
    codegen.codegen_program_with_config(&program, quotation_types, config)
}
