//! Float operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//! All float operations use the `f.` prefix to distinguish from integer operations.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, pop_two, push};
use crate::value::Value;

// =============================================================================
// Push Float
// =============================================================================

/// Push a float value onto the stack
///
/// # Safety
/// Stack pointer must be valid or null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_float(stack: Stack, value: f64) -> Stack {
    unsafe { push(stack, Value::Float(value)) }
}

// =============================================================================
// Arithmetic Operations
// =============================================================================

/// Float addition: ( Float Float -- Float )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_add(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.add") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Float(x + y)) },
        _ => panic!("f.add: expected two Floats on stack"),
    }
}

/// Float subtraction: ( Float Float -- Float )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_subtract(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.subtract") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Float(x - y)) },
        _ => panic!("f.subtract: expected two Floats on stack"),
    }
}

/// Float multiplication: ( Float Float -- Float )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_multiply(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.multiply") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Float(x * y)) },
        _ => panic!("f.multiply: expected two Floats on stack"),
    }
}

/// Float division: ( Float Float -- Float )
///
/// Division by zero returns infinity (IEEE 754 behavior)
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_divide(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.divide") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Float(x / y)) },
        _ => panic!("f.divide: expected two Floats on stack"),
    }
}

// =============================================================================
// Comparison Operations (return Int 0 or 1)
// =============================================================================

/// Float equality: ( Float Float -- Int )
///
/// **Warning:** Direct float equality can be surprising due to IEEE 754
/// rounding. For example, `0.1 0.2 f.add 0.3 f.=` may return 0.
/// Consider using epsilon-based comparison for tolerances.
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_eq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x == y { 1 } else { 0 }))
        },
        _ => panic!("f.=: expected two Floats on stack"),
    }
}

/// Float less than: ( Float Float -- Int )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_lt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x < y { 1 } else { 0 }))
        },
        _ => panic!("f.<: expected two Floats on stack"),
    }
}

/// Float greater than: ( Float Float -- Int )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_gt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.>") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x > y { 1 } else { 0 }))
        },
        _ => panic!("f.>: expected two Floats on stack"),
    }
}

/// Float less than or equal: ( Float Float -- Int )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_lte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x <= y { 1 } else { 0 }))
        },
        _ => panic!("f.<=: expected two Floats on stack"),
    }
}

/// Float greater than or equal: ( Float Float -- Int )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_gte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.>=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x >= y { 1 } else { 0 }))
        },
        _ => panic!("f.>=: expected two Floats on stack"),
    }
}

/// Float not equal: ( Float Float -- Int )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_neq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<>") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe {
            push(rest, Value::Int(if x != y { 1 } else { 0 }))
        },
        _ => panic!("f.<>: expected two Floats on stack"),
    }
}

// =============================================================================
// Type Conversions
// =============================================================================

/// Convert Int to Float: ( Int -- Float )
///
/// # Safety
/// Stack must have an Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_to_float(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "int->float: stack is empty");
    let (stack, val) = unsafe { pop(stack) };

    match val {
        Value::Int(i) => unsafe { push(stack, Value::Float(i as f64)) },
        _ => panic!("int->float: expected Int on stack"),
    }
}

/// Convert Float to Int: ( Float -- Int )
///
/// Truncates toward zero. Values outside i64 range are clamped:
/// - Values >= i64::MAX become i64::MAX
/// - Values <= i64::MIN become i64::MIN
/// - NaN becomes 0
///
/// # Safety
/// Stack must have a Float value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_float_to_int(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "float->int: stack is empty");
    let (stack, val) = unsafe { pop(stack) };

    match val {
        Value::Float(f) => {
            // Clamp to i64 range to avoid undefined behavior
            let i = if f.is_nan() {
                0
            } else if f >= i64::MAX as f64 {
                i64::MAX
            } else if f <= i64::MIN as f64 {
                i64::MIN
            } else {
                f as i64
            };
            unsafe { push(stack, Value::Int(i)) }
        }
        _ => panic!("float->int: expected Float on stack"),
    }
}

/// Convert Float to String: ( Float -- String )
///
/// # Safety
/// Stack must have a Float value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_float_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "float->string: stack is empty");
    let (stack, val) = unsafe { pop(stack) };

    match val {
        Value::Float(f) => {
            let s = f.to_string();
            unsafe { push(stack, Value::String(global_string(s))) }
        }
        _ => panic!("float->string: expected Float on stack"),
    }
}

