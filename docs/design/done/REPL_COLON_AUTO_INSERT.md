# `:` as REPL command-mode entry from Normal

## Intent

In `seqr`, REPL commands begin with `:` (`:ir`, `:stack`, `:help`, `:q`, `:e`,
`:include …`). Today a Normal-mode user must press `i` first, then `:cmd`.
Pressing `:` directly in Normal mode does nothing — the key is unhandled and
falls through to no-op in `handle_normal`.

The conceptual question is what `:` *means* here. It is not a vim Insert
shortcut; it is a sigil for **REPL command mode** — a way to leave vim's
buffer-editing modes entirely and address the REPL itself. The design should
reflect that, not just shave a keystroke.

## Constraints

- Must not change behavior in Insert / Visual / OperatorPending / ReplaceChar
  modes. Especially: `r:` (replace char with `:`), `c:` / `d:` (operator
  cancel on unknown motion), and Insert-mode literal `:` must keep working.
- Must not break word definitions: `: foo ... ;` is parsed by checking for
  `": "` / `":\t"` at the start of submitted input. That detection runs at
  submit time on the main buffer; any solution must keep word-definitions
  flowing through the main buffer, not the command path.
- `vim-line` is intended as a generic vim-style line editor. REPL-specific
  semantics either go behind a knob or live in the REPL.
- Existing tests must continue to pass — notably `test_repl_command` (`i` →
  `:` → `q` → `Enter`).

## Approach — three options

### A. Auto-Insert shim (the original proposal)

Normal-mode `:` flips to Insert and inserts a literal `:`. Equivalent to
`i:` in one keystroke. Cheap (one match arm, behind a flag on
`VimLineEditor`). But it lies about what's happening: the `:` is just a
character in the main buffer, dispatched at submit time. Doesn't model the
mode shift the user is actually making.

### B. Different sigil, real command mode

Keep `:` meaning "literal colon, use `i` first" as in real vim, and pick a
new key — e.g. `;`, `,`, or `Space` — to open a dedicated command-line
overlay. Honest to vim. But it breaks every existing `:cmd` muscle memory,
the help text in `main.rs`, and `App::execute_input`'s submit-time `:`
parsing. Migration cost is high for a cosmetic gain.

### C. `Mode::CommandLine` — a real Ex-style command line, entered by `:`

From Normal, `:` opens a **separate** single-line command buffer (not the
main input). The main buffer is untouched. The command line shows a `:`
prompt, accepts text, and on `Enter` is dispatched to `handle_command`;
`Escape` cancels back to Normal with the main buffer intact.

This is the honest model of what the user already does: `:` *is* a mode
shift out of buffer editing into REPL addressing. Modeling it as a mode
matches the mental model and pays off in side benefits (command history
independent of expression history, command-line completion, no risk of `:`
keystrokes contaminating the main buffer).

Word definitions are unaffected: `: foo ... ;` is typed in Insert mode into
the main buffer. The Normal-mode `:` keystroke is the only thing rebound,
and it does not touch the main buffer at all.

Implementation shape:

- Add `Mode::CommandLine { buf: String, cursor: usize }` to `vim-line` (or
  hold the command buffer in the REPL and have `vim-line` expose a "command
  mode requested" event — same idea, different ownership). Behind a flag on
  `VimLineEditor` so other consumers are unaffected.
- Status line shows `:` prompt + buffer while in CommandLine mode.
- `LineEditor` API gains a way to surface "submitted command string" vs.
  "submitted main buffer" — either a new `Action::SubmitCommand(String)`
  variant or two `Submit` shapes. The REPL routes `SubmitCommand` to
  `handle_command` and continues to route `Submit` through `execute_input`.
- `App::execute_input`'s legacy `:`-at-start-of-buffer detection stays for
  backwards compatibility (users in Insert who type `:cmd<Enter>` still
  work), but the canonical path becomes the CommandLine mode.

## Recommendation

**Option C.** It matches the user's framing ("`:` shifts out of vim modes
into REPL command mode"), it's the only one of the three that doesn't lie
about what's happening, and it opens up clean places to put command history
and completion later. Cost is real — a new mode in `vim-line`, a status-line
render, and a new submit channel — but each piece is small and contained.

If we want a stepping stone: ship Option A first (one match arm, behind a
flag) to deliver the keystroke savings now, and treat Option C as a later
upgrade. Option A's exit cost is low because removing the shim later is a
one-line revert. **Do not** ship Option B — the migration cost outweighs
the benefit.

## Domain Events

For Option C:

- **Produced:** `Mode::Normal -> Mode::CommandLine` transition; on `Enter`,
  a `SubmitCommand(String)` action carrying the typed command (without the
  leading `:`); on `Escape`, `Mode::CommandLine -> Mode::Normal` with no
  side effects on the main buffer.
- **Consumed:** REPL routes `SubmitCommand` to `handle_command`. The
  existing submit-time `:`-prefix dispatch in `App::execute_input` continues
  to handle Insert-mode-typed commands for backwards compatibility.

## Downsides considered

- **New mode in a "vim-line" library.** Mitigation: behind an opt-in flag;
  default behavior unchanged for other consumers.
- **Two paths to the same command.** Insert-typed `:cmd<Enter>` and
  CommandLine-typed `cmd<Enter>` both reach `handle_command`. Acceptable —
  the legacy path is the compatibility seam, and `handle_command` is the
  single dispatch point.
- **Status-line / rendering work.** The command line needs a place to draw.
  The REPL already has a status row; extending it is straightforward.
- **Keystroke parity with vim.** A vim user pressing `:` gets a `:` prompt
  — that's *more* vim-like than Option A, not less. The divergence is that
  the commands behind the prompt are REPL commands, not Ex commands.

## Checkpoints

1. `vim-line` unit test (flag on): Normal-mode `:` enters
   `Mode::CommandLine` with empty buffer. Main buffer is unchanged. Status
   reports `:` prompt.
2. `vim-line` unit test: typing in CommandLine mode appends to the command
   buffer, not the main buffer. `Enter` produces a `SubmitCommand` action;
   `Escape` returns to Normal with no side effects.
3. `vim-line` unit test (flag off): Normal-mode `:` is a no-op — protects
   other consumers of the library.
4. `vim-line` unit test: `r:`, `c:`, `d:`, Insert-mode literal `:` all
   behave exactly as before.
5. REPL test: from Normal, `:` `q` `Enter` quits without entering Insert
   and without writing `:q` into the main buffer.
6. REPL test: legacy Insert path still works — `i` `:` `q` `Enter` quits.
7. Manual: from Normal, type `:ir`, `:ir stack`, `:stack`, `:help` via the
   command line and confirm each dispatches correctly. Then in Insert, type
   `: foo ( -- ) 1 ;` and confirm it still hits `try_definition` — i.e. the
   word-definition path is untouched by the new mode.
