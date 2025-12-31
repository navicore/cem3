//! Vim-style line editor implementation.

use crate::{Action, EditResult, Key, KeyCode, LineEditor, TextEdit};
use std::ops::Range;

/// Vim editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    OperatorPending(Operator),
    Visual,
}

/// Operators that wait for a motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
}

/// A vim-style line editor.
///
/// Implements modal editing with Normal, Insert, Visual, and OperatorPending modes.
/// Designed for single "one-shot" inputs that may span multiple lines.
#[derive(Debug, Clone)]
pub struct VimLineEditor {
    cursor: usize,
    mode: Mode,
    /// Anchor point for visual selection (cursor is the other end).
    visual_anchor: Option<usize>,
    /// Last yanked text (for paste).
    yank_buffer: String,
}

impl Default for VimLineEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl VimLineEditor {
    /// Create a new editor in Normal mode.
    pub fn new() -> Self {
        Self {
            cursor: 0,
            mode: Mode::Normal,
            visual_anchor: None,
            yank_buffer: String::new(),
        }
    }

    /// Get the current mode.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Clamp cursor to valid range for the given text.
    fn clamp_cursor(&mut self, text: &str) {
        self.cursor = self.cursor.min(text.len());
    }

    /// Move cursor left by one character.
    fn move_left(&mut self, text: &str) {
        if self.cursor > 0 {
            // Find the previous character boundary
            let mut new_pos = self.cursor - 1;
            while new_pos > 0 && !text.is_char_boundary(new_pos) {
                new_pos -= 1;
            }
            self.cursor = new_pos;
        }
    }

    /// Move cursor right by one character.
    fn move_right(&mut self, text: &str) {
        if self.cursor < text.len() {
            // Find the next character boundary
            let mut new_pos = self.cursor + 1;
            while new_pos < text.len() && !text.is_char_boundary(new_pos) {
                new_pos += 1;
            }
            self.cursor = new_pos;
        }
    }

    /// Move cursor to start of line (0).
    fn move_line_start(&mut self, text: &str) {
        // Find the start of the current line
        self.cursor = text[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    /// Move cursor to first non-whitespace of line (^).
    fn move_first_non_blank(&mut self, text: &str) {
        self.move_line_start(text);
        // Skip whitespace
        let line_start = self.cursor;
        for (i, c) in text[line_start..].char_indices() {
            if c == '\n' || !c.is_whitespace() {
                self.cursor = line_start + i;
                return;
            }
        }
    }

    /// Move cursor to end of line ($).
    fn move_line_end(&mut self, text: &str) {
        // Find the end of the current line
        self.cursor = text[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i)
            .unwrap_or(text.len());
    }

    /// Move cursor forward by word (w).
    fn move_word_forward(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut pos = self.cursor;

        // Skip current word (non-whitespace)
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Skip whitespace
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }

        self.cursor = pos;
    }

    /// Move cursor backward by word (b).
    fn move_word_backward(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut pos = self.cursor;

        // Skip whitespace before cursor
        while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }
        // Skip word (non-whitespace)
        while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() {
            pos -= 1;
        }

        self.cursor = pos;
    }

    /// Move cursor to end of word (e).
    fn move_word_end(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let mut pos = self.cursor;

        // Move at least one character
        if pos < bytes.len() {
            pos += 1;
        }
        // Skip whitespace
        while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Move to end of word
        while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
            pos += 1;
        }
        // Back up one (end of word, not start of next)
        if pos > self.cursor + 1 {
            pos -= 1;
        }

