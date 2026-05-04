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
                        assert_eq!(s.as_str_or_empty(), "Arena message!");
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

        // Send messages with a tiny pause between each so the scheduler
        // gets a yield point per send. Without this, on a single-core
        // runner the whole send loop completes before any receiver is
        // scheduled, the first receiver drains the queue without ever
        // blocking (the others stay suspended on `receive`, so
        // `yield_now` finds no ready peers), and the distribution
        // assertion below fails even though every message was delivered.
        for i in 0..NUM_MESSAGES {
            let ch_ptr = CHANNEL_PTR.load(Ordering::SeqCst) as *const ChannelData;
            let channel = Arc::from_raw(ch_ptr);
            let channel_clone = Arc::clone(&channel);
            std::mem::forget(channel);

            let mut stack = push(crate::stack::alloc_test_stack(), Value::Int(i));
            stack = push(stack, Value::Channel(channel_clone));
            let _ = send(stack);
            std::thread::sleep(std::time::Duration::from_micros(50));
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
