# Clean Concatenative Language Design

## First Principles

### The Semantic Model (What Users Think About)

A concatenative language has:
1. **A stack of values**
2. **Operations that transform the stack**
3. **Values can be primitive (Int, Bool) or composite (Variant)**

Key insight: **The stack implementation (linked list) is an implementation detail.**

### The Implementation Model (What Actually Happens)

```rust
// WRONG (cem2):
struct StackCell {
    data: Data,
    next: *mut StackCell,  // Implementation detail leaked!
}
// Problem: Variants use next for field linking
// Problem: Match extraction uses next for temporary structure
// Problem: Stack shuffling changes next but variants expect it stable

// RIGHT:
struct Value {
    // Just data, no pointers
    // This is what the LANGUAGE talks about
}

struct StackNode {
    value: Value,
    next: *mut StackNode,  // Only for stack impl
}

struct Variant {
    tag: u32,
    fields: Box<[Value]>,  // Array, NOT linked list
}
```

## Core Invariants

### Invariant 1: Stack Operations Move Values
```cem
1 2 3 rot
# Semantic: Values 1,2,3 rearranged to 2,3,1
# Implementation: Can move StackNodes, but CONCEPTUALLY moving Values
```

### Invariant 2: Values Are Independent of Stack
```cem
: foo ( -- Int )
  42 ;

: main ( -- )
  foo  # Returns VALUE 42
  dup  # Duplicates VALUE, not stack cell
  +    # Consumes values, returns new value
;
```

When `foo` returns, it returns a VALUE. That value can be:
- Pushed on stack
- Stored in a variant
- Passed to a function
- Returned from a function

The VALUE is separate from any particular StackNode.

### Invariant 3: Variants Own Their Fields
```cem
Nil 42 swap Cons  # Creates Cons(42, Nil)
```

This creates a **new variant value** that OWNS its fields. The fields are:
- Copied from the stack (or moved, but ownership is clear)
- Stored in the variant's internal structure
- NOT linked via stack next pointers

When you match on it:
```cem
match
  Cons => [  # Extracts field VALUES
    # head and tail are VALUES pushed on stack
    # They're copies/moves from the variant
    # NOT references to variant internals
  ]
end
```

## What Went Wrong in cem2

### Problem 1: Variant Fields as Linked List
```rust
// cem2 does:
struct Variant {
    data: *mut StackCell,  // Points to first field
}
// field.next points to second field
// This mixes variant structure with stack structure!
```

**Should be:**
```rust
struct Variant {
    fields: Box<[Value]>,  // Independent array
}
```

### Problem 2: Match Extraction Creates Stack Cells
```rust
// cem2 does:
let head_cell = copy_cell(variant.data);
let tail_cell = copy_cell(variant.data.next);
head_cell.next = tail_cell;
tail_cell.next = rest_stack;
```

This creates stack cells with `next` pointers that represent variant field structure. Then when you shuffle, you're changing field structure!

**Should be:**
```rust
let head_value = variant.fields[0].clone();
let tail_value = variant.fields[1].clone();
push_value(stack, tail_value);
push_value(stack, head_value);
// Now they're just values on the stack
// next pointers only represent stack order
```

### Problem 3: Variant Construction from Stack
```rust
// cem2 does:
memcpy(field_cell, stack, 32);  // Copies next pointer!
memcpy(field_cell2, stack.next, 32);
field_cell.next = field_cell2;  // Tries to retrofit
```

This assumes `stack.next` points to the "next field" but after shuffling, that's not true!

**Should be:**
```rust
let field1 = extract_value(stack);  // Gets VALUE, not cell
let field2 = extract_value(stack);  // Gets VALUE, not cell
let variant = Variant {
    tag: tag,
    fields: Box::new([field1, field2]),  // Array of values
};
push_value(stack, Value::Variant(variant));
```

## Design for cem3

### Phase 1: Core Types

