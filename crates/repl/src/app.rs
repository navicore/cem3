//! TUI Application
//!
//! Main application state and event loop using crossterm.
//! Integrates all widgets and handles Vi mode editing via vim-line.
//!
//! Session file management is ported from the original REPL (crates/repl/src/main.rs).
//! Expressions accumulate in a temp file with `stack.dump` to show values.

use crate::completion::CompletionManager;
use crate::engine::{AnalysisResult, analyze, analyze_expression};
use crate::ir::stack_art::{Stack, StackEffect, render_transition};
use crate::keys::convert_key;
use crate::ui::ir_pane::{IrContent, IrPane, IrViewMode};
use crate::ui::layout::{ComputedLayout, LayoutConfig, StatusContent};
use crate::ui::repl_pane::{HistoryEntry, ReplPane, ReplState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::fs;
use std::io::{BufRead, BufReader, Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};
use tempfile::NamedTempFile;

/// Default execution timeout in seconds (can be overridden via SEQ_REPL_TIMEOUT)
/// Set higher now that weaves don't block (issue #287 fix) - this is a safety net
const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Result of running a command with timeout
#[allow(dead_code)] // Fields kept for API completeness
enum RunResult {
    /// Command completed successfully
    Success { stdout: String, stderr: String },
    /// Command failed with non-zero exit
    Failed {
        stdout: String,
        stderr: String,
        status: ExitStatus,
    },
    /// Command timed out and was killed
    Timeout,
    /// Command failed to start
    Error(String),
}

/// Run a compiled program with a timeout
///
/// This prevents the REPL from hanging indefinitely when a program blocks
/// (e.g., creating a weave without resuming it).
fn run_with_timeout(path: &Path) -> RunResult {
    let timeout_secs = std::env::var("SEQ_REPL_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    let timeout = Duration::from_secs(timeout_secs);

    // Spawn the child process
    let mut child = match Command::new(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return RunResult::Error(format!("Failed to start: {}", e)),
    };

    let start = Instant::now();
    let poll_interval = Duration::from_millis(50);

    // Poll for completion with timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited - collect output
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        let _ = s.read_to_string(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        let _ = s.read_to_string(&mut buf);
                        buf
                    })
                    .unwrap_or_default();

                if status.success() {
                    return RunResult::Success { stdout, stderr };
                } else {
                    return RunResult::Failed {
                        stdout,
                        stderr,
                        status,
                    };
                }
            }
            Ok(None) => {
                // Still running - check timeout
                if start.elapsed() >= timeout {
                    // Timeout - kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return RunResult::Timeout;
                }
                // Brief sleep before next poll
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return RunResult::Error(format!("Wait error: {}", e));
            }
        }
    }
}
use vim_line::{Action, LineEditor, TextEdit, VimLineEditor};

/// REPL template for new sessions (same as original REPL)
const REPL_TEMPLATE: &str = r#"# Seq REPL session
# Expressions are auto-printed via stack.dump

# --- includes ---

# --- definitions ---

# --- main ---
: main ( -- )
"#;

/// Closing for the main word
const MAIN_CLOSE: &str = "  stack.dump\n;\n";

/// Marker for includes section
const INCLUDES_MARKER: &str = "# --- includes ---";

/// Marker for main section
const MAIN_MARKER: &str = "# --- main ---";

/// Main application state
pub struct App {
    /// REPL state (history, input, cursor)
    pub repl_state: ReplState,
    /// IR content for visualization
    pub ir_content: IrContent,
    /// Current IR view mode
    pub ir_mode: IrViewMode,
    /// Vim-style line editor
    pub editor: VimLineEditor,
    /// Layout configuration
    pub layout_config: LayoutConfig,
    /// Current filename (display name)
    pub filename: String,
    /// Whether the IR pane is visible
    pub show_ir_pane: bool,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Whether the app should open editor
    pub should_edit: bool,
    /// Status message (clears after next action)
    pub status_message: Option<String>,
    /// Session file path (temp file or user-provided file)
    pub session_path: PathBuf,
    /// Temp file handle (kept alive to prevent deletion)
    _temp_file: Option<NamedTempFile>,
    /// Completion manager (handles LSP and builtin completions)
    completions: CompletionManager,
}

// Note: App intentionally does not implement Default because App::new() can fail
// (temp file creation, file I/O). Use App::new() directly and handle the Result.

/// Maximum history entries to keep in memory
const MAX_HISTORY_IN_MEMORY: usize = 1000;

/// Find the largest byte index <= pos that is a valid char boundary.
/// This is a stable implementation of the nightly `str::floor_char_boundary`.
fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        s.len()
    } else if s.is_char_boundary(pos) {
        pos
    } else {
        // Walk backwards to find a valid boundary
        let mut p = pos;
        while p > 0 && !s.is_char_boundary(p) {
            p -= 1;
        }
        p
    }
}

impl App {
    /// Create a new application with a temp session file
    pub fn new() -> Result<Self, String> {
        // Create temp file for session
        let temp_file = NamedTempFile::with_suffix(".seq")
            .map_err(|e| format!("Failed to create temp file: {}", e))?;
        let session_path = temp_file.path().to_path_buf();

        // Initialize with template
        let initial_content = format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE);
        fs::write(&session_path, &initial_content)
            .map_err(|e| format!("Failed to write session file: {}", e))?;

        // Create completion manager with LSP if available
        let completions = CompletionManager::try_with_lsp(&session_path, &initial_content);

