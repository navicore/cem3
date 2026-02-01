//! Abstract Syntax Tree for Seq
//!
//! Minimal AST sufficient for hello-world and basic programs.
//! Will be extended as we add more language features.

use crate::types::{Effect, StackType, Type};
use std::path::PathBuf;

/// Source location for error reporting and tooling
#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub file: PathBuf,
    /// Start line (0-indexed for LSP compatibility)
    pub start_line: usize,
    /// End line (0-indexed, inclusive)
    pub end_line: usize,
}

impl SourceLocation {
    /// Create a new source location with just a single line (for backward compatibility)
    pub fn new(file: PathBuf, line: usize) -> Self {
        SourceLocation {
            file,
            start_line: line,
            end_line: line,
        }
    }

    /// Create a source location spanning multiple lines
    pub fn span(file: PathBuf, start_line: usize, end_line: usize) -> Self {
        debug_assert!(
            start_line <= end_line,
            "SourceLocation: start_line ({}) must be <= end_line ({})",
            start_line,
            end_line
        );
        SourceLocation {
            file,
            start_line,
            end_line,
        }
    }
}

impl std::fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start_line == self.end_line {
            write!(f, "{}:{}", self.file.display(), self.start_line + 1)
        } else {
            write!(
                f,
                "{}:{}-{}",
                self.file.display(),
                self.start_line + 1,
                self.end_line + 1
            )
        }
    }
}

/// Include statement
#[derive(Debug, Clone, PartialEq)]
pub enum Include {
    /// Standard library include: `include std:http`
    Std(String),
    /// Relative path include: `include "my-utils"`
    Relative(String),
    /// FFI library include: `include ffi:readline`
    Ffi(String),
}

// ============================================================================
//                     ALGEBRAIC DATA TYPES (ADTs)
// ============================================================================

/// A field in a union variant
/// Example: `response-chan: Int`
#[derive(Debug, Clone, PartialEq)]
pub struct UnionField {
    pub name: String,
    pub type_name: String, // For now, just store the type name as string
}

/// A variant in a union type
/// Example: `Get { response-chan: Int }`
#[derive(Debug, Clone, PartialEq)]
pub struct UnionVariant {
    pub name: String,
    pub fields: Vec<UnionField>,
    pub source: Option<SourceLocation>,
}

/// A union type definition
/// Example:
/// ```seq
/// union Message {
///   Get { response-chan: Int }
///   Increment { response-chan: Int }
///   Report { op: Int, delta: Int, total: Int }
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct UnionDef {
    pub name: String,
    pub variants: Vec<UnionVariant>,
    pub source: Option<SourceLocation>,
}

/// A pattern in a match expression
/// For Phase 1: just the variant name (stack-based matching)
/// Later phases will add field bindings: `Get { chan }`
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Match a variant by name, pushing all fields to stack
    /// Example: `Get ->` pushes response-chan to stack
    Variant(String),

    /// Match a variant with named field bindings (Phase 5)
    /// Example: `Get { chan } ->` binds chan to the response-chan field
    VariantWithBindings { name: String, bindings: Vec<String> },
}

/// A single arm in a match expression
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<Statement>,
    /// Source span for error reporting (points to variant name)
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub includes: Vec<Include>,
    pub unions: Vec<UnionDef>,
    pub words: Vec<WordDef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WordDef {
    pub name: String,
    /// Optional stack effect declaration
    /// Example: ( ..a Int -- ..a Bool )
    pub effect: Option<Effect>,
    pub body: Vec<Statement>,
    /// Source location for error reporting (collision detection)
    pub source: Option<SourceLocation>,
    /// Lint IDs that are allowed (suppressed) for this word
    /// Set via `# seq:allow(lint-id)` annotation before the word definition
    pub allowed_lints: Vec<String>,
}

/// Source span for a single token or expression
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Span {
    /// Line number (0-indexed)
    pub line: usize,
    /// Start column (0-indexed)
    pub column: usize,
    /// Length of the span in characters
    pub length: usize,
}

impl Span {
    pub fn new(line: usize, column: usize, length: usize) -> Self {
        Span {
            line,
            column,
            length,
        }
    }
}

/// Source span for a quotation, supporting multi-line ranges
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuotationSpan {
    /// Start line (0-indexed)
    pub start_line: usize,
    /// Start column (0-indexed)
    pub start_column: usize,
    /// End line (0-indexed)
    pub end_line: usize,
    /// End column (0-indexed, exclusive)
    pub end_column: usize,
}

