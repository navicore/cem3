# String Interning Design Decision

## Current Implementation (Phase 0-7)

**Decision: NO string interning**

### How Strings Work Now

```rust
// In runtime/src/io.rs
pub unsafe extern "C" fn push_string(stack: Stack, c_str: *const i8) -> Stack {
    // Converts C string to owned Rust String
    let s = CStr::from_ptr(c_str).to_str().unwrap().to_owned(); // ALLOCATES
    push(stack, Value::String(s))
}
```

**Every string literal from the compiler allocates a new `String`.**

### Why No Interning (For Now)

1. **Foundation First Philosophy**
   - We're building a bulletproof concatenative core
   - String interning is a performance optimization, not a correctness requirement
   - Premature optimization adds complexity

2. **Simplicity**
   - Current approach: strings are just values, no special handling
   - Interning requires: global intern table, synchronization, lifetime management
   - More moving parts = more ways to break invariants

3. **Correct Before Fast**
   - Current implementation is obviously correct
   - Adding interning changes ownership model
   - Could introduce subtle bugs (e.g., pointer aliasing issues)

4. **Can Add Later**
   - String interning is a backward-compatible optimization
   - Can be added in Phase 9+ without breaking existing code
   - Current API doesn't preclude future interning

### Performance Characteristics

**Current Costs:**
- String literal `"Hello"` appearing 10 times → 10 allocations
- Each `dup` of a string → deep copy (clone)
- String equality → O(n) comparison

**These are acceptable for:**
- Small programs (< 1000 string ops)
- Development/testing phase
- Proving the concatenative foundation works

**These become problematic for:**
- Large programs with many repeated literals
- Hot loops with string operations
- Production-grade performance

## Future: String Interning (Phase 9+)

### When to Add It

**Triggers:**
- Benchmarks show string allocation is a bottleneck
- Real programs have high string literal duplication
- Foundation is stable and well-tested

**NOT before:**
- Core concatenative operations are proven solid
- Variants and pattern matching work flawlessly
- We have production programs to benchmark

### How It Would Work

```rust
// Hypothetical future design
pub struct InternTable {
    strings: RwLock<HashMap<&'static str, Arc<String>>>,
}

pub unsafe extern "C" fn push_string_interned(
    stack: Stack,
    c_str: *const i8
) -> Stack {
    let s = CStr::from_ptr(c_str).to_str().unwrap();

    // Check intern table first
    let interned = INTERN_TABLE.get_or_insert(s);

    push(stack, Value::String(interned)) // Arc<String> instead of String
}
```

**Benefits:**
- Identical strings share backing storage
- Equality is pointer comparison O(1)
- Reduced memory footprint

**Costs:**
- Global mutable state (intern table)
- Synchronization overhead (RwLock)
- Lifetime complexity (when to evict from table?)
- `Value` becomes more complex (Arc vs String)

### Alternative: Static String References

**Reviewer's suggestion:**
> "If string literals from the compiler are static, you might be able to use
> references instead, avoiding allocation."

This would work if we can guarantee:
1. All string literals outlive the runtime (static lifetime)
2. Compiler embeds literals in binary (like `const STR: &str = "Hello"`)
3. No runtime string construction (concatenation, formatting, etc.)

**Challenges:**
- Requires close compiler/runtime coordination
- Can't mix static literals with runtime strings easily
- `Value::String(&'static str)` vs `Value::String(String)` - two types

**This might be better than interning!** Worth exploring in Phase 8.

## Recommendation

### Phase 0-7 (Now)
**Status: ✅ Use owned Strings, no interning**

Reasoning:
- Simple, correct, maintainable
- Performance is adequate for foundation work
- Don't optimize before measuring

### Phase 8 (Compiler Integration)
**Consider: Static string references**

When adding the compiler:
- Evaluate if string literals can be `&'static str`
- Measure allocation overhead with real programs
- If overhead is low (< 5% runtime), stick with owned strings

### Phase 9+ (Optimizations)
**Consider: String interning OR reference-counted strings**

If benchmarks show string allocation is a bottleneck:
1. Profile first - is it actually the problem?
2. Consider simpler solutions (string pooling, copy-on-write)
3. Only add interning if clearly beneficial
4. Thoroughly test new ownership model

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-10-20 | No string interning in Phase 0-7 | Foundation first, correct before fast |
| TBD | Revisit during compiler integration | Measure before optimizing |

## Related Considerations

### Thread Safety
If we add interning, must consider:
- Multiple execution strands (cem's green threads)
- Concurrent access to intern table
- Lock contention impact on performance

### Memory Management
Interning raises questions:
- When to evict strings from intern table?
- What if program creates millions of unique strings?
- Do we leak memory or periodically collect?

### Compatibility
Current approach:
- Strings are just values (no hidden global state)
- Easy to serialize/deserialize
- Clear ownership (each Value owns its string)

Interning approach:
- Strings reference global table
- Harder to serialize (need to handle Arc/references)
- Less obvious ownership

## Summary

**We're deliberately NOT doing string interning now.**

It's not laziness - it's disciplined engineering:
1. Build correct foundation first
2. Measure before optimizing
3. Add complexity only when proven necessary

The current simple approach lets us focus on what matters: proving that
concatenative operations, stack shuffling, and variants work flawlessly.

If strings become a bottleneck later, we have clear paths forward (interning,
static references, or Arc<str>). But we'll cross that bridge when we get there,
armed with benchmarks and real-world data.
