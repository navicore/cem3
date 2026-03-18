# Tagged Pointer Values (40 → 8 bytes)

## Intent

Every value on the stack is 40 bytes (`5 x u64`). Most values only use
16 bytes (discriminant + payload); the rest is padding. This means every
`dup` copies 40 bytes, every list element occupies 40 bytes, and only
1.6 values fit per 64-byte cache line.

Shrinking to 8 bytes gives 5x better cache utilization, 5x less memory
bandwidth, and 5x smaller collections. This is the single change with
the largest impact across all benchmarks.

## Current Layout

```
StackValue (40 bytes):
┌────────┬────────┬────────┬────────┬────────┐
│ slot0  │ slot1  │ slot2  │ slot3  │ slot4  │
│ discr  │payload │payload │payload │payload │
└────────┴────────┴────────┴────────┴────────┘
```

What actually uses the 40 bytes:

| Type | Slots used | What's in them |
|------|-----------|----------------|
| Int | 0, 1 | discriminant, i64 value |
| Float | 0, 1 | discriminant, f64 bits |
| Bool | 0, 1 | discriminant, 0 or 1 |
| Quotation | 0, 1, 2 | discriminant, wrapper ptr, impl ptr |
| Closure | 0, 1, 2 | discriminant, fn_ptr, Box\<Arc\<[Value]\>\> ptr |
| WeaveCtx | 0, 1, 2 | discriminant, yield_chan ptr, resume_chan ptr |
| String | 0, 1, 2, 3, 4 | discriminant, ptr, len, capacity, global flag |
| Symbol | 0, 1, 2, 3, 4 | discriminant, ptr, len, capacity, global flag |
| Variant | 0, 1 | discriminant, Arc\<VariantData\> ptr |
| Map | 0, 1 | discriminant, Box\<HashMap\> ptr |
| Channel | 0, 1 | discriminant, Arc\<ChannelData\> ptr |

Only String and Symbol use all 5 slots. Everything else fits in 2-3.
Six types (Variant, Map, Channel, Closure, WeaveCtx, and effectively
Quotation) are already pointers to heap data.

## Target Layout: Tagged Pointer (8 bytes)

Use the low 3 bits of a 64-bit word for type tags (heap pointers are
8-byte aligned, so low 3 bits are always 0 for pointers):

```
63                              3  2  1  0
┌──────────────────────────────┬──┬──┬──┐
│          payload             │tag bits │
└──────────────────────────────┴──┴──┴──┘
```

### Encoding

| Tag (bits 2:0) | Type | Payload |
|---|---|---|
| `xx1` (odd) | **Int** | 63-bit signed integer (value << 1 \| 1) |
| `010` | **Heap pointer** | Pointer to HeapObject (clear low bits to deref) |
| `000` | **Special** | Subtypes: false=0, true=8, or heap pointer with tag=0 |
| `100` | **Float (option A)** | Index into float table, or use NaN-boxing instead |

**Int fast path**: 50% of the tag space is integers. Test with `val & 1`.
Extract with arithmetic shift: `val >> 1`. This makes integer arithmetic
a shift, an add, and a shift — no memory access.

**Alternative: NaN-boxing** (used by LuaJIT, JavaScriptCore):
- IEEE 754 doubles have a quiet NaN range: exponent=0x7FF, mantissa≠0
- Encode non-float values in the NaN mantissa (51 usable bits)
- Floats stored directly as 8-byte doubles
- Pros: float operations are zero-cost (no tagging). Cons: 51-bit integers
  (still enough for most use cases, fall back to heap BigInt if needed)

**Recommendation**: Start with the simpler tagged-pointer scheme. NaN-boxing
is an optimization on top if float performance matters. The tagged-pointer
scheme is easier to debug and the integer fast path is more important than
the float fast path for Seq's typical workloads.

### Heap Objects

Types that don't fit in 8 bytes get a heap-allocated header:

```rust
#[repr(C)]
struct HeapObject {
    tag: u8,          // String, Symbol, Quotation, Closure, Channel, WeaveCtx
    _pad: [u8; 3],
    refcount: u32,    // atomic, inline refcount (no Arc overhead)
    // ... type-specific payload follows
}
```

**String**: Heap header + ptr + len + capacity + global flag (same as today,
just not inline on the stack). This is the only type that gets slower for
the inline case. But strings are already the slow path (runtime FFI for
all string ops), so the extra indirection is noise.

**Quotation**: Heap header + wrapper ptr + impl ptr. Or: pack both function
pointers into a single 16-byte heap allocation. Quotations are created once
and called many times, so the allocation cost is amortized.

**Closure**: Heap header + fn_ptr + Arc<[Value]> env. Already heap-allocated
via Arc today.

**WeaveCtx**: Heap header + two Arc<WeaveChannelData> pointers. Already
heap-allocated.

### Inline Refcounting

Today, Variant/Channel/Closure use `Arc` which has its own refcount. With
tagged pointers, use an inline refcount in the HeapObject header. This
eliminates the double indirection (pointer → Arc → data) and the separate
Arc allocation.

