# Test Discovery: Tightening the `test-` Prefix Magic

## Intent

Issue #435: the test runner picks up *any* word whose name starts with
`test-` as a `( -- )` entry point. A user-defined helper like
`test-flag ( Int Int -- Bool )` then fails with a confusing
`stack underflow in 'main'` error. The `test-` prefix is too natural an
English prefix (predicates, validators, probes) to keep magic for *all*
words anywhere in the project.

Goal: stop the prefix being magic *outside* declared test files, and
stop it auto-promoting non-`( -- )` helpers to entry points *inside*
test files. Convention-based, no new syntax.

## Constraints

- All existing `tests/integration/src/test-*.seq` files keep working.
- The `test.init` / `test.finish` / `test.assert*` framework words are
  unchanged — this is about *discovery*, not the harness.
- No new annotations / decorators — the language doesn't have them and
  we are not adding them for this.
- The `seqlings` workflow (copy exercise to `test-<name>.seq` then run)
  keeps working under any tightening here.
- A user file with NO `test-*` words is not an error — `0 passed` is
  fine. But a *file* the user explicitly hands the runner that doesn't
  match the file pattern should be an explicit error, not silent zero.

## Audit (current state)

- File-level: `test_runner.rs:97` already requires
  `starts_with("test-") && ends_with(".seq")`. Directory descent already
  filters to `test-*.seq`. Issue #435's framing of option 3 ("restrict
  to `test-*.seq` files") is **already implemented at the file level**.
- Word-level: `test_runner.rs:127` filters words solely by
  `name.starts_with("test-")`, with no signature check. This is where
  #435's footgun lives.
- File handed explicitly to `seqc test` that doesn't match `test-*.seq`
  is silently dropped (no warning, no error). That's a separate UX
  problem worth fixing in the same change.

## Approach

Two small tightenings, one diagnostic:

1. **Signature filter at the word level.** Only words named `test-*`
   *and* with stack effect `( -- )` are discovered as test entry points.
   `test-flag ( Int Int -- Bool )` is no longer picked up. Keep the
   existing name filter; just AND in the effect check.
2. **Explicit error on non-matching file path.** If the user runs
   `seqc test foo.seq` and `foo.seq` does not match `test-*.seq`, error
   with a clear message ("Test files must be named `test-*.seq`. Got:
   `foo.seq`."). Today this silently produces zero results.
3. **Better diagnostic if a non-`( -- )` `test-*` word is found.** Print
   a one-line note: "Skipping `test-flag` — discovered by name but its
   stack effect is `( Int Int -- Bool )`, not `( -- )`. Rename if it's
   a helper; fix the signature if it's a test."

This is option 3 from the issue *plus* a thin slice of option 2
(signature gate) at the word level. No annotations, no new syntax, no
file-pattern proliferation.

## Domain Events

- **Signature filter lands** → `test-flag`-style helpers stop being
  promoted; #435's stack-underflow error becomes impossible from the
  reported shape. Audit `tests/integration/src/test-*.seq` for any
  existing word named `test-*` whose effect is not `( -- )` — the audit
  must come back empty before this lands (or those words must be fixed,
  not the rule).
- **Explicit error on path mismatch** → any tooling that hands the
  runner non-`test-*.seq` paths breaks loudly. None known today; the
  `justfile` hands `tests/integration/src/` (a directory).
- **Skip diagnostic** → noise on stdout for any helper that happens to
  start with `test-`. Acceptable; it's information-dense and tied to a
  real shape.

## Checkpoints

- New unit test in `test_runner.rs`: a file containing
  `test-flag ( Int Int -- Bool )` plus `test-real ( -- )` discovers
  exactly one test (`test-real`) and prints a skip note for `test-flag`.
- New unit test: `seqc test foo.seq` (non-matching name) returns a
  non-zero exit and an error mentioning the `test-*.seq` requirement.
- `just test-integration` still passes unchanged — no existing
  `test-*` word in `tests/integration/src/` has a non-`( -- )`
  effect (verify before merging).
- The reproducer from issue #435 compiles and runs without the spurious
  `stack underflow in 'main'` error. The user's `my-actual-test` word
  is still not picked up (it doesn't start with `test-`) — that's a
  separate UX choice the issue itself flags as out of scope.
