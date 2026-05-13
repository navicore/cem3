//! UDP Socket Operations for Seq
//!
//! Provides non-blocking UDP datagram operations using May's
//! coroutine-aware I/O. `udp.receive-from` yields the strand
//! while waiting for a datagram instead of blocking the OS thread.
//!
//! These functions are exported with C ABI for LLVM codegen.
//!
//! ## Payloads are byte-clean
//!
//! Datagrams carry whatever bytes the wire delivered — no UTF-8
//! validation. Binary protocols (DNS records, NTP packets, OSC
//! int32 / float32 arguments, multicast TLV, MessagePack-over-UDP)
//! round-trip through `udp.send-to` / `udp.receive-from` byte for
//! byte. See `docs/design/STRING_BYTE_CLEANLINESS.md` for the
//! `SeqString` design that makes this possible.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use may::net::UdpSocket;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};

// Maximum number of concurrent sockets to prevent unbounded growth.
// Same cap as `tcp.rs`.
const MAX_SOCKETS: usize = 10_000;

// Maximum bytes to read per datagram.
//
// UDP datagrams are protocol-capped at 65,507 bytes for IPv4 (the
// `udp.length` header is 16-bit, minus IP+UDP headers), and 65,535
// for IPv6 base-headered datagrams. We use the next power of two
// (65,536) as the receive buffer size — anything larger cannot
// arrive on the wire, so allocating more would be pure waste.
//
// This intentionally diverges from `tcp.rs`'s 1 MB cap, which makes
// sense for streaming reads but not for one-datagram-per-call recv.
const MAX_READ_SIZE: usize = 65_536;

// Socket registry with ID reuse via free list.
//
// Slots hold `Arc<UdpSocket>` rather than the socket directly. Reasons:
//
// - `may::net::UdpSocket`'s I/O methods (`send_to`, `recv_from`,
//   `local_addr`) all take `&self`, so multiple `Arc` clones across
//   strands are safe without any further synchronisation.
//
// - I/O paths clone the `Arc` out of the registry under the lock, then
//   drop the lock before doing the syscall. This is what the previous
//   `take()`-and-restore pattern was reaching for, but with `Arc` we
//   avoid the close-vs-in-flight race: `close` simply sets the slot to
//   `None` (and frees the id) regardless of whether other strands
//   currently hold an `Arc` clone. The in-flight strand's clone keeps
//   the OS socket alive until its `recv_from` / `send_to` returns; the
//   OS-level close only happens when the last `Arc` drops.
//
// - The id bookkeeping is now correct under all races: every successful
//   `close` pushes the id to `free_ids`, even if the slot was being
//   used for I/O.
//
// `tcp.rs` keeps the take-and-restore pattern because `TcpStream::read`
// is `&mut self` — multiple strands cannot share a TcpStream the same
// way. UDP's `&self`-only API is what makes the cleaner shape possible.
struct SocketRegistry<T> {
    sockets: Vec<Option<Arc<T>>>,
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
        let socket = Arc::new(socket);
        if let Some(id) = self.free_ids.pop() {
            self.sockets[id] = Some(socket);
            return Ok(id as i64);
        }
        if self.sockets.len() >= MAX_SOCKETS {
            return Err("Maximum socket limit reached");
        }
        let id = self.sockets.len();
        self.sockets.push(Some(socket));
        Ok(id as i64)
    }

    /// Clone the `Arc` out of the slot so the caller can do I/O after
    /// dropping the registry lock. Returns `None` if the slot is empty
    /// (handle invalid, out of range, or already closed).
    fn checkout(&self, id: usize) -> Option<Arc<T>> {
        self.sockets.get(id).and_then(|slot| slot.clone())
    }

    /// Drop the slot's `Arc`. Returns whether the slot held a socket
    /// (i.e. whether the close had any effect). Idempotent: a second
    /// close on the same id returns `false`. Independent of any
    /// in-flight I/O — those strands hold their own `Arc` clones.
    fn free(&mut self, id: usize) -> bool {
        if let Some(slot) = self.sockets.get_mut(id)
            && slot.is_some()
        {
            *slot = None;
            self.free_ids.push(id);
            return true;
        }
        false
    }
}

static SOCKETS: Mutex<SocketRegistry<UdpSocket>> = Mutex::new(SocketRegistry::new());

