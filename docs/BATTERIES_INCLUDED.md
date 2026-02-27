# Batteries Included: Seq Standard Library

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

| Capability | Rust Crate | Quality |
|------------|------------|---------|
| **Crypto hashing** | `sha2` | RustCrypto, audited |
| **HMAC** | `hmac` | RustCrypto, audited |
| **Encryption** | `aes-gcm` | RustCrypto, audited |
| **Signatures** | `ed25519-dalek` | Audited, widely used |
| **HTTP client** | `ureq` | Pure Rust, minimal |
| **TLS** | `rustls` | Memory-safe, modern |
| **Regex** | `regex` | Fastest in class |
| **Compression** | `flate2`, `zstd` | Fast, well-maintained |
| **Random** | `rand` | Industry standard |
| **UUID** | `uuid` | Complete implementation |
| **Database** | `rusqlite` | Mature, stable |

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

## Feature Flags & Binary Size

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

## Priority 1: HTTP Client

The HTTP client is implemented using the `ureq` Rust crate.

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

---

## Priority 2: Regular Expressions

Regex support is implemented via the Rust `regex` crate (v1.11). Fast, safe, and no catastrophic backtracking.

```seq
# Check if pattern matches anywhere in string
"hello world" "wo.ld" regex.match?      # ( String String -- Bool )

# Find first match
"a1 b2 c3" "[a-z][0-9]" regex.find      # ( String String -- String Bool )

# Find all matches
"a1 b2 c3" "[a-z][0-9]" regex.find-all  # ( String String -- List Bool )

# Replace first occurrence
"hello world" "world" "Seq" regex.replace  # ( String String String -- String Bool )

# Replace all occurrences
"a1 b2 c3" "[0-9]" "X" regex.replace-all   # ( String String String -- String Bool )

# Extract capture groups
"2024-01-15" "(\\d+)-(\\d+)-(\\d+)" regex.captures
# ( String String -- List Bool ) returns ["2024", "01", "15"] true

# Split by pattern
"a1b2c3" "[0-9]" regex.split            # ( String String -- List Bool )

# Check if pattern is valid
"[a-z]+" regex.valid?                   # ( String -- Bool )
```

**Examples:**
- `examples/text/regex-demo.seq` - Demonstrates all regex operations
- `examples/text/log-parser.seq` - Practical log parsing with regex

---

## Priority 3: Cryptography

Crypto is essential for real-world applications but often requires hunting through external packages. A batteries-included approach means shipping these out of the box.

### Tier 1: Essential

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.sha256` | `sha2` | Checksums, content addressing, password hashing input |
| `crypto.hmac-sha256` | `hmac` + `sha2` | Webhook verification, JWT signing, API auth |
| `crypto.random-bytes` | `rand` | Tokens, nonces, salts, session IDs |
| `crypto.uuid4` | `uuid` | Unique identifiers |
| `crypto.constant-time-eq` | custom | Timing-safe comparison for signatures |

```seq
# Hashing
"hello world" crypto.sha256          # ( String -- String ) hex-encoded

# HMAC for API authentication
"webhook-payload" "secret-key" crypto.hmac-sha256
# ( message key -- signature )

# Verify webhook signature
received-sig computed-sig crypto.constant-time-eq
# ( String String -- Bool ) timing-safe comparison

# Generate secure random token
32 crypto.random-bytes    # ( n -- String ) 32 random bytes as 64-char hex string

# Generate UUID v4
crypto.uuid4    # ( -- String ) "550e8400-e29b-41d4-a716-446655440000"
```

### Tier 2: Encryption

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.aes-gcm-encrypt` | `aes-gcm` | Encrypting data at rest, secure storage |
| `crypto.aes-gcm-decrypt` | `aes-gcm` | Decrypting data |
| `crypto.pbkdf2-sha256` | `pbkdf2` | Password-based key derivation |

```seq
# Symmetric encryption (AES-256-GCM)
# Key must be 64 hex chars (32 bytes = 256 bits)
plaintext hex-key crypto.aes-gcm-encrypt   # ( String String -- String Bool )
ciphertext hex-key crypto.aes-gcm-decrypt  # ( String String -- String Bool )

# Key derivation from password
"user-password" "salt" 100000 crypto.pbkdf2-sha256
# ( password salt iterations -- hex-key Bool )

# Full example: derive key and encrypt
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

### Tier 3: Signatures

| API | Rust Crate | Use Cases |
|-----|------------|-----------|
| `crypto.ed25519-keypair` | `ed25519-dalek` | Generate signing keys |
| `crypto.ed25519-sign` | `ed25519-dalek` | Digital signatures |
| `crypto.ed25519-verify` | `ed25519-dalek` | Signature verification |

```seq
# Generate keypair
crypto.ed25519-keypair    # ( -- public-key private-key ) both as 64-char hex

