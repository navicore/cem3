//! TCP Socket Operations for cem3
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

// Global registry for TCP listeners and streams
// Key is a simple integer ID (file descriptor analog)
// We don't need Arc<Mutex<>> for streams since coroutines are cooperative (single-threaded)
// and we take streams out of the registry before doing I/O
static LISTENERS: Mutex<Vec<Option<TcpListener>>> = Mutex::new(Vec::new());
static STREAMS: Mutex<Vec<Option<TcpStream>>> = Mutex::new(Vec::new());

/// TCP listen on a port
///
/// Stack effect: ( port -- listener_id )
///
/// Binds to 0.0.0.0:port and returns a listener ID
///
/// # Safety
/// Stack must have an Int (port number) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_listen(stack: Stack) -> Stack {
    unsafe {
        let (stack, port_val) = pop(stack);
        let port = match port_val {
            Value::Int(p) => p,
            _ => panic!(
                "tcp_listen: expected Int (port) on stack, got {:?}",
                port_val
            ),
        };

        // Bind to the port (non-blocking via May)
        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr)
            .unwrap_or_else(|e| panic!("tcp_listen: failed to bind to {}: {}", addr, e));

        // Store listener and get ID
        let mut listeners = LISTENERS.lock().unwrap();
        let listener_id = listeners.len() as i64;
        listeners.push(Some(listener));

        push(stack, Value::Int(listener_id))
    }
}

/// TCP accept a connection
///
/// Stack effect: ( listener_id -- client_id )
///
/// Accepts a connection (yields the strand until one arrives)
///
/// # Safety
/// Stack must have an Int (listener_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_accept(stack: Stack) -> Stack {
    unsafe {
        let (stack, listener_id_val) = pop(stack);
        let listener_id = match listener_id_val {
            Value::Int(id) => id as usize,
            _ => panic!(
                "tcp_accept: expected Int (listener_id), got {:?}",
                listener_id_val
            ),
        };

        // Take the listener out temporarily (so we don't hold lock during accept)
        let listener = {
            let mut listeners = LISTENERS.lock().unwrap();
            listeners
                .get_mut(listener_id)
                .and_then(|opt| opt.take())
                .unwrap_or_else(|| panic!("tcp_accept: invalid listener_id {}", listener_id))
        };
        // Lock released

        // Accept connection (this yields the strand, doesn't block OS thread)
        let (stream, _addr) = listener
            .accept()
            .unwrap_or_else(|e| panic!("tcp_accept: failed to accept connection: {}", e));

        // Put the listener back
        {
            let mut listeners = LISTENERS.lock().unwrap();
            if let Some(slot) = listeners.get_mut(listener_id) {
                *slot = Some(listener);
            }
        }

        // Store stream and get ID
        let mut streams = STREAMS.lock().unwrap();
        let client_id = streams.len() as i64;
        streams.push(Some(stream));

        push(stack, Value::Int(client_id))
    }
}

/// TCP read from a socket
///
/// Stack effect: ( socket_id -- string )
///
/// Reads all available data from the socket
///
/// # Safety
/// Stack must have an Int (socket_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_read(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => panic!(
                "tcp_read: expected Int (socket_id), got {:?}",
                socket_id_val
            ),
        };

        // Take the stream out of the registry (so we don't hold the lock during I/O)
        let mut stream = {
            let mut streams = STREAMS.lock().unwrap();
            streams
                .get_mut(socket_id)
                .and_then(|opt| opt.take())
                .unwrap_or_else(|| panic!("tcp_read: invalid socket_id {}", socket_id))
        };
        // Registry lock is now released

        // Read data (this yields the strand, doesn't block OS thread)
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];

        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break, // EOF
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => panic!("tcp_read: failed to read from socket: {}", e),
            }

            // For HTTP, stop at end of headers (blank line)
            if buffer.len() >= 4 && &buffer[buffer.len() - 4..] == b"\r\n\r\n" {
                break;
            }
        }

        let data = String::from_utf8(buffer)
            .unwrap_or_else(|e| panic!("tcp_read: invalid UTF-8 data: {}", e));

        // Put the stream back
        {
            let mut streams = STREAMS.lock().unwrap();
            if let Some(slot) = streams.get_mut(socket_id) {
                *slot = Some(stream);
            }
        }

        push(stack, Value::String(data.into()))
    }
}

/// TCP write to a socket
///
/// Stack effect: ( string socket_id -- )
///
/// Writes string to the socket
///
/// # Safety
/// Stack must have Int (socket_id) and String on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_write(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => panic!(
                "tcp_write: expected Int (socket_id), got {:?}",
                socket_id_val
            ),
        };

        let (stack, data_val) = pop(stack);
        let data = match data_val {
            Value::String(s) => s,
            _ => panic!("tcp_write: expected String, got {:?}", data_val),
        };

        // Take the stream out of the registry (so we don't hold the lock during I/O)
        let mut stream = {
            let mut streams = STREAMS.lock().unwrap();
            streams
                .get_mut(socket_id)
                .and_then(|opt| opt.take())
                .unwrap_or_else(|| panic!("tcp_write: invalid socket_id {}", socket_id))
        };
        // Registry lock is now released

        // Write data (non-blocking via May, yields strand as needed)
        stream
            .write_all(data.as_str().as_bytes())
            .unwrap_or_else(|e| panic!("tcp_write: failed to write to socket: {}", e));

        stream
            .flush()
            .unwrap_or_else(|e| panic!("tcp_write: failed to flush socket: {}", e));

        // Put the stream back
        {
            let mut streams = STREAMS.lock().unwrap();
            if let Some(slot) = streams.get_mut(socket_id) {
                *slot = Some(stream);
            }
        }

        stack
    }
}

/// TCP close a socket
///
/// Stack effect: ( socket_id -- )
///
/// Closes the socket connection
///
/// # Safety
/// Stack must have an Int (socket_id) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tcp_close(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => panic!(
                "tcp_close: expected Int (socket_id), got {:?}",
                socket_id_val
            ),
        };

        // Remove the stream
        let mut streams = STREAMS.lock().unwrap();
        if let Some(slot) = streams.get_mut(socket_id) {
            *slot = None; // Drop the stream, closing the connection
        }

        stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arithmetic::push_int;
    use crate::scheduler::scheduler_init;

    #[test]
    fn test_tcp_listen() {
        unsafe {
            scheduler_init();

            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 0); // Port 0 = OS assigns random port
            let stack = tcp_listen(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Int(listener_id) => {
                    assert!(listener_id >= 0, "Listener ID should be non-negative");
                }
                _ => panic!("Expected Int (listener_id), got {:?}", result),
            }
            assert!(stack.is_null());
        }
    }
}
