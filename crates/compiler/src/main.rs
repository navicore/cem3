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

        /// Pure inline test mode: bypass scheduler, return top of stack as exit code.
        /// Only supports inline operations (integers, arithmetic, stack ops).
        #[arg(long)]
        pure_inline: bool,
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

    /// Create a virtual environment with isolated seq binaries
    Venv {
        /// Name/path for the virtual environment directory
        name: PathBuf,
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
            pure_inline,
        } => {
            let output = output.unwrap_or_else(|| {
                // Default: input filename without .seq extension
                let stem = input.file_stem().unwrap_or_default();
                PathBuf::from(stem)
            });
            run_build(&input, &output, keep_ir, &ffi_manifests, pure_inline);
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
        Commands::Venv { name } => {
            run_venv(&name);
        }
    }
}

fn run_completions(shell: Shell) {
    let mut cmd = Cli::command();
    generate(shell, &mut cmd, "seqc", &mut io::stdout());
}

fn run_build(
    input: &Path,
    output: &Path,
    keep_ir: bool,
    ffi_manifests: &[PathBuf],
    pure_inline: bool,
) {
    // Build config with external FFI manifests
    let mut config = if ffi_manifests.is_empty() {
        seqc::CompilerConfig::default()
    } else {
        seqc::CompilerConfig::new().with_ffi_manifests(ffi_manifests.iter().cloned())
    };

    // Enable pure inline test mode if requested
    config.pure_inline_test = pure_inline;

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
    use seqc::{Parser, ProgramResourceAnalyzer};
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

    // Phase 1: Pattern-based linting
    let file_diagnostics = linter.lint_program(&program, path);
    diagnostics.extend(file_diagnostics);

    // Phase 2: Resource leak detection with cross-word analysis
    let mut resource_analyzer = ProgramResourceAnalyzer::new(path);
    let resource_diagnostics = resource_analyzer.analyze_program(&program);
    diagnostics.extend(resource_diagnostics);
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

fn run_venv(name: &Path) {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    // Get absolute path for the venv, normalizing to remove trailing slashes
    let venv_path: PathBuf = if name.is_absolute() {
        name.components().collect()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(name)
            .components()
            .collect()
    };

    // Check if directory already exists
    if venv_path.exists() {
        eprintln!("Error: {} already exists", venv_path.display());
        process::exit(1);
    }

    // Create directory structure
    let bin_dir = venv_path.join("bin");
    if let Err(e) = fs::create_dir_all(&bin_dir) {
        eprintln!("Error creating directory {}: {}", bin_dir.display(), e);
        process::exit(1);
    }

    // Find current executable's directory
    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error finding current executable: {}", e);
            process::exit(1);
        }
    };
    let exe_dir = match current_exe.parent() {
        Some(dir) => dir,
        None => {
            eprintln!("Error: could not determine executable directory");
            process::exit(1);
        }
    };

    // Copy binaries
    let binaries = ["seqc", "seqr", "seq-lsp"];
    let mut copied_count = 0;
    for binary in binaries {
        let src = exe_dir.join(binary);
        let dst = bin_dir.join(binary);

        if !src.exists() {
            eprintln!("Warning: {} not found, skipping", src.display());
            continue;
        }

        if let Err(e) = fs::copy(&src, &dst) {
            eprintln!("Error copying {}: {}", binary, e);
            process::exit(1);
        }

        // Set executable permissions on Unix
        #[cfg(unix)]
        if let Err(e) = fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)) {
            eprintln!("Warning: could not set permissions on {}: {}", binary, e);
        }

        println!("  Copied {}", binary);
        copied_count += 1;
    }

    if copied_count == 0 {
        eprintln!("Error: no seq binaries found in {}", exe_dir.display());
        process::exit(1);
    }

    // Generate activate scripts
    // Use components().last() instead of file_name() to handle trailing slashes
    let venv_name = venv_path
        .components()
        .next_back()
        .and_then(|c| c.as_os_str().to_str())
        .unwrap_or("seq-venv");

    if let Err(e) = generate_activate_bash(&venv_path, venv_name) {
        eprintln!("Error generating activate script: {}", e);
        process::exit(1);
    }

    if let Err(e) = generate_activate_fish(&venv_path, venv_name) {
        eprintln!("Error generating activate.fish script: {}", e);
        process::exit(1);
    }

    if let Err(e) = generate_activate_csh(&venv_path, venv_name) {
        eprintln!("Error generating activate.csh script: {}", e);
        process::exit(1);
    }

    println!("\nCreated virtual environment at {}", venv_path.display());
    println!("\nTo activate, run:");
    println!("  source {}/bin/activate", venv_path.display());
}

