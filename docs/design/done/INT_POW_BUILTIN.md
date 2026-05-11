# `i.pow` Integer Power Builtin

## Intent

`std:imath.seq:108` defines `pow` as a naive Seq-level recursive word
that calls itself `exp` times, multiplying via `i.multiply`. It is
slow (O(n), not O(log n)), not stack-safe for large exponents, and —
in v7.0, after the `f.*` builtin expansion lands — visually
inconsistent with `f.pow` being a single FFI to `f64::powf`.

The right shape is an `i.pow` builtin that mirrors `f.pow`'s
namespace placement, uses Rust's already-O(log n) `i64::pow`
internally, and surfaces the two cases the current stdlib quietly
ignores: **negative exponents** and **overflow**.

This earns its own design (separate from #468) because the semantics
of those two cases is the actual decision, not the implementation.

## Constraints

- **Breaking change is fine.** Same posture as the float-math change.
  No alias, no shim. The stdlib `pow` in `imath.seq` is deleted in
  the same commit; callers move to `i.pow`. v7.0 absorbs it.
- **Match the project's error idiom.** Integer arithmetic in this
  codebase already uses `(value Bool)` for partial functions
  (`i.modulo` returns `(Int Bool)` for division-by-zero, see
  `crates/compiler/src/builtins/arith.rs:19`). `i.pow` uses the same
  shape rather than panicking or saturating, so callers compose it
  the same way they compose `i.modulo`.
- **No new lint-bypass shortcut.** A non-negative literal exponent
  produces a `Bool` the caller must drop or branch on. The error-flag
  lint already covers this category; an `unchecked-i-pow` entry in
  `lints.toml` is added in the same change.
- **Overflow folds into the same Bool.** Don't introduce a separate
  overflow channel.
- **Out of scope:** `i.pow` for `Float` exponents (use `f.pow`),
  modular exponentiation (`i.pow-mod` is its own future word),
  big-integer arithmetic.

## Approach

### Signature

```
i.pow ( base exp -- result Bool )
```

- `base : Int`, `exp : Int`, `result : Int`, `Bool : true` on success.
- Underlying call: `i64::checked_pow(base, exp as u32)` when
  `exp >= 0` and `exp <= u32::MAX`, otherwise the failure path.

### Semantics

| Case | Result | Bool |
|------|--------|------|
| `base ^ exp` fits in i64, `exp >= 0` | computed value | `true` |
| `exp == 0` (any base, including 0) | `1` | `true` |
| `exp < 0` | `0` | `false` |
| `exp > u32::MAX` | `0` | `false` |
| Overflow (e.g. `2 63 i.pow`) | `0` | `false` |

`0^0 = 1` matches Rust, Python, JS, and the math convention used in
combinatorics. Documented in the doc string so it isn't a surprise.

### Cleanup in the same change

- Delete `imath.seq:108-121` (`: pow ( Int Int -- Int ) … ;`).
- Update any call site in `examples/`, `tests/`, `benchmarks/`, and
  seqlings that used the bare `pow` to call `i.pow` and handle the
  trailing `Bool`.
- Add lint:
  ```toml
  [[lint]]
  id = "unchecked-i-pow"
  pattern = "i.pow drop"
  message = "`i.pow` returns (Int Bool) - check the Bool for negative exponent or overflow"
  severity = "warning"
  ```
  matching the pattern at `lints.toml:240+` for TCP and `i.modulo`.

## Domain Events

- **One new builtin registered.** Signature in `builtins/arith.rs`,
  doc in same file, runtime shim `patch_seq_i_pow` in
  `runtime/src/arithmetic.rs` (or wherever `i.modulo` lives), wiring
  entry in `codegen/runtime/arith.rs`. ~25 lines total.
- **`std:imath` shrinks by one word.** `pow` removed; the
  `STDLIB_REFERENCE.md` `std:imath` section drops the row and the
  Builtins section grows one row.
- **Lint table grows by one entry.** `unchecked-i-pow` added to
  `lints.toml`; `error_flag_lint/state.rs` table gets a parallel entry
  if the analyzer covers this category programmatically.
- **MIGRATION_7_0.md gains a row.** `pow` (stdlib) → `i.pow`
  (builtin), error-handling now via trailing `Bool`.
- **Companion to #468.** This issue lands together with the float
  math expansion or immediately after; separating them in commits is
  fine, but they share the same migration doc and v7.0 release.

## Checkpoints

1. `2 10 i.pow` returns `(1024, true)`.
2. `0 0 i.pow` returns `(1, true)`. Documented.
3. `2 -1 i.pow` returns `(0, false)`.
4. `2 63 i.pow` returns `(0, false)` (overflow — `i64::MAX` is
   `2^63 - 1`).
5. `seqc lint` flags `2 5 i.pow drop` with the `unchecked-i-pow`
   warning.
6. `imath.seq` no longer defines `pow`; existing `pow` callers in
   examples, tests, benches, and seqlings updated.
7. `just ci` green.
8. `MIGRATION_7_0.md` documents the rename and the new error contract.
