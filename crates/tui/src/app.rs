//! TUI Application
//!
//! Main application state and event loop using crossterm.
//! Integrates all widgets and handles Vi mode editing.
//!
//! Session file management is ported from the original REPL (crates/repl/src/main.rs).
//! Expressions accumulate in a temp file with `stack.dump` to show values.

use crate::engine::{AnalysisResult, analyze};
use crate::ir::stack_art::{Stack, StackEffect, StackValue, render_transition};
use crate::ui::ir_pane::{IrContent, IrPane, IrViewMode};
use crate::ui::layout::{ComputedLayout, FocusedPane, LayoutConfig, StatusContent};
use crate::ui::repl_pane::{HistoryEntry, ReplPane, ReplState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::fs;
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
    /// Which pane is focused
    pub focused: FocusedPane,
    /// Layout configuration
    pub layout_config: LayoutConfig,
    /// Current filename (display name)
    pub filename: String,
    /// IR pane scroll offset
    pub ir_scroll: u16,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Status message (clears after next action)
    pub status_message: Option<String>,
    /// Session file path (temp file or user-provided file)
    pub session_path: PathBuf,
    /// Temp file handle (kept alive to prevent deletion)
    _temp_file: Option<NamedTempFile>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a new application with a temp session file
    pub fn new() -> Self {
        // Create temp file for session
        let temp_file = NamedTempFile::with_suffix(".seq").expect("Failed to create temp file");
        let session_path = temp_file.path().to_path_buf();

        // Initialize with template
        fs::write(&session_path, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE))
            .expect("Failed to write session file");

        Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            vi_mode: ViMode::default(),
            focused: FocusedPane::default(),
            layout_config: LayoutConfig::default(),
            filename: "(scratch)".to_string(),
            ir_scroll: 0,
            should_quit: false,
            status_message: None,
            session_path,
            _temp_file: Some(temp_file),
        }
    }

    /// Create application with an existing file
    pub fn with_file(path: PathBuf) -> Self {
        let filename = path.display().to_string();

        // Check if file exists, create if not
        if !path.exists() {
            fs::write(&path, format!("{}{}", REPL_TEMPLATE, MAIN_CLOSE))
                .expect("Failed to create session file");
        }

        Self {
            repl_state: ReplState::new(),
            ir_content: IrContent::new(),
            ir_mode: IrViewMode::default(),
            vi_mode: ViMode::default(),
            focused: FocusedPane::default(),
            layout_config: LayoutConfig::default(),
            filename,
            ir_scroll: 0,
            should_quit: false,
            status_message: None,
            session_path: path,
            _temp_file: None,
        }
    }

    /// Set the display filename (legacy method for compatibility)
    pub fn with_filename(mut self, name: impl Into<String>) -> Self {
        self.filename = name.into();
        self
    }

    /// Handle a key event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Clear status message on any key
        self.status_message = None;

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

            // Cursor movement
            KeyCode::Char('h') | KeyCode::Left => {
                if self.focused == FocusedPane::Repl {
                    self.repl_state.cursor_left();
                } else {
                    self.ir_mode = self.ir_mode.prev();
                }
            }
            KeyCode::Char('l') | KeyCode::Right => {
                if self.focused == FocusedPane::Repl {
                    self.repl_state.cursor_right();
                } else {
                    self.ir_mode = self.ir_mode.next();
                }
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
            KeyCode::Char('d') => {
                // dd would need multi-key, for now just clear line
                self.repl_state.clear_input();
            }

            // Scrolling IR pane
            KeyCode::Char('j') | KeyCode::Down => {
                if self.focused == FocusedPane::Ir {
                    self.ir_scroll = self.ir_scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.focused == FocusedPane::Ir {
                    self.ir_scroll = self.ir_scroll.saturating_sub(1);
                }
            }

            // Focus switching
            KeyCode::Tab => {
                self.focused = self.focused.toggle();
            }

            // Execute current input
            KeyCode::Enter => {
                self.execute_input();
            }

            // Quit
            KeyCode::Char('q') => {
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
            KeyCode::Home => {
                self.repl_state.cursor_home();
            }
            KeyCode::End => {
                self.repl_state.cursor_end();
            }
            KeyCode::Tab => {
                // Could trigger completion here
                self.repl_state.insert_char(' ');
                self.repl_state.insert_char(' ');
                self.repl_state.insert_char(' ');
                self.repl_state.insert_char(' ');
            }
            KeyCode::Char(c) => {
                self.repl_state.insert_char(c);
                self.update_ir_preview();
            }
            _ => {}
        }
    }

    /// Move cursor forward by word
    fn move_word_forward(&mut self) {
        let input = &self.repl_state.input;
        let mut pos = self.repl_state.cursor;

        // Skip current word
        while pos < input.len() && !input[pos..].starts_with(' ') {
            pos += 1;
        }
        // Skip whitespace
        while pos < input.len() && input[pos..].starts_with(' ') {
            pos += 1;
        }

        self.repl_state.cursor = pos;
    }

    /// Move cursor backward by word
    fn move_word_backward(&mut self) {
        let input = &self.repl_state.input;
        let mut pos = self.repl_state.cursor;

        // Skip whitespace before cursor
        while pos > 0 && input[..pos].ends_with(' ') {
            pos -= 1;
        }
        // Skip word
        while pos > 0 && !input[..pos].ends_with(' ') {
            pos -= 1;
        }

        self.repl_state.cursor = pos;
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
            ":help" | ":h" => {
                self.status_message = Some(
                    ":q :clear :pop :show :include <mod> | Vi: i/a insert, Esc normal, Tab focus"
                        .to_string(),
                );
            }
            _ if cmd.starts_with(":include ") => {
                let module = cmd.strip_prefix(":include ").unwrap().trim();
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
    }

    /// Update IR content from analysis result
    fn update_ir_from_result(&mut self, result: &AnalysisResult, input: &str) {
        // Generate stack art for the expression
        let stack_art = self.generate_stack_art(input);

        // Typed AST placeholder
        let typed_ast = vec![
            format!("Expression: {}", input),
            String::new(),
            "Types inferred successfully".to_string(),
        ];

        // LLVM IR
        let llvm_ir = if let Some(ir) = &result.llvm_ir {
            // Extract just the __repl__ function
            self.extract_repl_ir(ir)
        } else {
            vec![]
        };

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

            // Arithmetic
            "add" => Some(StackEffect::new(
                "add",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "subtract" => Some(StackEffect::new(
                "subtract",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "multiply" => Some(StackEffect::new(
                "multiply",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            )),
            "divide" => Some(StackEffect::new(
                "divide",
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

    /// Extract __repl__ function from LLVM IR
    fn extract_repl_ir(&self, ir: &str) -> Vec<String> {
        let mut lines = Vec::new();
        let mut in_repl = false;

        for line in ir.lines() {
            if line.contains("define") && line.contains("__repl__") {
                in_repl = true;
            }
            if in_repl {
                lines.push(line.to_string());
                if line.trim() == "}" {
                    break;
                }
            }
        }

        if lines.is_empty() {
            vec!["(LLVM IR not available)".to_string()]
        } else {
            lines
        }
    }

    /// Render the application to a frame
    pub fn render(&self, frame: &mut Frame) {
        let area = frame.area();
        let layout = ComputedLayout::compute(area, &self.layout_config);

        // Render REPL pane
        let repl_pane = ReplPane::new(&self.repl_state)
            .focused(self.focused == FocusedPane::Repl && self.vi_mode == ViMode::Insert)
            .prompt(if self.vi_mode == ViMode::Insert {
                "seq> "
            } else {
                "seq: "
            });
        frame.render_widget(&repl_pane, layout.repl);

        // Render IR pane (if visible)
        if layout.ir_visible() {
            let ir_pane = IrPane::new(&self.ir_content)
                .mode(self.ir_mode)
                .focused(self.focused == FocusedPane::Ir)
                .scroll(self.ir_scroll);
            frame.render_widget(&ir_pane, layout.ir);
        }

        // Render status bar
        self.render_status_bar(frame, layout.status);
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
    fn test_app_creation() {
        let app = App::new();
        assert_eq!(app.vi_mode, ViMode::Normal);
        assert_eq!(app.focused, FocusedPane::Repl);
        assert!(!app.should_quit);
    }

    #[test]
    fn test_mode_switching() {
        let mut app = App::new();

        // i enters insert mode
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        assert_eq!(app.vi_mode, ViMode::Insert);

        // Esc returns to normal
        app.handle_key(KeyEvent::from(KeyCode::Esc));
        assert_eq!(app.vi_mode, ViMode::Normal);
    }

    #[test]
    fn test_insert_mode_typing() {
        let mut app = App::new();
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));

        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));

        assert_eq!(app.repl_state.input, "hi");
    }

    #[test]
    fn test_normal_mode_navigation() {
        let mut app = App::new();
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
    }

    #[test]
    fn test_focus_toggle() {
        let mut app = App::new();
        assert_eq!(app.focused, FocusedPane::Repl);

        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.focused, FocusedPane::Ir);

        app.handle_key(KeyEvent::from(KeyCode::Tab));
        assert_eq!(app.focused, FocusedPane::Repl);
    }

    #[test]
    fn test_ir_view_cycling() {
        let mut app = App::new();
        app.focused = FocusedPane::Ir;

        assert_eq!(app.ir_mode, IrViewMode::StackArt);

        app.handle_key(KeyEvent::from(KeyCode::Char('l')));
        assert_eq!(app.ir_mode, IrViewMode::TypedAst);

        app.handle_key(KeyEvent::from(KeyCode::Char('l')));
        assert_eq!(app.ir_mode, IrViewMode::LlvmIr);

        app.handle_key(KeyEvent::from(KeyCode::Char('h')));
        assert_eq!(app.ir_mode, IrViewMode::TypedAst);
    }

    #[test]
    fn test_quit_command() {
        let mut app = App::new();
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        assert!(app.should_quit);
    }

    #[test]
    fn test_repl_command() {
        let mut app = App::new();
        app.handle_key(KeyEvent::from(KeyCode::Char('i')));
        app.handle_key(KeyEvent::from(KeyCode::Char(':')));
        app.handle_key(KeyEvent::from(KeyCode::Char('q')));
        app.handle_key(KeyEvent::from(KeyCode::Enter));
        assert!(app.should_quit);
    }

    #[test]
    fn test_word_effect_lookup() {
        let app = App::new();
        assert!(app.get_word_effect("dup").is_some());
        assert!(app.get_word_effect("swap").is_some());
        assert!(app.get_word_effect("unknown").is_none());
    }

    #[test]
    fn test_ctrl_c_quits() {
        let mut app = App::new();
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        app.handle_key(key);
        assert!(app.should_quit);
    }
}
