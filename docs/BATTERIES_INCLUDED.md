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
| Crypto (SHA-256, HMAC, AES-GCM, PBKDF2, Ed25519, Random, UUID) | `sha2`, `hmac`, `aes-gcm`, `pbkdf2`, `ed25519-dalek`, `rand`, `uuid` |
| Regular expressions | `regex` |
| Compression | `flate2`, `zstd` |
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

## Feature Flags & Binary Size (Implemented)

The runtime uses Cargo feature flags to allow opt-in compilation of optional modules. This keeps the core runtime smaller while allowing full batteries for apps that need them.

### Feature Structure

```toml
# crates/runtime/Cargo.toml
[features]
default = ["full", "diagnostics"]

# Full batteries - enable all optional modules
full = ["crypto", "http", "regex", "compression"]

# Optional modules - enable individually for smaller binaries
crypto = ["dep:sha2", "dep:hmac", "dep:rand", "dep:uuid", "dep:subtle",
          "dep:aes-gcm", "dep:pbkdf2", "dep:ed25519-dalek"]
http = ["dep:ureq", "dep:url"]
regex = ["dep:regex"]
compression = ["dep:flate2", "dep:zstd"]
```

### Static Library Sizes (Measured)

| Configuration | Library Size | Use Case |
|---------------|--------------|----------|
| `core` only | ~20 MiB | Embedded, CLI tools, scripts |
| `core` + `crypto` | ~22 MiB | Security tools, auth services |
| `core` + `compression` | ~22 MiB | Data processing, archiving |
| `core` + `regex` | ~27 MiB | Text processing, parsing |
| `core` + `http` | ~33 MiB | API clients, web scrapers |
| `full` (default) | ~43 MiB | Full web applications |

*Note: These are static library sizes. Final executable sizes depend on linking and may be smaller.*

### Compile-Time Gating

Each optional module is gated with `#[cfg(feature = "...")]`:

```rust
// Real implementation when feature enabled
#[cfg(feature = "crypto")]
pub mod crypto;

// Stub with helpful panic when feature disabled
#[cfg(not(feature = "crypto"))]
pub mod crypto_stub;
```

When a disabled feature is used at runtime:
```
thread 'main' panicked at 'crypto.sha256 requires crypto feature not enabled.
Rebuild with: cargo build --features crypto'
```

### Building with Features

```bash
# Full batteries (default)
cargo build --release

# Minimal build (core only - no crypto, http, regex, compression)
cargo build --release --no-default-features

# Specific capabilities
cargo build --release --no-default-features --features "crypto,http"

# Just crypto for a security tool
cargo build --release --no-default-features --features crypto
```

### Available Features

| Feature | Includes | Builtins |
|---------|----------|----------|
| `crypto` | SHA-256, HMAC, AES-GCM, PBKDF2, Ed25519, random, UUID | `crypto.*` |
| `http` | HTTP client with TLS | `http.get/post/put/delete` |
| `regex` | Regular expressions | `regex.*` |
| `compression` | gzip, zstd | `compress.*` |
| `diagnostics` | SIGQUIT strand dump | (debugging) |
| `full` | All of the above | Everything |

### Future: Compiler Integration

Eventually the compiler could:
- Detect which features a program needs at compile time
- Error with helpful message: "Enable --features crypto for crypto.sha256"
- Auto-generate feature requirements in `seq.toml`

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
| **Crypto** | SHA-256, HMAC-SHA256, AES-256-GCM, PBKDF2, Ed25519 signatures, secure random, UUID v4 |
| **HTTP Client** | GET, POST, PUT, DELETE with TLS support |
| **Regex** | match, find, find-all, replace, captures, split, valid? |
| **Compression** | gzip, gunzip, zstd, unzstd with compression levels |
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
| **HTTP client** | **Complete** | High |
| **Regex** | **Complete** | Medium |
| **TLS/HTTPS** | **Complete** (via ureq) | Medium |
| **Templates** | Not started | Medium |
| **`seq fmt`** | Not started | Medium |
| **`seq.toml`** | Not started | Medium |
| **Logging** | Not started | Low |
| **Compression** | **Complete** | Low |
| **Database** | Not started | Future |
| **HTML parsing** | Not started | Future |

### Already Mature

