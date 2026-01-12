//! Cryptographic operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # API
//!
//! ```seq
//! # SHA-256 hashing
//! "hello" crypto.sha256                    # ( String -- String ) hex digest
//!
//! # HMAC-SHA256 for webhook verification
//! "message" "secret" crypto.hmac-sha256    # ( String String -- String ) hex signature
//!
//! # Timing-safe comparison
//! sig1 sig2 crypto.constant-time-eq        # ( String String -- Bool )
//!
//! # Secure random bytes
//! 32 crypto.random-bytes                   # ( Int -- String ) hex-encoded random bytes
//!
//! # UUID v4
//! crypto.uuid4                             # ( -- String ) "550e8400-e29b-41d4-a716-446655440000"
//! ```

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

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
            hasher.update(s.as_str().as_bytes());
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
            let mut mac =
                HmacSha256::new_from_slice(key.as_str().as_bytes()).expect("HMAC can take any key");
            mac.update(msg.as_str().as_bytes());
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
            let a_bytes = a.as_str().as_bytes();
            let b_bytes = b.as_str().as_bytes();

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

/// Generate cryptographically secure random bytes
///
/// Stack effect: ( Int -- String )
///
/// Returns the random bytes as a lowercase hex string (2 chars per byte).
/// Uses the operating system's secure random number generator.
///
/// # Limits
/// - Maximum: 1024 bytes (to prevent memory exhaustion)
/// - Common use cases: 16-32 bytes for tokens/nonces, 32-64 bytes for keys
///
/// # Safety
/// Stack must have an Int value on top (number of bytes to generate)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_random_bytes(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "random-bytes: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::Int(n) => {
            if n < 0 {
                panic!("random-bytes: byte count must be non-negative, got {}", n);
            }
            if n > 1024 {
                panic!("random-bytes: byte count too large (max 1024), got {}", n);
            }

            let mut bytes = vec![0u8; n as usize];
            rand::thread_rng().fill_bytes(&mut bytes);
            let hex_str = hex::encode(&bytes);
            unsafe { push(stack, Value::String(global_string(hex_str))) }
        }
        _ => panic!("random-bytes: expected Int on stack, got {:?}", value),
    }
}

/// Generate a UUID v4 (random)
///
/// Stack effect: ( -- String )
///
/// Returns a UUID in standard format: "550e8400-e29b-41d4-a716-446655440000"
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_uuid4(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "uuid4: stack is empty");

    let uuid = Uuid::new_v4();
    unsafe { push(stack, Value::String(global_string(uuid.to_string()))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::pop;

    #[test]
    fn test_sha256() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = patch_seq_sha256(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // SHA-256 of "hello"
                    assert_eq!(
                        s.as_str(),
                        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
                    );
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_sha256_empty() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string(String::new())));
            let stack = patch_seq_sha256(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // SHA-256 of empty string
                    assert_eq!(
                        s.as_str(),
                        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    );
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_hmac_sha256() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("message".to_string())));
            let stack = push(stack, Value::String(global_string("secret".to_string())));
            let stack = patch_seq_hmac_sha256(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // HMAC-SHA256("message", "secret")
                    assert_eq!(
                        s.as_str(),
                        "8b5f48702995c1598c573db1e21866a9b825d4a794d169d7060a03605796360b"
                    );
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_constant_time_eq_equal() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = patch_seq_constant_time_eq(stack);
            let (_, value) = pop(stack);

            match value {
                Value::Bool(b) => assert!(b),
                _ => panic!("Expected Bool"),
            }
        }
    }

    #[test]
    fn test_constant_time_eq_different() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = push(stack, Value::String(global_string("world".to_string())));
            let stack = patch_seq_constant_time_eq(stack);
            let (_, value) = pop(stack);

            match value {
                Value::Bool(b) => assert!(!b),
                _ => panic!("Expected Bool"),
            }
        }
    }

    #[test]
    fn test_constant_time_eq_different_lengths() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = push(stack, Value::String(global_string("hi".to_string())));
            let stack = patch_seq_constant_time_eq(stack);
            let (_, value) = pop(stack);

            match value {
                Value::Bool(b) => assert!(!b),
                _ => panic!("Expected Bool"),
            }
        }
    }

    #[test]
    fn test_random_bytes() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(16));
            let stack = patch_seq_random_bytes(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // 16 bytes = 32 hex chars
                    assert_eq!(s.as_str().len(), 32);
                    // Should be valid hex
                    assert!(hex::decode(s.as_str()).is_ok());
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_random_bytes_zero() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(0));
            let stack = patch_seq_random_bytes(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    assert_eq!(s.as_str(), "");
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_uuid4() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = patch_seq_uuid4(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // UUID format: 8-4-4-4-12
                    assert_eq!(s.as_str().len(), 36);
                    assert_eq!(s.as_str().chars().filter(|c| *c == '-').count(), 4);
                    // Version 4 indicator at position 14
                    assert_eq!(s.as_str().chars().nth(14), Some('4'));
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_uuid4_unique() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = patch_seq_uuid4(stack);
            let (stack, value1) = pop(stack);
            let stack = patch_seq_uuid4(stack);
            let (_, value2) = pop(stack);

            match (value1, value2) {
                (Value::String(s1), Value::String(s2)) => {
                    assert_ne!(s1.as_str(), s2.as_str());
                }
                _ => panic!("Expected Strings"),
            }
        }
    }

    #[test]
    fn test_random_bytes_max_limit() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(1024)); // Max allowed
            let stack = patch_seq_random_bytes(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // 1024 bytes = 2048 hex chars
                    assert_eq!(s.as_str().len(), 2048);
                }
                _ => panic!("Expected String"),
            }
        }
    }
    // Note: Exceeding the 1024 byte limit causes a panic, which aborts in FFI context.
    // This is intentional - the limit prevents memory exhaustion attacks.
}
