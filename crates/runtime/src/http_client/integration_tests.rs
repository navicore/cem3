//! End-to-end tests for the HTTP client against a same-process server.
//!
//! These run the request through the full wire + pool stack, bypassing
//! only the SSRF validator (since the test server lives on 127.0.0.1
//! and the validator would correctly block that).

use super::pool;
use super::request::{perform_request, perform_request_with_target};
use super::ssrf::{Scheme, ValidatedTarget};
use crate::seqstring::global_string;
use crate::value::{MapKey, Value};
use may::net::TcpListener;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Server response shape selector. Each integration test picks the
/// flavour that exercises the wire path it cares about.
#[derive(Clone, Copy)]
enum ServerMode {
    /// Read request headers only, ignore any body, reply `world` with
    /// `Content-Length: 5` and `Connection: keep-alive`.
    FixedBody,
    /// Read Content-Length bytes after the request blank line, echo
    /// them back in the response body. Used by the POST round-trip
    /// test to verify the body bytes actually reach the wire.
    Echo,
    /// Reply with `Transfer-Encoding: chunked`, two data chunks plus
    /// the terminator. Used by the chunked-decode integration test.
    Chunked,
    /// Reply with `Connection: close` + Content-Length, then close
    /// the socket from the server side. Used to verify the pool
    /// drops close-marked entries instead of trying to reuse them.
    CloseOnce,
}

