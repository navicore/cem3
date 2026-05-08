//! File and directory operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // File Operations
    // =========================================================================

    builtin!(sigs, "file.slurp", (a String -- a String Bool)); // returns (content success) - errors are values
    builtin!(sigs, "file.exists?", (a String -- a Bool));
    builtin!(sigs, "file.spit", (a String String -- a Bool)); // (content path -- success)
    builtin!(sigs, "file.append", (a String String -- a Bool)); // (content path -- success)
    builtin!(sigs, "file.delete", (a String -- a Bool));
    builtin!(sigs, "file.size", (a String -- a Int Bool)); // (path -- size success)

    // Directory operations
    builtin!(sigs, "dir.exists?", (a String -- a Bool));
    builtin!(sigs, "dir.make", (a String -- a Bool));
    builtin!(sigs, "dir.delete", (a String -- a Bool));
    builtin!(sigs, "dir.list", (a String -- a V Bool)); // V = List variant

    // file.for-each-line: Complex quotation type - defined manually
    sigs.insert(
        "file.for-each-line".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()).push(Type::String),
                    StackType::RowVar("a".to_string()),
                )))),
            StackType::RowVar("a".to_string()).push(Type::Bool),
        ),
    );
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // File Operations
    docs.insert(
        "file.slurp",
        "Read entire file contents. Returns (String Bool) -- Bool is false if file not found or unreadable.",
    );
    docs.insert("file.exists?", "Check if a file exists at the given path.");
    docs.insert(
        "file.spit",
        "Write string to file (creates or overwrites). Returns Bool -- false on write failure.",
    );
    docs.insert(
        "file.append",
        "Append string to file (creates if needed). Returns Bool -- false on write failure.",
    );
    docs.insert(
        "file.delete",
        "Delete a file. Returns Bool -- false on failure.",
    );
    docs.insert(
        "file.size",
        "Get file size in bytes. Returns (Int Bool) -- Bool is false if file not found.",
    );
    docs.insert(
        "file.for-each-line",
        "Execute a quotation for each line in a file. Returns Bool -- false if the file could not be opened or a read error occurred.",
    );

    // Directory Operations
    docs.insert(
        "dir.exists?",
        "Check if a directory exists at the given path.",
    );
    docs.insert(
        "dir.make",
        "Create a directory (and parent directories if needed). Returns Bool -- false on failure.",
    );
    docs.insert(
        "dir.delete",
        "Delete an empty directory. Returns Bool -- false on failure.",
    );
    docs.insert(
        "dir.list",
        "List directory contents. Returns (List Bool) -- Bool is false if directory not found.",
    );
}
