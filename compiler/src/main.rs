//! cem3 Compiler CLI
//!
//! Command-line interface for compiling .cem programs to executables.

use clap::Parser as ClapParser;
use std::path::PathBuf;
use std::process;

#[derive(ClapParser)]
#[command(name = "cem3")]
#[command(about = "cem3 compiler - compile .cem programs to executables", long_about = None)]
struct Cli {
    /// Input .cem source file
    input: PathBuf,

    /// Output executable path
    #[arg(short, long)]
    output: PathBuf,

    /// Keep intermediate LLVM IR file (.ll)
    #[arg(long)]
    keep_ir: bool,
}

fn main() {
    let cli = Cli::parse();

    match cem3c::compile_file(&cli.input, &cli.output, cli.keep_ir) {
        Ok(_) => {
            println!(
                "Compiled {} -> {}",
                cli.input.display(),
                cli.output.display()
            );

            if cli.keep_ir {
                let ir_path = cli.output.with_extension("ll");
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