/// Bind a UDP socket to a local port.
///
/// Stack effect: ( port -- socket bound-port Bool )
///
/// Binds to `0.0.0.0:port`. `port=0` lets the OS pick a free port; the
/// returned `bound-port` is the actual bound port (equal to `port` if
/// non-zero). On failure pushes `(0, 0, false)`.
///
/// # Safety
/// Stack must have an Int (port) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_udp_bind(stack: Stack) -> Stack {
    unsafe {
        let (stack, port_val) = pop(stack);
        let port = match port_val {
            Value::Int(p) => p,
            _ => return push_bind_failure(stack),
        };

        if !(0..=65535).contains(&port) {
            return push_bind_failure(stack);
        }

        let addr = format!("0.0.0.0:{}", port);
        let socket = match UdpSocket::bind(&addr) {
            Ok(s) => s,
            Err(_) => return push_bind_failure(stack),
        };

        // Capture the actual bound port before the registry takes ownership.
        let bound_port = match socket.local_addr() {
            Ok(addr) => addr.port() as i64,
            Err(_) => return push_bind_failure(stack),
        };

        let mut sockets = SOCKETS.lock().unwrap();
        match sockets.allocate(socket) {
            Ok(socket_id) => {
                let stack = push(stack, Value::Int(socket_id));
                let stack = push(stack, Value::Int(bound_port));
                push(stack, Value::Bool(true))
            }
            Err(_) => push_bind_failure(stack),
        }
    }
}

unsafe fn push_bind_failure(stack: Stack) -> Stack {
    unsafe {
        let stack = push(stack, Value::Int(0));
        let stack = push(stack, Value::Int(0));
        push(stack, Value::Bool(false))
    }
}

/// Send a datagram to a host:port from a bound UDP socket.
///
/// Stack effect: ( bytes host port socket -- Bool )
///
/// Pops `socket`, `port`, `host`, `bytes` (in that order, so `bytes`
/// is below all of them on entry). Returns `false` on type mismatch,
/// invalid socket, address-resolution failure, or send error.
///
/// Host resolution goes through `dns::resolve` (the may-aware DNS
/// worker pool from PR1). Previously this path used
/// `format!("{host}:{port}")` + may's `ToSocketAddrs`, which silently
/// called blocking `getaddrinfo` on the calling may carrier whenever
/// `host` was a DNS name — a latent hazard that PR5 closes. IP
/// literals still work; they round-trip through the resolver's
/// numeric-host fast path. If resolution returns multiple addresses
/// (e.g. localhost → ::1, 127.0.0.1) we try them in order and stop at
/// the first `send_to` that doesn't error.
///
/// # Safety
/// Stack must have Int (socket), Int (port), String (host),
/// String (bytes) — top-down — on entry.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_udp_send_to(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_val) = pop(stack);
        // Reject negative ids before the `as usize` cast: a negative
        // i64 wraps to usize::MAX, which would silently fall through
        // to a benign `None` lookup. Catching it here is a clearer
        // signal than the indirect not-found path.
        let socket_id = match socket_val {
            Value::Int(id) if id >= 0 => id as usize,
            _ => return push(stack, Value::Bool(false)),
        };

        let (stack, port_val) = pop(stack);
        let port = match port_val {
            Value::Int(p) if (0..=65535).contains(&p) => p,
            _ => return push(stack, Value::Bool(false)),
        };

        let (stack, host_val) = pop(stack);
        let host = match host_val {
            Value::String(s) => s,
            _ => return push(stack, Value::Bool(false)),
        };

        let (stack, bytes_val) = pop(stack);
        let bytes = match bytes_val {
            Value::String(s) => s,
            _ => return push(stack, Value::Bool(false)),
        };

        // Clone the Arc<UdpSocket> out of the registry. We don't hold
        // the lock across the syscall, and a concurrent `close` is
        // free to drop the registry's slot reference — our clone keeps
        // the socket alive for the duration of this send.
        let socket = {
            let sockets = SOCKETS.lock().unwrap();
            match sockets.checkout(socket_id) {
                Some(s) => s,
                None => return push(stack, Value::Bool(false)),
            }
        };

        let hostname = host.as_str_or_empty();
        if hostname.is_empty() {
            return push(stack, Value::Bool(false));
        }
        let port_u16 = port as u16;
        // Resolver only emits IP-string forms produced by
        // `SocketAddr::ip().to_string()`; a parse failure here would
        // mean a runtime invariant violation — skip rather than panic
        // the carrier. Same pattern as `tcp::patch_seq_tcp_connect`.
        let addrs: Vec<IpAddr> = crate::dns::resolve(hostname)
            .iter()
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .collect();
        if addrs.is_empty() {
            return push(stack, Value::Bool(false));
        }
        // Walk addresses; first send that doesn't error wins. UDP
        // doesn't have a connect-handshake, so `send_to` failures are
        // typically address-family mismatches (e.g. v6 IP on a v4-only
        // socket) — the next address in the list usually clears it.
        let sent = addrs.iter().any(|ip| {
            socket
                .send_to(bytes.as_bytes(), SocketAddr::new(*ip, port_u16))
                .is_ok()
        });
        push(stack, Value::Bool(sent))
    }
}

