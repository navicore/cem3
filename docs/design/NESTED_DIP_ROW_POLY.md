---
name: Row-poly inference through quotation bodies
description: Fix the root cause behind issue #471 — make `StackType::pop` on an unconstrained row variable introduce a fresh type var and a fresh row var, so nested combinators (dip-in-dip, keep-in-dip, etc.) type-check without paper-over builtins.
type: design
---

# Row-Poly Inference Through Quotation Bodies

Status: design · issue [#471] · 2026-05-10

## Intent

Seq advertises "row-polymorphic stack effects" as a foundational property
of the type system. Today that property only holds at the top-level word
boundary; inside a quotation body, popping from the body's incoming row
variable is a hard error, which is why nested `dip` fails. This is a
correctness hole in the type system, not a missing convenience word.

Close the hole at the source: when the typechecker pops from an
unconstrained row variable, introduce a fresh type variable for the
popped value and a fresh row variable for the rest. The nested-`dip`
idiom — and every analogous pattern across `keep`, `bi`, `if`, and any
combinator we add later — type-checks because row polymorphism actually
behaves polymorphically inside quotation bodies.

This is the principled fix. We are not shipping a `2dip` / `3dip`
workaround. Either inference can be made correct or this issue stays
open with a documented reason.

## Constraints

- **Correctness is the bar.** Programs that fail today must continue to
  fail with crisp messages where the failure is a real type error. The
  new rule must not silently accept programs that have no sound type at
  runtime.
- **No language-level breaking changes.** The signatures of `dip`,
  `keep`, `bi`, `if` don't change. The fix is purely a relaxation of
  inference — programs that type-check today still type-check, and
  produce the same inferred effect for top-level words. No `.seq`
  source needs editing; no stdlib or examples migration.
- **No runtime or codegen impact.** This is a typechecker change.
  `patch_seq_dip` / `patch_seq_keep` / `patch_seq_bi` / `patch_seq_if`
  in `crates/runtime/src/combinators.rs` are unchanged. Codegen paths
  are unchanged.
- **No compile-time performance regression.** The new rule does one
  extra unification step per pop-on-row-var; that must not turn into
  pathological constraint growth in larger programs.
- **`just ci` stays green throughout.** Including stdlib, examples,
  integration tests, and `seqc lint`.
- **Off-ramps are explicit.** If the prototype reveals that the rule
  compromises soundness (a previously-rejected program now infers a
  nonsense type) or that error-message quality regresses meaningfully
  with no fix in sight, we stop. We do not fall back to a `2dip` /
  `3dip` builtin — issue #471 stays open and we re-open the inference
  question later.

## Approach

Two phases, with a hard decision point between them.

### Phase 1 — Prototype (time-boxed)

Goal: gain enough confidence in the rule and its ripple effects to
commit. Not a polished implementation.

1. **Locate the rule site.** `crates/compiler/src/typechecker/combinators.rs`
   line 53 (`infer_dip`'s preserved-value pop) is the immediate
   failure. The primitive is `StackType::pop` in
   `crates/compiler/src/types/` — decide whether the new rule lives on
   the primitive or in the combinator callers (likely the primitive,
   so `keep` / `bi` / `if` benefit without per-combinator changes).
2. **Implement the rule.** Popping from a row variable `..a` yields a
   fresh `Type::Var("T_n")` and a fresh row variable `..a_n`, with a
   substitution recording `..a := ..a_n T_n`. Threads through unification
   normally.
3. **Run the existing test suite.** `just ci`. Catalog every regression.
   Each one is either (a) a previously-broken case now correctly
   accepted — keep it, add a test, or (b) a previously-correct case
   now broken — that's a no-go signal.
4. **Trace four caveats explicitly:**
   - **`>aux` / `aux>` inside quotation bodies.** The aux stack is
     scope-local (`crates/compiler/src/typechecker/words.rs`
     `infer_to_aux` / `infer_from_aux`); confirm the new pop rule
     doesn't change aux-slot accounting.
   - **`Yield` effect propagation.** `dip` / `keep` / `bi` reject
     yield-bearing quotations today. Make sure that check still fires
     when the quotation is a nested combinator call.
   - **Closure capture analysis.** Auto-capture
     (`crates/compiler/src/capture_analysis.rs`) depends on inferred
     quotation effects; verify a nested-`dip` quotation's capture
     count is right.
   - **Error-message quality.** A real stack underflow at the top
     level (e.g., `dip` with no value below it) must still produce a
     crisp "expected a value below the quotation" — not a confusing
     row-variable mismatch downstream.
5. **Write the issue-#471 cases as runtime tests** and confirm they
   produce the expected stack (`100 5 3 → 150 5 3`, etc.), not just
   that they type-check.

### Decision Point — Go / No-Go

**Go** if all of:

- `just ci` green with the new rule.
- The four caveats above are clean or have small, understood fixes.
- The issue-#471 runtime tests produce correct stacks.
- A short list (≤5) of new typechecker tests exists covering nested
  `dip`/`keep`/`bi` permutations and explicit row annotations.

**No-go** if any of:

- A previously-passing program regresses and the cause isn't a small
  local fix.
- Inference accepts a program whose runtime behavior is unsound.
- Error messages on genuine underflows degrade with no clear remedy.
- Compile-time performance regresses meaningfully on the existing
  test corpus.

Document the prototype result in this doc before deciding. If no-go,
record specifically *why* — that's the artifact that future work
needs.

### Phase 2 — Full Implementation (only on go)

- Promote the prototype rule to a clean implementation with
  documentation in `crates/compiler/src/types/` (or wherever the rule
  lands).
- Update `docs/TYPE_SYSTEM_GUIDE.md` to describe the row-variable pop
  rule explicitly — this is now part of the language's documented
  inference behavior.
- Add the typechecker tests catalogued in Phase 1.
- Add an integration-test program that exercises the issue cases
  end-to-end.
- Cross-reference: restore Seqlings chapter 37 exercise 04
  (`04-dip-deeper`) to its natural `[ [ q ] dip ] dip` shape after
  the fix lands.

## Domain Events

- **Produces:** `[ [ q ] dip ] dip` and analogous nested-combinator
  patterns become accepted by the typechecker. The documented
  row-polymorphism property of Seq's type system stops being a
  half-truth inside quotation bodies. Every future combinator gets
  this for free.
- **Consumes:** issue #471 closes (or stays open with a documented
  no-go reason). The Seqlings nested-dip exercise is restorable.
  Any Factor/Joy port that depended on the idiom unblocks.
- **What must follow:** `docs/TYPE_SYSTEM_GUIDE.md` updated;
  Seqlings ch. 37/04 reverted to the natural form; the prototype
  result section of this doc is filled in either way.

## Checkpoints

1. Phase 1 produces a working prototype branch and a written
   decision-point record in this doc.
2. On go: `just ci` green; new typechecker tests cover nested-`dip`,
   `keep`-in-`dip`, `dip`-in-`keep`, `bi`-in-`dip`, and yield-flag
   propagation; the issue-#471 programs run and produce the expected
   stacks; `seq-lsp` hover and completions behave unchanged for
   top-level usages.
3. On go: `docs/TYPE_SYSTEM_GUIDE.md` documents the row-variable pop
   rule.
4. On no-go: this doc records the specific failure mode that
   blocked the fix, so a future attempt starts informed.
