# Batteries Included: Seq Standard Library Vision

## Philosophy

Inspired by Go's "batteries included" approach:
- **Opinionated**: One obvious way to do things
- **Self-sufficient**: Build real applications without external dependencies
- **Cohesive**: Consistent naming, patterns, and idioms across the stdlib
- **Practical**: Focus on what developers actually need, not academic completeness

## The Rust Advantage

Seq's runtime is implemented in Rust, which provides a massive architectural advantage for building a batteries-included stdlib. Instead of:

- Writing crypto from scratch (dangerous, years of work)
- Binding to C libraries like OpenSSL (complex, CVE-prone, platform headaches)
- Building HTTP/TLS stacks from first principles
- Maintaining fragile C FFI bindings

Seq can leverage Rust's ecosystem directly:

| Capability | Rust Crate | Quality | FFI Effort |
|------------|------------|---------|------------|
| **Crypto hashing** | `sha2` | RustCrypto, audited | ~1 day |
| **HMAC** | `hmac` | RustCrypto, audited | ~1 day |
| **Encryption** | `aes-gcm` | RustCrypto, audited | ~1 day |
| **Signatures** | `ed25519-dalek` | Audited, widely used | ~1 day |
| **HTTP client** | `ureq` | Pure Rust, minimal | 2-3 days |
| **TLS** | `rustls` | Memory-safe, modern | Comes with ureq |
| **Regex** | `regex` | Fastest in class | 1-2 days |
| **Compression** | `flate2`, `zstd` | Fast, well-maintained | 1 day each |
| **Random** | `rand` | Industry standard | Few hours |
| **UUID** | `uuid` | Complete implementation | Few hours |
| **Database** | `rusqlite` | Mature, stable | 2-3 days |

### Pattern Already Proven

This isn't theoretical - Seq already uses this pattern successfully:

| Existing Builtin | Rust Foundation |
|------------------|-----------------|
| TCP networking | `may` (coroutine-aware) |
| File I/O | `std::fs` |
| Channels | `crossbeam` |
| Time | `std::time` |
| String ops | `std::string` |
| Base64/Hex encoding | `base64`, `hex` |
| Arena allocator | Custom, but Rust memory safety |

Each builtin is a thin FFI wrapper that exposes Rust functionality to Seq. Adding crypto, HTTP client, or regex follows the exact same pattern.

### Why This Matters

1. **Security**: Audited Rust crates vs. hand-rolled crypto
2. **Speed**: Zero-cost abstractions, no interpreter overhead
3. **Reliability**: Rust's type system catches bugs at compile time
4. **Velocity**: Days to add features, not months
5. **Maintenance**: Crate updates flow through automatically
6. **Cross-platform**: Rust handles platform differences

This is Seq's unfair advantage: a concatenative language with the full power of the Rust ecosystem behind it.

---

## Feature Flags & Binary Size

As the runtime grows with crypto, HTTP, regex, etc., binary size becomes a concern. Not every application needs every capability. Cargo feature flags let users opt-in to what they need.

### Proposed Feature Structure

```toml
# Cargo.toml for seq-runtime
[features]
default = ["core"]
core = []                           # Stack ops, I/O, channels - always on
full = ["http", "crypto", "regex"]  # Everything

# Opt-in capabilities
http = ["dep:ureq", "dep:rustls"]   # HTTP client, adds ~1-2MB
crypto = ["dep:sha2", "dep:aes-gcm", "dep:ed25519-dalek"]  # Crypto, adds ~500KB
regex = ["dep:regex"]               # Regex, adds ~1MB
compression = ["dep:flate2"]        # Gzip/deflate, adds ~200KB
sqlite = ["dep:rusqlite"]           # SQLite, adds ~1MB
```

### Binary Size Estimates

| Configuration | Approx Size | Use Case |
|---------------|-------------|----------|
| `core` only | ~2-3MB | Embedded, CLI tools, scripts |
| `core` + `http` | ~4-5MB | API clients, web scrapers |
| `core` + `crypto` | ~3-4MB | Security tools, auth services |
| `full` | ~6-8MB | Full web applications |