For types that are currently `Arc<T>`, the HeapObject IS the T with a
refcount prefix. `dup` increments the refcount (atomic add). `drop`
decrements it and frees on zero.

For Int, Float, Bool: `dup` is just a register copy. `drop` is a no-op.
No tag check needed if the compiler knows the type (which it does — static
typing).

## Constraints

- **All existing tests pass** — this is a correctness-critical change.
- **LLVM IR type changes** — `%Value = type { i64, i64, i64, i64, i64 }`
  becomes `%Value = type i64`. Every `getelementptr %Value` changes.
- **Runtime FFI boundary** — Functions like `patch_seq_list_push` that take
  `Stack` (pointer to StackValue array) must agree on value size. Both
  Rust and LLVM IR must use 8 bytes.
- **SeqString layout changes** — Strings move from inline 4-slot to heap
  header + payload. The arena/global distinction stays but the storage
  format changes.
- **Interned symbols** — Currently use capacity=0 sentinel in inline slots.
  With heap objects, use a flag in the HeapObject header.
- **Virtual stack** — `VirtualValue::Int { ssa_var }` already tracks SSA
  values as `i64`. This stays the same. Spilling writes `i64` instead of
  40 bytes — strictly easier.

## Phases

### Phase 1: Core types (seq-core)

Change `StackValue` from `{ u64, u64, u64, u64, u64 }` to `u64`.

Files:
- `tagged_stack.rs` — StackValue becomes `u64`
- `value.rs` — Value enum may stay as-is (Rust-side representation) or
  become a tagged u64 wrapper
- `stack.rs` — `value_to_stack_value` and `stack_value_to_value` become
  tag/untag operations. `push`/`pop` move 8 bytes. `clone_stack_value`
  checks tag for refcount. `drop_stack_value` checks tag for dealloc.
- `seqstring.rs` — Add `HeapString` struct with header + string data.
  `into_raw_parts`/`from_raw_parts` change to heap pointer operations.

~500 lines changed. Can be validated with `cargo test -p seq-core`.

### Phase 2: Codegen (seq-compiler)

Update LLVM IR generation to use `i64` values.

Files (68 occurrences of `%Value` across 6 files):
- `program.rs` — `%Value = type i64` (was `{ i64, i64, i64, i64, i64 }`)
- `inline/ops.rs` — Arithmetic/comparison ops: remove GEP to slot1,
  operate on tagged i64 directly (shift to extract, shift to re-tag)
- `inline/dispatch.rs` — Inline op dispatch: tag check replaces
  discriminant load. ~45 occurrences.
- `virtual_stack.rs` — Spill writes 8 bytes. GEP stride becomes 1 i64.
- `control_flow.rs` — Branch on tag bits instead of loaded discriminant.
- `statements.rs` — Stack pointer arithmetic uses i64 stride.
- `specialization.rs` — Already operates on i64 payloads in many cases.

~2000 lines changed. Can be validated with full test suite + benchmarks.

### Phase 3: Runtime (seq-runtime)

Update runtime functions that pop/push values.

Most runtime functions call `pop()` / `push()` from seq-core, so the
changes are confined to:
- Functions that inspect Value variants (match on type) — these check
  tags instead of discriminants
- Functions that construct Values — these tag instead of writing slots
- String operations — work with HeapString pointers

~50 call sites. Mechanical changes.

### Phase 4: Validation

- All integration tests pass
- Benchmark suite shows expected improvement
- REPL works
- LSP works
- Example programs compile and run correctly

## What This Enables

Once values are 8 bytes:
- **COW collections**: Copying on fallback path moves 8 bytes per element
- **Loop lowering**: Phi nodes carry `i64` values, no 40-byte spills
- **Buffered channels**: Ring buffer slots are 8 bytes, 8x more per cache line
- **Future NaN-boxing**: If float perf matters, layer NaN-boxing on top
  of the tagged pointer scheme

## Risks

**Correctness**: Tag bit manipulation is error-prone. A wrong shift or
mask is a silent corruption, not a compile error. Mitigation: the test
suite is comprehensive, and Miri can catch most memory safety issues.

**String performance**: Adding a pointer indirection to every string
access. Mitigation: strings already go through runtime FFI for all
operations (concat, split, etc.), so the extra indirection is within
the noise of the function call.

**51-bit integer limit (NaN-boxing only)**: If NaN-boxing is used, integers
are limited to ~52 bits. Seq currently uses full i64. Mitigation: use
the simpler tagged-pointer scheme which gives 63-bit integers, or fall
back to heap-allocated BigInt for values > 2^62. In practice, most
integers in Seq programs fit easily.

## Checkpoints

1. **Phase 1 done**: `cargo test -p seq-core` passes with 8-byte StackValue
2. **Phase 2 done**: `cargo test --all` passes, generated IR uses `i64`
3. **Phase 3 done**: benchmark suite runs, all examples compile
4. **fib(40) under 800ms** (currently 2200ms) — 5x less memory traffic
5. **build-100k under 5000ms** (currently 15,888ms) — 5x smaller elements
   (further improved by COW on top of this)
6. **Memory: a 100k-int list under 1MB** (currently ~4MB at 40 bytes each)
