# Known Issues

## make-variant Type Annotation Bug (RESOLVED)

**Status**: Resolved - old `make-variant` removed, typed constructors available
**Discovered**: During SeqLisp implementation exercise

### Problem

The original `make-variant` builtin had an incomplete type signature that didn't account for field consumption. When a word used `make-variant` with an explicit type annotation, the type checker would fail.

### Solution

Replaced with type-safe variant constructors with fixed arity:

- `make-variant-0`: `( tag -- Variant )` - 0 fields
- `make-variant-1`: `( field1 tag -- Variant )` - 1 field
- `make-variant-2`: `( field1 field2 tag -- Variant )` - 2 fields
- `make-variant-3`: `( field1 field2 field3 tag -- Variant )` - 3 fields
- `make-variant-4`: `( field1 field2 field3 field4 tag -- Variant )` - 4 fields

### Usage

```seq
: snum ( Int -- Variant )
  1 make-variant-1 ;   # Type-safe!

: empty-array ( -- Variant )
  4 make-variant-0 ;   # Tag 4, no fields
```

### Migration Completed

- ✅ `make-variant-N` variants implemented (0-4 fields)
- ✅ `json.seq` migrated to use typed variants
- ✅ `yaml.seq` migrated to use typed variants
- ✅ Original `make-variant` removed

### Future Work

The fixed-arity approach works but has limitations (max 4 fields). If needed, potential improvements:
- Add `make-variant-5` through `make-variant-N` as needed
- Use `variant-append` for building dynamic collections (already supported)

## pick/roll Type Inference Bug (RESOLVED)

**Status**: Resolved - special-case handling added for literal indices
**Discovered**: During actor_counters.seq Phase 2 implementation

### Problem

The `pick` and `roll` builtins had type signatures that couldn't properly express their behavior. Their types depend on the *value* of the index, not just its type. The generic signatures caused type errors when used with explicit type annotations.

### Solution

Added special-case handling in `typechecker.rs` for literal `n pick` and `n roll` patterns:

- When the type checker sees `IntLiteral(n)` followed by `pick` or `roll`, it handles them as a fused operation
- `handle_literal_pick` and `handle_literal_roll` compute correct types based on the actual index
- Falls back to generic signatures for non-literal indices (less precise but still works)

### Files Changed

- `crates/compiler/src/typechecker.rs`: Added `handle_literal_pick`, `handle_literal_roll`, `get_type_at_position`, `rotate_type_to_top`

## spawn Type Signature Bug (RESOLVED)

**Status**: Resolved - spawn now accepts any quotation effect
**Discovered**: During actor_counters.seq implementation

### Problem

The `spawn` builtin required quotations with effect `( -- )` (empty input, empty output). But the actual runtime behavior passes the parent's stack to the child, so actors need to receive arguments.

### Solution

Changed spawn's type signature in `builtins.rs` to accept any quotation effect:

```rust
// Before: Quotation with ( -- ) effect required
// After: Quotation with any effect accepted
Type::Quotation(Box::new(Effect::new(
    StackType::RowVar("spawn_in".to_string()),
    StackType::RowVar("spawn_out".to_string()),
)))
```

## spawn Runtime Stack Passing Bug (RESOLVED)

**Status**: Resolved - spawn now clones and passes parent stack to child
**Discovered**: During actor_counters.seq implementation

### Problem

The `patch_seq_spawn` runtime function passed a null stack to spawned strands, so actors couldn't receive arguments from the parent.

### Solution

Modified `quotations.rs` to:
1. Clone the parent's stack using a new `clone_stack` function
2. Pass the cloned stack to the spawned strand
3. Use Box allocation (not thread-local pool) for cross-strand safety

### Files Changed

- `crates/runtime/src/stack.rs`: Added `clone_stack` function
- `crates/runtime/src/quotations.rs`: Updated `patch_seq_spawn` to clone and pass stack

## make-variant-N Argument Count Bug (RESOLVED)

**Status**: Resolved - actor_counters.seq updated to use correct argument counts
**Discovered**: During actor_counters.seq Phase 2 debugging

### Problem

The `make-report-msg` function in actor_counters.seq was using `make-variant-3` with only 3 arguments (delta, total, tag) when it requires 4 (field1, field2, field3, tag). This caused stack corruption - an extra value was consumed from the caller's stack.

### Symptoms

- Server hung after 5th increment (when store tried to send report to district)
- Stack corruption led to incorrect `4 pick` results

### Solution

Fixed `make-report-msg` to provide all 4 arguments to `make-variant-3`:

```seq
# Before (buggy):
: make-report-msg ( Int Int -- Variant )
  2 make-variant-3   # Only 3 items on stack, needs 4!
;

# After (correct):
: make-report-msg ( Int Int -- Variant )
  2                    # ( delta total 2 )
  rot                  # ( total 2 delta )
  rot                  # ( 2 delta total )
  2                    # ( 2 delta total 2 ) -> ( field0 field1 field2 tag )
  make-variant-3       # variant{tag=2, f0=2, f1=delta, f2=total}
;
```

Also updated extractor functions to match the corrected field layout:
- `msg-delta`: Changed from field 0 to field 1
- `msg-total`: Changed from field 1 to field 2

### Lesson

When using `make-variant-N`, always ensure exactly N+1 values are on the stack (N fields + 1 tag)
