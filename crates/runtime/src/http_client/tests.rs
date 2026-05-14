use super::ssrf::{Scheme, ValidatedTarget};
use super::wire::{read_response, write_request};
use super::*;
use std::io::Cursor;
use std::net::IpAddr;

// Unit tests focus on pure layers: response-map shape, SSRF
// classification, and wire framing. Pool behaviour and end-to-end
// request flow are covered by the integration tests.

fn dummy_target(host: &str, port: u16, scheme: Scheme, path_and_query: &str) -> ValidatedTarget {
    ValidatedTarget {
        scheme,
        host: host.to_string(),
        port,
        addrs: vec![IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))],
        path_and_query: path_and_query.to_string(),
    }
}

#[test]
fn test_build_response_map_success() {
    let response = build_response_map(200, b"Hello".to_vec(), true, None);

    match response {
        Value::Map(map_data) => {
            let map = map_data.as_ref();

            // Check status
            let status_key = MapKey::String(global_string("status".to_string()));
            assert!(matches!(map.get(&status_key), Some(Value::Int(200))));

            // Check body
            let body_key = MapKey::String(global_string("body".to_string()));
            if let Some(Value::String(s)) = map.get(&body_key) {
                assert_eq!(s.as_str_or_empty(), "Hello");
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
    let response = build_response_map(404, Vec::new(), false, Some("Not Found".to_string()));

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
                assert_eq!(s.as_str_or_empty(), "Not Found");
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
                assert_eq!(s.as_str_or_empty(), "Connection refused");
            } else {
                panic!("Expected error to be String");
            }
        }
        _ => panic!("Expected Map"),
    }
}

// Byte-cleanliness: HTTP response bodies are arbitrary octets per
// RFC 7230. The response map's "body" field must round-trip non-UTF-8
// bytes intact so binary downloads (images, Protobuf, MessagePack)
// reach Seq programs unmodified.

const HTTP_BIN: &[u8] = &[0x00, 0xDC, b'x', 0xFF, 0xC3, b'!', 0x80];

