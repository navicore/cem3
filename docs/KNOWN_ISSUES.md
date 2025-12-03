# Known Issues

## make-variant Type Annotation Bug (FIXED)

**Status**: Fixed in this commit
**Discovered**: During SeqLisp implementation exercise

### Problem

The original `make-variant` builtin had an incomplete type signature that didn't account for field consumption. When a word used `make-variant` with an explicit type annotation, the type checker would fail.

### Solution

Added type-safe variant constructors with fixed arity:

- `make-variant-0`: `( tag -- Variant )` - 0 fields
- `make-variant-1`: `( field1 tag -- Variant )` - 1 field
- `make-variant-2`: `( field1 field2 tag -- Variant )` - 2 fields
- `make-variant-3`: `( field1 field2 field3 tag -- Variant )` - 3 fields
- `make-variant-4`: `( field1 field2 field3 field4 tag -- Variant )` - 4 fields

### Usage

```seq
# Old way (incomplete type checking):
: snum ( Int -- Variant )
  1 1 make-variant ;   # ERROR: type mismatch

# New way (proper type safety):
: snum ( Int -- Variant )
  1 make-variant-1 ;   # Works correctly!
```

### Note

The original `make-variant` is still available for backward compatibility and for cases where the field count is dynamic, but it provides incomplete type checking. Prefer the typed `make-variant-N` variants for type safety.

### Future Work

We plan to revisit the type inference system to:

1. **Remove `make-variant`**: Once all code is migrated to use the typed `make-variant-N` variants, deprecate and remove the original `make-variant` to prevent accidentally bypassing type safety.

2. **Evaluate more robust solutions**: The fixed-arity approach works but has limitations (max 4 fields, verbose for common cases). Potential improvements to discuss:
   - **Literal tracking in type system**: Infer field count from compile-time literal values
   - **Dependent types**: Full dependent type support (significant complexity)
   - **Macro-based generation**: Generate `make-variant-N` at compile time based on usage
   - **Builder pattern**: Functional variant construction with `variant-new` / `variant-with-field`

3. **Update stdlib**: Migrate `json.seq`, `yaml.seq`, and other stdlib code to use typed variants.
