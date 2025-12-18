//! Channel operations for CSP-style concurrency
//!
//! Channels are the primary communication mechanism between strands.
//! They use May's MPMC channels with cooperative blocking.
//!
//! ## Non-Blocking Guarantee
//!
//! All channel operations (`send`, `receive`) cooperatively block using May's scheduler.
//! They NEVER block OS threads - May handles scheduling other strands while waiting.
//!
//! ## Multi-Consumer Support
//!
//! Channels support multiple producers AND multiple consumers (MPMC). Multiple strands
//! can receive from the same channel concurrently - each message is delivered to exactly
//! one receiver (work-stealing semantics).
//!
//! ## Error Handling
//!
//! Two variants are available for send/receive:
//!
//! - `send` / `receive` - Panic on errors (closed channel, invalid ID)
//! - `send-safe` / `receive-safe` - Return success flag instead of panicking
//!
//! The safe variants enable graceful shutdown patterns:
//! ```seq
//! value channel-id send-safe if
//!   # sent successfully
//! else
//!   # channel closed, handle gracefully
//! then
//! ```

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use may::sync::mpmc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Once};

/// Unique channel ID generation
static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

/// Global channel registry
/// Maps channel IDs to sender/receiver pairs
static CHANNEL_REGISTRY: Mutex<Option<HashMap<u64, ChannelPair>>> = Mutex::new(None);

/// Initialize the channel registry exactly once (lock-free after first call)
static REGISTRY_INIT: Once = Once::new();

/// Per-channel statistics (wrapped in Arc for lock-free access)
#[derive(Debug)]
struct ChannelStatsInner {
    /// Lifetime count of messages sent (monotonic)
    send_count: AtomicU64,
    /// Lifetime count of messages received (monotonic)
    receive_count: AtomicU64,
}

/// A channel pair (sender and receiver) with statistics
/// Both sender and receiver are cloneable (MPMC) - no mutex needed
/// Stats are Arc<> to allow updating after releasing the registry lock
struct ChannelPair {
    sender: mpmc::Sender<Value>,
    receiver: mpmc::Receiver<Value>,
    stats: Arc<ChannelStatsInner>,
}

/// Initialize the channel registry (lock-free after first call)
fn init_registry() {
    REGISTRY_INIT.call_once(|| {
            let mut guard = CHANNEL_REGISTRY.lock()
                .expect("init_registry: channel registry lock poisoned during initialization - strand panicked while holding lock");
        *guard = Some(HashMap::new());
    });
}

/// Get the number of open channels (for diagnostics)
///
/// Returns None if the registry lock is held (to avoid blocking in signal handler).
/// This is a best-effort diagnostic - the count may be slightly stale.
pub fn channel_count() -> Option<usize> {
    // Use try_lock to avoid blocking in signal handler context
    match CHANNEL_REGISTRY.try_lock() {
        Ok(guard) => guard.as_ref().map(|registry| registry.len()),
        Err(_) => None, // Lock held, return None rather than block
    }
}

/// Per-channel statistics for diagnostics
#[derive(Debug, Clone)]
pub struct ChannelStats {
    /// Channel ID
    pub id: u64,
    /// Current queue depth (sends - receives)
    pub queue_depth: u64,
    /// Lifetime count of messages sent
    pub send_count: u64,
    /// Lifetime count of messages received
    pub receive_count: u64,
}

/// Get per-channel statistics for all open channels (for diagnostics)
///
/// Returns None if the registry lock is held (to avoid blocking in signal handler).
/// Returns an empty Vec if no channels are open.
///
/// Queue depth is computed as send_count - receive_count. Due to the lock-free
/// nature of the counters, there may be brief inconsistencies (e.g., depth < 0
/// is clamped to 0), but this is acceptable for monitoring purposes.
pub fn channel_stats() -> Option<Vec<ChannelStats>> {
    // Use try_lock to avoid blocking in signal handler context
    match CHANNEL_REGISTRY.try_lock() {
        Ok(guard) => {
            guard.as_ref().map(|registry| {
                registry
                    .iter()
                    .map(|(&id, pair)| {
                        let send_count = pair.stats.send_count.load(Ordering::Relaxed);
                        let receive_count = pair.stats.receive_count.load(Ordering::Relaxed);
                        // Queue depth = sends - receives, clamped to 0
                        let queue_depth = send_count.saturating_sub(receive_count);
                        ChannelStats {
                            id,
                            queue_depth,
                            send_count,
                            receive_count,
                        }
                    })
                    .collect()
            })
        }
        Err(_) => None, // Lock held, return None rather than block
    }
}