        self.cursor = pos;
    }

    /// Move cursor up one line (k).
    fn move_up(&mut self, text: &str) {
        // Find current line start
        let line_start = text[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        if line_start == 0 {
            // Already on first line, can't go up
            return;
        }

        // Column offset from line start
        let col = self.cursor - line_start;

        // Find previous line start
        let prev_line_start = text[..line_start - 1]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Previous line length
        let prev_line_end = line_start - 1; // Position of \n
        let prev_line_len = prev_line_end - prev_line_start;

        // Move to same column or end of line
        self.cursor = prev_line_start + col.min(prev_line_len);
    }

    /// Move cursor down one line (j).
    fn move_down(&mut self, text: &str) {
        // Find current line start
        let line_start = text[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Column offset
        let col = self.cursor - line_start;

        // Find next line start
        let Some(newline_pos) = text[self.cursor..].find('\n') else {
            // Already on last line
            return;
        };
        let next_line_start = self.cursor + newline_pos + 1;

        if next_line_start >= text.len() {
            // Next line is empty/doesn't exist
            self.cursor = text.len();
            return;
        }

        // Find next line end
        let next_line_end = text[next_line_start..]
            .find('\n')
            .map(|i| next_line_start + i)
            .unwrap_or(text.len());

        let next_line_len = next_line_end - next_line_start;

        // Move to same column or end of line
        self.cursor = next_line_start + col.min(next_line_len);
    }

    /// Delete character at cursor (x).
    fn delete_char(&mut self, text: &str) -> EditResult {
        if self.cursor >= text.len() {
            return EditResult::none();
        }

        // Find the end of the current character
        let mut end = self.cursor + 1;
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }

        let deleted = text[self.cursor..end].to_string();
        EditResult::edit_and_yank(
            TextEdit::Delete {
                start: self.cursor,
                end,
            },
            deleted,
        )
    }

    /// Delete to end of line (D).
    fn delete_to_end(&mut self, text: &str) -> EditResult {
        let end = text[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i)
            .unwrap_or(text.len());

        if self.cursor >= end {
            return EditResult::none();
        }

        let deleted = text[self.cursor..end].to_string();
        EditResult::edit_and_yank(
            TextEdit::Delete {
                start: self.cursor,
                end,
            },
            deleted,
        )
    }

    /// Delete entire current line (dd).
    fn delete_line(&mut self, text: &str) -> EditResult {
        let line_start = text[..self.cursor]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        let line_end = text[self.cursor..]
            .find('\n')
            .map(|i| self.cursor + i + 1) // Include the newline
            .unwrap_or(text.len());

        // If this is the only line and no newline, include leading newline if any
        let (start, end) = if line_start == 0 && line_end == text.len() {
            (0, text.len())
        } else if line_end == text.len() && line_start > 0 {
            // Last line - delete the preceding newline instead
            (line_start - 1, text.len())
        } else {
            (line_start, line_end)
        };

        let deleted = text[start..end].to_string();
        self.cursor = start;

        EditResult::edit_and_yank(TextEdit::Delete { start, end }, deleted)
    }

    /// Paste after cursor (p).
    fn paste_after(&mut self, _text: &str) -> EditResult {
        if self.yank_buffer.is_empty() {
            return EditResult::none();
        }

        let insert_pos = self.cursor + 1;
        let to_insert = self.yank_buffer.clone();
        self.cursor = insert_pos + to_insert.len() - 1;

        EditResult::edit(TextEdit::Insert {
            at: insert_pos.min(_text.len()),
            text: to_insert,
        })
    }

    /// Paste before cursor (P).
    fn paste_before(&mut self, _text: &str) -> EditResult {
        if self.yank_buffer.is_empty() {
            return EditResult::none();
        }

        let to_insert = self.yank_buffer.clone();
        let insert_pos = self.cursor;
        self.cursor = insert_pos + to_insert.len();

        EditResult::edit(TextEdit::Insert {
            at: insert_pos,
            text: to_insert,
        })
    }

    /// Handle key in Normal mode.
    fn handle_normal(&mut self, key: Key, text: &str) -> EditResult {
        match key.code {
            // Mode switching
            KeyCode::Char('i') => {
                self.mode = Mode::Insert;
                EditResult::none()
            }
            KeyCode::Char('a') => {
                self.mode = Mode::Insert;
                self.move_right(text);
                EditResult::none()
            }
            KeyCode::Char('A') => {
                self.mode = Mode::Insert;
                self.move_line_end(text);
                EditResult::none()
            }
            KeyCode::Char('I') => {
                self.mode = Mode::Insert;
                self.move_first_non_blank(text);
                EditResult::none()
            }
            KeyCode::Char('o') => {
                self.mode = Mode::Insert;
                self.move_line_end(text);
                let pos = self.cursor;
                self.cursor = pos + 1;
                EditResult::edit(TextEdit::Insert {
                    at: pos,
                    text: "\n".to_string(),
                })
            }
            KeyCode::Char('O') => {
                self.mode = Mode::Insert;
                self.move_line_start(text);
                let pos = self.cursor;
                EditResult::edit(TextEdit::Insert {
                    at: pos,
                    text: "\n".to_string(),
                })
            }

            // Visual mode
            KeyCode::Char('v') => {
                self.mode = Mode::Visual;
                self.visual_anchor = Some(self.cursor);
                EditResult::none()
            }

            // Motions
            KeyCode::Char('h') | KeyCode::Left => {
                self.move_left(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.move_right(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('j') => {
                self.move_down(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('k') => {
                self.move_up(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('0') | KeyCode::Home => {
                self.move_line_start(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('^') => {
                self.move_first_non_blank(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('$') | KeyCode::End => {
                self.move_line_end(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('w') => {
                self.move_word_forward(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('b') => {
                self.move_word_backward(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('e') => {
                self.move_word_end(text);
                EditResult::cursor_only()
            }

            // Cancel (Ctrl+C)
            KeyCode::Char('c') if key.ctrl => EditResult::action(Action::Cancel),

            // Operators (enter pending mode)
            KeyCode::Char('d') => {
                self.mode = Mode::OperatorPending(Operator::Delete);
                EditResult::none()
            }
            KeyCode::Char('c') => {
                self.mode = Mode::OperatorPending(Operator::Change);
                EditResult::none()
            }
            KeyCode::Char('y') => {
                self.mode = Mode::OperatorPending(Operator::Yank);
                EditResult::none()
            }

            // Direct deletions
            KeyCode::Char('x') => self.delete_char(text),
            KeyCode::Char('D') => self.delete_to_end(text),
            KeyCode::Char('C') => {
                self.mode = Mode::Insert;
                self.delete_to_end(text)
            }

            // Paste
            KeyCode::Char('p') => self.paste_after(text),
            KeyCode::Char('P') => self.paste_before(text),

            // History (arrows only)
            KeyCode::Up => EditResult::action(Action::HistoryPrev),
            KeyCode::Down => EditResult::action(Action::HistoryNext),

            // Submit
            KeyCode::Enter if !key.shift => EditResult::action(Action::Submit),

            // Newline (Shift+Enter)
            KeyCode::Enter if key.shift => {
                self.mode = Mode::Insert;
                let pos = self.cursor;
                self.cursor = pos + 1;
                EditResult::edit(TextEdit::Insert {
                    at: pos,
                    text: "\n".to_string(),
                })
            }

            // Cancel (Escape when input is empty)
            KeyCode::Escape => {
                if text.is_empty() {
                    EditResult::action(Action::Cancel)
                } else {
                    EditResult::none()
                }
            }

            _ => EditResult::none(),
        }
    }

    /// Handle key in Insert mode.
    fn handle_insert(&mut self, key: Key, text: &str) -> EditResult {
        match key.code {
            KeyCode::Escape => {
                self.mode = Mode::Normal;
                // Move cursor left like vim does when exiting insert
                if self.cursor > 0 {
                    self.move_left(text);
                }
                EditResult::none()
            }

            // Ctrl+C exits insert mode
            KeyCode::Char('c') if key.ctrl => {
                self.mode = Mode::Normal;
                EditResult::none()
            }

            KeyCode::Char(c) if !key.ctrl && !key.alt => {
                let pos = self.cursor;
                self.cursor = pos + c.len_utf8();
                EditResult::edit(TextEdit::Insert {
                    at: pos,
                    text: c.to_string(),
                })
            }

            KeyCode::Backspace => {
                if self.cursor == 0 {
                    return EditResult::none();
                }
                let mut start = self.cursor - 1;
                while start > 0 && !text.is_char_boundary(start) {
                    start -= 1;
                }
                self.cursor = start;
                EditResult::edit(TextEdit::Delete {
                    start,
                    end: self.cursor + 1,
                })
            }

            KeyCode::Delete => self.delete_char(text),

            KeyCode::Left => {
                self.move_left(text);
                EditResult::cursor_only()
            }
            KeyCode::Right => {
                self.move_right(text);
                EditResult::cursor_only()
            }
            KeyCode::Up => {
                self.move_up(text);
                EditResult::cursor_only()
            }
            KeyCode::Down => {
                self.move_down(text);
                EditResult::cursor_only()
            }
            KeyCode::Home => {
                self.move_line_start(text);
                EditResult::cursor_only()
            }
            KeyCode::End => {
                self.move_line_end(text);
                EditResult::cursor_only()
            }

            // Enter inserts newline in insert mode
            KeyCode::Enter => {
                let pos = self.cursor;
                self.cursor = pos + 1;
                EditResult::edit(TextEdit::Insert {
                    at: pos,
                    text: "\n".to_string(),
                })
            }

            _ => EditResult::none(),
        }
    }

    /// Handle key in OperatorPending mode.
    fn handle_operator_pending(&mut self, op: Operator, key: Key, text: &str) -> EditResult {
        // First, handle escape to cancel
        if key.code == KeyCode::Escape {
            self.mode = Mode::Normal;
            return EditResult::none();
        }

        // Handle doubled operator (dd, cc, yy) - operates on whole line
        let is_line_op = matches!(
            (op, key.code),
            (Operator::Delete, KeyCode::Char('d'))
                | (Operator::Change, KeyCode::Char('c'))
                | (Operator::Yank, KeyCode::Char('y'))
        );

        if is_line_op {
            self.mode = Mode::Normal;
            return self.apply_operator_line(op, text);
        }

        // Handle motion
        let start = self.cursor;
        match key.code {
            KeyCode::Char('w') => self.move_word_forward(text),
            KeyCode::Char('b') => self.move_word_backward(text),
            KeyCode::Char('e') => {
                self.move_word_end(text);
                // Include the character at cursor for delete/change
                if self.cursor < text.len() {
                    self.cursor += 1;
                }
            }
            KeyCode::Char('0') | KeyCode::Home => self.move_line_start(text),
            KeyCode::Char('$') | KeyCode::End => self.move_line_end(text),
            KeyCode::Char('^') => self.move_first_non_blank(text),
            KeyCode::Char('h') | KeyCode::Left => self.move_left(text),
            KeyCode::Char('l') | KeyCode::Right => self.move_right(text),
            KeyCode::Char('j') => self.move_down(text),
            KeyCode::Char('k') => self.move_up(text),
            _ => {
                // Unknown motion, cancel
                self.mode = Mode::Normal;
                return EditResult::none();
            }
        }

        let end = self.cursor;
        self.mode = Mode::Normal;

        if start == end {
            return EditResult::none();
        }

        let (range_start, range_end) = if start < end {
            (start, end)
        } else {
            (end, start)
        };

        self.apply_operator(op, range_start, range_end, text)
    }

    /// Apply an operator to a range.
    fn apply_operator(
        &mut self,
        op: Operator,
        start: usize,
        end: usize,
        text: &str,
    ) -> EditResult {
        let affected = text[start..end].to_string();
        self.yank_buffer = affected.clone();
        self.cursor = start;

        match op {
            Operator::Delete => EditResult::edit_and_yank(TextEdit::Delete { start, end }, affected),
            Operator::Change => {
                self.mode = Mode::Insert;
                EditResult::edit_and_yank(TextEdit::Delete { start, end }, affected)
            }
            Operator::Yank => {
                // Just yank, no edit
                EditResult {
                    yanked: Some(affected),
                    ..Default::default()
                }
            }
        }
    }

    /// Apply an operator to the whole line.
    fn apply_operator_line(&mut self, op: Operator, text: &str) -> EditResult {
        match op {
            Operator::Delete => self.delete_line(text),
            Operator::Change => {
                let result = self.delete_line(text);
                self.mode = Mode::Insert;
                result
            }
            Operator::Yank => {
                let line_start = text[..self.cursor]
                    .rfind('\n')
                    .map(|i| i + 1)
                    .unwrap_or(0);
                let line_end = text[self.cursor..]
                    .find('\n')
                    .map(|i| self.cursor + i + 1)
                    .unwrap_or(text.len());
                let line = text[line_start..line_end].to_string();
                self.yank_buffer = line.clone();
                EditResult {
                    yanked: Some(line),
                    ..Default::default()
                }
            }
        }
    }

    /// Handle key in Visual mode.
    fn handle_visual(&mut self, key: Key, text: &str) -> EditResult {
        match key.code {
            KeyCode::Escape => {
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                EditResult::none()
            }

            // Motions extend selection
            KeyCode::Char('h') | KeyCode::Left => {
                self.move_left(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.move_right(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('j') => {
                self.move_down(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('k') => {
                self.move_up(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('w') => {
                self.move_word_forward(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('b') => {
                self.move_word_backward(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('e') => {
                self.move_word_end(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('0') | KeyCode::Home => {
                self.move_line_start(text);
                EditResult::cursor_only()
            }
            KeyCode::Char('$') | KeyCode::End => {
                self.move_line_end(text);
                EditResult::cursor_only()
            }

            // Operators on selection
            KeyCode::Char('d') | KeyCode::Char('x') => {
                let (start, end) = self.selection_range();
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                self.apply_operator(Operator::Delete, start, end, text)
            }
            KeyCode::Char('c') => {
                let (start, end) = self.selection_range();
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                self.apply_operator(Operator::Change, start, end, text)
            }
            KeyCode::Char('y') => {
                let (start, end) = self.selection_range();
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                self.apply_operator(Operator::Yank, start, end, text)
            }

            _ => EditResult::none(),
        }
    }

    /// Get the selection range (ordered).
    fn selection_range(&self) -> (usize, usize) {
        let anchor = self.visual_anchor.unwrap_or(self.cursor);
        if self.cursor < anchor {
            (self.cursor, anchor)
        } else {
            (anchor, self.cursor + 1) // Include cursor position
        }
    }
}

impl LineEditor for VimLineEditor {
    fn handle_key(&mut self, key: Key, text: &str) -> EditResult {
        self.clamp_cursor(text);

        let result = match self.mode {
            Mode::Normal => self.handle_normal(key, text),
            Mode::Insert => self.handle_insert(key, text),
            Mode::OperatorPending(op) => self.handle_operator_pending(op, key, text),
            Mode::Visual => self.handle_visual(key, text),
        };

        // Store yanked text
        if let Some(ref yanked) = result.yanked {
            self.yank_buffer = yanked.clone();
        }

        result
    }

    fn cursor(&self) -> usize {
        self.cursor
    }

    fn status(&self) -> &str {
        match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::OperatorPending(Operator::Delete) => "d...",
            Mode::OperatorPending(Operator::Change) => "c...",
            Mode::OperatorPending(Operator::Yank) => "y...",
            Mode::Visual => "VISUAL",
        }
    }

    fn selection(&self) -> Option<Range<usize>> {
        if self.mode == Mode::Visual {
            let (start, end) = self.selection_range();
            Some(start..end)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.cursor = 0;
        self.mode = Mode::Normal;
        self.visual_anchor = None;
        // Keep yank buffer across resets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_motion() {
        let mut editor = VimLineEditor::new();
        let text = "hello world";

        // Move right with 'l'
        editor.handle_key(Key::char('l'), text);
        assert_eq!(editor.cursor(), 1);

        // Move right with 'w'
        editor.handle_key(Key::char('w'), text);
        assert_eq!(editor.cursor(), 6); // Start of "world"

        // Move to end with '$'
        editor.handle_key(Key::char('$'), text);
        assert_eq!(editor.cursor(), 11);

        // Move to start with '0'
        editor.handle_key(Key::char('0'), text);
        assert_eq!(editor.cursor(), 0);
    }

    #[test]
    fn test_mode_switching() {
        let mut editor = VimLineEditor::new();
        let text = "hello";

        assert_eq!(editor.mode(), Mode::Normal);

        editor.handle_key(Key::char('i'), text);
        assert_eq!(editor.mode(), Mode::Insert);

        editor.handle_key(Key::code(KeyCode::Escape), text);
        assert_eq!(editor.mode(), Mode::Normal);
    }

    #[test]
    fn test_delete_word() {
        let mut editor = VimLineEditor::new();
        let text = "hello world";

        // dw should delete "hello "
        editor.handle_key(Key::char('d'), text);
        editor.handle_key(Key::char('w'), text);

        // Check we're back in Normal mode
        assert_eq!(editor.mode(), Mode::Normal);
    }

    #[test]
    fn test_insert_char() {
        let mut editor = VimLineEditor::new();
        let text = "";

        editor.handle_key(Key::char('i'), text);
        let result = editor.handle_key(Key::char('x'), text);

        assert_eq!(result.edits.len(), 1);
        match &result.edits[0] {
            TextEdit::Insert { at, text } => {
                assert_eq!(*at, 0);
                assert_eq!(text, "x");
            }
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn test_visual_mode() {
        let mut editor = VimLineEditor::new();
        let text = "hello world";

        // Enter visual mode
        editor.handle_key(Key::char('v'), text);
        assert_eq!(editor.mode(), Mode::Visual);

        // Extend selection
        editor.handle_key(Key::char('w'), text);

        // Selection should cover from 0 to cursor
        let sel = editor.selection().unwrap();
        assert_eq!(sel.start, 0);
        assert!(sel.end > 0);
    }
}