fn generate_activate_bash(venv_path: &Path, venv_name: &str) -> std::io::Result<()> {
    use std::fs;

    let script = format!(
        r#"# This file must be sourced with "source activate" from bash/zsh.
# It cannot be run directly.

deactivate () {{
    # Reset PATH
    if [ -n "${{_OLD_VIRTUAL_PATH:-}}" ]; then
        PATH="${{_OLD_VIRTUAL_PATH}}"
        export PATH
        unset _OLD_VIRTUAL_PATH
    fi

    # Reset prompt
    if [ -n "${{_OLD_VIRTUAL_PS1:-}}" ]; then
        PS1="${{_OLD_VIRTUAL_PS1}}"
        export PS1
        unset _OLD_VIRTUAL_PS1
    fi

    unset SEQ_VIRTUAL_ENV

    if [ ! "${{1:-}}" = "nondestructive" ]; then
        unset -f deactivate
    fi
}}

# Unset irrelevant variables
deactivate nondestructive

SEQ_VIRTUAL_ENV="{venv_path}"
export SEQ_VIRTUAL_ENV

_OLD_VIRTUAL_PATH="$PATH"
PATH="$SEQ_VIRTUAL_ENV/bin:$PATH"
export PATH

_OLD_VIRTUAL_PS1="${{PS1:-}}"
PS1="({venv_name}) ${{PS1:-}}"
export PS1
"#,
        venv_path = venv_path.display(),
        venv_name = venv_name
    );

    fs::write(venv_path.join("bin").join("activate"), script)?;
    println!("  Generated bin/activate");
    Ok(())
}

fn generate_activate_fish(venv_path: &Path, venv_name: &str) -> std::io::Result<()> {
    use std::fs;

    let script = format!(
        r#"# This file must be sourced with "source activate.fish" from fish.

function deactivate -d "Exit virtual environment"
    # Reset PATH
    if set -q _OLD_VIRTUAL_PATH
        set -gx PATH $_OLD_VIRTUAL_PATH
        set -e _OLD_VIRTUAL_PATH
    end

    # Reset prompt
    if functions -q _old_fish_prompt
        functions -e fish_prompt
        functions -c _old_fish_prompt fish_prompt
        functions -e _old_fish_prompt
    end

    set -e SEQ_VIRTUAL_ENV

    if test "$argv[1]" != "nondestructive"
        functions -e deactivate
    end
end

# Unset irrelevant variables
deactivate nondestructive

set -gx SEQ_VIRTUAL_ENV "{venv_path}"

set -gx _OLD_VIRTUAL_PATH $PATH
set -gx PATH "$SEQ_VIRTUAL_ENV/bin" $PATH

# Save current prompt
if functions -q fish_prompt
    functions -c fish_prompt _old_fish_prompt
end

function fish_prompt
    printf "({venv_name}) "
    _old_fish_prompt
end
"#,
        venv_path = venv_path.display(),
        venv_name = venv_name
    );

    fs::write(venv_path.join("bin").join("activate.fish"), script)?;
    println!("  Generated bin/activate.fish");
    Ok(())
}

fn generate_activate_csh(venv_path: &Path, venv_name: &str) -> std::io::Result<()> {
    use std::fs;

    let script = format!(
        r#"# This file must be sourced with "source activate.csh" from csh/tcsh.

alias deactivate 'if ($?_OLD_VIRTUAL_PATH) then; setenv PATH "$_OLD_VIRTUAL_PATH"; unsetenv _OLD_VIRTUAL_PATH; endif; unsetenv SEQ_VIRTUAL_ENV; unalias deactivate'

setenv SEQ_VIRTUAL_ENV "{venv_path}"

setenv _OLD_VIRTUAL_PATH "$PATH"
setenv PATH "$SEQ_VIRTUAL_ENV/bin:$PATH"

set prompt = "({venv_name}) $prompt"
"#,
        venv_path = venv_path.display(),
        venv_name = venv_name
    );

    fs::write(venv_path.join("bin").join("activate.csh"), script)?;
    println!("  Generated bin/activate.csh");
    Ok(())
}
