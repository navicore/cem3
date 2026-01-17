//! Channel operations for CSP-style concurrency
//!
//! Channels are the primary communication mechanism between strands.
//! They use May's MPMC channels with cooperative blocking.
//!
//! ## Zero-Mutex Design
//!
//! Channels are passed directly as `Value::Channel` on the stack. There is NO
//! global registry and NO mutex contention. Send/receive operations work directly
//! on the channel handles with zero locking overhead.
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
//! ## Stack Effects
//!
//! - `chan.make`: ( -- Channel ) - creates a new channel
//! - `chan.send`: ( value Channel -- Bool ) - sends value, returns success
//! - `chan.receive`: ( Channel -- value Bool ) - receives value and success flag
//!
//! ## Error Handling
//!
//! All operations return success flags - errors are values, not crashes:
//!
//! - `chan.send`: ( value Channel -- Bool ) - returns true on success, false on closed
//! - `chan.receive`: ( Channel -- value Bool ) - returns value and success flag

use crate::stack::{Stack, pop, push};
use crate::value::{ChannelData, Value};
use may::sync::mpmc;
use std::sync::Arc;

/// Create a new channel
///
/// Stack effect: ( -- Channel )
///
/// Returns a Channel value that can be used with send/receive operations.
/// The channel can be duplicated (dup) to share between strands.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_channel(stack: Stack) -> Stack {
    // Create an unbounded MPMC channel
    // May's mpmc::channel() creates coroutine-aware channels with multi-producer, multi-consumer
    // The recv() operation cooperatively blocks (yields) instead of blocking the OS thread
    let (sender, receiver) = mpmc::channel();

    // Wrap in Arc<ChannelData> and push directly - NO registry, NO mutex
    let channel = Arc::new(ChannelData { sender, receiver });

    unsafe { push(stack, Value::Channel(channel)) }
}

/// Close a channel (drop it from the stack)
///
/// Stack effect: ( Channel -- )
///
/// Simply drops the channel. When all references are dropped, the channel is closed.
/// This is provided for API compatibility but is equivalent to `drop`.
///
/// # Safety
/// Stack must have a Channel on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_close_channel(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "close_channel: stack is empty");

    // Pop and drop the channel
    let (rest, channel_value) = unsafe { pop(stack) };
    match channel_value {
        Value::Channel(_) => {} // Drop occurs here
        _ => panic!(
            "close_channel: expected Channel on stack, got {:?}",
            channel_value
        ),
    }

    rest
}

/// Send a value through a channel
///
/// Stack effect: ( value Channel -- Bool )
///
/// Returns true on success, false on failure (closed channel).
/// Errors are values, not crashes.
///
/// # Safety
/// Stack must have a Channel on top and a value below it
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_send(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "chan.send: stack is empty");

    // Pop channel
    let (stack, channel_value) = unsafe { pop(stack) };
    let channel = match channel_value {
        Value::Channel(ch) => ch,
        _ => {
            // Wrong type - consume value and return failure
            if !stack.is_null() {
                let (rest, _value) = unsafe { pop(stack) };
                return unsafe { push(rest, Value::Bool(false)) };
            }
            return unsafe { push(stack, Value::Bool(false)) };
        }
    };

    if stack.is_null() {
        // No value to send - return failure
        return unsafe { push(stack, Value::Bool(false)) };
    }

    // Pop value to send
    let (rest, value) = unsafe { pop(stack) };

    // Clone the value before sending
    let global_value = value.clone();

    // Send the value
    match channel.sender.send(global_value) {
        Ok(()) => unsafe { push(rest, Value::Bool(true)) },
        Err(_) => unsafe { push(rest, Value::Bool(false)) },
    }
}

