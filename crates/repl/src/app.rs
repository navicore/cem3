//! TUI Application
//!
//! Main application state and event loop using crossterm.
//! Integrates all widgets and handles Vi mode editing.
//!
//! Session file management is ported from the original REPL (crates/repl/src/main.rs).
//! Expressions accumulate in a temp file with `stack.dump` to show values.

use crate::engine::{AnalysisResult, analyze, analyze_expression};
use crate::ir::stack_art::{Stack, StackEffect, StackValue, render_transition};
use crate::lsp_client::LspClient;
use crate::ui::ir_pane::{IrContent, IrPane, IrViewMode};
use crate::ui::layout::{ComputedLayout, LayoutConfig, StatusContent};
use crate::ui::repl_pane::{HistoryEntry, ReplPane, ReplState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use lsp_types::CompletionItem;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use tempfile::NamedTempFile;

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

/// Vi editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViMode {
    /// Normal mode - navigation and commands
    #[default]
    Normal,
    /// Insert mode - text entry
    Insert,
}

impl ViMode {
    /// Get display name for status bar
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Insert => "INSERT",
        }
    }
}

/// Main application state
pub struct App {
    /// REPL state (history, input, cursor)
    pub repl_state: ReplState,
    /// IR content for visualization
    pub ir_content: IrContent,
    /// Current IR view mode
    pub ir_mode: IrViewMode,
    /// Current Vi mode
    pub vi_mode: ViMode,
    /// Layout configuration
    pub layout_config: LayoutConfig,
    /// Current filename (display name)
    pub filename: String,
    /// IR pane scroll offset
    pub ir_scroll: u16,
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
    /// LSP client for completions (None if unavailable)
    lsp_client: Option<LspClient>,
    /// Current completion items
    completions: Vec<CompletionItem>,
    /// Selected completion index
    completion_index: usize,
    /// Whether completion popup is visible
    show_completions: bool,
    /// Accumulated horizontal swipe delta (for gesture sensitivity)
    swipe_accumulator: i16,
}

// Note: App intentionally does not implement Default because App::new() can fail
// (temp file creation, file I/O). Use App::new() directly and handle the Result.

