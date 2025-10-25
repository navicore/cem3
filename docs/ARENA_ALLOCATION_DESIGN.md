# Arena Allocation Design for cem3

## Problem Statement

Currently, all String and Variant allocations go to the global heap. For long-running programs (HTTP servers with strands handling requests), this causes:

1. **Memory fragmentation** - Many small allocations/deallocations
2. **Allocation overhead** - Each String allocation ~100ns
3. **Memory leaks** - Without proper cleanup, memory grows unbounded

## CSP Use Case: HTTP Server

```
HTTP Request → Spawn Strand → Parse (temp Strings) → Process → Build Response → Send → Exit
```

- Each request = one strand
- Parsing creates many temporary Strings (headers, URL, body)
- Processing may create more temporaries
- Response String sent through channel
- Strand exits

**Without arena**: Each temp String malloc'd and free'd individually
**With arena**: Bump allocate all temps, one arena reset at strand exit

## Challenge: Rust's String Type

Rust's `String` uses the global allocator via `alloc::string::String`. Bumpalo provides `bumpalo::collections::String<'bump>`, but adding a lifetime parameter to `Value` would cascade everywhere and break `Send` trait.

## Solution: Custom String Type with Allocation Tracking

### Design

```rust
/// String that can be arena or globally allocated
pub struct CemString {
    ptr: *const u8,
    len: usize,
    global: bool,  // true = global allocator, false = thread-local arena
}

