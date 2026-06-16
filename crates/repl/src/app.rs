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
use crate::run::{RunResult, run_with_timeout};
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
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use tempfile::NamedTempFile;
use vim_line::{Action, LineEditor, TextEdit, VimLineEditor};

/// REPL template for new sessions.
///
/// All stdlib modules are pre-included so tab completion can surface their
/// words without the user having to remember to `:include` first. Includes
/// are cheap (parse-only until a word is actually called); prune via
/// `:edit` if you want a leaner session.
const REPL_TEMPLATE: &str = r#"# Seq REPL session
# Expressions are auto-printed via stack.dump

# --- includes ---
include std:control
include std:fmath
include std:http
include std:imath
include std:json
include std:list
include std:loops
include std:map
include std:signal
include std:son
include std:stack-utils
include std:yaml
include std:zipper

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

/// REPL command names completable by Tab in CommandLine mode (without the
/// leading `:`). Sorted alphabetically so cycle order is predictable. Keep
/// this in sync with the match arms in `App::handle_command`. `include `
/// keeps a trailing space so completing it positions the cursor for the
/// module argument.
const COMMANDS: &[&str] = &[
    "clear", "e", "edit", "h", "help", "include ", "ir", "ir ast", "ir llvm", "ir stack", "pop",
    "q", "quit", "s", "show", "stack", "v", "version",
];

/// Tab-cycle state for the `:` command line. Built from the buffer the
/// first time Tab is pressed; reset when the user types any non-Tab key.
#[derive(Debug, Clone)]
struct CmdlineCycle {
    /// Buffer contents at the moment cycling began.
    prefix: String,
    /// Indices into `COMMANDS` whose entries start with `prefix`.
    matches: Vec<usize>,
    /// `Some(i)` means we're showing `COMMANDS[matches[i]]`. `None` means
    /// we're showing the original `prefix` (the user-typed buffer).
    showing: Option<usize>,
}

/// Lines shown by the `:help` command in the IR pane.
const HELP_LINES: &[&str] = &[
    "╭─────────────────────────────────────╮",
    "│           Seq TUI REPL              │",
    "╰─────────────────────────────────────╯",
    "",
    "COMMANDS",
    "  :q, :quit     Exit the REPL",
    "  :version, :v  Show version",
    "  :clear        Clear session and history",
    "  :pop          Remove last expression",
    "  :stack, :s    Show current stack",
    "  :show         Show session file",
    "  :edit, :e     Open in $EDITOR",
    "  :ir           Toggle IR pane",
    "  :ir stack     Show stack effects",
    "  :ir ast       Show typed AST",
    "  :ir llvm      Show LLVM IR",
    "  :include <m>  Include module",
    "  :help, :h     Show this help",
    "",
    "VI MODE",
    "  i, a, A, I    Enter insert mode",
    "  Esc           Return to normal mode",
    "  h, l          Move cursor left/right",
    "  j, k          History down/up",
    "  w, b          Word forward/backward",
    "  0, $          Line start/end",
    "  x             Delete character",
    "  d             Clear line",
    "  /             Search history",
    "",
    "KEYS",
    "  F1            Toggle Stack Effects",
    "  F2            Toggle Typed AST",
    "  F3            Toggle LLVM IR",
    "  Tab           Show completions",
    "  Ctrl+N        Cycle IR views",
    "  Ctrl+D        Exit REPL",
    "  Enter         Execute expression",
    "  Up/Down       History navigation",
    "",
    "SEARCH MODE (after /)",
    "  Type          Filter history",
    "  Tab/Shift+Tab Cycle matches",
    "  Enter         Accept match",
    "  Esc           Cancel search",
];

/// State for vim `/` history search, grouped so `App` stays lean.
#[derive(Default)]
struct SearchState {
    /// Whether search mode is active.
    active: bool,
    /// Current search pattern.
    pattern: String,
    /// Indices of history entries matching `pattern`.
    matches: Vec<usize>,
    /// Current match index (into `matches`).
    match_index: usize,
    /// Input before search started (restored on cancel).
    original_input: String,
}