### Compile-Time Gating

```rust
// In runtime builtins
#[cfg(feature = "crypto")]
pub fn builtin_sha256(stack: *mut Stack) -> *mut Stack {
    // ...
}

#[cfg(not(feature = "crypto"))]
pub fn builtin_sha256(_: *mut Stack) -> *mut Stack {
    panic!("sha256 requires --features crypto")
}
```

### Compiler Integration

The compiler could check for required features:

```seq
// Error at compile time if crypto feature not enabled
"hello" crypto.sha256
// Error: 'crypto.sha256' requires runtime feature 'crypto'
// Hint: Compile with: cargo build --features crypto
```

Or at link time:
```
error: undefined symbol: patch_seq_sha256
note: Enable the 'crypto' feature in seq-runtime
```

### User Experience

```bash
# Minimal build for a simple script
seq build --features core myapp.seq

# Full batteries for a web service
seq build --features full server.seq

# Specific capabilities
seq build --features "core,http,crypto" api-client.seq
```

### Project Configuration

In `seq.toml`:
```toml
[package]
name = "my-api"

[features]
runtime = ["http", "crypto"]  # Only pull in what you need
```

This keeps the core runtime lean (~2-3MB) while allowing full batteries (~6-8MB) for applications that need them.

---

## Current State

### Runtime Builtins (Rust FFI)

| Category | Capabilities |
|----------|--------------|
| **Core** | Stack ops, arithmetic, booleans, bitwise |
| **Types** | Int, Float, Bool, String, Symbol, List, Map, Variant |
| **Strings** | Concat, split, trim, case conversion, JSON escape |
| **I/O** | stdin/stdout, file read/write, path operations |
| **Concurrency** | Channels, strands (green threads), weave (coroutines) |
| **Networking** | TCP listen/accept/connect/read/write |
| **Time** | Unix timestamp, high-res time, sleep |
| **Testing** | Assertions, test runner, pass/fail counts |
| **Serialization** | SON format (Seq Object Notation) |
| **Encoding** | Base64 (standard + URL-safe), Hex |
| **OS** | Args, env vars, path operations, exec, exit |

### Standard Library (Pure Seq)

Located in `crates/compiler/stdlib/` (~3800 lines):

| Module | Lines | Description |
|--------|-------|-------------|
| `json.seq` | 1234 | Full JSON parser and encoder |
| `yaml.seq` | 750 | YAML parser |
| `result.seq` | 268 | Result/Option monadic error handling |
| `http.seq` | 190 | HTTP response building, request parsing |
| `imath.seq` | 145 | Integer math utilities (abs, min, max, clamp) |
| `fmath.seq` | 109 | Float math utilities |
| `list.seq` | 55 | List helpers |
| `son.seq` | 57 | SON serialization helpers |
| `stack-utils.seq` | 46 | Stack manipulation utilities |
| `map.seq` | 30 | Map helpers |

### HTTP Server Example

`examples/http/http_server.seq` (18KB) demonstrates:
- Concurrent request handling with strands
- Channel-based worker dispatch
- HTTP routing with `cond`
- Closure capture for connection handling

```seq
# Working HTTP server pattern
8080 tcp.listen
[ conn-id |
  conn-id tcp.read
  http-request-path
  cond
    [ "/health" string.equal? ] [ drop "OK" http-ok ]
    [ "/api" string.starts-with ] [ handle-api ]
    [ true ] [ drop "Not Found" http-not-found ]
  end
  conn-id tcp.write
  conn-id tcp.close
] accept-loop
```

### Gaps for "Batteries Included"

| Category | Status | Priority |
|----------|--------|----------|
| **HTTP client** | Not started | High |
| **Regex** | Not started | Medium |
| **Crypto** | Not started | Medium |
| **TLS/HTTPS** | Not started | Medium |
| **Templates** | Not started | Medium |
| **`seq fmt`** | Not started | Medium |
| **`seq.toml`** | Not started | Medium |
| **UUID** | Not started | Low |
| **Logging** | Not started | Low |
| **Compression** | Not started | Low |
| **Database** | Not started | Future |
| **HTML parsing** | Not started | Future |

