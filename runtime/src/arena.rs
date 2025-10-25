//! Arena Allocator - Thread-local bump allocation for Values
//!
//! Uses bumpalo for fast bump allocation of Strings and Variants.
//! Each OS thread has an arena that's used by strands executing on it.
//!
//! Design:
//! - Thread-local Bump allocator
//! - Fast allocation (~5ns vs ~100ns for malloc)
//! - Periodic reset to prevent unbounded growth
//! - Manual reset when strand completes
//!
//! Note: This is thread-local, not strand-local. If a strand migrates
//! to another thread (rare with May), it will use that thread's arena.
//! This is acceptable for most workloads.

use bumpalo::Bump;
use std::cell::RefCell;

/// Configuration for the arena
const ARENA_RESET_THRESHOLD: usize = 10 * 1024 * 1024; // 10MB - reset when we exceed this

// Thread-local arena for value allocations
thread_local! {
    static ARENA: RefCell<Bump> = RefCell::new(Bump::new());
    static ARENA_BYTES_ALLOCATED: RefCell<usize> = const { RefCell::new(0) };
}

/// Execute a function with access to the thread-local arena
///
/// This is used by CemString to allocate strings from the arena.
///
/// # Performance
/// ~5ns vs ~100ns for global allocator (20x faster)
///
/// # Example
/// ```ignore
/// let arena_str = with_arena(|arena| arena.alloc_str("Hello"));
/// ```
pub fn with_arena<F, R>(f: F) -> R
where
    F: FnOnce(&Bump) -> R,
{
    ARENA.with(|arena| {
        let bump = arena.borrow();
        let result = f(&bump);

        // Track allocation size
        let allocated = bump.allocated_bytes();
        drop(bump); // Drop borrow before accessing ARENA_BYTES_ALLOCATED

        ARENA_BYTES_ALLOCATED.with(|bytes| {
            *bytes.borrow_mut() = allocated;
        });

        // Auto-reset if threshold exceeded
        if should_reset() {
            arena_reset();
        }

        result
    })
}

/// Reset the thread-local arena
///
/// Call this when a strand completes to free memory.
/// Also called automatically when arena exceeds threshold.
pub fn arena_reset() {
    ARENA.with(|arena| {
        arena.borrow_mut().reset();
    });
    ARENA_BYTES_ALLOCATED.with(|bytes| {
        *bytes.borrow_mut() = 0;
    });
}

/// Check if arena should be reset (exceeded threshold)
fn should_reset() -> bool {
    ARENA_BYTES_ALLOCATED.with(|bytes| *bytes.borrow() > ARENA_RESET_THRESHOLD)
}

/// Get current arena statistics
#[allow(dead_code)]
pub fn arena_stats() -> ArenaStats {
    // Read from our tracked bytes instead of Bump's internal state
    // This ensures consistency with arena_reset() which sets ARENA_BYTES_ALLOCATED to 0
    let allocated = ARENA_BYTES_ALLOCATED.with(|bytes| *bytes.borrow());
    ArenaStats {
        allocated_bytes: allocated,
    }
}

/// Arena statistics for debugging/monitoring
#[derive(Debug, Clone, Copy)]
pub struct ArenaStats {
    pub allocated_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arena_reset() {
        arena_reset(); // Start fresh

        // Allocate some strings via with_arena
        with_arena(|arena| {
            let _s1 = arena.alloc_str("Hello");
            let _s2 = arena.alloc_str("World");
        });

        let stats_before = arena_stats();
        assert!(stats_before.allocated_bytes > 0);

        // Reset arena
        arena_reset();

        let stats_after = arena_stats();
        // After reset, allocated bytes should be much less than before
        // (Bump might keep some internal overhead, so we don't assert == 0)
        assert!(
            stats_after.allocated_bytes < stats_before.allocated_bytes,
            "Arena should have less memory after reset (before: {}, after: {})",
            stats_before.allocated_bytes,
            stats_after.allocated_bytes
        );
    }

    #[test]
    fn test_with_arena() {
        arena_reset(); // Start fresh

        // We can't return the &str from the closure (lifetime issue)
        // Instead, test that allocation works and stats update
        let len = with_arena(|arena| {
            let s = arena.alloc_str("Test string");
            assert_eq!(s, "Test string");
            s.len()
        });

        assert_eq!(len, 11);

        let stats = arena_stats();
        assert!(stats.allocated_bytes > 0);
    }

    #[test]
    fn test_auto_reset_threshold() {
        arena_reset(); // Start fresh

        // Allocate just under threshold
        let big_str = "x".repeat(ARENA_RESET_THRESHOLD / 2);
        with_arena(|arena| {
            let _s = arena.alloc_str(&big_str);
        });

        let stats1 = arena_stats();
        let initial_bytes = stats1.allocated_bytes;
        assert!(initial_bytes > 0);

        // Allocate more to exceed threshold - this should trigger auto-reset
        let big_str2 = "y".repeat(ARENA_RESET_THRESHOLD / 2 + 1000);
        with_arena(|arena| {
            let _s = arena.alloc_str(&big_str2);
        });

        // Arena should have been reset and re-allocated with just the second string
        let stats2 = arena_stats();
        // After reset, we should only have the second allocation
        // (which is slightly larger than ARENA_RESET_THRESHOLD / 2)
        assert!(
            stats2.allocated_bytes < initial_bytes + (ARENA_RESET_THRESHOLD / 2 + 2000),
            "Arena should have reset: stats2={}, initial={}, threshold={}",
            stats2.allocated_bytes,
            initial_bytes,
            ARENA_RESET_THRESHOLD
        );
    }
}
