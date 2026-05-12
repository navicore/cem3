//! Runtime declarations for TLS operations.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[RuntimeDecl {
    decl: "declare ptr @patch_seq_tls_client(ptr)",
    category: Some("; TLS operations"),
}];

pub(super) static SYMBOLS: &[(&str, &str)] = &[("net.tls.client", "patch_seq_tls_client")];
