//! UDP socket operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // UDP Operations
    // =========================================================================
    //
    // Datagram-oriented; sockets are Int handles. Every word ends with a
    // success Bool on top so callers can `[ ... ] [ ... ] if`.
    //
    // `udp.bind` returns three values: (socket, bound-port, success).
    // The bound-port differs from the requested port only when the user
    // passed 0 (let the OS pick); for non-zero requests the returned
    // port equals the request.
    builtin!(sigs, "net.udp.bind",         (a Int    -- a Socket Int Bool));
    builtin!(sigs, "net.udp.send-to",      (a String String Int Socket -- a Bool));
    builtin!(sigs, "net.udp.receive-from", (a Socket -- a String String Int Bool));
    builtin!(sigs, "net.udp.close",        (a Socket -- a Bool));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    docs.insert(
        "net.udp.bind",
        "Bind a UDP socket to a local port. ( port -- socket bound-port Bool ). \
         port=0 lets the OS pick; bound-port is the actual assigned port. \
         On failure pushes (0, 0, false).",
    );
    docs.insert(
        "net.udp.send-to",
        "Send a datagram to host:port from a bound socket. \
         ( bytes host port socket -- Bool ).",
    );
    docs.insert(
        "net.udp.receive-from",
        "Receive one datagram (yields the strand). \
         ( socket -- bytes host port Bool ). \
         On failure pushes (\"\", \"\", 0, false).",
    );
    docs.insert("net.udp.close", "Release a UDP socket. ( socket -- Bool ).");
}
