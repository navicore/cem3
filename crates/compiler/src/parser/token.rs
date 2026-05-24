//! Token type and low-level tokenization/escape/float helpers.

/// A token with its source position (1-indexed).
#[derive(Debug, Clone)]
pub struct Token {
    pub text: String,
    /// Line number (0-indexed for LSP compatibility)
    pub line: usize,
    /// Column number (0-indexed)
    pub column: usize,
}

impl Token {
    fn new(text: String, line: usize, column: usize) -> Self {
        Token { text, line, column }
    }
}

impl PartialEq<&str> for Token {
    fn eq(&self, other: &&str) -> bool {
        self.text == *other
    }
}

impl PartialEq<str> for Token {
    fn eq(&self, other: &str) -> bool {
        self.text == other
    }
}

pub(super) fn annotate_error_with_line(msg: String, tok: Option<&Token>) -> String {
    if msg.starts_with("at line ") {
        return msg;
    }
    let line = tok.map(|t| t.line).unwrap_or(0);
    format!("at line {}: {}", line + 1, msg)
}

/// Check if a token looks like a float literal
///
/// Float literals contain either:
/// - A decimal point: `3.14`, `.5`, `5.`
/// - Scientific notation: `1e10`, `1E-5`, `1.5e3`
///
/// This check must happen BEFORE integer parsing to avoid
/// parsing "5" in "5.0" as an integer.
pub(super) fn is_float_literal(token: &str) -> bool {
    // Skip leading minus sign for negative numbers
    let s = token.strip_prefix('-').unwrap_or(token);

    // Must have at least one digit
    if s.is_empty() {
        return false;
    }

    // Check for decimal point or scientific notation
    s.contains('.') || s.contains('e') || s.contains('E')
}

/// Process escape sequences in a string literal, returning the raw byte
/// payload. Seq strings are byte-clean — `\xNN` produces the literal byte
/// `0xNN`, not the UTF-8 encoding of the codepoint U+00NN.
///
/// Supported escape sequences:
/// - `\"` -> `"`  (quote)
/// - `\\` -> `\`  (backslash)
/// - `\n` -> newline
/// - `\r` -> carriage return
/// - `\t` -> tab
/// - `\xNN` -> the single byte `0xNN` (00-FF)
///
/// # `\xNN` byte semantics
///
/// `\xNN` is a *byte*, not a codepoint:
/// - `\x41` -> `0x41` ('A')
/// - `\x1b` -> `0x1B` (ESC, for ANSI terminal codes)
/// - `\xDC` -> `0xDC` (one byte; not the 2-byte UTF-8 of U+00DC)
/// - `\x00` -> `0x00` (one NUL byte; embedded NULs are legal)
///
/// Non-escape characters in the source are copied to the output as their
/// UTF-8 byte sequence — so `"héllo"` is still 6 UTF-8 bytes. The change
/// is only that `\xNN` no longer round-trips through `char` (which it
/// did before, silently producing 2-byte UTF-8 for high-byte escapes).
///
/// This makes byte-clean binary protocol literals (OSC alignment NULs,
/// raw IEEE-754 byte patterns, magic-number headers) expressible in
/// Seq source.
///
/// # Errors
/// Returns error if an unknown escape sequence is encountered.
pub(super) fn unescape_string(s: &str) -> Result<Vec<u8>, String> {
    let mut result: Vec<u8> = Vec::with_capacity(s.len());
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push(b'"'),
                Some('\\') => result.push(b'\\'),
                Some('n') => result.push(b'\n'),
                Some('r') => result.push(b'\r'),
                Some('t') => result.push(b'\t'),
                Some('x') => {
                    // Hex escape: \xNN — emit the literal byte 0xNN.
                    let hex1 = chars.next().ok_or_else(|| {
                        "Incomplete hex escape sequence '\\x' - expected 2 hex digits".to_string()
                    })?;
                    let hex2 = chars.next().ok_or_else(|| {
                        format!(
                            "Incomplete hex escape sequence '\\x{}' - expected 2 hex digits",
                            hex1
                        )
                    })?;

                    let hex_str: String = [hex1, hex2].iter().collect();
                    let byte_val = u8::from_str_radix(&hex_str, 16).map_err(|_| {
                        format!(
                            "Invalid hex escape sequence '\\x{}' - expected 2 hex digits (00-FF)",
                            hex_str
                        )
                    })?;

                    result.push(byte_val);
                }
                Some(c) => {
                    return Err(format!(
                        "Unknown escape sequence '\\{}' in string literal. \
                         Supported: \\\" \\\\ \\n \\r \\t \\xNN",
                        c
                    ));
                }
                None => {
                    return Err("String ends with incomplete escape sequence '\\'".to_string());
                }
            }
        } else {
            // Source-level char: emit its UTF-8 bytes verbatim.
            let mut buf = [0u8; 4];
            result.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
        }
    }

    Ok(result)
}

