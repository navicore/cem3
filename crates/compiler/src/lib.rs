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
//! use seqc::{CompilerConfig, ExternalBuiltin, Effect, StackType, Type};
//! use seqc::compile_file_with_config;
//!
//! // Define stack effect: ( Int -- Int )
//! let effect = Effect::new(
//!     StackType::singleton(Type::Int),
//!     StackType::singleton(Type::Int),
//! );
//!
//! let config = CompilerConfig::new()
//!     .with_builtin(ExternalBuiltin::with_effect("my-op", "my_runtime_op", effect));
//!
//! compile_file_with_config(source, output, false, &config)?;
//! ```

pub mod ast;
pub mod builtins;
pub mod call_graph;
pub mod capture_analysis;
pub mod chan_yield_lint;
pub mod codegen;
pub mod config;
pub mod error_flag_lint;
pub mod ffi;
pub mod lint;
pub mod normalize;
pub mod parser;
pub mod resolver;
pub mod resource_lint;
pub mod script;
pub mod stdlib_embed;
pub mod test_runner;
pub mod typechecker;
pub mod types;
pub mod unification;

pub use ast::Program;
pub use chan_yield_lint::ChanYieldAnalyzer;
pub use codegen::CodeGen;
pub use config::{CompilerConfig, ExternalBuiltin, OptimizationLevel};
pub use error_flag_lint::ErrorFlagAnalyzer;
pub use lint::{LintConfig, LintDiagnostic, Linter, Severity};
pub use parser::Parser;
pub use resolver::{
    ResolveResult, Resolver, check_collisions, check_union_collisions, find_stdlib,
};
pub use resource_lint::{ProgramResourceAnalyzer, ResourceAnalyzer};
pub use typechecker::TypeChecker;
pub use types::{Effect, StackType, Type};

use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

/// Embedded runtime library (built by build.rs)
/// On docs.rs, this is an empty slice since the runtime isn't available.
#[cfg(not(docsrs))]
static RUNTIME_LIB: &[u8] = include_bytes!(env!("SEQ_RUNTIME_LIB_PATH"));

#[cfg(docsrs)]
static RUNTIME_LIB: &[u8] = &[];

/// Minimum clang/LLVM version required.
/// Our generated IR uses opaque pointers (`ptr`), which requires LLVM 15+.
const MIN_CLANG_VERSION: u32 = 15;

/// Cache for clang version check result.
/// Stores Ok(version) on success or Err(message) on failure.
static CLANG_VERSION_CHECKED: OnceLock<Result<u32, String>> = OnceLock::new();

/// Check that clang is available and meets minimum version requirements.
/// Returns Ok(version) on success, Err with helpful message on failure.
/// This check is cached - it only runs once per process.
fn check_clang_version() -> Result<u32, String> {
    CLANG_VERSION_CHECKED
        .get_or_init(|| {
            let output = Command::new("clang")
                .arg("--version")
                .output()
                .map_err(|e| {
                    format!(
                        "Failed to run clang: {}. \
                         Please install clang {} or later.",
                        e, MIN_CLANG_VERSION
                    )
                })?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(format!(
                    "clang --version failed with exit code {:?}: {}",
                    output.status.code(),
                    stderr
                ));
            }

            let version_str = String::from_utf8_lossy(&output.stdout);

            // Parse version from output like:
            // "clang version 15.0.0 (...)"
            // "Apple clang version 14.0.3 (...)"  (Apple's versioning differs)
            // "Homebrew clang version 17.0.6"
            let version = parse_clang_version(&version_str).ok_or_else(|| {
                format!(
                    "Could not parse clang version from: {}\n\
                     seqc requires clang {} or later (for opaque pointer support).",
                    version_str.lines().next().unwrap_or(&version_str),
                    MIN_CLANG_VERSION
                )
            })?;

            // Apple clang uses different version numbers - Apple clang 14 is based on LLVM 15
            // For simplicity, we check if it's Apple clang and adjust expectations
            let is_apple = version_str.contains("Apple clang");
            let effective_min = if is_apple { 14 } else { MIN_CLANG_VERSION };

            if version < effective_min {
                return Err(format!(
                    "clang version {} detected, but seqc requires {} {} or later.\n\
                     The generated LLVM IR uses opaque pointers (requires LLVM 15+).\n\
                     Please upgrade your clang installation.",
                    version,
                    if is_apple { "Apple clang" } else { "clang" },
                    effective_min
                ));
            }

            Ok(version)
        })
        .clone()
}

