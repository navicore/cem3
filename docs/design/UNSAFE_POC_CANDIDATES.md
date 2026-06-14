# Unsafe Rust POC candidates

A learning-oriented design doc, inspired by reading the Rustonomicon and
the [`iddqd`](https://crates.io/crates/iddqd) crate. Not a commitment to
ship anything; the goal is to identify where `unsafe` could plausibly
*earn its keep* in Seq, primarily for education.

## Intent

`iddqd`'s pitch is: when a data structure needs **multiple coherent
indexes over the same logical entry**, the safe-Rust phrasing forces
either RC overhead, double-hashing on lookup, or duplicated keys. A
small, well-tested `unsafe` core can collapse those costs while keeping
the safe API at the boundary.

Map that pitch onto Seq and look for shapes that fit. If none fit
honestly, say so and stop.

## Candidates considered

The unification-bearing parts of Seq — `compiler/src/unification.rs`
plus the `HashMap<String, _>` side tables in `typechecker/state.rs` —
are where iddqd-style multi-indexing has the most obvious fit.
Hindley-Milner with row polymorphism is the engine; substitutions and
the name→entry tables around it are the data structures worth
examining.

1. **Unification substitution arena** *(strongest iddqd shape)*
   - Today: `Subst` is two `HashMap<String, _>` — one for type vars,
     one for row vars. `apply_type`/`apply_stack` walk these maps;
     `compose` allocates a fresh map and re-hashes. `TypeChecker`
     additionally keeps an `env: HashMap<String, Effect>` plus several
     `RefCell<HashMap<String, _>>` side tables, all keyed by interned
     names that today are `String`.
   - iddqd-shaped move: an arena of `TypeVar { id: u32, … }`,
     multi-indexed by (a) numeric id and (b) interned name slice
     (`&'arena str`). A single unsafe core grants `&'arena str` lookups
     and stable id→entry references; the safe wrapper preserves the
     current API.
   - Why it pays (potentially): unification is run per word, per
     program; `compose` is hot during inference of long quotation
     chains and combinators. Reducing string hashing + `String` clones
     is a real win, not a synthetic one.
   - Risk profile: pure compile-time data structure. A bug surfaces as
     a typecheck miscompile in test, not as UB in a user's program.
     Easy to fuzz with the existing unification test suite.

2. **TaggedStack** *(half-unsafe already; perfect nomicon walkthrough)*
   - `core/src/tagged_stack.rs` already uses raw `alloc`/`realloc` and
     `*mut StackValue`. It's the most performance-sensitive structure
     in the runtime, hit by every push/pop.
   - Candidate experiments: `NonNull<StackValue>` instead of `*mut`,
     `MaybeUninit` for the tail, explicit `Layout` plumbing, `Drop`
     hardening, geometric vs. amortised growth. Each one is a chapter
     of the nomicon made concrete.
   - Not really iddqd-shaped (one index, not many), but it is the best
     vehicle in this codebase for learning the nomicon's actual
     content (aliasing, provenance, drop guards) on code we already own.

3. **Symbol / type-var interner** *(small, pure, miri-friendly)*
   - Today Symbols and type-var names are `SeqString` /
     `String` — full reference-counted or owned heap strings for what
     are effectively interned identifiers.
   - An interner returning `Sym(NonZeroU32)` with a side table of
     `&'arena str` is textbook unsafe territory and complements
     candidate 1 directly.

## Recommendation

For the iddqd-shaped POC: **candidate 1 (unification substitution
arena)**, optionally with **candidate 3 (interner)** as a prerequisite.
That gives the multi-indexed-entry story honestly and stays inside the
compiler where a bug is a test failure, not a crash in user code.

For the nomicon-walkthrough POC: **candidate 2 (TaggedStack)** as a
separate, unrelated exercise. Different lessons; do not bundle.

## Constraints

These are the bars the POC must clear *to be considered for
graduation*, not a fence keeping it out of production forever. They're
the same bars any unsafe code in the runtime should be held to.

- **Tests stay honest.** Must not weaken or delete an existing test —
  per CLAUDE.md, if a test fails the unsafe path is wrong, not the
  test. The existing unification / stack test suites are the
  correctness backstop; the POC must add to them, never subtract.
- **Miri clean.** `cargo +nightly miri test` passes for the chosen
  crate, including the new tests. Non-negotiable for graduation.
- **Single-module unsafe core.** All `unsafe` lives in one module with
  a written invariants block at the top. Each `unsafe { … }` block
  carries a `// SAFETY:` comment tied to those invariants. This is
  *the* comment-worth-writing case from CLAUDE.md — removing it would
  confuse the next reader.
- **Safe wrapper, stable API.** The wrapper preserves today's
  `Subst::apply_*` / `TaggedStack::push` signatures. Callers don't
  learn that anything changed.
- **Bench-justified.** A criterion bench shows a real improvement on a
  representative workload (`Subst::compose` driven by stdlib
  typechecking; `TaggedStack::push/pop` driven by an inner-loop
  microbench). "Same speed but unsafe" is not graduation-worthy and
  should be reverted; "faster on benches but not on stdlib typecheck
  wall-clock" is a useful signal too.
- **Single-crate scope per POC.** Candidate 1 stays inside
  `seq-compiler`; candidate 2 stays inside `seq-core`. No FFI surface
  change, no `Value` layout change, no `SeqString` rework.
- **Graduation path is gradual.** Land first as a default-off
  feature flag so trunk stays on the safe path while the unsafe one
  bakes. Once it has soaked in CI + at least one benchmark cycle and
  cleared the bars above, flip the default. Once the default has held
  for a release without regressions, delete the safe path. "POC" and
  "production" are the endpoints of one road, not separate roads.

## Domain Events

None. Both candidates are internal data-structure rewrites. They
produce no events, consume no events, and have no observable behaviour
change beyond performance. Worth stating because in some past
discussions this has been the spot where scope quietly grew — if a
candidate starts producing user-visible events, it has stopped being a
data-structure rewrite and become a feature, and the design needs
revisiting.

## Checkpoints

Roughly in the order they need to clear:

1. Existing test suites pass with the feature off (default on landing).
2. Existing test suites pass with the feature on.
3. `cargo +nightly miri test` is clean for the affected crate.
4. Criterion bench shows a measurable improvement on a representative
   workload, not just a microbench.
5. Soak: at least one release cycle with the feature available but
   default-off, no reported regressions.
6. Flip the default to on; safe path stays available behind the flag
   for one more cycle as an escape hatch.
7. Delete the safe path. Graduation complete.

If checkpoint 4 fails after honest effort, the POC concludes "unsafe
did not pay here" and the branch is dropped — the *learning* still
graduated even if the code didn't.
