# Seq Standard Library Reference

This document covers:
- **Built-in Operations** - primitives implemented in the runtime
- **Standard Library Modules** - Seq code included via `include std:<module>`

## Table of Contents

### Built-in Operations

- [I/O Operations](#io-operations)
- [Command-line Arguments](#command-line-arguments)
- [File Operations](#file-operations)
- [Type Conversions](#type-conversions)
- [Integer Arithmetic](#integer-arithmetic)
- [Integer Comparison](#integer-comparison)
- [Boolean Operations](#boolean-operations)
- [Bitwise Operations](#bitwise-operations)
- [Stack Operations](#stack-operations)
- [Control Flow](#control-flow)
- [Concurrency](#concurrency)
- [Channel Operations](#channel-operations)
- [Networking — net.tcp.*](#networking--nettcp)
- [Networking — net.udp.*](#networking--netudp)
- [Networking — net.dns.*](#networking--netdns)
- [Networking — net.tls.*](#networking--nettls)
- [Networking — Socket type](#networking--socket-type)
- [OS Operations](#os-operations)
- [Terminal Operations](#terminal-operations)
- [String Operations](#string-operations)
- [Encoding Operations](#encoding-operations)
- [Crypto Operations](#crypto-operations)
- [Networking — net.http.* (HTTP client)](#networking--nethttp-http-client)
- [Regular Expressions](#regular-expressions)
- [Compression](#compression)
- [Variant Operations](#variant-operations)
- [List Operations](#list-operations)
- [Map Operations](#map-operations)
- [Float Arithmetic](#float-arithmetic)
- [Float Comparison](#float-comparison)
- [Float Math](#float-math)
- [Test Framework](#test-framework)
- [Time Operations](#time-operations)
- [Serialization](#serialization)
- [Stack Introspection](#stack-introspection)

### Standard Library Modules

- [std:json](#stdjson---json-parsing)
- [std:yaml](#stdyaml---yaml-parsing)
- [std:http](#stdhttp---http-response-helpers)
- [std:list](#stdlist---list-utilities)
- [std:map](#stdmap---map-utilities)
- [std:imath](#stdimath---integer-math)
- [std:fmath](#stdfmath---float-math)
- [std:zipper](#stdzipper---functional-list-zipper)
- [std:signal](#stdsignal---signal-handling)
- [std:son](#stdson---seq-object-notation-helpers)
- [std:stack-utils](#stdstack-utils---stack-utilities)

---

## I/O Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `io.write` | `( String -- )` | Write string to stdout without newline |
| `io.write-line` | `( String -- )` | Write string to stdout with newline |
| `io.read-line` | `( -- String Bool )` | Read line from stdin. Returns (line, success) |
| `io.read-n` | `( Int -- String Int )` | Read N bytes from stdin. Returns (bytes, status) |

## Command-line Arguments

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `args.count` | `( -- Int )` | Get number of command-line arguments |
| `args.at` | `( Int -- String )` | Get argument at index N |

## File Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `file.slurp` | `( String -- String Bool )` | Read entire file. Returns content and success flag |
| `file.spit` | `( String String -- Bool )` | Write content to file. Takes content and path, returns success |
| `file.append` | `( String String -- Bool )` | Append content to file. Takes content and path, returns success |
| `file.exists?` | `( String -- Bool )` | Check if file exists at path |
| `file.delete` | `( String -- Bool )` | Delete a file at path. Returns success |
| `file.size` | `( String -- Int Bool )` | Get file size in bytes. Returns size and success |
| `file.for-each-line` | `( String [String --] -- Bool )` | Execute quotation for each line in file. Returns false if the file could not be opened or a read error occurred. |

## Directory Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `dir.exists?` | `( String -- Bool )` | Check if directory exists at path |
| `dir.make` | `( String -- Bool )` | Create a directory at path. Returns success |
| `dir.delete` | `( String -- Bool )` | Delete an empty directory. Returns success |
| `dir.list` | `( String -- List Bool )` | List directory contents. Returns filenames and success |

## Type Conversions

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `int->string` | `( Int -- String )` | Convert integer to string |
| `int->float` | `( Int -- Float )` | Convert integer to float |
| `float->int` | `( Float -- Int )` | Truncate float to integer |
| `float->string` | `( Float -- String )` | Convert float to string |
| `string->int` | `( String -- Int Bool )` | Parse string as integer. Returns (value, success) |
| `string->float` | `( String -- Float Bool )` | Parse string as float. Returns (value, success) |
| `char->string` | `( Int -- String )` | Convert Unicode codepoint to single-char string |
| `symbol->string` | `( Symbol -- String )` | Convert symbol to string |
| `string->symbol` | `( String -- Symbol )` | Intern string as symbol |
| `int.to-bytes-i32-be` | `( Int -- String )` | Encode Int as 4-byte big-endian i32 (low 32 bits). For binary protocol encoders |
| `float.to-bytes-f32-be` | `( Float -- String )` | Encode Float as 4-byte big-endian IEEE-754 f32. For binary protocol encoders |

## Integer Arithmetic

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `i.add` / `i.+` | `( Int Int -- Int )` | Add two integers (wrapping on overflow) |
| `i.subtract` / `i.-` | `( Int Int -- Int )` | Subtract second from first (wrapping on overflow) |
| `i.multiply` / `i.*` | `( Int Int -- Int )` | Multiply two integers (wrapping on overflow) |
| `i.divide` / `i./` | `( Int Int -- Int Bool )` | Integer division with success flag |
| `i.modulo` / `i.%` | `( Int Int -- Int Bool )` | Integer modulo with success flag |
| `i.pow` | `( Int Int -- Int Bool )` | Integer power `base^exp` with success flag (false on negative exp, exp > u32::MAX, or overflow; `0^0 = 1`) |

### Division and Modulo Behavior

Division and modulo operations return a result and a success flag:
- **Success** (`true`): Operation completed normally, result is valid
- **Failure** (`false`): Division by zero, result is 0

**Overflow handling**: `INT_MIN / -1` uses wrapping semantics and returns `INT_MIN` with success=`true`. This matches Forth/Factor behavior and avoids undefined behavior.

```seq
10 3 i./     # ( -- 3 true )   Normal division
10 0 i./     # ( -- 0 false )  Division by zero
-9223372036854775808 -1 i./  # ( -- -9223372036854775808 true )  Wrapping
```

## Integer Comparison

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `i.=` / `i.eq` | `( Int Int -- Bool )` | Test equality |
| `i.<` / `i.lt` | `( Int Int -- Bool )` | Test less than |
| `i.>` / `i.gt` | `( Int Int -- Bool )` | Test greater than |
| `i.<=` / `i.lte` | `( Int Int -- Bool )` | Test less than or equal |
| `i.>=` / `i.gte` | `( Int Int -- Bool )` | Test greater than or equal |
| `i.<>` / `i.neq` | `( Int Int -- Bool )` | Test not equal |

## Boolean Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `and` | `( Bool Bool -- Bool )` | Logical AND |
| `or` | `( Bool Bool -- Bool )` | Logical OR |
| `not` | `( Bool -- Bool )` | Logical NOT |

## Bitwise Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `band` | `( Int Int -- Int )` | Bitwise AND |
| `bor` | `( Int Int -- Int )` | Bitwise OR |
| `bxor` | `( Int Int -- Int )` | Bitwise XOR |
| `bnot` | `( Int -- Int )` | Bitwise NOT (complement) |
| `shl` | `( Int Int -- Int )` | Shift left by N bits |
| `shr` | `( Int Int -- Int )` | Shift right by N bits (logical) |
| `popcount` | `( Int -- Int )` | Count number of set bits |
| `clz` | `( Int -- Int )` | Count leading zeros |
| `ctz` | `( Int -- Int )` | Count trailing zeros |
| `int-bits` | `( -- Int )` | Push bit width of integers (63 — see language guide for the 63-bit Int model) |

## Stack Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `dup` | `( T -- T T )` | Duplicate top value |
| `drop` | `( T -- )` | Remove top value |
| `swap` | `( T U -- U T )` | Swap top two values |
| `over` | `( T U -- T U T )` | Copy second value to top |
| `rot` | `( T U V -- U V T )` | Rotate third to top |
| `nip` | `( T U -- U )` | Remove second value |
| `tuck` | `( T U -- U T U )` | Copy top below second |
| `2dup` | `( T U -- T U T U )` | Duplicate top two values |
| `3drop` | `( T U V -- )` | Remove top three values |
| `pick` | `( T Int -- T T )` | Copy value at depth N to top |
| `roll` | `( T Int -- T )` | Rotate N+1 items, bringing depth N to top |

## Control Flow

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `call` | `( Quotation -- ... )` | Call a quotation or closure |
| `cond` | `( T [T -- T Bool] [T -- T] ... N -- T )` | Multi-way conditional: N predicate/body pairs. Each predicate receives the value and returns `Bool`; first match wins. Panics if no predicate matches. |

## Concurrency

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `strand.spawn` | `( Quotation -- Int )` | Spawn concurrent strand. Returns strand ID |
| `strand.weave` | `( Quotation -- handle )` | Create generator/coroutine. Returns handle |
| `strand.resume` | `( handle T -- handle T Bool )` | Resume weave with value. Returns (handle, value, has_more) |
| `yield` | `( ctx T -- ctx T )` | Yield value from weave and receive resume value |
| `strand.weave-cancel` | `( handle -- )` | Cancel weave and release resources |

## Channel Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `chan.make` | `( -- Channel )` | Create new channel |
| `chan.send` | `( T Channel -- Bool )` | Send value on channel. Returns success |
| `chan.receive` | `( Channel -- T Bool )` | Receive from channel. Returns (value, success) |
| `chan.close` | `( Channel -- )` | Close channel |
| `chan.yield` | `( -- )` | Yield control to scheduler |

## Networking — net.tcp.*

All TCP operations return a Bool success flag for error handling. The fd
slot is typed as `Socket` (a phantom over Int) — `net.tcp.write` will not
accept an arbitrary integer.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `net.tcp.listen` | `( Int -- Socket Bool )` | Listen on port. Returns (socket, success) |
| `net.tcp.connect` | `( String Int -- Socket Bool )` | Connect to host:port. Returns (socket, success) |
| `net.tcp.accept` | `( Socket -- Socket Bool )` | Accept connection. Returns (client, success) |
| `net.tcp.read` | `( Socket -- String Bool )` | Read from socket. Returns (data, success) |
| `net.tcp.write` | `( String Socket -- Bool )` | Write to socket. Returns success |
| `net.tcp.close` | `( Socket -- Bool )` | Close socket. Returns success |

`net.tcp.connect` resolves the hostname through [net.dns.resolve](#networking--netdns) —
the lookup runs on the DNS worker pool, not the may carrier, and tries
each returned address in order until one connects (or all fail). IP
literals work as hostnames (they round-trip through `getaddrinfo`
without touching DNS). Bool is false on resolution failure,
every-address-failed, invalid port (must be 1..65535), or
socket-registry exhaustion.

**Known limitations — connect.** Two gaps inherited from the v1 surface:

- **No connect timeout.** `connect` parks the strand for the full kernel
  SYN timeout (≈60–130 s on Linux) against a silent peer. There is no
  caller-side bound; if you need one, wrap the call in `strand.spawn` and
  cancel from another strand, or use a non-blocking pattern at a higher
  layer. A timeout argument is a planned follow-up.
- **Serial fallback, no happy-eyeballs.** Addresses are tried in resolver
  order. If AAAA (IPv6) points somewhere unreachable on a misconfigured
  network, you eat one full SYN timeout per dead address before falling
  back to A (IPv4). RFC 8305 happy-eyeballs is a planned follow-up.

```seq
"example.com" 443 net.tcp.connect
[ # ( socket )
  "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n" over net.tcp.write drop
  dup net.tcp.read
  [ io.write-line ]
  [ drop "read failed" io.write-line ]
  if
  net.tcp.close drop
]
[ drop "connect failed" io.write-line ]
if
```

## Networking — net.udp.*

Datagram-oriented; sockets are `Socket` handles. Every word ends with a
success Bool on top so callers can `[ ... ] [ ... ] if`.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `net.udp.bind` | `( Int -- Socket Int Bool )` | Bind to local port. Returns (socket, bound-port, success). port=0 lets the OS pick. |
| `net.udp.send-to` | `( String String Int Socket -- Bool )` | Send a datagram. (bytes, host, port, socket) |
| `net.udp.receive-from` | `( Socket -- String String Int Bool )` | Receive (yields). Returns (bytes, host, port, success) |
| `net.udp.close` | `( Socket -- Bool )` | Release the socket |

**`net.udp.send-to` host resolution.** Hostnames in the `host` argument
are resolved through [net.dns.resolve](#networking--netdns) — the
may-aware DNS worker pool — so the carrier thread does not park on a
blocking `getaddrinfo`. IP literals work without DNS round-trip. If
resolution returns multiple addresses (e.g. `localhost` →
`::1, 127.0.0.1`) the addresses are tried in order; the first
`send_to` that doesn't error wins, which transparently handles the
case of a v4-only socket reached through a name that resolves
v6-first.

Note: for UDP, `send_to` returning OK means the kernel accepted the
datagram for the chosen address family — it does *not* confirm peer
receipt. The multi-address walk therefore catches local-side errors
(address-family mismatch, route-not-found) but cannot detect an
unreachable peer past the first successful queue.

## Networking — net.dns.*

Hostname resolution offloaded to a dedicated OS-thread pool so may
carriers never park on `getaddrinfo`. A small TTL cache (60s, max 256
entries) collapses fanout to the same host. Inherits all
platform-correct resolution behaviour (`/etc/hosts`, systemd-resolved,
VPN/corp DNS, mDNS) from libc.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `net.dns.resolve` | `( String -- V Bool )` | Resolve hostname to a list of IP-address strings. On failure (unresolvable, empty result, empty input) returns (empty-list, false). |

The list contains IP-string representations (`"127.0.0.1"`,
`"::1"`, …). IP literals fast-path through `getaddrinfo` without
touching DNS.

Worker count is configurable via `SEQ_DNS_WORKERS` (default 8, max 64).
`SEQ_DNS_WORKERS=0` is treated as unset and falls back to the default —
disabling the pool makes no architectural sense since the syscall has
to run somewhere off the may carrier.

**Known limitation — no single-flight.** The cache deduplicates
*sequential* fanout: once one strand fills the cache, later resolves
of the same host hit the fast path. It does **not** deduplicate
*concurrent* first-resolves — N strands racing to resolve the same
uncached host each enqueue a separate worker job. Wasted work under
bursty load (e.g., connection-pool warm-up); not a correctness issue.
Single-flight via an in-flight map keyed by hostname is a planned
follow-up.

```seq
"api.example.com" net.dns.resolve
[ "first IP: " io.write list.first
  [ io.write-line ] [ drop "no addresses" io.write-line ] if ]
[ "resolution failed" io.write-line ]
if
```

## Networking — net.tls.*

TLS client upgrade for an already-connected `Socket`. The handshake
runs eagerly inside the builtin (via `rustls::ClientConnection::complete_io`
over the underlying may-aware TCP stream) so transport errors, bad
certificates, hostname mismatches, and any other TLS-layer failure
surface as `(0, false)` — matching every other fallible networking
word. After a successful upgrade, the existing `net.tcp.read` /
`net.tcp.write` / `net.tcp.close` operate on the returned Socket
transparently — the runtime dispatches reads and writes through
rustls. Trust roots come from `webpki-roots`.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `net.tls.client` | `( Socket String -- Socket Bool )` | Upgrade a connected Socket to TLS. `String` is the hostname (drives SNI and certificate validation). Returns the *same* Socket id with its registry slot replaced in place (caller-side maps keyed on the id remain valid). On failure (handshake error, bad cert, empty hostname, etc.) returns `(0, false)`; if the failure happened after the original stream was taken out of the registry, the slot's id is released and the underlying socket is closed. |

**Known limitations (v1):**

- **`net.tcp.close` on a TLS socket is a hard close.** The underlying TCP stream is dropped without first sending the TLS `close_notify` alert (RFC 5246 expects clients to send it). Modern servers tolerate truncation; some older stacks log it as a truncation-attack indicator. A graceful-shutdown variant is a planned follow-up.
- **IP-literal hostnames syntactically work but almost always fail validation.** `ServerName::try_from` parses `"1.2.3.4"` cleanly, but cert validation against an IP requires the peer cert to carry that IP in a SubjectAltName entry — vanishingly rare for public-internet certs. Use a DNS name unless you control the peer cert.
- **No client-certificate authentication (mTLS).** A `with_no_client_auth()` config is used unconditionally. mTLS is a planned follow-up.
- **No caller-side ALPN selection.** Whatever rustls defaults negotiate (typically `h2` / `http/1.1` if the peer offers them) is what you get; there's no way to ask for or reject a specific protocol.
- **No peer-certificate or cipher inspection from Seq.** Once the handshake succeeds, only the upgraded Socket is exposed — the negotiated suite, peer cert chain, SNI accepted, etc., are not surfaced.
- **No session resumption knobs.** rustls's default in-process session cache applies; there's no Seq-level control.

```seq
"example.com" 443 net.tcp.connect
[ # ( socket )
  "example.com" net.tls.client
  [ # ( socket )
    "GET / HTTP/1.0\r\nHost: example.com\r\n\r\n" over net.tcp.write drop
    dup net.tcp.read
    [ io.write-line ]
    [ drop "read failed" io.write-line ]
    if
    net.tcp.close drop
  ]
  [ drop "TLS handshake failed" io.write-line ]
  if
]
[ drop "connect failed" io.write-line ]
if
```

## Networking — Socket type

`Socket` is a compile-time-only nominal wrapper over the `Int` file
descriptor; the runtime representation stays `Value::Int(fd)`. This means
the type checker rejects `42 net.tcp.write`. Two escape hatches exist
for FFI / debugging:

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `fd->socket` | `( Int -- Socket )` | Cast a raw fd to a Socket. No runtime conversion. |
| `socket->fd` | `( Socket -- Int )` | Cast a Socket back to a raw fd. No runtime conversion. |

## OS Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `os.getenv` | `( String -- String Bool )` | Get env variable. Returns (value, exists) |
| `os.home-dir` | `( -- String Bool )` | Get home directory. Returns (path, success) |
| `os.current-dir` | `( -- String Bool )` | Get current directory. Returns (path, success) |
| `os.path-exists` | `( String -- Bool )` | Check if path exists |
| `os.path-is-file` | `( String -- Bool )` | Check if path is regular file |
| `os.path-is-dir` | `( String -- Bool )` | Check if path is directory |
| `os.path-join` | `( String String -- String )` | Join two path components |
| `os.path-parent` | `( String -- String Bool )` | Get parent directory. Returns (path, success) |
| `os.path-filename` | `( String -- String Bool )` | Get filename. Returns (name, success) |
| `os.exit` | `( Int -- )` | Exit program with status code |
| `os.name` | `( -- String )` | Get OS name (e.g., "macos", "linux") |
| `os.arch` | `( -- String )` | Get CPU architecture (e.g., "aarch64", "x86_64") |

## Terminal Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `terminal.raw-mode` | `( Bool -- )` | Enable/disable raw mode. Raw: no buffering, no echo, Ctrl+C = byte 3 |
| `terminal.read-char` | `( -- Int )` | Read single byte (blocking). Returns 0-255 or -1 on EOF |
| `terminal.read-char?` | `( -- Int )` | Read single byte (non-blocking). Returns 0-255 or -1 if none |
| `terminal.width` | `( -- Int )` | Get terminal width in columns. Returns 80 if unknown |
| `terminal.height` | `( -- Int )` | Get terminal height in rows. Returns 24 if unknown |
| `terminal.flush` | `( -- )` | Flush stdout |

## String Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `string.concat` | `( String String -- String )` | Concatenate two strings |
| `string.length` | `( String -- Int )` | Get character length |
| `string.byte-length` | `( String -- Int )` | Get byte length |
| `string.char-at` | `( String Int -- Int )` | Get Unicode codepoint at index |
| `string.substring` | `( String Int Int -- String )` | Extract substring (start, length) |
| `string.find` | `( String String -- Int )` | Find substring. Returns index or -1 |
| `string.split` | `( String String -- List )` | Split by delimiter |
| `string.contains` | `( String String -- Bool )` | Check if contains substring |
| `string.starts-with` | `( String String -- Bool )` | Check if starts with prefix |
| `string.empty?` | `( String -- Bool )` | Check if empty |
| `string.equal?` | `( String String -- Bool )` | Check equality |
| `string.trim` | `( String -- String )` | Remove leading/trailing whitespace |
| `string.chomp` | `( String -- String )` | Remove trailing newline |
| `string.to-upper` | `( String -- String )` | Convert to uppercase |
| `string.to-lower` | `( String -- String )` | Convert to lowercase |
| `string.json-escape` | `( String -- String )` | Escape for JSON |
| `symbol.=` | `( Symbol Symbol -- Bool )` | Check symbol equality |

## Encoding Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `encoding.base64-encode` | `( String -- String )` | Encode to Base64 (standard, with padding) |
| `encoding.base64-decode` | `( String -- String Bool )` | Decode Base64. Returns (decoded, success) |
| `encoding.base64url-encode` | `( String -- String )` | Encode to URL-safe Base64 (no padding) |
| `encoding.base64url-decode` | `( String -- String Bool )` | Decode URL-safe Base64 |
| `encoding.hex-encode` | `( String -- String )` | Encode to lowercase hex |
| `encoding.hex-decode` | `( String -- String Bool )` | Decode hex string |

## Crypto Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `crypto.sha256` | `( String -- String )` | SHA-256 hash. Returns 64-char hex |
| `crypto.hmac-sha256` | `( String String -- String )` | HMAC-SHA256. (message, key) |
| `crypto.constant-time-eq` | `( String String -- Bool )` | Timing-safe comparison |
| `crypto.random-bytes` | `( Int -- String )` | Generate N random bytes as hex |
| `crypto.random-int` | `( Int Int -- Int )` | Generate random integer in range [min, max) |
| `crypto.uuid4` | `( -- String )` | Generate random UUID v4 |
| `crypto.aes-gcm-encrypt` | `( String String -- String Bool )` | AES-256-GCM encrypt. (plaintext, hex-key) |
| `crypto.aes-gcm-decrypt` | `( String String -- String Bool )` | AES-256-GCM decrypt. (ciphertext, hex-key) |
| `crypto.pbkdf2-sha256` | `( String String Int -- String Bool )` | Derive key. (password, salt, iterations) |
| `crypto.ed25519-keypair` | `( -- String String )` | Generate keypair. Returns (public, private) |
| `crypto.ed25519-sign` | `( String String -- String Bool )` | Sign message. (message, private-key) |
| `crypto.ed25519-verify` | `( String String String -- Bool )` | Verify signature. (message, signature, public-key) |

## Networking — net.http.* (HTTP client)

The HTTP **client** lives under `net.http.*`. The `std:http` stdlib module
provides server-side response/parsing helpers (`http-ok`, `http-request-path`,
…) and is unrelated.

The client is hand-rolled HTTP/1.1 over the may-aware TCP and TLS
primitives (see [net.tcp.connect](#networking--nettcp), [net.tls.client](#networking--nettls)).
Every IO step yields the strand cooperatively; the request never parks
the carrier thread. Hostname resolution and SSRF validation run through
the [net.dns.*](#networking--netdns) worker pool, so there is exactly
one `getaddrinfo` per request and it runs off the may carrier.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `net.http.get` | `( String -- Map )` | GET request. Map has `status`, `body`, `ok`, `error`. |
| `net.http.post` | `( String String String -- Map )` | POST request. `(url, body, content-type)`. Body is byte-clean — binary payloads round-trip intact. |
| `net.http.put` | `( String String String -- Map )` | PUT request. `(url, body, content-type)`. |
| `net.http.delete` | `( String -- Map )` | DELETE request. |

**Response Map shape:**

- `"status"` (Int): HTTP status code, or `0` on connection-level error.
- `"body"` (String): response body as raw bytes (byte-clean — binary downloads round-trip intact; decode as text yourself if you expected text).
- `"ok"` (Bool): true iff status is in `200..300`.
- `"error"` (String): error message; present only on failure.

**Connection pool.** Keep-alive connections are pooled by `(scheme, host, port)`
between requests. A second `net.http.get` against the same origin skips
the TCP and TLS handshakes when an idle entry is available. Defaults:

- 8 idle connections per `(scheme, host, port)`
- 30s idle timeout
- 256 idle connections globally
- MRU checkout (freshest pooled connection used first)
- Non-blocking half-closed peek on reuse: if the peer FIN'd the socket
  while it sat idle, the entry is dropped and a fresh connection is
  dialled.

**SSRF protection.** Requests are blocked when the URL's host resolves
to a loopback (`127.0.0.0/8`, `::1`), private (`10/8`, `172.16/12`,
`192.168/16`), link-local (`169.254/16`, including cloud metadata
endpoints), or unique-local (`fc00::/7`) IP. Schemes outside
`http`/`https` are rejected. The check runs against the addresses the
DNS layer already returned — no second resolution on the carrier.

**v1 limitations:**

- **No redirect following.** A 3xx response is returned to the caller as-is, with `Location` available in the body or via your own header parser.
- **No automatic decompression.** The client sends `Accept-Encoding: identity`. If you want gzip transfer, set up your own request and decompress with `compress.gunzip`.
- **No per-request timeout.** A deadline pass across the networking stack is a planned follow-up (alongside the connect-timeout gap inherited from PR2 and the handshake-timeout gap from PR3). The worst-case shape under this gap is a response with neither `Content-Length` nor `Transfer-Encoding: chunked` (EOF-framed body): the client reads until EOF, which an attacker-controlled server can stretch indefinitely. `Content-Length` and chunked responses are bounded by their own framing and by `MAX_BODY_SIZE` (10 MB).
- **No custom request headers.** Only `Host`, `User-Agent`, `Accept`, `Accept-Encoding`, `Connection`, and (for POST/PUT) `Content-Type` + `Content-Length` are sent. `Content-Type` rejects control characters in the value to prevent header injection. A header-bag API is a planned follow-up.
- **No client certificate auth, ALPN selection, or peer-cert inspection.** Inherited from `net.tls.client`.
- **POST is not auto-retried** on transient transport failure; GET/PUT/DELETE are (idempotent per RFC 9110). A POST that fails mid-flight surfaces as a connection error.

## Regular Expressions

All regex operations return a Bool success flag (false for invalid regex).

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `regex.match?` | `( String String -- Bool )` | Check if pattern matches. (text, pattern) |
| `regex.find` | `( String String -- String Bool )` | Find first match. Returns (match, success) |
| `regex.find-all` | `( String String -- List Bool )` | Find all matches. Returns (matches, success) |
| `regex.replace` | `( String String String -- String Bool )` | Replace first match. Returns (result, success) |
| `regex.replace-all` | `( String String String -- String Bool )` | Replace all matches. Returns (result, success) |
| `regex.captures` | `( String String -- List Bool )` | Extract capture groups. Returns (groups, success) |
| `regex.split` | `( String String -- List Bool )` | Split by pattern. Returns (parts, success) |
| `regex.valid?` | `( String -- Bool )` | Check if valid regex |

## Compression

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `compress.gzip` | `( String -- String Bool )` | Gzip compress. Returns base64-encoded |
| `compress.gzip-level` | `( String Int -- String Bool )` | Gzip at level 1-9 |
| `compress.gunzip` | `( String -- String Bool )` | Gzip decompress |
| `compress.zstd` | `( String -- String Bool )` | Zstd compress. Returns base64-encoded |
| `compress.zstd-level` | `( String Int -- String Bool )` | Zstd at level 1-22 |
| `compress.unzstd` | `( String -- String Bool )` | Zstd decompress |

## Variant Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `variant.field-count` | `( Variant -- Int )` | Get number of fields |
| `variant.tag` | `( Variant -- Symbol )` | Get tag (constructor name) |
| `variant.field-at` | `( Variant Int -- T )` | Get field at index |
| `variant.append` | `( Variant T -- Variant )` | Append value to variant |
| `variant.first` | `( Variant -- T )` | Get first field |
| `variant.last` | `( Variant -- T )` | Get last field |
| `variant.init` | `( Variant -- Variant )` | Get all fields except last |
| `variant.make-0` / `wrap-0` | `( Symbol -- Variant )` | Create variant with 0 fields |
| `variant.make-1` / `wrap-1` | `( T Symbol -- Variant )` | Create variant with 1 field |
| `variant.make-2` / `wrap-2` | `( T T Symbol -- Variant )` | Create variant with 2 fields |
| ... | ... | ... |
| `variant.make-12` / `wrap-12` | `( T ... T Symbol -- Variant )` | Create variant with 12 fields |

## List Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `list.make` | `( -- List )` | Create empty list |
| `list.push` | `( List T -- List )` | Push value onto list (COW: mutates in place if sole owner, else copies) |
| `list.get` | `( List Int -- T Bool )` | Get value at index. Returns (value, success) |
| `list.set` | `( List Int T -- List Bool )` | Set value at index. Returns (list, success) |
| `list.length` | `( List -- Int )` | Get number of elements |
| `list.empty?` | `( List -- Bool )` | Check if empty |
| `list.reverse` | `( List -- List )` | Return list with elements reversed |
| `list.first` | `( List -- T Bool )` | Get first element. Returns (value, success) — false on empty list |
| `list.last` | `( List -- T Bool )` | Get last element. Returns (value, success) — false on empty list |
| `list.map` | `( List [T -- U] -- List )` | Apply quotation to each element |
| `list.filter` | `( List [T -- Bool] -- List )` | Keep elements where quotation returns true |
| `list.fold` | `( List Acc [Acc T -- Acc] -- Acc )` | Reduce with accumulator |
| `list.each` | `( List [T --] -- )` | Execute quotation for each element |

## Map Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `map.make` | `( -- Map )` | Create empty map |
| `map.get` | `( Map K -- V Bool )` | Get value for key. Returns (value, success) |
| `map.set` | `( Map K V -- Map )` | Set key to value |
| `map.has?` | `( Map K -- Bool )` | Check if key exists |
| `map.remove` | `( Map K -- Map )` | Remove key |
| `map.keys` | `( Map -- List )` | Get all keys |
| `map.values` | `( Map -- List )` | Get all values |
| `map.size` | `( Map -- Int )` | Get number of entries |
| `map.empty?` | `( Map -- Bool )` | Check if empty |

## Float Arithmetic

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.add` / `f.+` | `( Float Float -- Float )` | Add two floats |
| `f.subtract` / `f.-` | `( Float Float -- Float )` | Subtract second from first |
| `f.multiply` / `f.*` | `( Float Float -- Float )` | Multiply two floats |
| `f.divide` / `f./` | `( Float Float -- Float )` | Divide first by second |

## Float Comparison

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.=` / `f.eq` | `( Float Float -- Bool )` | Test equality |
| `f.<` / `f.lt` | `( Float Float -- Bool )` | Test less than |
| `f.>` / `f.gt` | `( Float Float -- Bool )` | Test greater than |
| `f.<=` / `f.lte` | `( Float Float -- Bool )` | Test less than or equal |
| `f.>=` / `f.gte` | `( Float Float -- Bool )` | Test greater than or equal |
| `f.<>` / `f.neq` | `( Float Float -- Bool )` | Test not equal |

## Float Math

NaN/Infinity propagate per IEEE 754 — no `(value Bool)` flag. e.g. `f.sqrt`
of a negative returns `NaN`; `f.ln 0.0` returns `-Infinity`.

### Roots / powers

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.sqrt` | `( Float -- Float )` | Square root |
| `f.cbrt` | `( Float -- Float )` | Cube root (defined for negatives) |
| `f.pow` | `( Float Float -- Float )` | `base exp pow` |

### Exponential / logarithmic

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.exp` | `( Float -- Float )` | e^x |
| `f.ln` | `( Float -- Float )` | Natural log |
| `f.log10` | `( Float -- Float )` | Base-10 log |
| `f.log2` | `( Float -- Float )` | Base-2 log |

### Trigonometric (radians)

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.sin` | `( Float -- Float )` | Sine |
| `f.cos` | `( Float -- Float )` | Cosine |
| `f.tan` | `( Float -- Float )` | Tangent |
| `f.asin` | `( Float -- Float )` | Arcsine, range [-π/2, π/2] |
| `f.acos` | `( Float -- Float )` | Arccosine, range [0, π] |
| `f.atan` | `( Float -- Float )` | Arctangent, range (-π/2, π/2) |
| `f.atan2` | `( Float Float -- Float )` | `y x atan2` — angle of (x, y) from positive x-axis |

### Rounding

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.floor` | `( Float -- Float )` | Round toward -Infinity |
| `f.ceil` | `( Float -- Float )` | Round toward +Infinity |
| `f.round` | `( Float -- Float )` | Round to nearest, ties to even (banker's, IEEE 754 default) |
| `f.trunc` | `( Float -- Float )` | Round toward zero |

### Constants

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.pi` | `( -- Float )` | π |
| `f.e` | `( -- Float )` | e (Euler's number) |
| `f.tau` | `( -- Float )` | τ = 2π |

## Test Framework

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `test.init` | `( String -- )` | Initialize with test name |
| `test.finish` | `( -- )` | Finish and print results |
| `test.has-failures` | `( -- Bool )` | Check if any tests failed |
| `test.assert` | `( Bool -- )` | Assert boolean is true |
| `test.assert-not` | `( Bool -- )` | Assert boolean is false |
| `test.assert-eq` | `( actual expected -- )` | Assert two integers equal. Push the computed value first, the expected literal on top. |
| `test.assert-eq-str` | `( actual expected -- )` | Assert two strings equal. Push the computed value first, the expected literal on top. |
| `test.fail` | `( String -- )` | Mark test as failed with message |
| `test.pass-count` | `( -- Int )` | Get passed assertion count |
| `test.fail-count` | `( -- Int )` | Get failed assertion count |

## Time Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `time.now` | `( -- Int )` | Current Unix timestamp in seconds |
| `time.nanos` | `( -- Int )` | High-resolution monotonic time in nanoseconds |
| `time.sleep-ms` | `( Int -- )` | Sleep for N milliseconds |

## Serialization

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `son.dump` | `( T -- String )` | Serialize value to SON format (compact) |
| `son.dump-pretty` | `( T -- String )` | Serialize value to SON format (pretty) |

## Stack Introspection

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `stack.dump` | `( ... -- )` | Print all stack values and clear (REPL) |

---

## Standard Library Modules

These modules are included with `include std:<module-name>`.

---

### std:json - JSON Parsing

JSON parsing and serialization implemented in Seq.

```seq
include std:json

"hello" json-string json-serialize io.write-line  # "hello"
"{\"name\":\"bob\"}" json-parse drop json-serialize io.write-line
```

#### Value Constructors

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `json-null` | `( -- JsonValue )` | Create null |
| `json-bool` | `( Bool -- JsonValue )` | Create boolean |
| `json-true` | `( -- JsonValue )` | Create true |
| `json-false` | `( -- JsonValue )` | Create false |
| `json-number` | `( Float -- JsonValue )` | Create number |
| `json-int` | `( Int -- JsonValue )` | Create number from int |
| `json-string` | `( String -- JsonValue )` | Create string |
| `json-empty-array` | `( -- JsonArray )` | Create empty array |
| `json-empty-object` | `( -- JsonObject )` | Create empty object |

#### Builders

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `array-with` | `( JsonArray JsonValue -- JsonArray )` | Append element to array |
| `obj-with` | `( JsonObject JsonString JsonValue -- JsonObject )` | Add key-value pair |

#### Type Predicates

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `json-null?` | `( JsonValue -- JsonValue Bool )` | Check if null |
| `json-bool?` | `( JsonValue -- JsonValue Bool )` | Check if boolean |
| `json-number?` | `( JsonValue -- JsonValue Bool )` | Check if number |
| `json-string?` | `( JsonValue -- JsonValue Bool )` | Check if string |
| `json-array?` | `( JsonValue -- JsonValue Bool )` | Check if array |
| `json-object?` | `( JsonValue -- JsonValue Bool )` | Check if object |

#### Extractors

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `json-unwrap-bool` | `( JsonValue -- Bool )` | Extract boolean |
| `json-unwrap-number` | `( JsonValue -- Float )` | Extract number |
| `json-unwrap-string` | `( JsonValue -- String )` | Extract string |

#### Parsing & Serialization

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `json-parse` | `( String -- JsonValue Bool )` | Parse JSON string |
| `json-serialize` | `( JsonValue -- String )` | Serialize to JSON string |

---

### std:yaml - YAML Parsing

YAML parsing for configuration files and data.

```seq
include std:yaml

"name: hello\nport: 8080" yaml-parse drop yaml-serialize io.write-line
```

Supports: key-value pairs, nested objects (indentation), strings, numbers, booleans, null, comments.

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `yaml-parse` | `( String -- YamlValue Bool )` | Parse YAML string |
| `yaml-serialize` | `( YamlValue -- String )` | Serialize to JSON-like string |
| `yaml-null` | `( -- YamlValue )` | Create null |
| `yaml-bool` | `( Bool -- YamlValue )` | Create boolean |
| `yaml-number` | `( Float -- YamlValue )` | Create number |
| `yaml-string` | `( String -- YamlValue )` | Create string |
| `yaml-empty-object` | `( -- YamlObject )` | Create empty object |
| `yaml-obj-with` | `( YamlObject key value -- YamlObject )` | Add key-value pair |

---

### std:http - HTTP Response Helpers

Helper functions for building HTTP servers.

```seq
include std:http

"Hello, World!" http-ok   # Returns full HTTP 200 response
request http-request-path # Extract path from request
```

#### Response Building

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `http-ok` | `( String -- String )` | Build 200 OK response |
| `http-not-found` | `( String -- String )` | Build 404 response |
| `http-error` | `( String -- String )` | Build 500 response |
| `http-response` | `( Int String String -- String )` | Build custom response (code, reason, body) |

#### Request Parsing

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `http-request-line` | `( String -- String )` | Extract first line |
| `http-request-path` | `( String -- String )` | Extract path |
| `http-request-method` | `( String -- String )` | Extract method |
| `http-is-get` | `( String -- Bool )` | Check if GET request |
| `http-is-post` | `( String -- Bool )` | Check if POST request |
| `http-path-is` | `( String String -- Bool )` | Check if path matches |
| `http-path-starts-with` | `( String String -- Bool )` | Check path prefix |
| `http-path-suffix` | `( String String -- String )` | Extract path after prefix |

---

### std:list - List Utilities

Convenient words for building lists.

```seq
include std:list

list-of 1 lv 2 lv 3 lv  # Build [1, 2, 3]
```

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `list-of` | `( -- List )` | Create empty list (alias for `list.make`) |
| `lv` | `( List V -- List )` | Append value (alias for `list.push`) |

---

### std:map - Map Utilities

No additional utilities beyond the built-in [Map Operations](#map-operations).

---

### std:imath - Integer Math

Common mathematical operations for integers.

```seq
include std:imath

-5 abs            # 5
48 18 gcd         # 6
15 0 100 clamp    # 15
```

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `abs` | `( Int -- Int )` | Absolute value |
| `max` | `( Int Int -- Int )` | Maximum |
| `min` | `( Int Int -- Int )` | Minimum |
| `gcd` | `( Int Int -- Int )` | Greatest common divisor |
| `sign` | `( Int -- Int )` | Sign (-1, 0, or 1) |
| `square` | `( Int -- Int )` | Square |
| `clamp` | `( Int Int Int -- Int )` | Clamp between min and max |

For modulo and power, use the `i.modulo` and `i.pow` builtins directly (`( Int Int -- Int Bool )` — see the Integer Arithmetic section above).

---

### std:fmath - Float Math

Common mathematical operations for floats.

```seq
include std:fmath

-3.14 f.abs       # 3.14
2.5 3.7 f.max     # 3.7
1.5 f.square      # 2.25
```

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.abs` | `( Float -- Float )` | Absolute value |
| `f.max` | `( Float Float -- Float )` | Maximum |
| `f.min` | `( Float Float -- Float )` | Minimum |
| `f.sign` | `( Float -- Float )` | Sign (-1.0, 0.0, or 1.0) |
| `f.square` | `( Float -- Float )` | Square |
| `f.neg` | `( Float -- Float )` | Negate |
| `f.clamp` | `( Float Float Float -- Float )` | Clamp between min and max |

For sqrt, trig, exp/log, rounding, and constants (`f.pi`, `f.e`, `f.tau`),
see the [Float Math](#float-math) builtin section above — those are
always available without an `include`.

---

### std:zipper - Functional List Zipper

A zipper provides O(1) cursor movement and "editing" of immutable lists by maintaining
a focus element with left and right context.

```seq
include std:zipper

list-of 1 lv 2 lv 3 lv 4 lv 5 lv
zipper.from-list
zipper.right zipper.right   # focus is now 3
zipper.focus                # get current element (3)
10 zipper.set               # replace focus with 10
zipper.to-list              # [1, 2, 10, 4, 5]
```

#### Construction

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `zipper.from-list` | `( List -- Zipper )` | Create zipper from list, focus at first element |
| `zipper.to-list` | `( Zipper -- List )` | Convert zipper back to list |
| `zipper.make-empty` | `( -- Zipper )` | Create empty zipper |

#### Navigation

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `zipper.right` | `( Zipper -- Zipper )` | Move focus right (no-op at end) |
| `zipper.left` | `( Zipper -- Zipper )` | Move focus left (no-op at start) |
| `zipper.start` | `( Zipper -- Zipper )` | Move focus to first element |
| `zipper.end` | `( Zipper -- Zipper )` | Move focus to last element |

#### Query

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `zipper.focus` | `( Zipper -- T )` | Get focused element |
| `zipper.empty?` | `( Zipper -- Bool )` | Check if zipper is empty |
| `zipper.at-start?` | `( Zipper -- Zipper Bool )` | Check if at first element |
| `zipper.at-end?` | `( Zipper -- Zipper Bool )` | Check if at last element |
| `zipper.length` | `( Zipper -- Int )` | Get total number of elements |
| `zipper.index` | `( Zipper -- Int )` | Get current focus index (0-based) |

#### Modification

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `zipper.set` | `( Zipper T -- Zipper )` | Replace focused element |
| `zipper.insert-left` | `( Zipper T -- Zipper )` | Insert element to left of focus |
| `zipper.insert-right` | `( Zipper T -- Zipper )` | Insert element to right of focus |
| `zipper.delete` | `( Zipper -- Zipper )` | Delete focused element, focus moves right (or left at end) |

---

### std:signal - Signal Handling

Unix signal handling with a safe, flag-based API.

```seq
include std:signal

signal.trap-shutdown  # Trap SIGINT and SIGTERM

: server-loop ( -- )
  signal.shutdown-requested? if
    "Shutting down..." io.write-line
  else
    handle-request
    server-loop
  then
;
```

#### Signal Constants (builtins)

| Word | Description |
|------|-------------|
| `signal.SIGINT` | Interrupt (Ctrl+C) |
| `signal.SIGTERM` | Termination request |
| `signal.SIGHUP` | Hangup |
| `signal.SIGPIPE` | Broken pipe |
| `signal.SIGUSR1` | User-defined 1 |
| `signal.SIGUSR2` | User-defined 2 |

#### Convenience Words

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `signal.shutdown-requested?` | `( -- Bool )` | Check for SIGINT or SIGTERM |
| `signal.trap-shutdown` | `( -- )` | Trap SIGINT and SIGTERM |
| `signal.ignore-sigpipe` | `( -- )` | Ignore SIGPIPE (for servers) |

---

### std:son - Seq Object Notation Helpers

Convenience module that re-exports `std:map` and `std:list`, providing all the builder words needed for SON data construction.

```seq
include std:son

map-of "host" "localhost" kv "port" 8080 kv
list-of 1 lv 2 lv 3 lv
```

Including `std:son` gives you `map-of`, `kv`, `list-of`, and `lv`.

**Security warning:** SON files are executable Seq code. Only load SON from trusted sources.

---

### std:stack-utils - Stack Utilities

Common stack operations built from primitives.

```seq
include std:stack-utils

1 2 3 2drop  # Stack: 1
```

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `2drop` | `( A B -- )` | Drop top two values |

---

*Built-in operations: 152 total. Standard library modules provide additional functionality.*