/// Receive a value from a channel
///
/// Stack effect: ( Channel -- value Bool )
///
/// Returns (value, true) on success, (0, false) on failure (closed channel).
/// Errors are values, not crashes.
///
/// ## Multi-Consumer Support
///
/// Multiple strands can receive from the same channel concurrently (MPMC).
/// Each message is delivered to exactly one receiver (work-stealing semantics).
///
/// # Safety
/// Stack must have a Channel on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_chan_receive(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "chan.receive: stack is empty");

    // Pop channel
    let (rest, channel_value) = unsafe { pop(stack) };
    let channel = match channel_value {
        Value::Channel(ch) => ch,
        _ => {
            // Wrong type - return failure
            let stack = unsafe { push(rest, Value::Int(0)) };
            return unsafe { push(stack, Value::Bool(false)) };
        }
    };

    // Receive a value
    match channel.receiver.recv() {
        Ok(value) => {
            let stack = unsafe { push(rest, value) };
            unsafe { push(stack, Value::Bool(true)) }
        }
        Err(_) => {
            let stack = unsafe { push(rest, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_chan_receive as receive;
pub use patch_seq_chan_send as send;
pub use patch_seq_close_channel as close_channel;
pub use patch_seq_make_channel as make_channel;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::{spawn_strand, wait_all_strands};
    use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

    #[test]
    fn test_make_channel() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = make_channel(stack);

            // Should have Channel on stack
            let (_stack, value) = pop(stack);
            assert!(matches!(value, Value::Channel(_)));
        }
    }

    #[test]
    fn test_send_receive() {
        unsafe {
            // Create a channel
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);

            // Get channel (but keep it on stack for receive via dup-like pattern)
            let (_empty_stack, channel_value) = pop(stack);

            // Push value to send, then channel
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(42));
            stack = push(stack, channel_value.clone());
            stack = send(stack);

            // Check send succeeded
            let (stack, send_success) = pop(stack);
            assert_eq!(send_success, Value::Bool(true));

            // Receive value
            let mut stack = push(stack, channel_value);
            stack = receive(stack);

            // Check receive succeeded and got correct value
            let (stack, recv_success) = pop(stack);
            let (_stack, received) = pop(stack);
            assert_eq!(recv_success, Value::Bool(true));
            assert_eq!(received, Value::Int(42));
        }
    }

    #[test]
    fn test_channel_dup_sharing() {
        // Verify that duplicating a channel shares the same underlying sender/receiver
        unsafe {
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);

            let (_, ch1) = pop(stack);
            let ch2 = ch1.clone(); // Simulates dup

            // Send on ch1
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(99));
            stack = push(stack, ch1);
            stack = send(stack);

            // Pop send success flag
            let (stack, _) = pop(stack);

            // Receive on ch2
            let mut stack = push(stack, ch2);
            stack = receive(stack);

            // Pop success flag then value
            let (stack, _) = pop(stack);
            let (_, received) = pop(stack);
            assert_eq!(received, Value::Int(99));
        }
    }

    #[test]
    fn test_multiple_sends_receives() {
        unsafe {
            // Create a channel
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);
            let (_, channel_value) = pop(stack);

            // Send multiple values
            for i in 1..=5 {
                let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(i));
                stack = push(stack, channel_value.clone());
                stack = send(stack);
                let (_, success) = pop(stack);
                assert_eq!(success, Value::Bool(true));
            }

            // Receive them back in order
            for i in 1..=5 {
                let mut stack = push(crate::stack::alloc_test_stack(), channel_value.clone());
                stack = receive(stack);
                let (stack, success) = pop(stack);
                let (_, received) = pop(stack);
                assert_eq!(success, Value::Bool(true));
                assert_eq!(received, Value::Int(i));
            }
        }
    }

    #[test]
    fn test_close_channel() {
        unsafe {
            // Create and close a channel
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);

            let _stack = close_channel(stack);
        }
    }

    #[test]
    fn test_arena_string_send_between_strands() {
        // Verify that arena-allocated strings are properly cloned to global storage
        unsafe {
            static CHANNEL_PTR: AtomicI64 = AtomicI64::new(0);
            static VERIFIED: AtomicBool = AtomicBool::new(false);

            // Create a channel
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);
            let (_, channel_value) = pop(stack);

            // Store channel pointer for strands (hacky but works for test)
            let ch_ptr = match &channel_value {
                Value::Channel(arc) => Arc::as_ptr(arc) as i64,
                _ => panic!("Expected Channel"),
            };
            CHANNEL_PTR.store(ch_ptr, Ordering::Release);

            // Keep the Arc alive
            std::mem::forget(channel_value.clone());

            // Sender strand
            extern "C" fn sender(_stack: Stack) -> Stack {
                use crate::seqstring::arena_string;
                use crate::value::ChannelData;

                unsafe {
                    let ch_ptr = CHANNEL_PTR.load(Ordering::Acquire) as *const ChannelData;
                    let channel = Arc::from_raw(ch_ptr);
                    let channel_clone = Arc::clone(&channel);
                    std::mem::forget(channel); // Don't drop

                    // Create arena string (fast path)
                    let msg = arena_string("Arena message!");
                    assert!(!msg.is_global(), "Should be arena-allocated initially");

                    // Send through channel
                    let stack = push(crate::stack::alloc_test_stack(), Value::String(msg));
                    let stack = push(stack, Value::Channel(channel_clone));
                    let stack = send(stack);
                    // Pop success flag (we trust it worked for this test)
                    let (stack, _) = pop(stack);
                    stack
                }
            }

            // Receiver strand
            extern "C" fn receiver(_stack: Stack) -> Stack {
                use crate::value::ChannelData;

                unsafe {
                    let ch_ptr = CHANNEL_PTR.load(Ordering::Acquire) as *const ChannelData;
                    let channel = Arc::from_raw(ch_ptr);
                    let channel_clone = Arc::clone(&channel);
                    std::mem::forget(channel); // Don't drop

                    let mut stack = push(
                        crate::stack::alloc_test_stack(),
                        Value::Channel(channel_clone),
                    );
                    stack = receive(stack);
                    // Pop success flag first
                    let (stack, _) = pop(stack);
                    let (_, msg_val) = pop(stack);

                    match msg_val {
                        Value::String(s) => {
                            assert_eq!(s.as_str(), "Arena message!");
                            assert!(s.is_global(), "Received string should be global");
                            VERIFIED.store(true, Ordering::Release);
                        }
                        _ => panic!("Expected String"),
                    }

                    std::ptr::null_mut()
                }
            }

            spawn_strand(sender);
            spawn_strand(receiver);
            wait_all_strands();

            assert!(
                VERIFIED.load(Ordering::Acquire),
                "Receiver should have verified"
            );
        }
    }

    #[test]
    fn test_send_success() {
        unsafe {
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);
            let (_, channel_value) = pop(stack);

            // Send value
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(42));
            stack = push(stack, channel_value.clone());
            stack = send(stack);

            // Should return success (true)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));

            // Receive to verify
            let mut stack = push(stack, channel_value);
            stack = receive(stack);
            let (stack, success) = pop(stack);
            let (_, received) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(received, Value::Int(42));
        }
    }

    #[test]
    fn test_send_wrong_type() {
        unsafe {
            // Try to send with Int instead of Channel
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(42));
            stack = push(stack, Value::Int(999)); // Wrong type
            stack = send(stack);

            // Should return failure (false)
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(false));
        }
    }

    #[test]
    fn test_receive_success() {
        unsafe {
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);
            let (_, channel_value) = pop(stack);

            // Send value
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(42));
            stack = push(stack, channel_value.clone());
            stack = send(stack);
            let (_, _) = pop(stack); // pop send success

            // Receive
            let mut stack = push(crate::stack::alloc_test_stack(), channel_value);
            stack = receive(stack);

            // Should return (value, true)
            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(value, Value::Int(42));
        }
    }

    #[test]
    fn test_receive_wrong_type() {
        unsafe {
            // Try to receive with Int instead of Channel
            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(999));
            stack = receive(stack);

            // Should return (0, false)
            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            assert_eq!(value, Value::Int(0));
        }
    }

    #[test]
    fn test_mpmc_concurrent_receivers() {
        // Verify that multiple receivers work with MPMC
        unsafe {
            const NUM_MESSAGES: i64 = 100;
            const NUM_RECEIVERS: usize = 4;

            static RECEIVER_COUNTS: [AtomicI64; 4] = [
                AtomicI64::new(0),
                AtomicI64::new(0),
                AtomicI64::new(0),
                AtomicI64::new(0),
            ];
            static CHANNEL_PTR: AtomicI64 = AtomicI64::new(0);

            // Reset counters
            for counter in &RECEIVER_COUNTS {
                counter.store(0, Ordering::SeqCst);
            }

            // Create channel
            let mut stack = crate::stack::alloc_test_stack();
            stack = make_channel(stack);
            let (_, channel_value) = pop(stack);

            let ch_ptr = match &channel_value {
                Value::Channel(arc) => Arc::as_ptr(arc) as i64,
                _ => panic!("Expected Channel"),
            };
            CHANNEL_PTR.store(ch_ptr, Ordering::SeqCst);

            // Keep Arc alive
            for _ in 0..(NUM_RECEIVERS + 1) {
                std::mem::forget(channel_value.clone());
            }

            fn make_receiver(idx: usize) -> extern "C" fn(Stack) -> Stack {
                match idx {
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
                use crate::value::ChannelData;
                unsafe {
                    let ch_ptr = CHANNEL_PTR.load(Ordering::SeqCst) as *const ChannelData;
                    let channel = Arc::from_raw(ch_ptr);
                    let channel_clone = Arc::clone(&channel);
                    std::mem::forget(channel);

                    loop {
                        let mut stack = push(
                            crate::stack::alloc_test_stack(),
                            Value::Channel(channel_clone.clone()),
                        );
                        stack = receive(stack);
                        let (stack, success) = pop(stack);
                        let (_, value) = pop(stack);

                        match (success, value) {
                            (Value::Bool(true), Value::Int(v)) => {
                                if v < 0 {
                                    break; // Sentinel
                                }
                                RECEIVER_COUNTS[idx].fetch_add(1, Ordering::SeqCst);
                            }
                            _ => break,
                        }
                        may::coroutine::yield_now();
                    }
                    std::ptr::null_mut()
                }
            }

            // Spawn receivers
            for i in 0..NUM_RECEIVERS {
                spawn_strand(make_receiver(i));
            }

            std::thread::sleep(std::time::Duration::from_millis(10));

            // Send messages
            for i in 0..NUM_MESSAGES {
                let ch_ptr = CHANNEL_PTR.load(Ordering::SeqCst) as *const ChannelData;
                let channel = Arc::from_raw(ch_ptr);
                let channel_clone = Arc::clone(&channel);
                std::mem::forget(channel);

                let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(i));
                stack = push(stack, Value::Channel(channel_clone));
                let _ = send(stack);
            }

            // Send sentinels
            for _ in 0..NUM_RECEIVERS {
                let ch_ptr = CHANNEL_PTR.load(Ordering::SeqCst) as *const ChannelData;
                let channel = Arc::from_raw(ch_ptr);
                let channel_clone = Arc::clone(&channel);
                std::mem::forget(channel);

                let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(-1));
                stack = push(stack, Value::Channel(channel_clone));
                let _ = send(stack);
            }

            wait_all_strands();

            let total_received: i64 = RECEIVER_COUNTS
                .iter()
                .map(|c| c.load(Ordering::SeqCst))
                .sum();
            assert_eq!(total_received, NUM_MESSAGES);

            let active_receivers = RECEIVER_COUNTS
                .iter()
                .filter(|c| c.load(Ordering::SeqCst) > 0)
                .count();
            assert!(active_receivers >= 2, "Messages should be distributed");
        }
    }
}
