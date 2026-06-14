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

- Not for production. Feature-gated and default-off. `just ci` keeps
  exercising the safe path.
- Must not weaken or delete an existing test — per CLAUDE.md, if a test
  fails, the unsafe path is wrong, not the test.
- Must pass `cargo +nightly miri test -p seq-compiler` for candidate 1
  and `-p seq-core` for candidate 2.
- Must come with a criterion bench showing the unsafe path is at least
  faster than safe. "Same speed but unsafe" is a regression.
- Stays inside one crate per POC; no FFI surface change.
- No public-API change. The safe wrapper preserves today's
  `Subst::apply_*` / `TaggedStack::push` signatures.
- Out of scope: rewriting the runtime allocator, touching `SeqString`'s
  refcount, changing `Value`'s layout, any "while we're in there"
  cleanup. POC means POC.

## Domain Events

None. Both candidates are internal data-structure rewrites. They
produce no events, consume no events, and have no observable behaviour
change beyond performance. Worth stating because in some past
discussions this has been the spot where scope quietly grew — if a
candidate starts producing user-visible events, it has stopped being a
POC and become a feature.

## Checkpoints

1. Existing test suites pass unchanged with feature off (default).
2. Existing test suites pass with feature on.
3. `cargo +nightly miri test` is clean for the chosen crate.
4. Criterion bench (new, small) shows a measurable improvement for
   `Subst::compose` (candidate 1) or `TaggedStack::push`/`pop`
   (candidate 2). If not, the POC concludes "unsafe did not pay" and
   the code is deleted, not merged behind a flag.
5. The unsafe core fits in a single module with a written invariants
   block at the top — the kind of comment that *is* worth writing,
   because removing it would confuse the next reader.