| Category | Status |
|----------|--------|
| **JSON** | Complete (1234 lines) |
| **YAML** | Complete (750 lines) |
| **HTTP server** | Working (helpers + example) |
| **HTTP client** | Complete (builtin) - GET, POST, PUT, DELETE with TLS |
| **Regex** | Complete (builtin) - match, find, replace, captures, split |
| **Compression** | Complete (builtin) - gzip, zstd with levels |
| **LSP** | Complete (2200+ lines) - diagnostics, completions |
| **REPL** | Complete - with LSP integration, vim keybindings |
| **Testing** | Complete (builtin) |
| **Result/Option** | Complete (268 lines) |
| **Base64/Hex** | Complete (builtin) - standard, URL-safe, hex |
| **Crypto Phase 1** | Complete (builtin) - SHA-256, HMAC, random, UUID |
| **Crypto Phase 2** | Complete (builtin) - AES-256-GCM encryption, PBKDF2 key derivation |
| **Crypto Phase 3** | Complete (builtin) - Ed25519 digital signatures |

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

## Priority 1: HTTP Client (COMPLETE)

The HTTP client is now fully implemented using the `ureq` Rust crate.

### API

```seq
# GET request - returns response map
"https://api.example.com/users" http.get
# Stack: ( Map ) where Map = { "status": 200, "body": "...", "ok": true }

# POST request with body and content-type
"https://api.example.com/users" "{\"name\":\"Alice\"}" "application/json" http.post
# Stack: ( Map )

# PUT request (same signature as POST)
"https://api.example.com/users/1" "{\"name\":\"Bob\"}" "application/json" http.put

# DELETE request
"https://api.example.com/users/1" http.delete
# Stack: ( Map )
```

### Response Map

All HTTP operations return a Map with these keys:
- `"status"` (Int): HTTP status code (200, 404, 500, etc.) or 0 on connection error
- `"body"` (String): Response body as text
- `"ok"` (Bool): true if status is 2xx, false otherwise
- `"error"` (String): Error message (only present on failure)

### Example Usage

```seq
# Make a GET request and handle the response
"https://httpbin.org/get" http.get
dup "ok" map.get drop
if
  "body" map.get drop io.write-line
else
  "error" map.get drop "Error: " swap string.concat io.write-line
then
```

### Implementation Details

- **Crate**: `ureq` (pure Rust, blocking, minimal dependencies)
- **TLS**: Built-in via `rustls` (no OpenSSL dependency)
- **Timeout**: 30 seconds default
- **Max body size**: 10 MB
- **Connection pooling**: Enabled via shared agent instance

### Security: SSRF Protection

The HTTP client includes **built-in SSRF protection**. The following are automatically blocked:

- **Localhost**: `localhost`, `*.localhost`, `127.x.x.x`
- **Private networks**: `10.x.x.x`, `172.16-31.x.x`, `192.168.x.x`
- **Link-local/Cloud metadata**: `169.254.x.x` (blocks AWS/GCP/Azure metadata)
- **IPv6 private**: loopback, link-local, unique local addresses
- **Non-HTTP schemes**: `file://`, `ftp://`, `gopher://`, etc.

Blocked requests return an error response with `ok=false`.

**Additional recommendations**:
- Use domain allowlists for sensitive applications
- Apply network-level egress filtering

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

### Regular Expressions (Complete)

Regex support is now complete via the Rust `regex` crate (v1.11). Fast, safe, and no catastrophic backtracking.

```seq
# Check if pattern matches anywhere in string
"hello world" "wo.ld" regex.match?      # ( String String -- Bool )

# Find first match
"a1 b2 c3" "[a-z][0-9]" regex.find      # ( String String -- String Bool )

# Find all matches
"a1 b2 c3" "[a-z][0-9]" regex.find-all  # ( String String -- List )

# Replace first occurrence
"hello world" "world" "Seq" regex.replace  # ( String String String -- String )

# Replace all occurrences
"a1 b2 c3" "[0-9]" "X" regex.replace-all   # ( String String String -- String )

# Extract capture groups
"2024-01-15" "(\\d+)-(\\d+)-(\\d+)" regex.captures
# ( String String -- List Bool ) returns ["2024", "01", "15"] true

# Split by pattern
"a1b2c3" "[0-9]" regex.split            # ( String String -- List )

# Check if pattern is valid
"[a-z]+" regex.valid?                   # ( String -- Bool )
```

**Examples:**
- `examples/text/regex-demo.seq` - Demonstrates all regex operations
- `examples/text/log-parser.seq` - Practical log parsing with regex

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

### Tier 1: Essential (Implemented)

| API | Rust Crate | Use Cases | Status |
|-----|------------|-----------|--------|
| `crypto.sha256` | `sha2` | Checksums, content addressing, password hashing input | **Done** |
| `crypto.hmac-sha256` | `hmac` + `sha2` | Webhook verification, JWT signing, API auth | **Done** |
| `crypto.random-bytes` | `rand` | Tokens, nonces, salts, session IDs | **Done** |
| `crypto.uuid4` | `uuid` | Unique identifiers | **Done** |
| `crypto.constant-time-eq` | custom | Timing-safe comparison for signatures | **Done** |

