# Float Math Builtins (issue #468)

## Intent

`std:fmath` is missing the bread-and-butter float ops: sqrt/cbrt, trig,
exp/log, rounding, and constants. Without them you can't write graphics,
physics, signal processing, or even `hypotenuse`. Issue #468 lists ~20
specific functions and is currently blocking seqlings chapter 28
(`28-std-fmath`).

The issue's title is "Expand `std:fmath` …" but that framing is wrong
about the mechanic. Every function on the list is a one-liner over
`f64::sqrt`, `f64::sin`, `f64::round`, etc. By the project convention
(stdlib words must be *composed*, not pass-throughs over a single
builtin), those belong in the **builtin `f.*` namespace** — same shelf
as `f.+`, `f.<`, `f.divide` — not in `std:fmath`.

`std:fmath` keeps doing what it does today: hold the *composed* float
words (`f.abs`, `f.neg`, `f.sign`, `f.square`, `f.clamp`, `f.min`,
`f.max`). It just doesn't grow.

## Constraints

- **Breaking changes are welcome — in builtins *and* stdlib.** This
  change lands before the user base grows. No deprecation aliases, no
  compatibility shims, no parallel old/new names. If a current word
  has the wrong shape or the wrong home, rename, move, or delete it
  in the same commit. The compiler's "unknown word" error is the
  migration mechanism. v7.0 absorbs all of it together; one
  `MIGRATION_7_0.md` covers everything.
- **No pass-through wrappers in `std:fmath`.** A `: f.sqrt ( Float --
  Float ) f.sqrt ;` style entry adds zero value over a direct builtin;
  it's noise and obscures the layer split.
- **`std:imath` parity.** `imath`'s composed words (`abs`, `gcd`,
  `pow`, `clamp`, `sign`, `square`) earn their stdlib slot because
  they compose. The one current `imath` violator —
  `imath.seq:90-92`, `: mod ( Int Int -- Int Bool ) i.modulo ;` — is
  deleted outright in the same change, no rename, no alias.
- **No `(value Bool)` for these.** IEEE 754 already has well-defined
  results for "bad" inputs (`NaN`, `±Inf`, `-0.0`). `f.sqrt -1.0`
  returns `NaN`; `f.ln 0.0` returns `-Inf`. This matches the existing
  `std:fmath` doc note. Adding a success flag would be inconsistent
  with `f.+` / `f.divide`, which also propagate IEEE values rather
  than flagging.
- **Single canonical name per op.** No `f.sqrt` *and* `f.square-root`.
  No `f.ln` *and* `f.log`. Match the issue's list exactly.
- **`f.atan2` argument order is `( y x -- )`.** This matches C/Rust/JS
  and the issue spec; document at the builtin definition.
- **Out of scope (this change):** complex numbers, hyperbolic
  functions (`sinh`/`cosh`/`tanh`), `f.hypot`, degree-based trig,
  decimal arithmetic. See **Follow-ups** below for `i.pow`.

## Approach

### Three groups, all builtins

**Pass-through to `f64` methods.** One Rust function each in
`crates/runtime/src/float_ops.rs`, one signature each in
`crates/compiler/src/builtins/float.rs`, one wiring entry each in
`crates/compiler/src/codegen/runtime/float.rs`. Mechanical and
identical to the existing `f.add` / `f.divide` shape. The list:

- Roots/powers: `f.sqrt`, `f.cbrt`, `f.pow` (`( Float Float -- Float )`,
  `base exp pow`)
- Exp/log: `f.exp`, `f.ln`, `f.log10`, `f.log2`
- Trig: `f.sin`, `f.cos`, `f.tan`, `f.asin`, `f.acos`, `f.atan`,
  `f.atan2`
- Rounding: `f.floor`, `f.ceil`, `f.round`, `f.trunc`

**Constants.** `f.pi`, `f.e`, `f.tau` are zero-arg builtins that push a
constant, not stdlib `: f.pi ( -- Float ) 3.14159… ;` words. Reasons:
(1) discoverable without an include, matching every other `f.*`; (2) a
Seq-level constant *is* a single-builtin wrapper by another name;
(3) avoids drift — Rust's `f64::consts::PI` is the source of truth,
not a hand-typed literal.

**Round semantics.** `f.round` uses `f64::round_ties_even` (banker's
rounding). Reasons: it's the IEEE 754 default rounding mode, avoids
the systematic bias of half-away-from-zero in statistics, and matches
Python 3's default `round()`. `f.trunc` covers the truncation case
explicitly. Document the choice in the builtin doc string.

### Cleanup in the same change

- Delete `imath.seq:90-92` (`: mod ( Int Int -- Int Bool ) i.modulo
  ;`). Update any seqling/example that uses `mod` to call `i.modulo`
  directly. This is the existing pass-through-noise violator and the
  rule needs to be enforced consistently or it isn't a rule.

## Domain Events

- **20 new builtins registered.** `add_signatures` in
  `builtins/float.rs` gains 20 entries; `add_docs` gains 20 doc
  strings; `runtime/float_ops.rs` gains 20 `extern "C"` shims; the
  codegen runtime table in `codegen/runtime/float.rs` gains 20
  declarations and 20 name mappings.
- **3 zero-arg constant builtins added.** Same wiring shape but with
  empty-input effect `( -- Float )`.
- **`std:fmath` is unchanged.** No new entries. Doc header gets a note
  pointing readers at the `f.*` builtins for the full surface.
- **`std:imath` loses one word.** `mod` is removed; `STDLIB_REFERENCE.md`
  reflects that callers use `i.modulo` directly.
- **Docs grow.** `STDLIB_REFERENCE.md` gets a "Float Math Builtins"
  section listing all 20 + 3 with their stack effects, alongside the
  existing "Float Arithmetic" / "Float Comparison" sections. The
  `## std:fmath` section gains a one-line pointer up to those
  builtins.
- **Seqlings ch. 28 unblocks.** Once builtins land, the chapter can
  reference `f.sqrt`, `f.sin`, `f.pi`, etc. directly without an
  `include std:fmath` — the include is only needed for composed
  words like `f.clamp`.
- **Issue #468 closes.** Comment notes the reframe from "expand
  stdlib" to "expand builtins" so anyone reading the closed issue
  understands why no stdlib commit appears.

## Checkpoints

1. `4.0 f.sqrt float->string io.write-line` prints `2`.
2. `1.0 f.exp float->string io.write-line` prints `2.718281828459045`
   (matches `f64::E`).
3. `f.pi float->string io.write-line` prints
   `3.141592653589793` and is callable with no `include`.
4. `0.5 f.round` returns `0.0` (banker's), `1.5 f.round` returns `2.0`
   — both ties-to-even. `2.5 f.round` returns `2.0`. Documented in
   the builtin doc string.
5. `0.0 -1.0 f.atan2 float->string io.write-line` prints `3.14159…` —
   confirms `( y x -- )` order.
6. `seqc lint` clean; `just ci` green.
7. `imath.seq` no longer contains `mod`; any callers updated.
8. seqlings `28-std-fmath` exercises compile and pass against the
   actual stdlib.
