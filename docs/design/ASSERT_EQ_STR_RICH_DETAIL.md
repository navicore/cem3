# Readable `test.assert-eq-str` Failure Detail

## Intent

When `test.assert-eq-str` fails on a multi-line or whitespace-significant
string, today's report is actively broken. Two problems compound:

1. **Truncation** (correctness bug). The test runner's
   `collect_failure_block` (`crates/compiler/src/test_runner.rs:618`)
   attaches detail lines to a failure only while they start with
   whitespace. The runtime currently emits the value bytes raw, so an
   `expected "a\nb", got "c\nd"` value embeds a literal newline mid-line.
   Collection halts at the first non-indented continuation, capturing
   only the prefix up to that newline and silently dropping the rest —
   including the entire `actual` value. The user observed exactly this:
   one value truncated, the other missing.
2. **Whitespace invisibility** (UX bug). Even when the report does fit
   on one line (no embedded newlines), trailing-newline / control-byte
   differences look identical when printed raw. The exact case just hit
   in Seqlings exercise 04: `"…gamma"` vs `"…gamma\n"` — visually the
   same.

The runtime already captures both values
(`crates/runtime/src/test.rs:373-377`); the failure is in rendering.
Render so the failure block survives the runner's whitespace-prefix
parser **and** so invisible differences become visible.

Target shape:

```
test-quote-lines ... FAILED
  at line 18: assertion failed: strings not equal
    expected (21 bytes): "> alpha\n> beta\n> gamma"
    actual   (22 bytes): "> alpha\n> beta\n> gamma\n"
    first differ at byte 21
```

## Constraints

- **Don't change Seq-level signatures.** `test.assert-eq-str` stays
  `( ..a String String -- ..a )`.
- **Don't break the Seqlings stdout parser.** The `test-NAME ... FAILED`
  marker line and the `... ok` line must stay byte-identical. Detail
  lines remain indented continuation lines under the marker, on stdout —
  same convention `ASSERT_FAILURE_DETAILS.md` established.
- **Don't change exit codes or pass/fail summary footer.**
- **Don't widen the failure surface.** Render escapes the same way for
  any string value (passing tests unaffected; only failure rendering
  changes). Numeric `assert-eq` keeps its current single-line format —
  it doesn't suffer from embedded-newline confusion.
- **Out of scope:** colour output, char-by-char diff highlighting,
  structured/JSON event stream, a new assertion DSL, snapshot testing.

## Approach

Two changes, both in `crates/runtime/src/test.rs`.

### 1. Escape control bytes when rendering string values

Replace the raw `format!("\"{}\"", s)` calls
(`patch_seq_test_assert_eq_str`, lines 375-376) with an escape pass that
emits `\n`, `\r`, `\t`, `\\`, `\"` as their two-char source forms and any
other control byte (< 0x20 or 0x7F) as `\xNN`. Rust's
`std::ascii::escape_default` over the byte slice handles this; we just
collect into a `String`. Non-ASCII bytes pass through unchanged so legible
UTF-8 stays legible.

This is the load-bearing change: with no raw newlines in the rendered
value, every detail line stays whitespace-prefixed end-to-end, so
`collect_failure_block` captures the whole report. It also kills the
"looks identical but isn't" failure mode in the same stroke.

### 2. Multi-line layout for string failures

In `patch_seq_test_finish` (line 187), branch on the failure source: when
the recorded `message` indicates a string-equality failure, format the
detail as:

```
  at line N: assertion failed: strings not equal
    expected (E bytes): "<escaped expected>"
    actual   (A bytes): "<escaped actual>"
    first differ at byte K
```

`first differ at byte K` only prints when the strings have a common
prefix and aren't identical (i.e. the trivial different-length-only case
still shows a useful index). Computing K is a single byte-iteration over
the two `String`s — no new dependency.

For the numeric `assert-eq` path, keep the existing one-line
`expected E, got A` format. The branch is by failure source, not a
universal rewrite.

`record_failure`'s shape doesn't need to change; the formatting decision
lives in `test.finish`. If we'd rather centralize it later, that's a
follow-up.

## Domain Events

- **Produced:** when a string assertion fails, the test runner output now
  carries enough information to diagnose whitespace and control-byte
  differences without re-running with extra prints.
- **Consumed:** Seqlings' stdout-grep parser already only keys on the
  `... ok` / `... FAILED` markers and forwards the rest verbatim — the
  richer detail flows through to the learner unchanged on its end.
- **Must follow:** none — leaf change. The numeric assert path, line
  numbers, the per-test failure cap (Phase 3 of `ASSERT_FAILURE_DETAILS`),
  and the summary footer are untouched.

## Checkpoints

1. **Targeted failing test.** Add an integration test that does
   `"a\nb" "a\nb\n" test.assert-eq-str`. The captured stdout must contain:
   `expected (3 bytes): "a\nb"`, `actual   (4 bytes): "a\nb\n"`, and a
   `first differ at byte 3` line. **Critically:** the test runner's
   `error_output` for that test must contain *both* values — confirms
   `collect_failure_block` is no longer truncating at an embedded
   newline. A unit test in `crates/compiler/src/test_runner/tests.rs`
   feeding synthetic stdout with an escaped multi-line failure is the
   right harness.
2. **Control-byte rendering.** Test with `"hi\x07"` vs `"hi"`; output
   must show `\x07` as the four-character escape, not a raw bell byte.
3. **Numeric assert untouched.** Existing tests that scrape
   `expected N, got M` for `assert-eq` continue to pass.
4. **Seqlings smoke.** Run the Seqlings exercise set against the new
   binary; the marker-line parser still correctly classifies pass/fail
   for every exercise. (The exercise that just exposed this — file
   for-each-line — should now point clearly at its trailing-newline
   issue without `xxd`.)
5. `just ci` green.
