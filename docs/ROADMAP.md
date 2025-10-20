# cem3 Roadmap: Building a Solid Concatenative Foundation

## Philosophy

**Foundation First:** Get the concatenative core bulletproof before adding advanced features.

**No Compromises:** If something doesn't feel clean, stop and redesign.

**Test Everything:** Each phase has extensive tests before moving to the next.

**Learn from cem2:** Reference cem2 for "what not to do" and working examples.

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

## Phase 8: Add Compiler

### Goals
- Now that runtime is solid, add compiler
- Keep runtime clean, compiler emits to clean runtime API

### Tasks
- [ ] Simple parser (reuse cem2's if suitable)
- [ ] Type checker (reuse cem2's if suitable)
- [ ] Codegen targeting clean runtime API
- [ ] Ensure codegen emits correct operations
- [ ] Test compiled programs match hand-written runtime tests

### Success Criteria
✓ Can compile simple programs
✓ Compiled code behaves identically to runtime tests
✓ Type checker catches errors

---

## Phase 9: Advanced Features

### Goals
- Add features that rely on solid foundation
- Quotations, pattern matching, etc.

### Tasks
- [ ] Quotations with full closure support
- [ ] Pattern matching with complex patterns
- [ ] String operations
- [ ] I/O
- [ ] Module system

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

## Timeline

No pressure, no rush. Each phase is "done when it's done."

**Estimated phases:**
- Phase 0-2: Foundation (basic ops) - Could be 1-2 sessions
- Phase 3: Combinators - 1 session
- Phase 4-5: Values & simple variants - 1 session
- Phase 6: Multi-field variants - 1-2 sessions (this is the critical test)
- Phase 7: List operations - 1 session
- Phase 8+: Compiler and beyond - TBD

**Total to prove foundation:** ~6-8 sessions to get to validated list operations.

If that feels right, we can proceed with confidence to build anything on top.
