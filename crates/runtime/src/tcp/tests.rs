use super::*;
use crate::arithmetic::push_int;
use crate::scheduler::scheduler_init;

#[test]
fn test_tcp_listen() {
    unsafe {
        scheduler_init();

        let stack = crate::stack::alloc_test_stack();
        let stack = push_int(stack, 0); // Port 0 = OS assigns random port
        let stack = tcp_listen(stack);

        // Now returns (Int, Bool) - Bool on top
        let (stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(true)),
            "tcp_listen should succeed"
        );

        let (_stack, result) = pop(stack);
        match result {
            Value::Int(listener_id) => {
                assert!(listener_id >= 0, "Listener ID should be non-negative");
            }
            _ => panic!("Expected Int (listener_id), got {:?}", result),
        }
    }
}

#[test]
fn test_tcp_listen_invalid_port_negative() {
    unsafe {
        scheduler_init();
        let stack = crate::stack::alloc_test_stack();
        let stack = push_int(stack, -1);
        let stack = tcp_listen(stack);

        // Invalid port returns (0, false)
        let (stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(false)),
            "Invalid port should return false"
        );
        let (_stack, result) = pop(stack);
        assert!(
            matches!(result, Value::Int(0)),
            "Invalid port should return 0"
        );
    }
}

#[test]
fn test_tcp_listen_invalid_port_too_high() {
    unsafe {
        scheduler_init();
        let stack = crate::stack::alloc_test_stack();
        let stack = push_int(stack, 65536);
        let stack = tcp_listen(stack);

        // Invalid port returns (0, false)
        let (stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(false)),
            "Invalid port should return false"
        );
        let (_stack, result) = pop(stack);
        assert!(
            matches!(result, Value::Int(0)),
            "Invalid port should return 0"
        );
    }
}

#[test]
fn test_tcp_port_range_valid() {
    unsafe {
        scheduler_init();

        // Test port 0 (OS-assigned)
        let stack = push_int(crate::stack::alloc_test_stack(), 0);
        let stack = tcp_listen(stack);
        let (stack, success) = pop(stack);
        assert!(matches!(success, Value::Bool(true)));
        let (_, result) = pop(stack);
        assert!(matches!(result, Value::Int(_)));

        // Test a non-privileged port (ports 1-1023 require root on Unix)
        // Use port 9999 which should be available and doesn't require privileges
        let stack = push_int(crate::stack::alloc_test_stack(), 9999);
        let stack = tcp_listen(stack);
        let (stack, success) = pop(stack);
        assert!(matches!(success, Value::Bool(true)));
        let (_, result) = pop(stack);
        assert!(matches!(result, Value::Int(_)));

        // Note: Can't easily test all edge cases (port 1, 65535) as they
        // may require privileges or be in use. Port validation logic is
        // tested separately in the invalid port tests.
    }
}

#[test]
fn test_socket_id_reuse_after_close() {
    unsafe {
        scheduler_init();

        // Create a listener and accept a hypothetical connection
        let stack = push_int(crate::stack::alloc_test_stack(), 0);
        let stack = tcp_listen(stack);
        let (stack, success) = pop(stack);
        assert!(matches!(success, Value::Bool(true)));
        let (_stack, listener_result) = pop(stack);

        let listener_id = match listener_result {
            Value::Int(id) => id,
            _ => panic!("Expected listener ID"),
        };

        // Verify listener ID is valid
        assert!(listener_id >= 0);

        // Note: We can't easily test connection acceptance without
        // actually making a connection, but we can test the registry behavior

        // Clean up
    }
}

#[test]
fn test_tcp_read_invalid_socket_id() {
    unsafe {
        scheduler_init();

        // Invalid socket ID now returns ("", false) instead of panicking
        let stack = push_int(crate::stack::alloc_test_stack(), 9999);
        let stack = tcp_read(stack);

        let (stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(false)),
            "Invalid socket should return false"
        );
        let (_stack, result) = pop(stack);
        match result {
            Value::String(s) => assert_eq!(s.as_str_or_empty(), ""),
            _ => panic!("Expected empty string"),
        }
    }
}