/// Convert String to Float: ( String -- Float Int )
/// Returns the parsed float and 1 on success, or 0.0 and 0 on failure
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_float(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string->float: stack is empty");
    let (stack, val) = unsafe { pop(stack) };

    match val {
        Value::String(s) => match s.as_str().parse::<f64>() {
            Ok(f) => {
                let stack = unsafe { push(stack, Value::Float(f)) };
                unsafe { push(stack, Value::Int(1)) }
            }
            Err(_) => {
                let stack = unsafe { push(stack, Value::Float(0.0)) };
                unsafe { push(stack, Value::Int(0)) }
            }
        },
        _ => panic!("string->float: expected String on stack"),
    }
}

// =============================================================================
// Public re-exports with short names
// =============================================================================

pub use patch_seq_f_add as f_add;
pub use patch_seq_f_divide as f_divide;
pub use patch_seq_f_eq as f_eq;
pub use patch_seq_f_gt as f_gt;
pub use patch_seq_f_gte as f_gte;
pub use patch_seq_f_lt as f_lt;
pub use patch_seq_f_lte as f_lte;
pub use patch_seq_f_multiply as f_multiply;
pub use patch_seq_f_neq as f_neq;
pub use patch_seq_f_subtract as f_subtract;
pub use patch_seq_float_to_int as float_to_int;
pub use patch_seq_float_to_string as float_to_string;
pub use patch_seq_int_to_float as int_to_float;
pub use patch_seq_push_float as push_float;
pub use patch_seq_string_to_float as string_to_float;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_float() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_float(stack, 3.5);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(3.5));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_add() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(1.5));
            let stack = push(stack, Value::Float(2.5));

            let stack = f_add(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(4.0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_subtract() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(5.0));
            let stack = push(stack, Value::Float(2.0));

            let stack = f_subtract(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(3.0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_multiply() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(3.0));
            let stack = push(stack, Value::Float(4.0));

            let stack = f_multiply(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(12.0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_divide() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(10.0));
            let stack = push(stack, Value::Float(4.0));

            let stack = f_divide(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(2.5));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_divide_by_zero() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(1.0));
            let stack = push(stack, Value::Float(0.0));

            let stack = f_divide(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Float(f) => assert!(f.is_infinite()),
                _ => panic!("Expected Float"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_eq_true() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(3.5));
            let stack = push(stack, Value::Float(3.5));

            let stack = f_eq(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_eq_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(3.5));
            let stack = push(stack, Value::Float(2.5));

            let stack = f_eq(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_lt() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(1.5));
            let stack = push(stack, Value::Float(2.5));

            let stack = f_lt(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 1.5 < 2.5
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_gt() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(2.5));
            let stack = push(stack, Value::Float(1.5));

            let stack = f_gt(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 2.5 > 1.5
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_lte() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(2.5));
            let stack = push(stack, Value::Float(2.5));

            let stack = f_lte(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 2.5 <= 2.5
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_gte() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(2.5));
            let stack = push(stack, Value::Float(2.5));

            let stack = f_gte(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 2.5 >= 2.5
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_f_neq() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(1.0));
            let stack = push(stack, Value::Float(2.0));

            let stack = f_neq(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 1.0 <> 2.0
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_int_to_float() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Int(42));

            let stack = int_to_float(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Float(42.0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(3.7));

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3)); // Truncates toward zero
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int_negative() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(-3.7));

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-3)); // Truncates toward zero
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int_overflow_positive() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(1e20)); // Much larger than i64::MAX

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MAX)); // Clamped to max
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int_overflow_negative() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(-1e20)); // Much smaller than i64::MIN

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MIN)); // Clamped to min
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int_nan() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(f64::NAN));

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0)); // NaN becomes 0
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_int_infinity() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(f64::INFINITY));

            let stack = float_to_int(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MAX)); // +Inf becomes MAX
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_string() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(3.5));

            let stack = float_to_string(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "3.5"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_float_to_string_whole_number() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(42.0));

            let stack = float_to_string(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "42"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_nan_propagation() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(f64::NAN));
            let stack = push(stack, Value::Float(1.0));

            let stack = f_add(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Float(f) => assert!(f.is_nan()),
                _ => panic!("Expected Float"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_infinity() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::Float(f64::INFINITY));
            let stack = push(stack, Value::Float(1.0));

            let stack = f_add(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Float(f) => assert!(f.is_infinite()),
                _ => panic!("Expected Float"),
            }
            assert!(stack.is_null());
        }
    }
}