/// Parse major version number from clang --version output
fn parse_clang_version(output: &str) -> Option<u32> {
    // Look for "clang version X.Y.Z" pattern to avoid false positives
    // This handles: "clang version", "Apple clang version", "Homebrew clang version", etc.
    for line in output.lines() {
        if line.contains("clang version")
            && let Some(idx) = line.find("version ")
        {
            let after_version = &line[idx + 8..];
            // Extract the major version number
            let major: String = after_version
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !major.is_empty() {
                return major.parse().ok();
            }
        }
    }
    None
}

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
    let (mut program, ffi_includes) = if !program.includes.is_empty() {
        let stdlib_path = find_stdlib();
        let mut resolver = Resolver::new(stdlib_path);
        let result = resolver.resolve(source_path, program)?;
        (result.program, result.ffi_includes)
    } else {
        (program, Vec::new())
    };

    // Process FFI includes (embedded manifests from `include ffi:*`)
    let mut ffi_bindings = ffi::FfiBindings::new();
    for ffi_name in &ffi_includes {
        let manifest_content = ffi::get_ffi_manifest(ffi_name)
            .ok_or_else(|| format!("FFI manifest '{}' not found", ffi_name))?;
        let manifest = ffi::FfiManifest::parse(manifest_content)?;
        ffi_bindings.add_manifest(&manifest)?;
    }

    // Load external FFI manifests from config (--ffi-manifest)
    for manifest_path in &config.ffi_manifest_paths {
        let manifest_content = fs::read_to_string(manifest_path).map_err(|e| {
            format!(
                "Failed to read FFI manifest '{}': {}",
                manifest_path.display(),
                e
            )
        })?;
        let manifest = ffi::FfiManifest::parse(&manifest_content).map_err(|e| {
            format!(
                "Failed to parse FFI manifest '{}': {}",
                manifest_path.display(),
                e
            )
        })?;
        ffi_bindings.add_manifest(&manifest)?;
    }

    // RFC #345: Fix up type variables that should be union types
    // After resolving includes, we know all union names and can convert
    // Type::Var("UnionName") to Type::Union("UnionName") for proper nominal typing
    program.fixup_union_types();

    // Generate constructor words for all union types (Make-VariantName)
    // Always done here to consolidate constructor generation in one place
    program.generate_constructors()?;

    // Lower literal-quotation `if` triples to `Statement::If` so the
    // typechecker, codegen, type-specializer, and lints all see the
    // same shape they see for the keyword form. (See `normalize.rs`.)
    normalize::lower_literal_if_combinators(&mut program);

    // Check for word name collisions
    check_collisions(&program.words)?;

    // Check for union name collisions across modules
    check_union_collisions(&program.unions)?;

    // Verify we have a main word
    if program.find_word("main").is_none() {
        return Err("No main word defined".to_string());
    }

    // Validate all word calls reference defined words or built-ins
    // Include external builtins from config and FFI functions
    let mut external_names = config.external_names();
    external_names.extend(ffi_bindings.function_names());
    program.validate_word_calls_with_externals(&external_names)?;

    // Build call graph for mutual recursion detection (Issue #229)
    let call_graph = call_graph::CallGraph::build(&program);

    // Type check (validates stack effects, especially for conditionals)
    let mut type_checker = TypeChecker::new();
    type_checker.set_call_graph(call_graph.clone());

    // Register external builtins with the type checker
    // All external builtins must have explicit effects (v2.0 requirement)
    if !config.external_builtins.is_empty() {
        for builtin in &config.external_builtins {
            if builtin.effect.is_none() {
                return Err(format!(
                    "External builtin '{}' is missing a stack effect declaration.\n\
                     All external builtins must have explicit effects for type safety.",
                    builtin.seq_name
                ));
            }
        }
        let external_effects: Vec<(&str, &types::Effect)> = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.as_str(), b.effect.as_ref().unwrap()))
            .collect();
        type_checker.register_external_words(&external_effects);
    }

    // Register FFI functions with the type checker
    if !ffi_bindings.functions.is_empty() {
        let ffi_effects: Vec<(&str, &types::Effect)> = ffi_bindings
            .functions
            .values()
            .map(|f| (f.seq_name.as_str(), &f.effect))
            .collect();
        type_checker.register_external_words(&ffi_effects);
    }

    type_checker.check_program(&program)?;

    // Extract inferred quotation types (in DFS traversal order)
    let quotation_types = type_checker.take_quotation_types();
    // Extract per-statement type info for optimization (Issue #186)
    let statement_types = type_checker.take_statement_top_types();
    // Extract per-word aux stack max depths for codegen (Issue #350)
    let aux_max_depths = type_checker.take_aux_max_depths();
    // Extract per-quotation aux stack max depths for codegen (Issue #393)
    let quotation_aux_depths = type_checker.take_quotation_aux_depths();
    // Extract resolved arithmetic sugar for codegen
    let resolved_sugar = type_checker.take_resolved_sugar();

    // Generate LLVM IR with type information and external builtins
    // Note: Mutual TCO already works via existing musttail emission for all
    // user-word tail calls. The call_graph is used by type checker for
    // divergent branch detection, not by codegen.
    let mut codegen = if config.pure_inline_test {
        CodeGen::new_pure_inline_test()
    } else {
        CodeGen::new()
    };
    codegen.set_aux_slot_counts(aux_max_depths);
    codegen.set_quotation_aux_slot_counts(quotation_aux_depths);
    codegen.set_resolved_sugar(resolved_sugar);
    codegen.set_source_file(source_path.to_path_buf());
    codegen.set_call_graph(call_graph);
    let ir = codegen
        .codegen_program_with_ffi(
            &program,
            quotation_types,
            statement_types,
            config,
            &ffi_bindings,
        )
        .map_err(|e| e.to_string())?;

    // Write IR to file
    let ir_path = output_path.with_extension("ll");
    fs::write(&ir_path, ir).map_err(|e| format!("Failed to write IR file: {}", e))?;

    // Check clang version before attempting to compile
    check_clang_version()?;

    // Extract embedded runtime library to a temp file
    let runtime_path = std::env::temp_dir().join("libseq_runtime.a");
    {
        let mut file = fs::File::create(&runtime_path)
            .map_err(|e| format!("Failed to create runtime lib: {}", e))?;
        file.write_all(RUNTIME_LIB)
            .map_err(|e| format!("Failed to write runtime lib: {}", e))?;
    }

    // Build clang command with library paths
    let opt_flag = match config.optimization_level {
        config::OptimizationLevel::O0 => "-O0",
        config::OptimizationLevel::O1 => "-O1",
        config::OptimizationLevel::O2 => "-O2",
        config::OptimizationLevel::O3 => "-O3",
    };
    let mut clang = Command::new("clang");
    clang
        .arg(opt_flag)
        // Preserve DWARF emitted by codegen so runtime panics resolve
        // back to .seq:line via the standard backtrace path. Pure metadata
        // — no runtime cost; only increases binary size.
        .arg("-g")
        .arg(&ir_path)
        .arg("-o")
        .arg(output_path)
        .arg("-L")
        .arg(runtime_path.parent().unwrap())
        .arg("-lseq_runtime")
        // libm: float math builtins (f.sin, f.exp, f.pow, …) call libm
        // symbols from the runtime archive. Clang only auto-links libm
        // for math used directly in the IR, not for math reached via the
        // runtime, so the link must be explicit. Harmless on macOS where
        // libm is part of libSystem.
        .arg("-lm");

    // Add custom library paths from config
    for lib_path in &config.library_paths {
        clang.arg("-L").arg(lib_path);
    }

    // Add custom libraries from config
    for lib in &config.libraries {
        clang.arg("-l").arg(lib);
    }

    // Add FFI linker flags
    for lib in &ffi_bindings.linker_flags {
        clang.arg("-l").arg(lib);
    }

    // Strip code unreachable from `seq_main` at the final link, so the
    // binary contains only the runtime machinery the source program
    // could possibly execute. ld64 walks atom-level reachability from
    // the entry point; GNU ld / lld do the same via section
    // reachability. Other targets (notably Windows-MSVC, where clang
    // calls `link.exe`) get no flag — `--gc-sections` would be
    // rejected. See docs/design/done/NO_DEAD_CODE.md.
    if cfg!(target_os = "macos") {
        clang.arg("-Wl,-dead_strip");
    } else if cfg!(target_os = "linux") {
        clang.arg("-Wl,--gc-sections");
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

    normalize::lower_literal_if_combinators(&mut program);

    let external_names = config.external_names();
    program.validate_word_calls_with_externals(&external_names)?;

    let mut type_checker = TypeChecker::new();

    // Register external builtins with the type checker
    // All external builtins must have explicit effects (v2.0 requirement)
    if !config.external_builtins.is_empty() {
        for builtin in &config.external_builtins {
            if builtin.effect.is_none() {
                return Err(format!(
                    "External builtin '{}' is missing a stack effect declaration.\n\
                     All external builtins must have explicit effects for type safety.",
                    builtin.seq_name
                ));
            }
        }
        let external_effects: Vec<(&str, &types::Effect)> = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.as_str(), b.effect.as_ref().unwrap()))
            .collect();
        type_checker.register_external_words(&external_effects);
    }

    type_checker.check_program(&program)?;

    // Build the call graph for self-recursion classification (loop lowering).
    let call_graph = call_graph::CallGraph::build(&program);

    let quotation_types = type_checker.take_quotation_types();
    let statement_types = type_checker.take_statement_top_types();
    let aux_max_depths = type_checker.take_aux_max_depths();
    let quotation_aux_depths = type_checker.take_quotation_aux_depths();
    let resolved_sugar = type_checker.take_resolved_sugar();

    let mut codegen = CodeGen::new();
    codegen.set_aux_slot_counts(aux_max_depths);
    codegen.set_quotation_aux_slot_counts(quotation_aux_depths);
    codegen.set_resolved_sugar(resolved_sugar);
    codegen.set_call_graph(call_graph);
    codegen
        .codegen_program_with_config(&program, quotation_types, statement_types, config)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
