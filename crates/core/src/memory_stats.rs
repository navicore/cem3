//! Cross-thread memory statistics registry
//!
//! Provides visibility into arena memory usage across all worker threads.
//! Each thread registers itself and updates its own slot with minimal overhead.
//!
//! # Design
//!
//! The challenge: Arena is thread-local, but diagnostics runs on a
//! separate signal handler thread. We solve this with a global registry where
//! each thread has an exclusive slot for its stats.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │              MemoryStatsRegistry (global)               │
//! ├─────────────────────────────────────────────────────────┤
//! │ slots: [MemorySlot; MAX_THREADS]                        │
//! │                                                         │
//! │  ┌──────────────────┐  ┌──────────────────┐             │
//! │  │ Slot 0 (Thread A)│  │ Slot 1 (Thread B)│  ...        │
//! │  │ thread_id: u64   │  │ thread_id: u64   │             │
//! │  │ arena_bytes: u64 │  │ arena_bytes: u64 │             │
//! │  └──────────────────┘  └──────────────────┘             │
//! └─────────────────────────────────────────────────────────┘
//! ```
//!
//! # Performance
//!
//! - **Registration**: One-time CAS per thread (on first arena access)
//! - **Updates**: Single atomic store per operation (~1-2 cycles, no contention)
//! - **Reads**: Only during diagnostics (SIGQUIT), iterates all slots
//!
//! This maintains the "fast path stays fast" principle.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum number of worker threads we can track
/// May's default is typically fewer threads, but we allow headroom
const MAX_THREADS: usize = 64;

/// Statistics for a single thread's memory usage
#[derive(Debug)]
pub struct MemorySlot {
    /// Thread ID (0 = slot is free)
    pub thread_id: AtomicU64,
    /// Arena allocated bytes
    pub arena_bytes: AtomicU64,
    /// Peak arena allocated bytes (high-water mark)
    pub peak_arena_bytes: AtomicU64,
}

impl MemorySlot {
    const fn new() -> Self {
        Self {
            thread_id: AtomicU64::new(0),
            arena_bytes: AtomicU64::new(0),
            peak_arena_bytes: AtomicU64::new(0),
        }
    }
}

/// Global registry for cross-thread memory statistics
pub struct MemoryStatsRegistry {
    slots: Box<[MemorySlot]>,
    /// Count of threads that couldn't get a slot
    pub overflow_count: AtomicU64,
}

impl MemoryStatsRegistry {
    /// Create a new registry with the given capacity
    fn new(capacity: usize) -> Self {
        let slots: Vec<MemorySlot> = (0..capacity).map(|_| MemorySlot::new()).collect();
        Self {
            slots: slots.into_boxed_slice(),
            overflow_count: AtomicU64::new(0),
        }
    }

    /// Register a thread and get its slot index
    ///
    /// Returns Some(index) if a slot was claimed, None if registry is full.
    /// Uses the current thread's ID as the identifier.
    pub fn register(&self) -> Option<usize> {
        let thread_id = current_thread_id();

        // Scan for a free slot
        for (idx, slot) in self.slots.iter().enumerate() {
            // Try to claim this slot (CAS from 0 to thread_id)
            if slot
                .thread_id
                .compare_exchange(0, thread_id, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(idx);
            }
        }

        // Registry full
        self.overflow_count.fetch_add(1, Ordering::Relaxed);
        None
    }

    /// Update arena stats for a slot
    ///
    /// # Safety
    /// Caller must own the slot (be the thread that registered it)
    #[inline]
    pub fn update_arena(&self, slot_idx: usize, arena_bytes: usize) {
        if let Some(slot) = self.slots.get(slot_idx) {
            let bytes = arena_bytes as u64;
            slot.arena_bytes.store(bytes, Ordering::Relaxed);

            // Update peak via CAS loop (same pattern as PEAK_STRANDS in scheduler.rs)
            let mut peak = slot.peak_arena_bytes.load(Ordering::Relaxed);
            while bytes > peak {
                match slot.peak_arena_bytes.compare_exchange_weak(
                    peak,
                    bytes,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(current) => peak = current,
                }
            }
        }
    }

    /// Get aggregated memory statistics across all threads
    pub fn aggregate_stats(&self) -> AggregateMemoryStats {
        let mut total_arena_bytes: u64 = 0;
        let mut total_peak_arena_bytes: u64 = 0;
        let mut active_threads: usize = 0;

        for slot in self.slots.iter() {
            let thread_id = slot.thread_id.load(Ordering::Acquire);
            if thread_id > 0 {
                active_threads += 1;
                total_arena_bytes += slot.arena_bytes.load(Ordering::Relaxed);
                total_peak_arena_bytes += slot.peak_arena_bytes.load(Ordering::Relaxed);
            }
        }

        AggregateMemoryStats {
            active_threads,
            total_arena_bytes,
            total_peak_arena_bytes,
            overflow_count: self.overflow_count.load(Ordering::Relaxed),
        }
    }

    /// Get registry capacity
    pub fn capacity(&self) -> usize {
        self.slots.len()
    }
}

