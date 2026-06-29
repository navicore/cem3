//! Ed25519 signature operations: keypair generation, sign, verify.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
// Ed25519's `SigningKey::generate` requires a `CryptoRngCore` from
// `rand_core` 0.6 (the version ed25519-dalek 2.x is built against). We
// pull `OsRng` directly from `rand_core` 0.6 rather than the old
// `aes_gcm::aead::OsRng` re-export, which was removed in aead 0.6.
use rand_core::OsRng;

// ============================================================================
// Ed25519 Digital Signatures
// ============================================================================

/// Generate an Ed25519 keypair
///
/// Stack effect: ( -- public-key private-key )
///
/// Returns:
/// - public-key: Hex-encoded 32-byte public key (64 hex characters)
/// - private-key: Hex-encoded 32-byte private key (64 hex characters)
///
/// # Safety
/// Stack must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_keypair(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "crypto.ed25519-keypair: stack is null");

    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();

    let private_hex = hex::encode(signing_key.to_bytes());
    let public_hex = hex::encode(verifying_key.to_bytes());

    let stack = unsafe { push(stack, Value::String(global_string(public_hex))) };
    unsafe { push(stack, Value::String(global_string(private_hex))) }
}

/// Sign a message with an Ed25519 private key
///
/// Stack effect: ( message private-key -- signature success )
///
/// Parameters:
/// - message: The message to sign (any string)
/// - private-key: Hex-encoded 32-byte private key (64 hex characters)
///
/// Returns:
/// - signature: Hex-encoded 64-byte signature (128 hex characters)
/// - success: Bool indicating success
///
/// # Safety
/// Stack must have String, String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_sign(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "crypto.ed25519-sign: stack is null");

    let (stack, key_val) = unsafe { pop(stack) };
    let (stack, msg_val) = unsafe { pop(stack) };

    match (msg_val, key_val) {
        (Value::String(message), Value::String(private_key_hex)) => {
            // Message is byte-clean; the key is hex (text).
            match ed25519_sign(message.as_bytes(), private_key_hex.as_str_or_empty()) {
                Some(signature) => {
                    let stack = unsafe { push(stack, Value::String(global_string(signature))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                None => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("crypto.ed25519-sign: expected String, String on stack"),
    }
}

/// Verify an Ed25519 signature
///
/// Stack effect: ( message signature public-key -- valid )
///
/// Parameters:
/// - message: The original message
/// - signature: Hex-encoded 64-byte signature (128 hex characters)
/// - public-key: Hex-encoded 32-byte public key (64 hex characters)
///
/// Returns:
/// - valid: Bool indicating whether the signature is valid
///
/// # Safety
/// Stack must have String, String, String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_ed25519_verify(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "crypto.ed25519-verify: stack is null");

    let (stack, pubkey_val) = unsafe { pop(stack) };
    let (stack, sig_val) = unsafe { pop(stack) };
    let (stack, msg_val) = unsafe { pop(stack) };

    match (msg_val, sig_val, pubkey_val) {
        (Value::String(message), Value::String(signature_hex), Value::String(public_key_hex)) => {
            // Message is byte-clean; signature and key are hex (text).
            let valid = ed25519_verify(
                message.as_bytes(),
                signature_hex.as_str_or_empty(),
                public_key_hex.as_str_or_empty(),
            );
            unsafe { push(stack, Value::Bool(valid)) }
        }
        _ => panic!("crypto.ed25519-verify: expected String, String, String on stack"),
    }
}

// Helper functions for Ed25519

/// Sign arbitrary bytes (binary or text). The key is hex-encoded; the
/// returned signature is hex-encoded (always valid UTF-8).
pub(super) fn ed25519_sign(message: &[u8], private_key_hex: &str) -> Option<String> {
    let key_bytes = hex::decode(private_key_hex).ok()?;
    if key_bytes.len() != 32 {
        return None;
    }

    let key_array: [u8; 32] = key_bytes.try_into().ok()?;
    let signing_key = SigningKey::from_bytes(&key_array);
    let signature = signing_key.sign(message);

    Some(hex::encode(signature.to_bytes()))
}

/// Verify a signature against arbitrary bytes (binary or text).
pub(super) fn ed25519_verify(message: &[u8], signature_hex: &str, public_key_hex: &str) -> bool {
    let verify_inner = || -> Option<bool> {
        let sig_bytes = hex::decode(signature_hex).ok()?;
        if sig_bytes.len() != 64 {
            return Some(false);
        }

        let pubkey_bytes = hex::decode(public_key_hex).ok()?;
        if pubkey_bytes.len() != 32 {
            return Some(false);
        }

        let sig_array: [u8; 64] = sig_bytes.try_into().ok()?;
        let pubkey_array: [u8; 32] = pubkey_bytes.try_into().ok()?;

        let signature = Signature::from_bytes(&sig_array);
        let verifying_key = VerifyingKey::from_bytes(&pubkey_array).ok()?;

        Some(verifying_key.verify(message, &signature).is_ok())
    };

    verify_inner().unwrap_or(false)
}