# Sign a message
message private-key crypto.ed25519-sign    # ( String String -- String Bool )

# Verify signature
message signature public-key crypto.ed25519-verify  # ( String String String -- Bool )

# Full example
crypto.ed25519-keypair
"Important document" swap crypto.ed25519-sign
if
  swap "Important document" rot rot crypto.ed25519-verify
  if "Signature valid!" else "Signature invalid!" then io.write-line
else
  drop drop "Signing failed" io.write-line
then
```

### Encoding Helpers

Available as `encoding.*` builtins:

```seq
# Base64 (standard with padding)
"hello" encoding.base64-encode    # ( String -- String ) "aGVsbG8="
"aGVsbG8=" encoding.base64-decode # ( String -- String Bool )

# URL-safe Base64 (no padding, for JWTs/URLs)
data encoding.base64url-encode    # ( String -- String )
encoded encoding.base64url-decode # ( String -- String Bool )

# Hex (lowercase output, case-insensitive decode)
"hello" encoding.hex-encode       # ( String -- String ) "68656c6c6f"
"68656c6c6f" encoding.hex-decode  # ( String -- String Bool )
```

---

## Priority 4: Compression

Data compression via gzip and Zstandard (zstd). All operations use base64 encoding for string-safe output.

### API

```seq
# Gzip compression (default level 6)
"hello world" compress.gzip              # ( String -- String Bool )

# Gzip with custom level (1-9, where 1=fastest, 9=best)
"hello world" 9 compress.gzip-level      # ( String Int -- String Bool )

# Gzip decompression
compressed compress.gunzip               # ( String -- String Bool )

# Zstandard compression (default level 3)
"hello world" compress.zstd              # ( String -- String Bool )

# Zstandard with custom level (1-22, where 1=fastest, 22=best)
"hello world" 19 compress.zstd-level     # ( String Int -- String Bool )

# Zstandard decompression
compressed compress.unzstd               # ( String -- String Bool )
```

### Return Values

All compression operations return `( String Bool )`:
- On success: `compressed-data true` (data is base64-encoded)
- On failure: `error-message false`

Decompression accepts base64-encoded input and returns the original string.

### Example Usage

```seq
# Compress and decompress with gzip
"Hello, World!" compress.gzip
if
  dup "Compressed: " swap string.concat io.write-line
  compress.gunzip
  if
    "Decompressed: " swap string.concat io.write-line
  else
    drop "Decompression failed" io.write-line
  then
else
  drop "Compression failed" io.write-line
then

# Compare compression algorithms
"Large text data..." dup
compress.gzip if string.length else drop 0 then
swap compress.zstd if string.length else drop 0 then
# Compare sizes
```

### When to Use Each

| Algorithm | Best For | Level Range |
|-----------|----------|-------------|
| **gzip** | Web content, HTTP compression, broad compatibility | 1-9 |
| **zstd** | Large data, better ratio, modern systems | 1-22 |

- **gzip**: Universal compatibility, good for HTTP `Content-Encoding`
- **zstd**: Better compression ratio and speed, ideal for data storage

### Implementation Details

- **Crates**: `flate2` (gzip), `zstd` (Zstandard)
- **Output encoding**: Base64 for string-safe transport
- **Input/Output**: String → compressed base64 String

**Examples:**
- `examples/io/compress-demo.seq` - Demonstrates all compression operations

---

## Design Principles

### 1. Composition Over Configuration

```seq
# Good: Composable pieces
request
  auth-middleware
  logging-middleware
  rate-limit-middleware
  handler
http.handle

# Avoid: Giant config objects
```

### 2. Stack-Friendly APIs

```seq
# Good: Works naturally on stack
users [ "active" @ ] list.filter

# Avoid: Deeply nested structures that fight the stack
```

### 3. Explicit Over Magic

```seq
# Good: Clear what happens
response http.body json.decode "users" json.get

# Avoid: Hidden transformations
```

### 4. Errors as Values

```seq
# Use Result/Option patterns
file.read-text  # ( Path -- String Bool )
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

| Aspect | Go | Seq |
|--------|-----|-----|
| Paradigm | Imperative, structural | Stack-based, functional |
| Concurrency | Goroutines + channels | Strands + channels |
| Error handling | `error` return value | Result types on stack |
| Generics | Type parameters | Row polymorphism |
| Build | `go build` | `seqc build` |
| Packages | Module path | `include std:module` |
| Std library | ~150 packages | ~15 modules (focused) |

---

## References

- [Go Standard Library](https://pkg.go.dev/std)
- [HTMX](https://htmx.org/) - HTML-centric approach to interactivity
- [Hyperscript](https://hyperscript.org/) - Stack-like scripting for HTML
- [Factor](https://factorcode.org/) - Concatenative language with rich stdlib