### Already Mature

| Category | Status |
|----------|--------|
| **JSON** | Complete (1234 lines) |
| **YAML** | Complete (750 lines) |
| **HTTP server** | Working (helpers + example) |
| **LSP** | Complete (2200+ lines) - diagnostics, completions |
| **REPL** | Complete - with LSP integration, vim keybindings |
| **Testing** | Complete (builtin) |
| **Result/Option** | Complete (268 lines) |
| **Base64/Hex** | Complete (builtin) - standard, URL-safe, hex |

---

## Standard Library Structure

### Proposed Module Organization

```
stdlib/
├── core/           # Prelude, always loaded
│   ├── prelude.seq     # Common combinators, utilities
│   └── option.seq      # Option type and operations
│
├── text/           # Text processing
│   ├── json.seq        # JSON encode/decode
│   ├── regex.seq       # Regular expressions
│   └── template.seq    # String templating
│
├── net/            # Networking
│   ├── http.seq        # HTTP client and server
│   ├── url.seq         # URL parsing
│   └── tcp.seq         # Low-level TCP (wraps builtins)
│
├── crypto/         # Cryptography
│   ├── hash.seq        # SHA256, MD5, etc.
│   ├── hmac.seq        # HMAC
│   └── rand.seq        # Cryptographic random
│
├── io/             # I/O utilities
│   ├── file.seq        # File operations (wraps builtins)
│   ├── path.seq        # Path manipulation
│   └── buffer.seq      # Buffered I/O
│
├── time/           # Time and dates
│   ├── time.seq        # Timestamps, durations
│   └── format.seq      # Date/time formatting
│
├── encoding/       # Data formats (builtins available)
│   ├── base64          # encoding.base64-encode/decode (builtin)
│   ├── base64url       # encoding.base64url-encode/decode (builtin)
│   ├── hex             # encoding.hex-encode/decode (builtin)
│   └── son.seq         # SON format (wraps builtins)
│
├── testing/        # Testing framework
│   └── test.seq        # Test runner, assertions
│
└── ui/             # UI framework (future)
    ├── html.seq        # HTML generation
    ├── css.seq         # CSS utilities
    └── component.seq   # Component model
```

### Module Loading

```seq
// Import entire module
include "stdlib/net/http"

// Use qualified names
"https://api.example.com" http.get
response http.body
```

---

## Priority 1: HTTP Client

JSON and HTTP server basics already exist. The main gap is an **HTTP client**.

### Current HTTP Capabilities

**Server-side (exists):**
- `tcp.listen`, `tcp.accept`, `tcp.read`, `tcp.write`, `tcp.close`
- `http-ok`, `http-not-found`, `http-response` (response builders)
- `http-request-path`, `http-request-method` (request parsing)
- Working concurrent server example in `examples/http/http_server.seq`

**JSON (exists - 1234 lines):**
- Full parser and encoder in `stdlib/json.seq`
- Handles objects, arrays, strings, numbers, booleans, null

### Missing: HTTP Client

Currently no way to make outbound HTTP requests. Options:

**Option A: Pure Seq over TCP**
```seq
// Build HTTP/1.1 request manually
"example.com" 80 tcp.connect
"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n" swap tcp.write
tcp.read http-parse-response
```

Pros: No new FFI. Cons: No TLS, complex parsing.

**Option B: FFI to Rust HTTP client**
```seq
// New builtins wrapping ureq or reqwest
"https://api.example.com/users" http.get
// ( String -- Response )

response http.status   // ( Response -- Int )
response http.body     // ( Response -- String )
```

Pros: TLS support, proper HTTP handling. Cons: New FFI surface.

