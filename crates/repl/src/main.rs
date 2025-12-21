//! seqr - Watch-style REPL for Seq
//!
//! A REPL that works by compiling expressions to a file and running.
//! Supports file watching for external editor integration.
//!
//! Usage:
//!   seqr                    # Start with temp file
//!   seqr myprogram.seq      # Start with existing file
//!
//! Commands:
//!   :quit, :q               # Exit
//!   :edit                   # Open file in $EDITOR
//!   :run                    # Manually recompile and run
//!   :clear                  # Clear the session (reset file)
//!   :show                   # Show current file contents
//!   :help                   # Show help

use clap::Parser as ClapParser;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use rustyline::error::ReadlineError;
use rustyline::history::DefaultHistory;
use rustyline::{DefaultEditor, Editor};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

#[derive(ClapParser)]
#[command(name = "seqr")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Watch-style REPL for Seq", long_about = None)]
struct Args {
    /// Seq source file to use (creates temp file if not specified)
    file: Option<PathBuf>,

    /// Don't run on startup (just open the REPL)
    #[arg(long)]
    no_run: bool,
}

/// REPL template for new sessions
const REPL_TEMPLATE: &str = r#"# Seq REPL session
# Expressions are auto-printed via stack.dump

# --- definitions ---

# --- main ---
: main ( -- )
"#;

/// Closing for the main word
const MAIN_CLOSE: &str = "  stack.dump\n;\n";

/// Marker for main section
const MAIN_MARKER: &str = "# --- main ---";

fn main() {
    let args = Args::parse();

    // Create or use existing file
    let (seq_file, _temp_file) = match &args.file {
        Some(path) => {
            if !path.exists() {
                // Create new file with template
                if let Err(e) = fs::write(path, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)) {
                    eprintln!("Error creating file: {}", e);
                    std::process::exit(1);
                }
                println!("Created new file: {}", path.display());
            }
            (path.clone(), None)
        }
        None => {
            // Create temp file
            let temp = match NamedTempFile::with_suffix(".seq") {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Error creating temp file: {}", e);
                    std::process::exit(1);
                }
            };
            let path = temp.path().to_path_buf();
            if let Err(e) = fs::write(&path, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)) {
                eprintln!("Error writing temp file: {}", e);
                std::process::exit(1);
            }
            println!("Using temp file: {}", path.display());
            (path, Some(temp))
        }
    };

    // Track when we last wrote to the file (to debounce watcher)
    let last_write = Arc::new(AtomicU64::new(0));

    // Start file watcher
    let (watch_tx, watch_rx) = mpsc::channel();
    let _watcher = start_file_watcher(&seq_file, watch_tx);

    // Initial compile if not --no-run
    if !args.no_run {
        compile_and_run(&seq_file);
    }

    // Start REPL loop
    repl_loop(&seq_file, watch_rx, last_write);
}

/// Debounce window in milliseconds - ignore watcher events within this time after our writes
const DEBOUNCE_MS: u64 = 500;

/// Get current time as milliseconds since some fixed point
fn now_ms() -> u64 {
    static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let start = START.get_or_init(Instant::now);
    start.elapsed().as_millis() as u64
}

