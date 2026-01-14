//! Compression operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//! Uses flate2 for gzip and zstd for Zstandard compression.
//!
//! Compressed data is returned as base64-encoded strings for easy
//! storage and transmission in string-based contexts.
//!
//! # API
//!
//! ```seq
//! # Gzip compression (base64-encoded output)
//! "hello world" compress.gzip           # ( String -- String )
//! compressed compress.gunzip            # ( String -- String Bool )
//!
//! # Gzip with compression level (1-9, higher = smaller but slower)
//! "hello world" 9 compress.gzip-level   # ( String Int -- String )
//!
//! # Zstd compression (faster, better ratios)
//! "hello world" compress.zstd           # ( String -- String )
//! compressed compress.unzstd            # ( String -- String Bool )
//!
//! # Zstd with compression level (1-22, default is 3)
//! "hello world" 19 compress.zstd-level  # ( String Int -- String )
//! ```

use base64::{Engine, engine::general_purpose::STANDARD};
use flate2::Compression;
use flate2::read::{GzDecoder, GzEncoder};
use seq_core::seqstring::global_string;
use seq_core::stack::{Stack, pop, push};
use seq_core::value::Value;
use std::io::Read;

/// Compress data using gzip with default compression level (6)
///
/// Stack effect: ( String -- String )
///
/// Returns base64-encoded compressed data.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gzip(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.gzip: stack is null");

    let (stack, data_val) = unsafe { pop(stack) };

    match data_val {
        Value::String(data) => {
            let compressed = gzip_compress(data.as_str().as_bytes(), Compression::default());
            let encoded = STANDARD.encode(&compressed);
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("compress.gzip: expected String on stack"),
    }
}

/// Compress data using gzip with specified compression level
///
/// Stack effect: ( String Int -- String )
///
/// Level should be 1-9 (1=fastest, 9=best compression).
/// Returns base64-encoded compressed data.
///
/// # Safety
/// Stack must have Int and String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gzip_level(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.gzip-level: stack is null");

    let (stack, level_val) = unsafe { pop(stack) };
    let (stack, data_val) = unsafe { pop(stack) };

    match (data_val, level_val) {
        (Value::String(data), Value::Int(level)) => {
            let level = level.clamp(1, 9) as u32;
            let compressed = gzip_compress(data.as_str().as_bytes(), Compression::new(level));
            let encoded = STANDARD.encode(&compressed);
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("compress.gzip-level: expected String and Int on stack"),
    }
}

