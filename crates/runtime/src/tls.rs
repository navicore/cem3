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

use crate::http_client::conn::Conn;
use crate::stack::{Stack, pop, push};
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

        // The take/reinstall/free dance against the STREAMS registry
        // lives inside tcp.rs so the "slot is reserved across a
        // strand-yielding operation" invariant stays co-located with
        // the registry itself. We just provide the callback that
        // produces the TLS-wrapped stream.
        let ok = crate::tcp::upgrade_tcp_in_place(socket_id, |tcp| build_tls(tcp, hostname));
        if !ok {
            return push_failure(stack);
        }
        let stack = push(stack, Value::Int(socket_id as i64));
        push(stack, Value::Bool(true))
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

/// Variant of `build_tls` exposed to the HTTP client. Returns the
/// handshaked stream already type-erased as `Conn` (a
/// `Box<dyn HttpStream + Send>`).
///
/// Why erase here: the `Box::new(stream) as Conn` cast emits the
/// vtable for `dyn HttpStream` over `StreamOwned<ClientConnection,
/// TcpStream>`, and *that* vtable references rustls's drop chain.
/// Keeping the cast inside `tls.rs` means the vtable is reachable
/// only via `dial_tls`, which is itself reachable only via the HTTP
/// client's HTTPS path. When no Seq program reaches that path,
/// `--gc-sections` strips the vtable and the rustls drop chain
/// disappears from the binary. The HTTP client never holds a
/// concretely-typed TLS stream — it only ever sees `Conn`.
pub(crate) fn dial_tls(tcp: may::net::TcpStream, hostname: String) -> Result<Conn, ()> {
    let stream = build_tls(tcp, hostname)?;
    Ok(Box::new(stream) as Conn)
}

unsafe fn push_failure(stack: Stack) -> Stack {
    unsafe {
        let stack = push(stack, Value::Int(0));
        push(stack, Value::Bool(false))
    }
}
