//! Syntax Highlighting for Seq Code
//!
//! Tokenizes Seq source code for syntax highlighting in the TUI.
//! Returns tokens with their spans and semantic types.

use std::ops::Range;

/// A token type for syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// Keywords: if, else, loop, break, etc.
    Keyword,
    /// Built-in words: dup, drop, swap, over, etc.
    Builtin,
    /// Word definition marker (:)
    DefMarker,
    /// Definition end marker (;)
    DefEnd,
    /// Integer literals: 42, -17
    Integer,
    /// Float literals: 3.14, -2.5
    Float,
    /// Boolean literals: true, false
    Boolean,
    /// String literals: "hello"
    String,
    /// Comments: # ...
    Comment,
    /// Type annotations in stack effects: Int, Float, Bool
    TypeName,
    /// Stack effect delimiters: ( ) --
    StackEffect,
    /// Quotation brackets: [ ]
    Quotation,
    /// Include statements: include
    Include,
    /// Module paths: std:imath
    ModulePath,
    /// Regular identifiers/word references
    Identifier,
    /// Whitespace (usually not rendered specially)
    Whitespace,
    /// Unknown/error tokens
    Unknown,
}

/// A highlighted token with its position
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Range<usize>,
    pub(crate) text: String,
}

impl Token {
    fn new(kind: TokenKind, start: usize, end: usize, text: impl Into<String>) -> Self {
        Self {
            kind,
            span: start..end,
            text: text.into(),
        }
    }
}

/// Real language keywords recognized by the parser.
///
/// Intentionally narrow — only tokens the parser itself special-cases.
/// `true` / `false` and `include` are handled as their own `TokenKind`s
/// elsewhere in `classify_identifier`. Stdlib words (`spawn`, `send`,
/// `recv`, `yield`, …) are highlighted as `Builtin` via the compiler
/// lookup. `loop`, `break`, and `return` used to live here but aren't
/// parser keywords nor compiler builtins — they were stale copies from
/// an earlier plan and are intentionally absent.
const KEYWORDS: &[&str] = &["if", "else", "then", "match", "end", "union"];

/// User-nameable base types in stack effects (see `docs/GRAMMAR.md`).
///
/// The compiler also has `Type::Symbol` and `Type::Channel` internally,
/// but those aren't names users write in source — they appear only as
/// inferred types in compiler diagnostics.
const TYPE_NAMES: &[&str] = &["Int", "Float", "Bool", "String"];

/// Push a token spanning `start..end` of `chars` to `tokens`.
fn push_range(tokens: &mut Vec<Token>, chars: &[char], start: usize, end: usize, kind: TokenKind) {
    let text: String = chars[start..end].iter().collect();
    tokens.push(Token::new(kind, start, end, text));
}

/// Lex a number literal at `start` (a digit, or `-` followed by a digit).
/// Returns the end index and whether it parsed as `Integer` or `Float`.
fn lex_number(chars: &[char], start: usize) -> (usize, TokenKind) {
    let mut pos = start;
    // Optional leading minus
    if chars[pos] == '-' {
        pos += 1;
    }

    // Consume digits
    while pos < chars.len() && chars[pos].is_ascii_digit() {
        pos += 1;
    }

    // Check for float
    if pos < chars.len()
        && chars[pos] == '.'
        && pos + 1 < chars.len()
        && chars[pos + 1].is_ascii_digit()
    {
        pos += 1; // Skip dot
        while pos < chars.len() && chars[pos].is_ascii_digit() {
            pos += 1;
        }
        (pos, TokenKind::Float)
    } else {
        (pos, TokenKind::Integer)
    }
}

