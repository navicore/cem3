# Socket Type, Type-Variable Tightening, and Networking Namespace

## Intent

Three issues showed up while writing a TCP exercise. They're separate
problems but stacked: each higher layer is cosmetic until the one below
it is fixed.

1. **No nominal `Socket` type.** `tcp.listen` and `tcp.accept` return
   `Int` (a file descriptor). Any `Int` can be passed to `tcp.write` or
   `tcp.close`. Stack-effect annotations like `( Socket -- )` look like
   they describe intent, but the compiler treats `Socket` as a fresh
   polymorphic type variable — the annotation is documentation only.
2. **Unknown uppercase identifiers silently become type variables.**
   `crates/compiler/src/parser/type_parse.rs:99-110` accepts any
   uppercase token as a type variable when it isn't a known concrete
   type or registered union. So `Socket`, `Sokcet`, and `T` parse
   identically. The validator comment at
   `typechecker/validation.rs:86-91` already acknowledges this is a
   known hole. Without #1 there is nowhere to land — every name is
   either a real type or implicitly polymorphic.
3. **Networking namespace conflates layers.**
   `examples/io/README.md:10-19` writes `include std:http` and then
   uses `tcp.read-request` / `tcp.write-response`, implying TCP comes
   from `std:http`. The built-in `http.*` (HTTP client) shares a prefix
   with `std:http`'s hyphen-named server helpers (`http-ok`,
   `http-request-path`), making the two layers look like one bundle.
   Internally `http.*` builtin signatures live in `builtins/text.rs`
   next to base64.

The fix is layered: introduce a real `Socket` type (so words have
something concrete to reference), tighten the parser (so typos can't
masquerade as the new type), then reorganize the networking namespace
(now safe because the underlying types carry meaning).

## Constraints

- **Existing `(value Bool)` error contract is preserved.** Every
  renamed or retyped word keeps its trailing `Bool`.
- **`tcp.*` already returns an `Int` to user code.** A `Socket` type
  must be representable in the existing tagged-pointer stack without a
  new `Value` variant — easiest path is a thin nominal wrapper that
  lowers to the same i64 file descriptor at runtime.
- **Single-letter type variables (`T`, `U`, `A`, `V`) must remain
  type variables.** The parser tightening should hit *unknown
  multi-character* uppercase identifiers, not break existing
  polymorphic word definitions across the stdlib.
- **Breaking change is acceptable.** This is v7.0 material. Old word
  names disappear with a clear renamed-to error, no aliases, no
  deprecation shims.
- **Out of scope:** TLS, async sockets, an HTTP server framework, URL
  parsing, generalized opaque-handle infrastructure for other
  resources (file handles, regex compiled patterns, etc.) — Socket is
  the proof of concept; generalize after it lands.

## Approach

### Layer 1 — `Socket` as a nominal type

Add `Type::Socket` to `crates/compiler/src/types.rs`, treated like
`Type::Channel` or `Type::Int`: a known concrete type that unifies
only with itself. At runtime a Socket is still a tagged i64 file
descriptor — no new `Value` variant, no runtime cost. Update the
TCP/UDP builtin signatures in `crates/compiler/src/builtins/tcp.rs`
and `udp.rs`:

```
tcp.listen ( Int -- Socket Bool )
tcp.accept ( Socket -- Socket Bool )
tcp.read   ( Socket -- String Bool )
tcp.write  ( String Socket -- Bool )
tcp.close  ( Socket -- Bool )
```

Now the type checker rejects `42 tcp.write` and the documentation
matches reality.

### Layer 2 — parser tightens unknown uppercase identifiers

In `parser/type_parse.rs:99-110`, allow short single-letter uppercase
identifiers (`T`, `U`, `V`, `A`, …) as type variables, but require
multi-character uppercase identifiers to be a known concrete type or
registered union. `Socket` (now real) parses to `Type::Socket`.
`Sokcet` errors with "Unknown type 'Sokcet' — did you mean 'Socket'?"
This is a small whitelist change, plus a Levenshtein hint against the
known-types set.

Existing stdlib uses `Ctx`, `Acc`, `Handle` as type variables in a few
places — those need a one-line audit and either a rename to
single-letter or an explicit declaration syntax (`( T:: -- T )`
deferred to a later RFC). Concrete count: grep shows fewer than a
dozen sites.

### Layer 3 — networking namespace reorg

With `Socket` carrying meaning, the namespace cleanup is mechanical:

| Today | After |
|-------|-------|
| `tcp.listen/accept/read/write/close` | `net.tcp.listen/accept/read/write/close` |
| `udp.bind/send-to/receive-from/close` | `net.udp.bind/send-to/receive-from/close` |
| `http.get/post/put/delete` (in `text.rs`) | `net.http.get/post/put/delete` (in new `builtins/http.rs`) |
| `std:http` stdlib | unchanged — keeps server-side helpers |

The compiler emits `error: 'tcp.listen' was renamed to
'net.tcp.listen' in v7.0` for old names (small lookup table, removed
in v8.0). `lints.toml` and `error_flag_lint/state.rs` patterns move in
lockstep.

## Domain Events

- **New `Socket` type registered.** Type checker rejects `Int → Socket`
  flow without an explicit cast (`fd->socket`) and vice versa
  (`socket->fd`) — both added as builtins so escape hatches exist for
  FFI and debugging.
- **Parser typo errors emitted.** Unknown multi-character uppercase
  identifiers in stack effects produce a typed error instead of
  silently becoming polymorphic.
- **Networking words renamed.** Every old `tcp.*` / `udp.*` / `http.*`
  call site under `examples/`, `tests/`, `benchmarks/`, stdlib, and
  `crates/repl/src/app.rs` is updated. The compiler's rename table
  catches the rest.
- **Docs realigned.** `STDLIB_REFERENCE.md` gains a `Networking
  (net.*)` section. `examples/io/README.md` and `docs/EXAMPLES.md`
  stop showing TCP under `std:http`. A `MIGRATION_7_0.md` lists every
  rename with before/after.
- **Seqlings and any external `.seq` consumers break loudly.** That's
  the point — silent breakage is what got us here.

## Checkpoints

1. `42 tcp.close` (now `42 net.tcp.close`) is a type error: cannot
   unify `Int` with `Socket`. Today it compiles.
2. `: bad ( Sokcet -- ) drop ;` is a parse-time error with a "did you
   mean 'Socket'?" hint. Today it compiles.
3. `: poly ( T -- T T ) dup ;` still compiles unchanged.
4. `seqc lint` still fires on `net.tcp.listen drop` (renamed lint
   patterns work).
5. `examples/language/http_simple.seq` compiles, runs, and `curl
   localhost:8080/` returns the same response.
6. `STDLIB_REFERENCE.md` shows TCP and HTTP-client builtins under one
   `Networking` section, with `std:http` clearly labeled as
   server-side helpers built on top.
7. The seqlings exercise that triggered this (`26-tcp/05-echo.seq`)
   uses `( Socket -- )` and the annotation now means what it says.
