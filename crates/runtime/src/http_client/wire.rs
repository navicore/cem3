//! HTTP/1.1 framing.
//!
//! Hand-rolled request builder and response parser. The goal is the
//! smallest correct subset of RFC 9110/9112 needed for `net.http.*`,
//! not a general-purpose HTTP library. v1 explicitly does not:
//!
//! - follow redirects (3xx is returned as-is),
//! - negotiate compression (we send `Accept-Encoding: identity`),
//! - send chunked request bodies (we always use `Content-Length`),
//! - support header continuation lines (deprecated by RFC 9112),
//! - parse trailers in chunked responses (discarded).
//!
//! Both `write_request` and `read_response` operate on anything
//! `Read + Write`, which means a `StreamKind` (TCP or TLS) flows
//! through without the wire layer knowing the difference. Every
//! `read` / `write` yields the strand via the underlying may stream.

use super::ssrf::{Scheme, ValidatedTarget};
use std::io::{BufRead, BufReader, Read, Write};

/// Cap on the response body so a hostile peer can't OOM the process.
/// 10 MB matches the ureq-era ceiling.
pub(crate) const MAX_BODY_SIZE: usize = 10 * 1024 * 1024;

#[derive(Debug)]
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
    pub(crate) keep_alive: bool,
}

/// Write a complete HTTP/1.1 request: request line, framing headers,
/// optional body. Caller supplies `(content_type, body_bytes)` for
/// methods that carry a payload; pass `None` for GET/DELETE.
///
/// The User-Agent advertises the runtime version, which makes
/// server-side request logs identifiable.
pub(crate) fn write_request<W: Write>(
    w: &mut W,
    method: &str,
    target: &ValidatedTarget,
    body: Option<(&str, &[u8])>,
) -> std::io::Result<()> {
    // Any user-supplied header value that lands in the request must be
    // free of control characters — otherwise a `\r\n` in `ct` lets the
    // caller inject arbitrary headers and pull off classic HTTP request
    // smuggling against intermediaries. URL pieces are safe (url crate
    // percent-encodes controls); body bytes go via write_all (no
    // framing interpretation). Content-Type is the one user-string
    // that lands directly in the header block today, so we guard it
    // here. When the header-bag API lands, every supplied value needs
    // the same check.
    if let Some((ct, _)) = body
        && !header_value_safe(ct)
    {
        return Err(std::io::Error::other(
            "invalid byte in Content-Type (control characters not allowed)",
        ));
    }

    let host_header = match (target.scheme, target.port) {
        (Scheme::Http, 80) | (Scheme::Https, 443) => target.host.clone(),
        _ => format!("{}:{}", target.host, target.port),
    };
    let user_agent = concat!("seq/", env!("CARGO_PKG_VERSION"));

    write!(w, "{method} {} HTTP/1.1\r\n", target.path_and_query)?;
    write!(w, "Host: {host_header}\r\n")?;
    write!(w, "User-Agent: {user_agent}\r\n")?;
    write!(w, "Accept: */*\r\n")?;
    write!(w, "Accept-Encoding: identity\r\n")?;
    write!(w, "Connection: keep-alive\r\n")?;
    if let Some((ct, bytes)) = body {
        write!(w, "Content-Type: {ct}\r\n")?;
        write!(w, "Content-Length: {}\r\n", bytes.len())?;
        write!(w, "\r\n")?;
        w.write_all(bytes)?;
    } else {
        write!(w, "\r\n")?;
    }
    w.flush()
}

/// Reject ASCII control characters in a header value. RFC 9110 §5.5
/// allows visible US-ASCII plus HTAB/SP; everything below 0x20 except
/// HTAB is forbidden, as is 0x7F. We're strict and reject HTAB too —
/// no current caller has a reason to embed it, and tightening here
/// avoids a known smuggling vector via TAB-prefixed continuation
/// lines (RFC 9112 deprecated those but some intermediaries still
/// honour them).
fn header_value_safe(v: &str) -> bool {
    !v.bytes().any(|b| b < 0x20 || b == 0x7F)
}

/// Parse a complete HTTP/1.1 response.
///
/// Body framing precedence (RFC 9112 §6.3): `Transfer-Encoding`
/// (chunked) wins over `Content-Length`. If neither is present the
/// body terminates at EOF — a connection in that mode is not eligible
/// for the keep-alive pool.
///
/// Anti-smuggling guards (also RFC 9112 §6.3): a response that
/// declares *both* chunked transfer-encoding and Content-Length, or
/// that lists multiple disagreeing Content-Length values, is
/// rejected outright. These shapes are how request-smuggling attacks
/// fool intermediaries; we don't trust the framing of a server that
/// emits them.
///
/// Note: `BufReader` adds 8 KB of read-ahead. On HTTP/1.1 without
/// pipelining the server must not send bytes past the end of this
/// response on the same connection, so any read-ahead lands strictly
/// inside the current message. If pipelining is ever added, those
/// buffered bytes would be lost across a BufReader-drop boundary —
/// the implementation must change to thread the buffer state through
/// the pool, not just the underlying stream.
pub(crate) fn read_response<R: Read>(r: &mut R) -> Result<Response, String> {
    let mut reader = BufReader::new(r);

    let status_line = read_line_crlf(&mut reader)?;
    let status = parse_status_line(&status_line)?;

    let headers = read_headers(&mut reader)?;
    let mut keep_alive = true;
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for (name, value) in &headers {
        match name.as_str() {
            "connection" if value.eq_ignore_ascii_case("close") => {
                keep_alive = false;
            }
            "transfer-encoding"
                if value
                    .split(',')
                    .any(|t| t.trim().eq_ignore_ascii_case("chunked")) =>
            {
                chunked = true;
            }
            "content-length" => {
                let parsed = parse_content_length(value)?;
                if let Some(prev) = content_length
                    && prev != parsed
                {
                    return Err(format!(
                        "smuggling guard: conflicting Content-Length values ({prev} vs {parsed})"
                    ));
                }
                content_length = Some(parsed);
            }
            _ => {}
        }
    }

    if chunked && content_length.is_some() {
        return Err(
            "smuggling guard: response declares both Transfer-Encoding: chunked and Content-Length"
                .to_string(),
        );
    }

    let body = if chunked {
        read_chunked_body(&mut reader)?
    } else if let Some(len) = content_length {
        if len > MAX_BODY_SIZE {
            return Err(format!(
                "Response body too large ({len} bytes, max {MAX_BODY_SIZE})"
            ));
        }
        read_exact_bounded(&mut reader, len)?
    } else {
        // No framing — read to EOF. Connection can't be reused.
        keep_alive = false;
        let mut buf = Vec::new();
        reader
            .take(MAX_BODY_SIZE as u64 + 1)
            .read_to_end(&mut buf)
            .map_err(|e| format!("read body: {e}"))?;
        if buf.len() > MAX_BODY_SIZE {
            return Err(format!(
                "Response body too large (>{MAX_BODY_SIZE} bytes, EOF-framed)"
            ));
        }
        buf
    };

    Ok(Response {
        status,
        body,
        keep_alive,
    })
}