**Recommendation**: Option B with minimal API:
- `http.get` ( url -- response )
- `http.post` ( url body content-type -- response )
- `http.status` ( response -- status-code )
- `http.body` ( response -- body-string )
- `http.header` ( response header-name -- header-value )

### HTTP Server Improvements

The existing server works but could use:
- **Path parameters**: `/users/:id` → extract `id`
- **Query string parsing**: `/search?q=foo` → extract `q`
- **Request body access**: For POST/PUT handlers
- **Content-Type helpers**: `http-json-ok`, `http-html-ok`

```seq
# Proposed additions to stdlib/http.seq

: http-json-ok ( String -- String )
  dup string.byte-length int->string
  "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: "
  swap string.concat "\r\n\r\n" string.concat swap string.concat ;

: http-param ( String String -- String )
  # Extract path parameter: "/users/123" ":id" -> "123"
  ... ;

: http-query ( String String -- String )
  # Extract query param: "?q=foo&n=10" "q" -> "foo"
  ... ;
```

---

## Priority 2: Developer Experience

### Package Management

**Goals:**
- Single canonical source (like Go modules)
- Reproducible builds (lockfile)
- No central registry required (git-based)

**Proposed `seq.toml`:**
```toml
[package]
name = "my-app"
version = "0.1.0"
seq-version = "0.20"

[dependencies]
"github.com/example/utils" = "v1.2.0"
"github.com/example/http-middleware" = "v0.5.0"

[dev-dependencies]
"github.com/example/test-helpers" = "v1.0.0"
```

**Commands:**
```bash
seq init           # Create new project
seq build          # Compile
seq run            # Build and run
seq test           # Run tests
seq fmt            # Format code
seq get            # Add dependency
seq mod tidy       # Clean up dependencies
```

### Code Formatter (`seq fmt`)

**Opinionated rules:**
- 2-space indentation
- Spaces around operators in effect declarations
- One word per line in long definitions
- Consistent brace style for quotations

```seq
// Before
: process-data (Int String--String)
[dup i.> 0][swap string.concat]while ;

// After
: process-data ( Int String -- String )
  [ dup 0 i.> ] [ swap string.concat ] while ;
```

### Language Server Protocol (LSP)

Essential for IDE adoption:
- Go-to-definition for words
- Hover for stack effects and docs
- Autocomplete for builtins and user words
- Error diagnostics as you type
- Find references

---

## Priority 3: Regex & Templates

### Regular Expressions

```seq
// Match
"hello world" "wo.ld" regex.match?    // ( String String -- Bool )

// Find all matches
"a1 b2 c3" "[a-z][0-9]" regex.find-all  // ( String String -- List )

// Replace
"hello world" "world" "Seq" regex.replace  // ( String String String -- String )

// Compiled regex for performance
"[a-z]+" regex.compile                 // ( String -- Regex )
"hello" compiled-regex regex.match?    // ( String Regex -- Bool )

// Capture groups
"2024-01-15" "(\d+)-(\d+)-(\d+)" regex.captures
// ( String String -- List ) returns ["2024", "01", "15"]
```

### String Templates

```seq
// Simple interpolation
{ "name": "Alice", "count": 42 }
"Hello {{name}}, you have {{count}} messages."
template.render
// ( Map String -- String )

// HTML templates (auto-escaping)
{ "user": "<script>alert('xss')</script>" }
"<p>Welcome, {{user}}</p>"
template.render-html
// Output: <p>Welcome, &lt;script&gt;alert('xss')&lt;/script&gt;</p>

// Template files
"templates/email.html" template.load
data swap template.render
```

---

## Priority 4: Cryptography

Crypto is essential for real-world applications but often requires hunting through external packages. A batteries-included approach means shipping these out of the box.

### Tier 1: Essential (FFI to Rust crates)

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.sha256` | `sha2` | Checksums, content addressing, password hashing input |
| `crypto.sha512` | `sha2` | Higher security hashing |
| `crypto.hmac-sha256` | `hmac` + `sha2` | Webhook verification, JWT signing, API auth |
| `crypto.random-bytes` | `rand` | Tokens, nonces, salts, session IDs |

```seq
// Hashing
"hello world" crypto.sha256          // ( String -- String ) hex-encoded
"hello world" crypto.sha256-bytes    // ( String -- Bytes ) raw bytes

