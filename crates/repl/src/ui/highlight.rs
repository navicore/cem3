//! Syntax Highlighting for Seq Code
//!
//! Tokenizes Seq source code for syntax highlighting in the TUI.
//! Returns tokens with their spans and semantic types.

use std::ops::Range;

/// A token type for syntax highlighting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
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
    /// Module paths: std:math
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
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
    pub text: String,
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

/// Keywords that control flow
const KEYWORDS: &[&str] = &[
    "if", "else", "loop", "break", "match", "return", "yield", "spawn", "send", "recv", "select",
];

/// Built-in stack manipulation words
const BUILTINS: &[&str] = &[
    // Stack ops
    "dup",
    "drop",
    "swap",
    "over",
    "rot",
    "nip",
    "tuck",
    "pick",
    "roll",
    // Arithmetic
    "add",
    "subtract",
    "multiply",
    "divide",
    "modulo",
    "negate",
    // Comparison
    "equals",
    "not-equals",
    "less-than",
    "greater-than",
    "less-or-equal",
    "greater-or-equal",
    // Logic
    "and",
    "or",
    "not",
    // Quotation
    "apply",
    "dip",
    "keep",
    "bi",
    "tri",
    // I/O
    "print",
    "println",
    "debug",
    // Type constructors
    "none",
    "some",
    "ok",
    "err",
];

/// Type names for stack effects
const TYPE_NAMES: &[&str] = &[
    "Int", "Float", "Bool", "String", "Char", "Unit", "Option", "Result", "Channel", "Strand",
];

/// Tokenize Seq source code for syntax highlighting
pub fn tokenize(source: &str) -> Vec<Token> {
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
            let text: String = chars[start..pos].iter().collect();
            tokens.push(Token::new(TokenKind::Whitespace, start, pos, text));
            continue;
        }

        // Comments: # to end of line
        if ch == '#' {
            while pos < chars.len() && chars[pos] != '\n' {
                pos += 1;
            }
            let text: String = chars[start..pos].iter().collect();
            tokens.push(Token::new(TokenKind::Comment, start, pos, text));
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
            let text: String = chars[start..pos].iter().collect();
            tokens.push(Token::new(TokenKind::String, start, pos, text));
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
                let text: String = chars[start..pos].iter().collect();
                tokens.push(Token::new(TokenKind::ModulePath, start, pos, text));
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
            let is_negative = ch == '-';
            if is_negative {
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
                let text: String = chars[start..pos].iter().collect();
                tokens.push(Token::new(TokenKind::Float, start, pos, text));
            } else {
                let text: String = chars[start..pos].iter().collect();
                tokens.push(Token::new(TokenKind::Integer, start, pos, text));
            }
            continue;
        }

        // Identifiers and keywords
        if ch.is_alphabetic() || ch == '_' || ch == '-' {
            while pos < chars.len() {
                let c = chars[pos];
                if c.is_alphanumeric() || c == '_' || c == '-' || c == ':' {
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

        // Unknown character
        pos += 1;
        tokens.push(Token::new(TokenKind::Unknown, start, pos, ch.to_string()));
    }

    tokens
}

/// Classify an identifier as keyword, builtin, type, or regular identifier
fn classify_identifier(text: &str) -> TokenKind {
    // Check for booleans
    if text == "true" || text == "false" {
        return TokenKind::Boolean;
    }

    // Check for include
    if text == "include" {
        return TokenKind::Include;
    }

    // Check for keywords
    if KEYWORDS.contains(&text) {
        return TokenKind::Keyword;
    }

    // Check for builtins
    if BUILTINS.contains(&text) {
        return TokenKind::Builtin;
    }

    // Check for type names (capitalized)
    if TYPE_NAMES.contains(&text) {
        return TokenKind::TypeName;
    }

    // Check for module paths (contains :)
    if text.contains(':') {
        return TokenKind::ModulePath;
    }

    TokenKind::Identifier
}

/// Get tokens excluding whitespace (useful for rendering)
#[allow(dead_code)]
pub fn tokenize_visible(source: &str) -> Vec<Token> {
    tokenize(source)
        .into_iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let tokens = tokenize_visible(": square dup multiply ;");
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0].kind, TokenKind::DefMarker);
        assert_eq!(tokens[1].kind, TokenKind::Identifier); // word name
        assert_eq!(tokens[2].kind, TokenKind::Builtin);
        assert_eq!(tokens[3].kind, TokenKind::Builtin);
        assert_eq!(tokens[4].kind, TokenKind::DefEnd);
    }

    #[test]
    fn test_tokenize_keywords() {
        let tokens = tokenize_visible("if else loop break");
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
        let tokens = tokenize_visible("[ dup multiply ]");
        assert_eq!(tokens[0].kind, TokenKind::Quotation);
        assert_eq!(tokens[1].kind, TokenKind::Builtin);
        assert_eq!(tokens[2].kind, TokenKind::Builtin);
        assert_eq!(tokens[3].kind, TokenKind::Quotation);
    }

    #[test]
    fn test_tokenize_include() {
        let tokens = tokenize_visible("include std:math");
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
