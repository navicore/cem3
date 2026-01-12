//! HTTP client operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # API
//!
//! ```seq
//! # GET request
//! "https://api.example.com/users" http.get
//! # Stack: ( Map ) where Map = { "status": 200, "body": "...", "ok": true }
//!
//! # POST request
//! "https://api.example.com/users" "{\"name\":\"Alice\"}" "application/json" http.post
//! # Stack: ( Map ) where Map = { "status": 201, "body": "...", "ok": true }
//!
//! # Check response
//! dup "ok" map.get if
//!   "body" map.get json.decode  # Process JSON body
//! else
//!   "error" map.get io.write-line  # Handle error
//! then
//! ```
//!
//! # Response Map
//!
//! All HTTP operations return a Map with:
//! - `"status"` (Int): HTTP status code (200, 404, 500, etc.) or 0 on connection error
//! - `"body"` (String): Response body as text
//! - `"ok"` (Bool): true if status is 2xx, false otherwise
//! - `"error"` (String): Error message (only present on failure)
//!
//! # Security: SSRF Protection
//!
//! This HTTP client includes built-in protection against Server-Side Request Forgery
//! (SSRF) attacks. The following are automatically blocked:
//!
//! - **Localhost**: `localhost`, `*.localhost`, `127.x.x.x`
//! - **Private networks**: `10.x.x.x`, `172.16-31.x.x`, `192.168.x.x`
//! - **Link-local/Cloud metadata**: `169.254.x.x` (blocks AWS/GCP/Azure metadata endpoints)
//! - **IPv6 private**: loopback (`::1`), link-local (`fe80::/10`), unique local (`fc00::/7`)
//! - **Non-HTTP schemes**: `file://`, `ftp://`, `gopher://`, etc.
//!
//! Blocked requests return an error response with `ok=false` and an explanatory message.
//!
//! **Additional recommendations for defense in depth**:
//! - Use domain allowlists for sensitive applications
//! - Apply network-level egress filtering
//!
//! # Resource Limits
//!
//! - **Timeout**: 30 seconds per request (prevents indefinite hangs)
//! - **Max body size**: 10 MB (prevents memory exhaustion)
//! - **TLS**: Enabled by default via rustls (no OpenSSL dependency)
//! - **Connection pooling**: Enabled via shared agent instance

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::{MapKey, Value};

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, ToSocketAddrs};
use std::sync::LazyLock;
use std::time::Duration;

/// Default timeout for HTTP requests (30 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum response body size (10 MB)
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

/// Global HTTP agent for connection pooling
/// Using LazyLock for thread-safe lazy initialization
static HTTP_AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()
});

/// Check if an IPv4 address is in a private/dangerous range
fn is_dangerous_ipv4(ip: Ipv4Addr) -> bool {
    // Loopback: 127.0.0.0/8
    if ip.is_loopback() {
        return true;
    }
    // Private: 10.0.0.0/8
    if ip.octets()[0] == 10 {
        return true;
    }
    // Private: 172.16.0.0/12
    if ip.octets()[0] == 172 && (ip.octets()[1] >= 16 && ip.octets()[1] <= 31) {
        return true;
    }
    // Private: 192.168.0.0/16
    if ip.octets()[0] == 192 && ip.octets()[1] == 168 {
        return true;
    }
    // Link-local: 169.254.0.0/16 (includes cloud metadata endpoints)
    if ip.octets()[0] == 169 && ip.octets()[1] == 254 {
        return true;
    }
    // Broadcast
    if ip.is_broadcast() {
        return true;
    }
    false
}

/// Check if an IPv6 address is in a private/dangerous range
fn is_dangerous_ipv6(ip: Ipv6Addr) -> bool {
    // Loopback: ::1
    if ip.is_loopback() {
        return true;
    }
    // Link-local: fe80::/10
    let segments = ip.segments();
    if (segments[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    // Unique local: fc00::/7
    if (segments[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    // IPv4-mapped IPv6 addresses: check the embedded IPv4
    if let Some(ipv4) = ip.to_ipv4_mapped() {
        return is_dangerous_ipv4(ipv4);
    }
    false
}

/// Check if an IP address is dangerous (private, loopback, link-local, etc.)
fn is_dangerous_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_dangerous_ipv4(v4),
        IpAddr::V6(v6) => is_dangerous_ipv6(v6),
    }
}

/// Validate URL for SSRF protection
/// Returns Ok(()) if safe, Err(message) if blocked
fn validate_url_for_ssrf(url: &str) -> Result<(), String> {
    // Parse the URL
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(e) => return Err(format!("Invalid URL: {}", e)),
    };

    // Only allow http and https schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Blocked scheme '{}': only http/https allowed",
                scheme
            ));
        }
    }

    // Get the host
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return Err("URL has no host".to_string()),
    };

    // Block obvious localhost variants
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower == "localhost.localdomain"
        || host_lower.ends_with(".localhost")
    {
        return Err("Blocked: localhost access not allowed".to_string());
    }

    // Get port (default to 80/443)
    let port = parsed
        .port()
        .unwrap_or(if parsed.scheme() == "https" { 443 } else { 80 });

    // Resolve hostname to IP addresses and check each one
    let addr_str = format!("{}:{}", host, port);
    match addr_str.to_socket_addrs() {
        Ok(addrs) => {
            for addr in addrs {
                if is_dangerous_ip(addr.ip()) {
                    return Err(format!(
                        "Blocked: {} resolves to private/internal IP {}",
                        host,
                        addr.ip()
                    ));
                }
            }
        }
        Err(_) => {
            // DNS resolution failed - allow the request to proceed
            // (ureq will handle the DNS error appropriately)
        }
    }

    Ok(())
}

