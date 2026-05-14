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

/// Per-IO timeout for HTTP request/response in milliseconds.
/// Default 30 000ms (matches ureq's old default).
///
/// Bounds each individual read/write inside the wire layer. Catches
/// silent-server stalls and the EOF-framed-body slow-trickle attack
/// — any single read taking longer than this surfaces as a wire
/// error. Read once via LazyLock; override per-process with
/// `SEQ_HTTP_REQUEST_TIMEOUT_MS`.
const DEFAULT_HTTP_REQUEST_TIMEOUT_MS: u64 = 30_000;

static HTTP_REQUEST_TIMEOUT: std::sync::LazyLock<std::time::Duration> =
    std::sync::LazyLock::new(|| {
        let ms = std::env::var("SEQ_HTTP_REQUEST_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_HTTP_REQUEST_TIMEOUT_MS);
        std::time::Duration::from_millis(ms)
    });

/// Test-only override for `HTTP_REQUEST_TIMEOUT`. When `Some`, takes
/// precedence over the LazyLock-cached value (which can't be reset
/// once initialised). Lets timeout tests drive deterministic short
/// deadlines without depending on env-var read order.
#[cfg(test)]
static HTTP_REQUEST_TIMEOUT_OVERRIDE: std::sync::Mutex<Option<std::time::Duration>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn set_test_http_request_timeout(dur: Option<std::time::Duration>) {
    *HTTP_REQUEST_TIMEOUT_OVERRIDE.lock().unwrap() = dur;
}

fn http_request_timeout() -> std::time::Duration {
    #[cfg(test)]
    if let Some(dur) = *HTTP_REQUEST_TIMEOUT_OVERRIDE.lock().unwrap() {
        return dur;
    }
    *HTTP_REQUEST_TIMEOUT
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
    // Bound each individual read/write inside this round-trip. Each
    // wire-layer call (write_request, read_response, and every
    // intermediate read for chunked / EOF-framed bodies) is capped at
    // HTTP_REQUEST_TIMEOUT. A peer that goes silent mid-response or
    // sends a byte-per-N-seconds slow-trickle hits the per-op limit
    // and surfaces as a wire error.
    //
    // Reset to None after the round-trip so a connection returned to
    // the pool doesn't carry a stale per-op deadline for the next
    // user. The next caller will set its own.
    let timeout = Some(http_request_timeout());
    let _ = stream.set_read_timeout(timeout);
    let _ = stream.set_write_timeout(timeout);

    let result = (|| {
        wire::write_request(&mut stream, method, target, body)
            .map_err(|e| format!("write request: {e}"))?;
        wire::read_response(&mut stream)
    })();

    let _ = stream.set_read_timeout(None);
    let _ = stream.set_write_timeout(None);

    let resp = result?;
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
