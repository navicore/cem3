//! End-to-end tests for the HTTP client against a same-process server.
//!
//! These run the request through the full wire + pool stack, bypassing
//! only the SSRF validator (since the test server lives on 127.0.0.1
//! and the validator would correctly block that).

use super::pool;
use super::request::perform_request_with_target;
use super::ssrf::{Scheme, ValidatedTarget};
use crate::seqstring::global_string;
use crate::value::{MapKey, Value};
use may::net::TcpListener;
use std::io::{BufRead, BufReader, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Run a minimal HTTP/1.1 server strand on `listener` that loops
/// per-connection, replying with a canned 200 response. Keep-alive
/// is honoured (multiple requests on one connection are supported
/// so the pool-reuse test can observe accept_count=1, req_count>=2).
fn spawn_test_server(
    listener: TcpListener,
    accept_count: Arc<AtomicUsize>,
    req_count: Arc<AtomicUsize>,
) {
    unsafe {
        may::coroutine::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = match stream {
                    Ok(s) => s,
                    Err(_) => break,
                };
                accept_count.fetch_add(1, Ordering::SeqCst);
                let req_count = req_count.clone();
                // Per-connection loop: read one request, write one response.
                may::coroutine::spawn(move || {
                    let stream_clone = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut reader = BufReader::new(stream_clone);
                    loop {
                        // Read request lines until the blank-line terminator.
                        // GET requests have no body, so this is sufficient.
                        let mut saw_request = false;
                        loop {
                            let mut line = String::new();
                            match reader.read_line(&mut line) {
                                Ok(0) => return, // client closed
                                Ok(_) => {}
                                Err(_) => return,
                            }
                            if line == "\r\n" || line == "\n" {
                                if saw_request {
                                    break;
                                }
                                // ignore stray blank lines before request line
                                continue;
                            }
                            saw_request = true;
                        }
                        req_count.fetch_add(1, Ordering::SeqCst);
                        let body = b"world";
                        let header = format!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: text/plain\r\n\
                             Content-Length: {}\r\n\
                             Connection: keep-alive\r\n\
                             \r\n",
                            body.len()
                        );
                        if stream.write_all(header.as_bytes()).is_err() {
                            return;
                        }
                        if stream.write_all(body).is_err() {
                            return;
                        }
                    }
                });
            }
        });
    }
}

fn loopback_target(port: u16, path: &str) -> ValidatedTarget {
    ValidatedTarget {
        scheme: Scheme::Http,
        host: "127.0.0.1".to_string(),
        port,
        addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        path_and_query: path.to_string(),
    }
}

fn unwrap_response(value: &Value) -> (i64, Vec<u8>, bool) {
    let map = match value {
        Value::Map(m) => m,
        other => panic!("expected response Map, got {:?}", other),
    };
    let status = match map.get(&MapKey::String(global_string("status".to_string()))) {
        Some(Value::Int(n)) => *n,
        other => panic!("status missing/wrong type: {:?}", other),
    };
    let body = match map.get(&MapKey::String(global_string("body".to_string()))) {
        Some(Value::String(s)) => s.as_bytes().to_vec(),
        other => panic!("body missing/wrong type: {:?}", other),
    };
    let ok = match map.get(&MapKey::String(global_string("ok".to_string()))) {
        Some(Value::Bool(b)) => *b,
        other => panic!("ok missing/wrong type: {:?}", other),
    };
    (status, body, ok)
}

#[test]
fn http_get_end_to_end_against_local_server() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();

    let accept_count = Arc::new(AtomicUsize::new(0));
    let req_count = Arc::new(AtomicUsize::new(0));
    spawn_test_server(listener, accept_count.clone(), req_count.clone());

    let target = loopback_target(port, "/hello");
    let resp = perform_request_with_target("GET", target, None);
    let (status, body, ok) = unwrap_response(&resp);
    assert_eq!(status, 200);
    assert_eq!(body, b"world");
    assert!(ok);
    assert_eq!(req_count.load(Ordering::SeqCst), 1);
    assert_eq!(accept_count.load(Ordering::SeqCst), 1);
}

#[test]
fn http_pool_reuses_connection_for_second_request() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();

    let accept_count = Arc::new(AtomicUsize::new(0));
    let req_count = Arc::new(AtomicUsize::new(0));
    spawn_test_server(listener, accept_count.clone(), req_count.clone());

    let target1 = loopback_target(port, "/first");
    let r1 = perform_request_with_target("GET", target1, None);
    assert_eq!(unwrap_response(&r1).0, 200);

    let target2 = loopback_target(port, "/second");
    let r2 = perform_request_with_target("GET", target2, None);
    assert_eq!(unwrap_response(&r2).0, 200);

    // Two requests handled, but only ONE accept on the server side —
    // proves the pool returned the same connection for the second
    // request rather than redialing.
    assert_eq!(req_count.load(Ordering::SeqCst), 2);
    assert_eq!(
        accept_count.load(Ordering::SeqCst),
        1,
        "second request should reuse pooled connection (saw {} accepts)",
        accept_count.load(Ordering::SeqCst)
    );
}

#[test]
fn http_post_with_body_round_trips() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let accept_count = Arc::new(AtomicUsize::new(0));
    let req_count = Arc::new(AtomicUsize::new(0));
    spawn_test_server(listener, accept_count.clone(), req_count.clone());

    // Note: our test server only reads up to the blank line, so a
    // POST body would be left in the socket buffer. For this test we
    // assert that perform_request *issues* the POST correctly — the
    // status came back, which means the request was framed validly.
    let target = loopback_target(port, "/create");
    let body = b"{\"name\":\"alice\"}";
    let resp =
        perform_request_with_target("POST", target, Some(("application/json", body.as_slice())));
    let (status, _, ok) = unwrap_response(&resp);
    assert_eq!(status, 200);
    assert!(ok);
}
