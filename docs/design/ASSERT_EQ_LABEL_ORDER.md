# `test.assert-eq` Label Order

Status: design · 2026-05-08 · issue [#460]

## Intent

`test.assert-eq` and `test.assert-eq-str` currently document and implement
the stack effect as `( expected actual -- )` — expected pushed first,
actual on top. Every test in this repo (and in seqlings) writes the
opposite: compute the actual value, push the expected literal on top,
then assert. The result is that *every* assertion failure prints its
labels swapped:

```
test-x ... FAILED
  at line 5: expected 3, got 99    # but 3 is computed, 99 is the literal we asserted
```

Flip the runtime convention to match how the language is actually used:
`( actual expected -- )`. Keep the user-visible failure shape
(`expected X, got Y`) and the function names. The values that print
become correct.

## Constraints

- **Don't change Seq-level signatures.** `test.assert-eq` stays
  `( ..a Int Int -- ..a )`. Compiler builtin entry in
  `crates/compiler/src/builtins/diagnostics.rs:27-28` is symmetric and
  needs no change.
- **Don't weaken existing tests.** Passing tests in `tests/`,
  `tests/integration/`, and `examples/` must keep passing — they will,
  because equality is symmetric. Only failure-message labels change.
- **No new framework surface.** No diff renderer, no message-builder
  word, no `( actual expected message -- )` overload. Just fix the
  labels.
- **Out of scope:** colour output, structured failure events, a
  user-facing `expect`/`should` DSL, changes to `test.assert` /
  `test.assert-not` (single-arg, unaffected).

## Approach

Two files, two-line edits each.

1. **`crates/runtime/src/test.rs`**
   - In `patch_seq_test_assert_eq` (line 316) and
     `patch_seq_test_assert_eq_str` (line 353), swap which pop the
     names bind to:
     ```rust
     let (stack, expected_val) = pop(stack);   // top
     let (stack, actual_val)   = pop(stack);   // below
     ```
   - Update both `Stack effect: ( expected actual -- )` doc comments
     to `Stack effect: ( actual expected -- )`.
   - The downstream `expected == actual` check, `record_failure(...)`
     argument order, and the `expected {}, got {}` template at line
     205 stay as-is. The labels print correctly because the bindings
     now reflect the prevailing convention.

2. **`docs/TESTING_GUIDE.md` and `docs/STDLIB_REFERENCE.md`**
   - Add a one-line note next to the `test.assert-eq` table entry
     stating the convention: "Push the actual (computed) value first,
     the expected (literal) value on top."
   - Existing examples in `TESTING_GUIDE.md:12,18,124,127,151,158,178`
     already follow this convention; no example rewrites needed.

The runtime unit tests in `crates/runtime/src/test/tests.rs` only
check pass/fail counts and don't pin label ordering, so no Rust test
changes.

## Domain Events

- **Produced:** failure detail lines now print labels that match
  every example, doc, and exercise — `expected <literal>, got <computed>`.
- **Consumed:** the seqlings test runner (and any external parser)
  reads the same `expected X, got Y` line shape; only the *values*
  bound to those labels change. Existing parsers keep working.
- **Must follow:** none. This is a leaf change — no other word's
  semantics, signature, or codegen is touched.

## Checkpoints

- [ ] `just test` and `just ci` pass — confirms passing tests still
      pass under the swap (symmetric equality).
- [ ] Hand-craft a deliberately failing assertion in a scratch test:
      `2 3 i.+ 99 test.assert-eq`. Run `seqc test`. Output must read
      `expected 99, got 5`, not `expected 5, got 99`.
- [ ] Same for `assert-eq-str`: `"hi" "bye" test.assert-eq-str` must
      print `expected "bye", got "hi"`.
- [ ] Grep `examples/`, `tests/`, `tests/integration/` for any
      assertion whose intent depended on the old `( expected actual -- )`
      ordering (i.e. a deliberately-failing test whose expected output
      string is checked). None found in initial scan; re-verify before
      landing.
- [ ] Spot-check seqlings against a build that includes the swap —
      every intentionally-failing exercise should now read sensibly.

## Breaking-change note

User-visible failure text changes when an assertion fails. Passing
tests are unaffected. Any out-of-tree test that wrote
`<expected> <actual> test.assert-eq` (matching the *old* documented
order) will see swapped labels post-swap; the assertion still
passes/fails on the same condition. Bundle with the next breaking
release.