// HMAC for API authentication
"webhook-payload" "secret-key" crypto.hmac-sha256
// ( message key -- signature )

// Verify webhook signature
received-sig computed-sig crypto.constant-time-eq
// ( String String -- Bool ) timing-safe comparison

// Generate secure random token
32 crypto.random-bytes crypto.hex-encode
// ( n -- String ) 32 random bytes as 64-char hex string

// Generate UUID v4
crypto.uuid4    // ( -- String ) "550e8400-e29b-41d4-a716-446655440000"
```

### Tier 2: Encryption

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.aes-gcm-encrypt` | `aes-gcm` | Encrypting data at rest, secure storage |
| `crypto.aes-gcm-decrypt` | `aes-gcm` | Decrypting data |

```seq
// Symmetric encryption (AES-256-GCM)
plaintext key crypto.aes-gcm-encrypt   // ( String String -- String ) base64 ciphertext
ciphertext key crypto.aes-gcm-decrypt  // ( String String -- String Result )

// Key derivation from password
"user-password" "salt" 100000 crypto.pbkdf2-sha256
// ( password salt iterations -- key )
```

### Tier 3: Signatures & Key Exchange

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.ed25519-sign` | `ed25519-dalek` | Digital signatures |
| `crypto.ed25519-verify` | `ed25519-dalek` | Signature verification |
| `crypto.ed25519-keypair` | `ed25519-dalek` | Generate signing keys |

```seq
// Generate keypair
crypto.ed25519-keypair    // ( -- public-key private-key )

// Sign a message
message private-key crypto.ed25519-sign    // ( String String -- String )

// Verify signature
message signature public-key crypto.ed25519-verify  // ( String String String -- Bool )
```

### Encoding Helpers (Implemented)

Available now as `encoding.*` builtins:

```seq
// Base64 (standard with padding)
"hello" encoding.base64-encode    // ( String -- String ) "aGVsbG8="
"aGVsbG8=" encoding.base64-decode // ( String -- String Bool )

// URL-safe Base64 (no padding, for JWTs/URLs)
data encoding.base64url-encode    // ( String -- String )
encoded encoding.base64url-decode // ( String -- String Bool )

// Hex (lowercase output, case-insensitive decode)
"hello" encoding.hex-encode       // ( String -- String ) "68656c6c6f"
"68656c6c6f" encoding.hex-decode  // ( String -- String Bool )
```

### Implementation Priority

| Phase | APIs | Status |
|-------|------|--------|
| **Phase 1** | sha256, hmac-sha256, random-bytes, uuid4 | Not started |
| **Phase 2** | base64, hex, constant-time-eq | **Base64/Hex complete** |
| **Phase 3** | aes-gcm, pbkdf2 | Not started |
| **Phase 4** | ed25519 | Not started |

Phase 1 unlocks: JWT verification, webhook handling, secure tokens, API authentication.

---

## Future: UI Framework

### Philosophy

Not React-for-Seq. Instead:
- **Server-rendered HTML** with minimal client JS
- **HTMX-style interactions** for dynamic updates
- **Component model** for reusable pieces
- **CSS utilities** or integration with Tailwind

### Vision

```seq
// Define a component
: user-card ( User -- Html )
  [ user |
    <div class="card">
      <h2>{{ user "name" @ }}</h2>
      <p>{{ user "email" @ }}</p>
      <button hx-get="/users/{{ user "id" @ }}/details">
        View Details
      </button>
    </div>
  ] html.component ;

// Page composition
: users-page ( List -- Html )
  [ users |
    <html>
      <head>
        <title>Users</title>
        {{ "styles.css" css.link }}
      </head>
      <body>
        <h1>All Users</h1>
        {{ users [ user-card ] list.map html.join }}
      </body>
    </html>
  ] html.page ;

