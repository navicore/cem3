//! Seq Compiler CLI
//!
//! Command-line interface for compiling .seq programs to executables
//! and running lint checks.

use clap::{Parser as ClapParser, Subcommand};
use std::path::{Path, PathBuf};
use std::process;

#[derive(ClapParser)]
#[command(name = "seqc")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Seq compiler - compile .seq programs to executables", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Input .seq source file (for default compile command)
    #[arg(global = false)]
    input: Option<PathBuf>,

    /// Output executable path
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Keep intermediate LLVM IR file (.ll)
    #[arg(long)]
    keep_ir: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile a .seq file to an executable
    Build {
        /// Input .seq source file
        input: PathBuf,

        /// Output executable path
        #[arg(short, long)]
        output: PathBuf,

        /// Keep intermediate LLVM IR file (.ll)
        #[arg(long)]
        keep_ir: bool,
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
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Build {
            input,
            output,
            keep_ir,
        }) => {
            run_build(&input, &output, keep_ir);
        }
        Some(Commands::Lint {
            paths,
            config,
            errors_only,
        }) => {
            run_lint(&paths, config.as_deref(), errors_only);
        }
        None => {
            // Backward compatibility: if no subcommand, treat as build
            if let (Some(input), Some(output)) = (cli.input, cli.output) {
                run_build(&input, &output, cli.keep_ir);
            } else {
                eprintln!("Error: Missing required arguments. Use --help for usage.");
                eprintln!("  seqc <input> -o <output>       Compile a file");
                eprintln!("  seqc build <input> -o <output> Compile a file");
                eprintln!("  seqc lint <paths>...           Run lint checks");
                process::exit(1);
            }
        }
    }
}

fn run_build(input: &Path, output: &Path, keep_ir: bool) {
    match seqc::compile_file(input, output, keep_ir) {
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

fn run_lint(paths: &[PathBuf], config_path: Option<&std::path::Path>, errors_only: bool) {
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
                    let mut default = lint::LintConfig::default_config().unwrap();
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
        if all_diagnostics
            .iter()
            .any(|d| d.severity == lint::Severity::Error)
        {
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

/// Simple recursive directory walker
fn walkdir(dir: &PathBuf) -> Vec<PathBuf> {
    use std::fs;

    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walkdir(&path));
            } else {
                files.push(path);
            }
        }
    }
    files
}