/// Main REPL loop
fn repl_loop(seq_file: &Path, watch_rx: Receiver<()>, last_write: Arc<AtomicU64>) {
    let mut rl: Editor<(), DefaultHistory> = match DefaultEditor::new() {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Error initializing readline: {}", e);
            std::process::exit(1);
        }
    };

    // Load history if available
    let history_file = dirs_history_file();
    if let Some(ref path) = history_file {
        let _ = rl.load_history(path);
    }

    println!("\nSeq REPL (seqr). Type :help for commands, :quit to exit.\n");

    loop {
        // Check for external file changes (debounce our own writes)
        match watch_rx.try_recv() {
            Ok(()) => {
                let last = last_write.load(Ordering::Relaxed);
                let now = now_ms();
                if now.saturating_sub(last) > DEBOUNCE_MS {
                    println!("\n[File changed externally, recompiling...]");
                    compile_and_run(seq_file);
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => {}
        }

        // Read input
        let readline = rl.readline("seqr> ");
        match readline {
            Ok(line) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(line);

                match line {
                    ":quit" | ":q" => {
                        println!("Goodbye!");
                        break;
                    }
                    ":edit" => {
                        open_in_editor(seq_file);
                        // After editor closes, recompile
                        compile_and_run(seq_file);
                    }
                    ":run" => {
                        compile_and_run(seq_file);
                    }
                    ":clear" => {
                        clear_session(seq_file, &last_write);
                        println!("Session cleared.");
                    }
                    ":show" => {
                        show_file_contents(seq_file);
                    }
                    ":help" => {
                        print_help();
                    }
                    _ => {
                        // Check if this is a Seq word definition (": name ...")
                        // vs a REPL command (":command")
                        let trimmed = line.trim_start();
                        if trimmed.starts_with(": ") || trimmed.starts_with(":\t") {
                            // Seq word definition - add to definitions section
                            try_definition(seq_file, line, &last_write);
                        } else if trimmed.starts_with(':') && !trimmed.starts_with("::") {
                            // REPL command (no space after :)
                            println!(
                                "Unknown command: {}. Type :help for available commands.",
                                line
                            );
                        } else {
                            // Expression - replace current in main
                            try_expression(seq_file, line, &last_write);
                        }
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                // Don't exit, just cancel current input
            }
            Err(ReadlineError::Eof) => {
                println!("Goodbye!");
                break;
            }
            Err(err) => {
                eprintln!("Error: {:?}", err);
                break;
            }
        }
    }

    // Save history
    if let Some(ref path) = history_file {
        let _ = rl.save_history(path);
    }
}

/// Try adding a word definition to the definitions section
fn try_definition(seq_file: &Path, def: &str, last_write: &Arc<AtomicU64>) {
    let original = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return;
        }
    };

    // Add definition to definitions section
    if !add_definition(seq_file, def, last_write) {
        return;
    }

    // Try to compile
    let output_path = seq_file.with_extension("");
    match seqc::compile_file(seq_file, &output_path, false) {
        Ok(_) => {
            println!("Defined.");
            let _ = fs::remove_file(&output_path);
        }
        Err(e) => {
            eprintln!("Compile error: {}", e);
            if let Err(e) = fs::write(seq_file, &original) {
                eprintln!("Error rolling back: {}", e);
            }
            last_write.store(now_ms(), Ordering::Relaxed);
        }
    }
}

/// Add a definition to the definitions section
fn add_definition(seq_file: &Path, def: &str, last_write: &Arc<AtomicU64>) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Find the main marker
    let main_pos = match content.find(MAIN_MARKER) {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file (no main marker)");
            return false;
        }
    };

    // Insert definition before the main marker
    let mut new_content = String::new();
    new_content.push_str(&content[..main_pos]);
    new_content.push_str(def);
    new_content.push('\n');
    new_content.push('\n');
    new_content.push_str(&content[main_pos..]);

    if let Err(e) = fs::write(seq_file, new_content) {
        eprintln!("Error writing file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Relaxed);
    true
}

/// Try an expression: replace current, compile, rollback on failure
fn try_expression(seq_file: &Path, expr: &str, last_write: &Arc<AtomicU64>) {
    // Save current content for rollback
    let original = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return;
        }
    };

    // Replace the current expression (not append)
    if !set_current_expression(seq_file, expr, last_write) {
        return;
    }

    // Try to compile
    let output_path = seq_file.with_extension("");
    match seqc::compile_file(seq_file, &output_path, false) {
        Ok(_) => {
            // Success - run the program
            let status = Command::new(&output_path).status();
            match status {
                Ok(s) if !s.success() => {
                    eprintln!("(exit: {:?})", s.code());
                }
                Err(e) => eprintln!("Run error: {}", e),
                _ => {}
            }
            let _ = fs::remove_file(&output_path);
        }
        Err(e) => {
            // Failed - rollback to original
            eprintln!("Compile error: {}", e);
            if let Err(e) = fs::write(seq_file, &original) {
                eprintln!("Error rolling back: {}", e);
            }
            last_write.store(now_ms(), Ordering::Relaxed);
        }
    }
}

