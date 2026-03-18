# COW Collections (Copy-on-Write)

## Intent

`list.push` on a 100k-element list takes 15,888ms to build because every
push allocates a new `Arc<VariantData>` and copies all N existing fields.
Python does this in ~0ms. The goal is amortized O(1) push when the list
has a single owner, which is the overwhelmingly common case.

## Current Implementation

Lists are `Value::Variant(Arc<VariantData>)` where:
```rust
pub struct VariantData {
    pub tag: SeqString,
    pub fields: Box<[Value]>,  // immutable slice
}
```

Every `list.push` does:
1. `Vec::with_capacity(n + 1)`
2. Clone all N existing fields into the new vec
3. Push the new element
4. Wrap in `Arc::new(VariantData { ... })`

This is O(n) per push, so building a list of N elements is O(n^2).

## Constraints

- **Language semantics unchanged** — `list.push` must still return a "new"
  list. If the old list is referenced elsewhere, it must be unmodified.
- **Arc refcount is the signal** — `Arc::strong_count(&arc) == 1` means
  we are the sole owner and can mutate safely.
- **Thread safety** — `Arc::strong_count` is atomic. Reading it is safe
  from any thread. `Arc::get_mut` returns `Some` only when count == 1.
- **Don't change Value enum** — Keep `Variant(Arc<VariantData>)`.
- **Don't break variant operations** — Lists are variants. COW must not
  break `variant.append`, `variant.field-at`, etc.

## Approach

### Step 1: Switch `fields` from `Box<[Value]>` to `Vec<Value>`

```rust
pub struct VariantData {
    pub tag: SeqString,
    pub fields: Vec<Value>,  // <-- mutable backing store
}
```

`Vec` has the same read API as `Box<[Value]>` (indexing, len, iter) but
supports `push()` and `reserve()`. This change propagates to construction
sites but not to read sites.

### Step 2: COW in list.push

```rust
pub unsafe extern "C" fn patch_seq_list_push(stack: Stack) -> Stack {
    let (stack, value) = pop(stack);
    let (stack, list_val) = pop(stack);

    let mut arc = match list_val {
        Value::Variant(v) => v,
        _ => panic!("list.push: not a variant"),
    };

    // COW: if we're the sole owner, mutate in place
    if let Some(data) = Arc::get_mut(&mut arc) {
        data.fields.push(value);
        push(stack, Value::Variant(arc))
    } else {
        // Shared — clone and append
        let mut new_fields = Vec::with_capacity(arc.fields.len() + 1);
        new_fields.extend(arc.fields.iter().cloned());
        new_fields.push(value);
        let new_list = Value::Variant(Arc::new(VariantData::new(
            arc.tag.clone(),
            new_fields,
        )));
        push(stack, new_list)
    }
}
```

The same pattern applies to `list.set`, `variant.append`, and any other
operation that creates a "modified copy."

### Step 3: Pre-allocate capacity for map/filter

`list.map` knows the output size equals input size. Pre-allocate:
```rust
let mut results = Vec::with_capacity(variant_data.fields.len());
```
(This is already done in current code, so no change needed for map.)

For `list.filter`, consider COW on the input list when predicate keeps
most elements (optimization for later).

## What This Does NOT Fix

- **40-byte Value size** — Each element is still 40 bytes. A specialized
  `IntArray` backed by `Vec<i64>` would be the next step for numeric
  workloads but is a separate design.
- **Quotation call overhead in map/filter/fold** — Each element still
  requires a function call through the quotation. Loop fusion or
  inline expansion is a separate optimization.

## Checkpoints

1. **build-100k under 100ms** (currently 15,888ms) — the primary target
2. **map-double under 10ms** (currently 83ms) — should improve from
   reduced allocation but quotation overhead remains
3. **Existing tests pass** — `cargo test --all` with no regressions
4. **Seq programs using `variant.append` still work** — the COW path
   must handle all variant operations, not just lists