/// Main application state
pub(crate) struct App {
    /// REPL state (history, input, cursor)
    pub(crate) repl_state: ReplState,
    /// IR content for visualization
    pub(crate) ir_content: IrContent,
    /// Current IR view mode
    pub(crate) ir_mode: IrViewMode,
    /// Vim-style line editor
    pub(crate) editor: VimLineEditor,
    /// Layout configuration
    pub(crate) layout_config: LayoutConfig,
    /// Current filename (display name)
    pub(crate) filename: String,
    /// Whether the IR pane is visible
    pub(crate) show_ir_pane: bool,
    /// Whether the app should quit
    pub(crate) should_quit: bool,
    /// Whether the app should open editor
    pub(crate) should_edit: bool,
    /// Status message (clears after next action)
    pub(crate) status_message: Option<String>,
    /// Session file path (temp file or user-provided file)
    pub(crate) session_path: PathBuf,
    /// Temp file handle (kept alive to prevent deletion)
    _temp_file: Option<NamedTempFile>,
    /// Completion manager (handles LSP and builtin completions)
    completions: CompletionManager,
    /// Vim `/` history-search state.
    search: SearchState,
    /// In-progress Tab cycle for the `:` command line.
    cmdline_cycle: Option<CmdlineCycle>,
}

// Note: App intentionally does not implement Default because App::new() can fail
// (temp file creation, file I/O). Use App::new() directly and handle the Result.

/// Maximum history entries to keep in memory
const MAX_HISTORY_IN_MEMORY: usize = 1000;

