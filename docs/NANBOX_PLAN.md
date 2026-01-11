# NaN-Boxing Implementation Plan (Issue #188)

## Overview

Reduce Value size from 40 bytes to 8 bytes using NaN-boxing, improving cache utilization and reducing memory bandwidth.

## Current State

```
StackValue (40 bytes):
  slot0: u64  // discriminant (0-10)
  slot1: u64  // primary payload
  slot2: u64  // secondary payload
  slot3: u64  // unused for most types
  slot4: u64  // unused for most types
```

**Discriminants:**
- 0: Int, 1: Float, 2: Bool, 3: String, 4: Variant
- 5: Map, 6: Quotation, 7: Closure, 8: Channel, 9: WeaveCtx, 10: Symbol

## Proposed NaN-Boxing Scheme

IEEE 754 doubles use specific bit patterns for NaN. We can encode non-float values in the "quiet NaN" space:

```
Float (normal):    [any valid IEEE 754 double that isn't a signaling NaN]
Boxed values:      0x7FF8_TTTT_PPPP_PPPP
                        ^^^^-- 4-bit type tag (0-15)
                             ^^^^^^^^^^^-- 48-bit payload
```

### Encoding Options

**Option A: Simple NaN-box (recommended for Phase 1)**
```
Float:      [IEEE 754 double, excluding quiet NaN range]
Int:        0x7FF8_0000_IIII_IIII  (48-bit signed integer, ~±140 trillion)
Bool:       0x7FF8_1000_0000_000V  (V = 0 or 1)
Pointer:    0x7FF8_2PPP_PPPP_PPPP  (48-bit pointer, works on x86-64/ARM64)
            Subtypes via high 4 bits of pointer or separate tag
```

**Option B: Tagged pointer hybrid**
- Use low 3 bits of pointers (always 0 due to alignment) for small types
- More complex but allows full 51-bit integers

### Type Tag Allocation (Option A)
```
0x0: Int (48-bit signed)
0x1: Bool
0x2: String (ptr to SeqString)
0x3: Symbol (ptr to SeqString)
0x4: Variant (ptr to Arc<VariantData>)
0x5: Map (ptr to Box<HashMap>)
0x6: Quotation (ptr to QuotationData)
0x7: Closure (ptr to ClosureData)
0x8: Channel (ptr to Arc<ChannelData>)
0x9: WeaveCtx (ptr to WeaveCtxData)
```

## Trade-offs

### Pros
- 5x smaller values (8 vs 40 bytes)
- Better cache utilization (8 values per cache line vs 1.6)
- Faster stack operations (single load/store)
- More values fit in registers

### Cons
- **Integer range limited**: 48-bit signed (~±140 trillion) vs 64-bit
- **Float NaN handling**: Real NaN values need canonicalization
- **Pointer types need indirection**: Quotation (2 pointers), Closure (2 pointers), WeaveCtx (2 pointers) need heap allocation for multi-slot data
- **Complexity**: More intricate encoding/decoding logic

## Impact Analysis

### High-Impact Files (Core Changes)

| File | Lines | Scope |
|------|-------|-------|
| `runtime/value.rs` | 121-198, 313-328 | Value enum, size assertions |
| `runtime/tagged_stack.rs` | 49-90, 244-289 | StackValue struct, layout |
| `runtime/stack.rs` | 26-489 | Discriminants, conversions |
| `codegen/program.rs` | 81, 167 | `%Value` LLVM type |
| `codegen/inline/ops.rs` | 14-225 | Arithmetic codegen |
| `codegen/inline/dispatch.rs` | 34-230, 1057-1244 | Stack ops, size constants |
| `codegen/virtual_stack.rs` | 24-113 | Spill/push operations |

### Medium-Impact Files (Builtin Operations)

All runtime builtins need updated discriminant checks:
- `arithmetic.rs`, `float_ops.rs`, `string_ops.rs`
- `variant_ops.rs`, `map_ops.rs`, `list_ops.rs`
- `closures.rs`, `quotations.rs`, `channel.rs`, `weave.rs`
- `io.rs`, `cond.rs`

