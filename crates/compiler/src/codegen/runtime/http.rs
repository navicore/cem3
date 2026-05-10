//! Runtime declarations for HTTP client operations (under `net.*`).

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    RuntimeDecl {
        decl: "declare ptr @patch_seq_http_get(ptr)",
        category: Some("; HTTP client operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_http_post(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_http_put(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_http_delete(ptr)",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    ("net.http.get", "patch_seq_http_get"),
    ("net.http.post", "patch_seq_http_post"),
    ("net.http.put", "patch_seq_http_put"),
    ("net.http.delete", "patch_seq_http_delete"),
];
