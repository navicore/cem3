//! AES-256-GCM authenticated encryption.

use crate::seqstring::{global_bytes, global_string};
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit as AesKeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand_core::{OsRng, RngCore};

use super::{AES_GCM_TAG_SIZE, AES_KEY_SIZE, AES_NONCE_SIZE};

/// Encrypt plaintext using AES-256-GCM
///
/// Stack effect: ( String String -- String Bool )
///
/// Arguments:
/// - plaintext: The string to encrypt
/// - key: Hex-encoded 32-byte key (64 hex characters)
///
/// Returns:
/// - ciphertext: base64(nonce || ciphertext || tag)
/// - success: Bool indicating success
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_aes_gcm_encrypt(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "crypto.aes-gcm-encrypt: stack is null");

    let (stack, key_val) = unsafe { pop(stack) };
    let (stack, plaintext_val) = unsafe { pop(stack) };

    match (plaintext_val, key_val) {
        (Value::String(plaintext), Value::String(key_hex)) => {
            // Plaintext is byte-clean — random bytes, file content,
            // pre-encoded structured data all encrypt correctly.
            // Key is text (hex) so we still go through `as_str_or_empty`.
            match aes_gcm_encrypt(plaintext.as_bytes(), key_hex.as_str_or_empty()) {
                Some(ciphertext) => {
                    let stack = unsafe { push(stack, Value::String(global_string(ciphertext))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                None => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("crypto.aes-gcm-encrypt: expected two Strings on stack"),
    }
}

/// Decrypt ciphertext using AES-256-GCM
///
/// Stack effect: ( String String -- String Bool )
///
/// Arguments:
/// - ciphertext: base64(nonce || ciphertext || tag)
/// - key: Hex-encoded 32-byte key (64 hex characters)
///
/// Returns:
/// - plaintext: The decrypted string
/// - success: Bool indicating success
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_crypto_aes_gcm_decrypt(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "crypto.aes-gcm-decrypt: stack is null");

    let (stack, key_val) = unsafe { pop(stack) };
    let (stack, ciphertext_val) = unsafe { pop(stack) };

    match (ciphertext_val, key_val) {
        (Value::String(ciphertext), Value::String(key_hex)) => {
            // Ciphertext is base64 (text). Plaintext bytes come back
            // raw — wrap them in a byte-clean SeqString so binary
            // payloads survive the round-trip.
            match aes_gcm_decrypt(ciphertext.as_str_or_empty(), key_hex.as_str_or_empty()) {
                Some(plaintext_bytes) => {
                    let stack =
                        unsafe { push(stack, Value::String(global_bytes(plaintext_bytes))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                None => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("crypto.aes-gcm-decrypt: expected two Strings on stack"),
    }
}

/// Encrypt arbitrary bytes with AES-256-GCM. The plaintext is treated
/// as bytes (binary or text), the key is hex-encoded, and the
/// returned ciphertext is base64-encoded (so always valid UTF-8 and
/// safe to store in any string-typed field).
pub(super) fn aes_gcm_encrypt(plaintext: &[u8], key_hex: &str) -> Option<String> {
    // Decode hex key
    let key_bytes = hex::decode(key_hex).ok()?;
    if key_bytes.len() != AES_KEY_SIZE {
        return None;
    }

    // Create cipher
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).ok()?;

    // Generate random nonce
    let mut nonce_bytes = [0u8; AES_NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::try_from(&nonce_bytes[..]).expect("nonce is fixed-length");

    // Encrypt
    let ciphertext = cipher.encrypt(&nonce, plaintext).ok()?;

    // Combine: nonce || ciphertext (tag is appended by aes-gcm)
    let mut combined = Vec::with_capacity(AES_NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Some(STANDARD.encode(&combined))
}

/// Decrypt an AES-256-GCM ciphertext (base64-encoded) with the given
/// hex-encoded key. Returns the recovered plaintext as raw bytes —
/// callers can wrap them in a byte-clean SeqString (binary plaintext
/// preserved) or validate as UTF-8 if they expect text.
pub(super) fn aes_gcm_decrypt(ciphertext_b64: &str, key_hex: &str) -> Option<Vec<u8>> {
    // Decode base64
    let combined = STANDARD.decode(ciphertext_b64).ok()?;
    if combined.len() < AES_NONCE_SIZE + AES_GCM_TAG_SIZE {
        // At minimum: nonce + tag
        return None;
    }

    // Decode hex key
    let key_bytes = hex::decode(key_hex).ok()?;
    if key_bytes.len() != AES_KEY_SIZE {
        return None;
    }

    // Split nonce and ciphertext
    let (nonce_bytes, ciphertext) = combined.split_at(AES_NONCE_SIZE);
    let nonce = Nonce::try_from(nonce_bytes).expect("nonce is fixed-length");

    // Create cipher and decrypt
    let cipher = Aes256Gcm::new_from_slice(&key_bytes).ok()?;
    cipher.decrypt(&nonce, ciphertext).ok()
}
