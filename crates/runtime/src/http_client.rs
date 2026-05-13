//! HTTP client operations for Seq.
//!
//! Replaces the ureq-era client with a hand-rolled HTTP/1.1
//! implementation that yields the strand on every IO step. Sits on
//! top of the may-aware DNS, TCP, and TLS layers from PR1-PR3 and
//! maintains its own connection pool keyed by `(scheme, host, port)`.
//!
//! ## API (unchanged from the ureq-era surface)
//!
//! ```seq
//! "https://api.example.com/users" net.http.get
//! # Stack: ( Map ) where Map = { "status": 200, "body": "...", "ok": true }
//!
//! "https://api.example.com/users" "{\"name\":\"Alice\"}" "application/json" net.http.post
//! # Stack: ( Map ) where Map = { "status": 201, "body": "...", "ok": true }
//!
//! dup "ok" map.get if
//!   "body" map.get json.decode
//! else
//!   "error" map.get io.write-line
//! then
//! ```
//!
//! ## Response Map
//!
//! - `"status"` (Int): HTTP status code, or 0 on connection-level error.
//! - `"body"` (String): response body as raw bytes (byte-clean — binary downloads round-trip intact).
//! - `"ok"` (Bool): true iff status is 2xx.
//! - `"error"` (String): error message; present only on failure.
//!
//! ## Security: SSRF protection
//!
//! Requests are blocked when the URL's host resolves to a private,
//! loopback, link-local (cloud metadata), or unique-local IP. The
//! check uses the may-aware DNS layer (see `crate::dns::resolve`) and
//! passes its resolved address list to the connect path, so there is
//! exactly one `getaddrinfo` per request and it runs on a dedicated
//! worker thread — never on a may carrier.
//!
//! ## v1 limitations
//!
//! - No redirect following: 3xx is returned to the caller as-is.
//! - No automatic decompression: we send `Accept-Encoding: identity`.
//!   Use `compress.gunzip` etc. on the body if you ask for an encoded
//!   transfer manually.
//! - No per-request timeout (a deadline pass is planned across all
//!   networking layers).
//! - No client certificate authentication, ALPN selection, or
//!   peer-cert inspection — inherited from `net.tls.client`.
//! - No header customisation beyond `Content-Type` (set automatically
//!   for POST/PUT).

pub(crate) mod conn;
mod pool;
mod request;
mod ssrf;
mod wire;

use crate::seqstring::{global_bytes, global_string};
use crate::stack::{Stack, pop, push};
use crate::value::{MapKey, Value};
use std::collections::HashMap;

// Re-export for the existing unit tests that pre-date the submodule
// split.
#[cfg(test)]
pub(crate) use ssrf::{is_dangerous_ipv4, is_dangerous_ipv6, validate_url_for_ssrf};

/// Build the response Map shape that user code consumes.
///
/// `body` is the raw response payload — HTTP bodies are arbitrary
/// octets per RFC 9110, so we store them in a byte-clean SeqString
/// without UTF-8 validation. Seq programs that need text decode the
/// bytes themselves; binary downloads keep the original bytes intact.
pub(crate) fn build_response_map(
    status: i64,
    body: Vec<u8>,
    ok: bool,
    error: Option<String>,
) -> Value {
    let mut map: HashMap<MapKey, Value> = HashMap::new();
    map.insert(
        MapKey::String(global_string("status".to_string())),
        Value::Int(status),
    );
    map.insert(
        MapKey::String(global_string("body".to_string())),
        Value::String(global_bytes(body)),
    );
    map.insert(
        MapKey::String(global_string("ok".to_string())),
        Value::Bool(ok),
    );
    if let Some(err) = error {
        map.insert(
            MapKey::String(global_string("error".to_string())),
            Value::String(global_string(err)),
        );
    }
    Value::Map(Box::new(map))
}

/// Build an error response Map (status=0, ok=false).
pub(crate) fn error_response(error: String) -> Value {
    build_response_map(0, Vec::new(), false, Some(error))
}

/// HTTP GET. `( url -- response )`.
///
/// # Safety
/// Stack must have a String (URL) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_get(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.get: stack is empty");
    let (stack, url_value) = unsafe { pop(stack) };
    match url_value {
        Value::String(url) => {
            let response = request::perform_request("GET", url.as_str_or_empty(), None);
            unsafe { push(stack, response) }
        }
        _ => panic!("http.get: expected String (URL), got {:?}", url_value),
    }
}

/// HTTP POST. `( url body content-type -- response )`.
///
/// # Safety
/// Stack must have three Strings on top: url, body, content-type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_post(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.post: stack is empty");
    let (stack, content_type_value) = unsafe { pop(stack) };
    let (stack, body_value) = unsafe { pop(stack) };
    let (stack, url_value) = unsafe { pop(stack) };
    match (url_value, body_value, content_type_value) {
        (Value::String(url), Value::String(body), Value::String(content_type)) => {
            let response = request::perform_request(
                "POST",
                url.as_str_or_empty(),
                Some((content_type.as_str_or_empty(), body.as_bytes())),
            );
            unsafe { push(stack, response) }
        }
        (url, body, ct) => panic!(
            "http.post: expected (String, String, String), got ({:?}, {:?}, {:?})",
            url, body, ct
        ),
    }
}

/// HTTP PUT. `( url body content-type -- response )`.
///
/// # Safety
/// Stack must have three Strings on top: url, body, content-type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_put(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.put: stack is empty");
    let (stack, content_type_value) = unsafe { pop(stack) };
    let (stack, body_value) = unsafe { pop(stack) };
    let (stack, url_value) = unsafe { pop(stack) };
    match (url_value, body_value, content_type_value) {
        (Value::String(url), Value::String(body), Value::String(content_type)) => {
            let response = request::perform_request(
                "PUT",
                url.as_str_or_empty(),
                Some((content_type.as_str_or_empty(), body.as_bytes())),
            );
            unsafe { push(stack, response) }
        }
        (url, body, ct) => panic!(
            "http.put: expected (String, String, String), got ({:?}, {:?}, {:?})",
            url, body, ct
        ),
    }
}

/// HTTP DELETE. `( url -- response )`.
///
/// # Safety
/// Stack must have a String (URL) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_delete(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.delete: stack is empty");
    let (stack, url_value) = unsafe { pop(stack) };
    match url_value {
        Value::String(url) => {
            let response = request::perform_request("DELETE", url.as_str_or_empty(), None);
            unsafe { push(stack, response) }
        }
        _ => panic!("http.delete: expected String (URL), got {:?}", url_value),
    }
}

#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod tests;