/// Build a response map from status, body, ok flag, and optional error
fn build_response_map(status: i64, body: String, ok: bool, error: Option<String>) -> Value {
    let mut map: HashMap<MapKey, Value> = HashMap::new();

    map.insert(
        MapKey::String(global_string("status".to_string())),
        Value::Int(status),
    );
    map.insert(
        MapKey::String(global_string("body".to_string())),
        Value::String(global_string(body)),
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

/// Build an error response map
fn error_response(error: String) -> Value {
    build_response_map(0, String::new(), false, Some(error))
}

/// Perform HTTP GET request
///
/// Stack effect: ( url -- response )
///
/// Returns a Map with status, body, ok, and optionally error.
///
/// # Safety
/// Stack must have a String (URL) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_get(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.get: stack is empty");

    let (stack, url_value) = unsafe { pop(stack) };

    match url_value {
        Value::String(url) => {
            let response = perform_get(url.as_str());
            unsafe { push(stack, response) }
        }
        _ => panic!(
            "http.get: expected String (URL) on stack, got {:?}",
            url_value
        ),
    }
}

/// Perform HTTP POST request
///
/// Stack effect: ( url body content-type -- response )
///
/// Returns a Map with status, body, ok, and optionally error.
///
/// # Safety
/// Stack must have three String values on top (url, body, content-type)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_post(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.post: stack is empty");

    let (stack, content_type_value) = unsafe { pop(stack) };
    let (stack, body_value) = unsafe { pop(stack) };
    let (stack, url_value) = unsafe { pop(stack) };

    match (url_value, body_value, content_type_value) {
        (Value::String(url), Value::String(body), Value::String(content_type)) => {
            let response = perform_post(url.as_str(), body.as_str(), content_type.as_str());
            unsafe { push(stack, response) }
        }
        (url, body, ct) => panic!(
            "http.post: expected (String, String, String) on stack, got ({:?}, {:?}, {:?})",
            url, body, ct
        ),
    }
}

/// Perform HTTP PUT request
///
/// Stack effect: ( url body content-type -- response )
///
/// Returns a Map with status, body, ok, and optionally error.
///
/// # Safety
/// Stack must have three String values on top (url, body, content-type)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_put(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.put: stack is empty");

    let (stack, content_type_value) = unsafe { pop(stack) };
    let (stack, body_value) = unsafe { pop(stack) };
    let (stack, url_value) = unsafe { pop(stack) };

    match (url_value, body_value, content_type_value) {
        (Value::String(url), Value::String(body), Value::String(content_type)) => {
            let response = perform_put(url.as_str(), body.as_str(), content_type.as_str());
            unsafe { push(stack, response) }
        }
        (url, body, ct) => panic!(
            "http.put: expected (String, String, String) on stack, got ({:?}, {:?}, {:?})",
            url, body, ct
        ),
    }
}

/// Perform HTTP DELETE request
///
/// Stack effect: ( url -- response )
///
/// Returns a Map with status, body, ok, and optionally error.
///
/// # Safety
/// Stack must have a String (URL) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_delete(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "http.delete: stack is empty");

    let (stack, url_value) = unsafe { pop(stack) };

    match url_value {
        Value::String(url) => {
            let response = perform_delete(url.as_str());
            unsafe { push(stack, response) }
        }
        _ => panic!(
            "http.delete: expected String (URL) on stack, got {:?}",
            url_value
        ),
    }
}

