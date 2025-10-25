# cem3 Roadmap: Building a Solid Concatenative Foundation

## Philosophy

**Foundation First:** Get the concatenative core bulletproof before adding advanced features.

**No Compromises:** If something doesn't feel clean, stop and redesign.

**Test Everything:** Each phase has extensive tests before moving to the next.

**Learn from cem2:** Reference cem2 for "what not to do" and working examples.

**Concurrency is Core:** cem3 will be non-blocking from the start, inheriting cem2's proven CSP model (spawn/send/receive) with May coroutines. This is a defining characteristic of the language.

## Phase 0: Project Setup

### Goals
- Clean project structure
- Separate Value from StackNode from day 1
- Runtime only, no compiler yet (we'll use a simple test harness)

### Tasks
- [ ] Create `cem3/` directory structure
- [ ] Set up Rust runtime crate with proper module structure
- [ ] Define core types: `Value`, `StackNode`, `Stack`
- [ ] Write basic test harness (can push/pop values, call operations)
- [ ] Document the invariants we're maintaining

### Success Criteria
✓ Can push Int values on stack
✓ Can pop values off stack
✓ Stack is null when empty
✓ No memory leaks in basic push/pop

---

## Phase 1: Basic Stack Operations

### Goals
- Implement fundamental stack operations
- Prove that operations maintain invariants
- Test that operations compose correctly

### Operations to Implement
1. `dup: ( A -- A A )` - Copy top value
2. `drop: ( A -- )` - Remove top value
3. `swap: ( A B -- B A )` - Exchange top two

### Tasks
- [ ] Implement `dup` - must deep-copy values
- [ ] Implement `drop` - must free node but not worry about value (Value will handle its own cleanup)
- [ ] Implement `swap` - must correctly update next pointers
- [ ] Write tests for each operation in isolation
- [ ] Write tests for compositions: `dup swap drop` etc.
- [ ] Add assertions to verify stack structure after each op

### Success Criteria
✓ Each operation works with Int values
✓ Operations compose: `dup swap drop` leaves original value
✓ No memory leaks (run with valgrind/sanitizers)
✓ Stack structure is valid after any sequence

---

## Phase 2: Extended Stack Operations

### Goals
- Add more fundamental operations
- Test complex compositions
- Ensure shuffle patterns work correctly

### Operations to Implement
1. `over: ( A B -- A B A )` - Copy second
2. `rot: ( A B C -- B C A )` - Rotate top three
3. `nip: ( A B -- B )` - Remove second
4. `tuck: ( A B -- B A B )` - Copy top below second

### Tasks
- [ ] Implement each operation
- [ ] Test with Int values
- [ ] Test complex patterns: `rot swap rot`
- [ ] Verify stack order matches expected at each step
- [ ] Test the exact shuffle pattern from list-reverse: `rot swap rot rot swap`

### Success Criteria
✓ All operations work correctly with Int values
✓ Complex shuffle patterns produce correct stack order
✓ Stack structure remains valid
✓ No memory corruption or leaks

### Validation Test
```rust
// Stack starts: [10, 20, 30]
// After rot swap rot rot swap: [30, 10, 20]
#[test]
fn test_list_reverse_shuffle() {
    let stack = Stack::new();
    stack = push(stack, Value::Int(10));
    stack = push(stack, Value::Int(20));
    stack = push(stack, Value::Int(30));

    stack = rot(stack);
    stack = swap(stack);
    stack = rot(stack);
    stack = rot(stack);
    stack = swap(stack);

    let (stack, top) = pop(stack);
    assert_eq!(top, Value::Int(20));
    let (stack, second) = pop(stack);
    assert_eq!(second, Value::Int(10));
    let (_stack, third) = pop(stack);
    assert_eq!(third, Value::Int(30));
}
```

---

## Phase 3: Advanced Combinators

### Goals
- Implement `pick` and `dip`
- These are critical for flexible stack manipulation
- Prove they work with any stack depth

### Operations to Implement
1. `pick: ( ... N -- ... A )` - Copy Nth element (0 = top)
2. `dip: ( A [B -- C] -- C A )` - Execute quotation with top hidden

### Tasks
- [ ] Implement `pick` - must traverse stack correctly
- [ ] Test `pick` at various depths: 0, 1, 2, 5, 10
- [ ] Implement `dip` - requires quotation support (simple for now)
- [ ] Test `dip` with simple quotations
- [ ] Verify `pick` and `dip` compose with other ops

### Success Criteria
✓ `pick` works for any depth N
✓ `dip` correctly hides/restores top value
✓ Can implement complex patterns using pick/dip
✓ Stack structure remains valid

### Note
At this point we can pause and consider: **Is the core solid enough?**
- Can we shuffle arbitrarily without issues?
- Are operations composable?
- Is ownership clear?

**Only proceed to variants if YES to all above.**

---

## Phase 4: Simple Values (String, Bool)

### Goals
- Add more value types
- Ensure deep copying works correctly
- Test that all operations work with all value types

### Tasks
- [ ] Add `Value::Bool`
- [ ] Add `Value::String` (heap-allocated)
- [ ] Implement `Clone` for values (deep copy strings)
- [ ] Test all stack operations with Bool
- [ ] Test all stack operations with String
- [ ] Test mixed-type stacks: `[Int, String, Bool]`

### Success Criteria
✓ All operations work with all value types
✓ Strings are deep-copied correctly (no double-free)
✓ No memory leaks with heap-allocated values

---

## Phase 5: Single-Field Variants

### Goals
- Add variant support
- Prove that variants are independent of stack structure
- Test extraction + operations

### Tasks
- [ ] Define `VariantData` with fields as `Box<[Value]>`
- [ ] Implement `make_variant` for single-field (Some/None)
- [ ] Implement `extract_variant` - pushes field values on stack
- [ ] Test: Create `Some(42)`, extract, verify value
- [ ] Test: Create `Some(42)`, shuffle stack, extract - should still work

### Success Criteria
✓ Can create single-field variants
✓ Can extract fields
✓ Fields are independent of stack structure
✓ Shuffling before extraction doesn't break anything

### Critical Test
```rust
#[test]
fn test_variant_with_shuffle() {
    let stack = Stack::new();
    stack = push(stack, Value::Int(10));
    stack = push(stack, Value::Int(42));

    // Create Some(42)
    stack = make_variant(stack, TAG_SOME, 1);

    // Shuffle with another value
    stack = push(stack, Value::Int(99));
    stack = swap(stack);
    stack = dup(stack);
    stack = rot(stack);

    // Extract - should still work
    let (stack, variant) = pop(stack);
    stack = extract_variant(&variant, stack);

    let (stack, extracted) = pop(stack);
    assert_eq!(extracted, Value::Int(42));
}
```

---

## Phase 6: Multi-Field Variants

### Goals
- Add support for multiple fields
- Test the exact pattern that broke cem2
- Prove the new design is solid

### Tasks
- [ ] Implement `make_variant` for 2+ fields
- [ ] Test: Create `Cons(10, Nil)`
- [ ] Test: Extract Cons fields
- [ ] **Critical:** Create Cons, do `rot swap rot rot swap`, extract - MUST WORK
- [ ] Implement basic list operations: `list-head`, `list-tail`

### Success Criteria
✓ Can create multi-field variants
✓ Fields are stored in array, not linked by next pointers
✓ Extraction works after any shuffle pattern
✓ No crashes, no corruption

### The Crucial Test (What Broke cem2)
```rust
#[test]
fn test_cons_with_list_reverse_shuffle() {
    let stack = Stack::new();

    // Create Cons(10, Nil)
    stack = push(stack, make_nil());
    stack = push(stack, Value::Int(10));
    stack = make_variant(stack, TAG_CONS, 2);

    // Simulate match extraction
    let (stack, cons) = pop(stack);
    stack = push(stack, make_nil()); // acc
    stack = extract_variant(&cons, stack);
    // Stack now: [acc, head, tail]

    // Do the exact shuffle from list-reverse-helper
    stack = rot(stack);
    stack = swap(stack);
    stack = rot(stack);
    stack = rot(stack);
    stack = swap(stack);
    // Stack now: [tail, head, acc]

    // Create new Cons(head, acc)
    // Reorder: [head, acc]
    stack = rot(stack); // [head, tail, acc]
    let (stack, _tail) = pop(stack); // drop tail
    stack = swap(stack); // [acc, head]

    stack = make_variant(stack, TAG_CONS, 2);

    // Extract again - MUST WORK
    let (stack, new_cons) = pop(stack);
    stack = extract_variant(&new_cons, stack);

    let (stack, head) = pop(stack);
    assert_eq!(head, Value::Int(10));
}
```

**This test MUST pass before moving forward.**

---

## Phase 7: List Operations

### Goals
- Implement list operations as proof of correct foundation
- These are real-world use cases
- If these work, the foundation is solid

### Operations to Implement
1. `list-reverse-helper` - The exact pattern that broke cem2
2. `list-map-helper` - Uses pick for stack manipulation
3. `list-filter`
4. `list-fold`

### Tasks
- [ ] Implement list-reverse using the shuffle pattern
- [ ] Test with single-element list
- [ ] Test with multi-element list
- [ ] Test with deep recursion (100+ elements)
- [ ] Implement list-map using pick
- [ ] Test list-map with shuffle patterns

### Success Criteria
✓ list-reverse works with any list length
✓ list-map works correctly
✓ No stack corruption
✓ No memory leaks even with deep recursion
✓ All operations compose cleanly

---

## Phase 8: Add Compiler (Basic)

### Goals
- Now that runtime is solid, add compiler
- Keep runtime clean, compiler emits to clean runtime API
- Minimal type checking for safety (stack underflow, branch compatibility)

### Tasks
- [x] Simple parser for basic constructs (word definitions, literals, calls)
- [x] Minimal type checker:
  - [x] Stack depth tracking
  - [x] Conditional branch stack effect validation
  - [x] Stack underflow detection
- [x] Codegen targeting clean runtime API (LLVM IR)
- [x] Basic control flow (if/else/then conditionals)
- [x] Comparison operators (=, <, >, <=, >=, <>)
- [x] Ensure codegen emits correct operations
- [x] Test compiled programs match hand-written runtime tests

### Success Criteria
✓ Can compile simple programs with conditionals
✓ Compiled code behaves identically to runtime tests
✓ Type checker catches stack underflow at compile time
✓ Type checker validates conditional branches have same stack effects

### What's Missing
The current type checker is **minimal** - it tracks stack *depth* but not actual *types*. This is sufficient for basic safety but lacks:
- Type inference for user-defined words
- Stack effect declarations
- Full type tracking (distinguishing Int vs String at compile time)
- Row polymorphism for extensible stack effects
- Quotation types
- Variant/ADT type checking

These are addressed in Phase 8.5 below.

---

## Phase 8.5: Type System Design & Implementation

### Goals
- Design and implement a full type system for concatenative programming
- Support row polymorphism and stack effect inference
- Provide clear type error messages
- Enable type-safe quotations and variants

### Background
Concatenative languages present unique type system challenges:
- **Stack effects as types**: Functions are typed by their stack transformations
- **Row polymorphism**: Operations work on stacks with unknown "rest" elements
- **Quotation types**: Code-as-data requires first-class function types
- **Inference**: Types should be inferred when possible, not always declared

cem2's type system attempted this but had issues. We can learn from both its successes and failures.

### Design Spike Tasks
- [ ] **Review cem2's type system**
  - What worked? (bidirectional type checking, basic inference)
  - What didn't? (complexity, error messages, row polymorphism implementation)
- [ ] **Survey concatenative type systems**
  - Factor's effect system
  - Kitten's row polymorphism approach
  - Cat's compositional type system
- [ ] **Design stack effect syntax**
  - How to declare: `( a b -- c )` vs `( Int Int -- Int )`
  - Row variables: `( ..a Int -- ..a Bool )`
  - Quotation types: `( ..a [ ..b -- ..c ] -- ..d )`
- [ ] **Choose type inference algorithm**
  - Bidirectional type checking (cem2 approach)
  - Hindley-Milner with extensions
  - Constraint-based inference
- [ ] **Design error messages**
  - Show expected vs actual stack effects
  - Highlight location of type errors
  - Suggest fixes when possible

### Implementation Tasks
- [ ] **Extend AST with type annotations**
  - Add stack effect declarations to word definitions
  - Add type annotations for literals
  - Support inline type assertions
- [ ] **Implement type inference**
  - Infer types for literals (Int, String, Bool)
  - Infer stack effects for built-in operations
  - Infer stack effects for user-defined words
  - Handle recursive definitions
- [ ] **Add row polymorphism**
  - Support stack effects with row variables
  - Unification with row types
  - Constraints for row variable scope
- [ ] **Add quotation types**
  - Type quotations as `[ Stack1 -- Stack2 ]`
  - Ensure quotations compose correctly
  - Handle closures and captured values
- [ ] **Add variant/ADT type checking**
  - Type variant construction
  - Type pattern matching with exhaustiveness checking
  - Ensure variant fields have correct types
- [ ] **Improve error messages**
  - Show full stack effect context
  - Use colors/formatting in CLI output
  - Provide "did you mean?" suggestions

### Testing Strategy
- [ ] Create comprehensive type system test suite:
  - [ ] Basic type inference tests
  - [ ] Row polymorphism tests
  - [ ] Quotation type tests
  - [ ] Variant type tests
  - [ ] Error message quality tests
- [ ] Port cem2's type system tests (where applicable)
- [ ] Test complex programs with nested quotations and variants
- [ ] Ensure type checking performance scales to large programs

### Success Criteria
✓ Stack effects are inferred for all user-defined words
✓ Row polymorphism works: `dup` has type `( ..a T -- ..a T T )`
✓ Quotations are properly typed and compose
✓ Variants are type-safe with exhaustive pattern matching
✓ Type errors are clear and actionable
✓ Type checking completes in reasonable time (< 1s for 1000 LOC)

### References
- cem2's type checker: `cem2/compiler/src/typechecker/`
- Factor's effect system: [factorcode.org](https://factorcode.org)
- Kitten language: [kittenlang.org](https://kittenlang.org)
- "Type Inference for Concatenative Languages" (research)

### Notes
This phase may take several iterations to get right. The type system is a **defining feature** of the language and deserves careful attention. Don't rush it - a poor type system is worse than no type system.

Consider splitting this into sub-phases:
1. Basic type inference (no polymorphism)
2. Row polymorphism
3. Quotation types
4. Variant types with exhaustiveness

---

## Phase 9: Memory Management (Deterministic, Zero-Cost)

### Goals
- Stop leaking memory with deterministic, zero-cost management
- **No garbage collection** - Forth-style pools + arena allocators
- Maintain Rust-like deterministic performance

### Background
Currently (Phase 8), every stack operation leaks memory:
- Stack nodes are allocated but never freed
- String/Variant values are never freed
- Works for short programs, fails for long-running services

**See:** `docs/MEMORY_MANAGEMENT_DESIGN.md` for full analysis.

### Strategy
**Hybrid approach combining:**
1. **Stack node pooling** (global per thread) - Forth approach
   - Pre-allocated pools of nodes
   - Recycle immediately after use
   - ~10x faster than malloc
2. **Arena allocator per strand** - CSP approach
   - Bump allocator for String/Variant values
   - Free entire arena when strand exits
   - Very fast allocation

### Tasks
- [ ] Implement global stack node pool (thread-local)
- [ ] Implement arena allocator
- [ ] Integrate pool into stack operations (dup, swap, etc.)
- [ ] Integrate arena into value allocations
- [ ] Update strand lifecycle to free arena on exit
- [ ] Run Valgrind to verify no leaks
- [ ] Benchmark: verify no performance regression (expect improvement)
- [ ] Test long-running programs

### Success Criteria
✓ Long-running programs don't exhaust memory
✓ Valgrind shows no leaks
✓ No performance regression (likely faster)
✓ All existing tests pass
✓ CSP strands can run indefinitely

### Estimated Effort
2-3 sessions

### Non-Goals
- **NOT implementing GC** - this is deterministic management
- **NOT implementing linear types yet** - that's Phase 11+

---

## Phase 10: Advanced Features

### Goals
- Add features that rely on solid foundation
- Quotations, pattern matching, etc.

### Tasks
- [ ] Quotations with full closure support
- [ ] Pattern matching with complex patterns
- [ ] String operations and formatting
- [ ] **String optimization** - Consider interning or static references (see `docs/STRING_INTERNING_DESIGN.md`)
- [ ] I/O enhancements
- [ ] Module system

---

## Phase 11+: Linear Types (Aspirational)

### Goals
- Use compile-time type system to prove when values can be freed
- **Zero runtime cost** - all analysis at compile time
- Like Rust's borrow checker, **NOT garbage collection**

### Background
Linear types enable compile-time memory safety without runtime overhead:
- **Linear type**: Value used exactly once
- **Affine type**: Value used at most once
- Compiler inserts `free()` calls where values provably dead
- Reject programs that violate linearity
- Zero runtime cost - all checks at compile time

**This is NOT GC:** No tracing, no pauses, no runtime overhead. Pure static analysis.

### Prerequisites
- Phase 8.5 (type system) complete
- Phase 9 (memory management) working
- Research Rust's borrow checker, Linear Haskell

### Challenges
- Very complex to implement (Rust took years)
- User friction (error messages, learning curve)
- Interaction with CSP (channel sends = moves)
- Quotations complicate ownership

### Research Questions
1. Can linearity be optional (like Rust's `unsafe`)?
2. Can we infer linearity without annotations?
3. How does this interact with quotations?

### Recommended Approach
**Don't rush this.** Get Phase 9 working first. Linear types are cutting-edge PL research.

**See:** `docs/MEMORY_MANAGEMENT_DESIGN.md` for detailed analysis.

---

## Completed Phases

### ✅ Phase 10: Concurrency with Strands (CSP Model)

**Status:** COMPLETE ✓

**Implementation:**
- Brought cem2's proven CSP model to cem3
- May coroutines for efficient concurrency
- Erlang-inspired strands with Go-style channels

### Background
cem2 successfully implemented a concurrency model using:
- **Strands**: Lightweight processes (like Erlang processes)
- **CSP Communication**: Go-style channels for send/receive
- **May Coroutines**: Green threads for efficient concurrency
- **Non-blocking I/O**: All I/O operations yield instead of blocking OS threads

This model worked well in cem2 and will be brought forward to cem3.

### Tasks
- [ ] Initialize May runtime in generated `main()` function
- [ ] Make all I/O operations non-blocking (yield to scheduler)
  - [ ] Update `write_line` to use async I/O
  - [ ] Update `read_line` to use async I/O
- [ ] Add scheduler infrastructure to runtime
- [ ] Implement core concurrency operations:
  - [ ] `spawn: ( [quotation] -- strand-id )` - Create new strand
  - [ ] `send: ( value channel -- )` - Send to channel
  - [ ] `receive: ( channel -- value )` - Receive from channel (blocks strand, not thread)
- [ ] Add channel creation and management
- [ ] Test basic strand spawning and communication
- [ ] Test complex patterns (many strands, message passing)
- [ ] Verify no OS thread blocking on I/O

### Success Criteria
✓ Can spawn multiple strands
✓ Strands communicate via channels
✓ I/O operations are non-blocking (use May's async primitives)
✓ Thousands of strands can run efficiently
✓ No OS thread starvation
✓ CSP patterns from cem2 port cleanly

### Architecture Notes
- LLVM-generated code calls runtime functions (already in place)
- Runtime functions are May-aware (yield appropriately)
- Each strand has its own stack pointer
- Stack-threading model works naturally with strands
- No changes needed to codegen - it's all in the runtime

### Example Usage
```cem
: worker ( channel -- )
  receive        # Block this strand until message arrives
  write_line     # Print the message
;

: main ( -- )
  make-channel   # Create a channel
  [ worker ] spawn  # Spawn worker strand with channel
  "Hello from main!" swap send  # Send message to worker
;
```

---

## Validation Throughout

At each phase, we MUST:
1. **Run all previous phase tests** - no regressions
2. **Check for memory leaks** - valgrind clean
3. **Verify invariants** - add runtime assertions in debug builds
4. **Document any compromises** - if we make one, write it down

## When to Consult cem2

- **Reference for "what not to do"** - look at how cem2 conflated cells and values
- **Working examples** - cem2's type checker, parser may be reusable
- **Test cases** - cem2's test programs can be ported to cem3

## Decision Points

### After Phase 3: Is the Core Solid?
If we can't shuffle arbitrarily without issues, **STOP and redesign**.

### After Phase 6: Do Variants Work?
If the crucial test fails, **STOP and reconsider design**.

### After Phase 7: Is the Foundation Ready?
If list operations work cleanly, we can confidently build anything on this foundation.

---

## Success Metrics for cem3

At the end, we should be able to say:
- ✓ Values are completely independent of stack structure
- ✓ Any stack operation can be combined with any other
- ✓ Variants work with arbitrary shuffle patterns
- ✓ No memory leaks, no crashes
- ✓ Clear ownership model throughout
- ✓ Easy to add new features without breaking core
- ✓ Non-blocking I/O with efficient strand-based concurrency (CSP model)
- ✓ Thousands of strands can run concurrently without OS thread starvation

## Timeline

No pressure, no rush. Each phase is "done when it's done."

**Estimated phases:**
- Phase 0-2: Foundation (basic ops) - Could be 1-2 sessions ✓ Complete
- Phase 3: Combinators - 1 session ✓ Complete
- Phase 4-5: Values & simple variants - 1 session ✓ Complete
- Phase 6: Multi-field variants - 1-2 sessions (this is the critical test) ✓ Complete
- Phase 7: List operations - 1 session ✓ Complete
- Phase 8: Basic compiler - 2-3 sessions ✓ Complete
  - Parser, minimal type checker, conditionals, comparisons
- **Phase 8.5: Full type system - 4-6 sessions** (Design spike + implementation)
  - Design: 1-2 sessions (research, design decisions)
  - Basic inference: 1 session
  - Row polymorphism: 1-2 sessions
  - Quotation & variant types: 1-2 sessions
- Phase 9: Advanced features - TBD
- Phase 10: Concurrency/Strands - 2-3 sessions

**Total to prove foundation:** ~6-8 sessions to get to validated list operations. ✓ **DONE**

**Current status:** Phase 8 complete (basic compiler with minimal type checking)

**Next major milestone:** Phase 8.5 (full type system) OR Phase 10 (concurrency)
- Type system is important for language quality and developer experience
- Concurrency is important for the non-blocking I/O vision
- Both are foundational - prioritize based on immediate needs

**Recommendation:** Consider tackling Phase 10 (concurrency) next while type system design percolates. The minimal type checker is sufficient for basic programs, and the runtime foundation for strands is proven in cem2. Full type system can follow once concurrency is working.
