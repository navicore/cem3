//! TLS client for Seq.
//!
//! Wraps a connected `may::net::TcpStream` in a `rustls::ClientConnection`
//! and stores the result in the shared `STREAMS` registry as
//! `StreamKind::Tls`. Existing `net.tcp.read` / `net.tcp.write` /
//! `net.tcp.close` builtins dispatch over the `StreamKind` enum
//! transparently — the user upgrades a Socket and keeps using it.
//!
//! ## Surface
//!
//! `net.tls.client ( Socket String -- Socket Bool )` — consumes a
//! connected TCP socket and a hostname, returns a new Socket id whose
//! reads/writes go through TLS. The hostname drives SNI and webpki
//! certificate validation; trust roots come from `webpki-roots`.
//!
//! ## Handshake timing
//!
//! Eager: the handshake completes inside this builtin via
//! `conn.complete_io(&mut tcp)`. A bad cert, expired cert, hostname
//! mismatch, or any other TLS-layer error surfaces as
//! `(0, false)` — matching the way every other fallible Seq
//! networking word reports failure. A subsequent `net.tcp.read` reads
//! application data only.
//!
//! ## Known limitations (v1)
//!
//! - No client-certificate authentication (mTLS).
//! - No caller-side ALPN selection — rustls defaults apply.
//! - No way to inspect the negotiated cipher / peer certificate from
//!   Seq. Planned follow-ups once the four-layer stack stabilises.

use crate::seqstring::SeqString;
use crate::stack::{Stack, pop, push};
use crate::tcp::{STREAMS, StreamKind};
use crate::value::Value;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::sync::{Arc, LazyLock};

/// Process-wide TLS client config. Trust roots are the Mozilla CA
/// bundle shipped by `webpki-roots`; rustls's auto-installed `ring`
/// provider (the only one this build pulls) handles crypto. Cached
/// for the process lifetime — there's no per-connection state here,
/// so the `Arc<ClientConfig>` is cheap to clone into each handshake.
static TLS_CONFIG: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
});

/// Upgrade a connected Socket to TLS.
///
/// Stack effect: `( Socket String -- Socket Bool )` — port-of-call is the
/// hostname (String, top) and the existing TCP socket id (Socket,
/// second-from-top). On success, returns `(new_socket_id, true)` where
/// `new_socket_id` is a fresh Socket pointing at the TLS-wrapped
/// stream; the old id is freed and no longer valid. On failure (empty
/// hostname, type mismatch, wrong-kind socket, handshake error,
/// registry exhaustion), returns `(0, false)` and the underlying TCP
/// stream is dropped (which closes the socket).
///
/// # Safety
/// Stack must have a String (hostname) on top of a Socket (Int).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tls_client(stack: Stack) -> Stack {
    unsafe {
        // Hostname is on top; pop it first.
        let (stack, host_val) = pop(stack);
        let host = match host_val {
            Value::String(s) => s,
            _ => return push_failure(stack),
        };
        let (stack, sock_val) = pop(stack);
        let socket_id = match sock_val {
            Value::Int(id) => id as usize,
            _ => return push_failure(stack),
        };
        let hostname = host.as_str_or_empty().to_string();
        if hostname.is_empty() {
            return push_failure(stack);
        }

        // Pull the underlying TcpStream out of the registry. If the
        // id refers to a TLS-wrapped stream (double-upgrade) or
        // doesn't exist, fail without disturbing the slot.
        let tcp = match take_tcp(socket_id) {
            Some(t) => t,
            None => return push_failure(stack),
        };
        // From this point the slot at `socket_id` is None; release
        // the id back to the free list so subsequent failure paths
        // don't strand it. The TLS-wrapped stream (on success) gets
        // a brand new id, not this one.
        STREAMS.lock().unwrap().free(socket_id);

        // Build the rustls ClientConnection and run the handshake to
        // completion. complete_io drives reads/writes on the
        // underlying may TcpStream, which yields the strand
        // cooperatively while waiting on the network. On any failure
        // here the TcpStream is dropped, which closes the underlying
        // socket — no resource leak.
        let stream = match build_tls(tcp, &hostname, host) {
            Ok(s) => s,
            Err(()) => return push_failure(stack),
        };

        let new_id = match STREAMS
            .lock()
            .unwrap()
            .allocate(StreamKind::Tls(Box::new(stream)))
        {
            Ok(id) => id,
            Err(_) => return push_failure(stack),
        };
        let stack = push(stack, Value::Int(new_id));
        push(stack, Value::Bool(true))
    }
}

/// Take the underlying TcpStream out of STREAMS at `id`, only if the
/// slot holds a `Tcp` variant. A `Tls` variant or empty slot
/// short-circuits to `None`; a wrong-kind variant is restored so
/// `tls.client` on a TLS socket doesn't accidentally destroy it.
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

/// Build a fully-handshaked TLS stream over `tcp`. The TCP stream is
/// consumed regardless of outcome — on Err, it is dropped (which
/// closes the socket).
fn build_tls(
    mut tcp: may::net::TcpStream,
    hostname: &str,
    host_seqstring: SeqString,
) -> Result<StreamOwned<ClientConnection, may::net::TcpStream>, ()> {
    // rustls ServerName parses DNS names and IP literals; reuse the
    // owned String form so we don't fight the 'static lifetime on
    // ServerName<'static>. host_seqstring stays in scope until we
    // hand the cloned hostname to ServerName.
    let _ = host_seqstring;
    let server_name = ServerName::try_from(hostname.to_string()).map_err(|_| ())?;
    let mut conn = ClientConnection::new(TLS_CONFIG.clone(), server_name).map_err(|_| ())?;
    conn.complete_io(&mut tcp).map_err(|_| ())?;
    Ok(StreamOwned::new(conn, tcp))
}

unsafe fn push_failure(stack: Stack) -> Stack {
    unsafe {
        let stack = push(stack, Value::Int(0));
        push(stack, Value::Bool(false))
    }
}
