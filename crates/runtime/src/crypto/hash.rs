//! SHA-256, HMAC-SHA256, and timing-safe string comparison.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use hmac::digest::KeyInit;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

type HmacSha256 = Hmac<Sha256>;

/// Compute SHA-256 hash of a string
///
/// Stack effect: ( String -- String )
///
/// Returns the hash as a lowercase hex string (64 characters).
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_sha256(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "sha256: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let mut hasher = Sha256::new();
            hasher.update(s.as_bytes());
            let result = hasher.finalize();
            let hex_digest = hex::encode(result);
            unsafe { push(stack, Value::String(global_string(hex_digest))) }
        }
        _ => panic!("sha256: expected String on stack, got {:?}", value),
    }
}

/// Compute HMAC-SHA256 of a message with a key
///
/// Stack effect: ( message key -- String )
///
/// Returns the signature as a lowercase hex string (64 characters).
/// Used for webhook verification, JWT signing, API authentication.
///
/// # Safety
/// Stack must have two String values on top (message, then key)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_hmac_sha256(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "hmac-sha256: stack is empty");

    let (stack, key_value) = unsafe { pop(stack) };
    let (stack, msg_value) = unsafe { pop(stack) };

    match (msg_value, key_value) {
        (Value::String(msg), Value::String(key)) => {
            let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key.as_bytes())
                .expect("HMAC can take any key");
            mac.update(msg.as_bytes());
            let result = mac.finalize();
            let hex_sig = hex::encode(result.into_bytes());
            unsafe { push(stack, Value::String(global_string(hex_sig))) }
        }
        (msg, key) => panic!(
            "hmac-sha256: expected (String, String) on stack, got ({:?}, {:?})",
            msg, key
        ),
    }
}

/// Timing-safe string comparison
///
/// Stack effect: ( String String -- Bool )
///
/// Compares two strings in constant time to prevent timing attacks.
/// Essential for comparing signatures, hashes, tokens, etc.
///
/// Uses the `subtle` crate for cryptographically secure constant-time comparison.
/// This prevents timing side-channel attacks where an attacker could deduce
/// secret values by measuring comparison duration.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_constant_time_eq(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "constant-time-eq: stack is empty");

    let (stack, b_value) = unsafe { pop(stack) };
    let (stack, a_value) = unsafe { pop(stack) };

    match (a_value, b_value) {
        (Value::String(a), Value::String(b)) => {
            let a_bytes = a.as_bytes();
            let b_bytes = b.as_bytes();

            // Use subtle crate for truly constant-time comparison
            // This handles different-length strings correctly without timing leaks
            let eq = a_bytes.ct_eq(b_bytes);

            unsafe { push(stack, Value::Bool(bool::from(eq))) }
        }
        (a, b) => panic!(
            "constant-time-eq: expected (String, String) on stack, got ({:?}, {:?})",
            a, b
        ),
    }
}
