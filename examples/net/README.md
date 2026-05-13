# Networking

Five layers, bottom to top: DNS → TCP → UDP → TLS → HTTP. Each subfolder
has runnable example(s) using the corresponding `net.*` builtins.

The stack is **may-aware end to end**: every IO step yields the
cooperative carrier instead of blocking it. Hostnames resolve through
a dedicated worker pool so `getaddrinfo` runs off the carrier. See
`docs/STDLIB_REFERENCE.md` for the full word reference, or
`docs/design/done/NONBLOCKING_NETWORKING.md` for the design rationale.

## Examples

### `dns/resolve.seq` — `net.dns.resolve`

Resolves a hostname and prints each IP returned. Demonstrates the
worker-pool-offload path; useful as a one-liner for "what does this
hostname actually resolve to from inside Seq."

```
seqc build dns/resolve.seq -o /tmp/dns-resolve
/tmp/dns-resolve
```

### `tcp/client.seq` — `net.tcp.connect`

Minimal TCP client: connect to `example.com:80`, send an HTTP/1.0
request, read the response, close. Plain TCP — no framing helpers —
to show what `net.tcp.*` looks like on its own.

```
seqc build tcp/client.seq -o /tmp/tcp-client
/tmp/tcp-client
```

### `tcp/server.seq` — plain TCP echo server

Not all networking is HTTP. Accepts a TCP connection, echoes whatever
the client sent back, closes. Each connection runs in its own strand
(green thread) so the server handles concurrent clients cooperatively.

```
seqc build tcp/server.seq -o /tmp/tcp-server
/tmp/tcp-server &
echo hello | nc localhost 9000
```

### `tcp/http-routing.seq` — HTTP server on top of `net.tcp.*`

The companion to `tcp/server.seq`: same accept-loop shape, with
HTTP/1.1 request parsing and a `cond`-driven router on top. Doubles as
a tutorial on concatenative programming (the source is heavily
commented). Lives under `tcp/` because `net.http.*` is client-only —
the server is hand-rolled over `net.tcp.read` / `net.tcp.write`.

```
seqc build tcp/http-routing.seq -o /tmp/http-routing
/tmp/http-routing &
curl http://localhost:8080/
curl http://localhost:8080/health
curl http://localhost:8080/echo
curl http://localhost:8080/invalid   # 404
```

### `udp/echo.seq` — `net.udp.bind` + `net.udp.send-to` + `net.udp.receive-from`

Single-program UDP loopback: bind two sockets, send a datagram from
one to the other, receive it, print. Mirrors the round-trip pattern
the integration test uses.

```
seqc build udp/echo.seq -o /tmp/udp-echo
/tmp/udp-echo
```

### `tls/client.seq` — `net.tls.client`

The TCP client above with one extra step: after `net.tcp.connect`
returns the Socket, `net.tls.client` upgrades it in place to a
TLS-wrapped Socket. Subsequent `net.tcp.read` / `net.tcp.write` calls
dispatch through rustls transparently — the rest of the code looks
exactly like the plain-TCP version.

```
seqc build tls/client.seq -o /tmp/tls-client
/tmp/tls-client
```

### `http/client.seq` — `net.http.get` / `.post` / `.put` / `.delete`

High-level HTTP/1.1 client: hand `net.http.get` a URL, get a response
Map back (`status`, `body`, `ok`, `error`). The client handles DNS
resolution, SSRF validation against the resolved IPs, connection
pooling keyed on `(scheme, host, port)`, TLS for `https://`, and
HTTP/1.1 framing — all the layers below are still there, just
composed into one builtin. Exercises GET, POST, PUT, DELETE against
httpbin.org.

```
seqc build http/client.seq -o /tmp/http-client
/tmp/http-client
```

## Reading order

For a layered tour, read top-to-bottom: `dns/resolve.seq` →
`tcp/client.seq` → `tcp/server.seq` → `tls/client.seq` →
`http/client.seq`. The HTTP-routing server (`tcp/http-routing.seq`)
is the "everything on top of TCP" deep dive once the rest clicks.

## What's not here yet

- mTLS, ALPN selection, peer-cert inspection (planned follow-ups —
  see issue #483 for the test anchors).
- Per-request timeouts (planned — see issue #484).
- A header-bag API for custom HTTP request headers.
