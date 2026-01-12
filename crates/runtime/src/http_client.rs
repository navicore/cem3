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

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::{MapKey, Value};

use std::collections::HashMap;
use std::time::Duration;

/// Default timeout for HTTP requests (30 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Maximum response body size (10 MB)
const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

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

/// Internal: Perform GET request
fn perform_get(url: &str) -> Value {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build();

    match agent.get(url).call() {
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

/// Internal: Perform POST request
fn perform_post(url: &str, body: &str, content_type: &str) -> Value {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build();

    match agent
        .post(url)
        .set("Content-Type", content_type)
        .send_string(body)
    {
        Ok(response) => {
            let status = response.status() as i64;
            let ok = (200..300).contains(&response.status());

            match response.into_string() {
                Ok(resp_body) => {
                    if resp_body.len() > MAX_BODY_SIZE {
                        error_response(format!(
                            "Response body too large ({} bytes, max {})",
                            resp_body.len(),
                            MAX_BODY_SIZE
                        ))
                    } else {
                        build_response_map(status, resp_body, ok, None)
                    }
                }
                Err(e) => error_response(format!("Failed to read response body: {}", e)),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let resp_body = response.into_string().unwrap_or_default();
            build_response_map(
                code as i64,
                resp_body,
                false,
                Some(format!("HTTP error: {}", code)),
            )
        }
        Err(ureq::Error::Transport(e)) => error_response(format!("Connection error: {}", e)),
    }
}

/// Internal: Perform PUT request
fn perform_put(url: &str, body: &str, content_type: &str) -> Value {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build();

    match agent
        .put(url)
        .set("Content-Type", content_type)
        .send_string(body)
    {
        Ok(response) => {
            let status = response.status() as i64;
            let ok = (200..300).contains(&response.status());

            match response.into_string() {
                Ok(resp_body) => {
                    if resp_body.len() > MAX_BODY_SIZE {
                        error_response(format!(
                            "Response body too large ({} bytes, max {})",
                            resp_body.len(),
                            MAX_BODY_SIZE
                        ))
                    } else {
                        build_response_map(status, resp_body, ok, None)
                    }
                }
                Err(e) => error_response(format!("Failed to read response body: {}", e)),
            }
        }
        Err(ureq::Error::Status(code, response)) => {
            let resp_body = response.into_string().unwrap_or_default();
            build_response_map(
                code as i64,
                resp_body,
                false,
                Some(format!("HTTP error: {}", code)),
            )
        }
        Err(ureq::Error::Transport(e)) => error_response(format!("Connection error: {}", e)),
    }
}

/// Internal: Perform DELETE request
fn perform_delete(url: &str) -> Value {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build();

    match agent.delete(url).call() {
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
            let body = response.into_string().unwrap_or_default();
            build_response_map(
                code as i64,
                body,
                false,
                Some(format!("HTTP error: {}", code)),
            )
        }
        Err(ureq::Error::Transport(e)) => error_response(format!("Connection error: {}", e)),
    }
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
}