/// Run a minimal HTTP/1.1 server strand on `listener` that loops
/// per-connection. Keep-alive is honoured for multi-request tests.
fn spawn_test_server(
    listener: TcpListener,
    accept_count: Arc<AtomicUsize>,
    req_count: Arc<AtomicUsize>,
    mode: ServerMode,
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
                may::coroutine::spawn(move || {
                    let stream_clone = match stream.try_clone() {
                        Ok(s) => s,
                        Err(_) => return,
                    };
                    let mut reader = BufReader::new(stream_clone);
                    loop {
                        // Read request lines until the blank-line terminator.
                        // Capture Content-Length so we can read the body
                        // for the echo mode.
                        let mut saw_request = false;
                        let mut content_length: usize = 0;
                        loop {
                            let mut line = String::new();
                            match reader.read_line(&mut line) {
                                Ok(0) => return,
                                Ok(_) => {}
                                Err(_) => return,
                            }
                            if line == "\r\n" || line == "\n" {
                                if saw_request {
                                    break;
                                }
                                continue;
                            }
                            saw_request = true;
                            if let Some(rest) = line.strip_prefix("Content-Length:") {
                                content_length = rest.trim().parse::<usize>().unwrap_or(0);
                            } else if let Some(rest) = line.strip_prefix("content-length:") {
                                content_length = rest.trim().parse::<usize>().unwrap_or(0);
                            }
                        }
                        req_count.fetch_add(1, Ordering::SeqCst);

                        let body_in: Vec<u8> =
                            if content_length > 0 && matches!(mode, ServerMode::Echo) {
                                let mut buf = vec![0u8; content_length];
                                if reader.read_exact(&mut buf).is_err() {
                                    return;
                                }
                                buf
                            } else {
                                Vec::new()
                            };

                        let write_ok = match mode {
                            ServerMode::FixedBody => {
                                let body = b"world";
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\n\
                                     Content-Type: text/plain\r\n\
                                     Content-Length: {}\r\n\
                                     Connection: keep-alive\r\n\
                                     \r\n",
                                    body.len()
                                );
                                stream.write_all(header.as_bytes()).is_ok()
                                    && stream.write_all(body).is_ok()
                            }
                            ServerMode::Echo => {
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\n\
                                     Content-Type: application/octet-stream\r\n\
                                     Content-Length: {}\r\n\
                                     Connection: keep-alive\r\n\
                                     \r\n",
                                    body_in.len()
                                );
                                stream.write_all(header.as_bytes()).is_ok()
                                    && stream.write_all(&body_in).is_ok()
                            }
                            ServerMode::Chunked => {
                                // Two chunks ("Hello" + " world") followed by
                                // the 0-terminator. Exercises chunk-size hex
                                // parsing, multi-chunk accumulation, and the
                                // trailer-section drain on the client side.
                                let payload = b"HTTP/1.1 200 OK\r\n\
                                     Content-Type: text/plain\r\n\
                                     Transfer-Encoding: chunked\r\n\
                                     Connection: keep-alive\r\n\
                                     \r\n\
                                     5\r\nHello\r\n\
                                     6\r\n world\r\n\
                                     0\r\n\r\n";
                                stream.write_all(payload).is_ok()
                            }
                            ServerMode::CloseOnce => {
                                // Reply with `Connection: close` + a fixed
                                // body, then return from this per-connection
                                // strand. Returning drops `stream`, which
                                // sends FIN to the peer — the client's
                                // pool::release call sees keep_alive=false
                                // and drops the entry. The next request to
                                // this listener must trigger a fresh accept.
                                let body = b"closed";
                                let header = format!(
                                    "HTTP/1.1 200 OK\r\n\
                                     Content-Type: text/plain\r\n\
                                     Content-Length: {}\r\n\
                                     Connection: close\r\n\
                                     \r\n",
                                    body.len()
                                );
                                let _ = stream.write_all(header.as_bytes());
                                let _ = stream.write_all(body);
                                return;
                            }
                        };
                        if !write_ok {
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
    spawn_test_server(
        listener,
        accept_count.clone(),
        req_count.clone(),
        ServerMode::FixedBody,
    );

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
    spawn_test_server(
        listener,
        accept_count.clone(),
        req_count.clone(),
        ServerMode::FixedBody,
    );

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
    // Echo mode reads Content-Length bytes after the request blank
    // line and echoes them back. That makes this test actually prove
    // the body bytes traversed the wire — a regression in
    // `wire::write_request` that silently dropped the body would
    // surface here as a mismatch instead of a misleading 200 OK.
    spawn_test_server(
        listener,
        accept_count.clone(),
        req_count.clone(),
        ServerMode::Echo,
    );

    let target = loopback_target(port, "/create");
    let body = b"{\"name\":\"alice\"}";
    let resp =
        perform_request_with_target("POST", target, Some(("application/json", body.as_slice())));
    let (status, echoed, ok) = unwrap_response(&resp);
    assert_eq!(status, 200);
    assert!(ok);
    assert_eq!(echoed, body, "POST body must round-trip byte-for-byte");
}

#[test]
fn http_chunked_response_decodes_end_to_end() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let accept_count = Arc::new(AtomicUsize::new(0));
    let req_count = Arc::new(AtomicUsize::new(0));
    spawn_test_server(
        listener,
        accept_count.clone(),
        req_count.clone(),
        ServerMode::Chunked,
    );

    let target = loopback_target(port, "/chunked");
    let resp = perform_request_with_target("GET", target, None);
    let (status, body, ok) = unwrap_response(&resp);
    assert_eq!(status, 200);
    assert!(ok);
    // Server emits two chunks: "Hello" (5) + " world" (6). The client
    // must concatenate them in order.
    assert_eq!(body, b"Hello world");
    assert_eq!(req_count.load(Ordering::SeqCst), 1);
}

#[test]
fn http_connection_close_response_evicts_from_pool() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    let accept_count = Arc::new(AtomicUsize::new(0));
    let req_count = Arc::new(AtomicUsize::new(0));
    spawn_test_server(
        listener,
        accept_count.clone(),
        req_count.clone(),
        ServerMode::CloseOnce,
    );

    let key = pool::PoolKey {
        scheme: Scheme::Http,
        host: "127.0.0.1".to_string(),
        port,
    };

    let target1 = loopback_target(port, "/first");
    let r1 = perform_request_with_target("GET", target1, None);
    assert_eq!(unwrap_response(&r1).0, 200);

    // The critical assertion: after r1's release, the
    // `Connection: close` entry must NOT be in the pool. This pins
    // the release-path behaviour directly. The accept-count check
    // below catches a similar shape but can be rescued by
    // `is_reusable`'s POLLIN/FIN detection on localhost (FIN
    // propagation is sub-microsecond), so a regression where
    // `release` ignores `keep_alive=false` would silently slip past
    // it. Asserting on pool state closes that gap.
    assert_eq!(
        pool::idle_count_for_test(&key),
        0,
        "Connection: close response must not survive pool::release. \
         An entry here means release ignored keep_alive=false."
    );

    let target2 = loopback_target(port, "/second");
    let r2 = perform_request_with_target("GET", target2, None);
    assert_eq!(unwrap_response(&r2).0, 200);

    // The second request must dial fresh — accept_count == 2 is the
    // observable consequence of the pool-drop above. (On localhost
    // this also catches compound regressions where both release and
    // is_reusable misbehave; the idle_count_for_test assertion above
    // is the primary anchor for the targeted property.)
    assert_eq!(req_count.load(Ordering::SeqCst), 2);
    assert_eq!(
        accept_count.load(Ordering::SeqCst),
        2,
        "Connection: close response must NOT be pooled: each request \
         should trigger a fresh accept on the server."
    );
}