```seq
// Hashing
"hello world" crypto.sha256          // ( String -- String ) hex-encoded

// HMAC for API authentication
"webhook-payload" "secret-key" crypto.hmac-sha256
// ( message key -- signature )

// Verify webhook signature
received-sig computed-sig crypto.constant-time-eq
// ( String String -- Bool ) timing-safe comparison

// Generate secure random token
32 crypto.random-bytes    // ( n -- String ) 32 random bytes as 64-char hex string

// Generate UUID v4
crypto.uuid4    // ( -- String ) "550e8400-e29b-41d4-a716-446655440000"
```

### Tier 2: Encryption (Implemented)

| API | Rust Crate | Use Cases | Status |
|-----|------------|-----------|--------|
| `crypto.aes-gcm-encrypt` | `aes-gcm` | Encrypting data at rest, secure storage | **Done** |
| `crypto.aes-gcm-decrypt` | `aes-gcm` | Decrypting data | **Done** |
| `crypto.pbkdf2-sha256` | `pbkdf2` | Password-based key derivation | **Done** |

```seq
// Symmetric encryption (AES-256-GCM)
// Key must be 64 hex chars (32 bytes = 256 bits)
plaintext hex-key crypto.aes-gcm-encrypt   // ( String String -- String Bool )
ciphertext hex-key crypto.aes-gcm-decrypt  // ( String String -- String Bool )

// Key derivation from password
"user-password" "salt" 100000 crypto.pbkdf2-sha256
// ( password salt iterations -- hex-key Bool )

// Full example: derive key and encrypt
"user-password" "unique-salt" 100000 crypto.pbkdf2-sha256
if
  "secret data" swap crypto.aes-gcm-encrypt
  if
    "Encrypted: " swap string.concat io.write-line
  else
    drop "Encryption failed" io.write-line
  then
else
  drop "Key derivation failed" io.write-line
then
```

### Tier 3: Signatures & Key Exchange (Implemented)

| API | Rust Crate | Use Cases | Status |
|-----|------------|-----------|--------|
| `crypto.ed25519-keypair` | `ed25519-dalek` | Generate signing keys | **Done** |
| `crypto.ed25519-sign` | `ed25519-dalek` | Digital signatures | **Done** |
| `crypto.ed25519-verify` | `ed25519-dalek` | Signature verification | **Done** |

```seq
// Generate keypair
crypto.ed25519-keypair    // ( -- public-key private-key ) both as 64-char hex

// Sign a message
message private-key crypto.ed25519-sign    // ( String String -- String Bool )

// Verify signature
message signature public-key crypto.ed25519-verify  // ( String String String -- Bool )

// Full example
crypto.ed25519-keypair
"Important document" swap crypto.ed25519-sign
if
  swap "Important document" rot rot crypto.ed25519-verify
  if "Signature valid!" else "Signature invalid!" then io.write-line
else
  drop drop "Signing failed" io.write-line
then
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
| **Phase 1** | sha256, hmac-sha256, random-bytes, uuid4, constant-time-eq | **Complete** |
| **Phase 2** | base64, hex | **Complete** |
| **Phase 3** | aes-gcm-encrypt, aes-gcm-decrypt, pbkdf2-sha256 | **Complete** |
| **Phase 4** | ed25519-keypair, ed25519-sign, ed25519-verify | **Complete** |

All crypto phases complete: JWT verification, webhook handling, secure tokens, API authentication, encrypted storage, password-based key derivation, digital signatures all available.

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
- [x] Crypto Phase 1 (builtin) - SHA-256, HMAC-SHA256, random bytes, UUID v4
- [x] Crypto Phase 2 (builtin) - AES-256-GCM encryption/decryption, PBKDF2 key derivation
- [x] Crypto Phase 3 (builtin) - Ed25519 digital signatures
- [x] HTTP client (builtin) - GET, POST, PUT, DELETE with TLS via ureq
- [x] Regex (builtin) - match, find, replace, captures, split via regex crate
- [x] Compression (builtin) - gzip, zstd with levels via flate2/zstd crates

### Phase 1: Tooling
- [ ] `seq fmt` basic formatter
- [ ] `seq.toml` project file
- [ ] HTTP server improvements (path params, query strings)

### Phase 2: Text Processing
- [x] Regex (FFI to regex crate)
- [ ] String templates
- [ ] URL parsing (pure Seq or FFI)
- [ ] `seq get` package fetching

### Phase 3: Docs & Crypto Phase 2
- [ ] `seq doc` documentation generator
- [x] AES-GCM encryption
- [x] PBKDF2 key derivation
- [x] Ed25519 signatures

### Phase 4: Advanced
- [ ] Database connectivity
- [ ] HTML templating
- [x] Compression
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