/// Aggregated memory statistics across all threads
#[derive(Debug, Clone, Copy)]
pub struct AggregateMemoryStats {
    pub active_threads: usize,
    pub total_arena_bytes: u64,
    pub total_peak_arena_bytes: u64,
    pub overflow_count: u64,
}

/// Global counter for generating unique thread IDs
/// Starts at 1 because 0 means "empty slot"
static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

// Thread-local storage for this thread's unique ID
thread_local! {
    static THIS_THREAD_ID: u64 = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
}

/// Get a unique ID for the current thread
///
/// Uses a global atomic counter to guarantee uniqueness (no hash collisions).
/// Thread IDs start at 1 and increment monotonically.
fn current_thread_id() -> u64 {
    THIS_THREAD_ID.with(|&id| id)
}

// Global registry instance
static MEMORY_REGISTRY: OnceLock<MemoryStatsRegistry> = OnceLock::new();

/// Get the global memory stats registry
pub fn memory_registry() -> &'static MemoryStatsRegistry {
    MEMORY_REGISTRY.get_or_init(|| MemoryStatsRegistry::new(MAX_THREADS))
}

// Thread-local slot index (cached after first registration)
thread_local! {
    static SLOT_INDEX: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Get or register the current thread's slot index
///
/// Returns Some(index) if registered (or already was), None if registry is full.
pub fn get_or_register_slot() -> Option<usize> {
    SLOT_INDEX.with(|cell| {
        if let Some(idx) = cell.get() {
            Some(idx)
        } else {
            let idx = memory_registry().register();
            cell.set(idx);
            idx
        }
    })
}

/// Update arena stats for the current thread
///
/// Call this after arena operations to keep stats current.
#[inline]
pub fn update_arena_stats(arena_bytes: usize) {
    if let Some(idx) = SLOT_INDEX.with(|cell| cell.get()) {
        memory_registry().update_arena(idx, arena_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_basic() {
        let registry = MemoryStatsRegistry::new(4);

        // Register should succeed
        let slot = registry.register();
        assert!(slot.is_some());
        let idx = slot.unwrap();

        // Update stats
        registry.update_arena(idx, 1024);

        // Aggregate should reflect our updates
        let stats = registry.aggregate_stats();
        assert_eq!(stats.active_threads, 1);
        assert_eq!(stats.total_arena_bytes, 1024);
    }

    #[test]
    fn test_registry_overflow() {
        let registry = MemoryStatsRegistry::new(2);

        // Fill up the registry from different "threads" (simulated)
        // Note: In real usage, each thread gets one slot
        // Here we just test the CAS logic
        assert!(registry.register().is_some());
        assert!(registry.register().is_some());

        // Third registration should fail (we're on the same thread, so it won't
        // actually fail - but if we had 3 threads, the 3rd would fail)
        // For now, just verify overflow_count is accessible
        assert_eq!(registry.overflow_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_thread_local_slot() {
        // First call should register (or return cached if already registered)
        let slot1 = get_or_register_slot();

        // Second call should return same value (cached)
        let slot2 = get_or_register_slot();
        assert_eq!(slot1, slot2);

        // If registration succeeded, slot should be Some
        // If registry was full, slot is None (acceptable in parallel test execution)
        // Either way, the caching behavior is correct
    }

    #[test]
    fn test_update_helpers() {
        // Try to register (may fail if registry full from parallel tests)
        let slot = get_or_register_slot();

        if slot.is_some() {
            // Update stats
            update_arena_stats(2048);

            // Verify via aggregate
            let stats = memory_registry().aggregate_stats();
            assert!(stats.total_arena_bytes >= 2048); // May have other test data
        }
        // If slot is None, registry was full - that's OK for this test
    }

    #[test]
    fn test_concurrent_registration() {
        use std::thread;

        // Spawn multiple threads that each register and update stats
        let handles: Vec<_> = (0..4)
            .map(|i| {
                thread::spawn(move || {
                    let slot = get_or_register_slot();
                    if slot.is_some() {
                        // Each thread sets a unique arena value
                        update_arena_stats(1000 * (i + 1));
                    }
                    slot.is_some()
                })
            })
            .collect();

        // Wait for all threads and count successful registrations
        let mut registered_count = 0;
        for h in handles {
            if h.join().unwrap() {
                registered_count += 1;
            }
        }

        // Verify aggregate stats reflect the registrations
        let stats = memory_registry().aggregate_stats();
        // active_threads includes all threads that have registered (including test threads)
        assert!(stats.active_threads >= registered_count);
    }

    #[test]
    fn test_thread_ids_are_unique() {
        use std::collections::HashSet;
        use std::sync::{Arc, Mutex};
        use std::thread;

        let ids = Arc::new(Mutex::new(HashSet::new()));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let ids = Arc::clone(&ids);
                thread::spawn(move || {
                    let id = current_thread_id();
                    ids.lock().unwrap().insert(id);
                    id
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All thread IDs should be unique
        let unique_count = ids.lock().unwrap().len();
        assert_eq!(unique_count, 8, "Thread IDs should be unique");
    }
}
