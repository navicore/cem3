# Memory Management Design for cem3

## Current State (Phase 8)

### What We Have
- Linked-list stack nodes (`StackNode`: value ptr + next ptr)
- Heap-allocated variant fields (`Box<[Value]>`)
- Values: `Int`, `String`, `Variant`

### The Problem: Everything Leaks
```rust
// Every stack operation leaks the old node:
pub unsafe extern "C" fn dup(stack: Stack) -> Stack {
    let (rest, value) = pop(stack);  // Old node LEAKED
    push(push(rest, value.clone()), value)  // 2 new nodes allocated
}
```

**What we leak:**
1. Every stack node after each operation
2. Every `String` when dropped
3. Every `Variant` when dropped
4. The entire working stack as program runs

**Why this is okay for now:**
- Short-running programs complete before memory exhausted
- Allows us to focus on correctness (type system, concurrency)
- Simple implementation

**Why this is NOT okay long-term:**
- Long-running services will exhaust memory
- CSP strands that run forever will leak indefinitely
- Wastes resources

## Phase 9: Deterministic Memory Management

**Goal:** Zero-cost, deterministic memory management with no GC.

### Strategy 1: Stack Node Pooling (Forth Approach)

**Idea:** Pre-allocate pools of stack nodes, recycle them immediately.

```rust
// Global pool per strand
thread_local! {
    static NODE_POOL: RefCell<Vec<*mut StackNode>> = RefCell::new(Vec::new());
}

pub unsafe extern "C" fn dup(stack: Stack) -> Stack {
    let (rest, value) = pop(stack);
    free_node_to_pool(stack);  // Return node to pool immediately
    let node1 = alloc_from_pool();  // Reuse from pool
    let node2 = alloc_from_pool();
    // ... build new stack
}
```

**Benefits:**
- **Fast:** Pool allocation is ~10x faster than malloc
- **Deterministic:** No pauses, no tracing
- **Simple:** Fits concatenative model perfectly
- **Zero runtime overhead:** Just faster allocation

**Challenges:**
- Need careful tracking of when node is free
- Pool per strand (thread-local)

### Strategy 2: Arena Allocator per Strand

**Idea:** Each strand has an arena. When strand exits, free entire arena.

```rust
struct Strand {
    id: u64,
    arena: Arena,  // Bump allocator
    stack: Stack,
}

impl Drop for Strand {
    fn drop(&mut self) {
        // Free entire arena at once - O(1)
        self.arena.free_all();
    }
}
```

**Benefits:**
- **Very fast allocation:** Bump pointer (add offset, done)
- **Simple cleanup:** Drop entire arena when strand exits
- **Fits CSP model:** Strands are isolated, short-lived workers

**Challenges:**
- Can't share values between strands easily
- Long-lived strands still accumulate memory
- Need channel values to be copied/cloned across arenas

### Hybrid Approach (Recommended)

Combine both strategies:

1. **Stack nodes:** Use global pool (recycled immediately)
2. **Values (String, Variant):** Use per-strand arena (freed when strand exits)

```rust
// Node pool is global per thread
thread_local! {
    static NODE_POOL: RefCell<Vec<*mut StackNode>> = RefCell::new(Vec::new());
}

// Each strand has value arena
struct Strand {
    id: u64,
    value_arena: Arena,  // For String, Variant allocations
    stack: Stack,        // Points to nodes from global pool
}
```

**Benefits:**
- Nodes recycled immediately (no leak)
- Values freed when strand completes
- Fast allocation for both
- Zero runtime overhead

## Phase 10+: Linear Types (Aspirational)

**Goal:** Use type system to **prove** when values can be freed at **compile time**.

### What Are Linear Types?

**NOT garbage collection!** This is static analysis like Rust's borrow checker.

**Linear type:** Value used **exactly once**
**Affine type:** Value used **at most once**

### How It Works (Compile-Time)

