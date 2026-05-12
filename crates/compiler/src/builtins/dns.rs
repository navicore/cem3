//! DNS resolution.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // DNS Operations
    // =========================================================================
    //
    // Hostname resolution offloaded to an OS-thread pool so may carriers
    // never park on `getaddrinfo`. Returns a list of IP-address strings
    // with a success Bool on top — the standard `(value Bool)` shape so
    // `[ ... ] [ ... ] if` works directly.
    //
    // The list type uses `V` (same convention as list.* builtins): the
    // type checker treats lists nominally as the V family.
    builtin!(sigs, "net.dns.resolve", (a String -- a V Bool));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    docs.insert(
        "net.dns.resolve",
        "Resolve a hostname to a list of IP-address strings. \
         ( hostname -- list-of-strings Bool ). \
         Yields the strand cooperatively while the lookup runs on a \
         dedicated OS-thread pool. Inherits platform-correct resolution \
         (/etc/hosts, systemd-resolved, mDNS, VPN/corp DNS). On failure \
         pushes (empty-list, false). \
         Worker count via SEQ_DNS_WORKERS (default 8).",
    );
}
