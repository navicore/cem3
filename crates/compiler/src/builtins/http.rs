//! HTTP client operations (under the `net.*` umbrella).
//!
//! These are the request-side builtins (`net.http.get`, `net.http.post`,
//! etc.). The server-side response/parsing helpers live in
//! `stdlib/http.seq` (`include std:http`) and are unrelated.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    builtin!(sigs, "net.http.get",    (a String -- a M));
    builtin!(sigs, "net.http.post",   (a String String String -- a M));
    builtin!(sigs, "net.http.put",    (a String String String -- a M));
    builtin!(sigs, "net.http.delete", (a String -- a M));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    docs.insert(
        "net.http.get",
        "HTTP GET request. ( url -- response-map ) Map has status, body, ok, error.",
    );
    docs.insert(
        "net.http.post",
        "HTTP POST request. ( url body content-type -- response-map )",
    );
    docs.insert(
        "net.http.put",
        "HTTP PUT request. ( url body content-type -- response-map )",
    );
    docs.insert(
        "net.http.delete",
        "HTTP DELETE request. ( url -- response-map )",
    );
}
