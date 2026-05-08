# LSP Hover: Word Beats Quotation

## Intent

Hovering on an identifier inside a quotation should show that identifier's
signature, not the enclosing quotation's stack effect.

Today, hovering on `file.spit` inside `[ ... file.spit ... ]` shows the
quotation's overall `( a -- b Bool )` effect — useless when you're trying
to read the *word's* signature. The bug surfaced while debugging a stack
underflow that was caused by mis-remembering `file.spit`'s effect (no error
String pushed, unlike `file.slurp`); hover would have answered the question
in one keystroke had it not been shadowed by the quotation hover.

## Constraints

- Must not regress the existing "hover on a quotation shows its inferred
  effect" behaviour — that's still useful when the cursor is on the brackets
  themselves or on whitespace inside.
- Must keep the existing regression test for dotted builtins (`string.length`
  et al.) passing — `crates/lsp/src/main.rs:808`.
- Must not require new fields on `state.quotations` or a re-parse — this is
  ordering logic in the hover handler, nothing more.
- Out of scope: changing hover content (e.g. showing both the word *and* the
  enclosing quotation type). One-thing-at-a-time.

## Approach

Invert the precedence in `SeqLanguageServer::hover`
(`crates/lsp/src/main.rs:246`):

1. Try `lookup_word_hover(word, locals, included)` first.
2. If that returns `Some`, return it.
3. Only if there is no word at the cursor — or the word resolves to nothing
   — fall through to the quotation-span check and return the quotation's
   inferred effect.

The current handler already has both pieces of logic; only the order needs
to change. The whitespace / bracket case still gets the quotation hover
because `get_word_at_position` returns `None` there.

One subtlety: a word that exists in the source but isn't a recognized
local/included/builtin (e.g. an unresolved name) currently falls through to
`Ok(None)`. After the change it would *also* fall through and then hit the
quotation hover — arguably an improvement (you get *something*), but worth
confirming with the test below.

## Domain Events

- **Produced**: hover request on an identifier inside a quotation now
  resolves to the identifier's signature → editor tooltips become accurate
  for builtins, locals, and included words alike.
- **Consumed**: nothing new. Same `quotations` snapshot, same
  `local_words` / `included_words`, same `lookup_word_hover` return shape.
- **Follows**: completion / goto-definition already prioritize the word
  under the cursor; hover now matches that mental model. Consistent
  precedence across the three LSP features.

## Checkpoints

Add unit tests next to the existing `tests` module in
`crates/lsp/src/main.rs:800`:

1. **Builtin inside quotation** — given `: f ( -- ) [ "x" string.length drop ] call ;`,
   a hover on the column range covering `string.length` returns the
   builtin's signature, not the quotation effect.
2. **Local word inside quotation** — given a user word `helper` defined in
   the same file and called inside `[ helper ]`, hover on `helper` returns
   the local definition's hover, not the quotation effect.
3. **Bracket / whitespace fallback** — hover on the `[` or on whitespace
   between words inside a quotation still returns the quotation effect.
4. **Unchanged regression** — existing
   `dotted_builtin_extracted_as_single_word` still passes.

Manual check: open `tests/integration/src/test-file-safe.seq` (or any file
with `file.spit` / `file.slurp` inside `[ ... ]`), hover each builtin, and
confirm the tooltip shows the builtin signature. Repeat for a hover on the
opening `[` to confirm the quotation hover still works.

`just ci` should pass with no other changes.
