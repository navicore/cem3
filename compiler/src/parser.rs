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
use crate::types::{Effect, StackType, Type};

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

        // Parse stack effect if present: ( ..a Int -- ..a Bool )
        let effect = if self.check("(") {
            Some(self.parse_stack_effect()?)
        } else {
            None
        };

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

        Ok(WordDef { name, effect, body })
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

        // Check for quotation
        if token == "[" {
            return self.parse_quotation();
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

    fn parse_quotation(&mut self) -> Result<Statement, String> {
        let mut body = Vec::new();

        // Parse statements until ']'
        loop {
            if self.is_at_end() {
                return Err("Unexpected end of file in quotation".to_string());
            }

            // Skip newlines
            if self.check("\n") {
                self.advance();
                continue;
            }

            if self.check("]") {
                self.advance();
                return Ok(Statement::Quotation(body));
            }

            body.push(self.parse_statement()?);
        }
    }

    /// Parse a stack effect declaration: ( ..a Int -- ..a Bool )
    fn parse_stack_effect(&mut self) -> Result<Effect, String> {
        // Consume '('
        if !self.consume("(") {
            return Err("Expected '(' to start stack effect".to_string());
        }

        // Parse input stack types
        let mut input_types = Vec::new();
        let mut input_row_var = None;

        while !self.check("--") && !self.check(")") {
            if self.is_at_end() {
                return Err("Unclosed stack effect declaration".to_string());
            }

            let token = self
                .advance()
                .ok_or("Unexpected end in stack effect")?
                .clone();

            // Check for row variable: ..name
            if token.starts_with("..") {
                let var_name = token.trim_start_matches("..").to_string();
                if var_name.is_empty() {
                    return Err("Row variable must have a name after '..'".to_string());
                }
                input_row_var = Some(var_name);
            } else if token == "[" {
                // Parse quotation type: [inputs -- outputs]
                input_types.push(self.parse_quotation_type()?);
            } else {
                // Parse as concrete type
                input_types.push(self.parse_type(&token)?);
            }
        }

        // Consume '--'
        if !self.consume("--") {
            return Err("Expected '--' separator in stack effect".to_string());
        }

        // Parse output stack types
        let mut output_types = Vec::new();
        let mut output_row_var = None;

        while !self.check(")") {
            if self.is_at_end() {
                return Err("Unclosed stack effect declaration".to_string());
            }

            let token = self
                .advance()
                .ok_or("Unexpected end in stack effect")?
                .clone();

            // Check for row variable: ..name
            if token.starts_with("..") {
                let var_name = token.trim_start_matches("..").to_string();
                if var_name.is_empty() {
                    return Err("Row variable must have a name after '..'".to_string());
                }
                output_row_var = Some(var_name);
            } else if token == "[" {
                // Parse quotation type: [inputs -- outputs]
                output_types.push(self.parse_quotation_type()?);
            } else {
                // Parse as concrete type
                output_types.push(self.parse_type(&token)?);
            }
        }

        // Consume ')'
        if !self.consume(")") {
            return Err("Expected ')' to end stack effect".to_string());
        }

        // Build input StackType
        let inputs = self.build_stack_type(input_row_var, input_types);

        // Build output StackType
        let outputs = self.build_stack_type(output_row_var, output_types);

        Ok(Effect::new(inputs, outputs))
    }

    /// Parse a single type token into a Type
    fn parse_type(&self, token: &str) -> Result<Type, String> {
        match token {
            "Int" => Ok(Type::Int),
            "Bool" => Ok(Type::Bool),
            "String" => Ok(Type::String),
            _ => {
                // Check if it's a type variable (starts with uppercase)
                if let Some(first_char) = token.chars().next() {
                    if first_char.is_uppercase() {
                        Ok(Type::Var(token.to_string()))
                    } else {
                        Err(format!(
                            "Unknown type: '{}'. Expected Int, Bool, String, or a type variable (uppercase)",
                            token
                        ))
                    }
                } else {
                    Err(format!("Invalid type: '{}'", token))
                }
            }
        }
    }

    /// Parse a quotation type: [inputs -- outputs]
    /// Note: The opening '[' has already been consumed
    fn parse_quotation_type(&mut self) -> Result<Type, String> {
        // Parse input stack types
        let mut input_types = Vec::new();
        let mut input_row_var = None;

        while !self.check("--") && !self.check("]") {
            if self.is_at_end() {
                return Err("Unclosed quotation type - expected '--' or ']'".to_string());
            }

            let token = self
                .advance()
                .ok_or("Unexpected end in quotation type")?
                .clone();

            // Check for row variable: ..name
            if token.starts_with("..") {
                let var_name = token.trim_start_matches("..").to_string();
                if var_name.is_empty() {
                    return Err("Row variable must have a name after '..'".to_string());
                }
                input_row_var = Some(var_name);
            } else if token == "[" {
                // Nested quotation type
                input_types.push(self.parse_quotation_type()?);
            } else {
                // Parse as concrete type
                input_types.push(self.parse_type(&token)?);
            }
        }

        // Consume '--' if present (may be empty effect: [ -- ...])
        let has_separator = self.consume("--");

        // Parse output stack types
        let mut output_types = Vec::new();
        let mut output_row_var = None;

        if has_separator {
            while !self.check("]") {
                if self.is_at_end() {
                    return Err("Unclosed quotation type - expected ']'".to_string());
                }

                let token = self
                    .advance()
                    .ok_or("Unexpected end in quotation type")?
                    .clone();

                // Check for row variable: ..name
                if token.starts_with("..") {
                    let var_name = token.trim_start_matches("..").to_string();
                    if var_name.is_empty() {
                        return Err("Row variable must have a name after '..'".to_string());
                    }
                    output_row_var = Some(var_name);
                } else if token == "[" {
                    // Nested quotation type
                    output_types.push(self.parse_quotation_type()?);
                } else {
                    // Parse as concrete type
                    output_types.push(self.parse_type(&token)?);
                }
            }
        }

        // Consume ']'
        if !self.consume("]") {
            return Err("Expected ']' to end quotation type".to_string());
        }

        // Build input StackType
        let inputs = self.build_stack_type(input_row_var, input_types);

        // Build output StackType
        let outputs = self.build_stack_type(output_row_var, output_types);

        Ok(Type::Quotation(Box::new(Effect::new(inputs, outputs))))
    }

    /// Build a StackType from an optional row variable and a list of types
    /// Example: row_var="a", types=[Int, Bool] => RowVar("a") with Int on top of Bool
    ///
    /// IMPORTANT: If no row variable is given but types exist, auto-generate one.
    /// This provides implicit row polymorphism: ( String -- String ) means ( ..rest String -- ..rest String )
    fn build_stack_type(&self, row_var: Option<String>, types: Vec<Type>) -> StackType {
        let base = match row_var {
            Some(name) => StackType::RowVar(name),
            None => {
                // If we have types but no explicit row variable, auto-generate one
                // This makes ( String -- String ) implicitly row-polymorphic
                if !types.is_empty() {
                    StackType::RowVar("rest".to_string())
                } else {
                    // Only use Empty for truly empty stacks: ( -- ) or ( -- Int )
                    StackType::Empty
                }
            }
        };

        // Push types onto the stack (bottom to top order)
        types.into_iter().fold(base, |stack, ty| stack.push(ty))
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
        } else if "():;[]".contains(ch) {
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
: double ( Int -- Int )
  2 multiply ;

: quadruple ( Int -- Int )
  double double ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 2);
        assert_eq!(program.words[0].name, "double");
        assert_eq!(program.words[1].name, "quadruple");

        // Verify stack effects were parsed
        assert!(program.words[0].effect.is_some());
        assert!(program.words[1].effect.is_some());
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

    #[test]
    fn test_parse_simple_stack_effect() {
        // Test: ( Int -- Bool )
        // With implicit row polymorphism, this becomes: ( ..rest Int -- ..rest Bool )
        let source = ": test ( Int -- Bool ) 1 ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        let word = &program.words[0];
        assert!(word.effect.is_some());

        let effect = word.effect.as_ref().unwrap();

        // Input: Int on RowVar("rest") (implicit row polymorphism)
        assert_eq!(
            effect.inputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("rest".to_string())),
                top: Type::Int
            }
        );

        // Output: Bool on RowVar("rest") (implicit row polymorphism)
        assert_eq!(
            effect.outputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("rest".to_string())),
                top: Type::Bool
            }
        );
    }

    #[test]
    fn test_parse_row_polymorphic_stack_effect() {
        // Test: ( ..a Int -- ..a Bool )
        let source = ": test ( ..a Int -- ..a Bool ) 1 ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        let word = &program.words[0];
        assert!(word.effect.is_some());

        let effect = word.effect.as_ref().unwrap();

        // Input: Int on RowVar("a")
        assert_eq!(
            effect.inputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("a".to_string())),
                top: Type::Int
            }
        );

        // Output: Bool on RowVar("a")
        assert_eq!(
            effect.outputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("a".to_string())),
                top: Type::Bool
            }
        );
    }

    #[test]
    fn test_parse_multiple_types_stack_effect() {
        // Test: ( Int String -- Bool )
        // With implicit row polymorphism: ( ..rest Int String -- ..rest Bool )
        let source = ": test ( Int String -- Bool ) 1 ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input: String on Int on RowVar("rest")
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Int);
        assert_eq!(rest2, StackType::RowVar("rest".to_string()));

        // Output: Bool on RowVar("rest") (implicit row polymorphism)
        assert_eq!(
            effect.outputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("rest".to_string())),
                top: Type::Bool
            }
        );
    }

    #[test]
    fn test_parse_type_variable() {
        // Test: ( ..a T -- ..a T T ) for dup
        let source = ": dup ( ..a T -- ..a T T ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input: T on RowVar("a")
        assert_eq!(
            effect.inputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("a".to_string())),
                top: Type::Var("T".to_string())
            }
        );

        // Output: T on T on RowVar("a")
        let (rest, top) = effect.outputs.clone().pop().unwrap();
        assert_eq!(top, Type::Var("T".to_string()));
        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Var("T".to_string()));
        assert_eq!(rest2, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_parse_empty_stack_effect() {
        // Test: ( -- )
        let source = ": test ( -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        assert_eq!(effect.inputs, StackType::Empty);
        assert_eq!(effect.outputs, StackType::Empty);
    }

    #[test]
    fn test_parse_invalid_type() {
        // Test invalid type (lowercase, not a row var)
        let source = ": test ( invalid -- Bool ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown type"));
    }

    #[test]
    fn test_parse_unclosed_stack_effect() {
        // Test unclosed stack effect - parser tries to parse all tokens until ')' or EOF
        // In this case, it encounters "body" which is an invalid type
        let source = ": test ( Int -- Bool body ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        // Parser will try to parse "body" as a type and fail
        assert!(err_msg.contains("Unknown type"));
    }

    #[test]
    fn test_parse_no_stack_effect() {
        // Test word without stack effect (should still work)
        let source = ": test 1 2 add ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert!(program.words[0].effect.is_none());
    }

    #[test]
    fn test_parse_simple_quotation() {
        let source = r#"
: test ( -- Quot )
  [ 1 add ] ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].name, "test");
        assert_eq!(program.words[0].body.len(), 1);

        match &program.words[0].body[0] {
            Statement::Quotation(body) => {
                assert_eq!(body.len(), 2);
                assert_eq!(body[0], Statement::IntLiteral(1));
                assert_eq!(body[1], Statement::WordCall("add".to_string()));
            }
            _ => panic!("Expected Quotation statement"),
        }
    }

    #[test]
    fn test_parse_empty_quotation() {
        let source = ": test [ ] ;";

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);

        match &program.words[0].body[0] {
            Statement::Quotation(body) => {
                assert_eq!(body.len(), 0);
            }
            _ => panic!("Expected Quotation statement"),
        }
    }

    #[test]
    fn test_parse_quotation_with_call() {
        let source = r#"
: test ( -- )
  5 [ 1 add ] call ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].body.len(), 3);

        assert_eq!(program.words[0].body[0], Statement::IntLiteral(5));

        match &program.words[0].body[1] {
            Statement::Quotation(body) => {
                assert_eq!(body.len(), 2);
            }
            _ => panic!("Expected Quotation"),
        }

        assert_eq!(
            program.words[0].body[2],
            Statement::WordCall("call".to_string())
        );
    }

    #[test]
    fn test_parse_nested_quotation() {
        let source = ": test [ [ 1 add ] call ] ;";

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);

        match &program.words[0].body[0] {
            Statement::Quotation(outer_body) => {
                assert_eq!(outer_body.len(), 2);

                match &outer_body[0] {
                    Statement::Quotation(inner_body) => {
                        assert_eq!(inner_body.len(), 2);
                        assert_eq!(inner_body[0], Statement::IntLiteral(1));
                        assert_eq!(inner_body[1], Statement::WordCall("add".to_string()));
                    }
                    _ => panic!("Expected nested Quotation"),
                }

                assert_eq!(outer_body[1], Statement::WordCall("call".to_string()));
            }
            _ => panic!("Expected Quotation"),
        }
    }

    #[test]
    fn test_parse_while_with_quotations() {
        let source = r#"
: countdown ( Int -- )
  [ dup 0 > ] [ 1 subtract ] while drop ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].body.len(), 4);

        // First quotation: [ dup 0 > ]
        match &program.words[0].body[0] {
            Statement::Quotation(pred) => {
                assert_eq!(pred.len(), 3);
                assert_eq!(pred[0], Statement::WordCall("dup".to_string()));
                assert_eq!(pred[1], Statement::IntLiteral(0));
                assert_eq!(pred[2], Statement::WordCall(">".to_string()));
            }
            _ => panic!("Expected predicate quotation"),
        }

        // Second quotation: [ 1 subtract ]
        match &program.words[0].body[1] {
            Statement::Quotation(body) => {
                assert_eq!(body.len(), 2);
                assert_eq!(body[0], Statement::IntLiteral(1));
                assert_eq!(body[1], Statement::WordCall("subtract".to_string()));
            }
            _ => panic!("Expected body quotation"),
        }

        // while call
        assert_eq!(
            program.words[0].body[2],
            Statement::WordCall("while".to_string())
        );

        // drop
        assert_eq!(
            program.words[0].body[3],
            Statement::WordCall("drop".to_string())
        );
    }
}