/// Push the accumulated token (if any) onto `tokens` and reset the buffer.
fn flush_token(tokens: &mut Vec<Token>, current: &mut String, start_line: usize, start_col: usize) {
    if !current.is_empty() {
        tokens.push(Token::new(current.clone(), start_line, start_col));
        current.clear();
    }
}

/// Split source into tokens, tracking line/column and keeping string literals
/// (with their escapes) intact. Newlines are emitted as `"\n"` tokens so the
/// parser can handle line comments.
pub(super) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut current_start_line = 0;
    let mut current_start_col = 0;
    let mut in_string = false;
    let mut prev_was_backslash = false;

    // Track current position (0-indexed)
    let mut line = 0;
    let mut col = 0;

    for ch in source.chars() {
        if in_string {
            current.push(ch);
            if ch == '"' && !prev_was_backslash {
                // Unescaped quote ends the string
                in_string = false;
                tokens.push(Token::new(
                    current.clone(),
                    current_start_line,
                    current_start_col,
                ));
                current.clear();
                prev_was_backslash = false;
            } else if ch == '\\' && !prev_was_backslash {
                // Start of escape sequence
                prev_was_backslash = true;
            } else {
                // Regular character or escaped character
                prev_was_backslash = false;
            }
            // Track newlines inside strings
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        } else if ch == '"' {
            flush_token(
                &mut tokens,
                &mut current,
                current_start_line,
                current_start_col,
            );
            in_string = true;
            current_start_line = line;
            current_start_col = col;
            current.push(ch);
            prev_was_backslash = false;
            col += 1;
        } else if ch.is_whitespace() {
            flush_token(
                &mut tokens,
                &mut current,
                current_start_line,
                current_start_col,
            );
            // Preserve newlines for comment handling
            if ch == '\n' {
                tokens.push(Token::new("\n".to_string(), line, col));
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        } else if "():;[]{},#".contains(ch) {
            // `#` is split out so that `#comment` (no space) tokenizes as
            // `#` + `comment` and the parser's `skip_comments` consumes
            // it as a line comment, matching Python/Bash/Ruby behaviour.
            // Without this split, `#comment` would accumulate into a
            // single identifier-shaped token and reach the parser as an
            // undefined word call.
            flush_token(
                &mut tokens,
                &mut current,
                current_start_line,
                current_start_col,
            );
            tokens.push(Token::new(ch.to_string(), line, col));
            col += 1;
        } else {
            if current.is_empty() {
                current_start_line = line;
                current_start_col = col;
            }
            current.push(ch);
            col += 1;
        }
    }

    // Check for unclosed string literal
    if in_string {
        // Return error by adding a special error token
        // The parser will handle this as a parse error
        tokens.push(Token::new(
            "<<<UNCLOSED_STRING>>>".to_string(),
            current_start_line,
            current_start_col,
        ));
    } else if !current.is_empty() {
        tokens.push(Token::new(current, current_start_line, current_start_col));
    }

    tokens
}
