# Tagged Stack Codegen Design

## Overview

Replace heap-allocated linked-list stack with contiguous array of tagged 64-bit values.
Generate inline LLVM IR for primitive operations instead of FFI calls.

## Tagged Value Representation

```
64-bit tagged value:
┌─────────────────────────────────────────────────────────────────┐
│ Bit 0     │ Bits 1-63                                           │
├───────────┼─────────────────────────────────────────────────────┤
│ 1         │ 63-bit signed integer (value << 1)                  │
│ 0         │ Pointer to HeapObject (8-byte aligned)              │
└───────────┴─────────────────────────────────────────────────────┘
```

### Examples
- Integer `42` → `0x55` (42 << 1 | 1 = 85)
- Integer `-1` → `0xFFFFFFFFFFFFFFFF` (-1 << 1 | 1)
- Heap ptr → `0x00007f8a12340000` (aligned, low bit = 0)

### Type Checking
```llvm
; Is it an integer?
%is_int = trunc i64 %val to i1          ; check low bit

; Extract integer value
%int_val = ashr i64 %val, 1             ; arithmetic shift right

; Create tagged integer
%tagged = or i64 (shl i64 %val, 1), 1   ; (val << 1) | 1
```

## Stack Structure

```llvm
; Stack state passed through functions
%StackState = type {
    ptr,    ; base - pointer to stack array
    i64,    ; sp - current stack pointer (index)
    i64     ; capacity - stack size
}

; Or simpler: just pass sp as pointer, base/capacity in globals
```

## Codegen Examples

### Push Integer Literal

**Current (FFI call):**
```llvm
%new_stack = call ptr @patch_seq_push_int(ptr %stack, i64 42)
```

**New (inline):**
```llvm
; Push integer 42
%tagged = or i64 84, 1                           ; 42 << 1 | 1 = 85, precomputed
store i64 %tagged, ptr %sp                       ; *sp = tagged
%new_sp = getelementptr i64, ptr %sp, i64 1      ; sp++
```

### Integer Addition

**Current (FFI call):**
```llvm
%result = call ptr @patch_seq_add(ptr %stack)
; Internally: pop a, pop b, push (a+b) - ~6 operations + malloc/free
```

**New (inline):**
```llvm
; Integer add: ( a b -- a+b )
%sp1 = getelementptr i64, ptr %sp, i64 -1        ; sp--
%b = load i64, ptr %sp1                          ; b = *sp (tagged)
%sp2 = getelementptr i64, ptr %sp1, i64 -1       ; sp--
%a = load i64, ptr %sp2                          ; a = *(sp-1) (tagged)

; For tagged integers: (a|1) + (b|1) - 1 = (a+b)|1
; Since a = (val_a << 1 | 1) and b = (val_b << 1 | 1)
; a + b - 1 = (val_a << 1) + (val_b << 1) + 1 = ((val_a + val_b) << 1) | 1
%sum = add i64 %a, %b
%result = sub i64 %sum, 1                        ; adjust for double tag
store i64 %result, ptr %sp2                      ; store result
; sp = sp2 (one fewer element)
```

### Drop

**Current:**
```llvm
%result = call ptr @patch_seq_drop_op(ptr %stack)
; Internally: pop, free node to pool
```

**New:**
```llvm
; drop: ( a -- )
%new_sp = getelementptr i64, ptr %sp, i64 -1     ; sp--
; That's it! No deallocation needed for integers.
; For heap objects, may need to decrement refcount (see below)
```

### Dup

**Current:**
```llvm
%result = call ptr @patch_seq_dup(ptr %stack)
; Internally: peek top, malloc new node, push
```

**New:**
```llvm
; dup: ( a -- a a )
%top_ptr = getelementptr i64, ptr %sp, i64 -1    ; sp-1
%val = load i64, ptr %top_ptr                    ; read top
store i64 %val, ptr %sp                          ; *sp = val
%new_sp = getelementptr i64, ptr %sp, i64 1      ; sp++
; For heap objects, need to increment refcount
```

### Swap

**Current:**
```llvm
%result = call ptr @patch_seq_swap(ptr %stack)
; Internally: pop a, pop b, push a, push b - 4 mallocs/frees!
```

**New:**
```llvm
; swap: ( a b -- b a )
%ptr_b = getelementptr i64, ptr %sp, i64 -1      ; sp-1
%ptr_a = getelementptr i64, ptr %sp, i64 -2      ; sp-2
%a = load i64, ptr %ptr_a
%b = load i64, ptr %ptr_b
store i64 %b, ptr %ptr_a                         ; swap in place
store i64 %a, ptr %ptr_b
; sp unchanged - no allocation at all!
```

### Pick

**Current:**
```llvm
%stack1 = call ptr @patch_seq_push_int(ptr %stack, i64 3)
%result = call ptr @patch_seq_pick_op(ptr %stack1)
```

**New:**
```llvm
; 3 pick: copy element at depth 3 to top
%src = getelementptr i64, ptr %sp, i64 -4        ; sp-4 (0-indexed from top)
%val = load i64, ptr %src
store i64 %val, ptr %sp                          ; *sp = val
%new_sp = getelementptr i64, ptr %sp, i64 1      ; sp++
```

