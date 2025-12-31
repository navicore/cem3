//! vim-line: A line-oriented vim motions library for TUI applications
//!
//! This crate provides a trait-based interface for line editing with vim-style
//! keybindings. It's designed for "one-shot" editing scenarios like REPLs,
//! chat inputs, and command lines - not full buffer/file editing.
//!
//! # Design Philosophy
//!
//! - **Host-agnostic**: The library doesn't know about terminals or rendering
//! - **Command-based**: Returns mutations for the host to apply
//! - **Caller owns text**: The library never stores your text buffer
//! - **Multi-line aware**: Supports inputs with newlines that grow/shrink
//!
//! # Example
//!
//! ```
//! use vim_line::{LineEditor, VimLineEditor, Key, KeyCode};
//!
//! let mut editor = VimLineEditor::new();
//! let mut text = String::from("hello world");
//!
//! // Process 'dw' to delete word
//! let _ = editor.handle_key(Key::char('d'), &text);
//! let result = editor.handle_key(Key::char('w'), &text);
//!
//! // Apply edits
//! for edit in result.edits.into_iter().rev() {
//!     edit.apply(&mut text);
//! }
//! // text is now "world"
//! ```

mod vim;

pub use vim::VimLineEditor;

use std::ops::Range;

/// The contract between a line editor and its host application.
///
/// Implementations handle key interpretation and cursor management,
/// while the host owns the text buffer and handles rendering.
pub trait LineEditor {
    /// Process a key event, returning edits to apply and any action requested.
    ///
    /// The `text` parameter is the current content - the editor uses it to
    /// calculate motions but never modifies it directly.
    fn handle_key(&mut self, key: Key, text: &str) -> EditResult;

    /// Current cursor position as a byte offset into the text.
    fn cursor(&self) -> usize;

    /// Status text for display (e.g., "NORMAL", "INSERT", "-- VISUAL --").
    fn status(&self) -> &str;

    /// Selection range for highlighting, if in visual mode.
    fn selection(&self) -> Option<Range<usize>>;

    /// Reset editor state (call after submitting/clearing input).
    fn reset(&mut self);

    /// Set cursor position, clamped to valid bounds within text.
    fn set_cursor(&mut self, pos: usize, text: &str);
}

/// Result of processing a key event.
#[derive(Debug, Clone, Default)]
pub struct EditResult {
    /// Text mutations to apply, in order.
    pub edits: Vec<TextEdit>,
    /// Text that was yanked, if any (host can sync to clipboard).
    pub yanked: Option<String>,
    /// Action requested by the editor.
    pub action: Option<Action>,
}

impl EditResult {
    /// Create an empty result (no changes).
    pub fn none() -> Self {
        Self::default()
    }

    /// Create a result with a single action.
    pub fn action(action: Action) -> Self {
        Self {
            action: Some(action),
            ..Default::default()
        }
    }

    /// Create a result with a cursor move (no text change).
    pub fn cursor_only() -> Self {
        Self::default()
    }

    /// Create a result with a single edit.
    pub fn edit(edit: TextEdit) -> Self {
        Self {
            edits: vec![edit],
            ..Default::default()
        }
    }

    /// Create a result with edits and yanked text.
    pub fn edit_and_yank(edit: TextEdit, yanked: String) -> Self {
        Self {
            edits: vec![edit],
            yanked: Some(yanked),
            ..Default::default()
        }
    }
}

/// Actions the editor can request from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// User wants to submit the current input.
    Submit,
    /// User wants previous history entry.
    HistoryPrev,
    /// User wants next history entry.
    HistoryNext,
    /// User wants to cancel/abort.
    Cancel,
}

/// A single text mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextEdit {
    /// Delete text in the given byte range.
    Delete { start: usize, end: usize },
    /// Insert text at the given byte position.
    Insert { at: usize, text: String },
}

impl TextEdit {
    /// Apply this edit to a string.
    pub fn apply(&self, s: &mut String) {
        match self {
            TextEdit::Delete { start, end } => {
                s.replace_range(*start..*end, "");
            }
            TextEdit::Insert { at, text } => {
                s.insert_str(*at, text);
            }
        }
    }
}

/// A key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Key {
    /// Create a plain character key.
    pub fn char(c: char) -> Self {
        Self {
            code: KeyCode::Char(c),
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    /// Create a key with just a code (no modifiers).
    pub fn code(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            alt: false,
            shift: false,
        }
    }

    /// Add ctrl modifier.
    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Add shift modifier.
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Add alt modifier.
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }
}

/// Key codes for non-character keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Escape,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    Tab,
    Enter,
}
