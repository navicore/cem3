//! Scheduler - Green Thread Management with May
//!
//! CSP-style concurrency for cem3 using May coroutines.
//! Each strand is a lightweight green thread that can communicate via channels.
//!
//! ## Non-Blocking Guarantee
//!
//! Channel operations (`send`, `receive`) use May's cooperative blocking and NEVER
//! block OS threads. However, I/O operations (`write_line`, `read_line` in io.rs)
//! currently use blocking syscalls. Future work will make all I/O non-blocking.
//!
//! ## Panic Behavior
//!
//! Functions panic on invalid input (null stacks, negative IDs, closed channels).
//! In a production system, consider implementing error channels or Result-based
//! error handling instead of panicking.

use crate::pool;
use crate::stack::{Stack, StackNode};
use may::coroutine;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, Once};

static SCHEDULER_INIT: Once = Once::new();

// Strand lifecycle tracking
//
// Design rationale:
// - ACTIVE_STRANDS: Lock-free atomic counter for the hot path (spawn/complete)
//   Every strand increments on spawn, decrements on complete. This is extremely
//   fast (lock-free atomic ops) and suitable for high-frequency operations.
//
// - SHUTDOWN_CONDVAR/MUTEX: Event-driven synchronization for the cold path (shutdown wait)
//   Used only when waiting for all strands to complete (program shutdown).
//   Condvar provides event-driven wakeup instead of polling, which is critical
//   for a systems language - no CPU waste, proper OS-level blocking.
//
// Why not track JoinHandles?
// Strands are like Erlang processes - potentially hundreds of thousands of concurrent
// entities with independent lifecycles. Storing handles would require global mutable
// state with synchronization overhead on the hot path. The counter + condvar approach
// keeps the hot path lock-free while providing proper shutdown synchronization.
static ACTIVE_STRANDS: AtomicUsize = AtomicUsize::new(0);
static SHUTDOWN_CONDVAR: Condvar = Condvar::new();
static SHUTDOWN_MUTEX: Mutex<()> = Mutex::new(());

// Unique strand ID generation
static NEXT_STRAND_ID: AtomicU64 = AtomicU64::new(1);

/// Initialize the scheduler
///
/// # Safety
/// Safe to call multiple times (idempotent via Once).
/// May coroutines auto-initialize, so this is primarily a no-op marker.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scheduler_init() {
    SCHEDULER_INIT.call_once(|| {
        // May coroutines auto-initialize, no explicit setup needed
    });
}

/// Run the scheduler and wait for all coroutines to complete
///
/// # Safety
/// Returns the final stack (always null for now since May handles all scheduling).
/// This function blocks until all spawned strands have completed.
///
/// Uses a condition variable for event-driven shutdown synchronization rather than
/// polling. The mutex is only held during the wait protocol, not during strand
/// execution, so there's no contention on the hot path.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scheduler_run() -> Stack {
    let mut guard = SHUTDOWN_MUTEX.lock().expect(
        "scheduler_run: shutdown mutex poisoned - strand panicked during shutdown synchronization",
    );

    // Wait for all strands to complete
    // The condition variable will be notified when the last strand exits
    while ACTIVE_STRANDS.load(Ordering::Acquire) > 0 {
        guard = SHUTDOWN_CONDVAR
            .wait(guard)
            .expect("scheduler_run: condvar wait failed - strand panicked during shutdown wait");
    }

    // All strands have completed
    std::ptr::null_mut()
}

/// Shutdown the scheduler
///
/// # Safety
/// Safe to call. May doesn't require explicit shutdown, so this is a no-op.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scheduler_shutdown() {
    // May doesn't require explicit shutdown
    // This function exists for API symmetry with init
}

