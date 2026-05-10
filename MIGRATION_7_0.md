# Migrating to Seq 7.0

Status: in progress

Seq 7.0 cleans up tech debt in the standard library and expands the
floating-point math surface. Per the project's "no pass-through stdlib
wrappers" rule, the same change deletes one stdlib word that violated
it. Breaking changes land without aliases or shims — the compiler's
"Undefined word" error is the migration mechanism.

This document is the **transformation spec** — the rules below are
sufficient for a careful human or an LLM collaborator to migrate any
existing Seq source.

---

## Rule 1 — `mod` is removed; call `i.modulo` directly

The stdlib word `mod` was a one-line pass-through over the `i.modulo`
builtin, exactly the kind of noise the project rule forbids. It is
deleted from `std:imath` in 7.0.

`mod` had stack effect `( Int Int -- Int Bool )` — identical to
`i.modulo`. The migration is a literal rename:

```
a b mod
```

becomes

```
a b i.modulo
```

If you imported `std:imath` only for `mod`, you may also drop the
`include std:imath` line.

---

## Rule 2 — Float math now lives in builtins, not `std:fmath`

7.0 adds 23 floating-point math builtins (sqrt, cbrt, pow, exp/log,
trig, rounding, and the constants π / e / τ). These are **builtins**,
not `std:fmath` words — they are always available without an
`include`, just like `f.+` and `f.<`.

`std:fmath` is unchanged: it continues to provide `f.abs`, `f.neg`,
`f.sign`, `f.square`, `f.clamp`, `f.min`, `f.max` (the *composed*
words that earn their stdlib slot).

No code changes are required by this rule — the new builtins are
purely additive. The note exists so that future readers understand
why `f.sqrt`, `f.sin`, `f.pi`, etc. work without `include std:fmath`.

### Reference

| Group | Builtins |
|-------|----------|
| Roots / powers | `f.sqrt`, `f.cbrt`, `f.pow` |
| Exp / log | `f.exp`, `f.ln`, `f.log10`, `f.log2` |
| Trig | `f.sin`, `f.cos`, `f.tan`, `f.asin`, `f.acos`, `f.atan`, `f.atan2` |
| Rounding | `f.floor`, `f.ceil`, `f.round`, `f.trunc` |
| Constants | `f.pi`, `f.e`, `f.tau` |

### Notes on semantics

- IEEE 754 propagation is the error contract — no `(value Bool)` flag.
  `f.sqrt` of a negative is `NaN`; `f.ln 0.0` is `-Infinity`.
- `f.round` uses banker's rounding (ties to even, IEEE 754 default):
  `0.5 f.round → 0.0`, `1.5 f.round → 2.0`, `2.5 f.round → 2.0`.
  Use `f.trunc`, `f.floor`, or `f.ceil` if you need a different
  rounding mode.
- `f.atan2` argument order is `( y x -- result )`, matching C/Rust/JS.
- `f.pow` argument order is `( base exp -- result )`.

---

## Rule 3 — TCP/UDP file descriptors are now `Socket`, not `Int`

A new nominal type `Socket` (a compile-time phantom over `Int`) is
introduced so the type checker can distinguish a real socket fd from an
arbitrary integer. The runtime representation is unchanged
(`Value::Int(fd)`), so this is a pure compile-time tightening.

The TCP and UDP builtin signatures change as follows (both via Rule 5
rename to `net.*`):

| Word | Before | After |
|------|--------|-------|
| `net.tcp.listen` | `( Int -- Int Bool )` | `( Int -- Socket Bool )` |
| `net.tcp.accept` | `( Int -- Int Bool )` | `( Socket -- Socket Bool )` |
| `net.tcp.read`   | `( Int -- String Bool )` | `( Socket -- String Bool )` |
| `net.tcp.write`  | `( String Int -- Bool )` | `( String Socket -- Bool )` |
| `net.tcp.close`  | `( Int -- Bool )` | `( Socket -- Bool )` |
| `net.udp.bind`   | `( Int -- Int Int Bool )` | `( Int -- Socket Int Bool )` |
| `net.udp.send-to` | `( String String Int Int -- Bool )` | `( String String Int Socket -- Bool )` |
| `net.udp.receive-from` | `( Int -- String String Int Bool )` | `( Socket -- String String Int Bool )` |
| `net.udp.close`  | `( Int -- Bool )` | `( Socket -- Bool )` |