/// Parse a `Content-Length` header value.
///
/// RFC 9112 §6.3 allows the value to be a single integer OR a
/// comma-separated list of integers that must all agree. A list with
/// any mismatched entry is a smuggling-shaped framing error; reject
/// it. A list with all-agreeing entries reduces to the same single
/// value.
fn parse_content_length(value: &str) -> Result<usize, String> {
    let mut parts = value.split(',').map(|p| p.trim());
    let first = parts
        .next()
        .ok_or_else(|| "smuggling guard: empty Content-Length".to_string())?;
    let head: usize = first
        .parse()
        .map_err(|_| format!("invalid Content-Length: {first:?}"))?;
    for rest in parts {
        let n: usize = rest
            .parse()
            .map_err(|_| format!("invalid Content-Length: {rest:?}"))?;
        if n != head {
            return Err(format!(
                "smuggling guard: conflicting Content-Length list entries ({head} vs {n})"
            ));
        }
    }
    Ok(head)
}

/// Read one line terminated by `\r\n`. The terminator is stripped.
fn read_line_crlf<R: BufRead>(r: &mut R) -> Result<String, String> {
    let mut buf = Vec::new();
    let _ = r
        .read_until(b'\n', &mut buf)
        .map_err(|e| format!("read line: {e}"))?;
    if buf.is_empty() {
        return Err("unexpected EOF reading line".to_string());
    }
    if buf.ends_with(b"\r\n") {
        buf.truncate(buf.len() - 2);
    } else if buf.ends_with(b"\n") {
        // Lenient: some servers/proxies send bare LF.
        buf.truncate(buf.len() - 1);
    }
    String::from_utf8(buf).map_err(|_| "non-UTF8 in header line".to_string())
}

fn parse_status_line(line: &str) -> Result<u16, String> {
    // "HTTP/1.1 200 OK" — split on whitespace, take [1].
    let mut parts = line.splitn(3, ' ');
    let version = parts.next().ok_or("missing HTTP version")?;
    if !version.starts_with("HTTP/1.") {
        return Err(format!("unsupported HTTP version: {version}"));
    }
    let code = parts.next().ok_or("missing status code")?;
    code.parse::<u16>()
        .map_err(|_| format!("invalid status code: {code}"))
}

/// Read headers up to (but not including) the empty-line terminator.
/// Returns `(lowercased-name, value-trimmed)` pairs.
fn read_headers<R: BufRead>(r: &mut R) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    loop {
        let line = read_line_crlf(r)?;
        if line.is_empty() {
            return Ok(out);
        }
        let colon = line
            .find(':')
            .ok_or_else(|| format!("malformed header: {line}"))?;
        let name = line[..colon].trim().to_ascii_lowercase();
        let value = line[colon + 1..].trim().to_string();
        out.push((name, value));
        if out.len() > 256 {
            return Err("too many response headers (>256)".to_string());
        }
    }
}

fn read_exact_bounded<R: Read>(r: &mut R, len: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; len];
    r.read_exact(&mut buf)
        .map_err(|e| format!("read body: {e}"))?;
    Ok(buf)
}

/// RFC 9112 §7.1 chunked decoding. Trailers (between the terminating
/// `0\r\n` and the final `\r\n\r\n`) are read and discarded.
fn read_chunked_body<R: BufRead>(r: &mut R) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let size_line = read_line_crlf(r)?;
        // Strip chunk extensions (after first ';').
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| format!("invalid chunk size: {size_str}"))?;
        if size == 0 {
            // Drain optional trailer headers until blank line.
            loop {
                let line = read_line_crlf(r)?;
                if line.is_empty() {
                    break;
                }
            }
            return Ok(out);
        }
        if out.len().saturating_add(size) > MAX_BODY_SIZE {
            return Err(format!(
                "Response body too large (chunked, >{MAX_BODY_SIZE} bytes)"
            ));
        }
        let mut chunk = vec![0u8; size];
        r.read_exact(&mut chunk)
            .map_err(|e| format!("read chunk: {e}"))?;
        out.extend_from_slice(&chunk);
        // Discard the trailing \r\n after each chunk's data.
        let trailer = read_line_crlf(r)?;
        if !trailer.is_empty() {
            return Err(format!("expected blank line after chunk, got: {trailer}"));
        }
    }
}