impl App {
    /// Create a new application with a temp session file
    pub(crate) fn new() -> Result<Self, String> {
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
            editor: VimLineEditor::new().with_command_mode(true),
            layout_config: LayoutConfig::default(),
            filename: "(scratch)".to_string(),
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path,
            _temp_file: Some(temp_file),
            completions,
            search: SearchState::default(),
            cmdline_cycle: None,
        };
        app.load_history();
        Ok(app)
    }

    /// Create application with an existing file
    pub(crate) fn with_file(path: PathBuf) -> Result<Self, String> {
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
            editor: VimLineEditor::new().with_command_mode(true),
            layout_config: LayoutConfig::default(),
            filename,
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path: path,
            _temp_file: None,
            completions,
            search: SearchState::default(),
            cmdline_cycle: None,
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

    /// Save history to file. Reads from the navigation store (the
    /// authoritative deduped/bounded source) rather than the rendered
    /// transcript, so repeated commands don't bloat the saved file.
    pub(crate) fn save_history(&self) {
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
                    for entry in self.repl_state.store.entries() {
                        if let Err(e) = writeln!(file, "{}", entry) {
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

    /// Restore the session file to `original` content; surface a warning via
    /// the status bar if that write fails.
    fn rollback_session(&mut self, original: &str) {
        if let Err(rollback_err) = fs::write(&self.session_path, original) {
            self.status_message = Some(format!(
                "Warning: Could not rollback session file: {}",
                rollback_err
            ));
        }
    }

    /// Sync the vim-line editor and `repl_state.cursor` to the current
    /// `repl_state.input`, placing the cursor at end-of-input.
    fn sync_editor_to_input(&mut self) {
        self.editor.reset();
        self.editor
            .set_cursor(self.repl_state.input.len(), &self.repl_state.input);
        self.repl_state.cursor = self.editor.cursor();
    }

    /// Handle a key event
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        // Clear status message on any key
        self.status_message = None;

        // Each handler returns `true` when it fully consumes the key. Order
        // matters: the completion popup and `/` search are modal and take
        // precedence over normal editing.
        if self.handle_completion_popup_key(key) {
            return;
        }
        if self.search.active {
            self.handle_search_key(key);
            return;
        }
        if self.try_enter_search(key) {
            return;
        }
        if self.handle_ctrl_shortcut(key) {
            return;
        }
        if self.handle_function_key(key) {
            return;
        }
        if self.handle_command_line_tab(key) {
            return;
        }

        // Tab triggers main-buffer completion (before vim-line, which
        // doesn't handle Tab).
        if key.code == KeyCode::Tab {
            self.request_completions();
            return;
        }

        if self.handle_insert_enter(key) {
            return;
        }

        // Default: hand the key to vim-line and apply its edits/actions.
        // vim-line emits `HistoryPrev/Next` for `k`/`j` at the line boundary
        // and for `Up`/`Down` arrows — there's no need for a host-side
        // boundary shim anymore.
        self.dispatch_to_editor(key);
    }

    /// Completion-popup navigation. Returns `true` if the key was consumed; a
    /// non-navigation key hides the popup and returns `false` so the key still
    /// reaches the normal handlers.
    fn handle_completion_popup_key(&mut self, key: KeyEvent) -> bool {
        if !self.completions.is_visible() {
            return false;
        }
        match key.code {
            KeyCode::Esc => self.completions.hide(),
            KeyCode::Up | KeyCode::Char('k') if self.is_normal_mode() => self.completions.up(),
            KeyCode::Down | KeyCode::Char('j') if self.is_normal_mode() => self.completions.down(),
            KeyCode::Up => self.completions.up(),
            KeyCode::Down | KeyCode::Tab => self.completions.down(),
            KeyCode::Enter => self.accept_completion(),
            _ => {
                // Any other key hides completions and continues to normal handling.
                self.completions.hide();
                return false;
            }
        }
        true
    }

    /// Handle a key while in vim `/` search mode. Always consumes the key.
    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc => {
                // Cancel search - restore original input
                self.repl_state.input = self.search.original_input.clone();
                self.sync_editor_to_input();
                self.search.active = false;
                self.search.pattern.clear();
                self.search.matches.clear();
                self.status_message = None;
            }
            KeyCode::Enter => {
                // Accept current match (input already shows preview)
                self.search.active = false;
                self.search.pattern.clear();
                self.search.matches.clear();
            }
            KeyCode::Backspace => {
                self.search.pattern.pop();
                self.refresh_search();
            }
            KeyCode::Tab if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.step_search_match(true);
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.step_search_match(key.code == KeyCode::BackTab);
            }
            KeyCode::Char(c) => {
                self.search.pattern.push(c);
                self.refresh_search();
            }
            _ => {}
        }
        true
    }

    /// Recompute matches for the current pattern and preview the first hit.
    fn refresh_search(&mut self) {
        self.update_search_matches();
        self.preview_current_match();
        self.update_search_status();
    }

    /// Move to the previous (`backward`) or next search match and preview it.
    fn step_search_match(&mut self, backward: bool) {
        if self.search.matches.is_empty() {
            return;
        }
        self.search.match_index = if backward {
            if self.search.match_index == 0 {
                self.search.matches.len() - 1
            } else {
                self.search.match_index - 1
            }
        } else {
            (self.search.match_index + 1) % self.search.matches.len()
        };
        self.preview_current_match();
        self.update_search_status();
    }

    /// Enter vim `/` search mode when `/` is pressed in normal mode.
    fn try_enter_search(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Char('/') && self.is_normal_mode() {
            self.search.active = true;
            self.search.original_input = self.repl_state.input.clone();
            self.search.pattern.clear();
            self.search.matches.clear();
            self.search.match_index = 0;
            self.update_search_status();
            return true;
        }
        false
    }

    /// Ctrl-modified global shortcuts (quit, refresh, cycle IR view).
    fn handle_ctrl_shortcut(&mut self, key: KeyEvent) -> bool {
        if !key.modifiers.contains(KeyModifiers::CONTROL) {
            return false;
        }
        match key.code {
            KeyCode::Char('c') | KeyCode::Char('d') | KeyCode::Char('q') => {
                // Ctrl+C, Ctrl+D (EOF), Ctrl+Q all quit
                self.should_quit = true;
                true
            }
            KeyCode::Char('l') => true, // clear screen / refresh
            KeyCode::Char('n') => {
                // Cycle IR view modes (when visible)
                if self.show_ir_pane {
                    self.ir_mode = self.ir_mode.next();
                }
                true
            }
            _ => false,
        }
    }

    /// Function keys toggle IR pane views (F1=Stack, F2=AST, F3=LLVM).
    fn handle_function_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::F(1) => self.toggle_ir_view(IrViewMode::StackArt),
            KeyCode::F(2) => self.toggle_ir_view(IrViewMode::TypedAst),
            KeyCode::F(3) => self.toggle_ir_view(IrViewMode::LlvmIr),
            _ => return false,
        }
        true
    }

    /// CommandLine mode owns Tab / Shift+Tab for cycling REPL command
    /// completions. Any other key resets the cycle so the next Tab re-derives
    /// matches from the new buffer (returns `false` so the key keeps flowing).
    fn handle_command_line_tab(&mut self, key: KeyEvent) -> bool {
        if self.editor.command_line_buffer().is_none() {
            return false;
        }
        match key.code {
            KeyCode::Tab => {
                self.cycle_command_completion(key.modifiers.contains(KeyModifiers::SHIFT))
            }
            KeyCode::BackTab => self.cycle_command_completion(true),
            _ => {
                self.cmdline_cycle = None;
                return false;
            }
        }
        true
    }

    /// Insert/submit handling for Enter in INSERT mode: Shift/Alt-Enter and
    /// `\n` insert a literal newline; a plain Enter submits.
    fn handle_insert_enter(&mut self, key: KeyEvent) -> bool {
        if self.editor.status() != "INSERT" {
            return false;
        }
        // Terminals report Shift+Enter differently: Enter+SHIFT, Enter+ALT
        // (macOS Terminal/iTerm), or Char('\n').
        let is_modified_enter = key.code == KeyCode::Enter
            && (key.modifiers.contains(KeyModifiers::SHIFT)
                || key.modifiers.contains(KeyModifiers::ALT));
        if is_modified_enter || key.code == KeyCode::Char('\n') {
            let cursor = self.editor.cursor();
            self.repl_state.input.insert(cursor, '\n');
            self.editor.set_cursor(cursor + 1, &self.repl_state.input);
            self.repl_state.cursor = self.editor.cursor();
            return true;
        }
        // Plain Enter submits (REPL behavior, not vim's newline insertion).
        if key.code == KeyCode::Enter {
            self.execute_input();
            return true;
        }
        false
    }

    /// Default key path: hand the key to vim-line, apply its text edits, sync
    /// the cursor, run any resulting action, and refresh the IR preview.
    fn dispatch_to_editor(&mut self, key: KeyEvent) {
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
                // History-search vocabulary is reserved for a future search
                // sub-mode inside vim-line. seqr currently runs its own `/`
                // input loop above (see `handle_search_key`), so these
                // intents are not emitted by the editor today — we just
                // exhaustively match so adding emission later is a no-op
                // on this side.
                Action::HistorySearch(_) | Action::HistoryAccept | Action::HistoryCancel => {}
                Action::SubmitCommand(cmd) => {
                    // CommandLine submitted — re-attach the leading `:` so
                    // handle_command's existing dispatch matches the same
                    // strings as the legacy Insert-typed `:cmd⏎` path.
                    let full = format!(":{}", cmd);
                    self.handle_command(&full);
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

    /// Compile the session file to a sibling binary (the session path with its
    /// extension stripped). Returns the binary path on success — the caller is
    /// responsible for removing it — or the compiler error as a string.
    fn compile_session(&self) -> Result<PathBuf, String> {
        let output_path = self.session_path.with_extension("");
        seqc::compile_file(&self.session_path, &output_path, false)
            .map(|_| output_path)
            .map_err(|e| e.to_string())
    }

    /// Compile and run the session under the REPL timeout, removing the binary
    /// afterward. `Err` is a compile failure; a runtime failure/timeout is
    /// carried in the returned `RunResult`.
    fn compile_and_run(&self) -> Result<RunResult, String> {
        let output_path = self.compile_session()?;
        let result = run_with_timeout(&output_path);
        let _ = fs::remove_file(&output_path);
        Ok(result)
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
        match self.compile_session() {
            Ok(output_path) => {
                let _ = fs::remove_file(&output_path);
                self.repl_state
                    .add_entry(HistoryEntry::new(def).with_output("Defined."));
                self.repl_state.clear_input();
            }
            Err(e) => {
                // Rollback
                self.rollback_session(&original);
                self.add_error_entry(def, &e);
            }
        }
    }

    /// Add a definition to the definitions section
    fn add_definition(&mut self, def: &str) -> bool {
        let Ok(content) = fs::read_to_string(&self.session_path) else {
            return false;
        };

        // Find the main marker
        let Some(main_pos) = content.find(MAIN_MARKER) else {
            return false;
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
        match self.compile_and_run() {
            Ok(result) => {
                match result {
                    RunResult::Success { stdout } => {
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
                    RunResult::Failed { stderr, status } => {
                        // Rollback on runtime error - don't keep failed expression in session
                        self.rollback_session(&original);
                        let err = if stderr.is_empty() {
                            format!("exit: {:?}", status.code())
                        } else {
                            stderr.trim().to_string()
                        };
                        self.repl_state
                            .add_entry(HistoryEntry::new(expr).with_error(&err));
                    }
                    RunResult::Timeout { timeout_secs } => {
                        // Rollback on timeout - the expression caused blocking
                        self.rollback_session(&original);
                        self.repl_state
                            .add_entry(HistoryEntry::new(expr).with_error(format!(
                                "Timeout after {}s (SEQ_REPL_TIMEOUT to adjust)",
                                timeout_secs
                            )));
                    }
                    RunResult::Error(e) => {
                        // Rollback on run error - don't keep failed expression in session
                        self.rollback_session(&original);
                        self.add_error_entry(expr, &format!("Run error: {}", e));
                    }
                }
                self.repl_state.clear_input();
            }
            Err(e) => {
                // Rollback
                self.rollback_session(&original);
                self.add_error_entry(expr, &e);
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

        let Ok(content) = fs::read_to_string(&self.session_path) else {
            return false;
        };

        // Find "stack.dump" which marks the end of user code
        let Some(dump_pos) = content.find("  stack.dump") else {
            return false;
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
        let Ok(content) = fs::read_to_string(&self.session_path) else {
            return false;
        };

        // Find ": main ( -- )" line end
        let Some(main_pos) = content.find(": main") else {
            return false;
        };
        let Some(newline_offset) = content[main_pos..].find('\n') else {
            return false;
        };
        let main_line_end = main_pos + newline_offset + 1;

        // Find "  stack.dump"
        let Some(dump_pos) = content.find("  stack.dump") else {
            return false;
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
        let Ok(content) = fs::read_to_string(&self.session_path) else {
            return false;
        };

        // Check if already included
        let include_stmt = format!("include {}", module);
        if content.contains(&include_stmt) {
            self.status_message = Some(format!("'{}' is already included.", module));
            return false;
        }

        // Find the includes marker
        let Some(includes_pos) = content.find(INCLUDES_MARKER) else {
            return false;
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
        let Ok(original) = fs::read_to_string(&self.session_path) else {
            return;
        };

        if !self.add_include(module) {
            return;
        }

        // Try to compile
        match self.compile_session() {
            Ok(output_path) => {
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
            ":version" | ":v" => {
                let version = env!("CARGO_PKG_VERSION");
                self.repl_state
                    .add_entry(HistoryEntry::new(cmd).with_output(format!("seqr {version}")));
                self.status_message = Some(format!("seqr {}", version));
            }
            ":clear" => {
                self.clear_session();
                self.repl_state.add_entry(HistoryEntry::new(":clear"));
                self.status_message = Some("Session cleared.".to_string());
            }
            ":pop" => {
                if self.pop_last_expression() {
                    // Add :pop to history
                    self.repl_state.add_entry(HistoryEntry::new(":pop"));
                    // Show new stack state in IR pane (informational, not in history)
                    self.show_stack_in_ir_pane();
                    self.status_message = Some("Popped last expression.".to_string());
                } else {
                    self.status_message = Some("Nothing to pop.".to_string());
                }
            }
            ":stack" | ":s" => {
                // Show current stack state
                self.compile_and_show_stack(":stack");
            }
            ":show" => {
                // Show session file contents in IR pane
                self.repl_state.add_entry(HistoryEntry::new(":show"));
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
                self.repl_state.add_entry(HistoryEntry::new(":ir"));
                self.show_ir_pane = !self.show_ir_pane;
                if self.show_ir_pane {
                    self.status_message =
                        Some(format!("IR: {} (Ctrl+N to cycle)", self.ir_mode.name()));
                } else {
                    self.status_message = Some("IR pane hidden".to_string());
                }
            }
            ":ir stack" => {
                self.repl_state.add_entry(HistoryEntry::new(":ir stack"));
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::StackArt;
                self.status_message = Some("IR: Stack Effects".to_string());
            }
            ":ir ast" => {
                self.repl_state.add_entry(HistoryEntry::new(":ir ast"));
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::TypedAst;
                self.status_message = Some("IR: Typed AST".to_string());
            }
            ":ir llvm" => {
                self.repl_state.add_entry(HistoryEntry::new(":ir llvm"));
                self.show_ir_pane = true;
                self.ir_mode = IrViewMode::LlvmIr;
                self.status_message = Some("IR: LLVM IR".to_string());
            }
            ":edit" | ":e" => {
                // Signal that we need to open editor (handled by run loop)
                self.repl_state.add_entry(HistoryEntry::new(cmd));
                self.should_edit = true;
            }
            ":help" | ":h" => {
                // Show help in the IR pane
                self.repl_state.add_entry(HistoryEntry::new(cmd));
                self.ir_content = IrContent {
                    stack_art: HELP_LINES.iter().map(|s| s.to_string()).collect(),
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
                    self.repl_state.add_entry(HistoryEntry::new(cmd));
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

    /// Compile session and show current stack (used by :stack command)
    /// The command parameter is the actual command string (e.g., ":stack") for history
    fn compile_and_show_stack(&mut self, command: &str) {
        match self.compile_and_run() {
            Ok(RunResult::Success { stdout }) => {
                let output_text = stdout.trim();
                // Add command to history with stack output
                if !output_text.is_empty() {
                    self.repl_state
                        .add_entry(HistoryEntry::new(command).with_output(output_text));
                } else {
                    self.repl_state
                        .add_entry(HistoryEntry::new(command).with_output("(empty)"));
                }
            }
            Ok(RunResult::Timeout { timeout_secs }) => {
                self.status_message = Some(format!(
                    "Timeout after {}s while showing stack",
                    timeout_secs
                ));
            }
            Ok(_) => {
                // Failed or Error - just ignore for stack display
            }
            Err(e) => {
                self.status_message = Some(format!("Compile error: {}", e));
            }
        }
    }

    /// Show current stack in IR pane without adding to history (for informational display)
    fn show_stack_in_ir_pane(&mut self) {
        // Only a successful compile + run updates the pane; everything else
        // (compile error, runtime failure, timeout) is silently ignored here.
        if let Ok(RunResult::Success { stdout }) = self.compile_and_run() {
            let output_text = stdout.trim();
            let mut lines = vec!["Stack:".to_string()];
            if !output_text.is_empty() {
                lines.extend(output_text.lines().map(String::from));
            } else {
                lines.push("(empty)".to_string());
            }
            self.ir_content = IrContent {
                stack_art: lines,
                typed_ast: vec![],
                llvm_ir: vec![],
                errors: vec![],
            };
            self.ir_mode = IrViewMode::StackArt;
            self.show_ir_pane = true;
        }
    }

    /// Update search matches based on current search pattern. Matches come
    /// from the shared `vim_line::history::Store` so the matching rules
    /// (case-insensitive substring, most-recent first) are identical to
    /// what every other vim-line consumer gets.
    fn update_search_matches(&mut self) {
        self.search.match_index = 0;
        self.search.matches = self.repl_state.store.search(&self.search.pattern);
    }

    /// Preview the current search match in the input line.
    fn preview_current_match(&mut self) {
        if self.search.matches.is_empty() {
            self.repl_state.input = self.search.original_input.clone();
        } else {
            let idx = self.search.matches[self.search.match_index];
            if let Some(entry) = self.repl_state.store.at(idx) {
                self.repl_state.input = entry.to_string();
            }
        }
        self.sync_editor_to_input();
    }

    /// Update status message to show search state
    fn update_search_status(&mut self) {
        if self.search.matches.is_empty() {
            if self.search.pattern.is_empty() {
                self.status_message = Some("/".to_string());
            } else {
                self.status_message = Some(format!("/{} (no matches)", self.search.pattern));
            }
        } else {
            let match_num = self.search.match_index + 1;
            let total = self.search.matches.len();
            self.status_message = Some(format!(
                "/{} ({}/{})",
                self.search.pattern, match_num, total
            ));
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
        self.sync_editor_to_input();
    }

    /// Navigate to next history entry (newer)
    fn navigate_history_next(&mut self) {
        self.repl_state.history_down();
        self.sync_editor_to_input();
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
        crate::ir::stack_effects::get_effect(word)
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

    /// Advance (or reverse) the Tab-cycle for the `:` command line. On the
    /// first Tab after a non-Tab keystroke, build the cycle from the
    /// current buffer; on subsequent Tabs, walk through matches. The
    /// "original prefix" is treated as one slot in the cycle so the user
    /// can cycle past the last match back to what they typed.
    fn cycle_command_completion(&mut self, reverse: bool) {
        if self.editor.command_line_buffer().is_none() {
            return;
        }

        if self.cmdline_cycle.is_none() {
            let prefix = self.editor.command_line_buffer().unwrap_or("").to_string();
            // Exact-prefix matches are excluded so Tab always moves to a
            // *different* command than what's already in the buffer. The
            // user can step back to their original prefix via the extra
            // "showing = None" slot below.
            let matches: Vec<usize> = COMMANDS
                .iter()
                .enumerate()
                .filter(|(_, c)| c.starts_with(&prefix) && **c != prefix)
                .map(|(i, _)| i)
                .collect();
            self.cmdline_cycle = Some(CmdlineCycle {
                prefix,
                matches,
                showing: None,
            });
        }

        let cycle = self.cmdline_cycle.as_mut().expect("just set");
        if cycle.matches.is_empty() {
            return;
        }

        // Slots: 0..matches.len() = each match, then one extra slot for
        // the original prefix. Reverse just walks the other way.
        let n = cycle.matches.len();
        let next = match (cycle.showing, reverse) {
            (None, false) => Some(0),
            (None, true) => {
                if n == 0 {
                    None
                } else {
                    Some(n - 1)
                }
            }
            (Some(i), false) => {
                if i + 1 >= n {
                    None
                } else {
                    Some(i + 1)
                }
            }
            (Some(0), true) => None,
            (Some(i), true) => Some(i - 1),
        };
        cycle.showing = next;

        let new_buf = match next {
            Some(i) => COMMANDS[cycle.matches[i]].to_string(),
            None => cycle.prefix.clone(),
        };
        let new_cursor = new_buf.len();
        self.editor.set_command_line(new_buf, new_cursor);
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
    pub(crate) fn render(&self, frame: &mut Frame) {
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
        // CommandLine mode owns the status bar while it's active so the user
        // can see (and edit) the `:` buffer they're typing into.
        if let Some(buf) = self.editor.command_line_buffer() {
            self.render_command_line(frame, area, buf);
            return;
        }

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

    /// Render the Ex-style `:` command line in the status bar, with a block
    /// cursor at `editor.command_line_cursor()`.
    fn render_command_line(&self, frame: &mut Frame, area: Rect, buf: &str) {
        let bg = Style::default().bg(Color::DarkGray).fg(Color::White);
        let cursor_style = Style::default().bg(Color::White).fg(Color::Black);

        let cursor = self.editor.command_line_cursor().min(buf.len());
        let (before, after) = buf.split_at(cursor);
        let cursor_char_len = after.chars().next().map_or(0, |c| c.len_utf8());
        let (under_cursor, rest) = after.split_at(cursor_char_len);
        let cursor_glyph = if under_cursor.is_empty() {
            " "
        } else {
            under_cursor
        };

        let mut spans = vec![
            Span::styled(":".to_string(), bg),
            Span::styled(before.to_string(), bg),
            Span::styled(cursor_glyph.to_string(), cursor_style),
        ];
        if !rest.is_empty() {
            spans.push(Span::styled(rest.to_string(), bg));
        }
        // Pad the remainder of the row so the dark-gray background spans
        // the full status width like the normal status line does.
        let used: usize =
            1 + before.chars().count() + cursor_glyph.chars().count() + rest.chars().count();
        let pad = (area.width as usize).saturating_sub(used);
        if pad > 0 {
            spans.push(Span::styled(" ".repeat(pad), bg));
        }

        let paragraph = Paragraph::new(Line::from(spans));
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

        // Clamp the popup to the actual frame bounds. On a tiny terminal
        // (e.g. 78x7) the natural popup height can extend past the visible
        // buffer, which makes ratatui's Clear panic with
        // "index outside of buffer". Skip the popup entirely if there's no
        // room left for even one item plus the border.
        let frame_area = frame.area();
        let max_w = frame_area.right().saturating_sub(x);
        let max_h = frame_area.bottom().saturating_sub(y);
        let popup_width = popup_width.min(max_w);
        let popup_height = popup_height.min(max_h);
        if popup_width < 4 || popup_height < 3 {
            return;
        }

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
mod tests;
