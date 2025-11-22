//! Seq Compiler CLI
//!
//! Command-line interface for compiling .seq programs to executables.

use clap::Parser as ClapParser;
use std::path::PathBuf;
use std::process;

#[derive(ClapParser)]
#[command(name = "seqc")]
#[command(about = "Seq compiler - compile .seq programs to executables", long_about = None)]
struct Cli {
    /// Input .seq source file
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

    match seqc::compile_file(&cli.input, &cli.output, cli.keep_ir) {
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
