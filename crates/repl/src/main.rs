//! seqr - TUI REPL for Seq
//!
//! A split-pane terminal REPL with real-time IR visualization.
//! Stack persists across lines - build up values incrementally.
//!
//! Usage:
//!   seqr                    # Start with temp file
//!   seqr myprogram.seq      # Start with existing file
//!
//! Features:
//!   - Split-pane interface (REPL left, IR right)
//!   - Vi-style editing with syntax highlighting
//!   - Real-time IR visualization (stack effects, typed AST, LLVM IR)
//!   - Tab for LSP completions, h/l to cycle IR views
//!
//! Commands:
//!   :quit, :q               # Exit
//!   :pop                    # Remove last expression (undo)
//!   :clear                  # Clear the session (reset stack)
//!   :show                   # Show current file contents
//!   :edit, :e               # Open file in $EDITOR
//!   :include <mod>          # Include a module (e.g., std:math)
//!   :help                   # Show help

use clap::Parser as ClapParser;
use std::path::PathBuf;

#[derive(ClapParser)]
#[command(name = "seqr")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "TUI REPL for Seq with IR visualization", long_about = None)]
struct Args {
    /// Seq source file to use (creates temp file if not specified)
    file: Option<PathBuf>,
}

fn main() {
    let args = Args::parse();

    if let Err(e) = seq_tui::run(args.file.as_deref()) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