#[test]
fn byte_clean_response_body_round_trips_binary() {
    let response = build_response_map(200, HTTP_BIN.to_vec(), true, None);
    let map = match response {
        Value::Map(m) => m,
        _ => panic!("expected Map"),
    };
    let body_key = MapKey::String(global_string("body".to_string()));
    match map.get(&body_key) {
        Some(Value::String(s)) => assert_eq!(s.as_bytes(), HTTP_BIN),
        other => panic!("expected body String, got {:?}", other),
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

// Serialised with the SSRF-rebinding test in integration_tests.rs:
// both call into dns::resolve (which mutates the test-only counter
// + scripted-response queue), so they can't run concurrently.
#[test]
#[serial_test::serial(dns_global_state)]
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

// -----------------------------------------------------------------------------
// Wire-format tests
// -----------------------------------------------------------------------------

fn assert_request_contains(out: &[u8], needles: &[&str]) {
    let s = std::str::from_utf8(out).expect("request must be ASCII/UTF-8 here");
    for needle in needles {
        assert!(
            s.contains(needle),
            "expected request to contain {needle:?}, got:\n{s}"
        );
    }
}

#[test]
fn wire_get_request_format() {
    let target = dummy_target("example.com", 80, Scheme::Http, "/path?q=1");
    let mut buf = Vec::new();
    write_request(&mut buf, "GET", &target, None).unwrap();
    assert_request_contains(
        &buf,
        &[
            "GET /path?q=1 HTTP/1.1\r\n",
            "Host: example.com\r\n",
            "Accept-Encoding: identity\r\n",
            "Connection: keep-alive\r\n",
        ],
    );
    // Empty body — terminator is the only blank line.
    assert!(buf.ends_with(b"\r\n\r\n"));
}

#[test]
fn wire_get_https_default_port_omits_port_from_host() {
    let target = dummy_target("example.com", 443, Scheme::Https, "/");
    let mut buf = Vec::new();
    write_request(&mut buf, "GET", &target, None).unwrap();
    let s = std::str::from_utf8(&buf).unwrap();
    assert!(
        s.contains("Host: example.com\r\n"),
        "default https port (443) must not appear in Host: {s}"
    );
}

#[test]
fn wire_get_nonstandard_port_includes_port_in_host() {
    let target = dummy_target("example.com", 8080, Scheme::Http, "/");
    let mut buf = Vec::new();
    write_request(&mut buf, "GET", &target, None).unwrap();
    assert_request_contains(&buf, &["Host: example.com:8080\r\n"]);
}

#[test]
fn wire_post_request_includes_body_and_framing_headers() {
    let target = dummy_target("api.example.com", 443, Scheme::Https, "/users");
    let body = b"{\"name\":\"Alice\"}";
    let mut buf = Vec::new();
    write_request(&mut buf, "POST", &target, Some(("application/json", body))).unwrap();
    assert_request_contains(
        &buf,
        &[
            "POST /users HTTP/1.1\r\n",
            "Host: api.example.com\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 16\r\n",
        ],
    );
    assert!(
        buf.windows(body.len()).any(|w| w == body),
        "request must include the literal body bytes"
    );
}

#[test]
fn wire_response_content_length() {
    let raw = b"HTTP/1.1 200 OK\r\n\
                Content-Length: 5\r\n\
                Content-Type: text/plain\r\n\
                \r\n\
                hello";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("parse");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"hello");
    assert!(resp.keep_alive);
}

#[test]
fn wire_response_chunked() {
    // Two chunks: "Wiki" (4 bytes) and "pedia" (5 bytes), then 0-chunk terminator.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Transfer-Encoding: chunked\r\n\
                \r\n\
                4\r\nWiki\r\n\
                5\r\npedia\r\n\
                0\r\n\r\n";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("parse");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"Wikipedia");
    assert!(resp.keep_alive);
}

#[test]
fn wire_response_chunked_with_extensions_and_trailer() {
    let raw = b"HTTP/1.1 200 OK\r\n\
                Transfer-Encoding: chunked\r\n\
                \r\n\
                4;name=value\r\ndata\r\n\
                0\r\n\
                X-Trailer: ignored\r\n\
                \r\n";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("parse");
    assert_eq!(resp.body, b"data");
}

#[test]
fn wire_response_connection_close_eof_framed() {
    // No Content-Length, no Transfer-Encoding, Connection: close.
    // Body terminates at EOF; connection must not be pool-eligible.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Connection: close\r\n\
                \r\n\
                streamed body bytes";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("parse");
    assert_eq!(resp.body, b"streamed body bytes");
    assert!(!resp.keep_alive);
}

#[test]
fn wire_response_404_status() {
    let raw = b"HTTP/1.1 404 Not Found\r\n\
                Content-Length: 9\r\n\
                \r\n\
                not found";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("parse");
    assert_eq!(resp.status, 404);
    assert_eq!(resp.body, b"not found");
}

#[test]
fn wire_response_content_length_too_large_rejected() {
    let huge = super::wire::MAX_BODY_SIZE + 1;
    let raw = format!("HTTP/1.1 200 OK\r\nContent-Length: {huge}\r\n\r\n");
    let mut cursor = Cursor::new(raw.into_bytes());
    let err = read_response(&mut cursor).expect_err("oversized body must be rejected");
    assert!(err.contains("too large"), "got: {err}");
}

#[test]
fn wire_response_invalid_status_line_rejected() {
    let raw = b"NOT-AN-HTTP-RESPONSE\r\n\r\n";
    let mut cursor = Cursor::new(raw.to_vec());
    assert!(read_response(&mut cursor).is_err());
}

// -----------------------------------------------------------------------------
// Anti-smuggling: header-injection and conflicting framing
// -----------------------------------------------------------------------------

#[test]
fn wire_content_type_with_crlf_rejected() {
    // Classic header-injection payload: a CRLF in the Content-Type
    // value lets the caller append arbitrary headers and (with the
    // right Content-Length manipulation) smuggle a follow-up request
    // through an intermediary. write_request must reject before the
    // bytes hit the wire.
    let target = dummy_target("example.com", 443, Scheme::Https, "/");
    let bad_ct = "text/plain\r\nX-Injected: pwn";
    let mut buf = Vec::new();
    let result = write_request(&mut buf, "POST", &target, Some((bad_ct, b"body")));
    assert!(result.is_err(), "CRLF in Content-Type must be rejected");
    assert!(
        buf.is_empty()
            || !std::str::from_utf8(&buf)
                .unwrap_or("")
                .contains("X-Injected"),
        "rejected request must not have written the injected header"
    );
}

#[test]
fn wire_content_type_with_lf_only_rejected() {
    let target = dummy_target("example.com", 443, Scheme::Https, "/");
    let bad_ct = "text/plain\nX-Sneaky: 1";
    let mut buf = Vec::new();
    assert!(write_request(&mut buf, "POST", &target, Some((bad_ct, b"body"))).is_err());
}

#[test]
fn wire_content_type_with_null_byte_rejected() {
    let target = dummy_target("example.com", 443, Scheme::Https, "/");
    let bad_ct = "text/plain\0";
    let mut buf = Vec::new();
    assert!(write_request(&mut buf, "POST", &target, Some((bad_ct, b"body"))).is_err());
}

#[test]
fn wire_response_conflicting_content_length_rejected() {
    // Two Content-Length headers with different values — classic
    // request-smuggling shape. The server side of this is "malicious
    // server" but we still must not pick one and proceed.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Content-Length: 5\r\n\
                Content-Length: 7\r\n\
                \r\n\
                hello";
    let mut cursor = Cursor::new(raw.to_vec());
    let err = read_response(&mut cursor).expect_err("must reject conflicting CL");
    assert!(err.contains("smuggling"), "got: {err}");
}

#[test]
fn wire_response_conflicting_content_length_list_rejected() {
    // Single Content-Length header whose value is a comma-separated
    // list with disagreeing entries — same RFC 9112 §6.3 rule.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Content-Length: 5, 7\r\n\
                \r\n\
                hello";
    let mut cursor = Cursor::new(raw.to_vec());
    let err = read_response(&mut cursor).expect_err("must reject conflicting CL list");
    assert!(err.contains("smuggling"), "got: {err}");
}

#[test]
fn wire_response_content_length_list_all_agreeing_accepted() {
    // Same value repeated — RFC allows this; treat as the single
    // value.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Content-Length: 5, 5\r\n\
                \r\n\
                hello";
    let mut cursor = Cursor::new(raw.to_vec());
    let resp = read_response(&mut cursor).expect("agreeing CL list must parse");
    assert_eq!(resp.body, b"hello");
}

#[test]
fn wire_response_chunked_with_content_length_rejected() {
    // Both framings simultaneously — the canonical request-smuggling
    // setup. Reject; don't quietly prefer one.
    let raw = b"HTTP/1.1 200 OK\r\n\
                Transfer-Encoding: chunked\r\n\
                Content-Length: 5\r\n\
                \r\n\
                4\r\nWiki\r\n0\r\n\r\n";
    let mut cursor = Cursor::new(raw.to_vec());
    let err =
        read_response(&mut cursor).expect_err("must reject chunked + Content-Length combination");
    assert!(err.contains("smuggling"), "got: {err}");
}