/// Tokenize Seq source code for syntax highlighting
pub(crate) fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut pos = 0;

    while pos < chars.len() {
        let start = pos;
        let ch = chars[pos];

        // Whitespace
        if ch.is_whitespace() {
            while pos < chars.len() && chars[pos].is_whitespace() {
                pos += 1;
            }
            push_range(&mut tokens, &chars, start, pos, TokenKind::Whitespace);
            continue;
        }

        // Comments: # to end of line
        if ch == '#' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
            }
            push_range(&mut tokens, &chars, start, pos, TokenKind::Comment);
            continue;
        }

        // String literals
        if ch == '"' {
            pos += 1; // Skip opening quote
            while pos < chars.len() && chars[pos] != '"' {
                if chars[pos] == '\\' && pos + 1 < chars.len() {
                    pos += 2; // Skip escape sequence
                } else {
                    pos += 1;
                }
            }
            if pos < chars.len() {
                pos += 1; // Skip closing quote
            }
            push_range(&mut tokens, &chars, start, pos, TokenKind::String);
            continue;
        }

        // Quotation brackets
        if ch == '[' || ch == ']' {
            pos += 1;
            tokens.push(Token::new(TokenKind::Quotation, start, pos, ch.to_string()));
            continue;
        }

        // Stack effect parentheses
        if ch == '(' || ch == ')' {
            pos += 1;
            tokens.push(Token::new(
                TokenKind::StackEffect,
                start,
                pos,
                ch.to_string(),
            ));
            continue;
        }

        // Definition markers
        if ch == ':' {
            pos += 1;
            // Check if this is a module path separator (word:subword)
            if start > 0 && !chars[start - 1].is_whitespace() {
                // Part of a module path
                push_range(&mut tokens, &chars, start, pos, TokenKind::ModulePath);
            } else {
                tokens.push(Token::new(TokenKind::DefMarker, start, pos, ":"));
            }
            continue;
        }

        if ch == ';' {
            pos += 1;
            tokens.push(Token::new(TokenKind::DefEnd, start, pos, ";"));
            continue;
        }

        // Stack effect separator
        if ch == '-' && pos + 1 < chars.len() && chars[pos + 1] == '-' {
            pos += 2;
            tokens.push(Token::new(TokenKind::StackEffect, start, pos, "--"));
            continue;
        }

        // Numbers (including negative)
        if ch.is_ascii_digit()
            || (ch == '-' && pos + 1 < chars.len() && chars[pos + 1].is_ascii_digit())
        {
            let (end, kind) = lex_number(&chars, start);
            push_range(&mut tokens, &chars, start, end, kind);
            pos = end;
            continue;
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' || ch == '-' {
            while pos < chars.len() {
                let c = chars[pos];
                if c.is_alphanumeric() || c == '_' || c == '-' || c == ':' || c == '.' {
                    pos += 1;
                } else {
                    break;
                }
            }

            let text: String = chars[start..pos].iter().collect();
            let kind = classify_identifier(&text);
            tokens.push(Token::new(kind, start, pos, text));
            continue;
        }

        // Arithmetic sugar operators: +, *, /, %, =, <, >, <=, >=, <>
        // Note: `-` is omitted — it's handled by the identifier path (line 250)
        // since it can also start negative numbers and hyphenated words.
        if matches!(ch, '+' | '*' | '/' | '%' | '=' | '<' | '>') {
            pos += 1;
            // Check for two-character operators: <=, >=, <>
            if pos < chars.len() {
                let next = chars[pos];
                if (ch == '<' && (next == '=' || next == '>')) || (ch == '>' && next == '=') {
                    pos += 1;
                }
            }
            push_range(&mut tokens, &chars, start, pos, TokenKind::Builtin);
            continue;
        }

        // Unknown character
        pos += 1;
        tokens.push(Token::new(TokenKind::Unknown, start, pos, ch.to_string()));
    }

    tokens
}