/// Create a new channel
///
/// Stack effect: ( -- channel_id )
///
/// Returns a channel ID that can be used with send/receive operations.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_channel(stack: Stack) -> Stack {
    init_registry();

    // Create an unbounded MPMC channel
    // May's mpmc::channel() creates coroutine-aware channels with multi-producer, multi-consumer
    // The recv() operation cooperatively blocks (yields) instead of blocking the OS thread
    // Both sender and receiver are Clone - no mutex needed for sharing
    let (sender, receiver) = mpmc::channel();

    let channel_id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);

    // Store in registry
    let mut guard = CHANNEL_REGISTRY.lock().expect(
        "make_channel: channel registry lock poisoned - strand panicked while holding lock",
    );

    let registry = guard
        .as_mut()
        .expect("make_channel: channel registry not initialized - call init_registry first");

    registry.insert(
        channel_id,
        ChannelPair {
            sender,
            receiver,
            stats: Arc::new(ChannelStatsInner {
                send_count: AtomicU64::new(0),
                receive_count: AtomicU64::new(0),
            }),
        },
    );

    // Push channel ID onto stack
    unsafe { push(stack, Value::Int(channel_id as i64)) }
}

/// Send a value through a channel
///
/// Stack effect: ( value channel_id -- )
///
/// Blocks the strand if the channel is full until space becomes available.
/// This is cooperative blocking - the strand yields and May handles scheduling.
///
/// # Safety
/// Stack must have a channel ID (Int) on top and a value below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_send(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "send: stack is empty");

    // Pop channel ID
    let (stack, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => {
            if id < 0 {
                panic!("send: channel ID must be positive, got {}", id);
            }
            id as u64
        }
        _ => panic!("send: expected channel ID (Int) on stack"),
    };

    assert!(!stack.is_null(), "send: stack has only one value");

    // Pop value to send
    let (rest, value) = unsafe { pop(stack) };

    // Get sender from registry
    let guard = CHANNEL_REGISTRY
        .lock()
        .expect("send: channel registry lock poisoned - strand panicked while holding lock");

    let registry = guard
        .as_ref()
        .expect("send: channel registry not initialized - call init_registry first");

    let pair = match registry.get(&channel_id) {
        Some(p) => p,
        None => panic!("send: invalid channel ID {}", channel_id),
    };

    // Clone the sender and stats so we can use them outside the lock
    let sender = pair.sender.clone();
    let stats = Arc::clone(&pair.stats);
    drop(guard); // Release lock before potentially blocking

    // Clone the value before sending to ensure arena strings are promoted to global
    // CemString::clone() allocates from global heap (see cemstring.rs:75-78)
    // This prevents use-after-free when sender's arena is reset before receiver accesses the string
    let global_value = value.clone();

    // Send the value (may block if channel is full)
    // May's scheduler will handle the blocking cooperatively
    sender.send(global_value).expect("send: channel closed");

    // Update stats after successful send
    stats.send_count.fetch_add(1, Ordering::Relaxed);

    rest
}

/// Receive a value from a channel
///
/// Stack effect: ( channel_id -- value )
///
/// Blocks the strand until a value is available.
/// This is cooperative blocking - the strand yields and May handles scheduling.
///
/// ## Multi-Consumer Support
///
/// Multiple strands can receive from the same channel concurrently (MPMC).
/// Each message is delivered to exactly one receiver (work-stealing semantics).
/// No serialization - strands compete fairly for messages.
///
/// # Safety
/// Stack must have a channel ID (Int) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_receive(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "receive: stack is empty");

    // Pop channel ID
    let (rest, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => {
            if id < 0 {
                panic!("receive: channel ID must be positive, got {}", id);
            }
            id as u64
        }
        _ => panic!("receive: expected channel ID (Int) on stack"),
    };

    // Clone receiver and stats from registry (don't hold lock during recv!)
    // MPMC receiver is Clone - no mutex needed
    let (receiver, stats) = {
        let guard = CHANNEL_REGISTRY
            .lock()
            .expect("receive: channel registry lock poisoned - strand panicked while holding lock");

        let registry = guard
            .as_ref()
            .expect("receive: channel registry not initialized - call init_registry first");

        let pair = match registry.get(&channel_id) {
            Some(p) => p,
            None => panic!("receive: invalid channel ID {}", channel_id),
        };

        (pair.receiver.clone(), Arc::clone(&pair.stats))
    }; // Registry lock released here!

    // Receive a value (cooperatively blocks the strand until available)
    // May's recv() yields to the scheduler, not blocking the OS thread
    // Multiple strands can wait concurrently - MPMC handles synchronization
    let value = match receiver.recv() {
        Ok(v) => v,
        Err(_) => panic!("receive: channel closed"),
    };

    // Update stats after successful receive
    stats.receive_count.fetch_add(1, Ordering::Relaxed);

    unsafe { push(rest, value) }
}