### Hardcoded Size Dependencies

1. `inline/dispatch.rs:1061` - `mul i64 %n, 40` for roll/pick
2. `inline/dispatch.rs:1244` - `n * 40` for static calculations
3. `stack.rs:815,880,1072` - `/ std::mem::size_of::<StackValue>()`
4. `tagged_stack.rs:87-90` - `assert!(STACK_VALUE_SIZE == 40)`

## Migration Strategy

### Phase 1: Foundation (Breaking Change Prep)
1. Design and document final NaN-box encoding
2. Create `NanBoxedValue` type alongside existing `Value`
3. Implement encoding/decoding utilities with comprehensive tests
4. Handle edge cases: real NaN values, large integers

### Phase 2: Runtime Dual Support
1. Add feature flag for NaN-boxing (`--features nanbox`)
2. Update `StackValue` with conditional compilation
3. Update conversion functions with both paths
4. Ensure all 272+ tests pass in both modes

### Phase 3: Codegen Updates
1. Update `%Value` LLVM type conditionally
2. Update pointer arithmetic (getelementptr offsets)
3. Update inline operations for new layout
4. Ensure virtual stack works with new size

### Phase 4: FFI Boundaries
1. Update all `extern "C"` functions
2. Careful handling of Value-by-value passing
3. Update closure environment functions
4. Test FFI thoroughly

### Phase 5: Cleanup
1. Remove old 40-byte code paths
2. Update documentation
3. Update benchmarks
4. Performance validation

## New Heap-Allocated Types

Types that currently use multiple slots need heap allocation:

```rust
// Old: stored inline in slots 1-2
struct Quotation { wrapper: u64, impl_: u64 }

// New: heap allocated, pointer in NaN-box
struct QuotationData { wrapper: u64, impl_: u64 }
// NaN-box stores *mut QuotationData

// Similarly for:
struct ClosureData { fn_ptr: u64, env: *mut [Value] }
struct WeaveCtxData { yield_chan: Arc<ChannelData>, resume_chan: Arc<ChannelData> }
```

## Performance Considerations

### Expected Wins
- Stack-heavy code (dup, swap, rot, pick, roll)
- Loops with many iterations
- Recursive algorithms
- Programs with deep call stacks

### Potential Regressions
- Code that frequently creates Quotations/Closures (now heap-allocated)
- Code using integers > 48 bits (need BigInt or error)
- Float-heavy code with real NaN values (canonicalization overhead)

## Testing Strategy

1. **Unit tests**: Encoding/decoding round-trips for all types
2. **Edge cases**: MAX_INT, MIN_INT, NaN, subnormals, pointer alignment
3. **Integration tests**: All existing tests must pass
4. **Benchmarks**: Compare before/after on compute-heavy examples

## Open Questions

1. **Integer overflow policy**: Error at compile time? Runtime? Silent wrap?
2. **NaN canonicalization**: Store canonical NaN, or reserve NaN range?
3. **Quotation allocation**: Arena allocator? Per-quotation heap alloc?
4. **Phased rollout**: Feature flag for gradual adoption?

## Estimated Effort

| Phase | Effort | Risk |
|-------|--------|------|
| 1: Foundation | 2-3 days | Low |
| 2: Runtime | 3-5 days | Medium |
| 3: Codegen | 3-5 days | High |
| 4: FFI | 2-3 days | Medium |
| 5: Cleanup | 1-2 days | Low |

**Total: ~2-3 weeks** for careful implementation with testing.

## References

- [JavaScriptCore NaN-boxing](https://webkit.org/blog/7846/concurrent-javascript-it-can-work/)
- [LuaJIT implementation](https://luajit.org/ext_ffi_semantics.html)
- [Crafting Interpreters - NaN Boxing](https://craftinginterpreters.com/optimization.html)