/// Classify an identifier as keyword, builtin, type, or regular identifier
fn classify_identifier(text: &str) -> TokenKind {
    if text == "true" || text == "false" {
        return TokenKind::Boolean;
    }
    if text == "include" {
        return TokenKind::Include;
    }
    if KEYWORDS.contains(&text) {
        return TokenKind::Keyword;
    }
    // Ask the compiler whether this is a real builtin — single source of truth.
    if seqc::builtins::builtin_signature(text).is_some() {
        return TokenKind::Builtin;
    }
    if TYPE_NAMES.contains(&text) {
        return TokenKind::TypeName;
    }
    if text.contains(':') {
        return TokenKind::ModulePath;
    }
    TokenKind::Identifier
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokenize, dropping whitespace tokens — convenient for assertions.
    fn tokenize_visible(source: &str) -> Vec<Token> {
        tokenize(source)
            .into_iter()
            .filter(|t| t.kind != TokenKind::Whitespace)
            .collect()
    }

    #[test]
    fn test_tokenize_numbers() {
        let tokens = tokenize_visible("42 3.14 -5");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].kind, TokenKind::Integer);
        assert_eq!(tokens[0].text, "42");
        assert_eq!(tokens[1].kind, TokenKind::Float);
        assert_eq!(tokens[1].text, "3.14");
        assert_eq!(tokens[2].kind, TokenKind::Integer);
        assert_eq!(tokens[2].text, "-5");
    }

    #[test]
    fn test_tokenize_string() {
        let tokens = tokenize_visible("\"hello world\"");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String);
        assert_eq!(tokens[0].text, "\"hello world\"");
    }

    #[test]
    fn test_tokenize_comment() {
        let tokens = tokenize_visible("42 # this is a comment");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].kind, TokenKind::Integer);
        assert_eq!(tokens[1].kind, TokenKind::Comment);
    }

    #[test]
    fn test_tokenize_definition() {
        let tokens = tokenize_visible(": square dup i.multiply ;");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::DefMarker);
        assert_eq!(tokens[1].kind, TokenKind::Identifier); // word name
        assert_eq!(tokens[2].kind, TokenKind::Builtin);
        assert_eq!(tokens[3].kind, TokenKind::Builtin);
        assert_eq!(tokens[4].kind, TokenKind::DefEnd);
    }

    #[test]
    fn test_tokenize_keywords() {
        let tokens = tokenize_visible("if else match end");
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Keyword));
    }

    #[test]
    fn test_tokenize_builtins() {
        let tokens = tokenize_visible("dup drop swap over");
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Builtin));
    }

    #[test]
    fn test_tokenize_booleans() {
        let tokens = tokenize_visible("true false");
        assert!(tokens.iter().all(|t| t.kind == TokenKind::Boolean));
    }

    #[test]
    fn test_tokenize_stack_effect() {
        let tokens = tokenize_visible("( Int Int -- Int )");
        assert_eq!(tokens[0].kind, TokenKind::StackEffect); // (
        assert_eq!(tokens[1].kind, TokenKind::TypeName); // Int
        assert_eq!(tokens[2].kind, TokenKind::TypeName); // Int
        assert_eq!(tokens[3].kind, TokenKind::StackEffect); // --
        assert_eq!(tokens[4].kind, TokenKind::TypeName); // Int
        assert_eq!(tokens[5].kind, TokenKind::StackEffect); // )
    }

    #[test]
    fn test_tokenize_quotation() {
        let tokens = tokenize_visible("[ dup i.multiply ]");
        assert_eq!(tokens[0].kind, TokenKind::Quotation);
        assert_eq!(tokens[1].kind, TokenKind::Builtin);
        assert_eq!(tokens[2].kind, TokenKind::Builtin);
        assert_eq!(tokens[3].kind, TokenKind::Quotation);
    }

    #[test]
    fn test_tokenize_include() {
        let tokens = tokenize_visible("include std:imath");
        assert_eq!(tokens[0].kind, TokenKind::Include);
        assert_eq!(tokens[1].kind, TokenKind::ModulePath);
    }

    #[test]
    fn test_span_positions() {
        let source = "42 dup";
        let tokens = tokenize(source);

        // "42" at 0..2
        assert_eq!(tokens[0].span, 0..2);
        // " " at 2..3
        assert_eq!(tokens[1].span, 2..3);
        // "dup" at 3..6
        assert_eq!(tokens[2].span, 3..6);
    }

    #[test]
    fn test_escaped_string() {
        let tokens = tokenize_visible(r#""hello \"world\"""#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::String);
    }
}
