//! I/O, command-line arguments, and primitive type conversions.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // I/O Operations
    // =========================================================================

    builtin!(sigs, "io.write", (a String -- a)); // Write without newline
    builtin!(sigs, "io.write-line", (a String -- a));
    builtin!(sigs, "io.read-line", (a -- a String Bool)); // Returns line + success flag
    builtin!(sigs, "io.read-n", (a Int -- a String Int)); // Read N bytes, returns bytes + status

    // =========================================================================
    // Command-line Arguments
    // =========================================================================

    builtin!(sigs, "args.count", (a -- a Int));
    builtin!(sigs, "args.at", (a Int -- a String));

    // =========================================================================
    // Type Conversions
    // =========================================================================

    builtin!(sigs, "int->string", (a Int -- a String));
    builtin!(sigs, "int->float", (a Int -- a Float));
    builtin!(sigs, "float->int", (a Float -- a Int));
    builtin!(sigs, "float->string", (a Float -- a String));
    builtin!(sigs, "string->int", (a String -- a Int Bool)); // value + success flag
    builtin!(sigs, "string->float", (a String -- a Float Bool)); // value + success flag
    builtin!(sigs, "char->string", (a Int -- a String));
    builtin!(sigs, "symbol->string", (a Symbol -- a String));
    builtin!(sigs, "string->symbol", (a String -- a Symbol));

    // =========================================================================
    // Byte construction (for binary protocol encoders written in Seq)
    // =========================================================================

    builtin!(sigs, "int.to-bytes-i32-be", (a Int -- a String));
    builtin!(sigs, "float.to-bytes-f32-be", (a Float -- a String));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // I/O Operations
    docs.insert(
        "io.write",
        "Write a string to stdout without a trailing newline.",
    );
    docs.insert(
        "io.write-line",
        "Write a string to stdout followed by a newline.",
    );
    docs.insert(
        "io.read-line",
        "Read a line from stdin. Returns (String Bool) -- Bool is false on EOF or read error.",
    );
    docs.insert(
        "io.read-n",
        "Read N bytes from stdin. Returns (bytes, status_code).",
    );

    // Command-line Arguments
    docs.insert("args.count", "Get the number of command-line arguments.");
    docs.insert("args.at", "Get the command-line argument at index N.");

    // Type Conversions
    docs.insert(
        "int->string",
        "Convert an integer to its string representation.",
    );
    docs.insert(
        "int->float",
        "Convert an integer to a floating-point number.",
    );
    docs.insert("float->int", "Truncate a float to an integer.");
    docs.insert(
        "float->string",
        "Convert a float to its string representation.",
    );
    docs.insert(
        "string->int",
        "Parse a string as an integer. Returns (Int Bool) -- Bool is false if string is not a valid integer.",
    );
    docs.insert(
        "string->float",
        "Parse a string as a float. Returns (Float Bool) -- Bool is false if string is not a valid number.",
    );
    docs.insert(
        "char->string",
        "Convert a Unicode codepoint to a single-character string.",
    );
    docs.insert(
        "symbol->string",
        "Convert a symbol to its string representation.",
    );
    docs.insert("string->symbol", "Intern a string as a symbol.");

    // Byte construction
    docs.insert(
        "int.to-bytes-i32-be",
        "Encode an Int as a 4-byte big-endian i32 String. Truncates to 32 bits if out of range; result is byte-clean for binary protocols.",
    );
    docs.insert(
        "float.to-bytes-f32-be",
        "Encode a Float as a 4-byte big-endian IEEE-754 f32 String. Precision-converts the f64 to f32; result is byte-clean for binary protocols.",
    );
}