/// Close a channel and remove it from the registry
///
/// Stack effect: ( channel_id -- )
///
/// After closing, send/receive operations on this channel will fail.
///
/// # Safety
/// Stack must have a channel ID (Int) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_close_channel(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "close_channel: stack is empty");

    // Pop channel ID
    let (rest, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => {
            if id < 0 {
                panic!("close_channel: channel ID must be positive, got {}", id);
            }
            id as u64
        }
        _ => panic!("close_channel: expected channel ID (Int) on stack"),
    };

    // Remove from registry
    let mut guard = CHANNEL_REGISTRY.lock().expect(
        "close_channel: channel registry lock poisoned - strand panicked while holding lock",
    );

    let registry = guard
        .as_mut()
        .expect("close_channel: channel registry not initialized - call init_registry first");

    registry.remove(&channel_id);

    rest
}

/// Send a value through a channel, with error handling
///
/// Stack effect: ( value channel_id -- success_flag )
///
/// Returns 1 on success, 0 on failure (closed channel or invalid ID).
/// Does not panic on errors - returns 0 instead.
///
/// # Safety
/// Stack must have a channel ID (Int) on top and a value below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_send_safe(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "send-safe: stack is empty");

    // Pop channel ID
    let (stack, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => {
            if id < 0 {
                // Invalid channel ID - consume value and return failure
                if !stack.is_null() {
                    let (rest, _value) = unsafe { pop(stack) };
                    return unsafe { push(rest, Value::Int(0)) };
                }
                return unsafe { push(stack, Value::Int(0)) };
            }
            id as u64
        }
        _ => panic!("send-safe: expected channel ID (Int) on stack"),
    };

    if stack.is_null() {
        // No value to send - return failure
        return unsafe { push(stack, Value::Int(0)) };
    }

    // Pop value to send
    let (rest, value) = unsafe { pop(stack) };

    // Get sender and stats from registry
    let (sender, stats) = {
        let guard = match CHANNEL_REGISTRY.lock() {
            Ok(g) => g,
            Err(_) => return unsafe { push(rest, Value::Int(0)) },
        };

        let registry = match guard.as_ref() {
            Some(r) => r,
            None => return unsafe { push(rest, Value::Int(0)) },
        };

        match registry.get(&channel_id) {
            Some(p) => (p.sender.clone(), Arc::clone(&p.stats)),
            None => return unsafe { push(rest, Value::Int(0)) },
        }
    };

    // Clone the value before sending to ensure arena strings are promoted to global
    let global_value = value.clone();

    // Send the value
    match sender.send(global_value) {
        Ok(()) => {
            stats.send_count.fetch_add(1, Ordering::Relaxed);
            unsafe { push(rest, Value::Int(1)) }
        }
        Err(_) => unsafe { push(rest, Value::Int(0)) },
    }
}

