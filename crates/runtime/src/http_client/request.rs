//! Request orchestration: parse + SSRF, pool checkout or fresh dial,
//! request/response over the wire, return to pool or drop.
//!
//! Bridges the four `net.http.*` FFI builtins onto a single core
//! `perform_request` so all HTTP methods share the same connection,
//! handshake, pool, and framing paths.
//!
//! Connections are held as `Conn` (a `Box<dyn HttpStream + Send>`).
//! This is the key to the no-dead-code invariant: a TLS-vs-TCP enum
//! would pull rustls's drop chain into every binary, but a trait
//! object hides the concrete type behind a vtable that the linker
//! can strip when no Seq program reaches `tls::dial_tls`.

use super::conn::Conn;
use super::pool::{self, PoolKey};
use super::ssrf::{self, Scheme};
use super::wire;
use super::{build_response_map, error_response};
use crate::value::Value;

/// Issue an HTTP/1.1 request and produce the Seq response Map.
///
/// `body` carries `(content_type, bytes)` for methods with a payload
/// (POST/PUT); pass `None` for GET/DELETE.
pub(crate) fn perform_request(method: &str, url: &str, body: Option<(&str, &[u8])>) -> Value {
    let target = match ssrf::validate_url(url) {
        Ok(t) => t,
        Err(msg) => return error_response(msg),
    };
    if target.addrs.is_empty() {
        return error_response(format!("DNS resolution failed for {}", target.host));
    }
    perform_validated(method, target, body)
}

/// Test-only entry point: skip SSRF and DNS, dial the provided target
/// directly. Lets the in-process integration tests target a loopback
/// listener (which the SSRF validator would otherwise block).
#[cfg(test)]
pub(crate) fn perform_request_with_target(
    method: &str,
    target: ssrf::ValidatedTarget,
    body: Option<(&str, &[u8])>,
) -> Value {
    perform_validated(method, target, body)
}

fn perform_validated(
    method: &str,
    target: ssrf::ValidatedTarget,
    body: Option<(&str, &[u8])>,
) -> Value {
    let key = PoolKey {
        scheme: target.scheme,
        host: target.host.clone(),
        port: target.port,
    };

    // Try the pool once; on wire failure with an idempotent method,
    // retry with a fresh dial. A POST that fails mid-flight is not
    // safe to replay (the server may have processed it), so it
    // propagates the error as-is.
    let pooled = pool::checkout(&key);
    let from_pool = pooled.is_some();

    let stream: Conn = match pooled {
        Some(s) => s,
        None => match dial_fresh(&target) {
            Ok(s) => s,
            Err(msg) => return error_response(msg),
        },
    };

    match run_once(stream, method, &target, body) {
        Ok((resp, stream)) => {
            pool::release(key, stream, resp.keep_alive);
            let ok = (200..300).contains(&resp.status);
            build_response_map(resp.status as i64, resp.body, ok, None)
        }
        Err(msg) => {
            if from_pool && is_idempotent(method) {
                // Pool entry was stale (FIN'd between peek and write,
                // or server closed mid-stream). Dial fresh and retry.
                match dial_fresh(&target) {
                    Ok(stream) => match run_once(stream, method, &target, body) {
                        Ok((resp, stream)) => {
                            pool::release(key, stream, resp.keep_alive);
                            let ok = (200..300).contains(&resp.status);
                            build_response_map(resp.status as i64, resp.body, ok, None)
                        }
                        Err(msg) => error_response(format!("Connection error: {msg}")),
                    },
                    Err(msg) => error_response(msg),
                }
            } else {
                error_response(format!("Connection error: {msg}"))
            }
        }
    }
}

/// One write+read round-trip on an existing stream. Returns the
/// response and the stream (so a successful caller can return it to
/// the pool).
fn run_once(
    mut stream: Conn,
    method: &str,
    target: &ssrf::ValidatedTarget,
    body: Option<(&str, &[u8])>,
) -> Result<(wire::Response, Conn), String> {
    wire::write_request(&mut stream, method, target, body)
        .map_err(|e| format!("write request: {e}"))?;
    let resp = wire::read_response(&mut stream)?;
    Ok((resp, stream))
}

fn dial_fresh(target: &ssrf::ValidatedTarget) -> Result<Conn, String> {
    let tcp = crate::tcp::connect_to_addrs(&target.addrs, target.port).ok_or_else(|| {
        format!(
            "Connection error: all {} addresses for {} unreachable",
            target.addrs.len(),
            target.host
        )
    })?;
    match target.scheme {
        // The cast to `Box<dyn HttpStream + Send>` happens here for
        // plain TCP. The TCP vtable does not reference rustls, so its
        // drop chain is contained.
        Scheme::Http => Ok(Box::new(tcp) as Conn),
        // `tls::dial_tls` performs the cast to the trait object
        // *inside* tls.rs. That keeps the TLS-stream vtable (which
        // does reference the rustls drop chain) inside a module that
        // is itself reachable only via `patch_seq_tls_client` /
        // `tls::dial_tls`. When no Seq program uses either,
        // --gc-sections strips both vtable and drops.
        Scheme::Https => crate::tls::dial_tls(tcp, target.host.clone())
            .map_err(|()| "TLS handshake failed".to_string()),
    }
}

/// Methods that are safe to replay after a transport failure. PUT and
/// DELETE are idempotent per RFC 9110; POST is explicitly not, and
/// HEAD/OPTIONS aren't exposed by the Seq surface.
fn is_idempotent(method: &str) -> bool {
    matches!(method, "GET" | "PUT" | "DELETE")
}
