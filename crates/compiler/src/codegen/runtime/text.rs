//! Runtime declarations for string operations, binary encodings, crypto, and
//! the HTTP client. Grouped together because they all operate primarily on
//! string payloads.

use super::RuntimeDecl;

pub(super) static DECLS: &[RuntimeDecl] = &[
    // String operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_concat(ptr)",
        category: Some("; String operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_length(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_byte_length(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_char_at(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_substring(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_char_to_string(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_find(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_split(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_contains(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_starts_with(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_empty(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_trim(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_chomp(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_to_upper(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_to_lower(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_equal(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_join(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_json_escape(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_string_to_int(ptr)",
        category: None,
    },
    // Encoding operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_base64_encode(ptr)",
        category: Some("; Encoding operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_base64_decode(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_base64url_encode(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_base64url_decode(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_hex_encode(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_hex_decode(ptr)",
        category: None,
    },
    // Crypto operations
    RuntimeDecl {
        decl: "declare ptr @patch_seq_sha256(ptr)",
        category: Some("; Crypto operations"),
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_hmac_sha256(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_constant_time_eq(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_random_bytes(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_random_int(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_uuid4(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_aes_gcm_encrypt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_aes_gcm_decrypt(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_pbkdf2_sha256(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_ed25519_keypair(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_ed25519_sign(ptr)",
        category: None,
    },
    RuntimeDecl {
        decl: "declare ptr @patch_seq_crypto_ed25519_verify(ptr)",
        category: None,
    },
    // (HTTP client RuntimeDecls moved to codegen/runtime/http.rs.)
];

pub(super) static SYMBOLS: &[(&str, &str)] = &[
    // String operations
    ("string.concat", "patch_seq_string_concat"),
    ("string.length", "patch_seq_string_length"),
    ("string.byte-length", "patch_seq_string_byte_length"),
    ("string.char-at", "patch_seq_string_char_at"),
    ("string.substring", "patch_seq_string_substring"),
    ("char->string", "patch_seq_char_to_string"),
    ("string.find", "patch_seq_string_find"),
    ("string.split", "patch_seq_string_split"),
    ("string.contains", "patch_seq_string_contains"),
    ("string.starts-with", "patch_seq_string_starts_with"),
    ("string.empty?", "patch_seq_string_empty"),
    ("string.trim", "patch_seq_string_trim"),
    ("string.chomp", "patch_seq_string_chomp"),
    ("string.to-upper", "patch_seq_string_to_upper"),
    ("string.to-lower", "patch_seq_string_to_lower"),
    ("string.equal?", "patch_seq_string_equal"),
    ("string.join", "patch_seq_string_join"),
    ("string.json-escape", "patch_seq_json_escape"),
    ("string->int", "patch_seq_string_to_int"),
    // Encoding operations
    ("encoding.base64-encode", "patch_seq_base64_encode"),
    ("encoding.base64-decode", "patch_seq_base64_decode"),
    ("encoding.base64url-encode", "patch_seq_base64url_encode"),
    ("encoding.base64url-decode", "patch_seq_base64url_decode"),
    ("encoding.hex-encode", "patch_seq_hex_encode"),
    ("encoding.hex-decode", "patch_seq_hex_decode"),
    // Crypto operations
    ("crypto.sha256", "patch_seq_sha256"),
    ("crypto.hmac-sha256", "patch_seq_hmac_sha256"),
    ("crypto.constant-time-eq", "patch_seq_constant_time_eq"),
    ("crypto.random-bytes", "patch_seq_random_bytes"),
    ("crypto.random-int", "patch_seq_random_int"),
    ("crypto.uuid4", "patch_seq_uuid4"),
    ("crypto.aes-gcm-encrypt", "patch_seq_crypto_aes_gcm_encrypt"),
    ("crypto.aes-gcm-decrypt", "patch_seq_crypto_aes_gcm_decrypt"),
    ("crypto.pbkdf2-sha256", "patch_seq_crypto_pbkdf2_sha256"),
    ("crypto.ed25519-keypair", "patch_seq_crypto_ed25519_keypair"),
    ("crypto.ed25519-sign", "patch_seq_crypto_ed25519_sign"),
    ("crypto.ed25519-verify", "patch_seq_crypto_ed25519_verify"),
    // (HTTP client SYMBOLS moved to codegen/runtime/http.rs.)
];
