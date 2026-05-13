//! TCP Socket Operations for Seq
//!
//! Provides non-blocking TCP socket operations using May's coroutine-aware I/O.
//! All operations yield the strand instead of blocking the OS thread.
//!
//! These functions are exported with C ABI for LLVM codegen.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use may::net::{TcpListener, TcpStream};
use rustls::{ClientConnection, StreamOwned};
use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;

/// What a Socket id actually points at in the STREAMS registry.
///
/// `Tcp` is a connected plain stream (the only kind PR1/PR2 produced).
/// `Tls` is a connected stream that has been upgraded via
/// `net.tls.client` — every read/write goes through rustls over the
/// same may-aware TcpStream. Both arms implement `Read + Write`, so
/// `net.tcp.read` / `net.tcp.write` / `net.tcp.close` dispatch over
/// either variant without the caller knowing the difference.
///
/// The TLS arm is boxed not to shrink the *enum* (size_of TcpStream is
/// non-trivial and tends to dominate the discriminant size anyway) but
/// to keep the `StreamOwned<ClientConnection, _>` payload — which
/// embeds rustls's per-connection record buffers — off the registry
/// allocation. Without the box, every plain-TCP allocation would have
/// to find a contiguous chunk large enough for the TLS variant.
enum StreamKind {
    Tcp(TcpStream),
    Tls(Box<StreamOwned<ClientConnection, TcpStream>>),
}

impl Read for StreamKind {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            StreamKind::Tcp(s) => s.read(buf),
            StreamKind::Tls(s) => s.read(buf),
        }
    }
}

impl Write for StreamKind {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            StreamKind::Tcp(s) => s.write(buf),
            StreamKind::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            StreamKind::Tcp(s) => s.flush(),
            StreamKind::Tls(s) => s.flush(),
        }
    }
}

// Maximum number of concurrent connections to prevent unbounded growth
const MAX_SOCKETS: usize = 10_000;

// Architectural cap for any future read-N or full-body socket reader
// (e.g. an HTTP-client migration off ureq). The current `tcp.read` does
// one 4 KB read per call and so doesn't need to consult this; it stays
// here as the ceiling that bounded-buffer variants must respect.
#[allow(dead_code)]
const MAX_READ_SIZE: usize = 1_048_576; // 1 MB

// Socket registry with ID reuse via free list. Private — the only
// cross-module operation is `upgrade_tcp_in_place`, which hides the
// registry behind a callback.
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

// Global registry for TCP listeners and streams. STREAMS holds a
// StreamKind so that plain-TCP and TLS-wrapped sockets share one id
// space (and one set of read/write/close builtins).
//
// The "free + allocate is dangerous across a strand yield" invariant
// lives entirely inside this module — `tls::upgrade_tcp_to_tls` (the
// only cross-module reason to touch STREAMS) goes through
// `upgrade_tcp_in_place`, which holds the slot reserved (Some(None))
// across the caller's yield-able TLS handshake so no concurrent
// strand can grab the id.
static LISTENERS: Mutex<SocketRegistry<TcpListener>> = Mutex::new(SocketRegistry::new());
static STREAMS: Mutex<SocketRegistry<StreamKind>> = Mutex::new(SocketRegistry::new());

/// Take the underlying `TcpStream` out of `STREAMS[id]`, only if the
/// slot holds a `Tcp` variant. A `Tls` variant or empty slot
/// short-circuits to `None`; a wrong-kind variant is restored so a
/// double-upgrade caller doesn't accidentally destroy the connection.
///
/// On `Some` return, the slot at `id` is left holding `None` —
/// reserved for the caller. The caller MUST either reinstall a value
/// into that slot or release the id via `free_stream`. The recommended
/// way to do this safely across a strand-yielding operation (e.g. a
/// TLS handshake) is `upgrade_tcp_in_place`, which handles the
/// reinstall/free for you.
fn take_tcp(id: usize) -> Option<may::net::TcpStream> {
    let mut streams = STREAMS.lock().unwrap();
    let slot = streams.get_mut(id)?;
    match slot.take() {
        Some(StreamKind::Tcp(t)) => Some(t),
        Some(other) => {
            *slot = Some(other);
            None
        }
        None => None,
    }
}

fn free_stream(id: usize) {
    STREAMS.lock().unwrap().free(id);
}

