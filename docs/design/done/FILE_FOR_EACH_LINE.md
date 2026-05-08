# `file.for-each-line` — drop the sentinel pair

Status: design · 2026-05-08 · issue [#459]

## Intent

`file.for-each-line+` has stack effect `( ..a String Quot -- ..a String Bool )`,
leaving `( "" true )` on success or `( "<errmsg>" false )` on failure. Every
call site has to `drop drop` (or branch). Forgetting that surfaces as an
`Occurs check` against the enclosing word, which is a bad first failure for
learners.

Reshape to `( ..a String Quot -- ..a Bool )` and rename to
`file.for-each-line` (no `+`). Same shape as `file.spit` / `file.append` /
`file.delete`. One value to drop or branch on. Keeps the success/failure
signal so a typo'd path still surfaces, just without the opaque error
string.

## Constraints

- **No callers in repo.** Verified by grep across `tests/`, `examples/`,
  `crates/compiler/stdlib/`. The only references are in docs and the
  builtin/codegen wiring itself. Out-of-tree callers (notably the
  seqlings file-I/O chapter) will need the rename + drop adjustment.
- **Don't widen scope.** Do not rework error reporting for the rest of
  `file.*`. Do not introduce an error-info getter. Do not invent a new
  variant builtin.
- **Out of scope:** any change to `file.slurp`/`file.size`/etc., the
  `(value Bool)` convention used by other fallible ops, or the
  ergonomics of the `+` suffix more broadly (after this change there
  are zero `+` builtins, so the suffix table row goes away too).

## Approach

One renamed builtin, one trimmed stack effect, plus a drive-by bug fix.

1. **`crates/runtime/src/file.rs`**
   - Rename `patch_seq_file_for_each_line_plus` →
     `patch_seq_file_for_each_line`. Update header doc, panic strings,
     and the in-line example.
   - Stop pushing the empty-string-on-success and the error-string-on-
     failure. Each of the three exit paths pushes only the `Bool`:
     - Open error (line 165-166): push `Value::Bool(false)` — also
       fixes a latent bug where this path pushed `Value::Int(0)` while
       the typechecker said `Bool`.
     - Mid-stream read error (line 222-223): push `Value::Bool(false)`.
     - Success (line 229-230): push `Value::Bool(true)`.
   - Update the `pub use` re-export at line 509 to match.

2. **`crates/runtime/src/lib.rs:296`** — rename the re-export alias.

3. **`crates/compiler/src/builtins/fs.rs`**
   - Replace the manual `Effect` block (lines 27-41) with the new
     signature: input `( ..a String Quot )`, output `( ..a Bool )`. The
     quotation effect stays `( ..a String -- ..a )`.
   - Rename the key in `add_signatures` and `add_docs` to
     `file.for-each-line`. Tighten the doc string to mention the
     success bool.

4. **`crates/compiler/src/ast/program.rs:52`** — rename in the
   well-known-builtin list.

5. **`crates/compiler/src/codegen/runtime/fs.rs:16,58`** — rename the
   LLVM declaration and the symbol map entry.

6. **Docs**
   - `docs/STDLIB_REFERENCE.md:84` — new row:
     `file.for-each-line | ( String [String --] -- Bool )`.
   - `docs/language-guide.md:881-910` — replace the example so the
     caller branches on the `Bool` directly (no leading `drop`).
   - `docs/language-guide.md:984` — delete the `+` suffix row from
     the table; this was the only example.
   - `docs/language-guide.md:253` — the line-ending-normalization
     paragraph still mentions `file.for-each-line+`; rename.

## Domain Events

- **Produced:** the word `file.for-each-line+` no longer exists. Out-of-
  tree callers get a `Word not defined` error at compile time, which is
  the loudest possible signal for a rename — they update name and drop
  the trailing `drop drop` at the same time.
- **Consumed:** none. No other word's semantics change.
- **Must follow:** seqlings exercise 20 (file I/O) needs the rename and
  the simplified call site. Tracked separately in that repo.

## Checkpoints

- [ ] `just ci` passes.
- [ ] Hand-craft a test seq file:
      ```
      "a\nb\nc\n" "/tmp/probe.txt" file.spit drop
      "/tmp/probe.txt" [ io.write-line ] file.for-each-line
      [ "done" io.write-line ] [ "fail" io.write-line ] if
      ```
      Compiles without `drop drop`. Prints `a`, `b`, `c`, `done`.
- [ ] Same with a missing path: prints `fail`, exits 0 (not panic, not
      silent). Confirms the open-error path pushes `Bool` not `Int`.
- [ ] `seqc build` an old fixture with `file.for-each-line+` to confirm
      the error message clearly names the missing word.
