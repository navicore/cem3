# Phase 9: Memory Management Implementation Plan

## Current State Analysis

### Memory Allocation Patterns

**StackNodes:**
```rust
// stack.rs:29 - Every push allocates
pub unsafe fn push(stack: Stack, value: Value) -> Stack {
    let node = Box::new(StackNode { value, next: stack });
    Box::into_raw(node)
}

// stack.rs:44 - Only freed in pop
pub unsafe fn pop(stack: Stack) -> (Stack, Value) {
    let node = Box::from_raw(stack);  // Frees the node
    (node.next, node.value)
}
```

**Values:**
```rust
pub enum Value {
    Int(i64),              // Stack-allocated
    Bool(bool),            // Stack-allocated
    String(String),        // Heap-allocated via global allocator
    Variant(Box<VariantData>),  // Heap-allocated
    Quotation(usize),      // Stack-allocated
}
```

**Strand Cleanup:**
```rust
// scheduler.rs:148 - Called when strand exits
free_stack(final_stack);  // Walks stack and frees all nodes
```

### Problems

1. **Frequent malloc/free**
   - Every `push()` calls `Box::new()` → global allocator
   - ~100ns per allocation on modern systems
   - HTTP server with 1000 req/s = 100,000+ allocations/s

2. **No reuse**
   - Freed nodes go back to allocator
   - Next push allocates a "new" node (likely recycled memory, but slow)

3. **Value allocation unbounded**
   - Strings and Variants allocated via global allocator
   - No per-strand accounting
   - Long-running strands accumulate memory

## Solution: Two-Tier Memory Management

### Tier 1: Thread-Local Stack Node Pool

**Design:**
```rust
// Thread-local pool of StackNodes
thread_local! {
    static NODE_POOL: RefCell<NodePool> = RefCell::new(NodePool::new());
}

struct NodePool {
    free_list: *mut StackNode,  // Linked list of free nodes
    count: usize,                // Number of free nodes
    capacity: usize,             // Max pool size
}

impl NodePool {
    const INITIAL_CAPACITY: usize = 256;  // Pre-allocate 256 nodes
    const MAX_CAPACITY: usize = 1024;     // Don't grow beyond this

    fn allocate(&mut self, value: Value, next: *mut StackNode) -> *mut StackNode {
        if self.free_list.is_null() {
            // Pool empty - allocate new node
            Box::into_raw(Box::new(StackNode { value, next }))
        } else {
            // Reuse from pool (~10x faster than malloc)
            let node = self.free_list;
            self.free_list = unsafe { (*node).next };
            self.count -= 1;

            // Initialize the reused node
            unsafe {
                (*node).value = value;
                (*node).next = next;
            }
            node
        }
    }

    fn free(&mut self, node: *mut StackNode) {
        if self.count < self.capacity {
            // Return to pool
            unsafe {
                (*node).next = self.free_list;
            }
            self.free_list = node;
            self.count += 1;
        } else {
            // Pool full - actually free
            unsafe { drop(Box::from_raw(node)); }
        }
    }
}
```

**Benefits:**
- ~10x faster than malloc (benchmarked in Forth systems)
- Thread-local = no contention
- Bounded growth = no memory bloat

### Tier 2: Arena Allocator Per Strand

**Design:**
```rust
// Each strand has its own arena for Value allocations
pub struct Arena {
    chunks: Vec<Chunk>,
    current: *mut u8,
    end: *mut u8,
}

struct Chunk {
    data: Box<[u8]>,
    used: usize,
}

impl Arena {
    const CHUNK_SIZE: usize = 64 * 1024;  // 64KB chunks

    fn allocate<T>(&mut self, value: T) -> *mut T {
        let size = std::mem::size_of::<T>();
        let align = std::mem::align_of::<T>();

        // Align current pointer
        let aligned = (self.current as usize + align - 1) & !(align - 1);
        let next = aligned + size;

        if next > self.end as usize {
            // Need new chunk
            self.add_chunk();
            return self.allocate(value);
        }

        let ptr = aligned as *mut T;
        unsafe {
            std::ptr::write(ptr, value);
        }
        self.current = next as *mut u8;
        ptr
    }

    fn reset(&mut self) {
        // Don't free chunks - just reset pointers
        // Reuse chunks for next request
        if let Some(first) = self.chunks.first_mut() {
            self.current = first.data.as_mut_ptr();
            self.end = unsafe { self.current.add(first.data.len()) };
        }
        for chunk in &mut self.chunks {
            chunk.used = 0;
        }
    }
}

// Stored in strand-local storage
thread_local! {
    static STRAND_ARENA: RefCell<Option<Arena>> = RefCell::new(None);
}
```

**Benefits:**
- Super fast bump allocation (~5ns vs ~100ns)
- No individual frees during strand execution
- Bulk reset when strand exits
- Chunks reused across requests

## Implementation Strategy

### Phase 9.1: Stack Node Pool
1. Create `runtime/src/pool.rs`
2. Implement `NodePool` with thread-local storage
3. Update `stack::push()` to use pool
4. Update `stack::pop()` to return nodes to pool
5. Update `drop()` and other ops to use pool
6. Test: All existing tests pass
7. Benchmark: Measure speedup

### Phase 9.2: Arena Allocator
1. Create `runtime/src/arena.rs`
2. Implement `Arena` with chunk management
3. Add strand-local arena initialization
4. Update String allocation to use arena
5. Update Variant allocation to use arena
6. Update `free_stack()` to reset arena
7. Test: All existing tests pass
8. Test: Valgrind shows no leaks

### Phase 9.3: Integration & Optimization
1. Profile memory usage patterns
2. Tune pool/arena sizes
3. Add memory usage metrics
4. Benchmark vs Phase 8.5
5. Document trade-offs

## Success Criteria

✅ **Performance:**
- Stack operations >5x faster (pool vs malloc)
- No performance regression in any test
- Memory usage stable under load

✅ **Correctness:**
- All 124 tests pass
- Valgrind shows no leaks
- No use-after-free bugs

✅ **Production Ready:**
- HTTP server can handle 10,000+ requests without memory growth
- Strands can run indefinitely
- Graceful degradation if pool exhausted

## Testing Strategy

1. **Unit Tests:**
   - Pool allocation/free cycles
   - Arena allocation patterns
   - Boundary conditions (pool full, arena exhausted)

2. **Integration Tests:**
   - Existing test suite must pass unchanged
   - Long-running strand tests (1000+ operations)

3. **Memory Tests:**
   - Valgrind: `cargo test --release && valgrind --leak-check=full ./target/release/test`
   - Stress test: Spawn 1000 strands, each with 1000 operations

4. **Performance Tests:**
   - Benchmark push/pop cycles: pool vs malloc
   - Benchmark arena vs malloc for String allocation
   - Compare Phase 8.5 vs Phase 9 performance

## Risks & Mitigations

**Risk:** Pool/Arena sizes wrong for real workloads
**Mitigation:** Make sizes tunable, add metrics, profile real usage

**Risk:** Thread-local storage overhead
**Mitigation:** Benchmark early, compare to alternatives

**Risk:** Arena fragmentation
**Mitigation:** Use power-of-2 chunk sizes, reset strategy

**Risk:** Breaking existing code
**Mitigation:** Keep old API, change only internals

## Timeline

- **Phase 9.1** (Stack Node Pool): 1 session
- **Phase 9.2** (Arena Allocator): 1 session
- **Phase 9.3** (Integration): 0.5 session

**Total: 2-3 sessions** as estimated in roadmap.
