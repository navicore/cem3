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
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

// Maximum number of concurrent connections to prevent unbounded growth
const MAX_SOCKETS: usize = 10_000;

// Architectural cap for any future read-N or full-body socket reader
// (e.g. an HTTP-client migration off ureq). The current `tcp.read` does
// one 4 KB read per call and so doesn't need to consult this; it stays
// here as the ceiling that bounded-buffer variants must respect.
#[allow(dead_code)]
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

/// TCP connect to a remote endpoint.
///
/// Stack effect: ( host:String port:Int -- Socket Bool )
///
/// Resolves `host` through the may-aware DNS layer (cache + worker
/// pool — no `getaddrinfo` ever runs on a may carrier), then tries
/// each resolved address in order until one connects via
/// `may::net::TcpStream::connect`. Yields the strand on every step.
///
/// Returns `(0, false)` on resolution failure, every-address-failed,
/// invalid port, or socket-registry exhaustion.
///
/// # Safety
/// Stack must have a String (host) and Int (port) on top — port topmost.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_connect(stack: Stack) -> Stack {
    unsafe {
        let (stack, port_val) = pop(stack);
        let port = match port_val {
            Value::Int(p) => p,
            _ => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };
        if !(1..=65535).contains(&port) {
            let stack = push(stack, Value::Int(0));
            return push(stack, Value::Bool(false));
        }

        let (stack, host_val) = pop(stack);
        let host = match host_val {
            Value::String(s) => s,
            _ => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };
        let hostname = host.as_str_or_empty();
        if hostname.is_empty() {
            let stack = push(stack, Value::Int(0));
            return push(stack, Value::Bool(false));
        }

        let addrs = crate::dns::resolve(hostname);
        if addrs.is_empty() {
            let stack = push(stack, Value::Int(0));
            return push(stack, Value::Bool(false));
        }

        // Build SocketAddr directly from the parsed IpAddr — avoids
        // the `"::1:80"` IPv6 trap of formatting `ip:port` as a string
        // (an IPv6 literal needs square brackets in that form, but a
        // typed SocketAddr bypasses parsing entirely). Resolver only
        // emits IP-string forms produced by `SocketAddr::ip().to_string()`,
        // so a parse failure here would mean a runtime invariant
        // violation — treat it as "skip this address" rather than
        // panicking the carrier. connect() yields the strand on the
        // SYN/SYN-ACK round-trip.
        let port_u16 = port as u16;
        let stream = addrs.iter().find_map(|ip| {
            let ip_addr = ip.parse::<IpAddr>().ok()?;
            TcpStream::connect(SocketAddr::new(ip_addr, port_u16)).ok()
        });
        let stream = match stream {
            Some(s) => s,
            None => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        let mut streams = STREAMS.lock().unwrap();
        match streams.allocate(stream) {
            Ok(id) => {
                let stack = push(stack, Value::Int(id));
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

        // One read per call. may::net::TcpStream::read suspends the
        // strand until at least one byte is available (or the peer
        // closes), then returns whatever the kernel had ready in one
        // batch. Returning here lets a caller wait for client data
        // without our own read holding the socket past the first
        // payload — request/response framing happens in user code.
        //
        // The chunk size caps a single batch at 4 KB, well under
        // MAX_READ_SIZE, so a size check is unnecessary at the
        // per-read level.
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut read_error = false;
        match stream.read(&mut chunk) {
            Ok(0) => {} // EOF — return empty payload, success=true
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(_) => read_error = true,
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

        // The bytes go into a byte-clean SeqString unchanged — TCP can
        // now serve binary protocols (HTTP/2 frames, gRPC, raw TLS,
        // protocol-buffer streams, anything that isn't text). UTF-8 is
        // a property of the application protocol, not of the transport.
        let stack = push(stack, Value::String(crate::seqstring::global_bytes(buffer)));
        push(stack, Value::Bool(true))
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
        let write_result = stream.write_all(data.as_bytes());
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

        // A user-visible `Socket` unifies listeners and connected
        // streams, so close has to look in both registries. Streams
        // are checked first because they're far more common at
        // shutdown time. Ids are not globally unique across the two
        // registries (each starts at 0); for finite servers that
        // close exactly one of each that's fine, but multi-socket
        // shutdowns with id-aliasing across registries remain an
        // open design wart.
        {
            let mut streams = STREAMS.lock().unwrap();
            if streams
                .get_mut(socket_id)
                .is_some_and(|slot| slot.is_some())
            {
                streams.free(socket_id);
                return push(stack, Value::Bool(true));
            }
        }
        {
            let mut listeners = LISTENERS.lock().unwrap();
            if listeners
                .get_mut(socket_id)
                .is_some_and(|slot| slot.is_some())
            {
                listeners.free(socket_id);
                return push(stack, Value::Bool(true));
            }
        }
        push(stack, Value::Bool(false))
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_tcp_accept as tcp_accept;
pub use patch_seq_tcp_close as tcp_close;
pub use patch_seq_tcp_connect as tcp_connect;
pub use patch_seq_tcp_listen as tcp_listen;
pub use patch_seq_tcp_read as tcp_read;
pub use patch_seq_tcp_write as tcp_write;

/// Cast between Socket and Int (both directions): identity at runtime.
///
/// Socket is a compile-time-only nominal wrapper over the same i64 file
/// descriptor; the type checker enforces the distinction. This shim exists
/// so codegen can emit a callable symbol for `fd->socket` / `socket->fd`
/// without inventing a new value tag.
///
/// # Safety
/// Stack must have an Int (or Socket-shaped Int) value on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_socket_cast(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "fd<->socket cast: stack is empty");
    let (rest, val) = unsafe { pop(stack) };
    match val {
        Value::Int(fd) => unsafe { push(rest, Value::Int(fd)) },
        _ => panic!("fd<->socket cast: expected Int on stack"),
    }
}

#[cfg(test)]
mod tests;
