//! Stack-effect, type, and quotation-type parsing.
use crate::types::{Effect, SideEffect, StackType, Type};

use super::{Parser, Token};

impl Parser {
    /// Parse a full stack effect: `( inputs -- outputs | effects )`.
    pub(super) fn parse_stack_effect(&mut self) -> Result<Effect, String> {
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
    pub(super) fn parse_effect_annotations(&mut self) -> Result<Vec<SideEffect>, String> {
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
                    if let Some(type_token) = self.current_token() {
                        if type_token.text == ")" {
                            return Err("Expected type after 'Yield'".to_string());
                        }
                        let type_token = type_token.clone();
                        self.advance();
                        let yield_type = self.parse_type(&type_token)?;
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

    /// Parse a single type token into a Type.
    ///
    /// Concrete types (Int, Float, Bool, String, Symbol, Channel, Socket,
    /// Variant) parse to their nominal form. Anything else uppercase becomes
    /// `Type::Var(name)` here. The strict gate (multi-char must be a known
    /// type or registered union, else error with "did you mean" hint) lives
    /// in `typechecker::validation::validate_type`, AFTER
    /// `fixup_union_types()` resolves cross-file union references.
    pub(super) fn parse_type(&self, token: &Token) -> Result<Type, String> {
        match token.text.as_str() {
            "Int" => Ok(Type::Int),
            "Float" => Ok(Type::Float),
            "Bool" => Ok(Type::Bool),
            "String" => Ok(Type::String),
            "Symbol" => Ok(Type::Symbol),
            "Channel" => Ok(Type::Channel),
            "Socket" => Ok(Type::Socket),
            "Variant" => Ok(Type::Variant),
            // Reject 'Quotation' - it looks like a type but would be silently treated as a type variable.
            // Users must use explicit effect syntax like [Int -- Int] instead.
            "Quotation" => Err(format!(
                "'Quotation' is not a valid type at line {}, column {}. Use explicit quotation syntax like [Int -- Int] or [ -- ] instead.",
                token.line + 1,
                token.column + 1
            )),
            _ => {
                let Some(first_char) = token.text.chars().next() else {
                    return Err(format!(
                        "Invalid type: '{}' at line {}, column {}",
                        token.text.escape_default(),
                        token.line + 1,
                        token.column + 1
                    ));
                };
                if !first_char.is_uppercase() {
                    return Err(format!(
                        "Unknown type: '{}' at line {}, column {}. Expected a concrete type (Int, Float, Bool, String, Symbol, Channel, Socket, Variant), a registered union, or an uppercase type variable.",
                        token.text.escape_default(),
                        token.line + 1,
                        token.column + 1
                    ));
                }
                // Uppercase identifier → known union or type variable. Multi-char
                // names that aren't registered unions are caught by validation
                // after include resolution and fixup; see typechecker/validation.rs.
                if self.known_unions.contains(&token.text) {
                    Ok(Type::Union(token.text.to_string()))
                } else {
                    Ok(Type::Var(token.text.to_string()))
                }
            }
        }
    }

    /// Validate row variable name
    /// Row variables must start with a lowercase letter and contain only alphanumeric characters
    pub(super) fn validate_row_var_name(&self, name: &str) -> Result<(), String> {
        // Must start with lowercase letter
        let Some(first_char) = name.chars().next() else {
            return Err("Row variable must have a name after '..'".to_string());
        };
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
    pub(super) fn parse_type_list_until(
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
            // Skip comments and blank lines within type lists
            self.skip_comments();

            // Re-check terminators after skipping comments
            if terminators.iter().any(|t| self.check(t)) {
                break;
            }

            if self.is_at_end() {
                return Err(format!(
                    "Unexpected end while parsing {} - expected one of: {}",
                    context,
                    terminators.join(", ")
                ));
            }

            let token = self
                .advance_token()
                .ok_or_else(|| format!("Unexpected end in {}", context))?
                .clone();

            // Check for row variable: ..name
            if token.text.starts_with("..") {
                let var_name = token.text.trim_start_matches("..").to_string();
                self.validate_row_var_name(&var_name)?;
                row_var = Some(var_name);
            } else if token.text == "Closure" {
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
            } else if token.text == "[" {
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
    pub(super) fn parse_quotation_type(&mut self, depth: usize) -> Result<Type, String> {
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
    pub(super) fn build_stack_type(&self, row_var: Option<String>, types: Vec<Type>) -> StackType {
        // Always use row polymorphism - this is fundamental to concatenative semantics
        let base = match row_var {
            Some(name) => StackType::RowVar(name),
            None => StackType::RowVar("rest".to_string()),
        };

        // Push types onto the stack (bottom to top order)
        types.into_iter().fold(base, |stack, ty| stack.push(ty))
    }
}

/// Standard Levenshtein distance — used for "did you mean" type-name hints.
/// Tiny inputs (type names, < 20 chars), so the O(n*m) DP is fine.
pub(crate) fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0usize; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}