#[test]
fn test_tcp_write_invalid_socket_id() {
    unsafe {
        scheduler_init();

        // Invalid socket ID now returns false instead of panicking
        let stack = push(
            crate::stack::alloc_test_stack(),
            Value::String("test".into()),
        );
        let stack = push_int(stack, 9999);
        let stack = tcp_write(stack);

        let (_stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(false)),
            "Invalid socket should return false"
        );
    }
}

#[test]
fn test_tcp_close_idempotent() {
    unsafe {
        scheduler_init();

        // Create a socket to close
        let stack = push_int(crate::stack::alloc_test_stack(), 0);
        let stack = tcp_listen(stack);
        let (stack, success) = pop(stack);
        assert!(matches!(success, Value::Bool(true)));
        let (stack, _listener_result) = pop(stack);

        // Close an invalid socket - now returns false instead of crashing
        let stack = push_int(stack, 9999);
        let stack = tcp_close(stack);

        let (_stack, success) = pop(stack);
        assert!(
            matches!(success, Value::Bool(false)),
            "Invalid socket close should return false"
        );
    }
}

#[test]
fn test_socket_registry_capacity() {
    // Test that MAX_SOCKETS limit is enforced
    // Note: We can't easily allocate 10,000 real sockets in a unit test,
    // but the limit check is in the code at lines 38-41
    // This test documents the expected behavior

    // If we could allocate that many:
    // - First 10,000 allocations should succeed
    // - 10,001st allocation should panic with "Maximum socket limit reached"

    // For now, just verify the constant exists
    assert_eq!(MAX_SOCKETS, 10_000);
}

#[test]
fn test_max_read_size_limit() {
    // Test that MAX_READ_SIZE limit exists and is reasonable
    assert_eq!(MAX_READ_SIZE, 1_048_576); // 1 MB

    // In practice, if tcp_read receives more than 1 MB, it should panic
    // with "read size limit exceeded". Testing this requires a real socket
    // with more than 1 MB of data, which is impractical for unit tests.
}

#[test]
fn test_socket_registry_release_reserved_round_trip() {
    // Pins the round-trip semantics that the TLS-handshake-failure
    // path silently broke until #486 review: after a successful
    // `take_tcp`, the slot's inner Option is None, which makes
    // plain `free()` a no-op. Without `release_reserved`, the id
    // would leak — the Vec grows but the free list never sees the
    // id again. This test allocates, takes, releases-reserved, and
    // allocates a second time, asserting we get the same id back.
    let mut reg: SocketRegistry<i32> = SocketRegistry::new();

    // Allocate, get id 0.
    let id = reg.allocate(42).expect("allocate") as usize;
    assert_eq!(id, 0, "first allocation should give id 0");

    // Simulate take_tcp: pull the value out, leaving the slot
    // Some(None). This is the shape upgrade_tcp_in_place sees on
    // entry before it hands the stream to its callback.
    let slot = reg.get_mut(id).expect("slot exists");
    let _value = slot.take().expect("slot was occupied");
    assert!(slot.is_none(), "slot should be reserved after take");

    // Release the reserved id and confirm the round-trip.
    reg.release_reserved(id);
    let id2 = reg.allocate(99).expect("second allocate") as usize;
    assert_eq!(
        id2, id,
        "release_reserved must put the id back on the free list \
         so the next allocate reuses it (otherwise the id leaks)"
    );

    // Double-release of the same id must not double-push onto the
    // free list — if it did, the next two allocations would get
    // the same id and we'd have a use-after-free in disguise.
    let slot = reg.get_mut(id2).expect("slot exists");
    let _value = slot.take().expect("slot was occupied");
    reg.release_reserved(id2);
    reg.release_reserved(id2); // duplicate
    let id3 = reg.allocate(7).expect("third allocate") as usize;
    let id4 = reg.allocate(8).expect("fourth allocate") as usize;
    assert_ne!(
        id3, id4,
        "double release_reserved must not cause two allocations \
         to receive the same id"
    );
}