/// Spawn a strand (coroutine) with initial stack
///
/// # Safety
/// - `entry` must be a valid function pointer that can safely execute on any thread
/// - `initial_stack` must be either null or a valid pointer to a `StackNode` that:
///   - Was heap-allocated (e.g., via Box)
///   - Has a 'static lifetime or lives longer than the coroutine
///   - Is safe to access from the spawned thread
/// - The caller transfers ownership of `initial_stack` to the coroutine
/// - Returns a unique strand ID (positive integer)
///
/// # Memory Management
/// The spawned coroutine takes ownership of `initial_stack` and will automatically
/// free the final stack returned by `entry` upon completion.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strand_spawn(
    entry: extern "C" fn(Stack) -> Stack,
    initial_stack: Stack,
) -> i64 {
    // Generate unique strand ID
    let strand_id = NEXT_STRAND_ID.fetch_add(1, Ordering::Relaxed);

    // Increment active strand counter
    ACTIVE_STRANDS.fetch_add(1, Ordering::Release);

    // Function pointers are already Send, no wrapper needed
    let entry_fn = entry;

    // Convert pointer to usize (which is Send)
    // This is necessary because *mut T is !Send, but the caller guarantees thread safety
    let stack_addr = initial_stack as usize;

    unsafe {
        coroutine::spawn(move || {
            // Reconstruct pointer from address
            let stack_ptr = stack_addr as *mut StackNode;

            // Debug assertion: validate stack pointer alignment and reasonable address
            debug_assert!(
                stack_ptr.is_null() || stack_addr.is_multiple_of(std::mem::align_of::<StackNode>()),
                "Stack pointer must be null or properly aligned"
            );
            debug_assert!(
                stack_ptr.is_null() || stack_addr > 0x1000,
                "Stack pointer appears to be in invalid memory region (< 0x1000)"
            );

            // Execute the entry function
            let final_stack = entry_fn(stack_ptr);

            // Clean up the final stack to prevent memory leak
            free_stack(final_stack);

            // Decrement active strand counter
            // If this was the last strand, notify anyone waiting for shutdown
            // Use AcqRel to establish proper synchronization (both acquire and release barriers)
            let prev_count = ACTIVE_STRANDS.fetch_sub(1, Ordering::AcqRel);
            if prev_count == 1 {
                // We were the last strand - acquire mutex and signal shutdown
                // The mutex must be held when calling notify to prevent missed wakeups
                let _guard = SHUTDOWN_MUTEX.lock()
                    .expect("strand_spawn: shutdown mutex poisoned - strand panicked during shutdown notification");
                SHUTDOWN_CONDVAR.notify_all();
            }
        });
    }

    strand_id as i64
}

/// Free a stack allocated by the runtime
///
/// # Safety
/// - `stack` must be either:
///   - A null pointer (safe, will be a no-op)
///   - A valid pointer returned by runtime stack functions (push, etc.)
/// - The pointer must not have been previously freed
/// - After calling this function, the pointer is invalid and must not be used
/// - This function takes ownership and returns nodes to the pool
///
/// # Performance
/// Returns nodes to thread-local pool for reuse instead of freeing to heap
fn free_stack(mut stack: Stack) {
    if !stack.is_null() {
        use crate::value::Value;
        unsafe {
            // Walk the stack and return each node to the pool
            while !stack.is_null() {
                let next = (*stack).next;
                // Drop the value, then return node to pool
                // We need to drop the value to free any heap allocations (String, Variant)
                drop(std::mem::replace(&mut (*stack).value, Value::Int(0)));
                // Return node to pool for reuse
                pool::pool_free(stack);
                stack = next;
            }
        }
    }

    // Reset the thread-local arena to free all arena-allocated strings
    // This is safe because:
    // - Any arena strings in Values have been dropped above
    // - Global strings are unaffected (they have their own allocations)
    // - Channel sends clone to global, so no cross-strand arena pointers
    crate::arena::arena_reset();
}

/// Legacy spawn_strand function (kept for compatibility)
///
/// # Safety
/// `entry` must be a valid function pointer that can safely execute on any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spawn_strand(entry: extern "C" fn(Stack) -> Stack) {
    unsafe {
        strand_spawn(entry, std::ptr::null_mut());
    }
}

/// Yield execution to allow other coroutines to run
///
/// # Safety
/// Always safe to call from within a May coroutine.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn yield_strand() {
    coroutine::yield_now();
}

