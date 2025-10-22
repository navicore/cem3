//! Simple parser for cem3 syntax
//!
//! Syntax:
//! ```text
//! : word-name ( stack-effect )
//!   statement1
//!   statement2
//!   ... ;
//! ```

use crate::ast::{Program, Statement, WordDef};

pub struct Parser {
    tokens: Vec<String>,
    pos: usize,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let tokens = tokenize(source);
        Parser { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program::new();

        // Check for unclosed string error from tokenizer
        if self.tokens.iter().any(|t| t == "<<<UNCLOSED_STRING>>>") {
            return Err("Unclosed string literal - missing closing quote".to_string());
        }

        while !self.is_at_end() {
            self.skip_comments();
            if self.is_at_end() {
                break;
            }

            let word = self.parse_word_def()?;
            program.words.push(word);
        }

        Ok(program)
    }

    fn parse_word_def(&mut self) -> Result<WordDef, String> {
        // Expect ':'
        if !self.consume(":") {
            return Err(format!(
                "Expected ':' to start word definition, got '{}'",
                self.current()
            ));
        }

        // Get word name
        let name = self
            .advance()
            .ok_or("Expected word name after ':'")?
            .clone();

        // Skip stack effect comment if present: ( -- )
        if self.check("(") {
            self.skip_stack_effect()?;
        }

        // Parse body until ';'
        let mut body = Vec::new();
        while !self.check(";") {
            if self.is_at_end() {
                return Err(format!("Unexpected end of file in word '{}'", name));
            }

            // Skip newlines in body
            if self.check("\n") {
                self.advance();
                continue;
            }

            body.push(self.parse_statement()?);
        }

        // Consume ';'
        self.consume(";");

        Ok(WordDef { name, body })
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        let token = self.advance().ok_or("Unexpected end of file")?.clone();

        // Try to parse as integer literal
        if let Ok(n) = token.parse::<i64>() {
            return Ok(Statement::IntLiteral(n));
        }

        // Try to parse as boolean literal
        if token == "true" {
            return Ok(Statement::BoolLiteral(true));
        }
        if token == "false" {
            return Ok(Statement::BoolLiteral(false));
        }

        // Try to parse as string literal
        if token.starts_with('"') {
            let raw = token.trim_start_matches('"').trim_end_matches('"');
            let unescaped = unescape_string(raw)?;
            return Ok(Statement::StringLiteral(unescaped));
        }

        // Check for conditional
        if token == "if" {
            return self.parse_if();
        }

        // Otherwise it's a word call
        Ok(Statement::WordCall(token))
    }

    fn parse_if(&mut self) -> Result<Statement, String> {
        let mut then_branch = Vec::new();

        // Parse then branch until 'else' or 'then'
        loop {
            if self.is_at_end() {
                return Err("Unexpected end of file in 'if' statement".to_string());
            }

            // Skip newlines
            if self.check("\n") {
                self.advance();
                continue;
            }

            if self.check("else") {
                self.advance();
                // Parse else branch
                break;
            }

            if self.check("then") {
                self.advance();
                // End of if without else
                return Ok(Statement::If {
                    then_branch,
                    else_branch: None,
                });
            }

            then_branch.push(self.parse_statement()?);
        }

        // Parse else branch until 'then'
        let mut else_branch = Vec::new();
        loop {
            if self.is_at_end() {
                return Err("Unexpected end of file in 'else' branch".to_string());
            }

            // Skip newlines
            if self.check("\n") {
                self.advance();
                continue;
            }

            if self.check("then") {
                self.advance();
                return Ok(Statement::If {
                    then_branch,
                    else_branch: Some(else_branch),
                });
            }

            else_branch.push(self.parse_statement()?);
        }
    }

    fn skip_stack_effect(&mut self) -> Result<(), String> {
        self.consume("(");
        while !self.check(")") {
            if self.is_at_end() {
                return Err("Unclosed stack effect comment".to_string());
            }
            self.advance();
        }
        self.consume(")");
        Ok(())
    }

    fn skip_comments(&mut self) {
        loop {
            if self.check("#") {
                // Skip until newline
                while !self.is_at_end() && self.current() != "\n" {
                    self.advance();
                }
                if !self.is_at_end() {
                    self.advance(); // skip newline
                }
            } else if self.check("\n") {
                // Skip blank lines
                self.advance();
            } else {
                break;
            }
        }
    }

    fn check(&self, expected: &str) -> bool {
        if self.is_at_end() {
            return false;
        }
        self.current() == expected
    }

