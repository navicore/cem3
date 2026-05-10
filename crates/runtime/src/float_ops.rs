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
// Comparison Operations (return Bool)
// =============================================================================

/// Float equality: ( Float Float -- Bool )
///
/// **Warning:** Direct float equality can be surprising due to IEEE 754
/// rounding. For example, `0.1 0.2 f.add 0.3 f.=` may return false.
/// Consider using epsilon-based comparison for tolerances.
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_eq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x == y)) },
        _ => panic!("f.=: expected two Floats on stack"),
    }
}

/// Float less than: ( Float Float -- Bool )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_lt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x < y)) },
        _ => panic!("f.<: expected two Floats on stack"),
    }
}

/// Float greater than: ( Float Float -- Bool )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_gt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.>") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x > y)) },
        _ => panic!("f.>: expected two Floats on stack"),
    }
}

/// Float less than or equal: ( Float Float -- Bool )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_lte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x <= y)) },
        _ => panic!("f.<=: expected two Floats on stack"),
    }
}

/// Float greater than or equal: ( Float Float -- Bool )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_gte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.>=") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x >= y)) },
        _ => panic!("f.>=: expected two Floats on stack"),
    }
}

/// Float not equal: ( Float Float -- Bool )
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_neq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.<>") };
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => unsafe { push(rest, Value::Bool(x != y)) },
        _ => panic!("f.<>: expected two Floats on stack"),
    }
}

// =============================================================================
// Math Functions
// =============================================================================

// Helper macro: emit a unary `f.<name>` shim that maps Float -> Float via an
// `f64` method. Keeps the runtime entries one-liners since they all share the
// exact same shape (pop, match Float, push f(x)).
macro_rules! f_unary {
    ($fn_name:ident, $word:literal, $method:ident) => {
        /// Unary float operation. Bad inputs propagate NaN/Infinity per IEEE 754.
        ///
        /// # Safety
        /// Stack must have a Float value on top
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $fn_name(stack: Stack) -> Stack {
            assert!(!stack.is_null(), concat!($word, ": stack is empty"));
            let (rest, val) = unsafe { pop(stack) };
            match val {
                Value::Float(x) => unsafe { push(rest, Value::Float(x.$method())) },
                _ => panic!(concat!($word, ": expected Float on stack")),
            }
        }
    };
}

// Helper macro: emit a zero-arg `f.<name>` shim that pushes a constant.
macro_rules! f_const {
    ($fn_name:ident, $value:expr) => {
        /// Push a float constant onto the stack.
        ///
        /// # Safety
        /// Stack pointer must be valid or null
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $fn_name(stack: Stack) -> Stack {
            unsafe { push(stack, Value::Float($value)) }
        }
    };
}

// Roots / powers
f_unary!(patch_seq_f_sqrt, "f.sqrt", sqrt);
f_unary!(patch_seq_f_cbrt, "f.cbrt", cbrt);

/// Power: ( base exp -- result )
///
/// Bad inputs propagate NaN/Infinity per IEEE 754.
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_pow(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.pow") };
    match (a, b) {
        (Value::Float(base), Value::Float(exp)) => unsafe {
            push(rest, Value::Float(base.powf(exp)))
        },
        _ => panic!("f.pow: expected two Floats on stack"),
    }
}

// Exponential / logarithmic
f_unary!(patch_seq_f_exp, "f.exp", exp);
f_unary!(patch_seq_f_ln, "f.ln", ln);
f_unary!(patch_seq_f_log10, "f.log10", log10);
f_unary!(patch_seq_f_log2, "f.log2", log2);

// Trigonometric
f_unary!(patch_seq_f_sin, "f.sin", sin);
f_unary!(patch_seq_f_cos, "f.cos", cos);
f_unary!(patch_seq_f_tan, "f.tan", tan);
f_unary!(patch_seq_f_asin, "f.asin", asin);
f_unary!(patch_seq_f_acos, "f.acos", acos);
f_unary!(patch_seq_f_atan, "f.atan", atan);

