//! Runtime declarations for UDP socket operations.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    RuntimeDecl {
        decl: "declare ptr @patch_seq_udp_bind(ptr)",
        category: Some("; UDP operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_udp_send_to(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_udp_receive_from(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_udp_close(ptr)",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    ("net.udp.bind", "patch_seq_udp_bind"),
    ("net.udp.send-to", "patch_seq_udp_send_to"),
    ("net.udp.receive-from", "patch_seq_udp_receive_from"),
    ("net.udp.close", "patch_seq_udp_close"),
];
