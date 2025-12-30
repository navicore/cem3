//! Simple parser for Seq syntax
//!
//! Syntax:
//! ```text
//! : word-name ( stack-effect )
//!   statement1
//!   statement2
//!   ... ;
//! ```

use crate::ast::{
    Include, MatchArm, Pattern, Program, SourceLocation, Statement, UnionDef, UnionField,
    UnionVariant, WordDef,
};
use crate::types::{Effect, SideEffect, StackType, Type};

/// A token with source position information
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

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    /// Counter for assigning unique IDs to quotations
    /// Used by the typechecker to track inferred types
    next_quotation_id: usize,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let tokens = tokenize(source);
        Parser {
            tokens,
            pos: 0,
            next_quotation_id: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Program, String> {
        let mut program = Program::new();

        // Check for unclosed string error from tokenizer
        if let Some(error_token) = self.tokens.iter().find(|t| *t == "<<<UNCLOSED_STRING>>>") {
            return Err(format!(
                "Unclosed string literal at line {}, column {} - missing closing quote",
                error_token.line + 1, // 1-indexed for user display
                error_token.column + 1
            ));
        }

        while !self.is_at_end() {
            self.skip_comments();
            if self.is_at_end() {
                break;
            }

            // Check for include statement
            if self.check("include") {
                let include = self.parse_include()?;
                program.includes.push(include);
                continue;
            }

            // Check for union definition
            if self.check("union") {
                let union_def = self.parse_union_def()?;
                program.unions.push(union_def);
                continue;
            }

            let word = self.parse_word_def()?;
            program.words.push(word);
        }

        Ok(program)
    }

    /// Parse an include statement:
    ///   include std:http     -> Include::Std("http")
    ///   include ffi:readline -> Include::Ffi("readline")
    ///   include "my-utils"   -> Include::Relative("my-utils")
    fn parse_include(&mut self) -> Result<Include, String> {
        self.consume("include");

        let token = self
            .advance()
            .ok_or("Expected module name after 'include'")?
            .clone();

        // Check for std: prefix (tokenizer splits this into "std", ":", "name")
        if token == "std" {
            // Expect : token
            if !self.consume(":") {
                return Err("Expected ':' after 'std' in include statement".to_string());
            }
            // Get the module name
            let name = self
                .advance()
                .ok_or("Expected module name after 'std:'")?
                .clone();
            return Ok(Include::Std(name));
        }

        // Check for ffi: prefix
        if token == "ffi" {
            // Expect : token
            if !self.consume(":") {
                return Err("Expected ':' after 'ffi' in include statement".to_string());
            }
            // Get the library name
            let name = self
                .advance()
                .ok_or("Expected library name after 'ffi:'")?
                .clone();
            return Ok(Include::Ffi(name));
        }

        // Check for quoted string (relative path)
        if token.starts_with('"') && token.ends_with('"') {
            let path = token.trim_start_matches('"').trim_end_matches('"');
            return Ok(Include::Relative(path.to_string()));
        }

        Err(format!(
            "Invalid include syntax '{}'. Use 'include std:name', 'include ffi:lib', or 'include \"path\"'",
            token
        ))
    }

    /// Parse a union type definition:
    ///   union Message {
    ///     Get { response-chan: Int }
    ///     Increment { response-chan: Int }
    ///     Report { op: Int, delta: Int, total: Int }
    ///   }
    fn parse_union_def(&mut self) -> Result<UnionDef, String> {
        // Capture start line from 'union' token
        let start_line = self.current_token().map(|t| t.line).unwrap_or(0);

        // Consume 'union' keyword
        self.consume("union");

        // Get union name (must start with uppercase)
        let name = self
            .advance()
            .ok_or("Expected union name after 'union'")?
            .clone();

        if !name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            return Err(format!(
                "Union name '{}' must start with an uppercase letter",
                name
            ));
        }

        // Skip comments and newlines
        self.skip_comments();

        // Expect '{'
        if !self.consume("{") {
            return Err(format!(
                "Expected '{{' after union name '{}', got '{}'",
                name,
                self.current()
            ));
        }

        // Parse variants until '}'
        let mut variants = Vec::new();
        loop {
            self.skip_comments();

            if self.check("}") {
                break;
            }

            if self.is_at_end() {
                return Err(format!("Unexpected end of file in union '{}'", name));
            }

            variants.push(self.parse_union_variant()?);
        }

        // Capture end line from '}' token before consuming
        let end_line = self.current_token().map(|t| t.line).unwrap_or(start_line);

        // Consume '}'
        self.consume("}");

        if variants.is_empty() {
            return Err(format!("Union '{}' must have at least one variant", name));
        }

        // Check for duplicate variant names
        let mut seen_variants = std::collections::HashSet::new();
        for variant in &variants {
            if !seen_variants.insert(&variant.name) {
                return Err(format!(
                    "Duplicate variant name '{}' in union '{}'",
                    variant.name, name
                ));
            }
        }

        Ok(UnionDef {
            name,
            variants,
            source: Some(SourceLocation::span(
                std::path::PathBuf::new(),
                start_line,
                end_line,
            )),
        })
    }

    /// Parse a single union variant:
    ///   Get { response-chan: Int }
    ///   or just: Empty (no fields)
    fn parse_union_variant(&mut self) -> Result<UnionVariant, String> {
        let start_line = self.current_token().map(|t| t.line).unwrap_or(0);

        // Get variant name (must start with uppercase)
        let name = self.advance().ok_or("Expected variant name")?.clone();

        if !name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            return Err(format!(
                "Variant name '{}' must start with an uppercase letter",
                name
            ));
        }

        self.skip_comments();

        // Check for optional fields
        let fields = if self.check("{") {
            self.consume("{");
            let fields = self.parse_union_fields()?;
            if !self.consume("}") {
                return Err(format!("Expected '}}' after variant '{}' fields", name));
            }
            fields
        } else {
            Vec::new()
        };

        Ok(UnionVariant {
            name,
            fields,
            source: Some(SourceLocation::new(std::path::PathBuf::new(), start_line)),
        })
    }

    /// Parse union fields: name: Type, name: Type, ...
    fn parse_union_fields(&mut self) -> Result<Vec<UnionField>, String> {
        let mut fields = Vec::new();

        loop {
            self.skip_comments();

            if self.check("}") {
                break;
            }

            // Get field name
            let field_name = self.advance().ok_or("Expected field name")?.clone();

            // Expect ':'
            if !self.consume(":") {
                return Err(format!(
                    "Expected ':' after field name '{}', got '{}'",
                    field_name,
                    self.current()
                ));
            }

            // Get type name
            let type_name = self
                .advance()
                .ok_or("Expected type name after ':'")?
                .clone();

            fields.push(UnionField {
                name: field_name,
                type_name,
            });

            // Optional comma separator
            self.skip_comments();
            self.consume(",");
        }

        // Check for duplicate field names
        let mut seen_fields = std::collections::HashSet::new();
        for field in &fields {
            if !seen_fields.insert(&field.name) {
                return Err(format!("Duplicate field name '{}' in variant", field.name));
            }
        }

        Ok(fields)
    }

    fn parse_word_def(&mut self) -> Result<WordDef, String> {
        // Capture start line from ':' token
        let start_line = self.current_token().map(|t| t.line).unwrap_or(0);

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

            // Skip comments and newlines in body
            self.skip_comments();
            if self.check(";") {
                break;
            }

            body.push(self.parse_statement()?);
        }

        // Capture end line from ';' token before consuming
        let end_line = self.current_token().map(|t| t.line).unwrap_or(start_line);

        // Consume ';'
        self.consume(";");

        Ok(WordDef {
            name,
            effect,
            body,
            source: Some(crate::ast::SourceLocation::span(
                std::path::PathBuf::new(),
                start_line,
                end_line,
            )),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        use crate::ast::Span;
        let tok = self.advance_token().ok_or("Unexpected end of file")?;
        let token = &tok.text;
        let tok_line = tok.line;
        let tok_column = tok.column;
        let tok_len = tok.text.len();

        // Check if it looks like a float literal (contains . or scientific notation)
        // Must check this BEFORE integer parsing
        if let Some(f) = is_float_literal(token)
            .then(|| token.parse::<f64>().ok())
            .flatten()
        {
            return Ok(Statement::FloatLiteral(f));
        }

        // Try to parse as hex literal (0x or 0X prefix)
        if let Some(hex) = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
        {
            return i64::from_str_radix(hex, 16)
                .map(Statement::IntLiteral)
                .map_err(|_| format!("Invalid hex literal: {}", token));
        }

        // Try to parse as binary literal (0b or 0B prefix)
        if let Some(bin) = token
            .strip_prefix("0b")
            .or_else(|| token.strip_prefix("0B"))
        {
            return i64::from_str_radix(bin, 2)
                .map(Statement::IntLiteral)
                .map_err(|_| format!("Invalid binary literal: {}", token));
        }

        // Try to parse as decimal integer literal
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
            // Validate token has at least opening and closing quotes
            if token.len() < 2 || !token.ends_with('"') {
                return Err(format!("Malformed string literal: {}", token));
            }
            // Strip exactly one quote from each end (not all quotes, which would
            // incorrectly handle escaped quotes at string boundaries like "hello\"")
            let raw = &token[1..token.len() - 1];
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

        // Check for match expression
        if token == "match" {
            return self.parse_match();
        }

        // Otherwise it's a word call - preserve source span for precise diagnostics
        Ok(Statement::WordCall {
            name: token.to_string(),
            span: Some(Span::new(tok_line, tok_column, tok_len)),
        })
    }

    fn parse_if(&mut self) -> Result<Statement, String> {
        let mut then_branch = Vec::new();

        // Parse then branch until 'else' or 'then'
        loop {
            if self.is_at_end() {
                return Err("Unexpected end of file in 'if' statement".to_string());
            }

            // Skip comments and newlines
            self.skip_comments();

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

            // Skip comments and newlines
            self.skip_comments();

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

            // Skip comments and newlines
            self.skip_comments();

            if self.check("]") {
                self.advance();
                let id = self.next_quotation_id;
                self.next_quotation_id += 1;
                return Ok(Statement::Quotation { id, body });
            }

            body.push(self.parse_statement()?);
        }
    }

    /// Parse a match expression:
    ///   match
    ///     Get -> send-response
    ///     Increment -> do-increment send-response
    ///     Report -> aggregate-add
    ///   end
    fn parse_match(&mut self) -> Result<Statement, String> {
        let mut arms = Vec::new();

        loop {
            self.skip_comments();

            // Check for 'end' to terminate match
            if self.check("end") {
                self.advance();
                break;
            }

            if self.is_at_end() {
                return Err("Unexpected end of file in match expression".to_string());
            }

            arms.push(self.parse_match_arm()?);
        }

        if arms.is_empty() {
            return Err("Match expression must have at least one arm".to_string());
        }

        Ok(Statement::Match { arms })
    }

    /// Parse a single match arm:
    ///   Get -> send-response
    ///   or with bindings:
    ///   Get { chan } -> chan send-response
    fn parse_match_arm(&mut self) -> Result<MatchArm, String> {
        // Get variant name
        let variant_name = self
            .advance()
            .ok_or("Expected variant name in match arm")?
            .clone();

        self.skip_comments();

        // Check for optional bindings: { field1 field2 }
        let pattern = if self.check("{") {
            self.consume("{");
            let mut bindings = Vec::new();

            loop {
                self.skip_comments();

                if self.check("}") {
                    break;
                }

                if self.is_at_end() {
                    return Err(format!(
                        "Unexpected end of file in match arm bindings for '{}'",
                        variant_name
                    ));
                }

                let token = self.advance().ok_or("Expected binding name")?.clone();

                // Require > prefix to make clear these are stack extractions, not variables
                if let Some(field_name) = token.strip_prefix('>') {
                    if field_name.is_empty() {
                        return Err(format!(
                            "Expected field name after '>' in match bindings for '{}'",
                            variant_name
                        ));
                    }
                    bindings.push(field_name.to_string());
                } else {
                    return Err(format!(
                        "Match bindings must use '>' prefix to indicate stack extraction. \
                         Use '>{}' instead of '{}' in pattern for '{}'",
                        token, token, variant_name
                    ));
                }
            }

            self.consume("}");
            Pattern::VariantWithBindings {
                name: variant_name,
                bindings,
            }
        } else {
            Pattern::Variant(variant_name.clone())
        };

        self.skip_comments();

        // Expect '->' arrow
        if !self.consume("->") {
            return Err(format!(
                "Expected '->' after pattern '{}', got '{}'",
                match &pattern {
                    Pattern::Variant(n) => n.clone(),
                    Pattern::VariantWithBindings { name, .. } => name.clone(),
                },
                self.current()
            ));
        }

        // Parse body until next pattern or 'end'
        let mut body = Vec::new();
        loop {
            self.skip_comments();

            // Check for end of arm (next pattern starts with uppercase, or 'end')
            if self.check("end") {
                break;
            }

            // Check if next token looks like a match pattern (not just any uppercase word).
            // A pattern is: UppercaseName followed by '->' or '{'
            // This prevents confusing 'Make-Get' (constructor call) with a pattern.
            if let Some(token) = self.current_token()
                && let Some(first_char) = token.text.chars().next()
                && first_char.is_uppercase()
            {
                // Peek at next token to see if this is a pattern (followed by -> or {)
                if let Some(next) = self.peek_at(1)
                    && (next == "->" || next == "{")
                {
                    // This is the next pattern
                    break;
                }
                // Otherwise it's just an uppercase word call (like Make-Get), continue parsing body
            }

            if self.is_at_end() {
                return Err("Unexpected end of file in match arm body".to_string());
            }

            body.push(self.parse_statement()?);
        }

        Ok(MatchArm { pattern, body })
    }

    /// Parse a stack effect declaration: ( ..a Int -- ..a Bool )
    /// With optional computational effects: ( ..a Int -- ..a Bool | Yield Int )
    fn parse_stack_effect(&mut self) -> Result<Effect, String> {
        // Consume '('
        if !self.consume("(") {
            return Err("Expected '(' to start stack effect".to_string());
        }

        // Parse input stack types (until '--' or ')')
        let (input_row_var, input_types) =
            self.parse_type_list_until(&["--", ")"], "stack effect inputs", 0)?;

        // Consume '--'
        if !self.consume("--") {
            return Err("Expected '--' separator in stack effect".to_string());
        }

        // Parse output stack types (until ')' or '|')
        let (output_row_var, output_types) =
            self.parse_type_list_until(&[")", "|"], "stack effect outputs", 0)?;

        // Parse optional computational effects after '|'
        let effects = if self.consume("|") {
            self.parse_effect_annotations()?
        } else {
            Vec::new()
        };

        // Consume ')'
        if !self.consume(")") {
            return Err("Expected ')' to end stack effect".to_string());
        }

        // Build input and output StackTypes
        let inputs = self.build_stack_type(input_row_var, input_types);
        let outputs = self.build_stack_type(output_row_var, output_types);

        Ok(Effect::with_effects(inputs, outputs, effects))
    }

    /// Parse computational effect annotations after '|'
    /// Example: | Yield Int
    fn parse_effect_annotations(&mut self) -> Result<Vec<SideEffect>, String> {
        let mut effects = Vec::new();

        // Parse effects until we hit ')'
        while let Some(token) = self.peek_at(0) {
            if token == ")" {
                break;
            }

            match token {
                "Yield" => {
                    self.advance(); // consume "Yield"
                    // Parse the yield type
                    if let Some(type_token) = self.peek_at(0) {
                        if type_token == ")" {
                            return Err("Expected type after 'Yield'".to_string());
                        }
                        let type_str = type_token.to_string();
                        self.advance();
                        let yield_type = self.parse_type(&type_str)?;
                        effects.push(SideEffect::Yield(Box::new(yield_type)));
                    } else {
                        return Err("Expected type after 'Yield'".to_string());
                    }
                }
                _ => {
                    return Err(format!("Unknown effect '{}'. Expected 'Yield'", token));
                }
            }
        }

        if effects.is_empty() {
            return Err("Expected at least one effect after '|'".to_string());
        }

        Ok(effects)
    }

    /// Parse a single type token into a Type
    fn parse_type(&self, token: &str) -> Result<Type, String> {
        match token {
            "Int" => Ok(Type::Int),
            "Float" => Ok(Type::Float),
            "Bool" => Ok(Type::Bool),
            "String" => Ok(Type::String),
            _ => {
                // Check if it's a type variable (starts with uppercase)
                if let Some(first_char) = token.chars().next() {
                    if first_char.is_uppercase() {
                        Ok(Type::Var(token.to_string()))
                    } else {
                        Err(format!(
                            "Unknown type: '{}'. Expected Int, Bool, String, Closure, or a type variable (uppercase)",
                            token
                        ))
                    }
                } else {
                    Err(format!("Invalid type: '{}'", token))
                }
            }
        }
    }

    /// Validate row variable name
    /// Row variables must start with a lowercase letter and contain only alphanumeric characters
    fn validate_row_var_name(&self, name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Err("Row variable must have a name after '..'".to_string());
        }

        // Must start with lowercase letter
        let first_char = name.chars().next().unwrap();
        if !first_char.is_ascii_lowercase() {
            return Err(format!(
                "Row variable '..{}' must start with a lowercase letter (a-z)",
                name
            ));
        }

        // Rest must be alphanumeric or underscore
        for ch in name.chars() {
            if !ch.is_alphanumeric() && ch != '_' {
                return Err(format!(
                    "Row variable '..{}' can only contain letters, numbers, and underscores",
                    name
                ));
            }
        }

        // Check for reserved keywords (type names that might confuse users)
        match name {
            "Int" | "Bool" | "String" => {
                return Err(format!(
                    "Row variable '..{}' cannot use type name as identifier",
                    name
                ));
            }
            _ => {}
        }

        Ok(())
    }

    /// Parse a list of types until one of the given terminators is reached
    /// Returns (optional row variable, list of types)
    /// Used by both parse_stack_effect and parse_quotation_type
    ///
    /// depth: Current nesting depth for quotation types (0 at top level)
    fn parse_type_list_until(
        &mut self,
        terminators: &[&str],
        context: &str,
        depth: usize,
    ) -> Result<(Option<String>, Vec<Type>), String> {
        const MAX_QUOTATION_DEPTH: usize = 32;

        if depth > MAX_QUOTATION_DEPTH {
            return Err(format!(
                "Quotation type nesting exceeds maximum depth of {} (possible deeply nested types or DOS attack)",
                MAX_QUOTATION_DEPTH
            ));
        }

        let mut types = Vec::new();
        let mut row_var = None;

        while !terminators.iter().any(|t| self.check(t)) {
            if self.is_at_end() {
                return Err(format!(
                    "Unexpected end while parsing {} - expected one of: {}",
                    context,
                    terminators.join(", ")
                ));
            }

            let token = self
                .advance()
                .ok_or_else(|| format!("Unexpected end in {}", context))?
                .clone();

            // Check for row variable: ..name
            if token.starts_with("..") {
                let var_name = token.trim_start_matches("..").to_string();
                self.validate_row_var_name(&var_name)?;
                row_var = Some(var_name);
            } else if token == "Closure" {
                // Closure type: Closure[effect]
                if !self.consume("[") {
                    return Err("Expected '[' after 'Closure' in type signature".to_string());
                }
                let effect_type = self.parse_quotation_type(depth)?;
                match effect_type {
                    Type::Quotation(effect) => {
                        types.push(Type::Closure {
                            effect,
                            captures: Vec::new(), // Filled in by type checker
                        });
                    }
                    _ => unreachable!("parse_quotation_type should return Quotation"),
                }
            } else if token == "[" {
                // Nested quotation type
                types.push(self.parse_quotation_type(depth)?);
            } else {
                // Parse as concrete type
                types.push(self.parse_type(&token)?);
            }
        }

        Ok((row_var, types))
    }

    /// Parse a quotation type: [inputs -- outputs]
    /// Note: The opening '[' has already been consumed
    ///
    /// depth: Current nesting depth (incremented for each nested quotation)
    fn parse_quotation_type(&mut self, depth: usize) -> Result<Type, String> {
        // Parse input stack types (until '--' or ']')
        let (input_row_var, input_types) =
            self.parse_type_list_until(&["--", "]"], "quotation type inputs", depth + 1)?;

        // Require '--' separator for clarity
        if !self.consume("--") {
            // Check if user closed with ] without separator
            if self.check("]") {
                return Err(
                    "Quotation types require '--' separator. Did you mean '[Int -- ]' or '[ -- Int]'?"
                        .to_string(),
                );
            }
            return Err("Expected '--' separator in quotation type".to_string());
        }

        // Parse output stack types (until ']')
        let (output_row_var, output_types) =
            self.parse_type_list_until(&["]"], "quotation type outputs", depth + 1)?;

        // Consume ']'
        if !self.consume("]") {
            return Err("Expected ']' to end quotation type".to_string());
        }

        // Build input and output StackTypes
        let inputs = self.build_stack_type(input_row_var, input_types);
        let outputs = self.build_stack_type(output_row_var, output_types);

        Ok(Type::Quotation(Box::new(Effect::new(inputs, outputs))))
    }

    /// Build a StackType from an optional row variable and a list of types
    /// Example: row_var="a", types=[Int, Bool] => RowVar("a") with Int on top of Bool
    ///
    /// IMPORTANT: ALL stack effects are implicitly row-polymorphic in concatenative languages.
    /// This means:
    ///   ( -- )        becomes  ( ..rest -- ..rest )       - no-op, preserves stack
    ///   ( -- Int )    becomes  ( ..rest -- ..rest Int )   - pushes Int
    ///   ( Int -- )    becomes  ( ..rest Int -- ..rest )   - consumes Int
    ///   ( Int -- Int) becomes  ( ..rest Int -- ..rest Int ) - transforms top
    fn build_stack_type(&self, row_var: Option<String>, types: Vec<Type>) -> StackType {
        // Always use row polymorphism - this is fundamental to concatenative semantics
        let base = match row_var {
            Some(name) => StackType::RowVar(name),
            None => StackType::RowVar("rest".to_string()),
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

    /// Get the text of the current token
    fn current(&self) -> &str {
        if self.is_at_end() {
            ""
        } else {
            &self.tokens[self.pos].text
        }
    }

    /// Get the full current token with position info
    fn current_token(&self) -> Option<&Token> {
        if self.is_at_end() {
            None
        } else {
            Some(&self.tokens[self.pos])
        }
    }

    /// Peek at a token N positions ahead without consuming
    fn peek_at(&self, n: usize) -> Option<&str> {
        let idx = self.pos + n;
        if idx < self.tokens.len() {
            Some(&self.tokens[idx].text)
        } else {
            None
        }
    }

    /// Advance and return the token text (for compatibility with existing code)
    fn advance(&mut self) -> Option<&String> {
        if self.is_at_end() {
            None
        } else {
            let token = &self.tokens[self.pos];
            self.pos += 1;
            Some(&token.text)
        }
    }

    /// Advance and return the full token with position info
    fn advance_token(&mut self) -> Option<&Token> {
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

/// Check if a token looks like a float literal
///
/// Float literals contain either:
/// - A decimal point: `3.14`, `.5`, `5.`
/// - Scientific notation: `1e10`, `1E-5`, `1.5e3`
///
/// This check must happen BEFORE integer parsing to avoid
/// parsing "5" in "5.0" as an integer.
fn is_float_literal(token: &str) -> bool {
    // Skip leading minus sign for negative numbers
    let s = token.strip_prefix('-').unwrap_or(token);

    // Must have at least one digit
    if s.is_empty() {
        return false;
    }

    // Check for decimal point or scientific notation
    s.contains('.') || s.contains('e') || s.contains('E')
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

fn tokenize(source: &str) -> Vec<Token> {
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
            if !current.is_empty() {
                tokens.push(Token::new(
                    current.clone(),
                    current_start_line,
                    current_start_col,
                ));
                current.clear();
            }
            in_string = true;
            current_start_line = line;
            current_start_col = col;
            current.push(ch);
            prev_was_backslash = false;
            col += 1;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(Token::new(
                    current.clone(),
                    current_start_line,
                    current_start_col,
                ));
                current.clear();
            }
            // Preserve newlines for comment handling
            if ch == '\n' {
                tokens.push(Token::new("\n".to_string(), line, col));
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        } else if "():;[]{},".contains(ch) {
            if !current.is_empty() {
                tokens.push(Token::new(
                    current.clone(),
                    current_start_line,
                    current_start_col,
                ));
                current.clear();
            }
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
            Statement::WordCall { name, .. } => assert_eq!(name, "write_line"),
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
        assert!(matches!(
            &program.words[0].body[2],
            Statement::WordCall { name, .. } if name == "add"
        ));
    }

    #[test]
    fn test_parse_hex_literals() {
        let source = ": test ( -- ) 0xFF 0x10 0X1A ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words[0].body[0], Statement::IntLiteral(255));
        assert_eq!(program.words[0].body[1], Statement::IntLiteral(16));
        assert_eq!(program.words[0].body[2], Statement::IntLiteral(26));
    }

    #[test]
    fn test_parse_binary_literals() {
        let source = ": test ( -- ) 0b1010 0B1111 0b0 ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words[0].body[0], Statement::IntLiteral(10));
        assert_eq!(program.words[0].body[1], Statement::IntLiteral(15));
        assert_eq!(program.words[0].body[2], Statement::IntLiteral(0));
    }

    #[test]
    fn test_parse_invalid_hex_literal() {
        let source = ": test ( -- ) 0xGG ;";
        let mut parser = Parser::new(source);
        let err = parser.parse().unwrap_err();
        assert!(err.contains("Invalid hex literal"));
    }

    #[test]
    fn test_parse_invalid_binary_literal() {
        let source = ": test ( -- ) 0b123 ;";
        let mut parser = Parser::new(source);
        let err = parser.parse().unwrap_err();
        assert!(err.contains("Invalid binary literal"));
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

    /// Regression test for issue #117: escaped quote at end of string
    /// Previously failed with "String ends with incomplete escape sequence"
    #[test]
    fn test_escaped_quote_at_end_of_string() {
        let source = r#": main ( -- ) "hello\"" io.write-line ;"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        match &program.words[0].body[0] {
            Statement::StringLiteral(s) => assert_eq!(s, "hello\""),
            _ => panic!("Expected StringLiteral ending with escaped quote"),
        }
    }

    /// Test escaped quote at start of string (boundary case)
    #[test]
    fn test_escaped_quote_at_start_of_string() {
        let source = r#": main ( -- ) "\"hello" io.write-line ;"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        match &program.words[0].body[0] {
            Statement::StringLiteral(s) => assert_eq!(s, "\"hello"),
            _ => panic!("Expected StringLiteral starting with escaped quote"),
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
        let err_msg = result.unwrap_err();
        assert!(err_msg.contains("Unclosed string literal"));
        // Should include position information (line 1, column 15 for the opening quote)
        assert!(
            err_msg.contains("line 1"),
            "Expected line number in error: {}",
            err_msg
        );
        assert!(
            err_msg.contains("column 15"),
            "Expected column number in error: {}",
            err_msg
        );
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
            Statement::WordCall { name, .. } => assert_eq!(name, "helper"),
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
    fn test_parse_invalid_row_var_starts_with_digit() {
        // Test: Row variable cannot start with digit
        let source = ": test ( ..123 Int -- ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("lowercase letter"),
            "Expected error about lowercase letter, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_invalid_row_var_starts_with_uppercase() {
        // Test: Row variable cannot start with uppercase (that's a type variable)
        let source = ": test ( ..Int Int -- ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("lowercase letter") || err_msg.contains("type name"),
            "Expected error about lowercase letter or type name, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_invalid_row_var_with_special_chars() {
        // Test: Row variable cannot contain special characters
        let source = ": test ( ..a-b Int -- ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("letters, numbers, and underscores")
                || err_msg.contains("Unknown type"),
            "Expected error about valid characters, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_valid_row_var_with_underscore() {
        // Test: Row variable CAN contain underscore
        let source = ": test ( ..my_row Int -- ..my_row Bool ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_ok(), "Should accept row variable with underscore");
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
        // In concatenative languages, even empty effects are row-polymorphic
        // ( -- ) means ( ..rest -- ..rest ) - preserves stack
        let source = ": test ( -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Both inputs and outputs should use the same implicit row variable
        assert_eq!(effect.inputs, StackType::RowVar("rest".to_string()));
        assert_eq!(effect.outputs, StackType::RowVar("rest".to_string()));
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
    fn test_parse_simple_quotation_type() {
        // Test: ( [Int -- Int] -- )
        let source = ": apply ( [Int -- Int] -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input should be: Quotation(Int -- Int) on RowVar("rest")
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Quotation(quot_effect) => {
                // Check quotation's input: Int on RowVar("rest")
                assert_eq!(
                    quot_effect.inputs,
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Int
                    }
                );
                // Check quotation's output: Int on RowVar("rest")
                assert_eq!(
                    quot_effect.outputs,
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Int
                    }
                );
            }
            _ => panic!("Expected Quotation type, got {:?}", top),
        }
        assert_eq!(rest, StackType::RowVar("rest".to_string()));
    }

    #[test]
    fn test_parse_quotation_type_with_row_vars() {
        // Test: ( ..a [..a T -- ..a Bool] -- ..a )
        let source = ": test ( ..a [..a T -- ..a Bool] -- ..a ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input: Quotation on RowVar("a")
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Quotation(quot_effect) => {
                // Check quotation's input: T on RowVar("a")
                let (q_in_rest, q_in_top) = quot_effect.inputs.clone().pop().unwrap();
                assert_eq!(q_in_top, Type::Var("T".to_string()));
                assert_eq!(q_in_rest, StackType::RowVar("a".to_string()));

                // Check quotation's output: Bool on RowVar("a")
                let (q_out_rest, q_out_top) = quot_effect.outputs.clone().pop().unwrap();
                assert_eq!(q_out_top, Type::Bool);
                assert_eq!(q_out_rest, StackType::RowVar("a".to_string()));
            }
            _ => panic!("Expected Quotation type, got {:?}", top),
        }
        assert_eq!(rest, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_parse_nested_quotation_type() {
        // Test: ( [[Int -- Int] -- Bool] -- )
        let source = ": nested ( [[Int -- Int] -- Bool] -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input: Quotation([Int -- Int] -- Bool) on RowVar("rest")
        let (_, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Quotation(outer_effect) => {
                // Outer quotation input: [Int -- Int] on RowVar("rest")
                let (_, outer_in_top) = outer_effect.inputs.clone().pop().unwrap();
                match outer_in_top {
                    Type::Quotation(inner_effect) => {
                        // Inner quotation: Int -- Int
                        assert!(matches!(
                            inner_effect.inputs.clone().pop().unwrap().1,
                            Type::Int
                        ));
                        assert!(matches!(
                            inner_effect.outputs.clone().pop().unwrap().1,
                            Type::Int
                        ));
                    }
                    _ => panic!("Expected nested Quotation type"),
                }

                // Outer quotation output: Bool
                let (_, outer_out_top) = outer_effect.outputs.clone().pop().unwrap();
                assert_eq!(outer_out_top, Type::Bool);
            }
            _ => panic!("Expected Quotation type"),
        }
    }

    #[test]
    fn test_parse_deeply_nested_quotation_type_exceeds_limit() {
        // Test: Deeply nested quotation types should fail with max depth error
        // Build a quotation type nested 35 levels deep (exceeds MAX_QUOTATION_DEPTH = 32)
        let mut source = String::from(": deep ( ");

        // Build opening brackets: [[[[[[...
        for _ in 0..35 {
            source.push_str("[ -- ");
        }

        source.push_str("Int");

        // Build closing brackets: ...]]]]]]
        for _ in 0..35 {
            source.push_str(" ]");
        }

        source.push_str(" -- ) ;");

        let mut parser = Parser::new(&source);
        let result = parser.parse();

        // Should fail with depth limit error
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("depth") || err_msg.contains("32"),
            "Expected depth limit error, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_empty_quotation_type() {
        // Test: ( [ -- ] -- )
        // An empty quotation type is also row-polymorphic: [ ..rest -- ..rest ]
        let source = ": empty-quot ( [ -- ] -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        let (_, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Quotation(quot_effect) => {
                // Empty quotation preserves the stack (row-polymorphic)
                assert_eq!(quot_effect.inputs, StackType::RowVar("rest".to_string()));
                assert_eq!(quot_effect.outputs, StackType::RowVar("rest".to_string()));
            }
            _ => panic!("Expected Quotation type"),
        }
    }

    #[test]
    fn test_parse_quotation_type_in_output() {
        // Test: ( -- [Int -- Int] )
        let source = ": maker ( -- [Int -- Int] ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Output should be: Quotation(Int -- Int) on RowVar("rest")
        let (_, top) = effect.outputs.clone().pop().unwrap();
        match top {
            Type::Quotation(quot_effect) => {
                assert!(matches!(
                    quot_effect.inputs.clone().pop().unwrap().1,
                    Type::Int
                ));
                assert!(matches!(
                    quot_effect.outputs.clone().pop().unwrap().1,
                    Type::Int
                ));
            }
            _ => panic!("Expected Quotation type"),
        }
    }

    #[test]
    fn test_parse_unclosed_quotation_type() {
        // Test: ( [Int -- Int -- )  (missing ])
        let source = ": broken ( [Int -- Int -- ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        // Parser might error with various messages depending on where it fails
        // It should at least indicate a parsing problem
        assert!(
            err_msg.contains("Unclosed")
                || err_msg.contains("Expected")
                || err_msg.contains("Unexpected"),
            "Got error: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_multiple_quotation_types() {
        // Test: ( [Int -- Int] [String -- Bool] -- )
        let source = ": multi ( [Int -- Int] [String -- Bool] -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Pop second quotation (String -- Bool)
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Quotation(quot_effect) => {
                assert!(matches!(
                    quot_effect.inputs.clone().pop().unwrap().1,
                    Type::String
                ));
                assert!(matches!(
                    quot_effect.outputs.clone().pop().unwrap().1,
                    Type::Bool
                ));
            }
            _ => panic!("Expected Quotation type"),
        }

        // Pop first quotation (Int -- Int)
        let (_, top2) = rest.pop().unwrap();
        match top2 {
            Type::Quotation(quot_effect) => {
                assert!(matches!(
                    quot_effect.inputs.clone().pop().unwrap().1,
                    Type::Int
                ));
                assert!(matches!(
                    quot_effect.outputs.clone().pop().unwrap().1,
                    Type::Int
                ));
            }
            _ => panic!("Expected Quotation type"),
        }
    }

    #[test]
    fn test_parse_quotation_type_without_separator() {
        // Test: ( [Int] -- ) should be REJECTED
        //
        // Design decision: The '--' separator is REQUIRED for clarity.
        // [Int] looks like a list type in most languages, not a consumer function.
        // This would confuse users.
        //
        // Require explicit syntax:
        // - `[Int -- ]` for quotation that consumes Int and produces nothing
        // - `[ -- Int]` for quotation that produces Int
        // - `[Int -- Int]` for transformation
        let source = ": consumer ( [Int] -- ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        // Should fail with helpful error message
        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("require") && err_msg.contains("--"),
            "Expected error about missing '--' separator, got: {}",
            err_msg
        );
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
            Statement::Quotation { body, .. } => {
                assert_eq!(body.len(), 2);
                assert_eq!(body[0], Statement::IntLiteral(1));
                assert!(matches!(&body[1], Statement::WordCall { name, .. } if name == "add"));
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
            Statement::Quotation { body, .. } => {
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
            Statement::Quotation { body, .. } => {
                assert_eq!(body.len(), 2);
            }
            _ => panic!("Expected Quotation"),
        }

        assert!(matches!(
            &program.words[0].body[2],
            Statement::WordCall { name, .. } if name == "call"
        ));
    }

    #[test]
    fn test_parse_nested_quotation() {
        let source = ": test [ [ 1 add ] call ] ;";

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);

        match &program.words[0].body[0] {
            Statement::Quotation {
                body: outer_body, ..
            } => {
                assert_eq!(outer_body.len(), 2);

                match &outer_body[0] {
                    Statement::Quotation {
                        body: inner_body, ..
                    } => {
                        assert_eq!(inner_body.len(), 2);
                        assert_eq!(inner_body[0], Statement::IntLiteral(1));
                        assert!(
                            matches!(&inner_body[1], Statement::WordCall { name, .. } if name == "add")
                        );
                    }
                    _ => panic!("Expected nested Quotation"),
                }

                assert!(
                    matches!(&outer_body[1], Statement::WordCall { name, .. } if name == "call")
                );
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
            Statement::Quotation { body: pred, .. } => {
                assert_eq!(pred.len(), 3);
                assert!(matches!(&pred[0], Statement::WordCall { name, .. } if name == "dup"));
                assert_eq!(pred[1], Statement::IntLiteral(0));
                assert!(matches!(&pred[2], Statement::WordCall { name, .. } if name == ">"));
            }
            _ => panic!("Expected predicate quotation"),
        }

        // Second quotation: [ 1 subtract ]
        match &program.words[0].body[1] {
            Statement::Quotation { body, .. } => {
                assert_eq!(body.len(), 2);
                assert_eq!(body[0], Statement::IntLiteral(1));
                assert!(matches!(&body[1], Statement::WordCall { name, .. } if name == "subtract"));
            }
            _ => panic!("Expected body quotation"),
        }

        // while call
        assert!(matches!(
            &program.words[0].body[2],
            Statement::WordCall { name, .. } if name == "while"
        ));

        // drop
        assert!(matches!(
            &program.words[0].body[3],
            Statement::WordCall { name, .. } if name == "drop"
        ));
    }

    #[test]
    fn test_parse_simple_closure_type() {
        // Test: ( Int -- Closure[Int -- Int] )
        let source = ": make-adder ( Int -- Closure[Int -- Int] ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        let word = &program.words[0];
        assert!(word.effect.is_some());

        let effect = word.effect.as_ref().unwrap();

        // Input: Int on RowVar("rest")
        let (input_rest, input_top) = effect.inputs.clone().pop().unwrap();
        assert_eq!(input_top, Type::Int);
        assert_eq!(input_rest, StackType::RowVar("rest".to_string()));

        // Output: Closure[Int -- Int] on RowVar("rest")
        let (output_rest, output_top) = effect.outputs.clone().pop().unwrap();
        match output_top {
            Type::Closure { effect, captures } => {
                // Closure effect: Int -> Int
                assert_eq!(
                    effect.inputs,
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Int
                    }
                );
                assert_eq!(
                    effect.outputs,
                    StackType::Cons {
                        rest: Box::new(StackType::RowVar("rest".to_string())),
                        top: Type::Int
                    }
                );
                // Captures should be empty (filled in by type checker)
                assert_eq!(captures.len(), 0);
            }
            _ => panic!("Expected Closure type, got {:?}", output_top),
        }
        assert_eq!(output_rest, StackType::RowVar("rest".to_string()));
    }

    #[test]
    fn test_parse_closure_type_with_row_vars() {
        // Test: ( ..a Config -- ..a Closure[Request -- Response] )
        let source = ": make-handler ( ..a Config -- ..a Closure[Request -- Response] ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Output: Closure on RowVar("a")
        let (rest, top) = effect.outputs.clone().pop().unwrap();
        match top {
            Type::Closure { effect, .. } => {
                // Closure effect: Request -> Response
                let (_, in_top) = effect.inputs.clone().pop().unwrap();
                assert_eq!(in_top, Type::Var("Request".to_string()));
                let (_, out_top) = effect.outputs.clone().pop().unwrap();
                assert_eq!(out_top, Type::Var("Response".to_string()));
            }
            _ => panic!("Expected Closure type"),
        }
        assert_eq!(rest, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_parse_closure_type_missing_bracket() {
        // Test: ( Int -- Closure ) should fail
        let source = ": broken ( Int -- Closure ) ;";
        let mut parser = Parser::new(source);
        let result = parser.parse();

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("[") && err_msg.contains("Closure"),
            "Expected error about missing '[' after Closure, got: {}",
            err_msg
        );
    }

    #[test]
    fn test_parse_closure_type_in_input() {
        // Test: ( Closure[Int -- Int] -- )
        let source = ": apply-closure ( Closure[Int -- Int] -- ) ;";
        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        let effect = program.words[0].effect.as_ref().unwrap();

        // Input: Closure[Int -- Int] on RowVar("rest")
        let (_, top) = effect.inputs.clone().pop().unwrap();
        match top {
            Type::Closure { effect, .. } => {
                // Verify closure effect
                assert!(matches!(effect.inputs.clone().pop().unwrap().1, Type::Int));
                assert!(matches!(effect.outputs.clone().pop().unwrap().1, Type::Int));
            }
            _ => panic!("Expected Closure type in input"),
        }
    }

    // Tests for token position tracking

    #[test]
    fn test_token_position_single_line() {
        // Test token positions on a single line
        let source = ": main ( -- ) ;";
        let tokens = tokenize(source);

        // : is at line 0, column 0
        assert_eq!(tokens[0].text, ":");
        assert_eq!(tokens[0].line, 0);
        assert_eq!(tokens[0].column, 0);

        // main is at line 0, column 2
        assert_eq!(tokens[1].text, "main");
        assert_eq!(tokens[1].line, 0);
        assert_eq!(tokens[1].column, 2);

        // ( is at line 0, column 7
        assert_eq!(tokens[2].text, "(");
        assert_eq!(tokens[2].line, 0);
        assert_eq!(tokens[2].column, 7);
    }

    #[test]
    fn test_token_position_multiline() {
        // Test token positions across multiple lines
        let source = ": main ( -- )\n  42\n;";
        let tokens = tokenize(source);

        // Find the 42 token (after the newline)
        let token_42 = tokens.iter().find(|t| t.text == "42").unwrap();
        assert_eq!(token_42.line, 1);
        assert_eq!(token_42.column, 2); // After 2 spaces of indentation

        // Find the ; token (on line 2)
        let token_semi = tokens.iter().find(|t| t.text == ";").unwrap();
        assert_eq!(token_semi.line, 2);
        assert_eq!(token_semi.column, 0);
    }

    #[test]
    fn test_word_def_source_location_span() {
        // Test that word definitions capture correct start and end lines
        let source = r#": helper ( -- )
  "hello"
  write_line
;

: main ( -- )
  helper
;"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 2);

        // First word: helper spans lines 0-3
        let helper = &program.words[0];
        assert_eq!(helper.name, "helper");
        let helper_source = helper.source.as_ref().unwrap();
        assert_eq!(helper_source.start_line, 0);
        assert_eq!(helper_source.end_line, 3);

        // Second word: main spans lines 5-7
        let main_word = &program.words[1];
        assert_eq!(main_word.name, "main");
        let main_source = main_word.source.as_ref().unwrap();
        assert_eq!(main_source.start_line, 5);
        assert_eq!(main_source.end_line, 7);
    }

    #[test]
    fn test_token_position_string_with_newline() {
        // Test that newlines inside strings are tracked correctly
        let source = "\"line1\\nline2\"";
        let tokens = tokenize(source);

        // The string token should start at line 0, column 0
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].line, 0);
        assert_eq!(tokens[0].column, 0);
    }

    // ============================================================================
    //                         ADT PARSING TESTS
    // ============================================================================

    #[test]
    fn test_parse_simple_union() {
        let source = r#"
union Message {
  Get { response-chan: Int }
  Set { value: Int }
}

: main ( -- ) ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.unions.len(), 1);
        let union_def = &program.unions[0];
        assert_eq!(union_def.name, "Message");
        assert_eq!(union_def.variants.len(), 2);

        // Check first variant
        assert_eq!(union_def.variants[0].name, "Get");
        assert_eq!(union_def.variants[0].fields.len(), 1);
        assert_eq!(union_def.variants[0].fields[0].name, "response-chan");
        assert_eq!(union_def.variants[0].fields[0].type_name, "Int");

        // Check second variant
        assert_eq!(union_def.variants[1].name, "Set");
        assert_eq!(union_def.variants[1].fields.len(), 1);
        assert_eq!(union_def.variants[1].fields[0].name, "value");
        assert_eq!(union_def.variants[1].fields[0].type_name, "Int");
    }

    #[test]
    fn test_parse_union_with_multiple_fields() {
        let source = r#"
union Report {
  Data { op: Int, delta: Int, total: Int }
  Empty
}

: main ( -- ) ;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.unions.len(), 1);
        let union_def = &program.unions[0];
        assert_eq!(union_def.name, "Report");
        assert_eq!(union_def.variants.len(), 2);

        // Check Data variant with 3 fields
        let data_variant = &union_def.variants[0];
        assert_eq!(data_variant.name, "Data");
        assert_eq!(data_variant.fields.len(), 3);
        assert_eq!(data_variant.fields[0].name, "op");
        assert_eq!(data_variant.fields[1].name, "delta");
        assert_eq!(data_variant.fields[2].name, "total");

        // Check Empty variant with no fields
        let empty_variant = &union_def.variants[1];
        assert_eq!(empty_variant.name, "Empty");
        assert_eq!(empty_variant.fields.len(), 0);
    }

    #[test]
    fn test_parse_union_lowercase_name_error() {
        let source = r#"
union message {
  Get { }
}
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("uppercase"));
    }

    #[test]
    fn test_parse_union_empty_error() {
        let source = r#"
union Message {
}
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one variant"));
    }

    #[test]
    fn test_parse_union_duplicate_variant_error() {
        let source = r#"
union Message {
  Get { x: Int }
  Get { y: String }
}
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Duplicate variant name"));
        assert!(err.contains("Get"));
    }

    #[test]
    fn test_parse_union_duplicate_field_error() {
        let source = r#"
union Data {
  Record { x: Int, x: String }
}
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Duplicate field name"));
        assert!(err.contains("x"));
    }

    #[test]
    fn test_parse_simple_match() {
        let source = r#"
: handle ( -- )
  match
    Get -> send-response
    Set -> process-set
  end
;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);
        assert_eq!(program.words[0].body.len(), 1);

        match &program.words[0].body[0] {
            Statement::Match { arms } => {
                assert_eq!(arms.len(), 2);

                // First arm: Get ->
                match &arms[0].pattern {
                    Pattern::Variant(name) => assert_eq!(name, "Get"),
                    _ => panic!("Expected Variant pattern"),
                }
                assert_eq!(arms[0].body.len(), 1);

                // Second arm: Set ->
                match &arms[1].pattern {
                    Pattern::Variant(name) => assert_eq!(name, "Set"),
                    _ => panic!("Expected Variant pattern"),
                }
                assert_eq!(arms[1].body.len(), 1);
            }
            _ => panic!("Expected Match statement"),
        }
    }

    #[test]
    fn test_parse_match_with_bindings() {
        let source = r#"
: handle ( -- )
  match
    Get { >chan } -> chan send-response
    Report { >delta >total } -> delta total process
  end
;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        assert_eq!(program.words.len(), 1);

        match &program.words[0].body[0] {
            Statement::Match { arms } => {
                assert_eq!(arms.len(), 2);

                // First arm: Get { chan } ->
                match &arms[0].pattern {
                    Pattern::VariantWithBindings { name, bindings } => {
                        assert_eq!(name, "Get");
                        assert_eq!(bindings.len(), 1);
                        assert_eq!(bindings[0], "chan");
                    }
                    _ => panic!("Expected VariantWithBindings pattern"),
                }

                // Second arm: Report { delta total } ->
                match &arms[1].pattern {
                    Pattern::VariantWithBindings { name, bindings } => {
                        assert_eq!(name, "Report");
                        assert_eq!(bindings.len(), 2);
                        assert_eq!(bindings[0], "delta");
                        assert_eq!(bindings[1], "total");
                    }
                    _ => panic!("Expected VariantWithBindings pattern"),
                }
            }
            _ => panic!("Expected Match statement"),
        }
    }

    #[test]
    fn test_parse_match_bindings_require_prefix() {
        // Old syntax without > prefix should error
        let source = r#"
: handle ( -- )
  match
    Get { chan } -> chan send-response
  end
;
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains(">chan"));
        assert!(err.contains("stack extraction"));
    }

    #[test]
    fn test_parse_match_with_body_statements() {
        let source = r#"
: handle ( -- )
  match
    Get -> 1 2 add send-response
    Set -> process-value store
  end
;
"#;

        let mut parser = Parser::new(source);
        let program = parser.parse().unwrap();

        match &program.words[0].body[0] {
            Statement::Match { arms } => {
                // Get arm has 4 statements: 1, 2, add, send-response
                assert_eq!(arms[0].body.len(), 4);
                assert_eq!(arms[0].body[0], Statement::IntLiteral(1));
                assert_eq!(arms[0].body[1], Statement::IntLiteral(2));
                assert!(
                    matches!(&arms[0].body[2], Statement::WordCall { name, .. } if name == "add")
                );

                // Set arm has 2 statements: process-value, store
                assert_eq!(arms[1].body.len(), 2);
            }
            _ => panic!("Expected Match statement"),
        }
    }

    #[test]
    fn test_parse_match_empty_error() {
        let source = r#"
: handle ( -- )
  match
  end
;
"#;

        let mut parser = Parser::new(source);
        let result = parser.parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one arm"));
    }
}
