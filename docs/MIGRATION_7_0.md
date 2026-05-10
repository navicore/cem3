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
