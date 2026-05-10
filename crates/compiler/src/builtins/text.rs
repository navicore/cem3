//! String operations, encoding, crypto, HTTP, regex, and compression.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // String Operations
    // =========================================================================

    builtin!(sigs, "string.concat", (a String String -- a String));
    builtin!(sigs, "string.length", (a String -- a Int));
    builtin!(sigs, "string.byte-length", (a String -- a Int));
    builtin!(sigs, "string.char-at", (a String Int -- a Int));
    builtin!(sigs, "string.substring", (a String Int Int -- a String));
    builtin!(sigs, "string.find", (a String String -- a Int));
    builtin!(sigs, "string.split", (a String String -- a V)); // Returns Variant (list)
    builtin!(sigs, "string.contains", (a String String -- a Bool));
    builtin!(sigs, "string.starts-with", (a String String -- a Bool));
    builtin!(sigs, "string.empty?", (a String -- a Bool));
    builtin!(sigs, "string.equal?", (a String String -- a Bool));
    builtin!(sigs, "string.join", (a V String -- a String)); // ( list separator -- joined )

    // Symbol operations
    builtin!(sigs, "symbol.=", (a Symbol Symbol -- a Bool));

    // String transformations
    builtins_string_to_string!(
        sigs,
        "string.trim",
        "string.chomp",
        "string.to-upper",
        "string.to-lower",
        "string.json-escape"
    );

    // =========================================================================
    // Encoding Operations
    // =========================================================================

    builtin!(sigs, "encoding.base64-encode", (a String -- a String));
    builtin!(sigs, "encoding.base64-decode", (a String -- a String Bool));
    builtin!(sigs, "encoding.base64url-encode", (a String -- a String));
    builtin!(sigs, "encoding.base64url-decode", (a String -- a String Bool));
    builtin!(sigs, "encoding.hex-encode", (a String -- a String));
    builtin!(sigs, "encoding.hex-decode", (a String -- a String Bool));

    // =========================================================================
    // Crypto Operations
    // =========================================================================

    builtin!(sigs, "crypto.sha256", (a String -- a String));
    builtin!(sigs, "crypto.hmac-sha256", (a String String -- a String));
    builtin!(sigs, "crypto.constant-time-eq", (a String String -- a Bool));
    builtin!(sigs, "crypto.random-bytes", (a Int -- a String));
    builtin!(sigs, "crypto.random-int", (a Int Int -- a Int));
    builtin!(sigs, "crypto.uuid4", (a -- a String));
    builtin!(sigs, "crypto.aes-gcm-encrypt", (a String String -- a String Bool));
    builtin!(sigs, "crypto.aes-gcm-decrypt", (a String String -- a String Bool));
    builtin!(sigs, "crypto.pbkdf2-sha256", (a String String Int -- a String Bool));
    builtin!(sigs, "crypto.ed25519-keypair", (a -- a String String));
    builtin!(sigs, "crypto.ed25519-sign", (a String String -- a String Bool));
    builtin!(sigs, "crypto.ed25519-verify", (a String String String -- a Bool));

    // (HTTP Client signatures moved to builtins/http.rs under net.* namespace.)

    // =========================================================================
    // Regular Expression Operations
    // =========================================================================

    // Regex operations return Bool for error handling (invalid regex)
    builtin!(sigs, "regex.match?", (a String String -- a Bool));
    builtin!(sigs, "regex.find", (a String String -- a String Bool));
    builtin!(sigs, "regex.find-all", (a String String -- a V Bool));
    builtin!(sigs, "regex.replace", (a String String String -- a String Bool));
    builtin!(sigs, "regex.replace-all", (a String String String -- a String Bool));
    builtin!(sigs, "regex.captures", (a String String -- a V Bool));
    builtin!(sigs, "regex.split", (a String String -- a V Bool));
    builtin!(sigs, "regex.valid?", (a String -- a Bool));

    // =========================================================================
    // Compression Operations
    // =========================================================================

    builtin!(sigs, "compress.gzip", (a String -- a String Bool));
    builtin!(sigs, "compress.gzip-level", (a String Int -- a String Bool));
    builtin!(sigs, "compress.gunzip", (a String -- a String Bool));
    builtin!(sigs, "compress.zstd", (a String -- a String Bool));
    builtin!(sigs, "compress.zstd-level", (a String Int -- a String Bool));
    builtin!(sigs, "compress.unzstd", (a String -- a String Bool));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // String Operations
    docs.insert("string.concat", "Concatenate two strings.");
    docs.insert("string.length", "Get the character length of a string.");
    docs.insert("string.byte-length", "Get the byte length of a string.");
    docs.insert(
        "string.char-at",
        "Get Unicode codepoint at character index.",
    );
    docs.insert(
        "string.substring",
        "Extract substring from start index with length.",
    );
    docs.insert(
        "string.find",
        "Find substring. Returns index or -1 if not found. ( str substring -- Int )",
    );
    docs.insert(
        "string.split",
        "Split string by delimiter. Returns a list. ( str delimiter -- list )",
    );
    docs.insert(
        "string.contains",
        "Check if string contains substring. ( str substring -- Bool )",
    );
    docs.insert(
        "string.starts-with",
        "Check if string starts with prefix. ( str prefix -- Bool )",
    );
    docs.insert("string.empty?", "Check if string is empty. ( str -- Bool )");
    docs.insert(
        "string.equal?",
        "Check if two strings are equal. ( str str -- Bool )",
    );
    docs.insert(
        "string.join",
        "Join a list of values with a separator string. ( list sep -- string )",
    );
    docs.insert("string.trim", "Remove leading and trailing whitespace.");
    docs.insert("string.chomp", "Remove trailing newline.");
    docs.insert("string.to-upper", "Convert to uppercase.");
    docs.insert("string.to-lower", "Convert to lowercase.");
    docs.insert("string.json-escape", "Escape special characters for JSON.");
    docs.insert("symbol.=", "Check if two symbols are equal.");

    // Encoding Operations
    docs.insert(
        "encoding.base64-encode",
        "Encode a string to Base64 (standard alphabet with padding).",
    );
    docs.insert(
        "encoding.base64-decode",
        "Decode a Base64 string. Returns (decoded, success).",
    );
    docs.insert(
        "encoding.base64url-encode",
        "Encode to URL-safe Base64 (no padding). Suitable for JWTs and URLs.",
    );
    docs.insert(
        "encoding.base64url-decode",
        "Decode URL-safe Base64. Returns (decoded, success).",
    );
    docs.insert(
        "encoding.hex-encode",
        "Encode a string to lowercase hexadecimal.",
    );
    docs.insert(
        "encoding.hex-decode",
        "Decode a hexadecimal string. Returns (decoded, success).",
    );

    // Crypto Operations
    docs.insert(
        "crypto.sha256",
        "Compute SHA-256 hash of a string. Returns 64-char hex digest.",
    );
    docs.insert(
        "crypto.hmac-sha256",
        "Compute HMAC-SHA256 signature. ( message key -- signature )",
    );
    docs.insert(
        "crypto.constant-time-eq",
        "Timing-safe string comparison. Use for comparing signatures/tokens.",
    );
    docs.insert(
        "crypto.random-bytes",
        "Generate N cryptographically secure random bytes as hex string.",
    );
    docs.insert(
        "crypto.random-int",
        "Generate uniform random integer in [min, max). ( min max -- Int ) Uses rejection sampling to avoid modulo bias.",
    );
    docs.insert("crypto.uuid4", "Generate a random UUID v4 string.");
    docs.insert(
        "crypto.aes-gcm-encrypt",
        "Encrypt with AES-256-GCM. ( plaintext hex-key -- ciphertext success )",
    );
    docs.insert(
        "crypto.aes-gcm-decrypt",
        "Decrypt AES-256-GCM ciphertext. ( ciphertext hex-key -- plaintext success )",
    );
    docs.insert(
        "crypto.pbkdf2-sha256",
        "Derive key from password. ( password salt iterations -- hex-key success ) Min 1000 iterations, 100000+ recommended.",
    );
    docs.insert(
        "crypto.ed25519-keypair",
        "Generate Ed25519 keypair. ( -- public-key private-key ) Both as 64-char hex strings.",
    );
    docs.insert(
        "crypto.ed25519-sign",
        "Sign message with Ed25519 private key. ( message private-key -- signature success ) Signature is 128-char hex.",
    );
    docs.insert(
        "crypto.ed25519-verify",
        "Verify Ed25519 signature. ( message signature public-key -- valid )",
    );

    // (HTTP Client docs moved to builtins/http.rs under net.* namespace.)

    // Regular Expression Operations
    docs.insert(
        "regex.match?",
        "Check if pattern matches anywhere in string. ( text pattern -- bool )",
    );
    docs.insert(
        "regex.find",
        "Find first match. ( text pattern -- matched success )",
    );
    docs.insert(
        "regex.find-all",
        "Find all matches. ( text pattern -- list success )",
    );
    docs.insert(
        "regex.replace",
        "Replace first match. ( text pattern replacement -- result success )",
    );
    docs.insert(
        "regex.replace-all",
        "Replace all matches. ( text pattern replacement -- result success )",
    );
    docs.insert(
        "regex.captures",
        "Extract capture groups. ( text pattern -- groups success )",
    );
    docs.insert(
        "regex.split",
        "Split string by pattern. ( text pattern -- list success )",
    );
    docs.insert(
        "regex.valid?",
        "Check if pattern is valid regex. ( pattern -- bool )",
    );

    // Compression Operations
    docs.insert(
        "compress.gzip",
        "Compress string with gzip. Returns base64-encoded data. ( data -- compressed success )",
    );
    docs.insert(
        "compress.gzip-level",
        "Compress with gzip at level 1-9. ( data level -- compressed success )",
    );
    docs.insert(
        "compress.gunzip",
        "Decompress gzip data. ( base64-data -- decompressed success )",
    );
    docs.insert(
        "compress.zstd",
        "Compress string with zstd. Returns base64-encoded data. ( data -- compressed success )",
    );
    docs.insert(
        "compress.zstd-level",
        "Compress with zstd at level 1-22. ( data level -- compressed success )",
    );
    docs.insert(
        "compress.unzstd",
        "Decompress zstd data. ( base64-data -- decompressed success )",
    );
}
