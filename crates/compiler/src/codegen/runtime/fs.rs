//! Runtime declarations for file and directory operations.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    // File operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_slurp(ptr)",
        category: Some("; File operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_exists(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_for_each_line(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_spit(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_append(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_delete(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_file_size(ptr)",
        category: None,
    },
    // Directory operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_dir_exists(ptr)",
        category: Some("; Directory operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_dir_make(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_dir_delete(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_dir_list(ptr)",
        category: None,
    },
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    // File operations
    ("file.slurp", "patch_seq_file_slurp"),
    ("file.exists?", "patch_seq_file_exists"),
    ("file.for-each-line", "patch_seq_file_for_each_line"),
    ("file.spit", "patch_seq_file_spit"),
    ("file.append", "patch_seq_file_append"),
    ("file.delete", "patch_seq_file_delete"),
    ("file.size", "patch_seq_file_size"),
    // Directory operations
    ("dir.exists?", "patch_seq_dir_exists"),
    ("dir.make", "patch_seq_dir_make"),
    ("dir.delete", "patch_seq_dir_delete"),
    ("dir.list", "patch_seq_dir_list"),
];
