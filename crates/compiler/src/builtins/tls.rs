//! TLS client operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // net.tls.client ( Socket String -- Socket Bool )
    // Upgrades a connected TCP Socket into a TLS-wrapped Socket by
    // running the rustls handshake. The returned Socket is a fresh id;
    // the original is consumed.
    builtin!(sigs, "net.tls.client", (a Socket String -- a Socket Bool));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    docs.insert(
        "net.tls.client",
        "Upgrade a connected Socket to TLS. Stack: ( Socket String -- Socket Bool ) \
         where String is the hostname (drives SNI and certificate validation). \
         Bool is false on handshake failure (bad cert, hostname mismatch, \
         protocol error), empty hostname, type mismatch, or registry exhaustion. \
         Yields the strand on every step of the handshake. Trust roots come \
         from webpki-roots; subsequent net.tcp.read / net.tcp.write / \
         net.tcp.close on the returned Socket transparently dispatch through TLS.",
    );
}