pub enum Value {
    Int(i64),
    Bool(bool),
    String(CemString),  // Changed from String
    Variant(Box<VariantData>),
    Quotation(usize),
}
```

### Lifecycle

#### 1. Arena Allocation (Fast Path for Temporaries)

```rust
pub fn arena_string(s: &str) -> CemString {
    ARENA.with(|arena| {
        let arena_str = arena.borrow().alloc_str(s);
        CemString {
            ptr: arena_str.as_ptr(),
            len: arena_str.len(),
            global: false,
        }
    })
}
```

- **Performance**: ~5ns vs ~100ns for global allocator
- **Usage**: Temporary strings during strand execution
- **Lifetime**: Valid until arena reset (strand exit)

#### 2. Global Allocation (For Persisted Strings)

```rust
pub fn global_string(s: String) -> CemString {
    let len = s.len();
    let ptr = s.as_ptr();
    let cap = s.capacity();
    std::mem::forget(s);  // Transfer ownership, don't drop

    CemString {
        ptr,
        len,
        global: true,
    }
}
```

- **Performance**: Same as regular String
- **Usage**: Strings that outlive strand (sent through channels)
- **Lifetime**: Until explicitly dropped

#### 3. Clone (Channel Send Safety)

```rust
impl Clone for CemString {
    fn clone(&self) -> Self {
        // Always clone to global allocator for Send safety
        let s = self.as_str().to_string();
        global_string(s)
    }
}
```

**Critical**: When sending through channel, clone creates global-allocated copy. This ensures arena can be reset without invalidating the value in the channel.

#### 4. Drop Semantics

```rust
impl Drop for CemString {
    fn drop(&mut self) {
        if self.global {
            // Reconstruct String and drop it
            unsafe {
                let s = String::from_raw_parts(
                    self.ptr as *mut u8,
                    self.len,
                    self.len,  // capacity = len (we don't track capacity separately)
                );
                drop(s);
            }
        }
        // Arena strings don't need explicit drop - arena reset frees them
    }
}
```

#### 5. Arena Reset (Strand Exit)

```rust
pub fn arena_reset() {
    ARENA.with(|arena| {
        arena.borrow_mut().reset();
    });
}
```

Called in `scheduler.rs` when strand exits (already implemented in `free_stack`).

### Safety Invariants

1. **Arena strings only valid until reset**
   - Arena is thread-local
   - Reset happens when strand exits
   - No strand outlives its thread's arena

2. **Channel sends clone to global**
   - Clone always allocates globally
   - Receiver gets independent copy
   - Sender's arena can be freed

3. **No dangling pointers**
   - Global strings properly dropped
   - Arena strings freed on reset
   - Drop impl checks `global` flag

4. **Send safety**
   - CemString is Send (raw pointer + flag)
   - Global strings truly independent
   - Clone handles arena → global transfer

### Implementation Plan

#### Phase 9.2.1: Add CemString Type

1. Create `runtime/src/cemstring.rs`
   - Define CemString struct
   - Implement arena_string(), global_string()
   - Implement Clone, Drop, Debug, PartialEq
   - Add as_str() helper

2. Update `runtime/src/value.rs`
   - Change `String(String)` → `String(CemString)`
   - Update PartialEq impl

3. Update all String operations
   - `io.rs`: read_line, write_line, push_string
   - `pattern.rs`: variant field access
   - Tests that create Strings

#### Phase 9.2.2: Integrate Arena Reset

1. Update `scheduler.rs`
   - Call arena_reset() in free_stack() (before returning nodes to pool)
   - Ensures all arena memory freed when strand exits

2. Add arena stats/monitoring
   - Track bytes allocated
   - Auto-reset on threshold (e.g., 10MB)
   - Prevent unbounded growth within single strand

#### Phase 9.2.3: Testing

1. **Unit tests**
   - Arena string creation and drop
   - Global string creation and drop
   - Clone behavior
   - Mixed arena/global on same stack

2. **Integration tests**
   - Strand with arena strings, exits cleanly
   - Channel send/receive with Strings
   - Arena reset after strand exit

3. **Stress test**
   - Spawn 10,000 strands
   - Each creates 100 temp Strings
   - Verify memory doesn't grow
   - Compare perf vs global allocator

### Tradeoffs

#### Pros
- ✅ ~20x faster allocation for temporary strings
- ✅ Zero fragmentation (linear bump allocation)
- ✅ Bulk free on strand exit (one arena reset vs many free() calls)
- ✅ Predictable memory usage per strand
- ✅ Maintains Send for channel communication
- ✅ Compatible with CSP concurrency model

#### Cons
- ⚠️ Adds complexity to String type
- ⚠️ Requires careful unsafe code in Drop impl
- ⚠️ Clone always allocates globally (could be optimized later)
- ⚠️ Arena is thread-local, not strand-local (acceptable per CLAUDE.md)
- ⚠️ Need to track capacity for global strings (or accept waste)

#### Limitations
- **Thread-local arena**: If strand migrates threads (rare with May), it uses new thread's arena. This is acceptable (mentioned in CLAUDE.md: "This is acceptable for most workloads").
- **Clone cost**: Sending String through channel clones to global. Alternative would be reference counting (Arc), but that's more complex.
- **No capacity tracking**: Global strings reconstructed with `capacity = len`. Wastes some memory if original had excess capacity. Could fix by adding `cap` field to CemString.

### Alternative Approaches Considered

1. **bumpalo::String<'bump>**
   - Rejected: Requires lifetime on Value enum, breaks Send

2. **Arc<str> for all strings**
   - Rejected: No arena allocation, worse performance for temps

3. **Two separate Value variants (ArenaString vs String)**
   - Rejected: Complicates pattern matching, type system would need to track which variant

4. **Arena-per-strand (not thread-local)**
   - Rejected: May allows strand migration between threads, would need to pass arena pointer

### Future Optimizations

1. **Track capacity** - Add `cap: usize` to CemString for proper String reconstruction
2. **String interning** - Common strings (empty, whitespace) in static arena
3. **Copy-on-write** - Detect immutable strings, share backing data
4. **Arena-aware Clone** - If cloning within same thread, could share arena allocation

## Summary

This design enables efficient arena allocation for temporary Strings within strands while maintaining safety for channel communication. The key insight is tracking allocation source with a flag and always cloning to global allocator when crossing strand boundaries.

**Next Steps**: Implement Phase 9.2.1 starting with `cemstring.rs`.
