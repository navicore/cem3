//! Simple parser for cem3 syntax
//!
//! Syntax:
//! ```
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
        let name = self.advance().ok_or("Expected word name after ':'")?.clone();

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
            let s = token
                .trim_start_matches('"')
                .trim_end_matches('"')
                .to_string();
            return Ok(Statement::StringLiteral(s));
        }

        // Otherwise it's a word call
        Ok(Statement::WordCall(token))
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

fn tokenize(source: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_string {
            current.push(ch);
            if ch == '"' {
                in_string = false;
                tokens.push(current.clone());
                current.clear();
            }
        } else if ch == '"' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            in_string = true;
            current.push(ch);
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

    if !current.is_empty() {
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
}