// -----------------------------------------------------------------------------
// SSRF DNS-rebinding closure: the architectural property that perform_request
// calls dns::resolve at most once per request, and that the validated address
// list flows from SSRF straight into connect — not re-resolved downstream.
// A regression that passes `host: String` to connect (and re-resolves) would
// fail this loudly.
// -----------------------------------------------------------------------------

#[test]
#[serial_test::serial(dns_global_state)]
fn ssrf_dns_rebinding_closure_holds_at_most_one_resolve_per_request() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();
    crate::dns::clear_scripted_responses();
    crate::dns::reset_resolve_call_count();

    // Scripted DNS response. We push a single address that SSRF
    // *accepts* — 224.0.0.1 is an IPv4 multicast address (224.0.0.0/4
    // per RFC 5771) and isn't in any of the SSRF-blocked ranges, so
    // validate_url passes it through to connect. The TCP connect
    // then fails fast with `ENETUNREACH` (Linux kernel rejects
    // TCP-to-multicast in microseconds, not seconds) which keeps
    // this test cheap. The exact failure mode is irrelevant; this
    // test cares about call count, not connection outcome.
    //
    // Avoid TEST-NET-1 (192.0.2.0/24) here — that one takes the full
    // OS SYN timeout (~60s) on boxes that route it through a default
    // gateway, which would make the test prohibitively slow.
    crate::dns::push_scripted_response(vec!["224.0.0.1".to_string()]);

    // We don't care about the response body or status. Issue the
    // request and let it fail at the connect stage.
    let _resp = perform_request("GET", "http://example.com/", None);

    let count = crate::dns::resolve_call_count();
    assert_eq!(
        count, 1,
        "perform_request must call dns::resolve at most once per \
         request. count={count} > 1 means the connect or pool path \
         is re-resolving the hostname (DNS-rebinding regression): \
         the SSRF-validated address list should flow into connect, \
         not the original hostname string."
    );
}

// -----------------------------------------------------------------------------
// Happy-path TLS: end-to-end HTTPS request against a same-process rustls
// server. The cert is self-signed at test time (via rcgen) and serves as its
// own trust anchor; the runtime's TLS config is overridden for the duration
// of the test to trust it. Pins:
//
//   - net.tls.client handshake against a real rustls server completes.
//   - The TLS-wrapped Socket round-trips an HTTP/1.1 request and response.
//   - StreamOwned<ClientConnection, may::net::TcpStream>'s Read/Write impls
//     yield cooperatively through the may stream (test runs without
//     deadlocking the carrier).
// -----------------------------------------------------------------------------

fn build_tls_test_pair() -> (Arc<rustls::ServerConfig>, Arc<rustls::ClientConfig>) {
    // Self-signed cert with SAN=DNS:localhost. The CN matters because
    // the client SNI is "localhost" (see the target below); rustls
    // validates that one SAN entry covers the SNI.
    let rcgen::CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
            .expect("generate self-signed cert");

    let cert_der: rustls::pki_types::CertificateDer<'static> = cert.der().clone();
    let key_der: rustls::pki_types::PrivateKeyDer<'static> =
        rustls::pki_types::PrivatePkcs8KeyDer::from(key_pair.serialize_der()).into();

    let server_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key_der)
        .expect("server config");

    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der).expect("add test cert as trust root");
    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    (Arc::new(server_config), Arc::new(client_config))
}

