//! Bitwise operations: `band`, `bor`, `bxor`, `bnot`, `shl`, `shr`,
//! plus bit-counting intrinsics (`popcount`, `clz`, `ctz`) and
//! `int_bits` (the integer's bit width).
//!
//! Seq's `Int` is a 63-bit signed integer (the low bit of the tagged
//! stack encoding is the tag). All bitwise operations report results in
//! the 63-bit model, even though the underlying Rust storage is `i64`.
//! `shl`/`shr` results that fall outside the 63-bit range return 0
//! rather than silently truncating during retag.

use crate::stack::{Stack, pop, pop_two, push};
use crate::value::Value;

/// Minimum value representable in a 63-bit signed integer (-2^62).
const I63_MIN: i64 = -(1i64 << 62);
/// Maximum value representable in a 63-bit signed integer (2^62 - 1).
const I63_MAX: i64 = (1i64 << 62) - 1;
/// Bit mask covering the 63 value-bits of a Seq Int (everything except
/// the i64 sign-extension bit at position 63).
const I63_MASK: u64 = (1u64 << 63) - 1;

/// Whether `v` fits in the 63-bit signed range Seq's `Int` actually
/// represents.
#[inline]
fn fits_in_i63(v: i64) -> bool {
    (I63_MIN..=I63_MAX).contains(&v)
}

// ============================================================================
// Bitwise Operations
// ============================================================================

/// Bitwise AND
///
/// Stack effect: ( a b -- a&b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_band(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "band") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Int(a_val & b_val)) },
        _ => panic!("band: expected two integers on stack"),
    }
}

/// Bitwise OR
///
/// Stack effect: ( a b -- a|b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_bor(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "bor") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Int(a_val | b_val)) },
        _ => panic!("bor: expected two integers on stack"),
    }
}

/// Bitwise XOR
///
/// Stack effect: ( a b -- a^b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_bxor(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "bxor") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Int(a_val ^ b_val)) },
        _ => panic!("bxor: expected two integers on stack"),
    }
}

/// Bitwise NOT (one's complement)
///
/// Stack effect: ( a -- !a )
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_bnot(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "bnot: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(a_val) => unsafe { push(rest, Value::Int(!a_val)) },
        _ => panic!("bnot: expected integer on stack"),
    }
}

/// Shift left
///
/// Stack effect: ( value count -- result )
/// Shifts value left by count bits. Returns 0 for negative counts,
/// counts >= 64, or any result that falls outside the 63-bit Int
/// range. The 63-bit clamp matters because the tagged stack encoding
/// would otherwise silently lose bit 62 when retagging an out-of-range
/// `i64`.
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_shl(stack: Stack) -> Stack {
    let (rest, value, count) = unsafe { pop_two(stack, "shl") };
    match (value, count) {
        (Value::Int(v), Value::Int(c)) => {
            // Reject the count *before* casting to u32: the i64 range
            // includes counts > u32::MAX, which would otherwise truncate
            // into a small valid count and silently violate the contract.
            let result = if !(0..64).contains(&c) {
                0
            } else {
                let raw = v.checked_shl(c as u32).unwrap_or(0);
                if fits_in_i63(raw) { raw } else { 0 }
            };
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("shl: expected two integers on stack"),
    }
}

/// Logical shift right (zero-fill)
///
/// Stack effect: ( value count -- result )
/// Shifts value right by count bits, filling with zeros. Returns 0 for
/// negative counts, counts >= 64, or any result that falls outside the
/// 63-bit Int range. Logical-shifting a negative produces a u64 that
/// often exceeds `I63_MAX`; that case clamps to 0 rather than silently
/// truncating in the tagger.
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_shr(stack: Stack) -> Stack {
    let (rest, value, count) = unsafe { pop_two(stack, "shr") };
    match (value, count) {
        (Value::Int(v), Value::Int(c)) => {
            // Reject the count *before* casting to u32 — see `shl` for why.
            let result = if !(0..64).contains(&c) {
                0
            } else {
                let raw = (v as u64).checked_shr(c as u32).unwrap_or(0) as i64;
                if fits_in_i63(raw) { raw } else { 0 }
            };
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("shr: expected two integers on stack"),
    }
}

/// Population count (count number of 1 bits in the 63-bit Int).
///
/// Stack effect: ( n -- count )
///
/// Counts bits in the 63-bit two's-complement representation of `n`.
/// Negatives have bit 63 set in the underlying `i64` purely as sign
/// extension, so we mask it off before counting. `popcount(-1) = 63`.
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_popcount(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "popcount: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => {
            let count = ((v as u64) & I63_MASK).count_ones() as i64;
            unsafe { push(rest, Value::Int(count)) }
        }
        _ => panic!("popcount: expected integer on stack"),
    }
}

/// Count leading zeros, relative to the 63-bit Int width.
///
/// Stack effect: ( n -- count )
///
/// `clz(0) = 63`. The `i64::leading_zeros` count is one larger than the
/// 63-bit count for any non-negative value (the implicit sign-extension
/// bit), and one larger for `0` as well — `saturating_sub(1)` collapses
/// both cases. For negatives the i64 sign-extension bit is 1, so
/// `i64::leading_zeros` is already 0.
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_clz(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "clz: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => {
            let lz = (v.leading_zeros() as i64).saturating_sub(1);
            unsafe { push(rest, Value::Int(lz)) }
        }
        _ => panic!("clz: expected integer on stack"),
    }
}

/// Count trailing zeros, relative to the 63-bit Int width.
///
/// Stack effect: ( n -- count )
///
/// `ctz(0) = 63`. Any non-zero 63-bit value has at least one set bit at
/// position ≤ 62, so `i64::trailing_zeros` already produces a value in
/// `[0, 62]`; only `v == 0` needs the special case to avoid returning
/// 64.
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_ctz(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "ctz: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => {
            let tz = if v == 0 {
                63
            } else {
                v.trailing_zeros() as i64
            };
            unsafe { push(rest, Value::Int(tz)) }
        }
        _ => panic!("ctz: expected integer on stack"),
    }
}

/// Push the bit width of Int (63).
///
/// Stack effect: ( -- 63 )
///
/// Seq's `Int` is a 63-bit signed integer; the low bit of the tagged
/// stack encoding is the tag, leaving 63 bits for the value.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_bits(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(63)) }
}
