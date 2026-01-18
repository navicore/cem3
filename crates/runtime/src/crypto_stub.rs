//! Stub module for crypto operations when the "crypto" feature is disabled.
//!
//! These functions provide the same FFI interface but panic with helpful messages
//! instructing users to enable the crypto feature.

use seq_core::stack::Stack;

const FEATURE_MSG: &str = "crypto feature not enabled. Rebuild with: cargo build --features crypto";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_sha256(_stack: Stack) -> Stack {
    panic!("crypto.sha256 requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_hmac_sha256(_stack: Stack) -> Stack {
    panic!("crypto.hmac-sha256 requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_constant_time_eq(_stack: Stack) -> Stack {
    panic!("crypto.constant-time-eq requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_random_bytes(_stack: Stack) -> Stack {
    panic!("crypto.random-bytes requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_uuid4(_stack: Stack) -> Stack {
    panic!("crypto.uuid4 requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_random_int(_stack: Stack) -> Stack {
    panic!("crypto.random-int requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_aes_gcm_encrypt(_stack: Stack) -> Stack {
    panic!("crypto.aes-gcm-encrypt requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_aes_gcm_decrypt(_stack: Stack) -> Stack {
    panic!("crypto.aes-gcm-decrypt requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_pbkdf2_sha256(_stack: Stack) -> Stack {
    panic!("crypto.pbkdf2-sha256 requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_keypair(_stack: Stack) -> Stack {
    panic!("crypto.ed25519-keypair requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_sign(_stack: Stack) -> Stack {
    panic!("crypto.ed25519-sign requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_verify(_stack: Stack) -> Stack {
    panic!("crypto.ed25519-verify requires {}", FEATURE_MSG);
}
