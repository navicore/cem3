//! Shared types for the error-flag analysis: tracked flags, the abstract
//! flag-stack simulator, and lookup helpers for fallible builtin operations
//! and their checking consumers.

/// A tracked error flag with its origin
#[derive(Debug, Clone)]
pub(super) struct ErrorFlag {
    /// Line where the fallible operation was called (0-indexed)
    pub(super) created_line: usize,
    /// The operation that produced this flag
    pub(super) operation: String,
    /// Human-readable description of what failure the Bool indicates
    pub(super) description: String,
}

/// A value on the abstract stack
#[derive(Debug, Clone)]
pub(super) enum StackVal {
    /// A tracked error flag that hasn't been checked yet
    Flag(ErrorFlag),
    /// Any other value (not tracked)
    Other,
}

/// Abstract stack state for error flag tracking
#[derive(Debug, Clone)]
pub(super) struct FlagStack {
    pub(super) stack: Vec<StackVal>,
    pub(super) aux: Vec<StackVal>,
}

impl FlagStack {
    pub(super) fn new() -> Self {
        FlagStack {
            stack: Vec::new(),
            aux: Vec::new(),
        }
    }

    pub(super) fn push_other(&mut self) {
        self.stack.push(StackVal::Other);
    }

    pub(super) fn push_flag(&mut self, line: usize, operation: &str, description: &str) {
        let flag = ErrorFlag {
            created_line: line,
            operation: operation.to_string(),
            description: description.to_string(),
        };
        self.stack.push(StackVal::Flag(flag));
    }

    pub(super) fn pop(&mut self) -> Option<StackVal> {
        self.stack.pop()
    }

    pub(super) fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Join two states after branching (conservative: keep flags from either)
    pub(super) fn join(&self, other: &FlagStack) -> FlagStack {
        // Use the longer stack, preserving flags from either branch
        let len = self.stack.len().max(other.stack.len());
        let mut joined = Vec::with_capacity(len);

        for i in 0..len {
            let a = self.stack.get(i);
            let b = other.stack.get(i);
            // If either branch has a flag at this position, keep it
            let val = match (a, b) {
                (Some(StackVal::Flag(f)), _) => StackVal::Flag(f.clone()),
                (_, Some(StackVal::Flag(f))) => StackVal::Flag(f.clone()),
                _ => StackVal::Other,
            };
            joined.push(val);
        }

        // Join aux stacks similarly
        let aux_len = self.aux.len().max(other.aux.len());
        let mut joined_aux = Vec::with_capacity(aux_len);
        for i in 0..aux_len {
            let a = self.aux.get(i);
            let b = other.aux.get(i);
            let val = match (a, b) {
                (Some(StackVal::Flag(f)), _) => StackVal::Flag(f.clone()),
                (_, Some(StackVal::Flag(f))) => StackVal::Flag(f.clone()),
                _ => StackVal::Other,
            };
            joined_aux.push(val);
        }

        FlagStack {
            stack: joined,
            aux: joined_aux,
        }
    }
}

/// Information about a fallible operation.
pub(super) struct FallibleOpInfo {
    /// Number of values the operation consumes from the stack
    pub(super) inputs: usize,
    /// Number of values pushed BEFORE the Bool (e.g., 1 for `( -- String Bool )`)
    pub(super) values_before_bool: usize,
    /// Human-readable description of what failure the Bool indicates
    pub(super) description: &'static str,
}