/// Decompress gzip data
///
/// Stack effect: ( String -- String Bool )
///
/// Input should be base64-encoded gzip data.
/// Returns decompressed string and success flag.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gunzip(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.gunzip: stack is null");

    let (stack, data_val) = unsafe { pop(stack) };

    match data_val {
        Value::String(data) => {
            // Decode base64
            let decoded = match STANDARD.decode(data.as_str()) {
                Ok(d) => d,
                Err(_) => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    return unsafe { push(stack, Value::Bool(false)) };
                }
            };

            // Decompress
            match gzip_decompress(&decoded) {
                Some(decompressed) => {
                    let stack = unsafe { push(stack, Value::String(global_string(decompressed))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                None => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("compress.gunzip: expected String on stack"),
    }
}

/// Compress data using zstd with default compression level (3)
///
/// Stack effect: ( String -- String )
///
/// Returns base64-encoded compressed data.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_zstd(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.zstd: stack is null");

    let (stack, data_val) = unsafe { pop(stack) };

    match data_val {
        Value::String(data) => {
            let compressed = zstd::encode_all(data.as_str().as_bytes(), 3).unwrap_or_default();
            let encoded = STANDARD.encode(&compressed);
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("compress.zstd: expected String on stack"),
    }
}

/// Compress data using zstd with specified compression level
///
/// Stack effect: ( String Int -- String )
///
/// Level should be 1-22 (higher = better compression but slower).
/// Returns base64-encoded compressed data.
///
/// # Safety
/// Stack must have Int and String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_zstd_level(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.zstd-level: stack is null");

    let (stack, level_val) = unsafe { pop(stack) };
    let (stack, data_val) = unsafe { pop(stack) };

    match (data_val, level_val) {
        (Value::String(data), Value::Int(level)) => {
            let level = level.clamp(1, 22) as i32;
            let compressed = zstd::encode_all(data.as_str().as_bytes(), level).unwrap_or_default();
            let encoded = STANDARD.encode(&compressed);
            unsafe { push(stack, Value::String(global_string(encoded))) }
        }
        _ => panic!("compress.zstd-level: expected String and Int on stack"),
    }
}

/// Decompress zstd data
///
/// Stack effect: ( String -- String Bool )
///
/// Input should be base64-encoded zstd data.
/// Returns decompressed string and success flag.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_unzstd(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "compress.unzstd: stack is null");

    let (stack, data_val) = unsafe { pop(stack) };

    match data_val {
        Value::String(data) => {
            // Decode base64
            let decoded = match STANDARD.decode(data.as_str()) {
                Ok(d) => d,
                Err(_) => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    return unsafe { push(stack, Value::Bool(false)) };
                }
            };

            // Decompress
            match zstd::decode_all(decoded.as_slice()) {
                Ok(decompressed) => match String::from_utf8(decompressed) {
                    Ok(s) => {
                        let stack = unsafe { push(stack, Value::String(global_string(s))) };
                        unsafe { push(stack, Value::Bool(true)) }
                    }
                    Err(_) => {
                        let stack =
                            unsafe { push(stack, Value::String(global_string(String::new()))) };
                        unsafe { push(stack, Value::Bool(false)) }
                    }
                },
                Err(_) => {
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("compress.unzstd: expected String on stack"),
    }
}

// Helper functions

fn gzip_compress(data: &[u8], level: Compression) -> Vec<u8> {
    let mut encoder = GzEncoder::new(data, level);
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed).unwrap_or(0);
    compressed
}

fn gzip_decompress(data: &[u8]) -> Option<String> {
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = String::new();
    match decoder.read_to_string(&mut decompressed) {
        Ok(_) => Some(decompressed),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seq_core::stack::alloc_stack;

    #[test]
    fn test_gzip_roundtrip() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };

        // Compress
        let stack = unsafe { patch_seq_compress_gzip(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_gunzip(stack) };

        // Check success flag
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_gzip_level() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };
        let stack = unsafe { push(stack, Value::Int(9)) };

        // Compress with max level
        let stack = unsafe { patch_seq_compress_gzip_level(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_gunzip(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_zstd_roundtrip() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };

        // Compress
        let stack = unsafe { patch_seq_compress_zstd(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_unzstd(stack) };

        // Check success flag
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_zstd_level() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };
        let stack = unsafe { push(stack, Value::Int(19)) };

        // Compress with high level
        let stack = unsafe { patch_seq_compress_zstd_level(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_unzstd(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "hello world");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_gunzip_invalid_base64() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("not valid base64!!!".to_string())),
            )
        };

        let stack = unsafe { patch_seq_compress_gunzip(stack) };
        let (_, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(false));
    }

    #[test]
    fn test_gunzip_invalid_gzip() {
        let stack = alloc_stack();
        // Valid base64 but not gzip data
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("aGVsbG8gd29ybGQ=".to_string())),
            )
        };

        let stack = unsafe { patch_seq_compress_gunzip(stack) };
        let (_, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(false));
    }

    #[test]
    fn test_unzstd_invalid() {
        let stack = alloc_stack();
        // Valid base64 but not zstd data
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("aGVsbG8gd29ybGQ=".to_string())),
            )
        };

        let stack = unsafe { patch_seq_compress_unzstd(stack) };
        let (_, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(false));
    }

    #[test]
    fn test_empty_string() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };

        // Compress empty string
        let stack = unsafe { patch_seq_compress_gzip(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_gunzip(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "");
        } else {
            panic!("expected string");
        }
    }

    #[test]
    fn test_large_data() {
        let stack = alloc_stack();
        let large_data = "x".repeat(10000);
        let stack = unsafe { push(stack, Value::String(global_string(large_data.clone()))) };

        // Compress
        let stack = unsafe { patch_seq_compress_zstd(stack) };

        // Decompress
        let stack = unsafe { patch_seq_compress_unzstd(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));

        let (_, result) = unsafe { pop(stack) };
        if let Value::String(s) = result {
            assert_eq!(s.as_str(), large_data);
        } else {
            panic!("expected string");
        }
    }
}
