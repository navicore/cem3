//! URL parsing and SSRF (Server-Side Request Forgery) protection.
//!
//! Resolves the URL's host via the may-aware DNS layer (no blocking
//! `getaddrinfo` on the carrier) and rejects requests whose resolved
//! addresses fall into loopback, private, link-local, or other ranges
//! that an attacker could pivot off internal services through.
//!
//! Returns the resolved `IpAddr` list alongside the parsed target so
//! the caller can connect without a second DNS round-trip.
//!
//! ## Failure mode
//!
//! If DNS resolution fails entirely (empty result), the request is
//! *allowed* and is expected to fail at connect time. This matches
//! the previous ureq-era behaviour and avoids surprising callers who
//! reach an offline endpoint with an SSRF-shaped error.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// Scheme of a validated HTTP target. URL parsing rejects everything
/// outside this enum, so downstream code never has to handle other
/// schemes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Scheme {
    Http,
    Https,
}

/// The result of a successful SSRF check: scheme, host, port, the
/// resolved addresses to dial, and the path+query string the request
/// line should target.
pub(crate) struct ValidatedTarget {
    pub(crate) scheme: Scheme,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) addrs: Vec<IpAddr>,
    pub(crate) path_and_query: String,
}

/// Parse a URL, run SSRF checks, and return the dial target.
///
/// Yields the strand (via `dns::resolve`) while resolution is in
/// flight. Empty resolution result is treated as a *transient* DNS
/// failure: the request is allowed through and will surface its real
/// error at connect time — same as the ureq-era client.
pub(crate) fn validate_url(url: &str) -> Result<ValidatedTarget, String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {e}"))?;

    let scheme = match parsed.scheme() {
        "http" => Scheme::Http,
        "https" => Scheme::Https,
        s => return Err(format!("Blocked scheme '{s}': only http/https allowed")),
    };

    let host = parsed
        .host_str()
        .ok_or_else(|| "URL has no host".to_string())?
        .to_string();

    // Defense-in-depth: catch explicit localhost names before DNS.
    let host_lower = host.to_lowercase();
    if host_lower == "localhost"
        || host_lower == "localhost.localdomain"
        || host_lower.ends_with(".localhost")
    {
        return Err("Blocked: localhost access not allowed".to_string());
    }

    let port = parsed.port().unwrap_or(match scheme {
        Scheme::Https => 443,
        Scheme::Http => 80,
    });

    let addrs = crate::dns::resolve_to_ips(&host);
    for ip in &addrs {
        if is_dangerous_ip(*ip) {
            return Err(format!(
                "Blocked: {host} resolves to private/internal IP {ip}"
            ));
        }
    }

    // `url::Url::path()` for any hierarchical (http/https) URL is
    // guaranteed to start with `/` and is never empty — `url`
    // normalises bare `http://example.com` to `path = "/"`. So the
    // two arms below are exhaustive.
    let path_and_query = match parsed.query() {
        Some(q) => format!("{}?{q}", parsed.path()),
        None => parsed.path().to_string(),
    };

    Ok(ValidatedTarget {
        scheme,
        host,
        port,
        addrs,
        path_and_query,
    })
}

/// Thin wrapper that preserves the public surface of the ureq-era
/// validator. Existing unit tests assert against this signature.
#[allow(dead_code)] // exposed for tests; callers use validate_url
pub(crate) fn validate_url_for_ssrf(url: &str) -> Result<(), String> {
    validate_url(url).map(|_| ())
}

/// IPv4 address that an attacker could pivot off (loopback, RFC 1918
/// private, link-local incl. cloud metadata, broadcast).
pub(crate) fn is_dangerous_ipv4(ip: Ipv4Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    let o = ip.octets();
    if o[0] == 10 {
        return true;
    }
    if o[0] == 172 && (16..=31).contains(&o[1]) {
        return true;
    }
    if o[0] == 192 && o[1] == 168 {
        return true;
    }
    if o[0] == 169 && o[1] == 254 {
        return true;
    }
    ip.is_broadcast()
}

/// IPv6 address that an attacker could pivot off (loopback,
/// link-local `fe80::/10`, unique local `fc00::/7`, IPv4-mapped that
/// resolves to a dangerous IPv4).
pub(crate) fn is_dangerous_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_loopback() {
        return true;
    }
    let s = ip.segments();
    if (s[0] & 0xffc0) == 0xfe80 {
        return true;
    }
    if (s[0] & 0xfe00) == 0xfc00 {
        return true;
    }
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_dangerous_ipv4(v4);
    }
    false
}

pub(crate) fn is_dangerous_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_dangerous_ipv4(v4),
        IpAddr::V6(v6) => is_dangerous_ipv6(v6),
    }
}