/// Two-argument arctangent: ( y x -- result )
///
/// Returns the angle in radians of (x, y) from the positive x-axis.
/// Argument order matches C/Rust/JS.
///
/// # Safety
/// Stack must have two Float values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_atan2(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "f.atan2") };
    match (a, b) {
        (Value::Float(y), Value::Float(x)) => unsafe { push(rest, Value::Float(y.atan2(x))) },
        _ => panic!("f.atan2: expected two Floats on stack"),
    }
}

// Rounding
f_unary!(patch_seq_f_floor, "f.floor", floor);
f_unary!(patch_seq_f_ceil, "f.ceil", ceil);
f_unary!(patch_seq_f_round, "f.round", round_ties_even);
f_unary!(patch_seq_f_trunc, "f.trunc", trunc);

// Constants
f_const!(patch_seq_f_pi, std::f64::consts::PI);
f_const!(patch_seq_f_e, std::f64::consts::E);
f_const!(patch_seq_f_tau, std::f64::consts::TAU);

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
/// Truncates toward zero. Values outside 63-bit signed range are clamped:
/// - Values >= 2^62-1 become 2^62-1 (4611686018427387903)
/// - Values <= -(2^62) become -(2^62) (-4611686018427387904)
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
            // 63-bit signed integer range: -(2^62) to (2^62 - 1)
            const INT63_MAX: i64 = (1i64 << 62) - 1;
            const INT63_MIN: i64 = -(1i64 << 62);
            let i = if f.is_nan() {
                0
            } else if f >= INT63_MAX as f64 {
                INT63_MAX
            } else if f <= INT63_MIN as f64 {
                INT63_MIN
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
        Value::String(s) => match s.as_str_or_empty().parse::<f64>() {
            Ok(f) => {
                let stack = unsafe { push(stack, Value::Float(f)) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            Err(_) => {
                let stack = unsafe { push(stack, Value::Float(0.0)) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("string->float: expected String on stack"),
    }
}

// =============================================================================
// Public re-exports with short names
// =============================================================================

pub use patch_seq_f_acos as f_acos;
pub use patch_seq_f_add as f_add;
pub use patch_seq_f_asin as f_asin;
pub use patch_seq_f_atan as f_atan;
pub use patch_seq_f_atan2 as f_atan2;
pub use patch_seq_f_cbrt as f_cbrt;
pub use patch_seq_f_ceil as f_ceil;
pub use patch_seq_f_cos as f_cos;
pub use patch_seq_f_divide as f_divide;
pub use patch_seq_f_e as f_e;
pub use patch_seq_f_eq as f_eq;
pub use patch_seq_f_exp as f_exp;
pub use patch_seq_f_floor as f_floor;
pub use patch_seq_f_gt as f_gt;
pub use patch_seq_f_gte as f_gte;
pub use patch_seq_f_ln as f_ln;
pub use patch_seq_f_log2 as f_log2;
pub use patch_seq_f_log10 as f_log10;
pub use patch_seq_f_lt as f_lt;
pub use patch_seq_f_lte as f_lte;
pub use patch_seq_f_multiply as f_multiply;
pub use patch_seq_f_neq as f_neq;
pub use patch_seq_f_pi as f_pi;
pub use patch_seq_f_pow as f_pow;
pub use patch_seq_f_round as f_round;
pub use patch_seq_f_sin as f_sin;
pub use patch_seq_f_sqrt as f_sqrt;
pub use patch_seq_f_subtract as f_subtract;
pub use patch_seq_f_tan as f_tan;
pub use patch_seq_f_tau as f_tau;
pub use patch_seq_f_trunc as f_trunc;
pub use patch_seq_float_to_int as float_to_int;
pub use patch_seq_float_to_string as float_to_string;
pub use patch_seq_int_to_float as int_to_float;
pub use patch_seq_push_float as push_float;
pub use patch_seq_string_to_float as string_to_float;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests;