impl QuotationSpan {
    pub fn new(start_line: usize, start_column: usize, end_line: usize, end_column: usize) -> Self {
        QuotationSpan {
            start_line,
            start_column,
            end_line,
            end_column,
        }
    }

    /// Check if a position (line, column) falls within this span
    pub fn contains(&self, line: usize, column: usize) -> bool {
        if line < self.start_line || line > self.end_line {
            return false;
        }
        if line == self.start_line && column < self.start_column {
            return false;
        }
        if line == self.end_line && column >= self.end_column {
            return false;
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Integer literal: pushes value onto stack
    IntLiteral(i64),

    /// Floating-point literal: pushes IEEE 754 double onto stack
    FloatLiteral(f64),

    /// Boolean literal: pushes true/false onto stack
    BoolLiteral(bool),

    /// String literal: pushes string onto stack
    StringLiteral(String),

    /// Symbol literal: pushes symbol onto stack
    /// Syntax: :foo, :some-name, :ok
    /// Used for dynamic variant construction and SON.
    /// Note: Symbols are not currently interned (future optimization).
    Symbol(String),

    /// Word call: calls another word or built-in
    /// Contains the word name and optional source span for precise diagnostics
    WordCall { name: String, span: Option<Span> },

    /// Conditional: if/else/then
    ///
    /// Pops an integer from the stack (0 = zero, non-zero = non-zero)
    /// and executes the appropriate branch
    If {
        /// Statements to execute when condition is non-zero (the 'then' clause)
        then_branch: Vec<Statement>,
        /// Optional statements to execute when condition is zero (the 'else' clause)
        else_branch: Option<Vec<Statement>>,
        /// Source span for error reporting (points to 'if' keyword)
        span: Option<Span>,
    },

    /// Quotation: [ ... ]
    ///
    /// A block of deferred code (quotation/lambda)
    /// Quotations are first-class values that can be pushed onto the stack
    /// and executed later with combinators like `call`, `times`, or `while`
    ///
    /// The id field is used by the typechecker to track the inferred type
    /// (Quotation vs Closure) for this quotation. The id is assigned during parsing.
    /// The span field records the source location for LSP hover support.
    Quotation {
        id: usize,
        body: Vec<Statement>,
        span: Option<QuotationSpan>,
    },

    /// Match expression: pattern matching on union types
    ///
    /// Pops a union value from the stack and dispatches to the
    /// appropriate arm based on the variant tag.
    ///
    /// Example:
    /// ```seq
    /// match
    ///   Get -> send-response
    ///   Increment -> do-increment send-response
    ///   Report -> aggregate-add
    /// end
    /// ```
    Match {
        /// The match arms in order
        arms: Vec<MatchArm>,
        /// Source span for error reporting (points to 'match' keyword)
        span: Option<Span>,
    },
}

impl Program {
    pub fn new() -> Self {
        Program {
            includes: Vec::new(),
            unions: Vec::new(),
            words: Vec::new(),
        }
    }

    pub fn find_word(&self, name: &str) -> Option<&WordDef> {
        self.words.iter().find(|w| w.name == name)
    }

    /// Validate that all word calls reference either a defined word or a built-in
    pub fn validate_word_calls(&self) -> Result<(), String> {
        self.validate_word_calls_with_externals(&[])
    }

    /// Validate that all word calls reference a defined word, built-in, or external word.
    ///
    /// The `external_words` parameter should contain names of words available from
    /// external sources (e.g., included modules) that should be considered valid.
    pub fn validate_word_calls_with_externals(
        &self,
        external_words: &[&str],
    ) -> Result<(), String> {
        // List of known runtime built-ins
        // IMPORTANT: Keep this in sync with codegen.rs WordCall matching
        let builtins = [
            // I/O operations
            "io.write",
            "io.write-line",
            "io.read-line",
            "io.read-line+",
            "io.read-n",
            "int->string",
            "symbol->string",
            "string->symbol",
            // Command-line arguments
            "args.count",
            "args.at",
            // File operations
            "file.slurp",
            "file.exists?",
            "file.for-each-line+",
            "file.spit",
            "file.append",
            "file.delete",
            "file.size",
            // Directory operations
            "dir.exists?",
            "dir.make",
            "dir.delete",
            "dir.list",
            // String operations
            "string.concat",
            "string.length",
            "string.byte-length",
            "string.char-at",
            "string.substring",
            "char->string",
            "string.find",
            "string.split",
            "string.contains",
            "string.starts-with",
            "string.empty?",
            "string.trim",
            "string.chomp",
            "string.to-upper",
            "string.to-lower",
            "string.equal?",
            "string.json-escape",
            "string->int",
            // Symbol operations
            "symbol.=",
            // Encoding operations
            "encoding.base64-encode",
            "encoding.base64-decode",
            "encoding.base64url-encode",
            "encoding.base64url-decode",
            "encoding.hex-encode",
            "encoding.hex-decode",
            // Crypto operations
            "crypto.sha256",
            "crypto.hmac-sha256",
            "crypto.constant-time-eq",
            "crypto.random-bytes",
            "crypto.random-int",
            "crypto.uuid4",
            "crypto.aes-gcm-encrypt",
            "crypto.aes-gcm-decrypt",
            "crypto.pbkdf2-sha256",
            "crypto.ed25519-keypair",
            "crypto.ed25519-sign",
            "crypto.ed25519-verify",
            // HTTP client operations
            "http.get",
            "http.post",
            "http.put",
            "http.delete",
            // List operations
            "list.make",
            "list.push",
            "list.get",
            "list.set",
            "list.map",
            "list.filter",
            "list.fold",
            "list.each",
            "list.length",
            "list.empty?",
            // Map operations
            "map.make",
            "map.get",
            "map.set",
            "map.has?",
            "map.remove",
            "map.keys",
            "map.values",
            "map.size",
            "map.empty?",
            // Variant operations
            "variant.field-count",
            "variant.tag",
            "variant.field-at",
            "variant.append",
            "variant.last",
            "variant.init",
            "variant.make-0",
            "variant.make-1",
            "variant.make-2",
            "variant.make-3",
            "variant.make-4",
            // SON wrap aliases
            "wrap-0",
            "wrap-1",
            "wrap-2",
            "wrap-3",
            "wrap-4",
            // Integer arithmetic operations
            "i.add",
            "i.subtract",
            "i.multiply",
            "i.divide",
            "i.modulo",
            // Terse integer arithmetic
            "i.+",
            "i.-",
            "i.*",
            "i./",
            "i.%",
            // Integer comparison operations (return 0 or 1)
            "i.=",
            "i.<",
            "i.>",
            "i.<=",
            "i.>=",
            "i.<>",
            // Integer comparison operations (verbose form)
            "i.eq",
            "i.lt",
            "i.gt",
            "i.lte",
            "i.gte",
            "i.neq",
            // Stack operations (simple - no parameters)
            "dup",
            "drop",
            "swap",
            "over",
            "rot",
            "nip",
            "tuck",
            "2dup",
            "3drop",
            "pick",
            "roll",
            // Boolean operations
            "and",
            "or",
            "not",
            // Bitwise operations
            "band",
            "bor",
            "bxor",
            "bnot",
            "shl",
            "shr",
            "popcount",
            "clz",
            "ctz",
            "int-bits",
            // Channel operations
            "chan.make",
            "chan.send",
            "chan.receive",
            "chan.close",
            "chan.yield",
            // Quotation operations
            "call",
            "strand.spawn",
            "strand.weave",
            "strand.resume",
            "strand.weave-cancel",
            "yield",
            "cond",
            // TCP operations
            "tcp.listen",
            "tcp.accept",
            "tcp.read",
            "tcp.write",
            "tcp.close",
            // OS operations
            "os.getenv",
            "os.home-dir",
            "os.current-dir",
            "os.path-exists",
            "os.path-is-file",
            "os.path-is-dir",
            "os.path-join",
            "os.path-parent",
            "os.path-filename",
            "os.exit",
            "os.name",
            "os.arch",
            // Signal handling
            "signal.trap",
            "signal.received?",
            "signal.pending?",
            "signal.default",
            "signal.ignore",
            "signal.clear",
            "signal.SIGINT",
            "signal.SIGTERM",
            "signal.SIGHUP",
            "signal.SIGPIPE",
            "signal.SIGUSR1",
            "signal.SIGUSR2",
            "signal.SIGCHLD",
            "signal.SIGALRM",
            "signal.SIGCONT",
            // Terminal operations
            "terminal.raw-mode",
            "terminal.read-char",
            "terminal.read-char?",
            "terminal.width",
            "terminal.height",
            "terminal.flush",
            // Float arithmetic operations (verbose form)
            "f.add",
            "f.subtract",
            "f.multiply",
            "f.divide",
            // Float arithmetic operations (terse form)
            "f.+",
            "f.-",
            "f.*",
            "f./",
            // Float comparison operations (symbol form)
            "f.=",
            "f.<",
            "f.>",
            "f.<=",
            "f.>=",
            "f.<>",
            // Float comparison operations (verbose form)
            "f.eq",
            "f.lt",
            "f.gt",
            "f.lte",
            "f.gte",
            "f.neq",
            // Type conversions
            "int->float",
            "float->int",
            "float->string",
            "string->float",
            // Test framework operations
            "test.init",
            "test.finish",
            "test.has-failures",
            "test.assert",
            "test.assert-not",
            "test.assert-eq",
            "test.assert-eq-str",
            "test.fail",
            "test.pass-count",
            "test.fail-count",
            // Time operations
            "time.now",
            "time.nanos",
            "time.sleep-ms",
            // SON serialization
            "son.dump",
            "son.dump-pretty",
            // Stack introspection (for REPL)
            "stack.dump",
            // Regex operations
            "regex.match?",
            "regex.find",
            "regex.find-all",
            "regex.replace",
            "regex.replace-all",
            "regex.captures",
            "regex.split",
            "regex.valid?",
            // Compression operations
            "compress.gzip",
            "compress.gzip-level",
            "compress.gunzip",
            "compress.zstd",
            "compress.zstd-level",
            "compress.unzstd",
        ];

        for word in &self.words {
            self.validate_statements(&word.body, &word.name, &builtins, external_words)?;
        }

        Ok(())
    }

    /// Helper to validate word calls in a list of statements (recursively)
    fn validate_statements(
        &self,
        statements: &[Statement],
        word_name: &str,
        builtins: &[&str],
        external_words: &[&str],
    ) -> Result<(), String> {
        for statement in statements {
            match statement {
                Statement::WordCall { name, .. } => {
                    // Check if it's a built-in
                    if builtins.contains(&name.as_str()) {
                        continue;
                    }
                    // Check if it's a user-defined word
                    if self.find_word(name).is_some() {
                        continue;
                    }
                    // Check if it's an external word (from includes)
                    if external_words.contains(&name.as_str()) {
                        continue;
                    }
                    // Undefined word!
                    return Err(format!(
                        "Undefined word '{}' called in word '{}'. \
                         Did you forget to define it or misspell a built-in?",
                        name, word_name
                    ));
                }
                Statement::If {
                    then_branch,
                    else_branch,
                    span: _,
                } => {
                    // Recursively validate both branches
                    self.validate_statements(then_branch, word_name, builtins, external_words)?;
                    if let Some(eb) = else_branch {
                        self.validate_statements(eb, word_name, builtins, external_words)?;
                    }
                }
                Statement::Quotation { body, .. } => {
                    // Recursively validate quotation body
                    self.validate_statements(body, word_name, builtins, external_words)?;
                }
                Statement::Match { arms, span: _ } => {
                    // Recursively validate each match arm's body
                    for arm in arms {
                        self.validate_statements(&arm.body, word_name, builtins, external_words)?;
                    }
                }
                _ => {} // Literals don't need validation
            }
        }
        Ok(())
    }

    /// Generate constructor words for all union definitions
    ///
    /// Maximum number of fields a variant can have (limited by runtime support)
    pub const MAX_VARIANT_FIELDS: usize = 12;

    /// For each union variant, generates a `Make-VariantName` word that:
    /// 1. Takes the variant's field values from the stack
    /// 2. Pushes the variant tag (index)
    /// 3. Calls the appropriate `variant.make-N` builtin
    ///
    /// Example: For `union Message { Get { chan: Int } }`
    /// Generates: `: Make-Get ( Int -- Message ) 0 variant.make-1 ;`
    ///
    /// Returns an error if any variant exceeds the maximum field count.
    pub fn generate_constructors(&mut self) -> Result<(), String> {
        let mut new_words = Vec::new();

        for union_def in &self.unions {
            for variant in &union_def.variants {
                let constructor_name = format!("Make-{}", variant.name);
                let field_count = variant.fields.len();

                // Check field count limit before generating constructor
                if field_count > Self::MAX_VARIANT_FIELDS {
                    return Err(format!(
                        "Variant '{}' in union '{}' has {} fields, but the maximum is {}. \
                         Consider grouping fields into nested union types.",
                        variant.name,
                        union_def.name,
                        field_count,
                        Self::MAX_VARIANT_FIELDS
                    ));
                }

                // Build the stack effect: ( field_types... -- UnionType )
                // Input stack has fields in declaration order
                let mut input_stack = StackType::RowVar("a".to_string());
                for field in &variant.fields {
                    let field_type = parse_type_name(&field.type_name);
                    input_stack = input_stack.push(field_type);
                }

                // Output stack has the union type
                let output_stack =
                    StackType::RowVar("a".to_string()).push(Type::Union(union_def.name.clone()));

                let effect = Effect::new(input_stack, output_stack);

                // Build the body:
                // 1. Push the variant name as a symbol (for dynamic matching)
                // 2. Call variant.make-N which now accepts Symbol tags
                let body = vec![
                    Statement::Symbol(variant.name.clone()),
                    Statement::WordCall {
                        name: format!("variant.make-{}", field_count),
                        span: None, // Generated code, no source span
                    },
                ];

                new_words.push(WordDef {
                    name: constructor_name,
                    effect: Some(effect),
                    body,
                    source: variant.source.clone(),
                    allowed_lints: vec![],
                });
            }
        }

        self.words.extend(new_words);
        Ok(())
    }
}

/// Parse a type name string into a Type
/// Used by constructor generation to build stack effects
fn parse_type_name(name: &str) -> Type {
    match name {
        "Int" => Type::Int,
        "Float" => Type::Float,
        "Bool" => Type::Bool,
        "String" => Type::String,
        "Channel" => Type::Channel,
        other => Type::Union(other.to_string()),
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_builtin_words() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            }],
        };

