//! Type-erased connection handle for the HTTP client.
//!
//! The pool and request orchestrator hold `Box<dyn HttpStream + Send>`
//! rather than the runtime-wide `StreamKind` enum. Why: an enum that
//! has both a TCP and a TLS arm pulls in *both* arms' drop glue at any
//! reachable drop site, which forces rustls's per-connection drop
//! chain into the final binary even when no Seq program uses
//! `net.http.*` or `net.tls.client`. (See `docs/design/done/NO_DEAD_CODE.md`
//! and the PR4 review thread.)
//!
//! A trait object decouples the two: the vtable for the TLS-wrapped
//! stream is emitted only inside `tls::dial_tls`, which is itself
//! unreachable from a hello-world binary. `--gc-sections` then strips
//! the rustls types out cleanly.
//!
//! All read/write operations dispatch through the vtable. The extra
//! indirection is one nanosecond per call — negligible against the
//! micro-to-millisecond latency of a TCP/TLS round-trip.

use std::io::{Read, Write};
use std::os::fd::RawFd;

/// What the HTTP client needs from a connection: byte stream IO plus
/// the underlying TCP fd (for the pool's half-closed peek).
pub(crate) trait HttpStream: Read + Write + Send {
    /// Underlying TCP file descriptor, even for TLS-wrapped streams.
    /// Used by the pool's non-blocking `poll(POLLIN, 0)` reuse check.
    fn raw_fd(&self) -> RawFd;
}

impl HttpStream for may::net::TcpStream {
    fn raw_fd(&self) -> RawFd {
        use std::os::fd::AsRawFd;
        self.as_raw_fd()
    }
}

impl HttpStream
    for rustls::StreamOwned<rustls::ClientConnection, may::net::TcpStream>
{
    fn raw_fd(&self) -> RawFd {
        use std::os::fd::AsRawFd;
        self.sock.as_raw_fd()
    }
}

pub(crate) type Conn = Box<dyn HttpStream + Send>;
