//! Seq Compiler CLI
//!
//! Command-line interface for compiling .seq programs to executables
//! and running lint checks.

use clap::{CommandFactory, Parser as ClapParser, Subcommand};
use clap_complete::{Shell, generate};
use std::io;
use std::path::{Path, PathBuf};
use std::process;

#[derive(ClapParser)]
#[command(name = "seqc")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Seq compiler - compile .seq programs to executables", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .seq file to an executable
    Build {
        /// Input .seq source file
        input: PathBuf,

        /// Output executable path (defaults to input filename without .seq extension)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Keep intermediate LLVM IR file (.ll)
        #[arg(long)]
        keep_ir: bool,

        /// External FFI manifest file(s) to load
        #[arg(long = "ffi-manifest", value_name = "PATH")]
        ffi_manifests: Vec<PathBuf>,
    },

    /// Run lint checks on .seq files
    Lint {
        /// Input .seq files or directories to lint
        #[arg(required = true)]
        paths: Vec<PathBuf>,

        /// Path to custom lint configuration (TOML)
        #[arg(long)]
        config: Option<PathBuf>,

        /// Only show errors (not warnings or hints)
        #[arg(long)]
        errors_only: bool,

        /// Treat warnings as errors (exit with failure if any warnings)
        #[arg(long)]
        deny_warnings: bool,
    },

    /// Generate shell completion scripts
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Run tests in .seq files
    Test {
        /// Directories or files to test (defaults to current directory)
        #[arg(default_value = ".")]
        paths: Vec<PathBuf>,

        /// Filter: only run tests matching this pattern
        #[arg(short, long)]
        filter: Option<String>,

        /// Verbose output (show timing for each test)
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            input,
            output,
            keep_ir,
            ffi_manifests,
        } => {
            let output = output.unwrap_or_else(|| {
                // Default: input filename without .seq extension
                let stem = input.file_stem().unwrap_or_default();
                PathBuf::from(stem)
            });
            run_build(&input, &output, keep_ir, &ffi_manifests);
        }
        Commands::Lint {
            paths,
            config,
            errors_only,
            deny_warnings,
        } => {
            run_lint(&paths, config.as_deref(), errors_only, deny_warnings);
        }
        Commands::Completions { shell } => {
            run_completions(shell);
        }
        Commands::Test {
            paths,
            filter,
            verbose,
        } => {
            run_test(&paths, filter, verbose);
        }
    }
}

fn run_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "seqc", &mut io::stdout());
}

fn run_build(input: &Path, output: &Path, keep_ir: bool, ffi_manifests: &[PathBuf]) {
    // Build config with external FFI manifests
    let config = if ffi_manifests.is_empty() {
        seqc::CompilerConfig::default()
    } else {
        seqc::CompilerConfig::new().with_ffi_manifests(ffi_manifests.iter().cloned())
    };

    match seqc::compile_file_with_config(input, output, keep_ir, &config) {
        Ok(_) => {
            println!("Compiled {} -> {}", input.display(), output.display());

            if keep_ir {
                let ir_path = output.with_extension("ll");
                if ir_path.exists() {
                    println!("IR saved to {}", ir_path.display());
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}

fn run_lint(
    paths: &[PathBuf],
    config_path: Option<&std::path::Path>,
    errors_only: bool,
    deny_warnings: bool,
) {
    use seqc::lint;
    use std::fs;

    // Load lint configuration
    let config = match config_path {
        Some(path) => {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading lint config: {}", e);
                    process::exit(1);
                }
            };
            match lint::LintConfig::from_toml(&content) {
                Ok(user_config) => {
                    // Merge with defaults
                    let mut default = match lint::LintConfig::default_config() {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("Error loading default lint config: {}", e);
                            process::exit(1);
                        }
                    };
                    default.merge(user_config);
                    default
                }
                Err(e) => {
                    eprintln!("Error parsing lint config: {}", e);
                    process::exit(1);
                }
            }
        }
        None => match lint::LintConfig::default_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error loading default lint config: {}", e);
                process::exit(1);
            }
        },
    };

    let linter = match lint::Linter::new(&config) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error creating linter: {}", e);
            process::exit(1);
        }
    };

    let mut all_diagnostics = Vec::new();
    let mut files_checked = 0;

    for path in paths {
        if path.is_dir() {
            // Recursively find .seq files
            for entry in walkdir(path) {
                if entry.extension().is_some_and(|e| e == "seq") {
                    lint_file(&entry, &linter, &mut all_diagnostics);
                    files_checked += 1;
                }
            }
        } else if path.exists() {
            lint_file(path, &linter, &mut all_diagnostics);
            files_checked += 1;
        } else {
            eprintln!("Warning: {} does not exist", path.display());
        }
    }

    // Filter if errors_only
    if errors_only {
        all_diagnostics.retain(|d| d.severity == lint::Severity::Error);
    }

    // Print results
    if all_diagnostics.is_empty() {
        println!("No lint issues found in {} file(s)", files_checked);
    } else {
        print!("{}", lint::format_diagnostics(&all_diagnostics));
        println!(
            "\n{} issue(s) in {} file(s)",
            all_diagnostics.len(),
            files_checked
        );
        // Exit with error if there are any errors
        let has_errors = all_diagnostics
            .iter()
            .any(|d| d.severity == lint::Severity::Error);
        let has_warnings = all_diagnostics
            .iter()
            .any(|d| d.severity == lint::Severity::Warning);

        if has_errors || (deny_warnings && has_warnings) {
            process::exit(1);
        }
    }
}

fn lint_file(path: &PathBuf, linter: &seqc::Linter, diagnostics: &mut Vec<seqc::LintDiagnostic>) {
    use seqc::Parser;
    use std::fs;

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path.display(), e);
            return;
        }
    };

    let mut parser = Parser::new(&source);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parse error in {}: {}", path.display(), e);
            return;
        }
    };

    let file_diagnostics = linter.lint_program(&program, path);
    diagnostics.extend(file_diagnostics);
}

/// Simple recursive directory walker with error logging
fn walkdir(dir: &Path) -> Vec<PathBuf> {
    use std::fs;

    let mut files = Vec::new();
    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.is_dir() {
                            files.extend(walkdir(&path));
                        } else {
                            files.push(path);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not read directory entry in {}: {}",
                            dir.display(),
                            e
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Could not read directory {}: {}", dir.display(), e);
        }
    }
    files
}

fn run_test(paths: &[PathBuf], filter: Option<String>, verbose: bool) {
    use seqc::test_runner::TestRunner;

    let runner = TestRunner::new(verbose, filter);
    let summary = runner.run(paths);

    runner.print_results(&summary);

    if summary.failed > 0 {
        process::exit(1);
    } else if summary.total == 0 {
        eprintln!("No tests found");
        process::exit(2);
    }
}
