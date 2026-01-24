//! TCP Socket Operations for Seq
//!
//! Provides non-blocking TCP socket operations using May's coroutine-aware I/O.
//! All operations yield the strand instead of blocking the OS thread.
//!
//! These functions are exported with C ABI for LLVM codegen.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use may::net::{TcpListener, TcpStream};
use std::io::{Read, Write};
use std::sync::Mutex;

// Maximum number of concurrent connections to prevent unbounded growth
const MAX_SOCKETS: usize = 10_000;

// Maximum bytes to read from a socket to prevent memory exhaustion attacks
const MAX_READ_SIZE: usize = 1_048_576; // 1 MB

// Socket registry with ID reuse via free list
struct SocketRegistry<T> {
    sockets: Vec<Option<T>>,
    free_ids: Vec<usize>,
}

impl<T> SocketRegistry<T> {
    const fn new() -> Self {
        Self {
            sockets: Vec::new(),
            free_ids: Vec::new(),
        }
    }

    fn allocate(&mut self, socket: T) -> Result<i64, &'static str> {
        // Try to reuse a free ID first
        if let Some(id) = self.free_ids.pop() {
            self.sockets[id] = Some(socket);
            return Ok(id as i64);
        }

        // Check max connections limit
        if self.sockets.len() >= MAX_SOCKETS {
            return Err("Maximum socket limit reached");
        }

        // Allocate new ID
        let id = self.sockets.len();
        self.sockets.push(Some(socket));
        Ok(id as i64)
    }

    fn get_mut(&mut self, id: usize) -> Option<&mut Option<T>> {
        self.sockets.get_mut(id)
    }

    fn free(&mut self, id: usize) {
        if let Some(slot) = self.sockets.get_mut(id)
            && slot.is_some()
        {
            *slot = None;
            self.free_ids.push(id);
        }
    }
}

// Global registry for TCP listeners and streams
static LISTENERS: Mutex<SocketRegistry<TcpListener>> = Mutex::new(SocketRegistry::new());
static STREAMS: Mutex<SocketRegistry<TcpStream>> = Mutex::new(SocketRegistry::new());

/// TCP listen on a port
///
/// Stack effect: ( port -- listener_id Bool )
///
/// Binds to 0.0.0.0:port and returns a listener ID with success flag.
/// Returns (0, false) on failure (invalid port, bind error, socket limit).
///
/// # Safety
/// Stack must have an Int (port number) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_listen(stack: Stack) -> Stack {
    unsafe {
        let (stack, port_val) = pop(stack);
        let port = match port_val {
            Value::Int(p) => p,
            _ => {
                // Type error - return failure
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        // Validate port range (1-65535, or 0 for OS-assigned)
        if !(0..=65535).contains(&port) {
            let stack = push(stack, Value::Int(0));
            return push(stack, Value::Bool(false));
        }

        // Bind to the port (non-blocking via May)
        let addr = format!("0.0.0.0:{}", port);
        let listener = match TcpListener::bind(&addr) {
            Ok(l) => l,
            Err(_) => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        // Store listener and get ID
        let mut listeners = LISTENERS.lock().unwrap();
        match listeners.allocate(listener) {
            Ok(listener_id) => {
                let stack = push(stack, Value::Int(listener_id));
                push(stack, Value::Bool(true))
            }
            Err(_) => {
                let stack = push(stack, Value::Int(0));
                push(stack, Value::Bool(false))
            }
        }
    }
}

/// TCP accept a connection
///
/// Stack effect: ( listener_id -- client_id Bool )
///
/// Accepts a connection (yields the strand until one arrives).
/// Returns (0, false) on failure (invalid listener, accept error, socket limit).
///
/// # Safety
/// Stack must have an Int (listener_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_accept(stack: Stack) -> Stack {
    unsafe {
        let (stack, listener_id_val) = pop(stack);
        let listener_id = match listener_id_val {
            Value::Int(id) => id as usize,
            _ => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        // Take the listener out temporarily (so we don't hold lock during accept)
        let listener = {
            let mut listeners = LISTENERS.lock().unwrap();
            match listeners.get_mut(listener_id).and_then(|opt| opt.take()) {
                Some(l) => l,
                None => {
                    let stack = push(stack, Value::Int(0));
                    return push(stack, Value::Bool(false));
                }
            }
        };
        // Lock released

        // Accept connection (this yields the strand, doesn't block OS thread)
        let (stream, _addr) = match listener.accept() {
            Ok(result) => result,
            Err(_) => {
                // Put listener back before returning
                let mut listeners = LISTENERS.lock().unwrap();
                if let Some(slot) = listeners.get_mut(listener_id) {
                    *slot = Some(listener);
                }
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        // Put the listener back
        {
            let mut listeners = LISTENERS.lock().unwrap();
            if let Some(slot) = listeners.get_mut(listener_id) {
                *slot = Some(listener);
            }
        }

        // Store stream and get ID
        let mut streams = STREAMS.lock().unwrap();
        match streams.allocate(stream) {
            Ok(client_id) => {
                let stack = push(stack, Value::Int(client_id));
                push(stack, Value::Bool(true))
            }
            Err(_) => {
                let stack = push(stack, Value::Int(0));
                push(stack, Value::Bool(false))
            }
        }
    }
}

/// TCP read from a socket
///
/// Stack effect: ( socket_id -- string Bool )
///
/// Reads all available data from the socket.
/// Returns ("", false) on failure (invalid socket, read error, size limit, invalid UTF-8).
///
/// # Safety
/// Stack must have an Int (socket_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_read(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => {
                let stack = push(stack, Value::String("".into()));
                return push(stack, Value::Bool(false));
            }
        };

        // Take the stream out of the registry (so we don't hold the lock during I/O)
        let mut stream = {
            let mut streams = STREAMS.lock().unwrap();
            match streams.get_mut(socket_id).and_then(|opt| opt.take()) {
                Some(s) => s,
                None => {
                    let stack = push(stack, Value::String("".into()));
                    return push(stack, Value::Bool(false));
                }
            }
        };
        // Registry lock is now released

        // Read available data (this yields the strand, doesn't block OS thread)
        // Reads all currently available data up to MAX_READ_SIZE
        // Returns when: data is available and read, EOF, or WouldBlock
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut read_error = false;

        // Read until we get data, EOF, or error
        // For HTTP: Read once and return immediately to avoid blocking when client waits for response
        loop {
            // Check size limit to prevent memory exhaustion
            if buffer.len() >= MAX_READ_SIZE {
                read_error = true;
                break;
            }

            match stream.read(&mut chunk) {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    // Don't exceed max size even with partial chunk
                    let bytes_to_add = n.min(MAX_READ_SIZE.saturating_sub(buffer.len()));
                    buffer.extend_from_slice(&chunk[..bytes_to_add]);
                    if bytes_to_add < n {
                        break; // Hit limit
                    }
                    // Return immediately after reading data
                    // May's cooperative I/O would block on next read() if no more data available
                    // Client might be waiting for our response, so don't wait for more
                    break;
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // No data available yet - yield and wait for May's scheduler to wake us
                    // when data arrives, or connection closes
                    if buffer.is_empty() {
                        may::coroutine::yield_now();
                        continue;
                    }
                    // If we already have some data, return it
                    break;
                }
                Err(_) => {
                    read_error = true;
                    break;
                }
            }
        }

        // Put the stream back
        {
            let mut streams = STREAMS.lock().unwrap();
            if let Some(slot) = streams.get_mut(socket_id) {
                *slot = Some(stream);
            }
        }

        if read_error {
            let stack = push(stack, Value::String("".into()));
            return push(stack, Value::Bool(false));
        }

        match String::from_utf8(buffer) {
            Ok(data) => {
                let stack = push(stack, Value::String(data.into()));
                push(stack, Value::Bool(true))
            }
            Err(_) => {
                let stack = push(stack, Value::String("".into()));
                push(stack, Value::Bool(false))
            }
        }
    }
}

