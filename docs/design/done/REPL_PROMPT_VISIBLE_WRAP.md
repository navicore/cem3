# REPL Input Prompt Scrolls Off Bottom on Wrap (Issue #491)

Status: done · 2026-05-18

## Intent

The TUI REPL (`seqr`) is supposed to keep the input prompt pinned just
above the bottom of the visible area: as history grows, content
scrolls up so the user can always see what they're typing. Bug #491
was reported as "REPL freezes after crypto.ed25519-keypair /
ed25519-sign" — only Ctrl-C responds. The user later isolated the
real trigger: it isn't crypto, and the REPL isn't frozen. Input is
still accepted; the **prompt has scrolled below the visible area**.
Crypto output just hits the trigger faster than typical input
because `stack.dump` emits long unbreakable hex strings.

The trigger is **any input that contains a single word too long to
fit in the remaining columns of its line**. Many short words wrap
fine. The same number of long words eventually pushes the input
prompt off the bottom edge.

## Constraints

- The fix must not change the visual layout of the REPL in the
  common case (short words, short lines).
- Must not change `ReplPane`'s public surface — `build_lines`,
  `HistoryEntry`, `ReplState` etc. stay as-is.
- Out of scope: redesigning the REPL into a split layout (history
  pane on top + always-visible input pane at the bottom). That's a
  cleaner architecture but a bigger change; not needed for #491.
- Out of scope: changes to the crypto runtime, despite being the
  reported trigger. The runtime is fine.

## Approach

The bug is in `ReplPane`'s `render` (`crates/repl/src/ui/repl_pane.rs`).
We computed `wrapped_height` as `Σ ceil(chars_per_line / width)` per
source line. That's wrong when a single word doesn't fit in the
remaining columns: ratatui's `Wrap { trim: false }` uses a
word-aware `WordWrapper` that pushes the long word to a **fresh
line first** and then hard-breaks it across columns — producing one
extra display row per such occurrence that our estimate missed.
Cumulative underestimate ⇒ `scroll = wrapped_height − visible_height`
too small ⇒ the bottom of `lines` (the input prompt) ends up below
`area.height`.

ratatui-widgets 0.3 exposes `Paragraph::line_count(width)` behind
the `unstable-rendered-line-info` feature. It runs the same
`WordWrapper` ratatui uses internally to render, so the count
matches the actual display height by construction. We enable the
feature on the ratatui workspace dep and use `line_count` in place
of the hand-rolled math.

Code change is one Cargo feature flag plus replacing the manual
`wrapped_height` calculation with `paragraph.line_count(area.width)`
in `ReplPane::render`.

## Domain Events

- **Fix lands → close #491.** Reference the issue in the commit.
- **Defensive hardening (incidental):** `crates/repl/src/run.rs` now
  sets `.stdin(Stdio::null())` on the child `Command`. This was
  added in pursuit of a wrong hypothesis (child stealing keystrokes
  from the REPL's raw-mode TTY) but kept as a hardening — children
  should never share the parent's raw-mode stdin regardless. Side
  effect: `io.read-line` called from inside the TUI REPL now gets
  immediate EOF instead of potentially eating keystrokes during the
  10s timeout window. Acceptable; the REPL was never a viable place
  to read user input through child programs anyway (the TUI owns
  the terminal).
- **Follow-up to consider, not file:** if ratatui drops or renames
  `line_count`, we revisit. The instability flag is a real coupling
  to upstream; the alternative is replicating ratatui's WordWrapper
  in-tree, which we don't want to maintain.

## Checkpoints

1. `cargo build -p seq-repl` clean; `cargo test -p seq-repl` green
  (87 tests pass). ✓
2. Manual repro of #491: in `seqr`, recall a `crypto.ed25519-keypair`
  line via `Esc k` and re-submit with `Enter` many times. The input
  prompt stays pinned above the bottom edge throughout — does not
  scroll off-screen. ✓ (user-confirmed 2026-05-18)
3. Same with a long-X-line spam (one long word filling most of a
  line). Prompt stays visible. ✓
4. Common case unaffected: short-word inputs wrap and scroll
  identically to before.