/// Handle HTTP response result and convert to Value
fn handle_response(result: Result<ureq::Response, ureq::Error>) -> Value {
    match result {
        Ok(response) => {
            let status = response.status() as i64;
            let ok = (200..300).contains(&response.status());

            match response.into_string() {
                Ok(body) => {
                    if body.len() > MAX_BODY_SIZE {
                        error_response(format!(
                            "Response body too large ({} bytes, max {})",
                            body.len(),
                            MAX_BODY_SIZE
                        ))
                    } else {
                        build_response_map(status, body, ok, None)
                    }
                }
                Err(e) => error_response(format!("Failed to read response body: {}", e)),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            // HTTP error status (4xx, 5xx)
            let body = response.into_string().unwrap_or_default();
            build_response_map(
                code as i64,
                body,
                false,
                Some(format!("HTTP error: {}", code)),
            )
        }
        Err(ureq::Error::Transport(e)) => {
            // Connection/transport error
            error_response(format!("Connection error: {}", e))
        }
    }
}

/// Internal: Perform GET request
fn perform_get(url: &str) -> Value {
    // SSRF protection: validate URL before making request
    if let Err(msg) = validate_url_for_ssrf(url) {
        return error_response(msg);
    }
    handle_response(HTTP_AGENT.get(url).call())
}

/// Internal: Perform POST request
fn perform_post(url: &str, body: &str, content_type: &str) -> Value {
    // SSRF protection: validate URL before making request
    if let Err(msg) = validate_url_for_ssrf(url) {
        return error_response(msg);
    }
    handle_response(
        HTTP_AGENT
            .post(url)
            .set("Content-Type", content_type)
            .send_string(body),
    )
}

/// Internal: Perform PUT request
fn perform_put(url: &str, body: &str, content_type: &str) -> Value {
    // SSRF protection: validate URL before making request
    if let Err(msg) = validate_url_for_ssrf(url) {
        return error_response(msg);
    }
    handle_response(
        HTTP_AGENT
            .put(url)
            .set("Content-Type", content_type)
            .send_string(body),
    )
}

/// Internal: Perform DELETE request
fn perform_delete(url: &str) -> Value {
    // SSRF protection: validate URL before making request
    if let Err(msg) = validate_url_for_ssrf(url) {
        return error_response(msg);
    }
    handle_response(HTTP_AGENT.delete(url).call())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: HTTP tests require network access and a running server
    // Unit tests here focus on the response building logic

    #[test]
    fn test_build_response_map_success() {
        let response = build_response_map(200, "Hello".to_string(), true, None);

        match response {
            Value::Map(map_data) => {
                let map = map_data.as_ref();

                // Check status
                let status_key = MapKey::String(global_string("status".to_string()));
                assert!(matches!(map.get(&status_key), Some(Value::Int(200))));

                // Check body
                let body_key = MapKey::String(global_string("body".to_string()));
                if let Some(Value::String(s)) = map.get(&body_key) {
                    assert_eq!(s.as_str(), "Hello");
                } else {
                    panic!("Expected body to be String");
                }

                // Check ok
                let ok_key = MapKey::String(global_string("ok".to_string()));
                assert!(matches!(map.get(&ok_key), Some(Value::Bool(true))));

                // Check no error key
                let error_key = MapKey::String(global_string("error".to_string()));
                assert!(map.get(&error_key).is_none());
            }
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_build_response_map_error() {
        let response = build_response_map(404, String::new(), false, Some("Not Found".to_string()));

        match response {
            Value::Map(map_data) => {
                let map = map_data.as_ref();

                // Check status
                let status_key = MapKey::String(global_string("status".to_string()));
                assert!(matches!(map.get(&status_key), Some(Value::Int(404))));

                // Check ok is false
                let ok_key = MapKey::String(global_string("ok".to_string()));
                assert!(matches!(map.get(&ok_key), Some(Value::Bool(false))));

                // Check error message
                let error_key = MapKey::String(global_string("error".to_string()));
                if let Some(Value::String(s)) = map.get(&error_key) {
                    assert_eq!(s.as_str(), "Not Found");
                } else {
                    panic!("Expected error to be String");
                }
            }
            _ => panic!("Expected Map"),
        }
    }

    #[test]
    fn test_error_response() {
        let response = error_response("Connection refused".to_string());

        match response {
            Value::Map(map_data) => {
                let map = map_data.as_ref();

                // Check status is 0
                let status_key = MapKey::String(global_string("status".to_string()));
                assert!(matches!(map.get(&status_key), Some(Value::Int(0))));

                // Check ok is false
                let ok_key = MapKey::String(global_string("ok".to_string()));
                assert!(matches!(map.get(&ok_key), Some(Value::Bool(false))));

                // Check error message
                let error_key = MapKey::String(global_string("error".to_string()));
                if let Some(Value::String(s)) = map.get(&error_key) {
                    assert_eq!(s.as_str(), "Connection refused");
                } else {
                    panic!("Expected error to be String");
                }
            }
            _ => panic!("Expected Map"),
        }
    }

    // SSRF protection tests

    #[test]
    fn test_ssrf_blocks_localhost() {
        assert!(validate_url_for_ssrf("http://localhost/").is_err());
        assert!(validate_url_for_ssrf("http://localhost:8080/").is_err());
        assert!(validate_url_for_ssrf("http://LOCALHOST/").is_err());
        assert!(validate_url_for_ssrf("http://test.localhost/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_loopback_ip() {
        assert!(validate_url_for_ssrf("http://127.0.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://127.0.0.1:8080/").is_err());
        assert!(validate_url_for_ssrf("http://127.1.2.3/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_private_ranges() {
        // 10.0.0.0/8
        assert!(validate_url_for_ssrf("http://10.0.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://10.255.255.255/").is_err());

        // 172.16.0.0/12
        assert!(validate_url_for_ssrf("http://172.16.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://172.31.255.255/").is_err());

        // 192.168.0.0/16
        assert!(validate_url_for_ssrf("http://192.168.0.1/").is_err());
        assert!(validate_url_for_ssrf("http://192.168.255.255/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_link_local() {
        // Cloud metadata endpoint
        assert!(validate_url_for_ssrf("http://169.254.169.254/").is_err());
        assert!(validate_url_for_ssrf("http://169.254.0.1/").is_err());
    }

    #[test]
    fn test_ssrf_blocks_invalid_schemes() {
        assert!(validate_url_for_ssrf("file:///etc/passwd").is_err());
        assert!(validate_url_for_ssrf("ftp://example.com/").is_err());
        assert!(validate_url_for_ssrf("gopher://example.com/").is_err());
    }

    #[test]
    fn test_ssrf_allows_public_urls() {
        // These should be allowed (public IPs)
        assert!(validate_url_for_ssrf("https://example.com/").is_ok());
        assert!(validate_url_for_ssrf("https://httpbin.org/get").is_ok());
        assert!(validate_url_for_ssrf("http://8.8.8.8/").is_ok());
    }

    #[test]
    fn test_dangerous_ipv4() {
        use std::net::Ipv4Addr;

        // Loopback
        assert!(is_dangerous_ipv4(Ipv4Addr::new(127, 0, 0, 1)));
        assert!(is_dangerous_ipv4(Ipv4Addr::new(127, 1, 2, 3)));

        // Private 10.x.x.x
        assert!(is_dangerous_ipv4(Ipv4Addr::new(10, 0, 0, 1)));
        assert!(is_dangerous_ipv4(Ipv4Addr::new(10, 255, 255, 255)));

        // Private 172.16-31.x.x
        assert!(is_dangerous_ipv4(Ipv4Addr::new(172, 16, 0, 1)));
        assert!(is_dangerous_ipv4(Ipv4Addr::new(172, 31, 255, 255)));
        assert!(!is_dangerous_ipv4(Ipv4Addr::new(172, 15, 0, 1))); // Not private
        assert!(!is_dangerous_ipv4(Ipv4Addr::new(172, 32, 0, 1))); // Not private

        // Private 192.168.x.x
        assert!(is_dangerous_ipv4(Ipv4Addr::new(192, 168, 0, 1)));
        assert!(is_dangerous_ipv4(Ipv4Addr::new(192, 168, 255, 255)));

        // Link-local (cloud metadata)
        assert!(is_dangerous_ipv4(Ipv4Addr::new(169, 254, 169, 254)));

        // Public IPs - should NOT be dangerous
        assert!(!is_dangerous_ipv4(Ipv4Addr::new(8, 8, 8, 8)));
        assert!(!is_dangerous_ipv4(Ipv4Addr::new(1, 1, 1, 1)));
        assert!(!is_dangerous_ipv4(Ipv4Addr::new(93, 184, 216, 34)));
    }

    #[test]
    fn test_dangerous_ipv6() {
        use std::net::Ipv6Addr;

        // Loopback
        assert!(is_dangerous_ipv6(Ipv6Addr::LOCALHOST));

        // Link-local fe80::/10
        assert!(is_dangerous_ipv6(Ipv6Addr::new(
            0xfe80, 0, 0, 0, 0, 0, 0, 1
        )));

        // Unique local fc00::/7
        assert!(is_dangerous_ipv6(Ipv6Addr::new(
            0xfc00, 0, 0, 0, 0, 0, 0, 1
        )));
        assert!(is_dangerous_ipv6(Ipv6Addr::new(
            0xfd00, 0, 0, 0, 0, 0, 0, 1
        )));

        // Public - should NOT be dangerous
        assert!(!is_dangerous_ipv6(Ipv6Addr::new(
            0x2001, 0x4860, 0x4860, 0, 0, 0, 0, 0x8888
        ))); // Google DNS
    }
}
