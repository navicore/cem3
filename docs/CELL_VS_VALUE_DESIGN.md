# Cell vs Value: A Design Proposal

## The Problem

We've conflated three distinct concepts:
1. **Stack linked list nodes** (implementation detail)
2. **Semantic values** (what users think about)
3. **Variant field containers** (internal to variant structure)

This conflation causes bugs when stack shuffling moves cells around, because variant construction assumes cell `next` pointers have specific meaning.

## The Root Cause

```rust
struct StackCell {
    next: *mut StackCell,  // PROBLEM: Used for multiple purposes!
}
```

The `next` pointer serves:
- Stack linking: points to rest of stack
- Variant field linking: points to next field
- Match extraction: links extracted fields together

These are **different responsibilities** and we're using the same pointer for all of them.

## Proposal: Separate Concerns

### Design A: Value + StackNode (Clean Separation)

```rust
// Pure data, no pointers
struct Value {
    cell_type: CellType,
    data: CellDataUnion,
}

// Stack implementation
struct StackNode {
    value: Value,           // Owns the value
    next: *mut StackNode,   // Only for stack structure
}

// Variant fields (heap-allocated array or linked list, but separate from stack)
struct VariantData {
    tag: u32,
    fields: Vec<Value>,  // Owned by variant
}
```

**Pros:**
- Clear separation of concerns
- Values can be copied without worrying about `next` pointers
- Stack operations only touch StackNodes
- Variant construction operates on Values

**Cons:**
- Bigger refactor
- More allocations (Value + StackNode instead of just StackCell)

### Design B: Reference Counting

```rust
struct StackCell {
    cell_type: CellType,
    data: CellDataUnion,
    next: *mut StackCell,
    ref_count: AtomicUsize,  // Track references
}
```

**When to increment:**
- Variant points to this cell
- Match extracts this cell

**When to decrement:**
- Variant is dropped
- Match scope exits

**Pros:**
- Conservative (better to leak than crash)
- Can track who owns what
- Gradual refactor

**Cons:**
- Overhead of ref counting
- Still mixing concerns

### Design C: Arena Allocation for Match Scopes

```rust
// Match scope gets an arena
struct MatchScope {
    arena: Arena,  // All extracted cells allocated here
}

// When match completes, drop entire arena
// Variant construction COPIES from arena cells, doesn't reuse them
```

**Pros:**
- Simple ownership model
- No leaks (arena freed when match completes)
- Variant gets fresh cells

**Cons:**
- Extra copies
- Need arena per match depth

## Recommended Approach: Hybrid

**Phase 1: Conservative (Immediate Fix)**
- Variant construction: COPY values from stack, create fresh cells
- Never reuse stack cells for variant fields
- Match extraction: Create cells in a way that's safe to copy

```rust
// Instead of:
memcpy(field_cell, stack_cell, 32);  // Copies next pointer!

// Do:
field_cell = alloc_cell();
field_cell.cell_type = stack_cell.cell_type;
field_cell.data = deep_copy(stack_cell.data);
field_cell.next = null;  // Fresh cell, no stale pointers
```

**Phase 2: Separate Value from StackNode**
- Refactor to Design A
- Clean separation enables more sophisticated memory management later

## The Key Insight

**Stack shuffling should only move VALUES, not cells.**

When we do `rot swap rot`, we're conceptually moving values around. The implementation might move cells (changing next pointers), but the SEMANTIC operation is value movement.

Variant construction should operate on VALUES, not cells. It shouldn't care about stack cell `next` pointers at all.

## Memory Management Philosophy

**Better to leak than crash:**
- If uncertain about ownership → don't free it
- Leak detection tools can find leaks in testing
- Crashes lose user data

**Conservative copying:**
- When in doubt, copy
- Trade performance for correctness
- Optimize later when semantics are solid

**Clear ownership:**
- Stack owns stack cells (freed when popped)
- Variants own their field data (freed when variant dropped)
- Match scopes own extracted cells (freed when match completes)
- No shared ownership → no confusion

## Action Items

1. **Immediate**: Make variant construction copy values, not reuse cells
2. **Short term**: Add assertions to catch ownership violations
3. **Long term**: Refactor to separate Value from StackNode
4. **Documentation**: Make ownership explicit in all codegen

## Questions to Resolve

1. Should variant fields be heap-allocated array or linked list?
2. If linked, should they use the SAME `next` pointer as stack, or separate?
3. How do we handle deep nesting (variant contains variant)?
4. Should we add ref counting, or is copying sufficient?
