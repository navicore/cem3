//! TCP socket operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // TCP Operations
    // =========================================================================

    // TCP operations return Bool for error handling.
    // The fd slot is typed as `Socket` (a phantom over Int) so the type
    // checker rejects passing arbitrary integers to net.tcp.write / net.tcp.close.
    builtin!(sigs, "net.tcp.listen",  (a Int    -- a Socket Bool));
    builtin!(sigs, "net.tcp.connect", (a String Int -- a Socket Bool));
    builtin!(sigs, "net.tcp.accept",  (a Socket -- a Socket Bool));
    builtin!(sigs, "net.tcp.read",    (a Socket -- a String Bool));
    builtin!(sigs, "net.tcp.write",   (a String Socket -- a Bool));
    builtin!(sigs, "net.tcp.close",   (a Socket -- a Bool));

    // Escape hatches for FFI / debugging — at runtime both are identity
    // (Socket is a compile-time phantom over the same i64 fd).
    builtin!(sigs, "fd->socket", (a Int    -- a Socket));
    builtin!(sigs, "socket->fd", (a Socket -- a Int));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // TCP Operations (under net.* namespace)
    docs.insert(
        "net.tcp.listen",
        "Start listening on a port. Returns (Socket Bool) -- Bool is false on failure.",
    );
    docs.insert(
        "net.tcp.connect",
        "Connect to a remote endpoint. Stack: ( String Int -- Socket Bool ) where \
         String is the hostname (or IP literal) and Int is the port. Resolves the \
         hostname via net.dns.resolve, then connects with may::net::TcpStream. \
         Bool is false on resolution failure, every-address-failed, invalid port, \
         or socket-registry exhaustion. Yields the strand on every step.",
    );
    docs.insert(
        "net.tcp.accept",
        "Accept a connection. Returns (Socket Bool) -- Bool is false on failure.",
    );
    docs.insert(
        "net.tcp.read",
        "Read from a connection. Returns (String Bool) -- Bool is false on failure.",
    );
    docs.insert(
        "net.tcp.write",
        "Write to a connection. Returns Bool -- false on failure.",
    );
    docs.insert(
        "net.tcp.close",
        "Close a connection. Returns Bool -- false on failure.",
    );
    docs.insert(
        "fd->socket",
        "Cast a raw Int file descriptor to a Socket. Escape hatch for FFI; \
         no runtime conversion (Socket is a phantom over Int).",
    );
    docs.insert(
        "socket->fd",
        "Cast a Socket back to a raw Int file descriptor. Escape hatch for \
         FFI / debugging; no runtime conversion.",
    );
}
