# Non-Blocking Networking Rework

## Intent

Make Seq's networking stack honest about its values: every IO call yields
cooperatively, no Seq word ever parks an OS thread on a `may` carrier,
and the language exposes the primitives needed to build *clients* in Seq
itself — not just servers.

Today `net.http.*` wraps `ureq`, a synchronous `std::net` client. Every
request parks the OS thread that the requesting strand happens to be
running on, including the `getaddrinfo` call inside our own SSRF
validator (`http_client.rs:180`). With a default may carrier pool of a
few threads, real-world HTTP concurrency is silently bottlenecked. There
is also no `net.tcp.connect`, no DNS builtin, and no TLS builtin — so a
user cannot write a Redis, Postgres, SMTP, or raw-TCP client in Seq at
all. This rework fixes both gaps with one coherent stack.

## Constraints

**Must not break:**

- `net.tcp.*` server signatures, `net.udp.*` (already symmetric and
  may-aware), `net.http.get/post/put/delete` user-facing signatures and
  response-Map shape (`status` / `body` / `ok` / `error`).
- Byte-clean strings on socket reads and HTTP bodies.
- SSRF protection — relocate, never delete.

**Out of scope:**

- HTTP/2, HTTP/3, WebSockets, gRPC.
- A real connection pool (ureq did this transparently; revisit after
  the base stack lands).
- Resolver caching, redirects beyond minimum, cookies, sessions.
- The user-facing surface of `net.udp.*`. Internally, `send-to`'s
  hostname-to-IP resolution gets routed through the new DNS layer so
  the may carrier no longer blocks on `getaddrinfo` (closes a latent
  hazard at `udp.rs:225`); the builtin's signature and semantics are
  unchanged.
- Implementing HTTP framing in Seq itself — that's the eventual
  destination once string ergonomics catch up, not this rework.

**Hard invariants (CI-enforceable):**

- No `std::net::{TcpStream,TcpListener,UdpSocket,ToSocketAddrs}` call
  in any builtin reachable from a may carrier. The only allowed socket
  types are `may::net::*`.
- No new networking dep beyond `rustls` and `webpki-roots` (already
  pulled transitively). `ureq` is removed. `tokio`, `hyper`, `reqwest`,
  `trust-dns-resolver` are forbidden.

## Approach

Four layers, each may-aware, each composable:

1. **DNS** — `net.dns.resolve ( String -- List<String> Bool )`. A small
   dedicated OS-thread pool runs `getaddrinfo` (so we inherit all
   platform-correct resolution behaviour — `/etc/hosts`, systemd-resolved,
   VPN/corp DNS, mDNS, the lot — without re-implementing it). The
   requesting strand sends the hostname into the pool over a std mpsc
   queue and `recv()`s the answer on a `may::sync` channel, which yields
   cleanly. Carrier threads never block on a name lookup. A short
   in-process LRU caches recent answers so the fanout case (1k strands
   resolving the same host) doesn't queue 1k jobs through the pool.
2. **TCP outbound** — `net.tcp.connect ( String Int -- Socket Bool )`.
   Resolves via DNS layer, then `may::net::TcpStream::connect`. Same
   `Socket` nominal type, same registry, same byte-clean read/write.
3. **TLS** — `net.tls.client ( Socket String -- Socket Bool )`. Wraps a
   connected `Socket` in `rustls::ClientConnection` over a `may` stream;
   hostname drives SNI and cert validation; roots from `webpki-roots`.
   The result lives in the `Socket` registry; existing `tcp.read` /
   `tcp.write` / `tcp.close` work through it transparently.
4. **HTTP client** — rewrite `net.http.*` over (1)–(3). HTTP/1.1 framing
   lives in `runtime/src/http_client.rs` as hand-rolled may-aware code,
   no external HTTP crate. SSRF validation moves earlier in the pipeline
   and runs against the addresses the DNS layer already returns — no
   second `getaddrinfo`.

   **Connection pool ships in v1.** An idle map keyed by
   `(scheme, host, port)` retains keep-alive connections after each
   request; subsequent requests to the same host skip the TCP and TLS
   handshake. Bounded per-host count, idle-timeout eviction,
   half-closed-detection on reuse. This closes the only meaningful
   performance regression vs. the old `ureq`-based client.

   *Long-term direction:* this HTTP framing migrates from Rust into pure
   Seq stdlib once Seq's byte-string parsing primitives make it
   pleasant. The Rust impl is a bridge, not the destination. Seq must
   become more implemented in Seq — networking framing is the natural
   first port from FFI into the language itself.

Adjacent cleanup in scope:

- Revisit the `WouldBlock + yield_now()` loop in `tcp.rs:237–275`.
  `may::net::TcpStream::read` should yield natively; the dance probably
  papers over a wrongly-set-nonblocking socket.
- Route `net.udp.send-to`'s host resolution through the new DNS layer
  so its current `format!("{host}:{port}")` → `ToSocketAddrs` path
  (which silently calls blocking `getaddrinfo` on the may carrier for
  hostname arguments) becomes cooperative. Public signature unchanged.

## Domain Events

- **Produces:** a `Socket` handle whose every IO step yields the
  strand; an outbound TCP client capability that didn't exist before; a
  may-aware HTTP client; the foundation for a Seq-level networking
  stdlib.
- **Consumes:** the may scheduler invariant (carrier threads must not
  park); the `Socket`/byte-clean string contracts; the `Map`-shaped
  HTTP response contract.
- **Must follow:** any future protocol client (Redis, Postgres, SMTP,
  NATS, HTTP/2) builds on these four layers, not on `std::net`. The
  `net.http.*` builtins become migration candidates from FFI into
  stdlib Seq — they are the natural first port once Seq has the string
  parsing words to make it pleasant.

## Checkpoints

1. `cargo tree -e normal | grep -E '\bureq\b|\btokio\b|\bhyper\b'` is
   empty.
2. `grep -rEn 'std::net::(TcpStream|TcpListener|UdpSocket|ToSocketAddrs)'
   crates/runtime/src/` shows only IP-address parsing, no socket IO.
3. 1024 strands each issuing `net.http.get` against a 1-second-sleep
   endpoint complete in roughly 1s wall clock on the default carrier
   pool — not `1024 × 1s / carriers` as today.
4. `"1.1.1.1" 53 net.tcp.connect` returns a usable `Socket`.
5. `"https://example.com" net.http.get` returns the prior Map shape;
   `http://localhost` / `http://169.254.169.254` still blocked by SSRF.
6. `net.tls.client` succeeds against a valid cert, fails against
   expired or self-signed.
7. `MAX_BODY_SIZE` rejection on oversized bodies still triggers.
8. Two sequential `net.http.get` calls to the same `https://` host:
   the second call's wall time excludes TLS handshake RTTs (proves the
   pool reused the keep-alive connection). Visible as a perf assertion
   or via a debug counter on agent reuse.
9. `"" "telemetry.example.com" 8125 socket net.udp.send-to` against a
   hostname argument does not park the carrier thread (proves the
   `send-to` host-resolution fix landed). Pre-fix behaviour was a
   blocking `getaddrinfo` on the calling may worker.