        // Should succeed - i.add and io.write-line are built-ins
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_user_defined_words() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "helper".to_string(),
                    effect: None,
                    body: vec![Statement::IntLiteral(42)],
                    source: None,
                    allowed_lints: vec![],
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "helper".to_string(),
                        span: None,
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
            ],
        };

        // Should succeed - helper is defined
        assert!(program.validate_word_calls().is_ok());
    }

    #[test]
    fn test_validate_undefined_word() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "undefined_word".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            }],
        };

        // Should fail - undefined_word is not a built-in or user-defined word
        let result = program.validate_word_calls();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("undefined_word"));
        assert!(error.contains("main"));
    }

    #[test]
    fn test_validate_misspelled_builtin() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "wrte_line".to_string(),
                    span: None,
                }], // typo
                source: None,
                allowed_lints: vec![],
            }],
        };

        // Should fail with helpful message
        let result = program.validate_word_calls();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("wrte_line"));
        assert!(error.contains("misspell"));
    }

    #[test]
    fn test_generate_constructors() {
        let mut program = Program {
            includes: vec![],
            unions: vec![UnionDef {
                name: "Message".to_string(),
                variants: vec![
                    UnionVariant {
                        name: "Get".to_string(),
                        fields: vec![UnionField {
                            name: "response-chan".to_string(),
                            type_name: "Int".to_string(),
                        }],
                        source: None,
                    },
                    UnionVariant {
                        name: "Put".to_string(),
                        fields: vec![
                            UnionField {
                                name: "value".to_string(),
                                type_name: "String".to_string(),
                            },
                            UnionField {
                                name: "response-chan".to_string(),
                                type_name: "Int".to_string(),
                            },
                        ],
                        source: None,
                    },
                ],
                source: None,
            }],
            words: vec![],
        };

        // Generate constructors
        program.generate_constructors().unwrap();

        // Should have 2 constructor words
        assert_eq!(program.words.len(), 2);

        // Check Make-Get constructor
        let make_get = program
            .find_word("Make-Get")
            .expect("Make-Get should exist");
        assert_eq!(make_get.name, "Make-Get");
        assert!(make_get.effect.is_some());
        let effect = make_get.effect.as_ref().unwrap();
        // Input: ( ..a Int -- )
        // Output: ( ..a Message -- )
        assert_eq!(
            format!("{:?}", effect.outputs),
            "Cons { rest: RowVar(\"a\"), top: Union(\"Message\") }"
        );

        // Check Make-Put constructor
        let make_put = program
            .find_word("Make-Put")
            .expect("Make-Put should exist");
        assert_eq!(make_put.name, "Make-Put");
        assert!(make_put.effect.is_some());

        // Check the body generates correct code
        // Make-Get should be: :Get variant.make-1
        assert_eq!(make_get.body.len(), 2);
        match &make_get.body[0] {
            Statement::Symbol(s) if s == "Get" => {}
            other => panic!("Expected Symbol(\"Get\") for variant tag, got {:?}", other),
        }
        match &make_get.body[1] {
            Statement::WordCall { name, span: None } if name == "variant.make-1" => {}
            _ => panic!("Expected WordCall(variant.make-1)"),
        }

        // Make-Put should be: :Put variant.make-2
        assert_eq!(make_put.body.len(), 2);
        match &make_put.body[0] {
            Statement::Symbol(s) if s == "Put" => {}
            other => panic!("Expected Symbol(\"Put\") for variant tag, got {:?}", other),
        }
        match &make_put.body[1] {
            Statement::WordCall { name, span: None } if name == "variant.make-2" => {}
            _ => panic!("Expected WordCall(variant.make-2)"),
        }
    }
}
