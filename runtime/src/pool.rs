//! Stack Node Pool - Thread-local memory pool for fast stack node allocation
//!
//! Instead of malloc/free for every push/pop, we maintain a thread-local pool
//! of pre-allocated StackNodes. This provides ~10x speedup over Box::new().
//!
//! Design:
//! - Thread-local free list of nodes
//! - Bounded size to prevent unbounded growth
//! - Falls back to malloc if pool exhausted
//! - Returns to pool on free (up to capacity)
//!
//! Safety:
//! - Thread-local = no synchronization needed
//! - Nodes are only accessed by owning thread
//! - Pool size bounded = predictable memory usage

use crate::stack::StackNode;
use crate::value::Value;
use std::cell::RefCell;
use std::ptr;

/// Configuration for the stack node pool
const INITIAL_POOL_SIZE: usize = 256; // Pre-allocate this many nodes
const MAX_POOL_SIZE: usize = 1024; // Don't grow beyond this

/// Stack Node Pool
///
/// Maintains a free list of StackNodes that can be reused.
/// Thread-local to avoid synchronization overhead.
pub struct NodePool {
    /// Linked list of free nodes (using StackNode.next pointer)
    free_list: *mut StackNode,

    /// Number of nodes currently in the free list
    count: usize,

    /// Maximum capacity of the pool
    capacity: usize,
}

impl NodePool {
    /// Create a new empty pool
    fn new() -> Self {
        NodePool {
            free_list: ptr::null_mut(),
            count: 0,
            capacity: MAX_POOL_SIZE,
        }
    }

    /// Allocate a StackNode from the pool or heap
    ///
    /// If the pool has free nodes, reuse one (fast path).
    /// Otherwise, allocate from heap (slow path).
    pub fn allocate(&mut self, value: Value, next: *mut StackNode) -> *mut StackNode {
        if self.free_list.is_null() {
            // Pool empty - allocate from heap
            Box::into_raw(Box::new(StackNode { value, next }))
        } else {
            // Reuse from pool (fast path - ~10x faster than malloc)
            let node = self.free_list;
            unsafe {
                // Update free list to skip this node
                self.free_list = (*node).next;
                self.count -= 1;

                // Initialize the reused node with new value and next pointer
                (*node).value = value;
                (*node).next = next;
            }
            node
        }
    }

    /// Free a StackNode back to the pool or heap
    ///
    /// If pool has capacity, return node to free list (fast path).
    /// Otherwise, drop the node to free memory (slow path).
    ///
    /// # Safety
    /// - `node` must be a valid pointer to a StackNode
    /// - `node` must not have been previously freed
    /// - Caller must not use `node` after calling this function
    pub unsafe fn free(&mut self, node: *mut StackNode) {
        if node.is_null() {
            return;
        }

        if self.count < self.capacity {
            // Return to pool (fast path)
            // Link this node into the free list
            unsafe {
                (*node).next = self.free_list;
            }
            self.free_list = node;
            self.count += 1;
        } else {
            // Pool at capacity - actually free the memory
            unsafe {
                drop(Box::from_raw(node));
            }
        }
    }

    /// Pre-allocate nodes to fill the pool
    ///
    /// This is called on first use to populate the free list.
    /// Pre-allocation amortizes the cost of malloc across many operations.
    fn preallocate(&mut self, count: usize) {
        for _ in 0..count {
            if self.count >= self.capacity {
                break;
            }

            // Allocate a node with dummy value
            // We use Int(0) as a placeholder - it will be overwritten on first use
            let node = Box::into_raw(Box::new(StackNode {
                value: Value::Int(0),
                next: ptr::null_mut(),
            }));

            // Add to free list
            unsafe {
                (*node).next = self.free_list;
                self.free_list = node;
                self.count += 1;
            }
        }
    }

    /// Get current pool statistics (for debugging/profiling)
    #[allow(dead_code)]
    pub fn stats(&self) -> PoolStats {
        PoolStats {
            free_count: self.count,
            capacity: self.capacity,
        }
    }
}