/// Wait for all strands to complete
///
/// # Safety
/// Always safe to call. Blocks until all spawned strands have completed.
///
/// Uses event-driven synchronization via condition variable - no polling overhead.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wait_all_strands() {
    let mut guard = SHUTDOWN_MUTEX.lock()
        .expect("wait_all_strands: shutdown mutex poisoned - strand panicked during shutdown synchronization");

    // Wait for all strands to complete
    // The condition variable will be notified when the last strand exits
    while ACTIVE_STRANDS.load(Ordering::Acquire) > 0 {
        guard = SHUTDOWN_CONDVAR
            .wait(guard)
            .expect("wait_all_strands: condvar wait failed - strand panicked during shutdown wait");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::push;
    use crate::value::Value;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_spawn_strand() {
        unsafe {
            static COUNTER: AtomicU32 = AtomicU32::new(0);

            extern "C" fn test_entry(_stack: Stack) -> Stack {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                std::ptr::null_mut()
            }

            for _ in 0..100 {
                spawn_strand(test_entry);
            }

            std::thread::sleep(std::time::Duration::from_millis(200));
            assert_eq!(COUNTER.load(Ordering::SeqCst), 100);
        }
    }

    #[test]
    fn test_scheduler_init_idempotent() {
        unsafe {
            // Should be safe to call multiple times
            scheduler_init();
            scheduler_init();
            scheduler_init();
        }
    }

    #[test]
    fn test_free_stack_null() {
        // Freeing null should be a no-op
        free_stack(std::ptr::null_mut());
    }

    #[test]
    fn test_free_stack_valid() {
        unsafe {
            // Create a stack, then free it
            let stack = push(std::ptr::null_mut(), Value::Int(42));
            free_stack(stack);
            // If we get here without crashing, test passed
        }
    }

    #[test]
    fn test_strand_spawn_with_stack() {
        unsafe {
            static COUNTER: AtomicU32 = AtomicU32::new(0);

            extern "C" fn test_entry(stack: Stack) -> Stack {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                // Return the stack as-is (caller will free it)
                stack
            }

            let initial_stack = push(std::ptr::null_mut(), Value::Int(99));
            strand_spawn(test_entry, initial_stack);

            std::thread::sleep(std::time::Duration::from_millis(200));
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn test_scheduler_shutdown() {
        unsafe {
            scheduler_init();
            scheduler_shutdown();
            // Should not crash
        }
    }

    #[test]
    fn test_many_strands_stress() {
        unsafe {
            static COUNTER: AtomicU32 = AtomicU32::new(0);

            extern "C" fn increment(_stack: Stack) -> Stack {
                COUNTER.fetch_add(1, Ordering::SeqCst);
                std::ptr::null_mut()
            }

            // Reset counter for this test
            COUNTER.store(0, Ordering::SeqCst);

            // Spawn many strands to stress test synchronization
            for _ in 0..1000 {
                strand_spawn(increment, std::ptr::null_mut());
            }

            // Wait for all to complete
            wait_all_strands();

            // Verify all strands executed
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1000);
        }
    }

    #[test]
    fn test_strand_ids_are_unique() {
        unsafe {
            use std::collections::HashSet;

            extern "C" fn noop(_stack: Stack) -> Stack {
                std::ptr::null_mut()
            }

            // Spawn strands and collect their IDs
            let mut ids = Vec::new();
            for _ in 0..100 {
                let id = strand_spawn(noop, std::ptr::null_mut());
                ids.push(id);
            }

            // Wait for completion
            wait_all_strands();

            // Verify all IDs are unique
            let unique_ids: HashSet<_> = ids.iter().collect();
            assert_eq!(unique_ids.len(), 100, "All strand IDs should be unique");

            // Verify all IDs are positive
            assert!(
                ids.iter().all(|&id| id > 0),
                "All strand IDs should be positive"
            );
        }
    }

    #[test]
    fn test_arena_reset_with_strands() {
        unsafe {
            use crate::arena;
            use crate::cemstring::{arena_string, global_string};

            extern "C" fn create_temp_strings(stack: Stack) -> Stack {
                // Create many temporary arena strings (simulating request parsing)
                for i in 0..100 {
                    let temp = arena_string(&format!("temporary string {}", i));
                    // Use the string temporarily
                    assert!(!temp.as_str().is_empty());
                    // String is dropped, but memory stays in arena
                }

                // Arena should have allocated memory
                let stats = arena::arena_stats();
                assert!(stats.allocated_bytes > 0, "Arena should have allocations");

                stack // Return empty stack
            }

            // Reset arena before test
            arena::arena_reset();

            // Spawn strand that creates many temp strings
            strand_spawn(create_temp_strings, std::ptr::null_mut());

            // Wait for strand to complete (which calls free_stack -> arena_reset)
            wait_all_strands();

            // After strand exits, arena should be reset
            let stats_after = arena::arena_stats();
            assert_eq!(
                stats_after.allocated_bytes, 0,
                "Arena should be reset after strand exits"
            );
        }
    }

    #[test]
    fn test_arena_with_channel_send() {
        unsafe {
            use crate::cemstring::{arena_string, global_string};
            use crate::channel::{close_channel, make_channel};
            use crate::stack::{pop, push};
            use crate::value::Value;
            use std::sync::atomic::{AtomicU32, Ordering};

            static RECEIVED_COUNT: AtomicU32 = AtomicU32::new(0);

            // Create channel
            let stack = std::ptr::null_mut();
            let stack = make_channel(stack);
            let (stack, chan_val) = pop(stack);
            let chan_id = match chan_val {
                Value::Int(id) => id,
                _ => panic!("Expected channel ID"),
            };

            // Sender strand: creates arena string, sends through channel
            extern "C" fn sender(stack: Stack) -> Stack {
                use crate::cemstring::arena_string;
                use crate::channel::send;
                use crate::stack::{pop, push};
                use crate::value::Value;

                unsafe {
                    // Extract channel ID from stack
                    let (stack, chan_val) = pop(stack);
                    let chan_id = match chan_val {
                        Value::Int(id) => id,
                        _ => panic!("Expected channel ID"),
                    };

                    // Create arena string
                    let msg = arena_string("Hello from sender!");

                    // Push string and channel ID for send
                    let stack = push(stack, Value::String(msg));
                    let stack = push(stack, Value::Int(chan_id));

                    // Send (will clone to global)
                    send(stack)
                }
            }

            // Receiver strand: receives string from channel
            extern "C" fn receiver(stack: Stack) -> Stack {
                use crate::channel::receive;
                use crate::stack::{pop, push};
                use crate::value::Value;
                use std::sync::atomic::Ordering;

                unsafe {
                    // Extract channel ID from stack
                    let (stack, chan_val) = pop(stack);
                    let chan_id = match chan_val {
                        Value::Int(id) => id,
                        _ => panic!("Expected channel ID"),
                    };

                    // Push channel ID for receive
                    let stack = push(stack, Value::Int(chan_id));

                    // Receive message
                    let stack = receive(stack);

                    // Pop and verify message
                    let (stack, msg_val) = pop(stack);
                    match msg_val {
                        Value::String(s) => {
                            assert_eq!(s.as_str(), "Hello from sender!");
                            RECEIVED_COUNT.fetch_add(1, Ordering::SeqCst);
                        }
                        _ => panic!("Expected String"),
                    }

                    stack
                }
            }

            // Spawn sender and receiver
            let sender_stack = push(std::ptr::null_mut(), Value::Int(chan_id));
            strand_spawn(sender, sender_stack);

            let receiver_stack = push(std::ptr::null_mut(), Value::Int(chan_id));
            strand_spawn(receiver, receiver_stack);

            // Wait for both strands
            wait_all_strands();

            // Verify message was received
            assert_eq!(
                RECEIVED_COUNT.load(Ordering::SeqCst),
                1,
                "Receiver should have received message"
            );

            // Clean up channel
            let stack = push(stack, Value::Int(chan_id));
            close_channel(stack);
        }
    }
}
