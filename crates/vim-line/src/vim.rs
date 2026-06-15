//! Vim-style line editor implementation.
//!
//! Owns the `VimLineEditor` struct, its `Mode` / `Operator` state, cursor-
//! motion and edit helpers, the five mode-specific key handlers, and the
//! `LineEditor` trait implementation.

use crate::{Action, EditResult, Key, KeyCode, LineEditor, TextEdit};
use std::ops::Range;

mod command_line;
mod edits;
mod motions;

/// Vim editing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Normal,
    Insert,
    OperatorPending(Operator),
    Visual,
    /// Waiting for a character to replace the one under cursor (r command)
    ReplaceChar,
    /// Ex-style command line entered with `:` from Normal (opt-in via the
    /// editor's command-mode flag). Operates on a separate command buffer
    /// owned by the editor; the host's main text is untouched.
    CommandLine,
}

/// Operators that wait for a motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Operator {
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
    pub(in crate::vim) cursor: usize,
    pub(in crate::vim) mode: Mode,
    /// Anchor point for visual selection (cursor is the other end).
    pub(in crate::vim) visual_anchor: Option<usize>,
    /// Last yanked text (for paste).
    pub(in crate::vim) yank_buffer: String,
    /// Whether Normal-mode `:` enters `Mode::CommandLine`. Off by default so
    /// non-REPL hosts keep classic vim behavior (where `:` is a no-op until
    /// the host opts in to Ex-style command entry).
    pub(in crate::vim) command_mode_enabled: bool,
    /// Buffer for the Ex-style command line (only populated in
    /// `Mode::CommandLine`). Decoupled from the host's main text.
    pub(in crate::vim) command_buf: String,
    /// Cursor within `command_buf` (byte offset).
    pub(in crate::vim) command_cursor: usize,
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
            command_mode_enabled: false,
            command_buf: String::new(),
            command_cursor: 0,
        }
    }

    /// Enable (or disable) the opt-in `:` -> `Mode::CommandLine` behavior.
    /// Builder-style; call this on a freshly constructed editor.
    pub fn with_command_mode(mut self, enabled: bool) -> Self {
        self.command_mode_enabled = enabled;
        self
    }

    /// Current command-line buffer, if the editor is in `Mode::CommandLine`.
    /// Returns `None` in every other mode so the host can render it
    /// unconditionally.
    pub fn command_line_buffer(&self) -> Option<&str> {
        if self.mode == Mode::CommandLine {
            Some(&self.command_buf)
        } else {
            None
        }
    }

    /// Cursor offset within the command-line buffer (only meaningful while
    /// in `Mode::CommandLine`).
    pub fn command_line_cursor(&self) -> usize {
        self.command_cursor
    }

    /// Replace the command-line buffer and cursor. No-op outside
    /// `Mode::CommandLine`. Used by hosts that compute completions
    /// (`Tab`-cycling, etc.) and want to install the result.
    pub fn set_command_line(&mut self, buf: String, cursor: usize) {
        if self.mode != Mode::CommandLine {
            return;
        }
        let cursor = cursor.min(buf.len());
        // Snap to a char boundary if the caller handed us a mid-codepoint
        // offset.
        let cursor = if buf.is_char_boundary(cursor) {
            cursor
        } else {
            let mut p = cursor;
            while p > 0 && !buf.is_char_boundary(p) {
                p -= 1;
            }
            p
        };
        self.command_buf = buf;
        self.command_cursor = cursor;
    }

    /// Current mode — test-only accessor.
    #[cfg(test)]
    fn mode(&self) -> Mode {
        self.mode
    }

    /// Clamp cursor to valid range for the given text.
    fn clamp_cursor(&mut self, text: &str) {
        self.cursor = self.cursor.min(text.len());
    }

    /// Move cursor left by one character.
    fn move_left(&mut self, text: &str) {
        self.cursor = motions::move_left(self.cursor, text);
    }

    /// Move cursor right by one character.
    fn move_right(&mut self, text: &str) {
        self.cursor = motions::move_right(self.cursor, text);
    }

    /// Move cursor to start of line (0).
    fn move_line_start(&mut self, text: &str) {
        self.cursor = motions::move_line_start(self.cursor, text);
    }

    /// Move cursor to first non-whitespace of line (^).
    fn move_first_non_blank(&mut self, text: &str) {
        self.cursor = motions::move_first_non_blank(self.cursor, text);
    }

    /// Move cursor to end of line (Normal mode — stays on last char).
    fn move_line_end(&mut self, text: &str) {
        self.cursor = motions::move_line_end(self.cursor, text);
    }

    /// Move cursor past end of line (Insert mode).
    fn move_line_end_insert(&mut self, text: &str) {
        self.cursor = motions::move_line_end_insert(self.cursor, text);
    }

    /// Move cursor forward by word (w).
    fn move_word_forward(&mut self, text: &str) {
        self.cursor = motions::move_word_forward(self.cursor, text);
    }

    /// Move cursor backward by word (b).
    fn move_word_backward(&mut self, text: &str) {
        self.cursor = motions::move_word_backward(self.cursor, text);
    }

    /// Move cursor to end of word (e).
    fn move_word_end(&mut self, text: &str) {
        self.cursor = motions::move_word_end(self.cursor, text);
    }

    /// Move cursor up one line (k).
    fn move_up(&mut self, text: &str) {
        self.cursor = motions::move_up(self.cursor, text);
    }

    /// Move cursor down one line (j).
    fn move_down(&mut self, text: &str) {
        self.cursor = motions::move_down(self.cursor, text);
    }

    /// Move cursor to matching bracket (%).
    /// Supports (), [], {}, and <>.
    fn move_to_matching_bracket(&mut self, text: &str) {
        self.cursor = motions::move_to_matching_bracket(self.cursor, text);
    }

    /// Dispatch a shared motion key (h/l/j/k/0/$/^/w/b/e/%/Left/Right/Home/End)
    /// to the appropriate cursor helper. Returns `true` when the key was
    /// recognized as a motion, `false` otherwise.
    ///
    /// Up/Down arrow keys are intentionally NOT handled here — Normal mode
    /// treats them as history navigation, not motion.
    ///
    /// Called by Normal and Visual handlers so each can delegate motion
    /// interpretation to one place and then wrap the result in its own way.
    /// OperatorPending has extra `c`/`cw`/`ce` quirks and handles motion
    /// itself.
    fn dispatch_motion(&mut self, code: KeyCode, text: &str) -> bool {
        match code {
            KeyCode::Char('h') | KeyCode::Left => self.move_left(text),
            KeyCode::Char('l') | KeyCode::Right => self.move_right(text),
            KeyCode::Char('j') => self.move_down(text),
            KeyCode::Char('k') => self.move_up(text),
            KeyCode::Char('0') | KeyCode::Home => self.move_line_start(text),
            KeyCode::Char('^') => self.move_first_non_blank(text),
            KeyCode::Char('$') | KeyCode::End => self.move_line_end(text),
            KeyCode::Char('w') => self.move_word_forward(text),
            KeyCode::Char('b') => self.move_word_backward(text),
            KeyCode::Char('e') => self.move_word_end(text),
            KeyCode::Char('%') => self.move_to_matching_bracket(text),
            _ => return false,
        }
        true
    }

    /// Handle key in Normal mode.
    fn handle_normal(&mut self, key: Key, text: &str) -> EditResult {
        // History at line boundaries: `k` on the first line and `j` on the
        // last line become history navigation (matching readline vi-mode and
        // most REPL muscle memory). Off-boundary, both stay as line motions
        // via `dispatch_motion` below, so multi-line buffers keep working.
        match key.code {
            KeyCode::Char('k') if motions::is_on_first_line(self.cursor, text) => {
                return EditResult::action(Action::HistoryPrev);
            }
            KeyCode::Char('j') if motions::is_on_last_line(self.cursor, text) => {
                return EditResult::action(Action::HistoryNext);
            }
            _ => {}
        }

        // Shared motions (h/l/j/k/0/$/^/w/b/e/%/Left/Right/Home/End).
        // Up/Down are NOT motions in Normal — they're history navigation below.
        if self.dispatch_motion(key.code, text) {
            return EditResult::cursor_only();
        }

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
                self.move_line_end_insert(text);
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

            // Replace character (r)
            KeyCode::Char('r') => {
                self.mode = Mode::ReplaceChar;
                EditResult::none()
            }

            // Ex-style command line (`:`) — opt-in. Without the flag, `:` is
            // an unhandled key in Normal mode, matching classic vim's
            // "press `i` first" behavior so non-REPL hosts are unaffected.
            KeyCode::Char(':') if self.command_mode_enabled => {
                self.mode = Mode::CommandLine;
                self.command_buf.clear();
                self.command_cursor = 0;
                EditResult::none()
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

            // Escape in Normal mode is a no-op (safe to spam like in vim)
            // Use Ctrl+C to cancel/quit
            KeyCode::Escape => EditResult::none(),

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
                let end = self.cursor; // Save original cursor before updating
                self.cursor = start;
                EditResult::edit(TextEdit::Delete { start, end })
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
                // In Insert mode, cursor can go past the last character
                self.move_line_end_insert(text);
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
            KeyCode::Char('w') => {
                // Special case: cw behaves like ce (change to end of word, not including space)
                // This is a vim quirk for historical compatibility
                if op == Operator::Change {
                    self.move_word_end(text);
                    // Include the character at cursor
                    if self.cursor < text.len() {
                        self.cursor += 1;
                    }
                } else {
                    self.move_word_forward(text);
                }
            }
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

    /// Handle key in Visual mode.
    fn handle_visual(&mut self, key: Key, text: &str) -> EditResult {
        match key.code {
            KeyCode::Escape => {
                self.mode = Mode::Normal;
                self.visual_anchor = None;
                EditResult::none()
            }

            // Motions extend selection (note: `^` and `%` are intentionally
            // not wired here to preserve original behavior — tracked for a
            // separate behavior-change PR).
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

    /// Handle key in ReplaceChar mode (waiting for character after 'r').
    fn handle_replace_char(&mut self, key: Key, text: &str) -> EditResult {
        self.mode = Mode::Normal;

        match key.code {
            KeyCode::Escape => EditResult::none(),
            KeyCode::Char(c) if !key.ctrl && !key.alt => {
                // Replace character at cursor
                if self.cursor >= text.len() {
                    return EditResult::none();
                }

                // Find the end of the current character
                let mut end = self.cursor + 1;
                while end < text.len() && !text.is_char_boundary(end) {
                    end += 1;
                }

                // Delete current char and insert new one
                // Note: edits are applied in reverse order, so Insert comes first in vec
                EditResult {
                    edits: vec![
                        TextEdit::Insert {
                            at: self.cursor,
                            text: c.to_string(),
                        },
                        TextEdit::Delete {
                            start: self.cursor,
                            end,
                        },
                    ],
                    ..Default::default()
                }
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
            Mode::ReplaceChar => self.handle_replace_char(key, text),
            Mode::CommandLine => self.handle_command_line(key),
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
            Mode::ReplaceChar => "r...",
            Mode::CommandLine => "COMMAND",
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
        self.command_buf.clear();
        self.command_cursor = 0;
        // Keep yank buffer and command_mode_enabled flag across resets.
    }

    fn set_cursor(&mut self, pos: usize, text: &str) {
        // Clamp to text length and ensure we're at a char boundary
        let pos = pos.min(text.len());
        self.cursor = if text.is_char_boundary(pos) {
            pos
        } else {
            // Walk backwards to find a valid boundary
            let mut p = pos;
            while p > 0 && !text.is_char_boundary(p) {
                p -= 1;
            }
            p
        };
    }
}

#[cfg(test)]
mod tests;