## Heap Objects

For non-integer values, we still need heap allocation:

```llvm
%HeapObject = type {
    i8,     ; type tag (1=Float, 2=Bool, 3=String, etc.)
    [7 x i8], ; padding for alignment
    [0 x i8]  ; variable-length payload
}

%FloatObject = type {
    i8,       ; tag = 1
    [7 x i8], ; padding
    double    ; the float value
}

%QuotationObject = type {
    i8,       ; tag = 6
    [7 x i8], ; padding
    ptr,      ; wrapper function pointer
    ptr       ; impl function pointer
}
```

### Creating a Float
```llvm
; Push float 3.14
%obj = call ptr @seq_alloc_float(double 3.14)    ; allocate HeapObject
store ptr %obj, ptr %sp                          ; push pointer (low bit = 0)
%new_sp = getelementptr i64, ptr %sp, i64 1
```

### Type Checking at Runtime
```llvm
; Check if top of stack is an integer
%val = load i64, ptr %sp_minus_1
%is_int = trunc i64 %val to i1                   ; low bit

br i1 %is_int, label %int_path, label %heap_path

int_path:
  %int_val = ashr i64 %val, 1                    ; extract integer
  ; ... do integer operation

heap_path:
  %ptr = inttoptr i64 %val to ptr
  %tag = load i8, ptr %ptr                       ; read type tag
  switch i8 %tag, label %error [
    i8 1, label %float_path
    i8 2, label %bool_path
    ; ...
  ]
```

## Reference Counting for Heap Objects

Heap objects need reference counting for memory safety:

```llvm
%HeapObject = type {
    i8,       ; type tag
    i8,       ; flags (e.g., is_static)
    i16,      ; reserved
    i32,      ; refcount (atomic)
    [0 x i8]  ; payload
}
```

### Dup with Refcount
```llvm
; dup: ( a -- a a )
%val = load i64, ptr %sp_minus_1
%is_int = trunc i64 %val to i1
br i1 %is_int, label %dup_int, label %dup_heap

dup_int:
  ; Just copy the value, no refcount
  store i64 %val, ptr %sp
  br label %dup_done

dup_heap:
  ; Increment refcount
  %ptr = inttoptr i64 %val to ptr
  %rc_ptr = getelementptr %HeapObject, ptr %ptr, i32 0, i32 3
  %old = atomicrmw add ptr %rc_ptr, i32 1 monotonic
  store i64 %val, ptr %sp
  br label %dup_done

dup_done:
  %new_sp = getelementptr i64, ptr %sp, i64 1
```

### Drop with Refcount
```llvm
; drop: ( a -- )
%val = load i64, ptr %sp_minus_1
%is_int = trunc i64 %val to i1
br i1 %is_int, label %drop_done, label %drop_heap

drop_heap:
  %ptr = inttoptr i64 %val to ptr
  %rc_ptr = getelementptr %HeapObject, ptr %ptr, i32 0, i32 3
  %old = atomicrmw sub ptr %rc_ptr, i32 1 acq_rel
  %should_free = icmp eq i32 %old, 1
  br i1 %should_free, label %free_obj, label %drop_done

free_obj:
  call void @seq_free_heap_object(ptr %ptr)
  br label %drop_done

drop_done:
  ; sp-- handled by caller
```

## Stack Overflow Checking

```llvm
; Before push, check capacity
%space = sub i64 %capacity, %sp_index
%has_space = icmp ugt i64 %space, 0
br i1 %has_space, label %push_ok, label %grow_stack

grow_stack:
  %new_base = call ptr @seq_grow_stack(ptr %base, i64 %capacity)
  ; update base and capacity
  br label %push_ok

push_ok:
  ; proceed with push
```

## Migration Path

### Phase 1: Add new stack infrastructure
- Add `StackState` type to codegen
- Add stack allocation/growth runtime functions
- Keep old linked-list stack working

### Phase 2: Inline integer operations
- `push_int`, `drop`, `dup`, `swap`, `add`, `subtract`, `multiply`, `divide`
- Generate inline LLVM IR for these
- Fall back to runtime for type errors

### Phase 3: Add heap object support
- Allocator for HeapObjects
- Reference counting
- Type-specific operations (string concat, etc.)

### Phase 4: Remove old stack infrastructure
- Remove linked-list StackNode
- Remove pool allocator
- Remove old FFI functions for primitives

## Expected Performance

| Operation | Current | New | Speedup |
|-----------|---------|-----|---------|
| push int | FFI + malloc | 2 instructions | ~50x |
| drop | FFI + free | 1 instruction | ~100x |
| add | 3 FFI calls | 5 instructions | ~30x |
| swap | 4 FFI + 4 malloc/free | 4 instructions | ~100x |

For skynet with ~1.1M FFI calls for stack ops, this could reduce to ~0 FFI calls for integer operations, giving potentially **10-20x speedup**.