/// Maximum history entries to keep in memory
const MAX_HISTORY_IN_MEMORY: usize = 1000;

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

        // Try to start LSP client (like old REPL)
        let lsp_client = Self::try_start_lsp(&session_path, &initial_content);

        let mut app = Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            vi_mode: ViMode::default(),
            layout_config: LayoutConfig::default(),
            filename: "(scratch)".to_string(),
            ir_scroll: 0,
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path,
            _temp_file: Some(temp_file),
            lsp_client,
            completions: Vec::new(),
            completion_index: 0,
            show_completions: false,
            swipe_accumulator: 0,
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
            fs::read_to_string(&path).unwrap_or_default()
        };

        // Try to start LSP client
        let lsp_client = Self::try_start_lsp(&path, &content);

        let mut app = Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            vi_mode: ViMode::default(),
            layout_config: LayoutConfig::default(),
            filename,
            ir_scroll: 0,
            show_ir_pane: false,
            should_quit: false,
            should_edit: false,
            status_message: None,
            session_path: path,
            _temp_file: None,
            lsp_client,
            completions: Vec::new(),
            completion_index: 0,
            show_completions: false,
            swipe_accumulator: 0,
        };
        app.load_history();
        Ok(app)
    }

    /// Try to start the LSP client (like old REPL's SeqHelper::new)
    fn try_start_lsp(session_path: &Path, content: &str) -> Option<LspClient> {
        match LspClient::new(session_path) {
            Ok(mut client) => {
                // Open the document (like old REPL)
                if client.did_open(content).is_ok() {
                    Some(client)
                } else {
                    None
                }
            }
            Err(_) => None,
        }
    }

    /// Get the history file path (shared with original REPL)
    fn history_file_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("seqr_history"))
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
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }

            if let Ok(mut file) = fs::File::create(&path) {
                // Save the last 1000 entries
                let start = self.repl_state.history.len().saturating_sub(1000);
                for entry in &self.repl_state.history[start..] {
                    let _ = writeln!(file, "{}", entry.input);
                }
            }
        }
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Clear status message on any key
        self.status_message = None;

        // Handle completion popup navigation first
        if self.show_completions {
            match key.code {
                KeyCode::Esc => {
                    self.hide_completions();
                    return;
                }
                KeyCode::Up | KeyCode::Char('k') if self.vi_mode == ViMode::Normal => {
                    self.completion_up();
                    return;
                }
                KeyCode::Down | KeyCode::Char('j') if self.vi_mode == ViMode::Normal => {
                    self.completion_down();
                    return;
                }
                KeyCode::Up => {
                    self.completion_up();
                    return;
                }
                KeyCode::Down => {
                    self.completion_down();
                    return;
                }
                KeyCode::Tab => {
                    self.completion_down();
                    return;
                }
                KeyCode::Enter => {
                    self.accept_completion();
                    return;
                }
                _ => {
                    // Any other key hides completions and continues
                    self.hide_completions();
                }
            }
        }

        // Global shortcuts (work in any mode)
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('l') => {
                    // Clear screen / refresh
                    return;
                }
                _ => {}
            }
        }

        match self.vi_mode {
            ViMode::Normal => self.handle_normal_mode(key),
            ViMode::Insert => self.handle_insert_mode(key),
        }
    }

    /// Handle key in normal mode
    fn handle_normal_mode(&mut self, key: KeyEvent) {
        match key.code {
            // Mode switching
            KeyCode::Char('i') => {
                self.vi_mode = ViMode::Insert;
            }
            KeyCode::Char('a') => {
                self.vi_mode = ViMode::Insert;
                self.repl_state.cursor_right();
            }
            KeyCode::Char('A') => {
                self.vi_mode = ViMode::Insert;
                self.repl_state.cursor_end();
            }
            KeyCode::Char('I') => {
                self.vi_mode = ViMode::Insert;
                self.repl_state.cursor_home();
            }

            // Cursor movement (REPL always has focus)
            KeyCode::Char('h') | KeyCode::Left => {
                self.repl_state.cursor_left();
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.repl_state.cursor_right();
            }
            KeyCode::Char('0') | KeyCode::Home => {
                self.repl_state.cursor_home();
            }
            KeyCode::Char('$') | KeyCode::End => {
                self.repl_state.cursor_end();
            }

            // Word motion (simplified)
            KeyCode::Char('w') => {
                self.move_word_forward();
            }
            KeyCode::Char('b') => {
                self.move_word_backward();
            }

            // Deletion
            KeyCode::Char('x') => {
                self.repl_state.delete();
            }
            KeyCode::Char('d') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                // dd would need multi-key, for now just clear line
                self.repl_state.clear_input();
            }

            // History navigation (like rustyline in original REPL)
            KeyCode::Char('j') | KeyCode::Down => {
                self.repl_state.history_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.repl_state.history_up();
            }

            // Ctrl+N cycles IR view modes (when visible)
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.show_ir_pane {
                    self.ir_mode = self.ir_mode.next();
                    self.ir_scroll = 0; // Reset scroll when switching views
                }
            }

            // Tab triggers completion
            KeyCode::Tab => {
                self.request_completions();
            }

            // Execute current input
            KeyCode::Enter => {
                self.execute_input();
            }

            // Quit (q or Ctrl+D)
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Colon commands (simplified)
            KeyCode::Char(':') => {
                self.status_message = Some("Command mode not yet implemented".to_string());
            }

            _ => {}
        }
    }

    /// Handle key in insert mode
    fn handle_insert_mode(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.vi_mode = ViMode::Normal;
            }
            KeyCode::Enter => {
                self.execute_input();
                // Stay in insert mode after execution
            }
            KeyCode::Backspace => {
                self.repl_state.backspace();
                self.update_ir_preview();
            }
            KeyCode::Delete => {
                self.repl_state.delete();
                self.update_ir_preview();
            }
            KeyCode::Left => {
                self.repl_state.cursor_left();
            }
            KeyCode::Right => {
                self.repl_state.cursor_right();
            }
            KeyCode::Up => {
                self.repl_state.history_up();
            }
            KeyCode::Down => {
                self.repl_state.history_down();
            }
            KeyCode::Home => {
                self.repl_state.cursor_home();
            }
            KeyCode::End => {
                self.repl_state.cursor_end();
            }
            KeyCode::Tab => {
                // Trigger completion
                self.request_completions();
            }
            // Ctrl+D exits (like EOF in terminal)
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char(c) => {
                self.repl_state.insert_char(c);
                self.update_ir_preview();
            }
            _ => {}
        }
    }

    /// Move cursor forward by word (Unicode-safe)
    fn move_word_forward(&mut self) {
        let input = &self.repl_state.input;
        let chars: Vec<char> = input.chars().collect();
        let mut char_pos = self.byte_to_char_pos(input, self.repl_state.cursor);

        // Skip current word
        while char_pos < chars.len() && !chars[char_pos].is_whitespace() {
            char_pos += 1;
        }
        // Skip whitespace
        while char_pos < chars.len() && chars[char_pos].is_whitespace() {
            char_pos += 1;
        }

        self.repl_state.cursor = self.char_to_byte_pos(input, char_pos);
    }

    /// Move cursor backward by word (Unicode-safe)
    fn move_word_backward(&mut self) {
        let input = &self.repl_state.input;
        let chars: Vec<char> = input.chars().collect();
        let mut char_pos = self.byte_to_char_pos(input, self.repl_state.cursor);

        // Skip whitespace before cursor
        while char_pos > 0 && chars[char_pos - 1].is_whitespace() {
            char_pos -= 1;
        }
        // Skip word
        while char_pos > 0 && !chars[char_pos - 1].is_whitespace() {
            char_pos -= 1;
        }

        self.repl_state.cursor = self.char_to_byte_pos(input, char_pos);
    }

    /// Convert byte position to character position (Unicode-safe)
    fn byte_to_char_pos(&self, s: &str, byte_pos: usize) -> usize {
        s[..byte_pos.min(s.len())].chars().count()
    }

    /// Convert character position to byte position (Unicode-safe)
    fn char_to_byte_pos(&self, s: &str, char_pos: usize) -> usize {
        s.char_indices()
            .nth(char_pos)
            .map(|(i, _)| i)
            .unwrap_or(s.len())
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
                let _ = fs::write(&self.session_path, &original);
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
                // Run and capture output
                let output = Command::new(&output_path).output();

                let _ = fs::remove_file(&output_path);

                match output {
                    Ok(result) => {
                        let stdout = String::from_utf8_lossy(&result.stdout);
                        let stderr = String::from_utf8_lossy(&result.stderr);

                        // Update IR from the session file
                        self.update_ir_from_session(expr);

                        if result.status.success() {
                            let output_text = stdout.trim();
                            if output_text.is_empty() {
                                self.repl_state
                                    .add_entry(HistoryEntry::new(expr).with_output("ok"));
                            } else {
                                self.repl_state
                                    .add_entry(HistoryEntry::new(expr).with_output(output_text));
                            }
                        } else {
                            let err = if stderr.is_empty() {
                                format!("exit: {:?}", result.status.code())
                            } else {
                                stderr.trim().to_string()
                            };
                            self.repl_state
                                .add_entry(HistoryEntry::new(expr).with_error(&err));
                        }
                    }
                    Err(e) => {
                        self.add_error_entry(expr, &format!("Run error: {}", e));
                    }
                }
                self.repl_state.clear_input();
            }
            Err(e) => {
                // Rollback
                let _ = fs::write(&self.session_path, &original);
                self.add_error_entry(expr, &e.to_string());
            }
        }
    }

    /// Append an expression to main (before stack.dump)
    fn append_expression(&mut self, expr: &str) -> bool {
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
        let _ = fs::write(
            &self.session_path,
            format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE),
        );
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
                let _ = fs::write(&self.session_path, &original);
                self.status_message = Some(format!("Include error: {}", e));
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
                    self.compile_and_show_stack();
                    self.status_message = Some("Popped last expression.".to_string());
                } else {
                    self.status_message = Some("Nothing to pop.".to_string());
                }
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
    fn compile_and_show_stack(&mut self) {
        let output_path = self.session_path.with_extension("");
        match seqc::compile_file(&self.session_path, &output_path, false) {
            Ok(_) => {
                let output = Command::new(&output_path).output();
                let _ = fs::remove_file(&output_path);

                if let Ok(result) = output
                    && result.status.success()
                {
                    let stdout = String::from_utf8_lossy(&result.stdout);
                    let output_text = stdout.trim();
                    if !output_text.is_empty() {
                        // Add a "stack state" entry to show current stack
                        self.repl_state
                            .add_entry(HistoryEntry::new("(after pop)").with_output(output_text));
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
        // Reset scroll when content changes
        self.ir_scroll = 0;
    }

    /// Scroll the IR pane by delta lines (positive = down, negative = up)
    pub fn scroll_ir(&mut self, delta: i16) {
        if !self.show_ir_pane {
            return;
        }

        let content_len = self.ir_content_len();
        if content_len == 0 {
            return;
        }

        let new_scroll = if delta < 0 {
            self.ir_scroll.saturating_sub((-delta) as u16)
        } else {
            self.ir_scroll.saturating_add(delta as u16)
        };

        // Clamp to content length (leave some visible at the end)
        self.ir_scroll = new_scroll.min(content_len.saturating_sub(1) as u16);
    }

    /// Get the current IR content length (number of lines)
    fn ir_content_len(&self) -> usize {
        self.ir_content.content_for(self.ir_mode).len()
    }

    /// Swipe gesture threshold (accumulate this many events before triggering)
    const SWIPE_THRESHOLD: i16 = 10;

    /// Handle swipe right gesture: open IR pane or cycle to next view
    /// Flow: (closed) → Stack Effects → Typed AST → LLVM IR (stop)
    pub fn swipe_right(&mut self) {
        self.swipe_accumulator += 1;
        if self.swipe_accumulator < Self::SWIPE_THRESHOLD {
            return;
        }
        self.swipe_accumulator = 0;

        if !self.show_ir_pane {
            // Open IR pane (starts at Stack Effects)
            self.show_ir_pane = true;
            self.ir_mode = IrViewMode::StackArt;
            self.ir_scroll = 0;
        } else if self.ir_mode != IrViewMode::LlvmIr {
            // Cycle to next view (but stop at LLVM IR)
            self.ir_mode = self.ir_mode.next();
            self.ir_scroll = 0;
        }
        // If already on LLVM IR, do nothing
    }

    /// Handle swipe left gesture: cycle to previous view or close IR pane
    /// Flow: LLVM IR → Typed AST → Stack Effects → (closed)
    pub fn swipe_left(&mut self) {
        self.swipe_accumulator -= 1;
        if self.swipe_accumulator > -Self::SWIPE_THRESHOLD {
            return;
        }
        self.swipe_accumulator = 0;

        if !self.show_ir_pane {
            return;
        }

        if self.ir_mode == IrViewMode::StackArt {
            // Close IR pane
            self.show_ir_pane = false;
        } else {
            // Cycle to previous view
            self.ir_mode = self.ir_mode.prev();
            self.ir_scroll = 0;
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

        // Reset scroll for fresh content
        self.ir_scroll = 0;
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
            return Some(StackEffect::new(
                word,
                Stack::with_rest("a"),
                Stack::with_rest("a").push(StackValue::val(word.to_string())),
            ));
        }
        if word.parse::<f64>().is_ok() && word.contains('.') {
            return Some(StackEffect::new(
                word,
                Stack::with_rest("a"),
                Stack::with_rest("a").push(StackValue::val(word.to_string())),
            ));
        }
        if word == "true" || word == "false" {
            return Some(StackEffect::new(
                word,
                Stack::with_rest("a"),
                Stack::with_rest("a").push(StackValue::val(word.to_string())),
            ));
        }

        // Builtins
        match word {
            // Stack manipulation
            "dup" => Some(StackEffect::new(
                "dup",
                Stack::with_rest("a").push(StackValue::var("x")),
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("x")),
            )),
            "drop" => Some(StackEffect::new(
                "drop",
                Stack::with_rest("a").push(StackValue::var("x")),
                Stack::with_rest("a"),
            )),
            "swap" => Some(StackEffect::new(
                "swap",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a")
                    .push(StackValue::var("y"))
                    .push(StackValue::var("x")),
            )),
            "over" => Some(StackEffect::new(
                "over",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y"))
                    .push(StackValue::var("x")),
            )),
            "rot" => Some(StackEffect::new(
                "rot",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y"))
                    .push(StackValue::var("z")),
                Stack::with_rest("a")
                    .push(StackValue::var("y"))
                    .push(StackValue::var("z"))
                    .push(StackValue::var("x")),
            )),
            "nip" => Some(StackEffect::new(
                "nip",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a").push(StackValue::var("y")),
            )),
            "tuck" => Some(StackEffect::new(
                "tuck",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a")
                    .push(StackValue::var("y"))
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
            )),

            // Integer Arithmetic
            "i.add" => Some(StackEffect::new(
                "i.add",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "i.subtract" => Some(StackEffect::new(
                "i.subtract",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "i.multiply" => Some(StackEffect::new(
                "i.multiply",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "i.divide" => Some(StackEffect::new(
                "i.divide",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "modulo" => Some(StackEffect::new(
                "modulo",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "negate" => Some(StackEffect::new(
                "negate",
                Stack::with_rest("a").push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),

            // Comparison
            "equals" | "not-equals" | "less-than" | "greater-than" | "less-or-equal"
            | "greater-or-equal" => Some(StackEffect::new(
                word,
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("x")),
                Stack::with_rest("a").push(StackValue::ty("Bool")),
            )),

            // Logic
            "and" | "or" => Some(StackEffect::new(
                word,
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "not" => Some(StackEffect::new(
                "not",
                Stack::with_rest("a").push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),

            // Quotation combinators
            "apply" => Some(StackEffect::new(
                "apply",
                Stack::with_rest("a").push(StackValue::ty("Quot")),
                Stack::with_rest("b"),
            )),
            "dip" => Some(StackEffect::new(
                "dip",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::ty("Quot")),
                Stack::with_rest("b").push(StackValue::var("x")),
            )),

            _ => None,
        }
    }

    /// Request completions from LSP (mirrors old REPL's SeqHelper::complete)
    fn request_completions(&mut self) {
        let line = self.repl_state.input.clone();
        let pos = self.repl_state.cursor;

        // Find word start for replacement
        let word_start = line[..pos]
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        // Get the prefix the user has typed
        let prefix = &line[word_start..pos];

        // Don't show completions for empty prefix - too noisy
        if prefix.is_empty() {
            self.status_message = Some("Tab: type a prefix first".to_string());
            return;
        }

        // If no LSP client, fall back to built-in completions
        let Some(ref mut lsp) = self.lsp_client else {
            self.builtin_completions(prefix);
            return;
        };

        // Get file content
        let Ok(file_content) = fs::read_to_string(&self.session_path) else {
            self.builtin_completions(prefix);
            return;
        };

        // Find where to insert (before stack.dump) - like old REPL
        let Some(insert_pos) = file_content.find("  stack.dump") else {
            self.builtin_completions(prefix);
            return;
        };

        // Create virtual document: file content + current line at end of main
        let virtual_content = format!(
            "{}  {}\n{}",
            &file_content[..insert_pos],
            line,
            &file_content[insert_pos..]
        );

        // Calculate line/column in virtual document
        let lines_before: u32 = file_content[..insert_pos].matches('\n').count() as u32;
        let line_num = lines_before; // 0-indexed
        let col_num = pos as u32 + 2; // +2 for the "  " indent

        // Sync virtual document and get completions
        if lsp.did_change(&virtual_content).is_err() {
            self.builtin_completions(prefix);
            return;
        }

        let items = match lsp.completions(line_num, col_num) {
            Ok(items) => items,
            Err(_) => {
                // Restore original and fall back
                let _ = lsp.did_change(&file_content);
                self.builtin_completions(prefix);
                return;
            }
        };

        // Restore original document
        let _ = lsp.did_change(&file_content);

        // Filter by prefix (case-insensitive)
        let prefix_lower = prefix.to_lowercase();
        self.completions = items
            .into_iter()
            .filter(|item| item.label.to_lowercase().starts_with(&prefix_lower))
            .take(10)
            .collect();

        if !self.completions.is_empty() {
            self.completion_index = 0;
            self.show_completions = true;
        } else {
            self.status_message = Some("No completions".to_string());
        }
    }

    /// Provide built-in completions when LSP is not available
    fn builtin_completions(&mut self, prefix: &str) {
        let builtins = [
            "dup",
            "drop",
            "swap",
            "over",
            "rot",
            "nip",
            "tuck",
            "i.add",
            "i.subtract",
            "i.multiply",
            "i.divide",
            "modulo",
            "negate",
            "equals",
            "not-equals",
            "less-than",
            "greater-than",
            "less-or-equal",
            "greater-or-equal",
            "and",
            "or",
            "not",
            "apply",
            "dip",
            "if",
            "when",
            "unless",
            "while",
            "times",
            "true",
            "false",
            "stack.dump",
            "print",
            "println",
        ];

        self.completions = builtins
            .iter()
            .filter(|b| b.starts_with(prefix) && **b != prefix)
            .take(10)
            .map(|s| CompletionItem {
                label: s.to_string(),
                ..Default::default()
            })
            .collect();

        if !self.completions.is_empty() {
            self.completion_index = 0;
            self.show_completions = true;
        } else if !prefix.is_empty() {
            self.status_message = Some("No completions".to_string());
        }
    }

    /// Move up in completion list
    fn completion_up(&mut self) {
        if !self.completions.is_empty() {
            if self.completion_index > 0 {
                self.completion_index -= 1;
            } else {
                self.completion_index = self.completions.len() - 1;
            }
        }
    }

    /// Move down in completion list
    fn completion_down(&mut self) {
        if !self.completions.is_empty() {
            self.completion_index = (self.completion_index + 1) % self.completions.len();
        }
    }

    /// Accept the current completion
    fn accept_completion(&mut self) {
        if let Some(item) = self.completions.get(self.completion_index) {
            let input = &self.repl_state.input;
            let cursor = self.repl_state.cursor;

            // Find start of current word
            let word_start = input[..cursor]
                .rfind(|c: char| c.is_whitespace())
                .map(|i| i + 1)
                .unwrap_or(0);

            // Replace current word with completion
            let completion = &item.label;
            let before = &input[..word_start];
            let after = &input[cursor..];

            self.repl_state.input = format!("{}{}{}", before, completion, after);
            self.repl_state.cursor = word_start + completion.len();

            self.hide_completions();
            self.update_ir_preview();
        }
    }

    /// Hide completion popup
    fn hide_completions(&mut self) {
        self.show_completions = false;
        self.completions.clear();
        self.completion_index = 0;
    }

    /// Render the application to a frame
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let layout = ComputedLayout::compute(area, &self.layout_config, self.show_ir_pane);

        // Render REPL pane (always focused, no border)
        let repl_pane = ReplPane::new(&self.repl_state)
            .focused(self.vi_mode == ViMode::Insert)
            .prompt(if self.vi_mode == ViMode::Insert {
                "seq> "
            } else {
                "seq: "
            });
        frame.render_widget(&repl_pane, layout.repl);

        // Render IR pane with scrollbar (if enabled and space available)
        if self.show_ir_pane && layout.ir_visible() {
            let ir_pane = IrPane::new(&self.ir_content)
                .mode(self.ir_mode)
                .scroll(self.ir_scroll);
            frame.render_widget(&ir_pane, layout.ir);

            // Render scrollbar if content is scrollable
            let content_len = self.ir_content_len();
            let viewport_height = layout.ir.height.saturating_sub(2) as usize; // account for borders
            if content_len > viewport_height {
                let mut scrollbar_state = ScrollbarState::new(content_len)
                    .position(self.ir_scroll as usize)
                    .viewport_content_length(viewport_height);

                // Render scrollbar inside the IR pane area (on the right edge)
                frame.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .begin_symbol(Some("↑"))
                        .end_symbol(Some("↓")),
                    layout.ir,
                    &mut scrollbar_state,
                );
            }
        }

        // Render status bar
        self.render_status_bar(frame, layout.status);

        // Render completion popup (on top of everything)
        if self.show_completions && !self.completions.is_empty() {
            self.render_completions(frame, layout.repl);
        }
    }

    /// Render the status bar
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status = StatusContent::new()
            .filename(&self.filename)
            .mode(self.vi_mode.name())
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
        // Calculate popup position (above the input line)
        let popup_height = (self.completions.len() + 2) as u16; // +2 for border
        let popup_width = self
            .completions
            .iter()
            .map(|c| c.label.len())
            .max()
            .unwrap_or(10) as u16
            + 4; // +4 for padding and border

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
        let lines: Vec<Line> = self
            .completions
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.completion_index {
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
    fn test_vi_mode_names() {
        assert_eq!(ViMode::Normal.name(), "NORMAL");
        assert_eq!(ViMode::Insert.name(), "INSERT");
    }

    #[test]
    fn test_app_creation() -> Result<(), String> {
        let app = App::new()?;
        assert_eq!(app.vi_mode, ViMode::Normal);
        assert!(!app.should_quit);
        Ok(())
    }

    #[test]
    fn test_mode_switching() -> Result<(), String> {
        let mut app = App::new()?;

        // i enters insert mode
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.vi_mode, ViMode::Insert);

        // Esc returns to normal
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.vi_mode, ViMode::Normal);
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
        app.repl_state.input = "hello".to_string();
        app.repl_state.cursor = 2;

        // h moves left
        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        assert_eq!(app.repl_state.cursor, 1);

        // l moves right
        app.handle_key(KeyEvent::from(KeyCode::Char('l')));
        assert_eq!(app.repl_state.cursor, 2);

        // 0 goes to start
        app.handle_key(KeyEvent::from(KeyCode::Char('0')));
        assert_eq!(app.repl_state.cursor, 0);

        // $ goes to end
        app.handle_key(KeyEvent::from(KeyCode::Char('$')));
        assert_eq!(app.repl_state.cursor, 5);
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

        // k goes up in history (to most recent)
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "second");

        // k again goes to older entry
        app.handle_key(KeyEvent::from(KeyCode::Char('k')));
        assert_eq!(app.repl_state.input, "first");

        // j goes back down
        app.handle_key(KeyEvent::from(KeyCode::Char('j')));
        assert_eq!(app.repl_state.input, "second");
        Ok(())
    }

    #[test]
    fn test_quit_command() -> Result<(), String> {
        let mut app = App::new()?;
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
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
        assert!(app.get_word_effect("dup").is_some());
        assert!(app.get_word_effect("swap").is_some());
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
}
