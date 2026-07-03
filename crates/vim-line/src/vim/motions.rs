//! Pure cursor-motion functions.
//!
//! Every function here takes the current cursor byte offset and the text
//! buffer and returns the new cursor position. They don't touch editor
//! mode, yank buffer, or selection — callers on `VimLineEditor` apply the
//! returned offset into `self.cursor`.

/// Move cursor left by one character.
pub(super) fn move_left(cursor: usize, text: &str) -> usize {
    if cursor > 0 {
        // Find the previous character boundary
        let mut new_pos = cursor - 1;
        while new_pos > 0 && !text.is_char_boundary(new_pos) {
            new_pos -= 1;
        }
        new_pos
    } else {
        cursor
    }
}

/// Move cursor right by one character.
pub(super) fn move_right(cursor: usize, text: &str) -> usize {
    if cursor < text.len() {
        // Find the next character boundary
        let mut new_pos = cursor + 1;
        while new_pos < text.len() && !text.is_char_boundary(new_pos) {
            new_pos += 1;
        }
        new_pos
    } else {
        cursor
    }
}

/// Move cursor to start of line (0).
pub(super) fn move_line_start(cursor: usize, text: &str) -> usize {
    text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// Move cursor to first non-whitespace of line (^).
pub(super) fn move_first_non_blank(cursor: usize, text: &str) -> usize {
    let line_start = move_line_start(cursor, text);
    // Skip whitespace
    for (i, c) in text[line_start..].char_indices() {
        if c == '\n' || !c.is_whitespace() {
            return line_start + i;
        }
    }
    line_start
}

/// Move cursor to end of line.
///
/// In Normal mode, the cursor should be ON the last character.
/// When `past_end` is true (Insert mode), the cursor may go past the
/// last char onto the newline / EOF position.
fn line_end(cursor: usize, text: &str, past_end: bool) -> usize {
    // Find the end of the current line
    let line_end = text[cursor..]
        .find('\n')
        .map(|i| cursor + i)
        .unwrap_or(text.len());

    if past_end || line_end == 0 {
        line_end
    } else {
        // In Normal mode, cursor should be ON the last character
        // Find the start of the last character (handle multi-byte)
        let mut last_char_start = line_end.saturating_sub(1);
        while last_char_start > 0 && !text.is_char_boundary(last_char_start) {
            last_char_start -= 1;
        }
        last_char_start
    }
}

/// Move cursor to end of line (Normal mode — stays on last char).
pub(super) fn move_line_end(cursor: usize, text: &str) -> usize {
    line_end(cursor, text, false)
}

/// Move cursor past end of line (Insert mode).
pub(super) fn move_line_end_insert(cursor: usize, text: &str) -> usize {
    line_end(cursor, text, true)
}

/// Vim word class. Every byte of the buffer belongs to exactly one of
/// these; a "word" (lowercase `w`/`b`/`e`) is a maximal run of one class.
/// Whitespace-only breaks define vim WORDs (`W`/`B`/`E`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Whitespace,
    Keyword,
    Punctuation,
}

/// Classify a single byte into a vim word class.
///
/// - **Keyword**: ASCII alphanumeric + `_` (vim's default `iskeyword`).
/// - **Whitespace**: ASCII whitespace.
/// - **Punctuation**: any other byte (includes `(`, `)`, `,`, `:`, `-`,
///   and continuation bytes of multibyte chars, which are treated as
///   punctuation runs — matching the byte-oriented motion style of the
///   rest of this module).
fn class_of(b: u8) -> Class {
    if b.is_ascii_whitespace() {
        Class::Whitespace
    } else if b.is_ascii_alphanumeric() || b == b'_' {
        Class::Keyword
    } else {
        Class::Punctuation
    }
}

/// Move cursor forward by word (w) — vim *word* semantics.
///
/// Lands on the start of the next word, where a word is a maximal run of
/// one class (keyword / punctuation / whitespace). Punctuation runs are
/// their own words, so `foo,bar` jumps `f→,→b`.
pub(super) fn move_word_forward(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = cursor;

    if pos < bytes.len() {
        // Skip the rest of the current word-class run (no-op if we're
        // already on whitespace).
        let start_class = class_of(bytes[pos]);
        if start_class != Class::Whitespace {
            while pos < bytes.len() && class_of(bytes[pos]) == start_class {
                pos += 1;
            }
        }
        // Skip any whitespace separating words.
        while pos < bytes.len() && class_of(bytes[pos]) == Class::Whitespace {
            pos += 1;
        }
    }

    pos
}

/// Move cursor backward by word (b) — vim *word* semantics.
///
/// Lands on the start of the current/previous word.
///
/// Note: vim-line permits the normal-mode cursor to rest at `text.len()`
/// (one past the last char) — e.g. after `w` consumes a final word. Real
/// vim's cursor is always *on* a character, so we clamp the start to the
/// last char. Without this, `b` from EOB would land on a trailing
/// punctuation word (like the `,` in `foo,`) instead of skipping back to
/// the previous keyword, diverging from vim.
pub(super) fn move_word_backward(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return 0;
    }
    // Treat a cursor sitting at EOB as "on the last char" (see doc above).
    let mut pos = cursor.min(bytes.len() - 1);
    if pos == 0 {
        return 0;
    }
    pos -= 1;
    // Skip whitespace before the target word.
    while pos > 0 && class_of(bytes[pos]) == Class::Whitespace {
        pos -= 1;
    }
    if class_of(bytes[pos]) == Class::Whitespace {
        // Entire prefix is whitespace.
        return 0;
    }
    // Walk to the start of this word-class run.
    let target = class_of(bytes[pos]);
    while pos > 0 && class_of(bytes[pos - 1]) == target {
        pos -= 1;
    }

    pos
}