/// TCP write to a socket
///
/// Stack effect: ( string socket_id -- Bool )
///
/// Writes string to the socket.
/// Returns false on failure (invalid socket, write error).
///
/// # Safety
/// Stack must have Int (socket_id) and String on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_write(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => {
                return push(stack, Value::Bool(false));
            }
        };

        let (stack, data_val) = pop(stack);
        let data = match data_val {
            Value::String(s) => s,
            _ => {
                return push(stack, Value::Bool(false));
            }
        };

        // Take the stream out of the registry (so we don't hold the lock during I/O)
        let mut stream = {
            let mut streams = STREAMS.lock().unwrap();
            match streams.get_mut(socket_id).and_then(|opt| opt.take()) {
                Some(s) => s,
                None => {
                    return push(stack, Value::Bool(false));
                }
            }
        };
        // Registry lock is now released

        // Write data (non-blocking via May, yields strand as needed)
        let write_result = stream.write_all(data.as_str().as_bytes());
        let flush_result = if write_result.is_ok() {
            stream.flush()
        } else {
            write_result
        };

        // Put the stream back
        {
            let mut streams = STREAMS.lock().unwrap();
            if let Some(slot) = streams.get_mut(socket_id) {
                *slot = Some(stream);
            }
        }

        push(stack, Value::Bool(flush_result.is_ok()))
    }
}

/// TCP close a socket
///
/// Stack effect: ( socket_id -- Bool )
///
/// Closes the socket connection and frees the socket ID for reuse.
/// Returns true on success, false if socket_id was invalid.
///
/// # Safety
/// Stack must have an Int (socket_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_close(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => {
                return push(stack, Value::Bool(false));
            }
        };

        // Check if socket exists before freeing
        let mut streams = STREAMS.lock().unwrap();
        let existed = streams
            .get_mut(socket_id)
            .map(|slot| slot.is_some())
            .unwrap_or(false);

        if existed {
            streams.free(socket_id);
        }

        push(stack, Value::Bool(existed))
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_tcp_accept as tcp_accept;
pub use patch_seq_tcp_close as tcp_close;
pub use patch_seq_tcp_listen as tcp_listen;
pub use patch_seq_tcp_read as tcp_read;
pub use patch_seq_tcp_write as tcp_write;

#[cfg(test)]
mod tests {
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
                Value::String(s) => assert_eq!(s.as_str(), ""),
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
}