/// Spawn a TLS-speaking server on its own OS thread.
///
/// Deliberately uses `std::thread` + `std::net::TcpListener` rather
/// than `may::coroutine::spawn` + `may::net::TcpListener`. The TLS
/// handshake is a multi-round-trip dance: each round requires the
/// server side to be scheduled, read the next message, and write a
/// response. When `cargo test` runs the entire runtime suite in
/// parallel, the may worker pool ends up saturated with coroutines
/// from other tests and the handshake stalls. Putting the server on
/// a dedicated OS thread sidesteps may's scheduler entirely — the
/// kernel keeps the thread runnable as long as there's a CPU.
fn spawn_tls_test_server(
    listener: std::net::TcpListener,
    server_config: Arc<rustls::ServerConfig>,
) {
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => return,
            };
            let server_config = server_config.clone();
            std::thread::spawn(move || {
                let conn = match rustls::ServerConnection::new(server_config) {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let mut tls = rustls::StreamOwned::new(conn, stream);

                // Read request headers up to (and including) the
                // blank-line terminator. The TLS handshake happens
                // transparently on the first read; rustls drives
                // it via complete_io internally.
                let mut reader = BufReader::new(&mut tls);
                let mut saw_request = false;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) | Err(_) => return,
                        _ => {}
                    }
                    if line == "\r\n" || line == "\n" {
                        if saw_request {
                            break;
                        }
                        continue;
                    }
                    saw_request = true;
                }
                drop(reader); // releases the &mut tls borrow

                let body = b"hello-over-tls";
                let header = format!(
                    "HTTP/1.1 200 OK\r\n\
                     Content-Type: text/plain\r\n\
                     Content-Length: {}\r\n\
                     Connection: close\r\n\
                     \r\n",
                    body.len()
                );
                let _ = tls.write_all(header.as_bytes());
                let _ = tls.write_all(body);
                let _ = tls.flush();
            });
        }
    });
}

/// RAII guard that clears the process-wide test TLS config on drop.
///
/// The HTTPS test installs a `ClientConfig` whose trust roots only
/// include the test's self-signed CA. If a panic between `install`
/// and the explicit `clear` leaks the override, other tests that
/// later use TLS would fail validation against real servers.
/// Dropping the guard unconditionally clears the override even on
/// the panic path.
struct TestTlsConfigGuard;
impl Drop for TestTlsConfigGuard {
    fn drop(&mut self) {
        crate::tls::clear_test_tls_config();
    }
}

#[test]
// Serialised against any future test that touches `TEST_TLS_CONFIG`.
// No other test currently does — but once a second one lands, this
// tag prevents the two from clobbering each other's overrides.
#[serial_test::serial(tls_global_state)]
fn https_round_trip_against_same_process_rustls_server() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    let (server_config, client_config) = build_tls_test_pair();
    crate::tls::install_test_tls_config(client_config);
    // The guard fires on every exit from this function, including
    // panic. Must be created AFTER install (so a build_tls_test_pair
    // failure isn't preceded by a no-op clear) and BEFORE any
    // panicking code below.
    let _tls_guard = TestTlsConfigGuard;

    // std::net::TcpListener here — the server runs on its own OS
    // thread to avoid may-scheduler congestion (see
    // spawn_tls_test_server's doc).
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    spawn_tls_test_server(listener, server_config);

    // Build the target by hand so we can drive HTTPS against a
    // loopback address (SSRF would otherwise block 127.0.0.1).
    // host=localhost matches the cert's SAN; addrs=127.0.0.1 is the
    // actual dial target. perform_validated takes the addrs verbatim
    // and the host string flows into the TLS handshake as SNI.
    let target = ValidatedTarget {
        scheme: Scheme::Https,
        host: "localhost".to_string(),
        port,
        addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        path_and_query: "/hello".to_string(),
    };
    let resp = perform_request_with_target("GET", target, None);
    let (status, body, ok) = unwrap_response(&resp);

    assert_eq!(status, 200, "HTTPS round-trip status");
    assert!(ok, "HTTPS round-trip ok flag");
    assert_eq!(body, b"hello-over-tls", "HTTPS round-trip body");

    // Defensive — the server emits Connection: close so nothing
    // should be pooled, but clear anyway so a future failure mode
    // doesn't leak a TLS-wrapped Socket into another test's pool.
    pool::clear_for_test();
    // _tls_guard drops here, restoring the production TLS config.
}

// -----------------------------------------------------------------------------
// Per-request HTTP timeout: a peer that accepts TCP but never sends a response
// must surface as a connection error within the configured deadline, not
// stall the carrier indefinitely. Pins the per-IO timeout plumbed through
// run_once. Uses the same std::thread/std::net pattern as the TLS test to
// avoid may-scheduler congestion under the full test suite.
// -----------------------------------------------------------------------------