/// Set the current expression (replaces any previous current expression)
fn set_current_expression(seq_file: &Path, expr: &str, last_write: &Arc<AtomicU64>) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Find ": main"
    let main_pos = match content.find(": main") {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file (no main)");
            return false;
        }
    };

    // Find the newline after ": main ( -- )"
    let main_line_end = match content[main_pos..].find('\n') {
        Some(p) => main_pos + p + 1,
        None => {
            eprintln!("Error: Malformed session file");
            return false;
        }
    };

    // Find "stack.dump" which marks the end of user code
    let dump_pos = match content[main_line_end..].find("stack.dump") {
        Some(p) => main_line_end + p,
        None => {
            eprintln!("Error: Malformed session file (no stack.dump)");
            return false;
        }
    };

    // Build new content: everything before main body + new expression + stack.dump + rest
    let mut new_content = String::new();
    new_content.push_str(&content[..main_line_end]);
    new_content.push_str("  ");
    new_content.push_str(expr);
    new_content.push('\n');
    new_content.push_str(&content[dump_pos..]);

    if let Err(e) = fs::write(seq_file, new_content) {
        eprintln!("Error writing file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Relaxed);
    true
}

/// Compile and run the seq file
fn compile_and_run(seq_file: &Path) {
    let output_path = seq_file.with_extension("");

    // Compile
    match seqc::compile_file(seq_file, &output_path, false) {
        Ok(_) => {
            // Run the compiled program
            println!("---");
            let status = Command::new(&output_path).status();

            match status {
                Ok(s) if s.success() => {
                    println!("---");
                }
                Ok(s) => {
                    println!("--- (exit: {:?})", s.code());
                }
                Err(e) => {
                    eprintln!("Run error: {}", e);
                }
            }

            // Clean up executable
            let _ = fs::remove_file(&output_path);
        }
        Err(e) => {
            eprintln!("Compile error: {}", e);
        }
    }
}
/// Clear the session (reset to template)
fn clear_session(seq_file: &Path, last_write: &Arc<AtomicU64>) {
    if let Err(e) = fs::write(seq_file, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)) {
        eprintln!("Error clearing session: {}", e);
    } else {
        last_write.store(now_ms(), Ordering::Relaxed);
    }
}

/// Show current file contents
fn show_file_contents(seq_file: &Path) {
    match fs::read_to_string(seq_file) {
        Ok(content) => {
            println!("--- {} ---", seq_file.display());
            for (i, line) in content.lines().enumerate() {
                println!("{:3} | {}", i + 1, line);
            }
            println!("--- end ---");
        }
        Err(e) => {
            eprintln!("Error reading file: {}", e);
        }
    }
}

/// Open file in $EDITOR
fn open_in_editor(seq_file: &Path) {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    println!("Opening in {}...", editor);
    io::stdout().flush().ok();

    let status = Command::new(&editor).arg(seq_file).status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Editor exited with: {:?}", s.code());
        }
        Err(e) => {
            eprintln!("Failed to open editor '{}': {}", editor, e);
        }
    }
}

/// Start file watcher for external changes
fn start_file_watcher(path: &Path, tx: Sender<()>) -> Option<RecommendedWatcher> {
    let path_buf = path.to_path_buf();

    let mut watcher = match RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Only notify on modify events
                if event.kind.is_modify() {
                    let _ = tx.send(());
                }
            }
        },
        notify::Config::default().with_poll_interval(Duration::from_millis(500)),
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("Warning: Could not start file watcher: {}", e);
            return None;
        }
    };

    if let Err(e) = watcher.watch(&path_buf, RecursiveMode::NonRecursive) {
        eprintln!("Warning: Could not watch file: {}", e);
        return None;
    }

    Some(watcher)
}

/// Get history file path
fn dirs_history_file() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("seqr_history"))
}

/// Print help message
fn print_help() {
    println!(
        r#"
Seq REPL Commands:
  :quit, :q     Exit the REPL
  :edit         Open file in $EDITOR
  :run          Manually recompile and run
  :clear        Clear the session (reset to template)
  :show         Show current file contents
  :help         Show this help

Usage:
  - Type expressions to evaluate them (stack is shown automatically)
  - Each expression replaces the previous one
  - Define words with ": name ( sig ) ... ;" - these persist in the session
  - The stack is displayed after each expression and then cleared

Examples:
  seqr> 1 2 add
  [3]
  seqr> 5 dup multiply
  [25]
  seqr> : square ( Int -- Int ) dup multiply ;
  Defined.
  seqr> 7 square
  [49]
  seqr> 1 2 3
  [1, 2, 3]
"#
    );
}
