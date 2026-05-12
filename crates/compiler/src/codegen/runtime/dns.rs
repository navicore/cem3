//! Runtime declarations for DNS resolution.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[RuntimeDecl {
    decl: "declare ptr @patch_seq_dns_resolve(ptr)",
    category: Some("; DNS operations"),
}];

pub(super) static SYMBOLS: &[(&str, &str)] = &[("net.dns.resolve", "patch_seq_dns_resolve")];