/// Receive one datagram from a UDP socket.
///
/// Stack effect: ( socket -- bytes host port Bool )
///
/// Yields the strand until a datagram arrives. On failure pushes
/// `("", "", 0, false)` — invalid socket, recv error, datagram larger
/// than `MAX_READ_SIZE`, or non-UTF-8 payload (see module doc).
///
/// # Safety
/// Stack must have an Int (socket) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_udp_receive_from(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_val) = pop(stack);
        let socket_id = match socket_val {
            Value::Int(id) if id >= 0 => id as usize,
            _ => return push_receive_failure(stack),
        };

        // Clone the Arc<UdpSocket> out of the registry. The receive
        // strand keeps the socket alive even if another strand closes
        // the handle while we're in `recv_from`. When close drops the
        // registry's clone and ours returns, the OS-level close fires.
        let socket = {
            let sockets = SOCKETS.lock().unwrap();
            match sockets.checkout(socket_id) {
                Some(s) => s,
                None => return push_receive_failure(stack),
            }
        };

        let mut buffer = vec![0u8; MAX_READ_SIZE];
        let recv_result = socket.recv_from(&mut buffer);

        let (size, src) = match recv_result {
            Ok(pair) => pair,
            Err(_) => return push_receive_failure(stack),
        };

        buffer.truncate(size);
        // The payload is whatever bytes the wire delivered. We no longer
        // require UTF-8 — datagrams for OSC, DNS, NTP, MessagePack, etc.
        // routinely include high-bit bytes from int32 / float32 / blob
        // fields. The bytes go into a byte-clean SeqString unchanged.
        let stack = push(stack, Value::String(crate::seqstring::global_bytes(buffer)));
        let stack = push(stack, Value::String(src.ip().to_string().into()));
        let stack = push(stack, Value::Int(src.port() as i64));
        push(stack, Value::Bool(true))
    }
}

unsafe fn push_receive_failure(stack: Stack) -> Stack {
    unsafe {
        let stack = push(stack, Value::String("".into()));
        let stack = push(stack, Value::String("".into()));
        let stack = push(stack, Value::Int(0));
        push(stack, Value::Bool(false))
    }
}

/// Close a UDP socket and free its handle.
///
/// Stack effect: ( socket -- Bool )
///
/// Returns `true` if the handle was open (the registry slot held a
/// socket), `false` if it was already invalid (never allocated, or
/// previously closed). Idempotent across redundant calls on the same
/// id.
///
/// Concurrent I/O is safe: any strand mid-`send_to` / `recv_from`
/// holds its own `Arc<UdpSocket>` clone, so closing the registry slot
/// from another strand only drops the registry's reference. The
/// in-flight syscall completes; the OS-level close fires when the
/// last `Arc` is dropped. The id is recycled to the free list as
/// soon as `close` returns, regardless of any in-flight strand.
///
/// # Safety
/// Stack must have an Int (socket) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_udp_close(stack: Stack) -> Stack {
    unsafe {
        let (stack, socket_val) = pop(stack);
        let socket_id = match socket_val {
            Value::Int(id) if id >= 0 => id as usize,
            _ => return push(stack, Value::Bool(false)),
        };

        let mut sockets = SOCKETS.lock().unwrap();
        let existed = sockets.free(socket_id);
        push(stack, Value::Bool(existed))
    }
}

// Public re-exports with short names for in-module callers — the
// `tests` submodule below imports them via `use super::*`. The
// crate-root re-exports in `lib.rs` are the linker-facing aliases.
pub use patch_seq_udp_bind as udp_bind;
pub use patch_seq_udp_close as udp_close;
pub use patch_seq_udp_receive_from as udp_receive_from;
pub use patch_seq_udp_send_to as udp_send_to;

#[cfg(test)]
mod tests;
