//! Encoding operations for Seq (Base64, Hex)
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # API
//!
//! ```seq
//! # Base64 encoding/decoding
//! "hello" encoding.base64-encode     # ( String -- String ) "aGVsbG8="
//! "aGVsbG8=" encoding.base64-decode  # ( String -- String Bool )
//!
//! # URL-safe Base64 (for JWTs, URLs)
//! data encoding.base64url-encode     # ( String -- String )
//! encoded encoding.base64url-decode  # ( String -- String Bool )
//!
//! # Hex encoding/decoding
//! "hello" encoding.hex-encode        # ( String -- String ) "68656c6c6f"
//! "68656c6c6f" encoding.hex-decode   # ( String -- String Bool )
//! ```

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use base64::prelude::*;

/// Encode a string to Base64 (standard alphabet with padding)
///
/// Stack effect: ( String -- String )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_base64_encode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "base64-encode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let encoded = BASE64_STANDARD.encode(s.as_str().as_bytes());
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("base64-encode: expected String on stack, got {:?}", value),
    }
}

/// Decode a Base64 string (standard alphabet)
///
/// Stack effect: ( String -- String Bool )
///
/// Returns the decoded string and true on success, empty string and false on failure.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_base64_decode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "base64-decode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => match BASE64_STANDARD.decode(s.as_str().as_bytes()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(decoded) => {
                    let stack = unsafe { push(stack, Value::String(global_string(decoded))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                Err(_) => {
                    // Decoded bytes are not valid UTF-8
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            },
            Err(_) => {
                // Invalid Base64 input
                let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("base64-decode: expected String on stack, got {:?}", value),
    }
}

/// Encode a string to URL-safe Base64 (no padding)
///
/// Stack effect: ( String -- String )
///
/// Uses URL-safe alphabet (- and _ instead of + and /) with no padding.
/// Suitable for JWTs, URLs, and filenames.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_base64url_encode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "base64url-encode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let encoded = BASE64_URL_SAFE_NO_PAD.encode(s.as_str().as_bytes());
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!(
            "base64url-encode: expected String on stack, got {:?}",
            value
        ),
    }
}

/// Decode a URL-safe Base64 string (no padding expected)
///
/// Stack effect: ( String -- String Bool )
///
/// Returns the decoded string and true on success, empty string and false on failure.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_base64url_decode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "base64url-decode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => match BASE64_URL_SAFE_NO_PAD.decode(s.as_str().as_bytes()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(decoded) => {
                    let stack = unsafe { push(stack, Value::String(global_string(decoded))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                Err(_) => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            },
            Err(_) => {
                let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!(
            "base64url-decode: expected String on stack, got {:?}",
            value
        ),
    }
}

/// Encode a string to hexadecimal (lowercase)
///
/// Stack effect: ( String -- String )
///
/// Each byte becomes two hex characters.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_hex_encode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "hex-encode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let encoded = hex::encode(s.as_str().as_bytes());
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("hex-encode: expected String on stack, got {:?}", value),
    }
}

/// Decode a hexadecimal string
///
/// Stack effect: ( String -- String Bool )
///
/// Returns the decoded string and true on success, empty string and false on failure.
/// Accepts both uppercase and lowercase hex characters.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_hex_decode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "hex-decode: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => match hex::decode(s.as_str()) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(decoded) => {
                    let stack = unsafe { push(stack, Value::String(global_string(decoded))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                Err(_) => {
                    // Decoded bytes are not valid UTF-8
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            },
            Err(_) => {
                // Invalid hex input
                let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("hex-decode: expected String on stack, got {:?}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::pop;

    #[test]
    fn test_base64_encode() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = patch_seq_base64_encode(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => assert_eq!(s.as_str(), "aGVsbG8="),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_base64_decode_success() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("aGVsbG8=".to_string())));
            let stack = patch_seq_base64_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(true)) => assert_eq!(s.as_str(), "hello"),
                _ => panic!("Expected (String, true)"),
            }
        }
    }

    #[test]
    fn test_base64_decode_failure() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("not valid base64!!!".to_string())),
            );
            let stack = patch_seq_base64_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(false)) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected (empty String, false)"),
            }
        }
    }

    #[test]
    fn test_base64url_encode() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            // Use input that produces + and / in standard base64
            let stack = push(stack, Value::String(global_string("hello??".to_string())));
            let stack = patch_seq_base64url_encode(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => {
                    // Should not contain + or / or =
                    assert!(!s.as_str().contains('+'));
                    assert!(!s.as_str().contains('/'));
                    assert!(!s.as_str().contains('='));
                }
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_base64url_roundtrip() {
        unsafe {
            let original = "hello world! 123";
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string(original.to_string())));
            let stack = patch_seq_base64url_encode(stack);
            let stack = patch_seq_base64url_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(true)) => assert_eq!(s.as_str(), original),
                _ => panic!("Expected (String, true)"),
            }
        }
    }

    #[test]
    fn test_hex_encode() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_string())));
            let stack = patch_seq_hex_encode(stack);
            let (_, value) = pop(stack);

            match value {
                Value::String(s) => assert_eq!(s.as_str(), "68656c6c6f"),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_hex_decode_success() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("68656c6c6f".to_string())),
            );
            let stack = patch_seq_hex_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(true)) => assert_eq!(s.as_str(), "hello"),
                _ => panic!("Expected (String, true)"),
            }
        }
    }

    #[test]
    fn test_hex_decode_uppercase() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("68656C6C6F".to_string())),
            );
            let stack = patch_seq_hex_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(true)) => assert_eq!(s.as_str(), "hello"),
                _ => panic!("Expected (String, true)"),
            }
        }
    }

    #[test]
    fn test_hex_decode_failure() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("not hex!".to_string())));
            let stack = patch_seq_hex_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(false)) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected (empty String, false)"),
            }
        }
    }

    #[test]
    fn test_hex_roundtrip() {
        unsafe {
            let original = "Hello, World! 123";
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string(original.to_string())));
            let stack = patch_seq_hex_encode(stack);
            let stack = patch_seq_hex_decode(stack);

            let (stack, success) = pop(stack);
            let (_, decoded) = pop(stack);

            match (decoded, success) {
                (Value::String(s), Value::Bool(true)) => assert_eq!(s.as_str(), original),
                _ => panic!("Expected (String, true)"),
            }
        }
    }
}
