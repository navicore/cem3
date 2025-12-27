//! TUI REPL for the Seq programming language
//!
//! Provides a split-pane terminal interface with:
//! - REPL input with syntax highlighting and Vi mode editing
//! - Real-time IR visualization (stack effects, typed AST, LLVM snippets)
//! - ASCII art stack effect diagrams

pub mod engine;
pub mod ir;
pub mod ui;
// pub mod input;  // Phase 3
// pub mod app;    // Phase 3

/// Run the TUI REPL with an optional file
pub fn run(_file: Option<&std::path::Path>) -> Result<(), String> {
    // TODO: Phase 4 - full application integration
    Err("TUI mode not yet implemented - coming in Phase 4".to_string())
}
