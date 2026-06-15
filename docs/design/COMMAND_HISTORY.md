# Command History for vim-line

**Status: in-repo proof landed (2026-06-15);** patch-prolog `plgr`
migration pending in that repo. Splits "command history" into the two
concerns it actually is and gives each a home, so every app on `vim-line`
(seqr, patch-prolog's `plgr`, future tools) gets retrieval + search +
edit without re-implementing it — while `vim-line`'s core stays pure.

## Intent

`vim-line` today owns a *fragment* of history: it emits
`Action::HistoryPrev/HistoryNext` for `Up`/`Down`, leaving storage to the
host. But normal-mode `k`/`j` are bound to `move_up`/`move_down` (line
motions), so they emit no history intent — inconsistent with every
readline vi-mode (bash: `k`/`j` browse history). And the *store* (ring,
dedup, search, persistence) is reinvented per app (seqr has one; `plgr`
hand-rolls a `Vec<String>`). Both are wrong. Factor cleanly:

1. **Keymap → intent** is editor territory → finish it in `vim-line`.
2. **Intent → data** (the store) is stateful + I/O-adjacent → a separate,
   pure, reusable piece the host composes.

## Constraints

- **`vim-line` core stays host-agnostic and owns no text and does no
  I/O.** The store must not pull `std::fs` or a buffer into the core.
- **Additive only.** Existing `Action` variants and seqr's current
  behavior (arrows → history) must keep working unchanged.
- **Multi-line motion is preserved.** `vim-line` is multi-line aware;
  `k`/`j` must still move between lines when there *are* lines — history
  is the *boundary* fall-through, not a blanket rebind.
- **The store never touches the editor.** It returns a `&str`; the host
  applies it via `editor.set(...)`. Neither half knows the other's type.

## Approach

**Layer 1 — `vim-line` core (keymap):**
- Normal-mode `k` on the first line → `HistoryPrev`; `j` on the last line
  → `HistoryNext`. Off-boundary, they stay `move_up`/`move_down`. On a
  single line (the REPL case) both boundaries coincide, so it just *is*
  history nav.
- Add a history-search sub-mode mirroring the existing opt-in
  `Mode::CommandLine`: `/` (or `Ctrl-R`) enters it, keystrokes build a
  query in a decoupled buffer, and it emits search intents. New additive
  `Action`s, e.g. `HistorySearch(String)` (incremental), `HistoryAccept`,
  `HistoryCancel`; `n`/`N` repeat. Core stores nothing — it only says
  "the user wants to search for X / accept / cancel."

**Layer 2 — the history store (pure):**
- A ring of entries with: `push` (+ dedup of consecutive duplicates,
  bounded max size), `prev`/`next` cursor, substring/prefix `search`, and
  a **draft stash** (saves the in-progress line on first `prev`, restored
  when `next` walks back past the newest entry).
- **No I/O.** Persistence is the host's: `load(Vec<String>)` /
  `entries() -> &[String]` to dump. The host owns the file path.
- Home: `vim_line::history`, gated by a default-off `history` Cargo
  feature. Core stays I/O-free with the feature off; the store is
  co-located and discoverable with it on. (Considered a sibling
  `line-history` crate and rejected — every realistic consumer also
  consumes vim-line; pre-extracting designs for a hypothetical.)

**Layer 3 — migrate the consumers (both, same pass).** The point of a
shared store is proven only by deleting the duplicates:
- **seqr** (in this repo): remove its bespoke history implementation and
  back onto the store. Behavior stays the same to the user; the code path
  becomes the shared one. This is the in-repo proof and lands here.
- **`plgr`** (patch-prolog): drop its hand-rolled `Vec<String>`/`hist_pos`
  likewise — tracked in patch-prolog `docs/design/REPL_HISTORY.md`, gated
  on the `vim-line` publish.

Migrating both at once is the reusability test (cf. the shared builtin
vocabulary table): if the store needs per-app special-casing to fit seqr
*and* `plgr`, the abstraction is wrong and we fix it before publishing.

**Host glue (the pattern both consumers follow):** feed keys to
`vim-line`; route `History*` intents into the store; apply the store's
returned entry via `editor.set`. Recall-then-edit is then free (the
recalled text is just normal editable buffer). On submit,
`store.push(line)`.

## Domain Events

- **`k`/`j` at a line boundary (normal mode)** → `HistoryPrev/Next` →
  host calls `store.prev()/next()` → recalled `&str` → `editor.set`.
- **First `prev` from a non-empty line** → store stashes the draft → it
  returns when the user walks back down past the newest entry.
- **`/` then a query** → history-search sub-mode → `HistorySearch(q)` →
  host calls `store.search(q)` → preview/accept.
- **Submit** → `store.push(entry)` (consecutive-dup-collapsed).

## Checkpoints

1. In **both** seqr and `plgr`, normal-mode `k`/`j` browse history;
   arrows still do too.
2. With genuinely multi-line buffer content, `k`/`j` move *between lines*;
   only at the top/bottom do they reach history (no regression to
   multi-line motion). Covered by a vim-line unit test.
3. Draft stash: type `abc`, `k` `k`, then `j` `j` returns to `abc`.
4. `grep -rn 'std::fs\|std::io' crates/vim-line/src` over the **core**
   (history feature off) is empty — purity held.
5. Store unit tests: push/dedup/bounds, prev/next cursor, search, draft.
6. **seqr migrated:** its bespoke history code is deleted and it backs
   onto the shared store; to the user, behavior is unchanged (arrows +
   now `k`/`j` work). All current vim-line tests pass.
7. **Reusability proof:** seqr and `plgr` consume the *same* store with no
   per-app special-casing — if either needed a fork, the API changed
   before publish.

## Resolved decisions (2026-06-15)

- **Module, not crate.** `vim_line::history` behind a default-off
  `history` feature.
- **Cheap protocol now.** Define `HistorySearch(String)` /
  `HistoryAccept` / `HistoryCancel` in the `Action` vocabulary and a
  `store.search(&str)` API; no incremental search sub-mode in this
  pass. Apps that want richer search wire it up later without an
  `Action` rev.
- **Store ↔ editor only via the host.** Store returns `&str`; never
  calls into `vim-line`. Enforced by absence of a `vim-line` dep in
  the store module's imports.
