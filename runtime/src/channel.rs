//! Channel operations for CSP-style concurrency
//!
//! Channels are the primary communication mechanism between strands.
//! They use May's `sync_channel` for bounded channels with blocking send/receive.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use may::sync::mpsc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Unique channel ID generation
static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);

/// Global channel registry
/// Maps channel IDs to sender/receiver pairs
static CHANNEL_REGISTRY: Mutex<Option<HashMap<u64, ChannelPair>>> = Mutex::new(None);

/// A channel pair (sender and receiver)
/// Receiver is Arc<Mutex<>> to allow access without holding global registry lock
struct ChannelPair {
    sender: mpsc::Sender<Value>,
    receiver: Arc<Mutex<mpsc::Receiver<Value>>>,
}

/// Initialize the channel registry
fn init_registry() {
    let mut guard = match CHANNEL_REGISTRY.lock() {
        Ok(g) => g,
        Err(poisoned) => panic!("Channel registry lock poisoned: {}", poisoned),
    };

    if guard.is_none() {
        *guard = Some(HashMap::new());
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
pub unsafe extern "C" fn make_channel(stack: Stack) -> Stack {
    init_registry();

    // Create an unbounded channel
    // May's mpsc::channel() creates coroutine-aware channels
    // The recv() operation cooperatively blocks (yields) instead of blocking the OS thread
    let (sender, receiver) = mpsc::channel();

    let channel_id = NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed);

    // Store in registry
    let mut guard = match CHANNEL_REGISTRY.lock() {
        Ok(g) => g,
        Err(poisoned) => panic!("Channel registry lock poisoned: {}", poisoned),
    };

    let registry = match guard.as_mut() {
        Some(r) => r,
        None => panic!("Channel registry not initialized"),
    };

    registry.insert(
        channel_id,
        ChannelPair {
            sender,
            receiver: Arc::new(Mutex::new(receiver)),
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
pub unsafe extern "C" fn send(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "send: stack is empty");

    // Pop channel ID
    let (stack, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => id as u64,
        _ => panic!("send: expected channel ID (Int) on stack"),
    };

    assert!(!stack.is_null(), "send: stack has only one value");

    // Pop value to send
    let (rest, value) = unsafe { pop(stack) };

    // Get sender from registry
    let guard = match CHANNEL_REGISTRY.lock() {
        Ok(g) => g,
        Err(poisoned) => panic!("Channel registry lock poisoned: {}", poisoned),
    };

    let registry = match guard.as_ref() {
        Some(r) => r,
        None => panic!("Channel registry not initialized"),
    };

    let pair = match registry.get(&channel_id) {
        Some(p) => p,
        None => panic!("send: invalid channel ID {}", channel_id),
    };

    // Clone the sender so we can use it outside the lock
    let sender = pair.sender.clone();
    drop(guard); // Release lock before potentially blocking

    // Send the value (may block if channel is full)
    // May's scheduler will handle the blocking cooperatively
    sender.send(value).expect("send: channel closed");

    rest
}

/// Receive a value from a channel
///
/// Stack effect: ( channel_id -- value )
///
/// Blocks the strand until a value is available.
/// This is cooperative blocking - the strand yields and May handles scheduling.
///
/// # Safety
/// Stack must have a channel ID (Int) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn receive(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "receive: stack is empty");

    // Pop channel ID
    let (rest, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => id as u64,
        _ => panic!("receive: expected channel ID (Int) on stack"),
    };

    // Get receiver Arc from registry (don't hold lock during recv!)
    let receiver_arc = {
        let guard = match CHANNEL_REGISTRY.lock() {
            Ok(g) => g,
            Err(poisoned) => panic!("Channel registry lock poisoned: {}", poisoned),
        };

        let registry = match guard.as_ref() {
            Some(r) => r,
            None => panic!("Channel registry not initialized"),
        };

        let pair = match registry.get(&channel_id) {
            Some(p) => p,
            None => panic!("receive: invalid channel ID {}", channel_id),
        };

        Arc::clone(&pair.receiver)
    }; // Registry lock released here!

    // Receive a value (cooperatively blocks the strand until available)
    // May's recv() yields to the scheduler, not blocking the OS thread
    // We do NOT hold the global registry lock, avoiding deadlock
    let receiver = match receiver_arc.lock() {
        Ok(r) => r,
        Err(poisoned) => panic!("Receiver lock poisoned: {}", poisoned),
    };

    let value = match receiver.recv() {
        Ok(v) => v,
        Err(_) => panic!("receive: channel closed"),
    };

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
pub unsafe extern "C" fn close_channel(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "close_channel: stack is empty");

    // Pop channel ID
    let (rest, channel_id_value) = unsafe { pop(stack) };
    let channel_id = match channel_id_value {
        Value::Int(id) => id as u64,
        _ => panic!("close_channel: expected channel ID (Int) on stack"),
    };

    // Remove from registry
    let mut guard = match CHANNEL_REGISTRY.lock() {
        Ok(g) => g,
        Err(poisoned) => panic!("Channel registry lock poisoned: {}", poisoned),
    };

    let registry = match guard.as_mut() {
        Some(r) => r,
        None => panic!("Channel registry not initialized"),
    };

    registry.remove(&channel_id);

    rest
}

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
}
