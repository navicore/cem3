# Concatenative Core Invariants

## Priority: Foundation First

We need to get the concatenative core operations rock-solid before supporting advanced features like pattern matching with multi-field variants.

## Stack Representation

The runtime stack is a singly-linked list of `StackCell`:
```rust
struct StackCell {
    cell_type: CellType,
    data: CellDataUnion,
    next: *mut StackCell,  // Points to rest of stack
}
```

## Core Invariants

These MUST hold after every stack operation:

### Invariant 1: Valid Linked List
**The stack pointer always points to either:**
- `null` (empty stack), OR
- A valid `StackCell` whose `next` pointer forms a valid linked list

### Invariant 2: Stack Order
**If the conceptual stack is `(A B C)` reading top-to-bottom, then:**
```
stack_ptr -> Cell(A, next: ptr_to_B)
ptr_to_B  -> Cell(B, next: ptr_to_C)
ptr_to_C  -> Cell(C, next: null or rest)
```

**Following `next` pointers from `stack_ptr` must traverse the cells in stack order.**

### Invariant 3: No Dangling Pointers
**After any operation returns a new stack pointer:**
- The old stack pointer's cells may be freed/reused
- The new stack pointer must not reference freed cells
- All `next` pointers in reachable cells must be valid

### Invariant 4: Deep vs Shallow Copying
**When copying cells (for dup, over, pick, etc.):**
- Heap-allocated data (strings, variants) must be deep-copied
- `next` pointers are NEVER copied - they're set by push/pop operations
- Copying a cell should create an independent cell with `next: null`

## Core Operations

### drop: ( A -- )
```rust
pub unsafe extern "C" fn drop(stack: *mut StackCell) -> *mut StackCell {
    let (rest, cell) = StackCell::pop(stack);
    drop(cell);  // Free the cell
    rest         // Return rest
}
```
**Post-condition:** Returns pointer to rest, top cell is freed

### dup: ( A -- A A )
```rust
pub unsafe extern "C" fn dup(stack: *mut StackCell) -> *mut StackCell {
    let copy = deep_clone(&*stack);
    StackCell::push(stack, Box::new(copy))
}
```
**Post-condition:** Stack grows by 1, top cell is deep-copied with fresh next pointer

### swap: ( A B -- B A )
```rust
pub unsafe extern "C" fn swap(stack: *mut StackCell) -> *mut StackCell {
    let (rest, b) = StackCell::pop(stack);
    let (rest, a) = StackCell::pop(rest);
    let rest = StackCell::push(rest, b);  // b.next = rest
    StackCell::push(rest, a)               // a.next = &b, return &a
}
```
**Post-condition:** Top two cells swapped, next pointers correctly updated

### rot: ( A B C -- B C A )
```rust
pub unsafe extern "C" fn rot(stack: *mut StackCell) -> *mut StackCell {
    let (rest, c) = StackCell::pop(stack);
    let (rest, b) = StackCell::pop(rest);
    let (rest, a) = StackCell::pop(rest);
    let rest = StackCell::push(rest, b);  // b.next = rest
    let rest = StackCell::push(rest, c);  // c.next = &b
    StackCell::push(rest, a)               // a.next = &c, return &a
}
```
**Post-condition:** Top three cells rotated, next pointers form: a->c->b->rest

## The Current Bug

**Symptom:** After `match` extraction followed by `rot swap rot rot swap`, constructing a multi-field variant fails.

**Root Cause Hypothesis:**
1. Match extraction creates temporary cells with `next` pointers linking them
2. Stack shuffling operations (rot/swap) correctly update next pointers
3. BUT: Variant construction relies on following `next` pointers to find field values
4. After shuffling, the RUNTIME stack (following next pointers) doesn't match the CONCEPTUAL stack order

**The Real Issue:**
We're conflating two different linked lists:
- **Runtime stack:** The actual linked list of cells (following `next` pointers)
- **Conceptual stack:** The logical sequence of values

After match extraction creates cells `[head, tail]` linked as `head->tail->rest`, and we do complex shuffling, these lists diverge.

## Testing Strategy

### Phase 1: Core Operations (No Match)
Test each operation in isolation:
```cem
: test-drop ( -- ) 1 2 3 drop drop drop ;  # Should succeed
: test-dup ( -- ) 42 dup + 84 = assert ;
: test-swap ( -- ) 1 2 swap 1 = assert drop 2 = assert ;
: test-rot ( -- ) 1 2 3 rot 1 = assert drop 3 = assert drop 2 = assert ;
```

### Phase 2: Combinations
Test complex shuffle patterns:
```cem
: test-shuffle ( -- )
  1 2 3 4 5
  rot swap rot rot swap
  # Verify final order is correct
  ...
;
```

### Phase 3: With Variants
Only after core is solid:
```cem
: test-variant-simple ( -- )
  1 2 Cons
  list-head 1 = assert ;

: test-variant-after-shuffle ( -- )
  1 2 3 rot swap Cons
  list-head ...
;
```

## Proposed Fix Strategy

### Option A: Fix Stack Operations (Preferred)
Ensure rot/swap/etc maintain Invariant 2 perfectly. Add runtime assertions.

### Option B: Fix Variant Construction
Instead of following `next` pointers, use `skip_n` to traverse from original stack pointer.

### Option C: Simplify Match (Temporary)
Remove multi-field pattern matching temporarily until core is bulletproof.

### Option D: Redesign Stack Representation
Separate "cell" from "stack node". Cells contain data, stack nodes just link them.

## Success Criteria

1. All core operations have formal invariants documented
2. Runtime assertions verify invariants (in debug builds)
3. Extensive test suite for all operation combinations
4. No operation violates the stack invariants
5. Multi-field variants work with ANY shuffle pattern

## Test Results

### ✅ Core Operations Verified (2025-10-20)

Created test: `tests/test-core-shuffle.cem`

**Result:** The exact shuffle pattern from `list-reverse-helper` (`rot swap rot rot swap`) works PERFECTLY with plain integers. Stack values end up exactly where expected:

```
Initial: 10 20 30
After full shuffle: 30 10 20  ✓
```

**Conclusion:** The concatenative core (rot, swap, drop, etc.) is **CORRECT**. The operations maintain proper stack invariants.

## The Real Problem

Since core operations work correctly, the bug must be in the **interaction between variants and the stack**. Specifically:

1. Match extraction creates temporary cells
2. These cells are linked with `next` pointers
3. Stack operations (rot/swap) correctly rearrange the runtime linked list
4. BUT: When we construct a variant using `skip_n(stack, 2)`, we're skipping in the CURRENT runtime stack
5. However, the variant construction code that copies field values was already done BEFORE the shuffle
6. The copied field cells still have STALE `next` pointers from before the shuffle

The issue is **when we copy cells for variant fields vs when we compute rest_stack**. There's a timing/ordering issue.

## Next Steps

Two possible fixes:

### Option A: Don't pre-copy field values
Instead of copying field values early and shuffling those copies, delay the copy until right before `push_variant`. This ensures we're always working with the current stack state.

### Option B: Simplify match extraction
Don't pre-link extracted cells. Just put them on the stack without internal linking, rely entirely on the stack pointer chain.

### Option C: Change variant construction
Don't rely on `skip_n` or following pointers. Explicitly pass the rest_stack as a separate parameter to variant construction.
