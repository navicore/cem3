//! seqr - Watch-style REPL for Seq
//!
//! A REPL that works by compiling expressions to a file and running.
//! Stack persists across lines - build up values incrementally.
//! Supports file watching for external editor integration.
//!
//! Usage:
//!   seqr                    # Start with temp file
//!   seqr myprogram.seq      # Start with existing file
//!
//! Commands:
//!   :quit, :q               # Exit
//!   :pop                    # Remove last expression (undo)
//!   :clear                  # Clear the session (reset stack)
//!   :show                   # Show current file contents
//!   :edit, :e               # Open file in $EDITOR
//!   :include <mod>          # Include a module (e.g., std:math)
//!   :run                    # Manually recompile and run
//!   :repair                 # Validate/repair session file
//!   :help                   # Show help
//!
//! Vi Mode:
//!   Set SEQR_VI_MODE=1 or have $EDITOR contain vi/vim/nvim

mod lsp_client;

use clap::Parser as ClapParser;
use lsp_client::LspClient;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use rustyline::completion::{Completer, Pair};
use rustyline::config::Config;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{Context, EditMode, Editor, Helper};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

// Memory ordering notes:
// - Use Ordering::Release on stores to ensure writes are visible to other threads
// - Use Ordering::Acquire on loads to see writes from other threads
// This is used for cross-thread communication between file watcher and main REPL loop
use std::cell::RefCell;
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

/// Helper for rustyline that provides LSP-based completions
struct SeqHelper {
    /// LSP client for completions (optional - degrades gracefully if unavailable)
    lsp: RefCell<Option<LspClient>>,
    /// Path to the seq file being edited
    seq_file: PathBuf,
    /// Cached file content to avoid repeated I/O during completions
    cached_content: RefCell<String>,
}

