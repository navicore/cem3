//! Runtime declarations for TCP socket operations.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_listen(ptr)",
        category: Some("; TCP operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_connect(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_accept(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_read(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_write(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_tcp_close(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_socket_cast(ptr)",
        category: Some("; Socket <-> Int casts (identity at runtime — Socket is a phantom)"),
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    ("net.tcp.listen", "patch_seq_tcp_listen"),
    ("net.tcp.connect", "patch_seq_tcp_connect"),
    ("net.tcp.accept", "patch_seq_tcp_accept"),
    ("net.tcp.read", "patch_seq_tcp_read"),
    ("net.tcp.write", "patch_seq_tcp_write"),
    ("net.tcp.close", "patch_seq_tcp_close"),
    // Socket is a compile-time phantom over Int, so both casts share one
    // identity shim in the runtime.
    ("fd->socket", "patch_seq_socket_cast"),
    ("socket->fd", "patch_seq_socket_cast"),
];