        let mut app = Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            editor: VimLineEditor::new(),
            layout_config: LayoutConfig::default(),
            filename: "(scratch)".to_string(),
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path,
            _temp_file: Some(temp_file),
            completions,
        };
        app.load_history();
        Ok(app)
    }

    /// Create application with an existing file
    pub fn with_file(path: PathBuf) -> Result<Self, String> {
        let filename = path.display().to_string();

        // Check if file exists, create if not
        let content = if !path.exists() {
            let c = format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE);
            fs::write(&path, &c).map_err(|e| format!("Failed to create session file: {}", e))?;
            c
        } else {
            match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "Warning: Could not read session file '{}': {}",
                        path.display(),
                        e
                    );
                    eprintln!("Starting with empty session.");
                    String::new()
                }
            }
        };

        // Create completion manager with LSP if available
        let completions = CompletionManager::try_with_lsp(&path, &content);

        let mut app = Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            editor: VimLineEditor::new(),
            layout_config: LayoutConfig::default(),
            filename,
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path: path,
            _temp_file: None,
            completions,
        };
        app.load_history();
        Ok(app)
    }

    /// Get the history file path (shared with original REPL)
    fn history_file_path() -> Option<PathBuf> {
        home::home_dir().map(|d| d.join(".local/share/seqr_history"))
    }

    /// Load history from file
    fn load_history(&mut self) {
        if let Some(path) = Self::history_file_path()
            && path.exists()
            && let Ok(file) = fs::File::open(&path)
        {
            let reader = BufReader::new(file);
            // Collect lines, then take only the last MAX_HISTORY_IN_MEMORY entries
            let lines: Vec<String> = reader
                .lines()
                .map_while(Result::ok)
                .filter(|line| !line.is_empty())
                .collect();

            // Only load the most recent entries to prevent memory exhaustion
            let start = lines.len().saturating_sub(MAX_HISTORY_IN_MEMORY);
            for line in &lines[start..] {
                // Add as history entry (no output - it's from a previous session)
                self.repl_state
                    .add_entry(HistoryEntry::new(line.clone()).with_output("(previous session)"));
            }
        }
    }

    /// Save history to file
    pub fn save_history(&self) {
        if let Some(path) = Self::history_file_path() {
            // Ensure parent directory exists
            if let Some(parent) = path.parent()
                && let Err(e) = fs::create_dir_all(parent)
            {
                eprintln!("Warning: could not create history directory: {e}");
                return;
            }

            match fs::File::create(&path) {
                Ok(mut file) => {
                    // Save the last 1000 entries
                    let start = self.repl_state.history.len().saturating_sub(1000);
                    for entry in &self.repl_state.history[start..] {
                        if let Err(e) = writeln!(file, "{}", entry.input) {
                            eprintln!("Warning: could not write history entry: {e}");
                            return;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: could not create history file: {e}");
                }
            }
        }
    }

    /// Check if editor is in normal mode (for completion navigation)
    fn is_normal_mode(&self) -> bool {
        self.editor.status() == "NORMAL"
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Clear status message on any key
        self.status_message = None;

        // Handle completion popup navigation first
        if self.completions.is_visible() {
            match key.code {
                KeyCode::Esc => {
                    self.completions.hide();
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') if self.is_normal_mode() => {
                    self.completions.up();
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if self.is_normal_mode() => {
                    self.completions.down();
                    return;
                }
                KeyCode::Up => {
                    self.completions.up();
                    return;
                }
                KeyCode::Down => {
                    self.completions.down();
                    return;
                }
                KeyCode::Tab => {
                    self.completions.down();
                    return;
                }
                KeyCode::Enter => {
                    self.accept_completion();
                    return;
                }
                _ => {
                    // Any other key hides completions and continues
                    self.completions.hide();
                }
            }
        }

        // Global shortcuts (work in any mode)
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('d') | KeyCode::Char('q') => {
                    // Ctrl+C, Ctrl+D (EOF), Ctrl+Q all quit
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('l') => {
                    // Clear screen / refresh
                    return;
                }
                KeyCode::Char('n') => {
                    // Cycle IR view modes (when visible)
                    if self.show_ir_pane {
                        self.ir_mode = self.ir_mode.next();
                    }
                    return;
                }
                _ => {}
            }
        }

        // Function keys toggle IR pane views (F1=Stack, F2=AST, F3=LLVM)
        match key.code {
            KeyCode::F(1) => {
                self.toggle_ir_view(IrViewMode::StackArt);
                return;
            }
            KeyCode::F(2) => {
                self.toggle_ir_view(IrViewMode::TypedAst);
                return;
            }
            KeyCode::F(3) => {
                self.toggle_ir_view(IrViewMode::LlvmIr);
                return;
            }
            _ => {}
        }

        // Tab triggers completion (before vim-line, which doesn't handle Tab)
        if key.code == KeyCode::Tab {
            self.request_completions();
            return;
        }

        // Shift+Enter in Insert mode inserts a newline
        // Terminals report Shift+Enter differently:
        // - Some as Enter with SHIFT modifier
        // - Some as Enter with ALT modifier (e.g., macOS Terminal/iTerm)
        // - Some as Char('\n')
        let is_modified_enter = key.code == KeyCode::Enter
            && (key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT));
        let is_newline_char = key.code == KeyCode::Char('\n');

        if (is_modified_enter || is_newline_char) && self.editor.status() == "INSERT" {
            let cursor = self.editor.cursor();
            self.repl_state.input.insert(cursor, '\n');
            self.editor.set_cursor(cursor + 1, &self.repl_state.input);
            self.repl_state.cursor = self.editor.cursor();
            return;
        }

        // Enter in Insert mode submits (REPL behavior, not vim's newline insertion)
        if key.code == KeyCode::Enter && self.editor.status() == "INSERT" {
            self.execute_input();
            return;
        }

        // Context-aware j/k in Normal mode: navigate history when at buffer boundaries
        // This makes j/k intuitive for single-line inputs (the common case) while
        // still supporting multi-line navigation when there are multiple lines
        if self.editor.status() == "NORMAL" {
            let input = &self.repl_state.input;
            let cursor = floor_char_boundary(input, self.editor.cursor());

            match key.code {
                KeyCode::Char('k') => {
                    // k at top line (or single line) → history prev
                    let on_first_line = !input[..cursor].contains('\n');
                    if on_first_line {
                        self.navigate_history_prev();
                        return;
                    }
                }
                KeyCode::Char('j') => {
                    // j at bottom line (or single line) → history next
                    let on_last_line = !input[cursor..].contains('\n');
                    if on_last_line {
                        self.navigate_history_next();
                        return;
                    }
                }
                _ => {}
            }
        }

        // Convert to vim-line key and process
        let vl_key = convert_key(key);
        let result = self.editor.handle_key(vl_key, &self.repl_state.input);

        // Apply text edits (in reverse order to preserve offsets)
        let had_edits = !result.edits.is_empty();
        for edit in result.edits.into_iter().rev() {
            match edit {
                TextEdit::Delete { start, end } => {
                    self.repl_state.input.replace_range(start..end, "");
                }
                TextEdit::Insert { at, text } => {
                    self.repl_state.input.insert_str(at, &text);
                }
            }
        }

        // Sync cursor from editor
        self.repl_state.cursor = self.editor.cursor();

        // Handle actions
        if let Some(action) = result.action {
            match action {
                Action::Submit => {
                    self.execute_input();
                }
                Action::HistoryPrev => {
                    self.navigate_history_prev();
                }
                Action::HistoryNext => {
                    self.navigate_history_next();
                }
                Action::Cancel => {
                    self.should_quit = true;
                }
            }
        }

        // Update IR preview if text changed
        if had_edits {
            self.update_ir_preview();
        }
    }

    /// Execute the current input
    fn execute_input(&mut self) {
        let input = self.repl_state.current_input().to_string();
        if input.trim().is_empty() {
            return;
        }

        // Handle REPL commands (start with : but not ": " which is a word definition)
        let trimmed = input.trim_start();
        if trimmed.starts_with(':') && !trimmed.starts_with(": ") && !trimmed.starts_with(":\t") {
            let cmd = input.clone();
            self.handle_command(&cmd);
            return;
        }

        // Check if this is a word definition
        if trimmed.starts_with(": ") || trimmed.starts_with(":\t") {
            self.try_definition(&input);
            return;
        }

        // It's an expression - append to session and run
        self.try_expression(&input);
    }

    /// Try adding a word definition to the session file
    fn try_definition(&mut self, def: &str) {
        // Save current content for rollback
        let original = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(e) => {
                self.add_error_entry(def, &format!("Error reading file: {}", e));
                return;
            }
        };

        // Add definition before main marker
        if !self.add_definition(def) {
            return;
        }

        // Try to compile
        let output_path = self.session_path.with_extension("");
        match seqc::compile_file(&self.session_path, &output_path, false) {
            Ok(_) => {
                let _ = fs::remove_file(&output_path);
                self.repl_state
                    .add_entry(HistoryEntry::new(def).with_output("Defined."));
                self.repl_state.clear_input();
            }
            Err(e) => {
                // Rollback
                if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                    self.status_message = Some(format!(
                        "Warning: Could not rollback session file: {}",
                        rollback_err
                    ));
                }
                self.add_error_entry(def, &e.to_string());
            }
        }
    }

    /// Add a definition to the definitions section
    fn add_definition(&mut self, def: &str) -> bool {
        let content = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Find the main marker
        let main_pos = match content.find(MAIN_MARKER) {
            Some(p) => p,
            None => return false,
        };

        // Insert definition before the main marker
        let mut new_content = String::new();
        new_content.push_str(&content[..main_pos]);
        new_content.push_str(def);
        new_content.push_str("\n\n");
        new_content.push_str(&content[main_pos..]);

        fs::write(&self.session_path, new_content).is_ok()
    }

    /// Try an expression: append to session, compile, run, show output
    fn try_expression(&mut self, expr: &str) {
        // Save current content for rollback
        let original = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(e) => {
                self.add_error_entry(expr, &format!("Error reading file: {}", e));
                return;
            }
        };

        // Append the expression
        if !self.append_expression(expr) {
            self.add_error_entry(expr, "Failed to append expression");
            return;
        }

        // Try to compile and run
        let output_path = self.session_path.with_extension("");
        match seqc::compile_file(&self.session_path, &output_path, false) {
            Ok(_) => {
                // Run with timeout to prevent hanging on blocked operations
                let result = run_with_timeout(&output_path);
                let _ = fs::remove_file(&output_path);

                match result {
                    RunResult::Success { stdout, stderr: _ } => {
                        // Update IR from the session file - only on success
                        self.update_ir_from_session(expr);

                        let output_text = stdout.trim();
                        if output_text.is_empty() {
                            self.repl_state
                                .add_entry(HistoryEntry::new(expr).with_output("ok"));
                        } else {
                            self.repl_state
                                .add_entry(HistoryEntry::new(expr).with_output(output_text));
                        }
                    }
                    RunResult::Failed {
                        stdout: _,
                        stderr,
                        status,
                    } => {
                        // Rollback on runtime error - don't keep failed expression in session
                        if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                            self.status_message = Some(format!(
                                "Warning: Could not rollback session file: {}",
                                rollback_err
                            ));
                        }
                        let err = if stderr.is_empty() {
                            format!("exit: {:?}", status.code())
                        } else {
                            stderr.trim().to_string()
                        };
                        self.repl_state
                            .add_entry(HistoryEntry::new(expr).with_error(&err));
                    }
                    RunResult::Timeout => {
                        // Rollback on timeout - the expression caused blocking
                        if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                            self.status_message = Some(format!(
                                "Warning: Could not rollback session file: {}",
                                rollback_err
                            ));
                        }
                        self.repl_state
                            .add_entry(HistoryEntry::new(expr).with_error(
                                "Timeout: execution exceeded limit (infinite loop? blocking I/O?)",
                            ));
                    }
                    RunResult::Error(e) => {
                        // Rollback on run error - don't keep failed expression in session
                        if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                            self.status_message = Some(format!(
                                "Warning: Could not rollback session file: {}",
                                rollback_err
                            ));
                        }
                        self.add_error_entry(expr, &format!("Run error: {}", e));
                    }
                }
                self.repl_state.clear_input();
            }
            Err(e) => {
                // Rollback
                if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                    self.status_message = Some(format!(
                        "Warning: Could not rollback session file: {}",
                        rollback_err
                    ));
                }
                self.add_error_entry(expr, &e.to_string());
            }
        }
    }

    /// Append an expression to main (before stack.dump)
    fn append_expression(&mut self, expr: &str) -> bool {
        // Don't persist stack.dump - it's an introspection command that should only
        // run once. The auto-appended stack.dump at the end of main will show the
        // current stack state. This fixes issue #193 where user-typed stack.dump
        // accumulated in the session file, causing multiple "stack:" lines.
        if expr.trim() == "stack.dump" {
            return true; // Skip appending but allow compile/run to proceed
        }

        let content = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Find "stack.dump" which marks the end of user code
        let dump_pos = match content.find("  stack.dump") {
            Some(p) => p,
            None => return false,
        };

        // Insert new expression before stack.dump
        let mut new_content = String::new();
        new_content.push_str(&content[..dump_pos]);
        new_content.push_str("  ");
        new_content.push_str(expr);
        new_content.push('\n');
        new_content.push_str(&content[dump_pos..]);

        fs::write(&self.session_path, new_content).is_ok()
    }

    /// Pop the last expression from main
    fn pop_last_expression(&mut self) -> bool {
        let content = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Find ": main ( -- )" line end
        let main_pos = match content.find(": main") {
            Some(p) => p,
            None => return false,
        };
        let main_line_end = match content[main_pos..].find('\n') {
            Some(p) => main_pos + p + 1,
            None => return false,
        };

        // Find "  stack.dump"
        let dump_pos = match content.find("  stack.dump") {
            Some(p) => p,
            None => return false,
        };

        // Get the expressions section
        let expr_section = &content[main_line_end..dump_pos];
        let lines: Vec<&str> = expr_section.lines().collect();

        // Find last non-empty line
        let mut last_expr_idx = None;
        for (i, line) in lines.iter().enumerate().rev() {
            if !line.trim().is_empty() {
                last_expr_idx = Some(i);
                break;
            }
        }

        let last_expr_idx = match last_expr_idx {
            Some(i) => i,
            None => return false, // Nothing to pop
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

        fs::write(&self.session_path, new_content).is_ok()
    }

    /// Clear the session (reset to template)
    fn clear_session(&mut self) {
        if let Err(e) = fs::write(
            &self.session_path,
            format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE),
        ) {
            self.status_message = Some(format!("Warning: Could not clear session file: {}", e));
            return;
        }
        self.repl_state = ReplState::new();
        self.ir_content = IrContent::new();
    }

    /// Add an include to the includes section
    fn add_include(&mut self, module: &str) -> bool {
        let content = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Check if already included
        let include_stmt = format!("include {}", module);
        if content.contains(&include_stmt) {
            self.status_message = Some(format!("'{}' is already included.", module));
            return false;
        }

        // Find the includes marker
        let includes_pos = match content.find(INCLUDES_MARKER) {
            Some(p) => p,
            None => return false,
        };

        // Find end of marker line
        let marker_end = includes_pos + INCLUDES_MARKER.len();
        let after_marker = &content[marker_end..];
        let newline_pos = after_marker.find('\n').unwrap_or(0);
        let insert_pos = marker_end + newline_pos + 1;

        // Insert include after marker
        let mut new_content = String::new();
        new_content.push_str(&content[..insert_pos]);
        new_content.push_str("include ");
        new_content.push_str(module);
        new_content.push('\n');
        new_content.push_str(&content[insert_pos..]);

        fs::write(&self.session_path, new_content).is_ok()
    }

    /// Try including a module
    fn try_include(&mut self, module: &str) {
        let original = match fs::read_to_string(&self.session_path) {
            Ok(c) => c,
            Err(_) => return,
        };

        if !self.add_include(module) {
            return;
        }

        // Try to compile
        let output_path = self.session_path.with_extension("");
        match seqc::compile_file(&self.session_path, &output_path, false) {
            Ok(_) => {
                let _ = fs::remove_file(&output_path);
                self.status_message = Some(format!("Included '{}'.", module));
            }
            Err(e) => {
                if let Err(rollback_err) = fs::write(&self.session_path, &original) {
                    self.status_message = Some(format!(
                        "Include error: {} (also failed to rollback: {})",
                        e, rollback_err
                    ));
                } else {
                    self.status_message = Some(format!("Include error: {}", e));
                }
            }
        }
        self.repl_state.clear_input();
    }

    /// Update IR from the current session file
    fn update_ir_from_session(&mut self, expr: &str) {
        if let Ok(source) = fs::read_to_string(&self.session_path) {
            let result = analyze(&source);
            if result.errors.is_empty() {
                self.update_ir_from_result(&result, expr);
            }
        }
    }

    /// Helper to add an error entry
    fn add_error_entry(&mut self, input: &str, error: &str) {
        self.repl_state
            .add_entry(HistoryEntry::new(input).with_error(error));
        self.repl_state.clear_input();
    }

    /// Handle a REPL command
    fn handle_command(&mut self, cmd: &str) {
        let cmd = cmd.trim();
        match cmd {
            ":q" | ":quit" => {
                self.should_quit = true;
            }
            ":clear" => {
                self.clear_session();
                self.status_message = Some("Session cleared.".to_string());
            }
            ":pop" => {
                if self.pop_last_expression() {
                    // Recompile and show new stack state
                    self.compile_and_show_stack("(after pop)");
                    self.status_message = Some("Popped last expression.".to_string());
                } else {
                    self.status_message = Some("Nothing to pop.".to_string());
                }
            }
            ":stack" | ":s" => {
                // Show current stack state
                self.compile_and_show_stack("Stack:");
            }
            ":show" => {
                // Show session file contents in IR pane
                if let Ok(content) = fs::read_to_string(&self.session_path) {
                    self.ir_content = IrContent {
                        stack_art: content.lines().map(String::from).collect(),
                        typed_ast: vec!["(session file contents)".to_string()],
                        llvm_ir: vec![],
                        errors: vec![],
                    };
                    self.ir_mode = IrViewMode::StackArt;
                }
            }
            ":ir" => {
                // Toggle IR pane visibility
                self.show_ir_pane = !self.show_ir_pane;
                if self.show_ir_pane {
                    self.status_message =
                        Some(format!("IR: {} (Ctrl+N to cycle)", self.ir_mode.name()));
                } else {
                    self.status_message = Some("IR pane hidden".to_string());
                }
            }
            ":ir stack" => {
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::StackArt;
                self.status_message = Some("IR: Stack Effects".to_string());
            }
            ":ir ast" => {
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::TypedAst;
                self.status_message = Some("IR: Typed AST".to_string());
            }
            ":ir llvm" => {
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::LlvmIr;
                self.status_message = Some("IR: LLVM IR".to_string());
            }
            ":edit" | ":e" => {
                // Signal that we need to open editor (handled by run loop)
                self.should_edit = true;
            }
            ":help" | ":h" => {
                // Show help in the IR pane
                self.ir_content = IrContent {
                    stack_art: vec![
                        "╭─────────────────────────────────────╮".to_string(),
                        "│           Seq TUI REPL              │".to_string(),
                        "╰─────────────────────────────────────╯".to_string(),
                        String::new(),
                        "COMMANDS".to_string(),
                        "  :q, :quit     Exit the REPL".to_string(),
                        "  :clear        Clear session and history".to_string(),
                        "  :pop          Remove last expression".to_string(),
                        "  :stack, :s    Show current stack".to_string(),
                        "  :show         Show session file".to_string(),
                        "  :edit, :e     Open in $EDITOR".to_string(),
                        "  :ir           Toggle IR pane".to_string(),
                        "  :ir stack     Show stack effects".to_string(),
                        "  :ir ast       Show typed AST".to_string(),
                        "  :ir llvm      Show LLVM IR".to_string(),
                        "  :include <m>  Include module".to_string(),
                        "  :help, :h     Show this help".to_string(),
                        String::new(),
                        "VI MODE".to_string(),
                        "  i, a, A, I    Enter insert mode".to_string(),
                        "  Esc           Return to normal mode".to_string(),
                        "  h, l          Move cursor left/right".to_string(),
                        "  j, k          History down/up".to_string(),
                        "  w, b          Word forward/backward".to_string(),
                        "  0, $          Line start/end".to_string(),
                        "  x             Delete character".to_string(),
                        "  d             Clear line".to_string(),
                        String::new(),
                        "KEYS".to_string(),
                        "  F1            Toggle Stack Effects".to_string(),
                        "  F2            Toggle Typed AST".to_string(),
                        "  F3            Toggle LLVM IR".to_string(),
                        "  Tab           Show completions".to_string(),
                        "  Ctrl+N        Cycle IR views".to_string(),
                        "  Ctrl+D        Exit REPL".to_string(),
                        "  Enter         Execute expression".to_string(),
                        "  Up/Down       History navigation".to_string(),
                    ],
                    typed_ast: vec![],
                    llvm_ir: vec![],
                    errors: vec![],
                };
                self.ir_mode = IrViewMode::StackArt;
                self.show_ir_pane = true;
            }
            _ if cmd.starts_with(":include ") => {
                // Safe: we just verified the prefix exists
                let module = &cmd[":include ".len()..].trim();
                if module.is_empty() {
                    self.status_message = Some("Usage: :include <module>".to_string());
                } else {
                    self.try_include(module);
                    return; // try_include clears input
                }
            }
            _ => {
                self.status_message = Some(format!("Unknown command: {}", cmd));
            }
        }
        self.repl_state.clear_input();
    }

    /// Compile session and show current stack (used after :pop)
    fn compile_and_show_stack(&mut self, label: &str) {
        let output_path = self.session_path.with_extension("");
        match seqc::compile_file(&self.session_path, &output_path, false) {
            Ok(_) => {
                let result = run_with_timeout(&output_path);
                let _ = fs::remove_file(&output_path);

                match result {
                    RunResult::Success { stdout, stderr: _ } => {
                        let output_text = stdout.trim();
                        if !output_text.is_empty() {
                            self.repl_state
                                .add_entry(HistoryEntry::new(label).with_output(output_text));
                        } else {
                            self.repl_state
                                .add_entry(HistoryEntry::new(label).with_output("(empty)"));
                        }
                    }
                    RunResult::Timeout => {
                        self.status_message = Some("Timeout while showing stack".to_string());
                    }
                    _ => {
                        // Failed or Error - just ignore for stack display
                    }
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Compile error: {}", e));
            }
        }
    }

    /// Update IR preview as user types
    fn update_ir_preview(&mut self) {
        let input = self.repl_state.current_input().to_string();
        if input.trim().is_empty() {
            self.ir_content = IrContent::new();
            return;
        }

        // For live preview, just show stack art for known words
        // Don't run full analysis on every keystroke - too noisy with errors
        self.ir_content = IrContent {
            stack_art: self.generate_stack_art(&input),
            typed_ast: vec![format!("Expression: {}", input)],
            llvm_ir: vec!["(compile with Enter to see LLVM IR)".to_string()],
            errors: vec![],
        };
    }

    /// Navigate to previous history entry (older)
    fn navigate_history_prev(&mut self) {
        self.repl_state.history_up();
        self.editor.reset();
        self.editor
            .set_cursor(self.repl_state.input.len(), &self.repl_state.input);
        self.repl_state.cursor = self.editor.cursor();
    }

    /// Navigate to next history entry (newer)
    fn navigate_history_next(&mut self) {
        self.repl_state.history_down();
        self.editor.reset();
        self.editor
            .set_cursor(self.repl_state.input.len(), &self.repl_state.input);
        self.repl_state.cursor = self.editor.cursor();
    }

    /// Toggle IR pane to a specific view mode
    /// If already showing this mode, hide the pane. Otherwise show/switch to it.
    fn toggle_ir_view(&mut self, mode: IrViewMode) {
        if self.show_ir_pane && self.ir_mode == mode {
            // Same view - toggle off
            self.show_ir_pane = false;
            self.status_message = Some("IR pane hidden".to_string());
        } else {
            // Different view or hidden - show this view
            self.show_ir_pane = true;
            self.ir_mode = mode;
            self.status_message = Some(format!("IR: {}", mode.name()));
        }
    }

    /// Update IR content from analysis result
    fn update_ir_from_result(&mut self, _result: &AnalysisResult, input: &str) {
        // Generate stack art for the expression
        let stack_art = self.generate_stack_art(input);

        // Typed AST placeholder
        let typed_ast = vec![
            format!("Expression: {}", input),
            String::new(),
            "Types inferred successfully".to_string(),
        ];

        // LLVM IR - compile the expression standalone for clean, focused IR
        let llvm_ir = analyze_expression(input)
            .unwrap_or_else(|| vec!["(expression could not be compiled standalone)".to_string()]);

        self.ir_content = IrContent {
            stack_art,
            typed_ast,
            llvm_ir,
            errors: vec![],
        };
    }

    /// Generate stack art for an expression
    fn generate_stack_art(&self, input: &str) -> Vec<String> {
        // Parse the expression into words and generate stack transitions
        let words: Vec<&str> = input.split_whitespace().collect();
        if words.is_empty() {
            return vec![];
        }

        let mut lines = vec![format!("Expression: {}", input), String::new()];

        // For now, show individual word effects
        for word in &words {
            if let Some(effect) = self.get_word_effect(word) {
                let before = Stack::with_rest("s");
                let after = Stack::with_rest("s");
                let transition = render_transition(&effect, &before, &after);
                lines.extend(transition);
                lines.push(String::new());
            }
        }

        if lines.len() <= 2 {
            lines.push("(no stack effects to display)".to_string());
        }

        lines
    }

    /// Get the stack effect for a word or literal
    fn get_word_effect(&self, word: &str) -> Option<StackEffect> {
        // Check for literals first
        if word.parse::<i64>().is_ok() {
            return Some(StackEffect::literal(word));
        }
        if word.parse::<f64>().is_ok() && word.contains('.') {
            return Some(StackEffect::literal(word));
        }
        if word == "true" || word == "false" {
            return Some(StackEffect::literal(word));
        }

        // Look up in static effects table
        crate::ir::stack_effects::get_effect(word).cloned()
    }

    /// Request completions from LSP or builtins
    fn request_completions(&mut self) {
        let input = &self.repl_state.input;
        let cursor = self.repl_state.cursor;

        if let Some(msg) = self.completions.request(input, cursor, &self.session_path) {
            self.status_message = Some(msg);
        } else if self.completions.items().is_empty() {
            self.status_message = Some("No completions".to_string());
        }
    }

    /// Accept the current completion
    fn accept_completion(&mut self) {
        let input = &self.repl_state.input;
        let cursor = self.repl_state.cursor;

        if let Some((word_start, completion)) = self.completions.accept(input, cursor) {
            let before = &input[..word_start];
            let after = &input[cursor..];

            self.repl_state.input = format!("{}{}{}", before, completion, after);
            self.repl_state.cursor = word_start + completion.len();
            self.update_ir_preview();
        }
    }

    /// Render the application to a frame
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let layout = ComputedLayout::compute(area, &self.layout_config, self.show_ir_pane);

        // Render REPL pane (always focused, no border)
        // Cursor should always be visible in both Normal and Insert modes
        let repl_pane = ReplPane::new(&self.repl_state).focused(true).prompt(
            if self.editor.status() == "INSERT" {
                "seq> "
            } else {
                "seq: "
            },
        );
        frame.render_widget(&repl_pane, layout.repl);

        // Render IR pane (if enabled and space available)
        if self.show_ir_pane && layout.ir_visible() {
            let ir_pane = IrPane::new(&self.ir_content).mode(self.ir_mode);
            frame.render_widget(&ir_pane, layout.ir);
        }

        // Render status bar
        self.render_status_bar(frame, layout.status);

        // Render completion popup (on top of everything)
        if self.completions.is_visible() && !self.completions.items().is_empty() {
            self.render_completions(frame, layout.repl);
        }
    }

    /// Render the status bar
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status = StatusContent::new()
            .filename(&self.filename)
            .mode(self.editor.status())
            .ir_view(self.ir_mode.name());

        let status_text = if let Some(msg) = &self.status_message {
            msg.clone()
        } else {
            status.format(area.width)
        };

        let style = Style::default().bg(Color::DarkGray).fg(Color::White);
        let paragraph = Paragraph::new(Line::from(Span::styled(status_text, style)));
        frame.render_widget(paragraph, area);
    }

    /// Render the completion popup
    fn render_completions(&self, frame: &mut Frame, repl_area: Rect) {
        let items = self.completions.items();
        let selected_index = self.completions.index();

        // Calculate popup position (above the input line)
        let popup_height = (items.len() + 2) as u16; // +2 for border
        let popup_width = items.iter().map(|c| c.label.len()).max().unwrap_or(10) as u16 + 4; // +4 for padding and border

        // Position popup near the cursor
        let prompt_len = 5; // "seq> " or "seq: "
        let x = repl_area.x + prompt_len + self.repl_state.cursor as u16;
        let x = x.min(repl_area.right().saturating_sub(popup_width));

        // Put it above the current line if possible
        let y = if repl_area.bottom() > popup_height + 1 {
            repl_area.bottom() - popup_height - 1
        } else {
            repl_area.y
        };

        let popup_area = Rect::new(x, y, popup_width, popup_height);

        // Clear the area first
        frame.render_widget(Clear, popup_area);

        // Build completion lines
        let lines: Vec<Line> = items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == selected_index {
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(Span::styled(format!(" {} ", item.label), style))
            })
            .collect();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, popup_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() -> Result<(), String> {
        let app = App::new()?;
        assert_eq!(app.editor.status(), "NORMAL");
        assert!(!app.should_quit);
        Ok(())
    }

    #[test]
    fn test_mode_switching() -> Result<(), String> {
        let mut app = App::new()?;

        // i enters insert mode
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.editor.status(), "INSERT");

        // Esc returns to normal
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.editor.status(), "NORMAL");
        Ok(())
    }

    #[test]
    fn test_insert_mode_typing() -> Result<(), String> {
        let mut app = App::new()?;
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));

        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));

        assert_eq!(app.repl_state.input, "hi");
        Ok(())
    }

    #[test]
    fn test_normal_mode_navigation() -> Result<(), String> {
        let mut app = App::new()?;

        // Type "hello" in insert mode
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        for c in "hello".chars() {
            app.handle_key(KeyEvent::from(KeyCode::Char(c)));
        }
        app.handle_key(KeyEvent::from(KeyCode::Esc));

        // Now in normal mode at position 4 (esc moves left one from end)
        // Move to start with 0
        app.handle_key(KeyEvent::from(KeyCode::Char('0')));
        assert_eq!(app.repl_state.cursor, 0);

        // l moves right
        app.handle_key(KeyEvent::from(KeyCode::Char('l')));
        assert_eq!(app.repl_state.cursor, 1);

        // h moves left
        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        assert_eq!(app.repl_state.cursor, 0);

        // $ goes to end (ON last char, not past it - vim normal mode behavior)
        app.handle_key(KeyEvent::from(KeyCode::Char('$')));
        assert_eq!(app.repl_state.cursor, 4); // 'o' is at index 4
        Ok(())
    }

    #[test]
    fn test_history_navigation() -> Result<(), String> {
        let mut app = App::new()?;

        // Add some history entries manually
        app.repl_state
            .add_entry(HistoryEntry::new("first").with_output("1"));
        app.repl_state
            .add_entry(HistoryEntry::new("second").with_output("2"));

        // Up arrow goes up in history (to most recent)
        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.repl_state.input, "second");

        // Up again goes to older entry
        app.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(app.repl_state.input, "first");

        // Down goes back to newer entry
        app.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(app.repl_state.input, "second");
        Ok(())
    }

    #[test]
    fn test_jk_history_navigation() -> Result<(), String> {
        let mut app = App::new()?;

        // Add some history entries
        app.repl_state
            .add_entry(HistoryEntry::new("first").with_output("1"));
        app.repl_state
            .add_entry(HistoryEntry::new("second").with_output("2"));

        // In normal mode (default), k on single line goes to history
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "second");

        // k again goes to older entry
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "first");

        // j goes back to newer entry
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "second");

        // j again returns to empty input (current line)
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "");
        Ok(())
    }

    #[test]
    fn test_jk_multiline_navigation() -> Result<(), String> {
        let mut app = App::new()?;

        // Add history entries: oldest first, then multi-line, then newest
        app.repl_state
            .add_entry(HistoryEntry::new("oldest").with_output("1"));
        app.repl_state
            .add_entry(HistoryEntry::new("line1\nline2\nline3").with_output("2"));

        // Navigate to the multi-line history entry
        app.handle_key(KeyEvent::from(KeyCode::Char('k'))); // Go to newest (multi-line)
        assert_eq!(app.repl_state.input, "line1\nline2\nline3");

        // Cursor should be at end of last line after history navigation
        // j on last line should go forward in history (to current empty input)
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "");

        // k goes back to multi-line
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "line1\nline2\nline3");

        // Now test navigation within the multi-line buffer
        // Go to start of last line with 0
        app.handle_key(KeyEvent::from(KeyCode::Char('0')));

        // k should move up within buffer (to line2), not to history
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "line1\nline2\nline3"); // Still same input

        // k again moves to line1 (now on first line)
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "line1\nline2\nline3"); // Still same input

        // k on first line goes to older history
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "oldest");

        Ok(())
    }

    #[test]
    fn test_jk_with_unicode() -> Result<(), String> {
        let mut app = App::new()?;

        // Add history with emoji
        app.repl_state
            .add_entry(HistoryEntry::new("hello 👋").with_output("1"));

        // k should navigate to history without panicking
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "hello 👋");

        // j should navigate back
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "");

        Ok(())
    }

    #[test]
    fn test_jk_empty_input() -> Result<(), String> {
        let mut app = App::new()?;

        // Add history so we have somewhere to navigate
        app.repl_state
            .add_entry(HistoryEntry::new("history").with_output("1"));

        // With empty input, j/k should work without panic
        assert_eq!(app.repl_state.input, "");
        app.handle_key(KeyEvent::from(KeyCode::Char('k'))); // Should go to history
        assert_eq!(app.repl_state.input, "history");

        app.handle_key(KeyEvent::from(KeyCode::Char('j'))); // Should return to empty
        assert_eq!(app.repl_state.input, "");

        Ok(())
    }

    #[test]
    fn test_jk_insert_mode_no_history() -> Result<(), String> {
        let mut app = App::new()?;

        // Add history
        app.repl_state
            .add_entry(HistoryEntry::new("history").with_output("1"));

        // Enter insert mode
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.editor.status(), "INSERT");

        // j/k in insert mode should NOT navigate history - they should be passed
        // to vim-line which handles them as cursor movement (no-op on single line)
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "j"); // j inserted as text

        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "jk"); // k inserted as text

        Ok(())
    }

    #[test]
    fn test_quit_command() -> Result<(), String> {
        let mut app = App::new()?;
        // Ctrl+Q quits the application
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
        Ok(())
    }

    #[test]
    fn test_repl_command() -> Result<(), String> {
        let mut app = App::new()?;
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        app.handle_key(KeyEvent::from(KeyCode::Char(':')));
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.should_quit);
        Ok(())
    }

    #[test]
    fn test_word_effect_lookup() -> Result<(), String> {
        let app = App::new()?;
        // Stack manipulation
        assert!(app.get_word_effect("dup").is_some());
        assert!(app.get_word_effect("swap").is_some());

        // Integer arithmetic - long and symbolic forms
        assert!(app.get_word_effect("i.add").is_some());
        assert!(app.get_word_effect("i.+").is_some());
        assert!(app.get_word_effect("i.multiply").is_some());
        assert!(app.get_word_effect("i.*").is_some());

        // Integer comparisons
        assert!(app.get_word_effect("i.<").is_some());
        assert!(app.get_word_effect("i.=").is_some());

        // Float arithmetic - long and symbolic forms
        assert!(app.get_word_effect("f.add").is_some());
        assert!(app.get_word_effect("f.*").is_some());

        // Float comparisons
        assert!(app.get_word_effect("f.<").is_some());

        // Unknown word
        assert!(app.get_word_effect("unknown").is_none());
        Ok(())
    }

    #[test]
    fn test_ctrl_c_quits() -> Result<(), String> {
        let mut app = App::new()?;
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.handle_key(key);
        assert!(app.should_quit);
        Ok(())
    }

    #[test]
    fn test_tab_completion() -> Result<(), String> {
        let mut app = App::new()?;

        // Enter insert mode and type "du"
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        app.handle_key(KeyEvent::from(KeyCode::Char('d')));
        app.handle_key(KeyEvent::from(KeyCode::Char('u')));

        // Verify input and cursor are correct
        assert_eq!(app.repl_state.input, "du");
        assert_eq!(app.repl_state.cursor, 2);

        // Press Tab to trigger completion
        app.handle_key(KeyEvent::from(KeyCode::Tab));

        // Should show completions with "dup" matching
        assert!(
            app.completions.is_visible(),
            "Completions should be visible after Tab"
        );
        assert!(
            !app.completions.items().is_empty(),
            "Should have completions for 'du'"
        );
        assert!(
            app.completions.items().iter().any(|c| c.label == "dup"),
            "Should include 'dup' in completions"
        );

        Ok(())
    }

    #[test]
    fn test_tab_completion_empty_prefix() -> Result<(), String> {
        let mut app = App::new()?;

        // Press Tab with empty input
        app.handle_key(KeyEvent::from(KeyCode::Tab));

        // Should not show completions, should show status message
        assert!(
            !app.completions.is_visible(),
            "Should not show completions for empty prefix"
        );
        assert!(
            app.status_message
                .as_ref()
                .is_some_and(|m| m.contains("type a prefix")),
            "Should show 'type a prefix' message"
        );

        Ok(())
    }
}