impl SeqHelper {
    fn new(seq_file: PathBuf) -> Self {
        // Read initial content
        let content = fs::read_to_string(&seq_file).unwrap_or_default();

        // Try to start LSP client, but don't fail if unavailable
        let lsp = match LspClient::new(&seq_file) {
            Ok(mut client) => {
                // Open the document
                if client.did_open(&content).is_ok() {
                    Some(client)
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        Self {
            lsp: RefCell::new(lsp),
            seq_file,
            cached_content: RefCell::new(content),
        }
    }

    /// Sync the document content with the LSP after changes.
    /// Also updates the cached content for completions.
    fn sync_document(&self) {
        if let Ok(content) = fs::read_to_string(&self.seq_file) {
            // Update cache
            if let Ok(mut cache) = self.cached_content.try_borrow_mut() {
                *cache = content.clone();
            }
            // Sync with LSP
            if let Ok(mut lsp_guard) = self.lsp.try_borrow_mut()
                && let Some(ref mut lsp) = *lsp_guard
            {
                let _ = lsp.did_change(&content);
            }
        }
    }

    /// Get the cached file content, or read from disk if cache unavailable
    fn get_content(&self) -> Option<String> {
        self.cached_content
            .try_borrow()
            .ok()
            .map(|c| c.clone())
            .or_else(|| fs::read_to_string(&self.seq_file).ok())
    }
}

impl Completer for SeqHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        // Find word start for replacement - we do this first to get the prefix
        let word_start = line[..pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        // Get the prefix the user has typed (for filtering)
        let prefix = &line[word_start..pos];

        // Don't show completions for empty prefix - too noisy
        if prefix.is_empty() {
            return Ok((pos, vec![]));
        }

        // If no LSP client or borrow fails, return empty completions
        let Ok(mut lsp_guard) = self.lsp.try_borrow_mut() else {
            return Ok((pos, vec![]));
        };
        let lsp = match lsp_guard.as_mut() {
            Some(lsp) => lsp,
            None => return Ok((pos, vec![])),
        };

        // Get cached file content (avoids disk I/O on every completion)
        let Some(file_content) = self.get_content() else {
            return Ok((pos, vec![]));
        };

        // Create virtual document: file content + current line at end of main
        // Find where to insert (before stack.dump)
        let insert_pos = match file_content.find("  stack.dump") {
            Some(p) => p,
            None => return Ok((pos, vec![])),
        };

        let virtual_content = format!(
            "{}  {}\n{}",
            &file_content[..insert_pos],
            line,
            &file_content[insert_pos..]
        );

        // Calculate line/column in virtual document
        // Line count up to insert point + 1 for the user's line
        let lines_before: u32 = file_content[..insert_pos].matches('\n').count() as u32;
        let line_num = lines_before; // 0-indexed
        let col_num = pos as u32 + 2; // +2 for the "  " indent

        // Sync virtual document and get completions
        if lsp.did_change(&virtual_content).is_err() {
            return Ok((pos, vec![]));
        }

        let completions = match lsp.completions(line_num, col_num) {
            Ok(items) => items,
            Err(_) => return Ok((pos, vec![])),
        };

        // Filter and map completions - only show those matching the prefix
        let prefix_lower = prefix.to_lowercase();
        let pairs: Vec<Pair> = completions
            .into_iter()
            .filter(|item| {
                // Match against label, case-insensitive prefix match
                item.label.to_lowercase().starts_with(&prefix_lower)
            })
            .map(|item| {
                let display = item.label.clone();
                let replacement = item.insert_text.unwrap_or(item.label);
                Pair {
                    display,
                    replacement,
                }
            })
            .collect();

        // Restore original document
        let _ = lsp.did_change(&file_content);

        Ok((word_start, pairs))
    }
}

impl Hinter for SeqHelper {
    type Hint = String;
}

impl Highlighter for SeqHelper {}
impl Validator for SeqHelper {}
impl Helper for SeqHelper {}

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

# --- includes ---

# --- definitions ---

# --- main ---
: main ( -- )
"#;

/// Marker for includes section
const INCLUDES_MARKER: &str = "# --- includes ---";

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

    // Validate and repair session file if needed
    if !repair_session_file(&seq_file, &last_write) {
        eprintln!("Could not initialize session file");
        std::process::exit(1);
    }

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
/// Detect if vi mode should be enabled.
/// Checks SEQR_VI_MODE=1 first, then falls back to checking if $EDITOR contains vi/vim/nvim.
fn should_use_vi_mode() -> bool {
    // Explicit override
    if std::env::var("SEQR_VI_MODE")
        .map(|v| v == "1")
        .unwrap_or(false)
    {
        return true;
    }

    // Check $EDITOR for vi/vim/nvim
    if let Ok(editor) = std::env::var("EDITOR") {
        let editor_lower = editor.to_lowercase();
        return editor_lower.contains("vi") || editor_lower.contains("nvim");
    }

    false
}

fn repl_loop(seq_file: &Path, watch_rx: Receiver<()>, last_write: Arc<AtomicU64>) {
    let vi_mode = should_use_vi_mode();
    let config = Config::builder()
        .edit_mode(if vi_mode {
            EditMode::Vi
        } else {
            EditMode::Emacs
        })
        .build();

    let mut rl: Editor<SeqHelper, DefaultHistory> = match Editor::with_config(config) {
        Ok(editor) => editor,
        Err(e) => {
            eprintln!("Error initializing readline: {}", e);
            std::process::exit(1);
        }
    };

    // Set up LSP-based completion helper
    let helper = SeqHelper::new(seq_file.to_path_buf());
    let has_lsp = helper.lsp.borrow().is_some();
    rl.set_helper(Some(helper));

    // Load history if available
    let history_file = dirs_history_file();
    if let Some(ref path) = history_file {
        let _ = rl.load_history(path);
    }

    let mode_str = if vi_mode { "vi" } else { "emacs" };
    let lsp_str = if has_lsp { ", Tab for completions" } else { "" };
    println!(
        "\nSeq REPL (seqr) [{} mode{}]. Type :help for commands, :quit to exit.\n",
        mode_str, lsp_str
    );

    loop {
        // Check for external file changes (debounce our own writes)
        match watch_rx.try_recv() {
            Ok(()) => {
                let last = last_write.load(Ordering::Acquire);
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
                    ":edit" | ":e" => {
                        open_in_editor(seq_file);
                        // Drain any file watcher events from during the edit session
                        while watch_rx.try_recv().is_ok() {}
                        // Update last_write to prevent "external change" message
                        last_write.store(now_ms(), Ordering::Release);
                        // Sync LSP with new file contents
                        if let Some(helper) = rl.helper() {
                            helper.sync_document();
                        }
                        // After editor closes, recompile
                        compile_and_run(seq_file);
                    }
                    ":run" => {
                        compile_and_run(seq_file);
                    }
                    ":clear" => {
                        clear_session(seq_file, &last_write);
                        if let Some(helper) = rl.helper() {
                            helper.sync_document();
                        }
                        println!("Session cleared.");
                    }
                    ":pop" => {
                        if pop_last_expression(seq_file, &last_write) {
                            if let Some(helper) = rl.helper() {
                                helper.sync_document();
                            }
                            // Show the new stack state
                            compile_and_run(seq_file);
                        }
                    }
                    ":show" => {
                        show_file_contents(seq_file);
                    }
                    ":repair" => {
                        if repair_session_file(seq_file, &last_write) {
                            println!("Session file is valid.");
                        }
                    }
                    ":help" => {
                        print_help();
                    }
                    _ if line.starts_with(":include ") => {
                        let module = line.strip_prefix(":include ").unwrap().trim();
                        if module.is_empty() {
                            println!("Usage: :include <module>");
                            println!("Example: :include std:math");
                        } else if try_include(seq_file, module, &last_write) {
                            if let Some(helper) = rl.helper() {
                                helper.sync_document();
                            }
                            println!("Included '{}'.", module);
                        }
                    }
                    _ => {
                        // Check if this is a Seq word definition (": name ...")
                        // vs a REPL command (":command")
                        let trimmed = line.trim_start();
                        if trimmed.starts_with(": ") || trimmed.starts_with(":\t") {
                            // Seq word definition - add to definitions section
                            try_definition(seq_file, line, &last_write);
                            if let Some(helper) = rl.helper() {
                                helper.sync_document();
                            }
                        } else if trimmed.starts_with(':') && !trimmed.starts_with("::") {
                            // REPL command (no space after :)
                            println!(
                                "Unknown command: {}. Type :help for available commands.",
                                line
                            );
                        } else {
                            // Expression - replace current in main
                            try_expression(seq_file, line, &last_write);
                            if let Some(helper) = rl.helper() {
                                helper.sync_document();
                            }
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

/// Try adding an include to the includes section
fn try_include(seq_file: &Path, module: &str, last_write: &Arc<AtomicU64>) -> bool {
    let original = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Check if already included
    let include_stmt = format!("include {}", module);
    if original.contains(&include_stmt) {
        println!("'{}' is already included.", module);
        return false;
    }

    // Add include to includes section
    if !add_include(seq_file, module, last_write) {
        return false;
    }

    // Try to compile
    let output_path = seq_file.with_extension("");
    match seqc::compile_file(seq_file, &output_path, false) {
        Ok(_) => {
            remove_file_logged(&output_path);
            last_write.store(now_ms(), Ordering::Release);
            true
        }
        Err(e) => {
            eprintln!("Include error: {}", e);
            if let Err(e) = fs::write(seq_file, &original) {
                eprintln!("Error rolling back: {}", e);
            }
            last_write.store(now_ms(), Ordering::Release);
            false
        }
    }
}

/// Add an include statement to the includes section
fn add_include(seq_file: &Path, module: &str, last_write: &Arc<AtomicU64>) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Find the includes marker
    let includes_pos = match content.find(INCLUDES_MARKER) {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file (no includes marker)");
            return false;
        }
    };

    // Find the end of the includes marker line
    let marker_end = includes_pos + INCLUDES_MARKER.len();
    let after_marker = &content[marker_end..];
    let newline_pos = after_marker.find('\n').unwrap_or(0);
    let insert_pos = marker_end + newline_pos + 1;

    // Insert include after the marker
    let mut new_content = String::new();
    new_content.push_str(&content[..insert_pos]);
    new_content.push_str("include ");
    new_content.push_str(module);
    new_content.push('\n');
    new_content.push_str(&content[insert_pos..]);

    if let Err(e) = fs::write(seq_file, new_content) {
        eprintln!("Error writing file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Release);
    true
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
            remove_file_logged(&output_path);
            // Update last_write after compile to suppress file watcher
            last_write.store(now_ms(), Ordering::Release);
        }
        Err(e) => {
            eprintln!("Compile error: {}", e);
            if let Err(e) = fs::write(seq_file, &original) {
                eprintln!("Error rolling back: {}", e);
            }
            last_write.store(now_ms(), Ordering::Release);
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
    last_write.store(now_ms(), Ordering::Release);
    true
}

/// Try an expression: append to main, compile, rollback on failure
fn try_expression(seq_file: &Path, expr: &str, last_write: &Arc<AtomicU64>) {
    // Save current content for rollback
    let original = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return;
        }
    };

    // Append the expression (stack persists across lines)
    if !append_expression(seq_file, expr, last_write) {
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
            remove_file_logged(&output_path);
            // Update last_write after compile+run to suppress file watcher
            last_write.store(now_ms(), Ordering::Release);
        }
        Err(e) => {
            // Failed - rollback to original
            eprintln!("Compile error: {}", e);
            if let Err(e) = fs::write(seq_file, &original) {
                eprintln!("Error rolling back: {}", e);
            }
            last_write.store(now_ms(), Ordering::Release);
        }
    }
}

/// Append an expression to main (stack persists across lines)
fn append_expression(seq_file: &Path, expr: &str, last_write: &Arc<AtomicU64>) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Find "stack.dump" which marks the end of user code
    let dump_pos = match content.find("  stack.dump") {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file (no stack.dump)");
            return false;
        }
    };

    // Insert new expression before stack.dump
    let mut new_content = String::new();
    new_content.push_str(&content[..dump_pos]);
    new_content.push_str("  ");
    new_content.push_str(expr);
    new_content.push('\n');
    new_content.push_str(&content[dump_pos..]);

    if let Err(e) = fs::write(seq_file, new_content) {
        eprintln!("Error writing file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Release);
    true
}

/// Remove the last expression from main (undo last line)
fn pop_last_expression(seq_file: &Path, last_write: &Arc<AtomicU64>) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {}", e);
            return false;
        }
    };

    // Find ": main ( -- )" line end
    let main_pos = match content.find(": main") {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file");
            return false;
        }
    };
    let main_line_end = match content[main_pos..].find('\n') {
        Some(p) => main_pos + p + 1,
        None => {
            eprintln!("Error: Malformed session file");
            return false;
        }
    };

    // Find "  stack.dump"
    let dump_pos = match content.find("  stack.dump") {
        Some(p) => p,
        None => {
            eprintln!("Error: Malformed session file");
            return false;
        }
    };

    // Get the expressions section
    let expr_section = &content[main_line_end..dump_pos];

    // Find the last expression line
    let lines: Vec<&str> = expr_section.lines().collect();
    if lines.is_empty() || lines.iter().all(|l| l.trim().is_empty()) {
        println!("Nothing to pop.");
        return false;
    }

    // Find last non-empty line index
    let mut last_expr_idx = None;
    for (i, line) in lines.iter().enumerate().rev() {
        if !line.trim().is_empty() {
            last_expr_idx = Some(i);
            break;
        }
    }

    let last_expr_idx = match last_expr_idx {
        Some(i) => i,
        None => {
            println!("Nothing to pop.");
            return false;
        }
    };

    // Rebuild without the last expression
    let mut new_content = String::new();
    new_content.push_str(&content[..main_line_end]);
    for (i, line) in lines.iter().enumerate() {
        if i != last_expr_idx {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }
    new_content.push_str(&content[dump_pos..]);

    if let Err(e) = fs::write(seq_file, new_content) {
        eprintln!("Error writing file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Release);
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
            remove_file_logged(&output_path);
        }
        Err(e) => {
            eprintln!("Compile error: {}", e);
        }
    }
}
/// Validate that the session file has the required structure
/// Returns true if valid, false if malformed
fn validate_session_file(seq_file: &Path) -> bool {
    let content = match fs::read_to_string(seq_file) {
        Ok(c) => c,
        Err(_) => return false,
    };
    content.contains(MAIN_MARKER) && content.contains("  stack.dump")
}

/// Attempt to repair a malformed session file
/// Returns true if repair was successful or not needed
fn repair_session_file(seq_file: &Path, last_write: &Arc<AtomicU64>) -> bool {
    if validate_session_file(seq_file) {
        return true; // Already valid
    }

    eprintln!("Session file appears malformed. Resetting to template...");
    if let Err(e) = fs::write(seq_file, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)) {
        eprintln!("Error repairing session file: {}", e);
        return false;
    }
    last_write.store(now_ms(), Ordering::Release);
    eprintln!("Session file repaired.");
    true
}

/// Clear the session (reset to template)
fn clear_session(seq_file: &Path, last_write: &Arc<AtomicU64>) {
    if let Err(e) = fs::write(seq_file, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)) {
        eprintln!("Error clearing session: {}", e);
    } else {
        last_write.store(now_ms(), Ordering::Release);
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
///
/// Supports editors with flags, e.g., EDITOR="code --wait" or EDITOR="emacs -nw"
fn open_in_editor(seq_file: &Path) {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

    // Split EDITOR into command and arguments
    // This handles cases like "code --wait" or "emacs -nw"
    let parts: Vec<&str> = editor.split_whitespace().collect();
    if parts.is_empty() {
        eprintln!("EDITOR is empty, using vi");
        let _ = Command::new("vi").arg(seq_file).status();
        return;
    }

    let cmd = parts[0];
    let editor_args = &parts[1..];

    println!("Opening in {}...", editor);
    io::stdout().flush().ok();

    let status = Command::new(cmd).args(editor_args).arg(seq_file).status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            eprintln!("Editor exited with: {:?}", s.code());
        }
        Err(e) => {
            eprintln!("Failed to open editor '{}': {}", editor, e);
            eprintln!("Hint: Make sure the editor command is in your PATH");
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

/// Remove a file, logging any errors (useful for cleanup)
fn remove_file_logged(path: &Path) {
    if let Err(e) = fs::remove_file(path) {
        // Only warn if file exists but couldn't be removed
        // (ignore "not found" errors during cleanup)
        if e.kind() != std::io::ErrorKind::NotFound {
            eprintln!("Warning: could not remove {}: {}", path.display(), e);
        }
    }
}

/// Print help message
fn print_help() {
    println!(
        r#"
Seq REPL Commands:
  :quit, :q        Exit the REPL
  :pop             Remove last expression (undo)
  :clear           Clear the session (reset stack and expressions)
  :show            Show current file contents
  :edit, :e        Open file in $EDITOR (yank code from here)
  :include <mod>   Include a module (e.g., :include std:math)
  :run             Manually recompile and run
  :repair          Validate and repair malformed session file
  :help            Show this help

Usage:
  - Type expressions to evaluate them (stack is shown automatically)
  - Stack persists across lines - build up values incrementally
  - Define words with ": name ( sig ) ... ;" - these persist in the session
  - Use :include to add stdlib modules for math, json, etc.
  - Use :pop to undo the last expression
  - Use :clear to start fresh

Vi Mode:
  - Auto-enabled when $EDITOR contains vi/vim/nvim
  - Or set SEQR_VI_MODE=1 to force vi mode

Examples:
  seqr> 1 2
  [1, 2]
  seqr> add
  [3]
  seqr> :include std:math
  Included 'std:math'.
  seqr> 3.14159 sin
  [3, 0.0000026...]
  seqr> : square ( Int -- Int ) dup multiply ;
  Defined.
  seqr> drop drop 3 square
  [9]
"#
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU64;
    use tempfile::NamedTempFile;

    fn create_test_file() -> (PathBuf, NamedTempFile) {
        let temp = NamedTempFile::with_suffix(".seq").unwrap();
        let path = temp.path().to_path_buf();
        fs::write(&path, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE)).unwrap();
        (path, temp)
    }

    #[test]
    fn test_append_expression() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Append first expression
        assert!(append_expression(&path, "1 2", &last_write));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("  1 2\n"));
        assert!(content.contains("  stack.dump"));

        // Append second expression
        assert!(append_expression(&path, "add", &last_write));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("  1 2\n"));
        assert!(content.contains("  add\n"));
    }

