//! Program-level AST methods: word-call validation, auto-generated variant
//! constructors (`Make-Variant`), and type fix-up for union types declared
//! in stack effects.

use crate::types::{Effect, StackType, Type};

use super::{Program, Statement, WordDef};

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
            "string.join",
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
            "list.reverse",
            "list.first",
            "list.last",
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
            "map.each",
            "map.fold",
            // Variant operations
            "variant.field-count",
            "variant.tag",
            "variant.field-at",
            "variant.append",
            "variant.first",
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
            // Aux stack operations
            ">aux",
            "aux>",
            // Boolean operations
            "and",
            "or",
            "not",
            // Bitwise operations
            "band",
            "bor",
            "bxor",
            "bnot",
            "i.neg",
            "negate",
            // Arithmetic sugar (resolved to concrete ops by typechecker)
            "+",
            "-",
            "*",
            "/",
            "%",
            "=",
            "<",
            ">",
            "<=",
            ">=",
            "<>",
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
            // Dataflow combinators
            "dip",
            "keep",
            "bi",
            "if",
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
            // UDP operations
            "udp.bind",
            "udp.send-to",
            "udp.receive-from",
            "udp.close",
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
            // Byte construction (binary protocol encoders)
            "int.to-bytes-i32-be",
            "float.to-bytes-f32-be",
            // Test framework operations
            "test.init",
            "test.set-name",
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

    /// Generate helper words for union types:
    /// 1. Constructors: `Make-VariantName` - creates variant instances
    /// 2. Predicates: `is-VariantName?` - tests if value is a specific variant
    /// 3. Accessors: `VariantName-fieldname` - extracts field values (RFC #345)
    ///
    /// Example: For `union Message { Get { chan: Int } }`
    /// Generates:
    ///   `: Make-Get ( Int -- Message ) :Get variant.make-1 ;`
    ///   `: is-Get? ( Message -- Bool ) variant.tag :Get symbol.= ;`
    ///   `: Get-chan ( Message -- Int ) 0 variant.field-at ;`
    ///
    /// Returns an error if any variant exceeds the maximum field count.
    pub fn generate_constructors(&mut self) -> Result<(), String> {
        let mut new_words = Vec::new();

        for union_def in &self.unions {
            for variant in &union_def.variants {
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

                // 1. Generate constructor: Make-VariantName
                let constructor_name = format!("Make-{}", variant.name);
                let mut input_stack = StackType::RowVar("a".to_string());
                for field in &variant.fields {
                    let field_type = parse_type_name(&field.type_name);
                    input_stack = input_stack.push(field_type);
                }
                let output_stack =
                    StackType::RowVar("a".to_string()).push(Type::Union(union_def.name.clone()));
                let effect = Effect::new(input_stack, output_stack);
                let body = vec![
                    Statement::Symbol(variant.name.clone()),
                    Statement::WordCall {
                        name: format!("variant.make-{}", field_count),
                        span: None,
                    },
                ];
                new_words.push(WordDef {
                    name: constructor_name,
                    effect: Some(effect),
                    body,
                    source: variant.source.clone(),
                    allowed_lints: vec![],
                });

                // 2. Generate predicate: is-VariantName?
                // Effect: ( UnionType -- Bool )
                // Body: variant.tag :VariantName symbol.=
                let predicate_name = format!("is-{}?", variant.name);
                let predicate_input =
                    StackType::RowVar("a".to_string()).push(Type::Union(union_def.name.clone()));
                let predicate_output = StackType::RowVar("a".to_string()).push(Type::Bool);
                let predicate_effect = Effect::new(predicate_input, predicate_output);
                let predicate_body = vec![
                    Statement::WordCall {
                        name: "variant.tag".to_string(),
                        span: None,
                    },
                    Statement::Symbol(variant.name.clone()),
                    Statement::WordCall {
                        name: "symbol.=".to_string(),
                        span: None,
                    },
                ];
                new_words.push(WordDef {
                    name: predicate_name,
                    effect: Some(predicate_effect),
                    body: predicate_body,
                    source: variant.source.clone(),
                    allowed_lints: vec![],
                });

                // 3. Generate field accessors: VariantName-fieldname
                // Effect: ( UnionType -- FieldType )
                // Body: N variant.field-at
                for (index, field) in variant.fields.iter().enumerate() {
                    let accessor_name = format!("{}-{}", variant.name, field.name);
                    let field_type = parse_type_name(&field.type_name);
                    let accessor_input = StackType::RowVar("a".to_string())
                        .push(Type::Union(union_def.name.clone()));
                    let accessor_output = StackType::RowVar("a".to_string()).push(field_type);
                    let accessor_effect = Effect::new(accessor_input, accessor_output);
                    let accessor_body = vec![
                        Statement::IntLiteral(index as i64),
                        Statement::WordCall {
                            name: "variant.field-at".to_string(),
                            span: None,
                        },
                    ];
                    new_words.push(WordDef {
                        name: accessor_name,
                        effect: Some(accessor_effect),
                        body: accessor_body,
                        source: variant.source.clone(), // Use variant's source for field accessors
                        allowed_lints: vec![],
                    });
                }
            }
        }

        self.words.extend(new_words);
        Ok(())
    }

    /// RFC #345: Fix up type variables in stack effects that should be union types
    ///
    /// When parsing files with includes, type variables like "Message" in
    /// `( Message -- Int )` may be parsed as `Type::Var("Message")` if the
    /// union definition is in an included file. After resolving includes,
    /// we know all union names and can convert these to `Type::Union("Message")`.
    ///
    /// This ensures proper nominal type checking for union types across files.
    pub fn fixup_union_types(&mut self) {
        // Collect all union names from the program
        let union_names: std::collections::HashSet<String> =
            self.unions.iter().map(|u| u.name.clone()).collect();

        // Fix up types in all word effects
        for word in &mut self.words {
            if let Some(ref mut effect) = word.effect {
                Self::fixup_stack_type(&mut effect.inputs, &union_names);
                Self::fixup_stack_type(&mut effect.outputs, &union_names);
            }
        }
    }

    /// Recursively fix up types in a stack type
    fn fixup_stack_type(stack: &mut StackType, union_names: &std::collections::HashSet<String>) {
        match stack {
            StackType::Empty | StackType::RowVar(_) => {}
            StackType::Cons { rest, top } => {
                Self::fixup_type(top, union_names);
                Self::fixup_stack_type(rest, union_names);
            }
        }
    }

    /// Fix up a single type, converting Type::Var to Type::Union if it matches a union name
    fn fixup_type(ty: &mut Type, union_names: &std::collections::HashSet<String>) {
        match ty {
            Type::Var(name) if union_names.contains(name) => {
                *ty = Type::Union(name.clone());
            }
            Type::Quotation(effect) => {
                Self::fixup_stack_type(&mut effect.inputs, union_names);
                Self::fixup_stack_type(&mut effect.outputs, union_names);
            }
            Type::Closure { effect, captures } => {
                Self::fixup_stack_type(&mut effect.inputs, union_names);
                Self::fixup_stack_type(&mut effect.outputs, union_names);
                for cap in captures {
                    Self::fixup_type(cap, union_names);
                }
            }
            _ => {}
        }
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