impl Drop for NodePool {
    fn drop(&mut self) {
        // Free all nodes in the pool when thread exits
        unsafe {
            let mut node = self.free_list;
            while !node.is_null() {
                let next = (*node).next;
                drop(Box::from_raw(node));
                node = next;
            }
        }
    }
}

/// Pool statistics for debugging/profiling
#[derive(Debug, Clone, Copy)]
pub struct PoolStats {
    pub free_count: usize,
    pub capacity: usize,
}

// Thread-local storage for the pool
thread_local! {
    static NODE_POOL: RefCell<NodePool> = {
        let mut pool = NodePool::new();
        // Pre-allocate nodes on first access
        pool.preallocate(INITIAL_POOL_SIZE);
        RefCell::new(pool)
    };
}

/// Allocate a StackNode from the thread-local pool
///
/// Fast path: Reuse from pool (~10x faster than malloc)
/// Slow path: Allocate from heap if pool empty
pub fn pool_allocate(value: Value, next: *mut StackNode) -> *mut StackNode {
    NODE_POOL.with(|pool| pool.borrow_mut().allocate(value, next))
}

/// Free a StackNode back to the thread-local pool
///
/// Fast path: Return to pool for reuse
/// Slow path: Drop if pool at capacity
///
/// # Safety
/// - `node` must be a valid pointer to a StackNode
/// - `node` must not have been previously freed
/// - Caller must not use `node` after calling this function
pub unsafe fn pool_free(node: *mut StackNode) {
    NODE_POOL.with(|pool| unsafe { pool.borrow_mut().free(node) })
}

/// Get pool statistics for the current thread
#[allow(dead_code)]
pub fn pool_stats() -> PoolStats {
    NODE_POOL.with(|pool| pool.borrow().stats())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_allocate_and_free() {
        let node1 = pool_allocate(Value::Int(42), ptr::null_mut());
        assert!(!node1.is_null());

        unsafe {
            assert_eq!((*node1).value, Value::Int(42));
            assert!((*node1).next.is_null());
        }

        unsafe { pool_free(node1) };

        // Allocating again should reuse the freed node
        let node2 = pool_allocate(Value::Int(99), ptr::null_mut());
        assert_eq!(node1, node2); // Same address - node was reused!

        unsafe {
            assert_eq!((*node2).value, Value::Int(99));
        }

        unsafe { pool_free(node2) };
    }

    #[test]
    fn test_pool_reuse() {
        // Allocate and free multiple times
        let mut nodes = Vec::new();

        for i in 0..10 {
            let node = pool_allocate(Value::Int(i), ptr::null_mut());
            nodes.push(node);
        }

        // Free all nodes
        for node in &nodes {
            unsafe { pool_free(*node) };
        }

        // Allocate again - should reuse
        let reused = pool_allocate(Value::Int(100), ptr::null_mut());
        assert!(nodes.contains(&reused)); // Address should be one we freed

        unsafe { pool_free(reused) };
    }

    #[test]
    fn test_pool_stats() {
        let stats_before = pool_stats();
        let initial_count = stats_before.free_count;

        let node = pool_allocate(Value::Int(42), ptr::null_mut());
        let stats_after_alloc = pool_stats();
        assert_eq!(stats_after_alloc.free_count, initial_count - 1);

        unsafe { pool_free(node) };
        let stats_after_free = pool_stats();
        assert_eq!(stats_after_free.free_count, initial_count);
    }

    #[test]
    fn test_pool_overflow() {
        // The pool should handle more allocations than its capacity
        let mut nodes = Vec::new();

        for i in 0..2000 {
            let node = pool_allocate(Value::Int(i), ptr::null_mut());
            nodes.push(node);
        }

        // All allocations should succeed
        assert_eq!(nodes.len(), 2000);

        // Free all
        for node in nodes {
            unsafe { pool_free(node) };
        }

        // Pool stats should show it's at or below capacity
        let stats = pool_stats();
        assert!(stats.free_count <= stats.capacity);
    }
}
