//! Ex-style command-line mode for `VimLineEditor`.
//!
//! Entered from Normal via `:` when `command_mode_enabled` is on. Operates
//! on `self.command_buf` only — the host's main text is never touched.
//! Submission emits `Action::SubmitCommand` carrying the typed buffer
//! (without the leading `:`); the host routes that to its command dispatch.

use super::{Mode, VimLineEditor};
use crate::{Action, EditResult, Key, KeyCode};

impl VimLineEditor {
    pub(super) fn handle_command_line(&mut self, key: Key) -> EditResult {
        match key.code {
            KeyCode::Escape => {
                self.exit_command_line();
                EditResult::none()
            }

            // Ctrl+C cancels like Escape.
            KeyCode::Char('c') if key.ctrl => {
                self.exit_command_line();
                EditResult::none()
            }

            KeyCode::Enter => {
                let cmd = std::mem::take(&mut self.command_buf);
                self.exit_command_line();
                EditResult::action(Action::SubmitCommand(cmd))
            }

            KeyCode::Backspace => {
                if self.command_cursor == 0 {
                    return EditResult::none();
                }
                let mut start = self.command_cursor - 1;
                while start > 0 && !self.command_buf.is_char_boundary(start) {
                    start -= 1;
                }
                self.command_buf
                    .replace_range(start..self.command_cursor, "");
                self.command_cursor = start;
                EditResult::none()
            }

            KeyCode::Delete => {
                if self.command_cursor >= self.command_buf.len() {
                    return EditResult::none();
                }
                let mut end = self.command_cursor + 1;
                while end < self.command_buf.len() && !self.command_buf.is_char_boundary(end) {
                    end += 1;
                }
                self.command_buf.replace_range(self.command_cursor..end, "");
                EditResult::none()
            }

            KeyCode::Left => {
                if self.command_cursor > 0 {
                    let mut p = self.command_cursor - 1;
                    while p > 0 && !self.command_buf.is_char_boundary(p) {
                        p -= 1;
                    }
                    self.command_cursor = p;
                }
                EditResult::none()
            }

            KeyCode::Right => {
                if self.command_cursor < self.command_buf.len() {
                    let mut p = self.command_cursor + 1;
                    while p < self.command_buf.len() && !self.command_buf.is_char_boundary(p) {
                        p += 1;
                    }
                    self.command_cursor = p;
                }
                EditResult::none()
            }

            KeyCode::Home => {
                self.command_cursor = 0;
                EditResult::none()
            }

            KeyCode::End => {
                self.command_cursor = self.command_buf.len();
                EditResult::none()
            }

            KeyCode::Char(c) if !key.ctrl && !key.alt => {
                self.command_buf.insert(self.command_cursor, c);
                self.command_cursor += c.len_utf8();
                EditResult::none()
            }

            _ => EditResult::none(),
        }
    }

    fn exit_command_line(&mut self) {
        self.command_buf.clear();
        self.command_cursor = 0;
        self.mode = Mode::Normal;
    }
}