    #[test]
    fn test_pop_last_expression() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Add two expressions
        append_expression(&path, "1 2", &last_write);
        append_expression(&path, "add", &last_write);

        // Pop the last one
        assert!(pop_last_expression(&path, &last_write));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("  1 2\n"));
        assert!(!content.contains("  add\n"));

        // Pop again
        assert!(pop_last_expression(&path, &last_write));
        let content = fs::read_to_string(&path).unwrap();
        assert!(!content.contains("  1 2\n"));

        // Pop on empty should return false
        assert!(!pop_last_expression(&path, &last_write));
    }

    #[test]
    fn test_add_definition() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Add a definition
        assert!(add_definition(
            &path,
            ": square ( Int -- Int ) dup multiply ;",
            &last_write
        ));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains(": square ( Int -- Int ) dup multiply ;"));

        // Definition should be before main marker
        let def_pos = content.find(": square").unwrap();
        let main_pos = content.find(MAIN_MARKER).unwrap();
        assert!(def_pos < main_pos);
    }

    #[test]
    fn test_clear_session() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Add some content
        append_expression(&path, "1 2 3", &last_write);
        add_definition(&path, ": test ( -- ) ;", &last_write);

        // Clear
        clear_session(&path, &last_write);
        let content = fs::read_to_string(&path).unwrap();

        // Should be back to template
        assert!(!content.contains("1 2 3"));
        assert!(!content.contains(": test"));
        assert!(content.contains(MAIN_MARKER));
        assert!(content.contains("stack.dump"));
    }

    #[test]
    fn test_malformed_file_handling() {
        let temp = NamedTempFile::with_suffix(".seq").unwrap();
        let path = temp.path().to_path_buf();
        let last_write = Arc::new(AtomicU64::new(0));

        // Write malformed content (no stack.dump marker)
        fs::write(&path, ": main ( -- )\n;\n").unwrap();

        // Should return false for append
        assert!(!append_expression(&path, "1 2", &last_write));
    }

    #[test]
    fn test_command_vs_definition_detection() {
        // ": foo" with space is a definition
        assert!(": square ( -- ) ;".trim_start().starts_with(": "));

        // ":foo" without space is a command
        assert!(!":quit".trim_start().starts_with(": "));
        assert!(!":help".trim_start().starts_with(": "));

        // ":" alone followed by tab is also a definition
        assert!(":\tfoo".trim_start().starts_with(":\t"));
    }

    #[test]
    fn test_add_include() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Add an include
        assert!(add_include(&path, "std:math", &last_write));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("include std:math"));

        // Include should be after includes marker but before definitions marker
        let inc_pos = content.find("include std:math").unwrap();
        let includes_marker_pos = content.find(INCLUDES_MARKER).unwrap();
        let def_marker_pos = content.find("# --- definitions ---").unwrap();
        assert!(inc_pos > includes_marker_pos);
        assert!(inc_pos < def_marker_pos);
    }

    #[test]
    fn test_duplicate_include_detection() {
        let (path, _temp) = create_test_file();
        let last_write = Arc::new(AtomicU64::new(0));

        // Add include first time - should succeed
        assert!(add_include(&path, "std:math", &last_write));

        // Check the include is present
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("include std:math"));

        // Count occurrences - should be exactly 1
        let count = content.matches("include std:math").count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_template_has_all_markers() {
        // Verify template has all required markers
        assert!(REPL_TEMPLATE.contains(INCLUDES_MARKER));
        assert!(REPL_TEMPLATE.contains(MAIN_MARKER));
        assert!(REPL_TEMPLATE.contains("# --- definitions ---"));
    }

    #[test]
    fn test_vi_mode_detection() {
        // Test the vi mode detection logic directly
        // Note: This tests the pattern matching, not env var reading
        let vim_editors = ["vim", "nvim", "vi", "/usr/bin/vim", "nvim-qt"];
        let non_vim_editors = ["nano", "emacs", "code", "subl", ""];

        for editor in vim_editors {
            let editor_lower = editor.to_lowercase();
            assert!(
                editor_lower.contains("vi") || editor_lower.contains("nvim"),
                "Expected '{}' to be detected as vi-like",
                editor
            );
        }

        for editor in non_vim_editors {
            let editor_lower = editor.to_lowercase();
            assert!(
                !(editor_lower.contains("vi") || editor_lower.contains("nvim")),
                "Expected '{}' to NOT be detected as vi-like",
                editor
            );
        }
    }
}