/// Receive a value from a channel, with error handling
///
/// Stack effect: ( channel_id -- value success_flag )
///
/// Returns (value, 1) on success, (0, 0) on failure (closed channel or invalid ID).
/// Does not panic on errors - returns (0, 0) instead.
///
/// ## Multi-Consumer Support
///
/// Multiple strands can receive from the same channel concurrently (MPMC).
/// Each message is delivered to exactly one receiver (work-stealing semantics).
///
/// # Safety
/// Stack must have a channel ID (Int) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_receive_safe(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "receive-safe: stack is empty");

    // Pop channel ID
    let (rest, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => {
            if id < 0 {
                // Invalid channel ID - return failure
                let stack = unsafe { push(rest, Value::Int(0)) };
                return unsafe { push(stack, Value::Int(0)) };
            }
            id as u64
        }
        _ => panic!("receive-safe: expected channel ID (Int) on stack"),
    };

    // Clone receiver and stats from registry (MPMC receiver is Clone)
    let (receiver, stats) = {
        let guard = match CHANNEL_REGISTRY.lock() {
            Ok(g) => g,
            Err(_) => {
                let stack = unsafe { push(rest, Value::Int(0)) };
                return unsafe { push(stack, Value::Int(0)) };
            }
        };

        let registry = match guard.as_ref() {
            Some(r) => r,
            None => {
                let stack = unsafe { push(rest, Value::Int(0)) };
                return unsafe { push(stack, Value::Int(0)) };
            }
        };

        match registry.get(&channel_id) {
            Some(p) => (p.receiver.clone(), Arc::clone(&p.stats)),
            None => {
                let stack = unsafe { push(rest, Value::Int(0)) };
                return unsafe { push(stack, Value::Int(0)) };
            }
        }
    };

    // Receive a value - MPMC handles concurrent receivers
    match receiver.recv() {
        Ok(value) => {
            stats.receive_count.fetch_add(1, Ordering::Relaxed);
            let stack = unsafe { push(rest, value) };
            unsafe { push(stack, Value::Int(1)) }
        }
        Err(_) => {
            let stack = unsafe { push(rest, Value::Int(0)) };
            unsafe { push(stack, Value::Int(0)) }
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_chan_receive as receive;
pub use patch_seq_chan_receive_safe as receive_safe;
pub use patch_seq_chan_send as send;
pub use patch_seq_chan_send_safe as send_safe;
pub use patch_seq_close_channel as close_channel;
pub use patch_seq_make_channel as make_channel;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{spawn_strand, wait_all_strands};
    use std::sync::atomic::{AtomicI64, Ordering};

    #[test]
    fn test_make_channel() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_channel(stack);

            // Should have channel ID on stack
            let (stack, value) = pop(stack);
            assert!(matches!(value, Value::Int(_)));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_send_receive() {
        unsafe {
            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);

            // Get channel ID
            let (empty_stack, channel_id_value) = pop(stack);
            assert!(empty_stack.is_null());

            // Push value to send
            let mut stack = push(std::ptr::null_mut(), Value::Int(42));
            stack = push(stack, channel_id_value.clone());
            stack = send(stack);
            assert!(stack.is_null());

            // Receive value
            stack = push(stack, channel_id_value);
            stack = receive(stack);

            // Should have received value
            let (stack, received) = pop(stack);
            assert_eq!(received, Value::Int(42));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_channel_communication_between_strands() {
        unsafe {
            static RECEIVED_VALUE: AtomicI64 = AtomicI64::new(0);

            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);
            let channel_id = match channel_id_value {
                Value::Int(id) => id,
                _ => panic!("Expected Int"),
            };

            // Receiver strand
            extern "C" fn receiver(_stack: Stack) -> Stack {
                unsafe {
                    let channel_id = RECEIVED_VALUE.load(Ordering::Acquire); // Temporary storage
                    let mut stack = push(std::ptr::null_mut(), Value::Int(channel_id));
                    stack = receive(stack);
                    let (_, value) = pop(stack);
                    if let Value::Int(n) = value {
                        RECEIVED_VALUE.store(n, Ordering::Release);
                    }
                    std::ptr::null_mut()
                }
            }

            // Store channel ID temporarily
            RECEIVED_VALUE.store(channel_id, Ordering::Release);

            // Spawn receiver strand
            spawn_strand(receiver);

            // Give receiver time to start
            std::thread::sleep(std::time::Duration::from_millis(10));

            // Send value from main strand
            let mut stack = push(std::ptr::null_mut(), Value::Int(123));
            stack = push(stack, Value::Int(channel_id));
            let _ = send(stack);

            // Wait for all strands
            wait_all_strands();

            // Check received value
            assert_eq!(RECEIVED_VALUE.load(Ordering::Acquire), 123);
        }
    }

    #[test]
    fn test_multiple_sends_receives() {
        unsafe {
            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);

            // Send multiple values
            for i in 1..=5 {
                let mut stack = push(std::ptr::null_mut(), Value::Int(i));
                stack = push(stack, channel_id_value.clone());
                let _ = send(stack);
            }

            // Receive them back in order
            for i in 1..=5 {
                let mut stack = push(std::ptr::null_mut(), channel_id_value.clone());
                stack = receive(stack);
                let (_, received) = pop(stack);
                assert_eq!(received, Value::Int(i));
            }
        }
    }

    #[test]
    fn test_close_channel() {
        unsafe {
            // Create and close a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (rest, channel_id) = pop(stack);

            stack = push(rest, channel_id);
            stack = close_channel(stack);
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_arena_string_send_between_strands() {
        // This test verifies that arena-allocated strings are properly cloned
        // to global storage when sent through channels (fix for issue #13)
        unsafe {
            use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

            static CHANNEL_ID: AtomicI64 = AtomicI64::new(0);
            static VERIFIED: AtomicBool = AtomicBool::new(false);

            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);
            let channel_id = match channel_id_value {
                Value::Int(id) => id,
                _ => panic!("Expected Int"),
            };

            // Store channel ID for strands
            CHANNEL_ID.store(channel_id, Ordering::Release);

            // Sender strand: creates arena string and sends it
            extern "C" fn sender(_stack: Stack) -> Stack {
                use crate::seqstring::arena_string;
                use crate::stack::push;
                use crate::value::Value;
                use std::sync::atomic::Ordering;

                unsafe {
                    let chan_id = CHANNEL_ID.load(Ordering::Acquire);

                    // Create arena string (fast path)
                    let msg = arena_string("Arena message!");
                    assert!(!msg.is_global(), "Should be arena-allocated initially");

                    // Send through channel (will be cloned to global)
                    let stack = push(std::ptr::null_mut(), Value::String(msg));
                    let stack = push(stack, Value::Int(chan_id));
                    send(stack)
                }
            }

            // Receiver strand: receives string and verifies it
            extern "C" fn receiver(_stack: Stack) -> Stack {
                use crate::stack::{pop, push};
                use crate::value::Value;
                use std::sync::atomic::Ordering;

                unsafe {
                    let chan_id = CHANNEL_ID.load(Ordering::Acquire);

                    let mut stack = push(std::ptr::null_mut(), Value::Int(chan_id));
                    stack = receive(stack);
                    let (_, msg_val) = pop(stack);

                    match msg_val {
                        Value::String(s) => {
                            assert_eq!(s.as_str(), "Arena message!");
                            // Verify it was cloned to global
                            assert!(s.is_global(), "Received string should be global");
                            VERIFIED.store(true, Ordering::Release);
                        }
                        _ => panic!("Expected String"),
                    }

                    std::ptr::null_mut()
                }
            }

            // Spawn both strands
            spawn_strand(sender);
            spawn_strand(receiver);

            // Wait for both strands
            wait_all_strands();

            // Verify message was received correctly
            assert!(
                VERIFIED.load(Ordering::Acquire),
                "Receiver should have verified the message"
            );
        }
    }

    // Note: Cannot test negative channel ID panics with #[should_panic] because
    // these are extern "C" functions which cannot unwind. The validation is still
    // in place at runtime - see lines 100-102, 157-159, 217-219.

    #[test]
    fn test_send_safe_success() {
        unsafe {
            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);

            // Send value using send-safe
            let mut stack = push(std::ptr::null_mut(), Value::Int(42));
            stack = push(stack, channel_id_value.clone());
            stack = send_safe(stack);

            // Should return success (1)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());

            // Receive value to verify it was sent
            let mut stack = push(std::ptr::null_mut(), channel_id_value);
            stack = receive(stack);
            let (_, received) = pop(stack);
            assert_eq!(received, Value::Int(42));
        }
    }

    #[test]
    fn test_send_safe_invalid_channel() {
        unsafe {
            // Try to send to invalid channel ID
            let mut stack = push(std::ptr::null_mut(), Value::Int(42));
            stack = push(stack, Value::Int(999999)); // Non-existent channel
            stack = send_safe(stack);

            // Should return failure (0)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_send_safe_negative_channel() {
        unsafe {
            // Try to send to negative channel ID
            let mut stack = push(std::ptr::null_mut(), Value::Int(42));
            stack = push(stack, Value::Int(-1));
            stack = send_safe(stack);

            // Should return failure (0), value consumed per stack effect
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null()); // Value was properly consumed
        }
    }

    #[test]
    fn test_receive_safe_success() {
        unsafe {
            // Create a channel and send a value
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);

            // Send value
            let mut stack = push(std::ptr::null_mut(), Value::Int(42));
            stack = push(stack, channel_id_value.clone());
            let _ = send(stack);

            // Receive using receive-safe
            let mut stack = push(std::ptr::null_mut(), channel_id_value);
            stack = receive_safe(stack);

            // Should return (value, 1)
            let (stack, success) = pop(stack);
            let (stack, value) = pop(stack);
            assert_eq!(success, Value::Int(1));
            assert_eq!(value, Value::Int(42));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_receive_safe_invalid_channel() {
        unsafe {
            // Try to receive from invalid channel ID
            let mut stack = push(std::ptr::null_mut(), Value::Int(999999));
            stack = receive_safe(stack);

            // Should return (0, 0)
            let (stack, success) = pop(stack);
            let (stack, value) = pop(stack);
            assert_eq!(success, Value::Int(0));
            assert_eq!(value, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_receive_safe_closed_channel() {
        unsafe {
            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);
            let channel_id = match &channel_id_value {
                Value::Int(id) => *id,
                _ => panic!("Expected Int"),
            };

            // Close the channel
            let stack = push(std::ptr::null_mut(), channel_id_value);
            let _ = close_channel(stack);

            // Try to receive from closed channel
            let mut stack = push(std::ptr::null_mut(), Value::Int(channel_id));
            stack = receive_safe(stack);

            // Should return (0, 0)
            let (stack, success) = pop(stack);
            let (stack, value) = pop(stack);
            assert_eq!(success, Value::Int(0));
            assert_eq!(value, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    // Helper to get stats with retry (handles parallel test lock contention)
    fn get_stats_with_retry() -> Option<Vec<super::ChannelStats>> {
        for _ in 0..10 {
            if let Some(stats) = super::channel_stats() {
                return Some(stats);
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        None
    }

    #[test]
    fn test_channel_stats() {
        unsafe {
            // Create a channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);
            let channel_id = match &channel_id_value {
                Value::Int(id) => *id as u64,
                _ => panic!("Expected Int"),
            };

            // Initially, stats should show 0 sends and 0 receives
            // Use retry to handle parallel test lock contention
            let stats = match get_stats_with_retry() {
                Some(s) => s,
                None => {
                    // Skip test if we can't get lock after retries (parallel test contention)
                    let stack = push(std::ptr::null_mut(), channel_id_value);
                    let _ = close_channel(stack);
                    return;
                }
            };
            let our_channel = stats.iter().find(|s| s.id == channel_id);
            assert!(our_channel.is_some(), "Our channel should be in stats");
            let stat = our_channel.unwrap();
            assert_eq!(stat.send_count, 0);
            assert_eq!(stat.receive_count, 0);
            assert_eq!(stat.queue_depth, 0);

            // Send some values
            for i in 1..=5 {
                let mut stack = push(std::ptr::null_mut(), Value::Int(i));
                stack = push(stack, channel_id_value.clone());
                let _ = send(stack);
            }

            // Check stats after sends
            let stats = get_stats_with_retry().expect("Should get stats after retries");
            let stat = stats.iter().find(|s| s.id == channel_id).unwrap();
            assert_eq!(stat.send_count, 5);
            assert_eq!(stat.receive_count, 0);
            assert_eq!(stat.queue_depth, 5);

            // Receive some values
            for _ in 0..3 {
                let mut stack = push(std::ptr::null_mut(), channel_id_value.clone());
                stack = receive(stack);
                let _ = pop(stack);
            }

            // Check stats after receives
            let stats = get_stats_with_retry().expect("Should get stats after retries");
            let stat = stats.iter().find(|s| s.id == channel_id).unwrap();
            assert_eq!(stat.send_count, 5);
            assert_eq!(stat.receive_count, 3);
            assert_eq!(stat.queue_depth, 2);

            // Clean up - receive remaining and close
            for _ in 0..2 {
                let mut stack = push(std::ptr::null_mut(), channel_id_value.clone());
                stack = receive(stack);
                let _ = pop(stack);
            }

            let stack = push(std::ptr::null_mut(), channel_id_value);
            let _ = close_channel(stack);
        }
    }

    #[test]
    fn test_mpmc_concurrent_receivers() {
        // Verify that multiple receivers can receive from the same channel concurrently
        // and that messages are distributed (not duplicated)
        unsafe {
            use std::sync::atomic::{AtomicI64, Ordering};

            const NUM_MESSAGES: i64 = 100;
            const NUM_RECEIVERS: usize = 4;

            // Shared counters for each receiver
            static RECEIVER_COUNTS: [AtomicI64; 4] = [
                AtomicI64::new(0),
                AtomicI64::new(0),
                AtomicI64::new(0),
                AtomicI64::new(0),
            ];
            static CHANNEL_ID: AtomicI64 = AtomicI64::new(0);

            // Reset counters
            for counter in &RECEIVER_COUNTS {
                counter.store(0, Ordering::SeqCst);
            }

            // Create channel
            let mut stack = std::ptr::null_mut();
            stack = make_channel(stack);
            let (_, channel_id_value) = pop(stack);
            let channel_id = match channel_id_value {
                Value::Int(id) => id,
                _ => panic!("Expected Int"),
            };
            CHANNEL_ID.store(channel_id, Ordering::SeqCst);

            // Receiver strand factory
            fn make_receiver(receiver_idx: usize) -> extern "C" fn(Stack) -> Stack {
                match receiver_idx {
                    0 => receiver_0,
                    1 => receiver_1,
                    2 => receiver_2,
                    3 => receiver_3,
                    _ => panic!("Invalid receiver index"),
                }
            }

            extern "C" fn receiver_0(stack: Stack) -> Stack {
                receive_loop(0, stack)
            }
            extern "C" fn receiver_1(stack: Stack) -> Stack {
                receive_loop(1, stack)
            }
            extern "C" fn receiver_2(stack: Stack) -> Stack {
                receive_loop(2, stack)
            }
            extern "C" fn receiver_3(stack: Stack) -> Stack {
                receive_loop(3, stack)
            }

            fn receive_loop(idx: usize, _stack: Stack) -> Stack {
                unsafe {
                    let chan_id = CHANNEL_ID.load(Ordering::SeqCst);
                    loop {
                        let mut stack = push(std::ptr::null_mut(), Value::Int(chan_id));
                        stack = receive_safe(stack);
                        let (stack, success) = pop(stack);
                        let (_, value) = pop(stack);

                        match (success, value) {
                            (Value::Int(1), Value::Int(v)) => {
                                if v < 0 {
                                    // Sentinel - exit
                                    break;
                                }
                                RECEIVER_COUNTS[idx].fetch_add(1, Ordering::SeqCst);
                            }
                            _ => break, // Channel closed or error
                        }
                        may::coroutine::yield_now();
                    }
                    std::ptr::null_mut()
                }
            }

            // Spawn receivers
            for i in 0..NUM_RECEIVERS {
                crate::scheduler::spawn_strand(make_receiver(i));
            }

            // Give receivers time to start
            std::thread::sleep(std::time::Duration::from_millis(10));

            // Send messages
            for i in 0..NUM_MESSAGES {
                let mut stack = push(std::ptr::null_mut(), Value::Int(i));
                stack = push(stack, Value::Int(channel_id));
                let _ = send(stack);
            }

            // Send sentinels to stop receivers
            for _ in 0..NUM_RECEIVERS {
                let mut stack = push(std::ptr::null_mut(), Value::Int(-1));
                stack = push(stack, Value::Int(channel_id));
                let _ = send(stack);
            }

            // Wait for all strands
            crate::scheduler::wait_all_strands();

            // Verify: total received should equal messages sent
            let total_received: i64 = RECEIVER_COUNTS
                .iter()
                .map(|c| c.load(Ordering::SeqCst))
                .sum();

            assert_eq!(
                total_received, NUM_MESSAGES,
                "Total received ({}) should equal messages sent ({})",
                total_received, NUM_MESSAGES
            );

            // Verify: messages were distributed (not all to one receiver)
            // At least 2 receivers should have received messages
            let active_receivers = RECEIVER_COUNTS
                .iter()
                .filter(|c| c.load(Ordering::SeqCst) > 0)
                .count();

            assert!(
                active_receivers >= 2,
                "Messages should be distributed across receivers, but only {} received any",
                active_receivers
            );

            // Clean up
            let stack = push(std::ptr::null_mut(), Value::Int(channel_id));
            let _ = close_channel(stack);
        }
    }
}
