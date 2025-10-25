# String Interning Design Decision

## Current Implementation (Phase 9.2 Complete)

**Decision: NO string interning, BUT we have arena allocation**

### How Strings Work Now

As of Phase 9.2, cem3 uses **CemString** with two allocation strategies:

```rust
// Arena allocation (fast, temporary strings)
pub fn arena_string(s: &str) -> CemString {
    // Allocates from thread-local bump allocator (~5ns)
    // Freed in bulk when strand exits
}

// Global allocation (persistent strings)
pub fn global_string(s: String) -> CemString {
    // Allocates from global heap (~100ns)
    // Dropped individually when Value is dropped
}

// In runtime/src/io.rs
pub unsafe extern "C" fn push_string(stack: Stack, c_str: *const i8) -> Stack {
    let s = CStr::from_ptr(c_str).to_str().unwrap().to_owned();
    push(stack, Value::String(s.into())) // Uses global_string()
}
```

**Current behavior:**
- String literals from compiler: **Global allocation** (each allocation ~100ns)
- Temporary strings in strand: **Could use arena** (each allocation ~5ns)
- Clone for channel send: **Always global** (ensures safety across strands)

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

### Performance Characteristics (Phase 9.2)

**Current Costs:**
- String literal `"Hello"` appearing 10 times → 10 global allocations (~1000ns total)
- Temporary string in strand → arena allocation (~5ns)
- Each `dup` of a string → deep copy (clone to global, ~100ns)
- String equality → O(n) comparison (content-based, not pointer)

**Phase 9.2 Improvements:**
✅ **Arena allocation** - Temporary strings 20x faster than Phase 0-7
✅ **Bulk free** - All arena strings freed on strand exit (zero overhead)
✅ **CSP-safe** - Channel sends clone to global (no cross-strand pointers)

**Still room for improvement:**
- ❌ String literals from compiler still use global allocation
- ❌ Repeated literals ("true", "false", "") allocate multiple times
- ❌ String equality is O(n), not O(1)

**These are acceptable for:**
- Programs with strands handling requests (HTTP servers)
- Temporary string manipulation (parsing, formatting)
- CSP-style concurrency with message passing

**String interning would help with:**
- Programs with many repeated string literals
- Hot loops comparing constant strings
- Reducing memory footprint for duplicate literals

## Future: String Interning (Phase 10)

### When to Add It

**Triggers:**
- Benchmarks show string *literal* allocation is a bottleneck
- Real programs have high string literal duplication
- Profiling shows arena allocation isn't sufficient

**Already addressed by Phase 9.2:**
✅ Temporary strings during computation (use arena)
✅ Bulk freeing on strand exit (arena reset)
✅ CSP-safe channel communication (clone to global)

**Still problematic:**
- String literals from compiler (each literal allocates globally)
- Comparing constant strings in hot loops (O(n) not O(1))

### Design Option 1: Static Arena Interning

With Phase 9.2's infrastructure, we could have a **static arena** for string literals:

```rust
// Static arena for interned strings (never freed)
static INTERNED_ARENA: Lazy<Bump> = Lazy::new(|| Bump::new());
static INTERN_TABLE: Lazy<RwLock<HashMap<&'static str, *const u8>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

pub fn intern_string(s: &str) -> CemString {
    let table = INTERN_TABLE.read().unwrap();

    if let Some(&ptr) = table.get(s) {
        // Already interned - return reference to static arena
        return CemString { ptr, len: s.len(), global: false };
    }

    drop(table);
    let mut table = INTERN_TABLE.write().unwrap();

    // Allocate in static arena (never freed)
    let interned = INTERNED_ARENA.alloc_str(s);
    table.insert(interned, interned.as_ptr());

    CemString { ptr: interned.as_ptr(), len: s.len(), global: false }
}
```

**Benefits:**
- Identical literals share storage
- No RwLock contention on reads (fast path)
- Integrates with existing CemString infrastructure
- Still arena-allocated (just a static arena)

**Costs:**
- Global state (intern table + static arena)
- Memory never freed (acceptable for literals)
- RwLock overhead (only on first occurrence of each literal)

### Design Option 2: Reference-Counted Interning

Alternative: Use `Arc<str>` for interned strings:

```rust
static INTERN_TABLE: Lazy<RwLock<HashMap<String, Arc<str>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

// CemString would need a third variant
pub enum CemString {
    Arena { ptr: *const u8, len: usize },
    Global { ptr: *const u8, len: usize },
    Interned(Arc<str>), // New variant
}
```

**Benefits:**
- Automatic cleanup (Arc drops when last reference goes)
- No static arena growth
- Standard Rust pattern

**Costs:**
- More complex CemString enum
- Reference counting overhead on clone/drop
- Larger memory footprint per string (Arc metadata)

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

### Phase 9.2 (Current - Complete ✓)
**Status: ✅ Arena allocation for temporary strings, global for persistent**

What we have:
- **CemString** with dual allocation strategy
- **Arena allocation** for ~20x performance on temporaries
- **Global allocation** for persistent/cross-strand strings
- **CSP-safe** channel communication (clone to global)

What we DON'T have (yet):
- String literal interning
- O(1) string equality for constants
- Memory sharing for duplicate literals

**Current performance is good enough for:**
- HTTP servers with strand-per-request
- Programs with temporary string manipulation
- CSP concurrency patterns

### Phase 10 (Future)
**Consider: String literal interning**

Only if benchmarks show:
- String literal allocation is a bottleneck (> 5% runtime)
- Many duplicate literals in real programs
- O(n) equality comparisons in hot paths

**Recommended approach:**
1. **Option 1: Static arena interning** (integrates with Phase 9.2 infrastructure)
2. **Option 2: Arc<str> interning** (if cleanup is needed)
3. **Measure first** - Don't add complexity without proof

**Do NOT add interning for:**
- Temporary strings (already fast with arena)
- Cross-strand strings (already handled by clone)
- Theoretical optimization without benchmarks

## Decision Log

| Date | Decision | Rationale |
|------|----------|-----------|
| 2025-10-20 | No string interning in Phase 0-7 | Foundation first, correct before fast |
| 2025-10-25 | **Phase 9.2: Arena allocation (NO interning)** | Arena solves temporary string performance; interning deferred to Phase 10 |
| TBD | Phase 10: Revisit interning for literals | Only if benchmarks show literal allocation as bottleneck |

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

**Phase 9.2: Arena allocation solves the primary string performance problem.**

String interning remains **deliberately deferred to Phase 10** because:

1. **Arena allocation handles the common case**
   - Temporary strings: ~5ns (20x faster than Phase 0-7)
   - Bulk free on strand exit: zero overhead
   - Perfect for CSP/HTTP server workloads

2. **Interning addresses a different problem**
   - String literals (not temporaries)
   - Duplicate literal allocations
   - O(1) equality for constants

3. **Measure before optimizing**
   - Arena allocation may be sufficient for real programs
   - Interning adds global state, synchronization, lifetime complexity
   - Only add if benchmarks prove it's needed

**Current approach: Simple, fast for common case, correct.**

If string *literals* become a bottleneck in Phase 10, we have clear paths forward:
- **Option 1:** Static arena interning (integrates with Phase 9.2)
- **Option 2:** Arc<str> interning (if cleanup needed)
- **Option 3:** Compiler-assisted static references

But we'll make that decision armed with benchmarks from real cem3 programs, not
theoretical concerns.