```rust
// Value: What the language talks about
enum Value {
    Int(i64),
    Bool(bool),
    String(*mut i8),
    Quotation(*mut fn),
    Variant(Box<VariantData>),
}

// VariantData: Composite values
struct VariantData {
    tag: u32,
    fields: Box<[Value]>,  // Owned array, NOT linked list
}

// StackNode: Implementation detail
struct StackNode {
    value: Value,          // Owns the value
    next: *mut StackNode,  // Only for stack structure
}

type Stack = *mut StackNode;
```

### Phase 2: Operations

```rust
// Push: Takes ownership of value
fn push(stack: Stack, value: Value) -> Stack {
    let node = Box::new(StackNode {
        value,
        next: stack,
    });
    Box::into_raw(node)
}

// Pop: Returns ownership of value
fn pop(stack: Stack) -> (Stack, Value) {
    let node = Box::from_raw(stack);
    (node.next, node.value)
}

// Dup: Clones the value
fn dup(stack: Stack) -> Stack {
    let value = (*stack).value.clone();
    push(stack, value)
}

// Swap: Moves values
fn swap(stack: Stack) -> Stack {
    let (rest, b) = pop(stack);
    let (rest, a) = pop(rest);
    let rest = push(rest, b);
    push(rest, a)
}
```

**Key insight:** Operations work on VALUES. StackNodes are just containers.

### Phase 3: Variants

```rust
// Construct variant
fn make_variant(stack: Stack, tag: u32, field_count: usize) -> Stack {
    let mut fields = Vec::new();
    let mut current_stack = stack;

    // Pop field values (not cells!)
    for _ in 0..field_count {
        let (rest, value) = pop(current_stack);
        fields.push(value);
        current_stack = rest;
    }

    let variant = Value::Variant(Box::new(VariantData {
        tag,
        fields: fields.into_boxed_slice(),
    }));

    push(current_stack, variant)
}

// Match/extract
fn extract_variant(variant: &VariantData, stack: Stack) -> Stack {
    let mut current_stack = stack;

    // Push field values (not cells!)
    for field in variant.fields.iter().rev() {
        current_stack = push(current_stack, field.clone());
    }

    current_stack
}
```

**Key insight:** Variants store VALUES in an array. No next pointers involved.

## Migration Path

### Option A: Refactor cem2 Incrementally
1. Add `Value` type
2. Wrap existing StackCell in StackNode
3. Change variant fields from linked list to array
4. Update all operations to work with Values

**Risk:** Touching everything, hard to validate correctness

### Option B: Start cem3 Clean
1. Implement core with clean design
2. Port working features one at a time
3. Compare outputs with cem2 tests
4. Keep cem2 as reference implementation

**Benefit:** Clean slate, can validate each feature

## Recommendation

**Start cem3.** Here's why:

1. **The foundation affects everything** - trying to refactor cem2 means touching the entire codebase
2. **We learned what NOT to do** - cem2 is valuable as a learning experience
3. **Clean implementation is faster** - less fighting with legacy decisions
4. **Validation is easier** - can run same test programs on both, compare
5. **No sunk cost fallacy** - cem2 has value as documentation of what we learned

## cem3 Roadmap

### Phase 1: Core (No Variants)
- Implement Value type
- Implement StackNode
- Implement basic ops: dup, drop, swap, over, rot
- Implement pick, dip
- Test extensively with shuffle patterns

### Phase 2: Simple Variants (Single Field)
- Add Variant type with array of fields
- Implement make_variant
- Implement extract_variant
- Test Some/None

### Phase 3: Multi-Field Variants
- Test Cons
- Implement list operations
- Test with shuffling

### Phase 4: Advanced Features
- Quotations
- Recursion
- Type checking
- Optimization

## Success Criteria

✓ Can shuffle stack arbitrarily without breaking anything
✓ Variant fields are independent of stack structure
✓ Clear ownership model (no leaks, no use-after-free)
✓ All operations compose cleanly
✓ No special cases for "variant cells" vs "stack cells"