    fn consume(&mut self, expected: &str) -> bool {
        if self.check(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn current(&self) -> &str {
        if self.is_at_end() {
            ""
        } else {
            &self.tokens[self.pos]
        }
    }

    fn advance(&mut self) -> Option<&String> {
        if self.is_at_end() {
            None
        } else {
            let token = &self.tokens[self.pos];
            self.pos += 1;
            Some(token)
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }
}

/// Process escape sequences in a string literal
///
/// Supported escape sequences:
/// - `\"` -> `"`  (quote)
/// - `\\` -> `\`  (backslash)
/// - `\n` -> newline
/// - `\r` -> carriage return
/// - `\t` -> tab
///
/// # Errors
/// Returns error if an unknown escape sequence is encountered
fn unescape_string(s: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = s.chars();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some(c) => {
                    return Err(format!(
                        "Unknown escape sequence '\\{}' in string literal. \
                         Supported: \\\" \\\\ \\n \\r \\t",
                        c
                    ));
                }
                None => {
                    return Err("String ends with incomplete escape sequence '\\'".to_string());
                }
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut prev_was_backslash = false;

    for ch in source.chars() {
        if in_string {
            current.push(ch);
            if ch == '"' && !prev_was_backslash {
                // Unescaped quote ends the string
                in_string = false;
                tokens.push(current.clone());
                current.clear();
                prev_was_backslash = false;
            } else if ch == '\\' && !prev_was_backslash {
                // Start of escape sequence
                prev_was_backslash = true;
            } else {
                // Regular character or escaped character
                prev_was_backslash = false;
            }
        } else if ch == '"' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            in_string = true;
            current.push(ch);
            prev_was_backslash = false;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            // Preserve newlines for comment handling
            if ch == '\n' {
                tokens.push("\n".to_string());
            }
        } else if "():;".contains(ch) {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(ch.to_string());
        } else {
            current.push(ch);
        }
    }

    // Check for unclosed string literal
    if in_string {
        // Return error by adding a special error token
        // The parser will handle this as a parse error
        tokens.push("<<<UNCLOSED_STRING>>>".to_string());
    } else if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hello_world() {
        let source = r#"
: main ( -- )
  "Hello, World!" write_line ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].name, "main");
        assert_eq!(program.words[0].body.len(), 2);

        match &program.words[0].body[0] {
            Statement::StringLiteral(s) => assert_eq!(s, "Hello, World!"),
            _ => panic!("Expected StringLiteral"),
        }

        match &program.words[0].body[1] {
            Statement::WordCall(name) => assert_eq!(name, "write_line"),
            _ => panic!("Expected WordCall"),
        }
    }

    #[test]
    fn test_parse_with_numbers() {
        let source = ": add-example ( -- ) 2 3 add ;";

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words[0].body.len(), 3);
        assert_eq!(program.words[0].body[0], Statement::IntLiteral(2));
        assert_eq!(program.words[0].body[1], Statement::IntLiteral(3));
        assert_eq!(
            program.words[0].body[2],
            Statement::WordCall("add".to_string())
        );
    }

    #[test]
    fn test_parse_escaped_quotes() {
        let source = r#": main ( -- ) "Say \"hello\" there" write_line ;"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].body.len(), 2);

        match &program.words[0].body[0] {
            // Escape sequences should be processed: \" becomes actual quote
            Statement::StringLiteral(s) => assert_eq!(s, "Say \"hello\" there"),
            _ => panic!("Expected StringLiteral with escaped quotes"),
        }
    }

    #[test]
    fn test_escape_sequences() {
        let source = r#": main ( -- ) "Line 1\nLine 2\tTabbed" write_line ;"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        match &program.words[0].body[0] {
            Statement::StringLiteral(s) => assert_eq!(s, "Line 1\nLine 2\tTabbed"),
            _ => panic!("Expected StringLiteral"),
        }
    }

    #[test]
    fn test_unknown_escape_sequence() {
        let source = r#": main ( -- ) "Bad \x sequence" write_line ;"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown escape sequence"));
    }

    #[test]
    fn test_unclosed_string_literal() {
        let source = r#": main ( -- ) "unclosed string ;"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unclosed string literal"));
    }

    #[test]
    fn test_multiple_word_definitions() {
        let source = r#"
: double ( n -- n*2 )
  2 multiply ;

: quadruple ( n -- n*4 )
  double double ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 2);
        assert_eq!(program.words[0].name, "double");
        assert_eq!(program.words[1].name, "quadruple");
    }

    #[test]
    fn test_user_word_calling_user_word() {
        let source = r#"
: helper ( -- )
  "helper called" write_line ;

: main ( -- )
  helper ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 2);

        // Check main calls helper
        match &program.words[1].body[0] {
            Statement::WordCall(name) => assert_eq!(name, "helper"),
            _ => panic!("Expected WordCall to helper"),
        }
    }
}