// Serve it
8080 http.listen [ request |
  get-all-users users-page http.html-respond
]
```

### Alternative: Immediate-Mode UI (for CLI/TUI)

```seq
// Terminal UI framework
ui.screen [
  "Welcome to Seq" ui.title

  ui.row [
    "Name: " ui.label
    name ui.input name!
  ]

  ui.row [
    "Submit" [ handle-submit ] ui.button
    "Cancel" [ ui.exit ] ui.button
  ]
] ui.render
```

---

## Implementation Roadmap

### Already Complete
- [x] JSON parser/encoder (1234 lines)
- [x] YAML parser (750 lines)
- [x] Result/Option types (268 lines)
- [x] HTTP server response/request helpers
- [x] Concurrent HTTP server example
- [x] Math utilities (imath, fmath)
- [x] Testing framework
- [x] LSP server (2200+ lines) - diagnostics, completions, include resolution
- [x] REPL with LSP integration and vim keybindings
- [x] Base64/Hex encoding (builtin) - standard, URL-safe, hex

### Phase 1: HTTP Client & Tooling
- [ ] HTTP client FFI (ureq or reqwest)
- [ ] `seq fmt` basic formatter
- [ ] `seq.toml` project file
- [ ] HTTP server improvements (path params, query strings)

### Phase 2: Text Processing
- [ ] Regex (FFI to regex crate)
- [ ] String templates
- [ ] URL parsing (pure Seq or FFI)
- [ ] `seq get` package fetching

### Phase 3: Security & Docs
- [ ] Crypto basics (hashing, HMAC)
- [ ] TLS/HTTPS support
- [ ] `seq doc` documentation generator

### Phase 4: Advanced
- [ ] Database connectivity
- [ ] HTML templating
- [ ] Compression
- [ ] UI framework exploration

---

## Design Principles

### 1. Composition Over Configuration

```seq
// Good: Composable pieces
request
  auth-middleware
  logging-middleware
  rate-limit-middleware
  handler
http.handle

// Avoid: Giant config objects
```

### 2. Stack-Friendly APIs

```seq
// Good: Works naturally on stack
users [ "active" @ ] list.filter

// Avoid: Deeply nested structures that fight the stack
```

### 3. Explicit Over Magic

```seq
// Good: Clear what happens
response http.body json.decode "users" json.get

// Avoid: Hidden transformations
```

### 4. Errors as Values

```seq
// Use Result/Option patterns
file.read-text  // ( Path -- String Bool )
[ process-content ] [ "File not found" error ] if-else
```

### 5. Consistent Naming

| Pattern | Example | Meaning |
|---------|---------|---------|
| `noun.verb` | `http.get`, `json.encode` | Action on type |
| `noun?` | `list.empty?`, `map.has?` | Predicate |
| `noun!` | `http.url!`, `buffer.flush!` | Mutation/side effect |
| `->` | `string->int`, `json->map` | Conversion |

---

## Comparison: Seq vs Go

| Aspect | Go | Seq (Planned) |
|--------|-----|---------------|
| Paradigm | Imperative, structural | Stack-based, functional |
| Concurrency | Goroutines + channels | Strands + channels |
| Error handling | `error` return value | Result types on stack |
| Generics | Type parameters | Row polymorphism |
| Build | `go build` | `seq build` |
| Format | `gofmt` | `seq fmt` |
| Packages | Module path | Module path |
| Std library | ~150 packages | ~15 modules (focused) |

---

## Next Steps

1. **Create `stdlib/` directory structure**
2. **Design HTTP client API in detail**
3. **Implement JSON as first stdlib module**
4. **Draft `seq.toml` specification**
5. **Prototype `seq fmt` basic rules**

---

## References

- [Go Standard Library](https://pkg.go.dev/std)
- [HTMX](https://htmx.org/) - HTML-centric approach to interactivity
- [Hyperscript](https://hyperscript.org/) - Stack-like scripting for HTML
- [Factor](https://factorcode.org/) - Concatenative language with rich stdlib
