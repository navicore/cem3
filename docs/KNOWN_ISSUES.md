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
