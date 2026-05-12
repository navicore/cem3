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
//! connected TCP socket and a hostname, returns the *same* Socket id
//! now pointing at a TLS-wrapped stream. The hostname drives SNI and
//! webpki certificate validation; trust roots come from `webpki-roots`.
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
//! - `net.tcp.close` on a TLS-wrapped socket is a *hard* close — the
//!   underlying `TcpStream` is dropped without first sending the TLS
//!   `close_notify` alert. RFC 5246 expects clients to send the alert
//!   before closing; modern servers tolerate truncation but some older
//!   stacks log it as a truncation-attack indicator. A graceful-shutdown
//!   variant is a planned follow-up.
//! - No client-certificate authentication (mTLS).
//! - No caller-side ALPN selection — rustls defaults apply.
//! - No way to inspect the negotiated cipher / peer certificate from
//!   Seq. Planned follow-ups once the four-layer stack stabilises.

use crate::stack::{Stack, pop, push};
use crate::tcp::{STREAMS, StreamKind};
use crate::value::Value;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::sync::{Arc, LazyLock};

/// Process-wide TLS client config. Trust roots are the Mozilla CA
/// bundle shipped by `webpki-roots`; the `ring` crypto provider is
/// installed defensively here so we don't depend on rustls's
/// crate-features auto-install — if any transitive dep ever enables
/// `aws_lc_rs` alongside `ring`, the auto-install path would panic at
/// first use ("multiple default providers"). Cached for the process
/// lifetime — the `Arc<ClientConfig>` is cheap to clone into each
/// handshake.
static TLS_CONFIG: LazyLock<Arc<ClientConfig>> = LazyLock::new(|| {
    // Ignore the "already installed" Err — if another module beat us
    // to it (or the crate-features path raced us), the provider is
    // still ring, which is the only one this build pulls.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
});

/// Upgrade a connected Socket to TLS.
///
/// Stack effect: `( Socket String -- Socket Bool )` — top of stack
/// is the hostname (String), with the existing TCP socket id beneath
/// it. On success, returns `(socket_id, true)` where `socket_id` is
/// the *same* id the caller passed in: the registry slot is upgraded
/// in place from `Tcp` to `Tls`, so any caller-side data structures
/// keyed on the socket id remain valid. On failure (empty hostname,
/// type mismatch, wrong-kind socket, handshake error, no slot found),
/// returns `(0, false)`; on the failure paths that already took the
/// stream out of the registry, the underlying socket is closed (the
/// `TcpStream` is dropped) and the slot is freed.
///
/// # Safety
/// Stack must have a String (hostname) on top of a Socket (Int).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tls_client(stack: Stack) -> Stack {
    unsafe {
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
        //
        // Critically, we do NOT call free(socket_id) here. The slot
        // is now Some-but-None (the Vec entry exists, but holds an
        // empty Option) — which reserves the id for the upgrade.
        // Freeing would push id onto the free list, and the handshake
        // below yields the strand on every network round-trip; another
        // strand could allocate that id in the interim, leaving the
        // user holding a Socket integer that now refers to someone
        // else's stream.
        let tcp = match take_tcp(socket_id) {
            Some(t) => t,
            None => return push_failure(stack),
        };

        // Build the rustls ClientConnection and run the handshake to
        // completion. complete_io drives reads/writes on the
        // underlying may TcpStream, which yields the strand
        // cooperatively while waiting on the network.
        let stream = match build_tls(tcp, hostname) {
            Ok(s) => s,
            Err(()) => {
                // Handshake (or earlier setup) failed. build_tls
                // dropped the TcpStream, so the socket is closed.
                // Release the id back to the free list so it can be
                // reused.
                STREAMS.lock().unwrap().free(socket_id);
                return push_failure(stack);
            }
        };

        // Reinstall under the *same* id. The slot has been reserved
        // for us since take_tcp; no other strand could have claimed
        // it across the handshake yield.
        let installed = {
            let mut streams = STREAMS.lock().unwrap();
            match streams.get_mut(socket_id) {
                Some(slot) => {
                    *slot = Some(StreamKind::Tls(Box::new(stream)));
                    true
                }
                None => false,
            }
        };
        if !installed {
            // The Vec shrunk under us — currently impossible with the
            // append-only registry, but treated as a failure rather
            // than a panic so a future eviction policy doesn't blow up
            // the process.
            return push_failure(stack);
        }
        let stack = push(stack, Value::Int(socket_id as i64));
        push(stack, Value::Bool(true))
    }
}

/// Take the underlying TcpStream out of STREAMS at `id`, only if the
/// slot holds a `Tcp` variant. A `Tls` variant or empty slot
/// short-circuits to `None`; a wrong-kind variant is restored so
/// `tls.client` on a TLS socket doesn't accidentally destroy it.
///
/// On `Some` return, the slot at `id` is left holding `None` (i.e.
/// reserved for the caller). The caller MUST either reinstall a
/// value into that slot or call `free(id)`.
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
/// closes the socket). The hostname is moved in: rustls's
/// `ServerName<'static>` takes an owned `String`, so threading the
/// caller's owned hostname through avoids a redundant clone.
fn build_tls(
    mut tcp: may::net::TcpStream,
    hostname: String,
) -> Result<StreamOwned<ClientConnection, may::net::TcpStream>, ()> {
    let server_name = ServerName::try_from(hostname).map_err(|_| ())?;
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