/// Single source of truth for all fallible operations.
/// Maps operation name → (inputs consumed, values before Bool, description).
pub(super) fn fallible_op_info(name: &str) -> Option<FallibleOpInfo> {
    let (inputs, values_before_bool, description) = match name {
        // Division — ( Int Int -- Int Bool )
        "i./" | "i.divide" => (2, 1, "division by zero"),
        "i.%" | "i.modulo" => (2, 1, "modulo by zero"),
        "i.pow" => (2, 1, "negative exponent or overflow"),

        // File I/O
        "file.slurp" => (1, 1, "file read failure"),
        "file.spit" => (2, 0, "file write failure"),
        "file.append" => (2, 0, "file append failure"),
        "file.delete" => (1, 0, "file delete failure"),
        "file.size" => (1, 1, "file size failure"),
        "dir.make" => (1, 0, "directory creation failure"),
        "dir.delete" => (1, 0, "directory delete failure"),
        "dir.list" => (1, 1, "directory list failure"),

        // I/O — ( -- String Bool )
        "io.read-line" => (0, 1, "read failure"),

        // Parsing — ( String -- value Bool )
        "string->int" => (1, 1, "parse failure"),
        "string->float" => (1, 1, "parse failure"),

        // Channels
        "chan.send" => (2, 0, "send failure"),
        "chan.receive" => (1, 1, "receive failure"),

        // Map/List lookups
        "map.get" => (2, 1, "key not found"),
        "list.get" => (2, 1, "index out of bounds"),
        "list.set" => (3, 1, "index out of bounds"),
        "list.first" => (1, 1, "empty list"),
        "list.last" => (1, 1, "empty list"),

        // TCP (net.tcp.*)
        "net.tcp.listen" => (1, 1, "listen failure"),
        "net.tcp.connect" => (2, 1, "TCP connect failure"),
        "net.tcp.accept" => (1, 1, "accept failure"),
        "net.tcp.local-port" => (1, 1, "local-port lookup failure"),
        "net.tcp.read" => (1, 1, "read failure"),
        "net.tcp.write" => (2, 0, "write failure"),
        "net.tcp.close" => (1, 0, "close failure"),

        // DNS (net.dns.*) — ( String -- List Bool )
        "net.dns.resolve" => (1, 1, "DNS resolution failure"),

        // TLS (net.tls.*) — ( Socket String -- Socket Bool )
        "net.tls.client" => (2, 1, "TLS handshake failure"),

        // OS
        "os.getenv" => (1, 1, "env var not set"),
        "os.home-dir" => (0, 1, "home dir not available"),
        "os.current-dir" => (0, 1, "current dir not available"),
        "os.path-parent" => (1, 1, "no parent path"),
        "os.path-filename" => (1, 1, "no filename"),

        // Regex
        "regex.find" => (2, 1, "no match or invalid regex"),
        "regex.find-all" => (2, 1, "invalid regex"),
        "regex.replace" => (3, 1, "invalid regex"),
        "regex.replace-all" => (3, 1, "invalid regex"),
        "regex.captures" => (2, 1, "invalid regex"),
        "regex.split" => (2, 1, "invalid regex"),

        // Encoding
        "encoding.base64-decode" => (1, 1, "invalid base64"),
        "encoding.base64url-decode" => (1, 1, "invalid base64url"),
        "encoding.hex-decode" => (1, 1, "invalid hex"),

        // Crypto
        "crypto.aes-gcm-encrypt" => (2, 1, "encryption failure"),
        "crypto.aes-gcm-decrypt" => (2, 1, "decryption failure"),
        "crypto.pbkdf2-sha256" => (3, 1, "key derivation failure"),
        "crypto.ed25519-sign" => (2, 1, "signing failure"),

        // Compression
        "compress.gzip" => (1, 1, "compression failure"),
        "compress.gzip-level" => (2, 1, "compression failure"),
        "compress.gunzip" => (1, 1, "decompression failure"),
        "compress.zstd" => (1, 1, "compression failure"),
        "compress.zstd-level" => (2, 1, "compression failure"),
        "compress.unzstd" => (1, 1, "decompression failure"),

        _ => return None,
    };
    Some(FallibleOpInfo {
        inputs,
        values_before_bool,
        description,
    })
}

/// Words that consume a Bool as an error-checking mechanism
pub(super) fn is_checking_consumer(name: &str) -> bool {
    // `if` is handled structurally (it's a Statement::If, not a WordCall)
    // `cond` consumes Bools as conditions
    name == "cond"
}
