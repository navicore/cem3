//! Seq Compiler CLI
//!
//! Command-line interface for compiling .seq programs to executables
//! and running lint checks.

use clap::{CommandFactory, Parser as ClapParser, Subcommand};
use clap_complete::{Shell, generate};
use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process;

#[path = "main/build.rs"]
mod build;
#[path = "main/lint.rs"]
mod lint;
#[path = "main/test.rs"]
mod test;
#[path = "main/venv.rs"]
mod venv;

use build::run_build;
use lint::run_lint;
use test::run_test;
use venv::run_venv;

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

        /// Bake per-word atomic call counters into the binary.
        /// Use with SEQ_REPORT=words to see call counts at exit.
        #[arg(long)]
        instrument: bool,

        /// Lower self-tail-recursive loops to native LLVM loops (experimental).
        /// See docs/design/LOOP_LOWERING.md. When set, `loop_yield_cadence`
        /// (if present) overrides the default yield cadence.
        #[arg(long = "loop-opt")]
        loop_opt: bool,

        /// Iterations between cooperative yields inside a loop-opt loop.
        /// Must be a power of two. [default: 1024]
        #[arg(long = "loop-yield-cadence", value_name = "N")]
        loop_yield_cadence: Option<u32>,
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
    let args: Vec<OsString> = std::env::args_os().collect();

    // Check for script mode: seqc <file.seq> [args...]
    // This runs before clap parsing to handle shebang invocation
    if args.len() >= 2 {
        let first_arg = args[1].to_string_lossy();
        if first_arg.ends_with(".seq") && !first_arg.starts_with('-') {
            let source_path = PathBuf::from(&args[1]);
            let script_args = &args[2..];

            match seqc::script::run_script(&source_path, script_args) {
                Ok(never) => match never {},
                Err(e) => {
                    eprintln!("Error: {}", e);
                    process::exit(1);
                }
            }
        }
    }

    // Normal subcommand processing
    let cli = Cli::parse();

    match cli.command {
        Commands::Build {
            input,
            output,
            keep_ir,
            ffi_manifests,
            pure_inline,
            instrument,
            loop_opt,
            loop_yield_cadence,
        } => {
            let output = output.unwrap_or_else(|| {
                // Default: input filename without .seq extension
                let stem = input.file_stem().unwrap_or_default();
                PathBuf::from(stem)
            });
            run_build(
                &input,
                &output,
                keep_ir,
                &ffi_manifests,
                pure_inline,
                instrument,
                loop_opt.then(|| loop_yield_cadence.unwrap_or(1024)),
            );
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
