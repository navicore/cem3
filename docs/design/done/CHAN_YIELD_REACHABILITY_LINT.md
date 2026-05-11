# `chan.yield` Reachability Lint

Status: design · 2026-05-09

## Intent

`chan.yield` is a cooperative-scheduler hint: "I'm at a safe point, please
run another strand if one wants to." Its contract is unsatisfiable in a
program that never spawns. Calling it from code that is not transitively
reachable from a `strand.spawn` body is, by construction, a programming
error — there is no peer to yield to and no observable effect, so the call
is dead code masquerading as concurrency machinery.

Today the compiler accepts it silently. The cost shows up as confusion
(seqlings ch24/05 teaches `chan.yield` in a single-strand context where
the assertion has nothing to do with the primitive) and, worse, as latent
bugs in user code: a program that *intended* to spawn but doesn't, or
whose spawn was deleted in a refactor, looks correct because the yields
are still there. The reader sees cooperative-style code; the runtime sees
dead instructions.

This is the same shape of bug as the error-flag mistakes that the v3.0
lint addressed (PR #388): a contract-level invariant the type system
doesn't model, made checkable by a whole-program post-resolution pass.
Patch-seq has explicitly chosen lint-based safety over heavier effect
machinery; this fits that path.

## Constraints

- **Compile-time only.** No runtime check, no panic. The lint runs in
  the existing lint phase after typecheck and word resolution. If it
  fires, `seqc` rejects the program.
- **No new effect / no signature noise.** Do not give `chan.yield` a
  `Cooperative` side effect that propagates through quotations and word
  signatures. The type-level alternative was considered (mirrors how
  `Yield` is discharged by `strand.weave`) and explicitly rejected for
  the same reason patch-seq rejected generics: the cost in surface-area
  noise on every transitively-yielding word is not worth ~5% extra
  precision over a reachability lint.
- **No user opt-out flag.** This is a correctness lint, not a style
  preference. Behaves like the error-flag lint: on by default, error
  level (not warning), no `--allow` knob.
- **Library words are permitted.** A word like the fanout benchmark's
  `worker-loop` calls `chan.yield` and is correct *because* it is reached
  through `[ worker ] strand.spawn`. The lint must permit any
  `chan.yield` reachable from at least one spawned root, even if also
  reachable from `main` directly.
- **Out of scope:** `yield` (the generator primitive). Already handled
  correctly by the type system — its `SideEffect::Yield` cannot be
  discharged outside a `strand.weave`, so misuse is a type error today.
  Also out of scope: detecting `strand.weave-cancel` misuse, channel-leak
  patterns, or any other contract-level concurrency check. Those are
  candidates for the same lint *family*, not this PR.
- **Stdlib must be clean under the new lint.** Any `chan.yield` in
  `crates/compiler/stdlib/*.seq` must already sit under a spawn root
  (or the call is itself a bug to fix).

## Approach

A reachability pass over the resolved call graph, run alongside the
existing lints:

1. **Build the call graph** from the resolved program. Nodes are user
   words plus the relevant builtins (`strand.spawn`, `strand.weave`,
   `chan.yield`). Edges follow direct calls and quotation literals
   (a quotation passed to `strand.spawn` makes its body a child of the
   spawn site, not of the enclosing word).
2. **Compute the cooperative root set.** Every word reachable from the
   body of a quotation passed to `strand.spawn` is in the cooperative
   set. (Conservative: also include words reachable from `strand.weave`
   bodies, since those run in the same scheduling fabric and may
   legitimately yield. Cost is near-zero, prevents false positives in
   weave-heavy code.)
3. **Walk every `chan.yield` call site.** If the enclosing word is
   reachable from `main` but not in the cooperative set, emit a lint
   error pointing at the call site: *"`chan.yield` has no peer to yield
   to — this code path is not reachable from any `strand.spawn`."*
4. **Conservatism on quotations stored in data.** A quotation handed to
   a non-spawn combinator and later re-invoked via `call` is treated as
   running in its enclosing context, not spawned. This may produce
   false positives for unusual patterns; in practice every cooperative
   use we have follows the literal `[ ... ] strand.spawn` shape. If the
   false-positive rate matters in real code, escalate then; do not
   pre-engineer for it.

The pass shape is deliberately the same as the error-flag lint's
abstract-stack walk: reuse its traversal scaffolding rather than
inventing a parallel mechanism. Lives in `crates/compiler/src/lint/`.

## Domain events

- **Program compiles** → call graph is built; cooperative root set is
  computed; every `chan.yield` site is classified.
- **Lint fires** → `seqc build` exits non-zero with a diagnostic
  pointing at the offending call, naming the enclosing word, and
  noting that no `strand.spawn` reaches it. Same diagnostic shape as
  the existing lint family.
- **A word is moved under a spawn** → the lint clears with no source
  change at the call site; the fix is at the architectural level
  (introduce the spawn) where the bug actually lives.
- **Seqlings ch24/05 is updated** → the existing exercise either
  moves to ch25 (spawn) with two real strands, or is deleted. Either
  way, the curriculum stops teaching `chan.yield` in a context where
  it is now a compile error.
- **Future cooperative-contract lints** → land in the same module,
  reuse the same call-graph pass.

## Checkpoints

1. **Single-strand `chan.yield` errors.** A program of the form
   `: main ( -- Int ) chan.yield 0 ;` fails to compile with a
   reachability diagnostic. Smallest correctness signal.
2. **Library word under spawn passes.** The fanout benchmark
   (`benchmarks/fanout/seq.seq`) compiles unchanged: `worker-loop`'s
   `chan.yield` is reachable through `[ worker ] strand.spawn`.
3. **Library word *not* under spawn fails.** A copy of `worker-loop`
   called directly from `main` without a spawn fails. Confirms the
   lint is reachability-based, not call-site-based.
4. **Stdlib + integration suite green.** `just ci` passes after the
   lint lands; no `chan.yield` in `crates/compiler/stdlib/*.seq` or
   `tests/integration/` sits outside a cooperative root.
5. **Diagnostic quality.** The error names the enclosing word and
   the spawn-reachability gap, not just "lint failure at line N."
   Verified by reading the message on a hand-crafted offender.
6. **No `seqc` flag added.** The CLI surface is unchanged; the lint
   is on by default at error level. Confirmed by reading help output.

## Pedagogical follow-up (not blocking)

Once the lint lands, seqlings ch24/05 (`05-yield.seq`) becomes a
compile error. It should be deleted or rewritten as a ch25 exercise
where two spawned strands yield visibly to each other. Coordinate
with the seqlings repo before merging the lint, or expect a broken
curriculum step until the exercise is updated.
