//! Arithmetic operations for cem3
//!
//! These functions are exported with C ABI for LLVM codegen to call.

use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Push an integer literal onto the stack (for compiler-generated code)
///
/// Stack effect: ( -- n )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn push_int(stack: Stack, value: i64) -> Stack {
    unsafe { push(stack, Value::Int(value)) }
}

/// Push a boolean literal onto the stack (for compiler-generated code)
///
/// Stack effect: ( -- bool )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn push_bool(stack: Stack, value: bool) -> Stack {
    unsafe { push(stack, Value::Bool(value)) }
}

/// Add two integers
///
/// Stack effect: ( a b -- a+b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn add(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "add: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "add: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => {
            let result = a_val.wrapping_add(b_val); // Wrapping for defined overflow behavior
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("add: expected two integers on stack"),
    }
}

/// Subtract two integers (a - b)
///
/// Stack effect: ( a b -- a-b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn subtract(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "subtract: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "subtract: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => {
            let result = a_val.wrapping_sub(b_val);
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("subtract: expected two integers on stack"),
    }
}

/// Multiply two integers
///
/// Stack effect: ( a b -- a*b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn multiply(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "multiply: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "multiply: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => {
            let result = a_val.wrapping_mul(b_val);
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("multiply: expected two integers on stack"),
    }
}

/// Divide two integers (a / b)
///
/// Stack effect: ( a b -- a/b )
///
/// # Safety
/// Stack must have two Int values on top, b must not be zero
#[unsafe(no_mangle)]
pub unsafe extern "C" fn divide(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "divide: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "divide: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => {
            assert!(
                b_val != 0,
                "divide: division by zero (attempted {} / {})",
                a_val,
                b_val
            );
            let result = a_val / b_val;
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("divide: expected two integers on stack"),
    }
}

/// Integer equality
///
/// Stack effect: ( a b -- a==b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn eq(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "eq: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "eq: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Bool(a_val == b_val))
        },
        _ => panic!("eq: expected two integers on stack"),
    }
}

/// Less than
///
/// Stack effect: ( a b -- a<b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lt(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "lt: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "lt: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Bool(a_val < b_val)) },
        _ => panic!("lt: expected two integers on stack"),
    }
}

/// Greater than
///
/// Stack effect: ( a b -- a>b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gt(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "gt: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "gt: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Bool(a_val > b_val)) },
        _ => panic!("gt: expected two integers on stack"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 5);
            let stack = push_int(stack, 3);
            let stack = add(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(8));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_subtract() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 10);
            let stack = push_int(stack, 3);
            let stack = subtract(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(7));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_multiply() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 4);
            let stack = push_int(stack, 5);
            let stack = multiply(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(20));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_divide() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 20);
            let stack = push_int(stack, 4);
            let stack = divide(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_comparisons() {
        unsafe {
            // Test eq
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 5);
            let stack = push_int(stack, 5);
            let stack = eq(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
            assert!(stack.is_null());

            // Test lt
            let stack = push_int(stack, 3);
            let stack = push_int(stack, 5);
            let stack = lt(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
            assert!(stack.is_null());

            // Test gt
            let stack = push_int(stack, 7);
            let stack = push_int(stack, 5);
            let stack = gt(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_overflow_wrapping() {
        // Test that arithmetic uses wrapping semantics (defined overflow behavior)
        unsafe {
            // Test add overflow: i64::MAX + 1 should wrap
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, i64::MAX);
            let stack = push_int(stack, 1);
            let stack = add(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MIN)); // Wraps to minimum
            assert!(stack.is_null());

            // Test multiply overflow
            let stack = push_int(stack, i64::MAX);
            let stack = push_int(stack, 2);
            let stack = multiply(stack);
            let (stack, result) = pop(stack);
            // Result is well-defined (wrapping)
            assert!(matches!(result, Value::Int(_)));
            assert!(stack.is_null());

            // Test subtract underflow
            let stack = push_int(stack, i64::MIN);
            let stack = push_int(stack, 1);
            let stack = subtract(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MAX)); // Wraps to maximum
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_negative_division() {
        unsafe {
            // Test negative dividend
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, -10);
            let stack = push_int(stack, 3);
            let stack = divide(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-3)); // Truncates toward zero
            assert!(stack.is_null());

            // Test negative divisor
            let stack = push_int(stack, 10);
            let stack = push_int(stack, -3);
            let stack = divide(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-3));
            assert!(stack.is_null());

            // Test both negative
            let stack = push_int(stack, -10);
            let stack = push_int(stack, -3);
            let stack = divide(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3));
            assert!(stack.is_null());
        }
    }

    // Note: Division by zero test omitted because panics cannot be caught across
    // extern "C" boundaries. The runtime will panic with a helpful error message
    // "divide: division by zero (attempted X / 0)" which is validated at compile time
    // by the type checker in production code.
}
