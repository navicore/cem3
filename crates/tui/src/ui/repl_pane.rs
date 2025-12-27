//! REPL Pane Widget
//!
//! Displays the REPL interface with:
//! - Command history with syntax highlighting
//! - Current input line with cursor
//! - Output/result display

use crate::ui::highlight::{TokenKind, tokenize};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// A single entry in the REPL history
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// The input that was entered
    pub input: String,
    /// The output/result (if any)
    pub output: Option<String>,
    /// Whether this entry had an error
    pub is_error: bool,
}

impl HistoryEntry {
    /// Create a new history entry
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            output: None,
            is_error: false,
        }
    }

    /// Set the output
    pub fn with_output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    /// Mark as an error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.output = Some(error.into());
        self.is_error = true;
        self
    }
}

/// The REPL pane state
#[derive(Debug, Clone, Default)]
pub struct ReplState {
    /// Command history
    pub history: Vec<HistoryEntry>,
    /// Current input line
    pub input: String,
    /// Cursor position in the input
    pub cursor: usize,
    /// Scroll offset for history
    pub scroll: u16,
}

impl ReplState {
    /// Create a new REPL state
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a history entry
    pub fn add_entry(&mut self, entry: HistoryEntry) {
        self.history.push(entry);
    }

    /// Clear the current input
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    /// Insert a character at the cursor
    pub fn insert_char(&mut self, ch: char) {
        self.input.insert(self.cursor, ch);
        self.cursor += 1;
    }

    /// Delete the character before the cursor
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
        }
    }

    /// Delete the character at the cursor
    pub fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Get the current input
    pub fn current_input(&self) -> &str {
        &self.input
    }
}

/// The REPL pane widget
pub struct ReplPane<'a> {
    /// The REPL state
    state: &'a ReplState,
    /// Whether this pane is focused
    focused: bool,
    /// The prompt string
    prompt: &'a str,
}

impl<'a> ReplPane<'a> {
    /// Create a new REPL pane
    pub fn new(state: &'a ReplState) -> Self {
        Self {
            state,
            focused: true,
            prompt: "seq> ",
        }
    }

    /// Set whether the pane is focused
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set the prompt string
    pub fn prompt(mut self, prompt: &'a str) -> Self {
        self.prompt = prompt;
        self
    }

    /// Highlight a line of Seq code
    fn highlight_code(&self, code: &str) -> Line<'a> {
        let tokens = tokenize(code);
        let spans: Vec<Span> = tokens
            .into_iter()
            .map(|token| {
                let style = match token.kind {
                    TokenKind::Keyword => Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                    TokenKind::Builtin => Style::default().fg(Color::Cyan),
                    TokenKind::DefMarker | TokenKind::DefEnd => Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    TokenKind::WordName => Style::default().fg(Color::White),
                    TokenKind::Integer | TokenKind::Float => Style::default().fg(Color::Blue),
                    TokenKind::Boolean => Style::default().fg(Color::Magenta),
                    TokenKind::String => Style::default().fg(Color::Green),
                    TokenKind::Comment => Style::default().fg(Color::DarkGray),
                    TokenKind::TypeName => Style::default().fg(Color::Green),
                    TokenKind::StackEffect => Style::default().fg(Color::DarkGray),
                    TokenKind::Quotation => Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    TokenKind::Include => Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD),
                    TokenKind::ModulePath => Style::default().fg(Color::Cyan),
                    TokenKind::Identifier => Style::default().fg(Color::White),
                    TokenKind::Whitespace => Style::default(),
                    TokenKind::Unknown => Style::default().fg(Color::Red),
                };
                Span::styled(token.text, style)
            })
            .collect();
        Line::from(spans)
    }

    /// Build the display lines
    fn build_lines(&self) -> Vec<Line<'a>> {
        let mut lines = Vec::new();

        // Render history
        for entry in &self.state.history {
            // Input line with prompt
            let mut input_spans = vec![Span::styled(
                self.prompt.to_string(),
                Style::default().fg(Color::Green),
            )];
            input_spans.extend(self.highlight_code(&entry.input).spans);
            lines.push(Line::from(input_spans));

            // Output line (if any)
            if let Some(output) = &entry.output {
                let style = if entry.is_error {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default().fg(Color::White)
                };
                for line in output.lines() {
                    lines.push(Line::from(Span::styled(format!("  {}", line), style)));
                }
            }
        }

        // Current input line with cursor
        let mut input_spans = vec![Span::styled(
            self.prompt.to_string(),
            Style::default().fg(Color::Green),
        )];

        if self.focused {
            // Split input at cursor position and show cursor
            let (before, after) = self.state.input.split_at(self.state.cursor);

            if !before.is_empty() {
                input_spans.extend(self.highlight_code(before).spans);
            }

            // Cursor character (block cursor)
            let cursor_char = if after.is_empty() {
                " "
            } else {
                &after[..after.chars().next().map_or(0, |c| c.len_utf8())]
            };
            input_spans.push(Span::styled(
                cursor_char.to_string(),
                Style::default().bg(Color::White).fg(Color::Black),
            ));

            // Rest after cursor
            if !after.is_empty() && after.len() > cursor_char.len() {
                input_spans.extend(self.highlight_code(&after[cursor_char.len()..]).spans);
            }
        } else {
            input_spans.extend(self.highlight_code(&self.state.input).spans);
        }

        lines.push(Line::from(input_spans));

        lines
    }
}

impl Widget for &ReplPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = if self.focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .title(" REPL ")
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = self.build_lines();

        // Calculate scroll to keep cursor visible
        let content_height = lines.len() as u16;
        let visible_height = inner.height;
        let scroll = if content_height > visible_height {
            content_height.saturating_sub(visible_height)
        } else {
            0
        };

        let paragraph = Paragraph::new(lines)
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false });

        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_entry() {
        let entry = HistoryEntry::new("5 dup").with_output("5 5");
        assert_eq!(entry.input, "5 dup");
        assert_eq!(entry.output.as_deref(), Some("5 5"));
        assert!(!entry.is_error);

        let error = HistoryEntry::new("bad").with_error("unknown word");
        assert!(error.is_error);
    }

    #[test]
    fn test_repl_state_input() {
        let mut state = ReplState::new();

        state.insert_char('h');
        state.insert_char('i');
        assert_eq!(state.input, "hi");
        assert_eq!(state.cursor, 2);

        state.backspace();
        assert_eq!(state.input, "h");
        assert_eq!(state.cursor, 1);

        state.cursor_left();
        state.insert_char('x');
        assert_eq!(state.input, "xh");
    }

    #[test]
    fn test_repl_state_cursor_movement() {
        let mut state = ReplState::new();
        state.input = "hello".to_string();
        state.cursor = 2;

        state.cursor_left();
        assert_eq!(state.cursor, 1);

        state.cursor_home();
        assert_eq!(state.cursor, 0);

        state.cursor_end();
        assert_eq!(state.cursor, 5);
    }

    #[test]
    fn test_repl_pane_render() {
        let mut state = ReplState::new();
        state.add_entry(HistoryEntry::new("42 dup").with_output("42 42"));
        state.input = "swap".to_string();

        let pane = ReplPane::new(&state);

        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        (&pane).render(area, &mut buf);

        // Just verify it doesn't panic
    }

    #[test]
    fn test_highlight_code() {
        let state = ReplState::new();
        let pane = ReplPane::new(&state);

        let line = pane.highlight_code("42 dup add");
        assert!(!line.spans.is_empty());
    }
}