### Migration

Update any user-defined word that takes or returns a socket fd to spell
the type `Socket` in its stack effect:

```
: handle-conn ( Int -- )      →    : handle-conn ( Socket -- )
```

If you must round-trip through a raw `Int` (FFI, debugging), use the
two cast builtins:

```
fd->socket    ( Int    -- Socket )
socket->fd    ( Socket -- Int )
```

`42 net.tcp.close` is now a type error; use `42 fd->socket net.tcp.close`
if you really mean it.

---

## Rule 4 — Multi-character ad-hoc type variables are now an error

Previously, any uppercase identifier in a stack effect that wasn't a
known type silently degraded to a fresh polymorphic type variable. So
`( Sokcet -- )` compiled. So did `( Acc Ctx Handle -- )` even though
those names had no meaning to the compiler — they unified with anything.

Under the strict rule, only **single-character** uppercase identifiers
(`T`, `U`, `V`, `K`, `M`, …) are accepted as type variables. Multi-
character uppercase identifiers must be either a concrete type
(`Int`, `Float`, `Bool`, `String`, `Symbol`, `Channel`, `Socket`,
`Variant`) or a registered union. Anything else is a parse / validation
error with a Levenshtein "did you mean" hint.

### Migration

Rename ad-hoc multi-character names to single letters:

```
( Ctx Int -- )       →   ( W Int -- )
( Acc List -- Acc )  →   ( A L -- A )
( Sokcet -- )        →   ( Socket -- )         # if you actually meant Socket
```

If you have a meaningful domain type, define it as a `union` so the
parser sees it as nominal:

```
union TokenList { TNil, TCons { head: String, tail: TokenList } }
: tokenize ( String -- TokenList ) ... ;
```

---

## Rule 5 — Networking builtins moved under `net.*`

`tcp.*`, `udp.*`, and `http.*` (the HTTP **client**) are renamed to
`net.tcp.*`, `net.udp.*`, and `net.http.*`. The transport layer (TCP,
UDP) and the HTTP client now share a single `net.*` umbrella, sibling
to each other rather than awkwardly grouped with unrelated builtins.

`std:http` is **unchanged** — it remains the home for server-side
response/parsing helpers (`http-ok`, `http-not-found`,
`http-request-path`, `http-response`, …). Only the include path stays
the same; the words inside have not moved.

### Rename table

| Before | After |
|--------|-------|
| `tcp.listen` | `net.tcp.listen` |
| `tcp.accept` | `net.tcp.accept` |
| `tcp.read` | `net.tcp.read` |
| `tcp.write` | `net.tcp.write` |
| `tcp.close` | `net.tcp.close` |
| `udp.bind` | `net.udp.bind` |
| `udp.send-to` | `net.udp.send-to` |
| `udp.receive-from` | `net.udp.receive-from` |
| `udp.close` | `net.udp.close` |
| `http.get` | `net.http.get` |
| `http.post` | `net.http.post` |
| `http.put` | `net.http.put` |
| `http.delete` | `net.http.delete` |

The compiler emits a targeted error for each old name:

```
'tcp.listen' was renamed to 'net.tcp.listen' in v7.0 (called in word 'main').
See docs/MIGRATION_7_0.md.
```

The rename table is removed in v8.0.

The `seq:allow(unchecked-tcp-write)` lint allow-list IDs are unchanged
— only the patterns they match against were updated. Existing
suppressions keep working.
