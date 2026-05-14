//! Connection pool keyed by `(scheme, host, port)`.
//!
//! Idle keep-alive connections are stored here between requests so a
//! follow-up call to the same host can skip the TCP+TLS handshakes —
//! the only meaningful performance win this rewrite carries vs. the
//! ureq-era client.
//!
//! ## Sizing (conservative v1 defaults)
//!
//! - 8 idle connections per `(scheme, host, port)`
//! - 30s idle timeout
//! - 256 idle connections globally
//!
//! Sized to match Go's `http.DefaultTransport`. Idle slots that exceed
//! the per-host cap or the global cap are dropped on insertion. Idle
//! entries beyond the timeout are dropped on next checkout.
//!
//! ## Reuse safety
//!
//! Before reusing a pooled connection, `is_reusable` does a non-blocking
//! `poll(POLLIN, 0)` on the underlying TCP fd. If the socket is
//! readable but has 0 bytes pending the peer has sent FIN (graceful
//! close); if `POLLHUP/POLLERR` is set the connection is broken;
//! either way the entry is discarded and the caller redials.
//!
//! Checkout order is MRU (push_front + pop_front) so freshly-returned
//! connections are reused first — they're the least likely to have
//! been closed by the peer in the interim.
//!
//! ## Why `Conn` and not `StreamKind`
//!
//! See `conn.rs`. In short: a trait-object handle keeps rustls's drop
//! chain out of the binary for Seq programs that don't use TLS.

use super::conn::Conn;
use super::ssrf::Scheme;
use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

const MAX_PER_HOST: usize = 8;
const MAX_GLOBAL: usize = 256;
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PoolKey {
    pub(crate) scheme: Scheme,
    pub(crate) host: String,
    pub(crate) port: u16,
}

struct PooledConn {
    stream: Conn,
    inserted: Instant,
}

struct Pool {
    idle: HashMap<PoolKey, VecDeque<PooledConn>>,
    total: usize,
}

impl Pool {
    fn new() -> Self {
        Self {
            idle: HashMap::new(),
            total: 0,
        }
    }
}

static POOL: LazyLock<Mutex<Pool>> = LazyLock::new(|| Mutex::new(Pool::new()));

/// Try to check out a live idle connection for `key`. Returns `None`
/// if no entries exist, all entries are expired, or every candidate
/// fails the half-closed peek (caller should dial fresh).
pub(crate) fn checkout(key: &PoolKey) -> Option<Conn> {
    let mut pool = POOL.lock().unwrap();
    let now = Instant::now();
    loop {
        let conn = pool.idle.get_mut(key).and_then(|q| q.pop_front())?;
        pool.total = pool.total.saturating_sub(1);
        if now.duration_since(conn.inserted) > IDLE_TIMEOUT {
            continue;
        }
        if is_reusable(conn.stream.raw_fd()) {
            return Some(conn.stream);
        }
    }
}

/// Return a connection to the pool. Drops it (= closes the underlying
/// socket) when the global cap is reached, when the per-host cap is
/// reached, or when the caller passes `keep_alive=false`. Note: the
/// global cap is checked *first*, so a host with an empty per-host
/// queue can still be refused if the process-wide pool is full
/// elsewhere — same shape as Go's `http.DefaultTransport`. Operators
/// tuning these knobs should raise `MAX_GLOBAL` before `MAX_PER_HOST`
/// to widen the most-likely bottleneck.
pub(crate) fn release(key: PoolKey, stream: Conn, keep_alive: bool) {
    if !keep_alive {
        return;
    }
    let mut pool = POOL.lock().unwrap();
    if pool.total >= MAX_GLOBAL {
        return;
    }
    let entries = pool.idle.entry(key).or_default();
    if entries.len() >= MAX_PER_HOST {
        return;
    }
    entries.push_front(PooledConn {
        stream,
        inserted: Instant::now(),
    });
    pool.total += 1;
}

/// Non-blocking liveness check on the underlying TCP fd. Returns
/// `true` iff the socket is safe to reuse for the next request —
/// the kernel reports no readable bytes, no FIN, no error.
fn is_reusable(fd: std::os::fd::RawFd) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: `pfd` is a valid pointer to one initialised pollfd; nfds=1.
    let rc = unsafe { libc::poll(&mut pfd, 1, 0) };
    if rc < 0 {
        return false;
    }
    if rc == 0 {
        return true;
    }
    if pfd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
        return false;
    }
    // Reached by elimination: rc > 0 and revents indicates POLLIN with
    // no error flag. An idle keep-alive should have nothing to say
    // until we send the next request; readable bytes here mean either
    // (a) the peer sent FIN (read would return 0) or (b) the server
    // pushed unsolicited bytes that would corrupt our framing of the
    // next request. Both shapes mean "discard, dial fresh."
    //
    // This check covers TCP-layer liveness only; a TLS connection
    // with a pending close_notify alert that hasn't surfaced at the
    // transport layer can still slip through. The idempotent-retry
    // path in request::perform_validated catches that case by
    // redialing on the next write/read failure.
    false
}

#[cfg(test)]
pub(crate) fn clear_for_test() {
    let mut pool = POOL.lock().unwrap();
    pool.idle.clear();
    pool.total = 0;
}
