//! Program-level AST methods: word-call validation, auto-generated variant
//! constructors (`Make-Variant`), and type fix-up for union types declared
//! in stack effects.

use crate::types::{Effect, StackType, Type};

use super::{Program, SourceLocation, Statement, WordDef};

/// Names of all runtime built-ins recognised by the validator.
///
/// IMPORTANT: Keep this in sync with codegen.rs WordCall matching.
const BUILTINS: &[&str] = &[
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
    "file.for-each-line",
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
    "net.http.get",
    "net.http.post",
    "net.http.put",
    "net.http.delete",
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
    "i.pow",
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
    "net.tcp.listen",
    "net.tcp.connect",
    "net.tcp.accept",
    "net.tcp.local-port",
    "net.tcp.read",
    "net.tcp.write",
    "net.tcp.close",
    // Socket <-> Int casts (FFI escape hatches)
    "fd->socket",
    "socket->fd",
    // UDP operations
    "net.udp.bind",
    "net.udp.send-to",
    "net.udp.receive-from",
    "net.udp.close",
    // DNS operations
    "net.dns.resolve",
    // TLS operations
    "net.tls.client",
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
    // Float math — roots/powers
    "f.sqrt",
    "f.cbrt",
    "f.pow",
    // Float math — exp/log
    "f.exp",
    "f.ln",
    "f.log10",
    "f.log2",
    // Float math — trig
    "f.sin",
    "f.cos",
    "f.tan",
    "f.asin",
    "f.acos",
    "f.atan",
    "f.atan2",
    // Float math — rounding
    "f.floor",
    "f.ceil",
    "f.round",
    "f.trunc",
    // Float constants
    "f.pi",
    "f.e",
    "f.tau",
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
        for word in &self.words {
            self.validate_statements(&word.body, &word.name, BUILTINS, external_words)?;
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
                    if builtins.contains(&name.as_str()) {
                        continue;
                    }
                    if self.find_word(name).is_some() {
                        continue;
                    }
                    if external_words.contains(&name.as_str()) {
                        continue;
                    }
                    // v7.0 rename: pre-net.* networking names get a targeted
                    // hint instead of the generic "did you misspell" message,
                    // so the migration is obvious.
                    if let Some(replacement) = v7_renamed_to(name) {
                        return Err(format!(
                            "'{}' was renamed to '{}' in v7.0 (called in word '{}'). \
                             See docs/MIGRATION_7_0.md.",
                            name, replacement, word_name
                        ));
                    }
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
                    self.validate_statements(then_branch, word_name, builtins, external_words)?;
                    if let Some(eb) = else_branch {
                        self.validate_statements(eb, word_name, builtins, external_words)?;
                    }
                }
                Statement::Quotation { body, .. } => {
                    self.validate_statements(body, word_name, builtins, external_words)?;
                }
                Statement::Match { arms, span: _ } => {
                    for arm in arms {
                        self.validate_statements(&arm.body, word_name, builtins, external_words)?;
                    }
                }
                _ => {} // Literals don't need validation
            }
        }
        Ok(())
    }

    /// Maximum number of fields a variant can have (limited by runtime support)
    const MAX_VARIANT_FIELDS: usize = 12;

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

                let union_ty = Type::Union(union_def.name.clone());
                let source = variant.source.clone();
                let field_types: Vec<Type> = variant
                    .fields
                    .iter()
                    .map(|f| parse_type_name(&f.type_name))
                    .collect();

                // 1. Constructor: Make-VariantName ( field_types -- UnionType )
                new_words.push(make_helper_word(
                    format!("Make-{}", variant.name),
                    &field_types,
                    union_ty.clone(),
                    vec![
                        Statement::Symbol(variant.name.clone()),
                        Statement::WordCall {
                            name: format!("variant.make-{}", field_count),
                            span: None,
                        },
                    ],
                    source.clone(),
                ));

                // 2. Predicate: is-VariantName? ( UnionType -- Bool )
                new_words.push(make_helper_word(
                    format!("is-{}?", variant.name),
                    std::slice::from_ref(&union_ty),
                    Type::Bool,
                    vec![
                        Statement::WordCall {
                            name: "variant.tag".to_string(),
                            span: None,
                        },
                        Statement::Symbol(variant.name.clone()),
                        Statement::WordCall {
                            name: "symbol.=".to_string(),
                            span: None,
                        },
                    ],
                    source.clone(),
                ));

                // 3. Field accessors: VariantName-fieldname ( UnionType -- FieldType )
                for (index, field) in variant.fields.iter().enumerate() {
                    new_words.push(make_helper_word(
                        format!("{}-{}", variant.name, field.name),
                        std::slice::from_ref(&union_ty),
                        field_types[index].clone(),
                        vec![
                            Statement::IntLiteral(index as i64),
                            Statement::WordCall {
                                name: "variant.field-at".to_string(),
                                span: None,
                            },
                        ],
                        source.clone(),
                    ));
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
        let union_names: std::collections::HashSet<String> =
            self.unions.iter().map(|u| u.name.clone()).collect();

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

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a generated helper `WordDef` whose effect is
/// `( ..a inputs -- ..a output )`. Used to emit the constructor /
/// predicate / accessor words for each union variant.
fn make_helper_word(
    name: String,
    inputs: &[Type],
    output: Type,
    body: Vec<Statement>,
    source: Option<SourceLocation>,
) -> WordDef {
    let mut input_stack = StackType::RowVar("a".to_string());
    for ty in inputs {
        input_stack = input_stack.push(ty.clone());
    }
    let output_stack = StackType::RowVar("a".to_string()).push(output);
    WordDef {
        name,
        effect: Some(Effect::new(input_stack, output_stack)),
        body,
        source,
        allowed_lints: vec![],
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
        "Socket" => Type::Socket,
        other => Type::Union(other.to_string()),
    }
}

/// Map a pre-v7.0 networking word to its current name, or None if unknown.
/// Used to turn the generic "Undefined word" error into a targeted migration
/// hint when a user calls one of the renamed builtins. Remove this table
/// in v8.0.
fn v7_renamed_to(name: &str) -> Option<&'static str> {
    Some(match name {
        "tcp.listen" => "net.tcp.listen",
        "tcp.accept" => "net.tcp.accept",
        "tcp.read" => "net.tcp.read",
        "tcp.write" => "net.tcp.write",
        "tcp.close" => "net.tcp.close",
        "udp.bind" => "net.udp.bind",
        "udp.send-to" => "net.udp.send-to",
        "udp.receive-from" => "net.udp.receive-from",
        "udp.close" => "net.udp.close",
        "http.get" => "net.http.get",
        "http.post" => "net.http.post",
        "http.put" => "net.http.put",
        "http.delete" => "net.http.delete",
        // imath stdlib pass-through removed in v7.0; route to the underlying
        // builtin so callers learn the right name.
        "mod" => "i.modulo",
        _ => return None,
    })
}