/// Move cursor to end of word (e) — vim *word* semantics.
///
/// Lands on the last byte of the current/next word.
pub(super) fn move_word_end(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = cursor;

    if pos >= bytes.len() {
        return pos;
    }
    // Move at least one character forward.
    pos += 1;
    // Skip whitespace.
    while pos < bytes.len() && class_of(bytes[pos]) == Class::Whitespace {
        pos += 1;
    }
    if pos >= bytes.len() {
        return bytes.len();
    }
    // Walk to the last byte of this word-class run.
    let target = class_of(bytes[pos]);
    while pos + 1 < bytes.len() && class_of(bytes[pos + 1]) == target {
        pos += 1;
    }

    pos
}

/// Move cursor forward by WORD (W) — vim *WORD* semantics.
///
/// A WORD is a maximal run of non-whitespace. This is the classic
/// whitespace-only motion.
pub(super) fn move_word_forward_word(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = cursor;

    // Skip current WORD (non-whitespace)
    while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    // Skip whitespace
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }

    pos
}

/// Move cursor backward by WORD (B) — vim *WORD* semantics.
pub(super) fn move_word_backward_word(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = cursor;

    // Skip whitespace before cursor
    while pos > 0 && bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }
    // Skip WORD (non-whitespace)
    while pos > 0 && !bytes[pos - 1].is_ascii_whitespace() {
        pos -= 1;
    }

    pos
}

/// Move cursor to end of WORD (E) — vim *WORD* semantics.
pub(super) fn move_word_end_word(cursor: usize, text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut pos = cursor;

    // Move at least one character
    if pos < bytes.len() {
        pos += 1;
    }
    // Skip whitespace
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    // Move to end of WORD
    while pos < bytes.len() && !bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    // Back up one (end of WORD, not start of next)
    if pos > cursor + 1 {
        pos -= 1;
    }

    pos
}

/// True when `cursor` is on the first line of `text` — i.e. there is no
/// newline at or before the byte preceding the cursor. A single-line
/// buffer is always on the first line.
pub(super) fn is_on_first_line(cursor: usize, text: &str) -> bool {
    !text[..cursor.min(text.len())].contains('\n')
}

/// True when `cursor` is on the last line of `text` — i.e. there is no
/// newline at or after the cursor. A single-line buffer is always on the
/// last line.
pub(super) fn is_on_last_line(cursor: usize, text: &str) -> bool {
    !text[cursor.min(text.len())..].contains('\n')
}

/// Move cursor up one line (k).
pub(super) fn move_up(cursor: usize, text: &str) -> usize {
    // Find current line start
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);

    if line_start == 0 {
        // Already on first line, can't go up
        return cursor;
    }

    // Column offset from line start
    let col = cursor - line_start;

    // Find previous line start
    let prev_line_start = text[..line_start - 1]
        .rfind('\n')
        .map(|i| i + 1)
        .unwrap_or(0);

    // Previous line length
    let prev_line_end = line_start - 1; // Position of \n
    let prev_line_len = prev_line_end - prev_line_start;

    // Move to same column or end of line
    prev_line_start + col.min(prev_line_len)
}

/// Move cursor down one line (j).
pub(super) fn move_down(cursor: usize, text: &str) -> usize {
    // Find current line start
    let line_start = text[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);

    // Column offset
    let col = cursor - line_start;

    // Find next line start
    let Some(newline_pos) = text[cursor..].find('\n') else {
        // Already on last line
        return cursor;
    };
    let next_line_start = cursor + newline_pos + 1;

    if next_line_start >= text.len() {
        // Next line is empty/doesn't exist
        return text.len();
    }

    // Find next line end
    let next_line_end = text[next_line_start..]
        .find('\n')
        .map(|i| next_line_start + i)
        .unwrap_or(text.len());

    let next_line_len = next_line_end - next_line_start;

    // Move to same column or end of line
    next_line_start + col.min(next_line_len)
}

/// Move cursor to matching bracket (%).
/// Supports (), [], {}, and <>.
pub(super) fn move_to_matching_bracket(cursor: usize, text: &str) -> usize {
    if cursor >= text.len() {
        return cursor;
    }

    // Get the character at the cursor
    let Some(c) = text[cursor..].chars().next() else {
        return cursor;
    };

    // Define bracket pairs: (opening, closing)
    let pairs = [('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];

    // Check if current char is an opening or closing bracket
    for (open, close) in pairs.iter() {
        if c == *open {
            // Search forward for matching close
            if let Some(pos) = find_matching_forward(cursor, text, *open, *close) {
                return pos;
            }
            return cursor;
        }
        if c == *close {
            // Search backward for matching open
            if let Some(pos) = find_matching_backward(cursor, text, *open, *close) {
                return pos;
            }
            return cursor;
        }
    }

    cursor
}

/// Find matching closing bracket, searching forward from cursor.
pub(super) fn find_matching_forward(
    cursor: usize,
    text: &str,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;
    let mut pos = cursor;

    // Move past the opening bracket
    pos += open.len_utf8();

    for (i, c) in text[pos..].char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(pos + i);
            }
        }
    }
    None
}

/// Find matching opening bracket, searching backward from cursor.
pub(super) fn find_matching_backward(
    cursor: usize,
    text: &str,
    open: char,
    close: char,
) -> Option<usize> {
    let mut depth = 1;

    // Search backward from just before cursor
    let search_text = &text[..cursor];
    for (i, c) in search_text.char_indices().rev() {
        if c == close {
            depth += 1;
        } else if c == open {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}
