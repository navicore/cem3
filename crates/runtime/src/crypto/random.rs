//! Cryptographic randomness: `random_bytes`, `uuid4`, `random_int`.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

use rand::{Rng, rng};
use uuid::Uuid;

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
            rng().fill_bytes(&mut bytes);
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

/// Generate a cryptographically secure random integer in a range
///
/// Stack effect: ( min max -- Int )
///
/// Returns a uniform random integer in the range [min, max).
/// Uses rejection sampling to avoid modulo bias.
///
/// # Edge Cases
/// - If min >= max, returns min
/// - Uses the same CSPRNG as crypto.random-bytes
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_random_int(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "random-int: stack is empty");

    let (stack, max_val) = unsafe { pop(stack) };
    let (stack, min_val) = unsafe { pop(stack) };

    match (min_val, max_val) {
        (Value::Int(min), Value::Int(max)) => {
            let result = if min >= max {
                min // Edge case: return min if range is empty or invalid
            } else {
                random_int_range(min, max)
            };
            unsafe { push(stack, Value::Int(result)) }
        }
        (min, max) => panic!(
            "random-int: expected (Int, Int) on stack, got ({:?}, {:?})",
            min, max
        ),
    }
}

/// Generate a uniform random integer in [min, max) using rejection sampling
///
/// This avoids modulo bias by rejecting values that would cause uneven distribution.
fn random_int_range(min: i64, max: i64) -> i64 {
    // Use wrapping subtraction in unsigned space to handle full i64 range
    // without overflow (e.g., min=i64::MIN, max=i64::MAX would overflow in signed)
    let range = (max as u64).wrapping_sub(min as u64);
    if range == 0 {
        return min;
    }

    // Use rejection sampling to get unbiased result
    // For ranges that are powers of 2, no rejection needed
    // For other ranges, we reject values >= (u64::MAX - (u64::MAX % range))
    // to ensure uniform distribution
    let threshold = u64::MAX - (u64::MAX % range);

    loop {
        // Generate random u64 using fill_bytes (same CSPRNG as random_bytes)
        let mut bytes = [0u8; 8];
        rng().fill_bytes(&mut bytes);
        let val = u64::from_le_bytes(bytes);

        if val < threshold {
            // Add offset to min using unsigned arithmetic to avoid overflow
            // when min is negative and offset is large
            let result = (min as u64).wrapping_add(val % range);
            return result as i64;
        }
        // Rejection: try again (very rare, < 1 in 2^63 for most ranges)
    }
}