/// In-place upgrade of a TCP socket to its TLS-wrapped form.
///
/// The flow:
/// 1. Take the underlying `TcpStream` out of `STREAMS[id]` via
///    `take_tcp`. The slot is now reserved (Some(None)) — concurrent
///    strands can't allocate this id while the caller runs `f`.
/// 2. Hand the stream to `f` (which is allowed to yield the strand —
///    typically the TLS handshake).
/// 3. On success, wrap the returned StreamOwned in `StreamKind::Tls`
///    and reinstall into the same slot — the Socket id is preserved.
/// 4. On failure, drop the (already-consumed) `TcpStream` and release
///    the id back to the free list.
///
/// Returns `true` iff the slot was found, the upgrade succeeded, and
/// the reinstall completed. The Socket id is unchanged on success.
///
/// Crate-internal entry point for `tls::patch_seq_tls_client`. Keeps
/// the "reserve across yield" invariant inside this module.
pub(crate) fn upgrade_tcp_in_place<F>(id: usize, f: F) -> bool
where
    F: FnOnce(
        may::net::TcpStream,
    )
        -> Result<rustls::StreamOwned<rustls::ClientConnection, may::net::TcpStream>, ()>,
{
    let tcp = match take_tcp(id) {
        Some(t) => t,
        None => return false,
    };
    let stream = match f(tcp) {
        Ok(s) => s,
        Err(()) => {
            // `f` consumed (and dropped) the TcpStream — socket closed.
            // Release the reserved id back to the free list.
            free_stream(id);
            return false;
        }
    };
    let mut streams = STREAMS.lock().unwrap();
    match streams.get_mut(id) {
        Some(slot) => {
            *slot = Some(StreamKind::Tls(Box::new(stream)));
            true
        }
        // Currently impossible with the append-only registry; future
        // eviction would surface here as a clean false rather than a
        // panic.
        None => false,
    }
}

/// Connect to the first reachable address in `addrs` at `port`.
///
/// Walks the list in order, returning the first successful
/// `may::net::TcpStream::connect`. Yields the strand on each
/// SYN/SYN-ACK round-trip. Returns `None` if every address fails.
///
/// Building `SocketAddr` directly from the `IpAddr` avoids the
/// IPv6 string-formatting trap (`"::1:80"` is not a parseable
/// SocketAddr — brackets are required in that form).
///
/// Exposed to the crate so the HTTP client (which pre-resolves +
/// SSRF-validates before connecting) can dial without re-resolving.
pub(crate) fn connect_to_addrs(addrs: &[IpAddr], port: u16) -> Option<TcpStream> {
    addrs
        .iter()
        .find_map(|ip| TcpStream::connect(SocketAddr::new(*ip, port)).ok())
}

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

        let addrs = crate::dns::resolve_to_ips(hostname);
        if addrs.is_empty() {
            let stack = push(stack, Value::Int(0));
            return push(stack, Value::Bool(false));
        }
        let stream = match connect_to_addrs(&addrs, port as u16) {
            Some(s) => s,
            None => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        let mut streams = STREAMS.lock().unwrap();
        match streams.allocate(StreamKind::Tcp(stream)) {
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
        match streams.allocate(StreamKind::Tcp(stream)) {
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
pub use patch_seq_tcp_local_port as tcp_local_port;
pub use patch_seq_tcp_read as tcp_read;
pub use patch_seq_tcp_write as tcp_write;

/// Get the local port a Socket is bound to.
///
/// Stack effect: `( Socket -- Int Bool )`
///
/// Works on both listeners (returns the port from `net.tcp.listen`,
/// useful when `0` was passed to let the OS pick) and connected
/// streams (returns the ephemeral local port the kernel chose for
/// the connection). For TLS-wrapped sockets, returns the underlying
/// TCP local port.
///
/// Returns `(0, false)` when the socket id is invalid or the OS
/// can't report the local address.
///
/// # Safety
/// Stack must have an Int (socket_id) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tcp_local_port(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_id_val) = pop(stack);
        let socket_id = match socket_id_val {
            Value::Int(id) => id as usize,
            _ => {
                let stack = push(stack, Value::Int(0));
                return push(stack, Value::Bool(false));
            }
        };

        // Streams first, then listeners — same dispatch order as close.
        let port: Option<u16> = {
            let mut streams = STREAMS.lock().unwrap();
            streams
                .get_mut(socket_id)
                .and_then(|slot| slot.as_ref())
                .and_then(|sk| match sk {
                    StreamKind::Tcp(s) => s.local_addr().ok().map(|a| a.port()),
                    StreamKind::Tls(s) => s.sock.local_addr().ok().map(|a| a.port()),
                })
        };
        if let Some(port) = port {
            let stack = push(stack, Value::Int(port as i64));
            return push(stack, Value::Bool(true));
        }
        let port: Option<u16> = {
            let mut listeners = LISTENERS.lock().unwrap();
            listeners
                .get_mut(socket_id)
                .and_then(|slot| slot.as_ref())
                .and_then(|l| l.local_addr().ok())
                .map(|a| a.port())
        };
        if let Some(port) = port {
            let stack = push(stack, Value::Int(port as i64));
            return push(stack, Value::Bool(true));
        }

        let stack = push(stack, Value::Int(0));
        push(stack, Value::Bool(false))
    }
}

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
