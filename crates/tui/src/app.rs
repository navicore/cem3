//! TUI Application
//!
//! Main application state and event loop using crossterm.
//! Integrates all widgets and handles Vi mode editing.

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
    /// Current filename
    pub filename: String,
    /// IR pane scroll offset
    pub ir_scroll: u16,
    /// Whether the app should quit
    pub should_quit: bool,
    /// Status message (clears after next action)
    pub status_message: Option<String>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Create a new application
    pub fn new() -> Self {
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
        }
    }

    /// Set the filename
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

        // Handle REPL commands
        if input.starts_with(':') {
            let cmd = input.clone();
            self.handle_command(&cmd);
            return;
        }

        // Wrap in a minimal program for analysis
        let source = format!(
            ": __repl__ ( -- )\n    {}\n;\n: main ( -- ) __repl__ ;",
            input
        );

        let result = analyze(&source);

        let entry = if result.errors.is_empty() {
            // Update IR content - clone input to avoid borrow issues
            let input_clone = input.clone();
            self.update_ir_from_result(&result, &input_clone);
            HistoryEntry::new(&input).with_output("ok")
        } else {
            let error = result.errors.join("\n");
            HistoryEntry::new(&input).with_error(&error)
        };

        self.repl_state.add_entry(entry);
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
                self.repl_state = ReplState::new();
                self.ir_content = IrContent::new();
            }
            ":help" | ":h" => {
                self.status_message = Some(
                    "Commands: :q(uit), :clear, :help | Vi: i/a/A insert, Esc normal, Tab focus"
                        .to_string(),
                );
            }
            _ => {
                self.status_message = Some(format!("Unknown command: {}", cmd));
            }
        }
        self.repl_state.clear_input();
    }

    /// Update IR preview as user types
    fn update_ir_preview(&mut self) {
        let input = self.repl_state.current_input().to_string();
        if input.trim().is_empty() {
            self.ir_content = IrContent::new();
            return;
        }

        // Try to parse and show stack effect
        let source = format!(
            ": __repl__ ( -- )\n    {}\n;\n: main ( -- ) __repl__ ;",
            input
        );

        let result = analyze(&source);

        if result.errors.is_empty() {
            self.update_ir_from_result(&result, &input);
        } else {
            // Show parse errors in IR pane
            self.ir_content = IrContent::with_error(&result.errors[0]);
        }
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

    /// Get the stack effect for a word
    fn get_word_effect(&self, word: &str) -> Option<StackEffect> {
        // Common builtins
        match word {
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
            "add" => Some(StackEffect::new(
                "add",
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
