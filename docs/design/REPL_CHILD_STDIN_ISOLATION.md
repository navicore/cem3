# REPL Child stdin Isolation (Issue #491)

Status: design · 2026-05-18

## Intent

The TUI REPL (`seqr`) holds the controlling terminal in raw mode with
keyboard enhancement flags pushed (`crates/repl/src/main.rs:78-89`).
Each user expression is compiled to an executable and run in a child
process via `run_with_timeout` (`crates/repl/src/run.rs:29-87`). That
spawn currently pipes only stdout and stderr — **stdin is inherited**,
so the child and the REPL share fd 0 to the same raw-mode TTY.

For fast-running words the race window is tiny and nobody notices. For
heavier first-call init paths (the trigger in #491 is
`SigningKey::generate(&mut OsRng)` warming `getrandom` /
`ed25519-dalek` tables), the child lives long enough that keystrokes
the user types during the run land in the child's stdin buffer and are
silently discarded when the child exits. The REPL feels "stuck" —
keys do nothing until enough state has been lost that Ctrl-C is the
only escape.

The intent is to make child runs incapable of stealing input from the
REPL, full stop, regardless of how slow they are or what they do.

## Constraints

- Must not change the user-visible behavior of any program that
  doesn't read stdin. Output capture (success/failure/timeout
  classification) stays identical.
- Must not break `seqc build` + direct execution of the resulting
  binary outside the REPL — those still need a real inherited stdin
  for `io.read-line` use cases.
- Out of scope: making `io.read-line` *work* from inside the TUI
  REPL. It's already broken there (raw mode + alt screen means the
  user can't sanely type into a child), and fixing it requires its
  own design (suspend the TUI, hand the TTY to the child, restore on
  exit — like `:edit` does for `$EDITOR`).
- Out of scope: changing how `seq-lsp` or `$EDITOR` subprocesses
  handle stdin. Those already do the right thing.

## Approach

In `crates/repl/src/run.rs`, set `.stdin(Stdio::null())` on the
`Command` before `.spawn()`. One line. The child now has `/dev/null`
on fd 0 — no shared TTY, no race, no stolen keystrokes. Any program
that calls `io.read-line` from inside the REPL gets immediate EOF
instead of blocking on stolen input; that's a strict improvement on
the current behavior, where it would block until the 10s timeout
while eating keys.

The fix is intentionally trivial. The interesting work is the
follow-up audit (see Domain Events).

## Domain Events

- **PR for #491 lands → close #491.** Reference the issue in the
  commit; no separate doc-update needed since the language guide
  doesn't describe REPL spawn internals.
- **Fix lands → audit other subprocess spawns.** Sweep for
  `Command::new(...).spawn()` / `.status()` across the workspace and
  confirm each either pipes/nulls stdin or genuinely wants it
  inherited (e.g. `open_in_editor` in `main.rs:174`, which is
  correct because it disables raw mode first). Known candidates to
  verify: `run.rs` (this fix), `lsp_client.rs:68-72` (already
  correct), `main.rs::open_in_editor` (already correct).
- **Fix lands → file a follow-up issue for `io.read-line` in REPL.**
  Document the limitation. Decide whether to (a) accept "EOF inside
  REPL" as the contract, (b) suspend-and-restore the TUI around
  reads, or (c) emit a friendlier error. Probably (a) for now.

## Checkpoints

1. `cargo build -p seq-repl` clean; `cargo test -p seq-repl` green.
2. Manual repro of #491: in `seqr`, run `crypto.ed25519-keypair`
   followed by `crypto.ed25519-sign` and type continuously through
   both. Keystrokes typed during the child's run no longer affect
   REPL state after the child exits — the REPL stays responsive.
3. Sanity check unrelated paths: `42 dup` still works; long-running
   `i.pow`-style computations still complete and display correctly;
   `:edit` still hands the terminal to `$EDITOR` cleanly.
4. Quick smoke on `io.read-line` from inside `seqr` — expect prompt
   EOF (or a clear error), **not** a 10s hang. If that surfaces a
   bad error message, file the follow-up issue described above.