/// RAII guard restoring the HTTP timeout override on drop. Mirrors
/// the TLS guard so a panic between override-install and the test's
/// happy-path cleanup can't leak short deadlines into other tests.
struct HttpRequestTimeoutGuard;
impl Drop for HttpRequestTimeoutGuard {
    fn drop(&mut self) {
        super::request::set_test_http_request_timeout(None);
    }
}

#[test]
fn http_request_timeout_fires_on_silent_server() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    // Drop deadline to 200ms so the test finishes quickly.
    super::request::set_test_http_request_timeout(Some(std::time::Duration::from_millis(200)));
    let _timeout_guard = HttpRequestTimeoutGuard;

    // Silent server: accept one TCP connection, then sit on it.
    // Never write a response. Uses std::net + std::thread so the
    // server doesn't fight the may scheduler.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            // Hold the connection open until the test process exits.
            // The client side bails on its per-read timeout long
            // before this sleep ends.
            std::thread::sleep(std::time::Duration::from_secs(60));
            drop(stream);
        }
    });

    let target = loopback_target(port, "/silent");
    let start = std::time::Instant::now();
    let resp = perform_request_with_target("GET", target, None);
    let elapsed = start.elapsed();

    let (status, _body, ok) = unwrap_response(&resp);

    // The request must fail (status=0, ok=false) — the server never
    // wrote a response — and must do so within a small multiple of
    // the configured deadline. 2s is generous (we set 200ms); a
    // regression that ignored the timeout would block until the
    // server's 60s sleep ends.
    assert_eq!(status, 0, "silent-server request must surface as error");
    assert!(!ok, "ok flag must be false");
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "HTTP request must respect the per-IO timeout (elapsed={elapsed:?}). \
         Above 2s suggests the deadline isn't being plumbed through to \
         wire::read_response."
    );
}

// -----------------------------------------------------------------------------
// TLS handshake timeout: a peer that accepts plain TCP but never speaks TLS
// must surface as a handshake failure within the configured deadline. Pins
// the per-IO timeout that build_tls sets on the TcpStream before complete_io.
// -----------------------------------------------------------------------------

struct TlsHandshakeTimeoutGuard;
impl Drop for TlsHandshakeTimeoutGuard {
    fn drop(&mut self) {
        crate::tls::set_test_tls_handshake_timeout(None);
    }
}

#[test]
#[serial_test::serial(tls_global_state)]
fn tls_handshake_timeout_fires_on_silent_peer() {
    unsafe { crate::scheduler::scheduler_init() };
    pool::clear_for_test();

    // Need the test TLS config installed so build_tls doesn't fail
    // earlier (validation against webpki-roots), but the handshake
    // never actually progresses past the first read — the peer
    // doesn't speak TLS. The cert content doesn't matter here.
    let (_server_config, client_config) = build_tls_test_pair();
    crate::tls::install_test_tls_config(client_config);
    let _tls_guard = TestTlsConfigGuard;

    crate::tls::set_test_tls_handshake_timeout(Some(std::time::Duration::from_millis(200)));
    let _handshake_guard = TlsHandshakeTimeoutGuard;

    // Plain-TCP server: accept, never write. The client's
    // complete_io will read for the server's ServerHello, the
    // per-read timeout fires, build_tls returns Err(()).
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            std::thread::sleep(std::time::Duration::from_secs(60));
            drop(stream);
        }
    });

    let target = ValidatedTarget {
        scheme: Scheme::Https,
        host: "localhost".to_string(),
        port,
        addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        path_and_query: "/".to_string(),
    };
    let start = std::time::Instant::now();
    let resp = perform_request_with_target("GET", target, None);
    let elapsed = start.elapsed();

    let (status, _body, ok) = unwrap_response(&resp);

    assert_eq!(status, 0, "silent-peer TLS upgrade must surface as error");
    assert!(!ok);
    assert!(
        elapsed < std::time::Duration::from_secs(2),
        "TLS handshake must respect the per-IO timeout (elapsed={elapsed:?}). \
         Above 2s suggests build_tls isn't setting read/write timeouts on \
         the TcpStream before complete_io."
    );

    pool::clear_for_test();
}
