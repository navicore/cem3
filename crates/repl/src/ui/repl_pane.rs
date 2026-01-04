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
    widgets::{Paragraph, Widget, Wrap},
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
    /// Scroll offset for history display
    pub scroll: u16,
    /// History navigation index (None = current input, Some(i) = browsing history)
    history_index: Option<usize>,
    /// Saved input when browsing history
    saved_input: String,
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

    /// Clear the current input and reset history navigation
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
        self.saved_input.clear();
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

    /// Navigate to previous command in history (up arrow / k)
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // First time navigating up - save current input and go to last history entry
                self.saved_input = self.input.clone();
                let last_idx = self.history.len() - 1;
                self.history_index = Some(last_idx);
                self.input = self.history[last_idx].input.clone();
            }
            Some(idx) if idx > 0 => {
                // Navigate further back
                self.history_index = Some(idx - 1);
                self.input = self.history[idx - 1].input.clone();
            }
            Some(_) => {
                // Already at oldest entry
            }
        }
        self.cursor = self.input.len();
    }

    /// Navigate to next command in history (down arrow / j)
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(idx) if idx + 1 < self.history.len() => {
                // Navigate forward in history
                self.history_index = Some(idx + 1);
                self.input = self.history[idx + 1].input.clone();
            }
            Some(_) => {
                // At newest history entry - restore saved input
                self.history_index = None;
                self.input = std::mem::take(&mut self.saved_input);
            }
            None => {
                // Already at current input, nothing to do
            }
        }
        self.cursor = self.input.len();
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
        let continuation_prompt = ".... ";

        // Render history (with multiline support)
        for entry in &self.state.history {
            // Split input by newlines for multiline history entries
            for (i, input_line) in entry.input.split('\n').enumerate() {
                let prompt = if i == 0 {
                    self.prompt
                } else {
                    continuation_prompt
                };
                let mut spans = vec![Span::styled(
                    prompt.to_string(),
                    Style::default().fg(Color::Green),
                )];
                spans.extend(self.highlight_code(input_line).spans);
                lines.push(Line::from(spans));
            }

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

        // Current input with multiline support
        let input_lines: Vec<&str> = self.state.input.split('\n').collect();

        // Find which line the cursor is on and the column within that line
        let (cursor_line, cursor_col) = if self.focused {
            let mut line_idx = 0;
            let mut col = self.state.cursor;
            let mut pos = 0;
            for (i, line_text) in input_lines.iter().enumerate() {
                let line_end = pos + line_text.len();
                if self.state.cursor <= line_end {
                    line_idx = i;
                    col = self.state.cursor - pos;
                    break;
                }
                pos = line_end + 1; // +1 for the newline character
                line_idx = i + 1; // cursor is past this line
            }
            // Clamp to valid range
            let col = col.min(input_lines.get(line_idx).map_or(0, |l| l.len()));
            (line_idx, col)
        } else {
            (0, 0)
        };

        // Render each input line
        for (i, line_text) in input_lines.iter().enumerate() {
            let prompt = if i == 0 {
                self.prompt
            } else {
                continuation_prompt
            };
            let mut spans = vec![Span::styled(
                prompt.to_string(),
                Style::default().fg(Color::Green),
            )];

            if self.focused && i == cursor_line {
                // This line has the cursor - split at cursor position
                let col = cursor_col.min(line_text.len());
                let (before, after) = line_text.split_at(col);

                if !before.is_empty() {
                    spans.extend(self.highlight_code(before).spans);
                }

                // Cursor character (block cursor)
                let cursor_char = if after.is_empty() {
                    " "
                } else {
                    &after[..after.chars().next().map_or(0, |c| c.len_utf8())]
                };
                spans.push(Span::styled(
                    cursor_char.to_string(),
                    Style::default().bg(Color::White).fg(Color::Black),
                ));

                // Rest after cursor
                if !after.is_empty() && after.len() > cursor_char.len() {
                    spans.extend(self.highlight_code(&after[cursor_char.len()..]).spans);
                }
            } else {
                // No cursor on this line
                spans.extend(self.highlight_code(line_text).spans);
            }

            lines.push(Line::from(spans));
        }

        lines
    }
}

impl Widget for &ReplPane<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // No border for REPL - it's the primary interface
        let lines = self.build_lines();

        // Calculate wrapped content height for scroll
        // Each line may wrap to multiple display lines
        let width = area.width.max(1) as usize;
        let wrapped_height: u16 = lines
            .iter()
            .map(|line| {
                let line_width: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
                // At least 1 line, or ceil(line_width / width)
                line_width.max(1).div_ceil(width) as u16
            })
            .sum();

        let visible_height = area.height;
        let scroll = wrapped_height.saturating_sub(visible_height);

        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0));

        paragraph.render(area, buf);
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
