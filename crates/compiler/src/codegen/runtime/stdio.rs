//! Runtime declarations for stdio, value pushes, and `Int`/`Symbol`/`String`
//! conversions.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    // Core push operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_int(ptr, i64)",
        category: Some("; Runtime function declarations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_string(ptr, ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_string_bytes(ptr, ptr, i64)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_symbol(ptr, ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_push_interned_symbol(ptr, ptr)",
        category: None,
    },
    // I/O operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_write(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_write_line(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_read_line(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_read_n(ptr)",
        category: None,
    },
    // Type conversions
    RuntimeDecl {
        decl: "declare ptr @patch_seq_int_to_string(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_symbol_to_string(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_to_symbol(ptr)",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    ("io.write", "patch_seq_write"),
    ("io.write-line", "patch_seq_write_line"),
    ("io.read-line", "patch_seq_read_line"),
    ("io.read-n", "patch_seq_read_n"),
    ("int->string", "patch_seq_int_to_string"),
    ("symbol->string", "patch_seq_symbol_to_string"),
    ("string->symbol", "patch_seq_string_to_symbol"),
];