```cem
# Linear type - String consumed exactly once
: consume-string ( String -- )
  write_line ;  # Compiler knows String is consumed
  # Compiler inserts free() after write_line - STATICALLY

# Affine type - String preserved
: keep-string ( String -- String )
  dup write_line ;  # Can't free - caller expects it back
```

**Compiler analysis (at compile time):**
1. Track how many times each value is used
2. Insert `free()` calls where value provably dead
3. Reject programs that violate linearity
4. **Zero runtime cost** - all checks at compile time

### Example: `dup` is Special

```cem
# dup violates linearity - makes 2 copies!
: dup ( ..a T -- ..a T T )
  # Compiler error: T must be copyable
  # Only allow dup for Copy types (Int, Bool)
  # String, Variant would be compile error
```

This is similar to Rust:
```rust
let s = String::from("hello");
let s2 = s;  // Move - s is now invalid
let s3 = s;  // Compile error: s was moved
```

### Why This is NOT GC

| Garbage Collection | Linear Types |
|-------------------|--------------|
| **Runtime** tracing/marking | **Compile-time** proof |
| Non-deterministic pauses | Zero runtime overhead |
| Memory overhead (metadata) | No runtime tracking |
| Works with cycles | No cycles allowed |
| Examples: Java, Go | Examples: Rust, Clean |

**Linear types are compile-time borrow checking**, not runtime GC.

### Challenges

1. **Very complex to implement**
   - Requires full type inference
   - Requires region analysis
   - Rust took years to get this right

2. **User friction**
   - Users must understand linearity
   - Error messages can be cryptic
   - Forth programmers expect freedom

3. **Interaction with CSP**
   - Sending value over channel = move
   - Receiving value = acquire ownership
   - Need to prove channels don't create cycles

### Research Questions

1. Can we make linearity **optional** (like Rust's `unsafe`)?
2. Can we infer linearity without annotations?
3. How does this interact with quotations (closures)?

## Recommended Phasing

### Phase 9 (Next Priority)
**Goal:** Stop leaking memory with deterministic, zero-cost management.

**Implementation:**
1. Stack node pooling (global per thread)
2. Arena allocator per strand
3. Measure performance (should be faster than malloc)
4. Verify no leaks with Valgrind

**Success Criteria:**
- Long-running programs don't exhaust memory
- No performance regression (likely improvement)
- All existing tests pass

**Estimated Effort:** 2-3 sessions

### Phase 10+ (Aspirational)
**Goal:** Explore compile-time linear types for zero-cost safety.

**Prerequisites:**
- Phase 8.5 (type system) complete
- Phase 9 (memory management) working
- Research Rust's borrow checker, Linear Haskell

**Don't rush this.** Linear types are cutting-edge PL research. Get Phase 9 working first.

## Alignment Notes

**Current approach (`repr(Rust)`):** Fine for now.
- Natural alignment (8-byte for pointers on 64-bit)
- Optimal packing
- No ABI guarantees (we don't need them)

**When to use `repr(C)`:**
- Only if exposing ABI to C code
- Only if interop with C structs
- Not needed for internal runtime

**Current alignment checks:**
```rust
// runtime/src/scheduler.rs:137
debug_assert!(
    stack_ptr.is_null() || stack_addr.is_multiple_of(std::mem::align_of::<StackNode>()),
    "Stack pointer must be null or properly aligned"
);
```

This is good - validates alignment in debug builds.

## Bottom Line

**Phase 9 (Memory Management):**
- Deterministic, zero-cost
- No GC, no pauses, no overhead
- Forth-style pools + arena allocators
- **Do this next after Phase 8.5 complete**

**Phase 10+ (Linear Types):**
- **NOT garbage collection**
- Compile-time proof, zero runtime cost
- Like Rust's borrow checker
- Research/aspirational
- **Don't rush - get Phase 9 first**

**Rust's determinism is the model we're following.**
