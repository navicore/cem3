---
name: chan.close semantics — runtime sentinel via the WeaveMessage pattern
description: Issue #499 — make chan.close actually close, internally, by reusing the typed-sentinel pattern already used for weave channels. No language-level breaking change; no polling; no mutex on the hot path.
type: design
---

# `chan.close` Semantics — Sentinel via the WeaveMessage Pattern

Status: design · issue [#499] · 2026-05-10

**Supersedes the earlier "split Sender / Receiver" proposal in this
doc.** That direction was a breaking language change that pushed an
implementation bug into the user's mental model. This revision keeps
the bug — and the fix — entirely inside the runtime.

## Intent

Fix `chan.close` so it does what every Seq doc says it does — after
the channel is closed and drained, `chan.receive` returns
`( default false )` instead of blocking forever — **without changing
the user-facing channel API**. The user still writes `Channel` in
stack effects and union field types. `chan.make` still produces one
value. The fix is invisible to existing programs.

The pattern is already in this codebase: `WeaveChannelData` wraps its
underlying `may::mpmc` channel in a typed `WeaveMessage::{Value,
Done, Cancel}` enum so lifecycle signals travel alongside user data
through one cooperative-blocking channel. We borrow that pattern for
plain channels.

## Constraints

- **No language-visible breaking change.** `Type::Channel` stays.
  Builtin signatures `chan.make ( -- Channel )`, `chan.send
  ( T Channel -- Bool )`, `chan.receive ( Channel -- T Bool )`,
  `chan.close ( Channel -- )` are unchanged. No `.seq` source needs
  editing.
- **Preserve "zero-mutex" on the hot path.** A single
  `Arc<AtomicBool>` for the closed flag is acceptable (one atomic
  load per send, one atomic store at close, mirroring how the
  weave/sched code already uses atomics). No `Mutex` on `chan.send`
  or `chan.receive`.
- **No polling.** Blocked receivers wake when the close sentinel
  arrives through the same channel they were already blocked on —
  not by waking on a timer to check a flag.
- **`just ci` stays green** through the change — including stdlib,
  examples, integration tests, and `seqc lint`. Existing channel
  unit tests should continue to pass with no source edits beyond the
  Rust types in `crates/core` / `crates/runtime`.
- **Out of scope:** weave channels (already correct), generators,
  any user-visible API rework. Channel equality semantics
  (already identity-based) are unchanged.

## Approach

Mirror `WeaveMessage` for plain channels.

```rust
// crates/core/src/value.rs
pub enum ChannelMsg {
    Value(Value),
    Closed,             // lifecycle sentinel — never collides with user data
}

pub struct ChannelData {
    pub sender:   mpmc::Sender<ChannelMsg>,
    pub receiver: mpmc::Receiver<ChannelMsg>,
    pub closed:   Arc<AtomicBool>,
}
```

Op semantics (`crates/runtime/src/channel.rs`):

- **`chan.close`**: `closed.compare_exchange(false, true, …)`. If we
  won the race (first close), `sender.send(ChannelMsg::Closed)`. Then
  drop the handle. Idempotent on repeat close.
- **`chan.send`**: if `closed.load()` → push `Bool(false)`. Else
  `sender.send(ChannelMsg::Value(v))` and push `Bool(true)`.
- **`chan.receive`**: `recv()`. On `Ok(Value(v))` → `( v, true )`. On
  `Ok(Closed)` → re-send `Closed` (so the next blocked receiver also
  wakes — Go-style propagation through an MPMC channel of unknown
  consumer count) and return `( default, false )`. On `Err(_)` (all
  senders truly dropped via Arc refcount) → `( default, false )`.

The re-broadcast is the key mechanic that makes one close fan out to
N consumers without us knowing N. Each consumer wakes, re-sends, and
exits — the sentinel propagates lazily as each blocked receiver is
scheduled.

## Domain Events

- **Produces:** `chan.close` becomes load-bearing semantics —
  one strand calling close wakes every blocked receiver on the
  channel with `( default, false )`, matching the documented
  contract and the seqlings ch.24 curriculum. The runtime's
  `channel.rs:75-77` "equivalent to `drop`" comment is replaced.
  Pattern reuse: confirms the `WeaveMessage` typed-enum approach
  generalizes; no new abstraction introduced.
- **Consumes:** every prior-session edit on the breaking-change
  branch is reverted before this lands — see "Revert" below.
- **What must follow:** `docs/STDLIB_REFERENCE.md:215` and
  `docs/language-guide.md:1229–1241` pick up minor accuracy fixes
  ("Receive returns `( default false )` after every sender has
  dropped *or* `chan.close` has been called"). Seqlings ch.24
  unblocks; the issue #499 reporter's redesign plan is restored.

## Revert

The prior session implemented the asymmetric Sender/Receiver split
and migrated `.seq` sources to match. That work needs to be undone
before this approach lands. Files touched:

- `crates/core/`: `value.rs`, `lib.rs`, `stack.rs`, `son.rs`
- `crates/runtime/`: `lib.rs`, `channel.rs`, `channel/tests.rs`,
  `scheduler/tests.rs`, `serialize.rs`
- `crates/compiler/`: `types.rs`, `unification.rs`,
  `parser/type_parse.rs`, `builtins/macros.rs`,
  `builtins/concurrency.rs`, `ast/program.rs`,
  `typechecker/{freshen,validation,tests}.rs`
- `crates/lsp/src/completion.rs`, `crates/repl/src/ir/stack_effects.rs`
- `.seq` source migrations: `tests/integration/src/test-channel-safe.seq`,
  `tests/integration/src/test-net-dns.seq`,
  `examples/language/{unions,http_simple}.seq`,
  `examples/net/tcp/http-routing.seq`,
  `examples/paradigms/actor/actor_counters.seq`

Git operations are the user's; the agent will not run `git checkout`
or similar. After revert, this design doc remains the source of truth
for the next implementation pass.

## Checkpoints

1. After revert + new implementation: both issue #499 reproducers
   compile and run to completion, with the third `chan.receive`
   returning `( default, false )`.
2. New runtime unit test: MPMC drain — multiple receivers blocked on
   the same channel all wake on a single `chan.close` (validates the
   re-broadcast propagation).
3. New runtime unit test: `chan.send` after `chan.close` returns
   `false` without panicking.
4. New integration test in `tests/integration/src/test-channel-*.seq`
   covering single-strand and cross-strand close-and-drain end-to-end.
5. `just ci` green. **Zero `.seq` source edits required** — that's
   the test of the "no breaking change" claim.
6. Runtime doc-comments at `channel.rs:75-77` rewritten to describe
   the real semantics; `language-guide.md` / `STDLIB_REFERENCE.md`
   given an accuracy pass.
