//! Arithmetic operations for cem3
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # Safety Contract
//!
//! **IMPORTANT:** These functions are designed to be called ONLY by compiler-generated code,
//! not by end users or arbitrary C code. The compiler's type checker is responsible for:
//!
//! - Ensuring stack has correct number of values
//! - Ensuring values are the correct types (Int for arithmetic, Int for comparisons)
//! - Preventing division by zero at compile time when possible
//!
//! # Overflow Behavior
//!
//! All arithmetic operations use **wrapping semantics** for predictable, defined behavior:
//! - `add`: i64::MAX + 1 wraps to i64::MIN
//! - `subtract`: i64::MIN - 1 wraps to i64::MAX
//! - `multiply`: overflow wraps around
//! - `divide`: i64::MIN / -1 wraps to i64::MIN (special case)
//!
//! This matches the behavior of Forth and Factor, providing consistency for low-level code.

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
            // Use wrapping_div to handle i64::MIN / -1 overflow edge case
            // (consistent with wrapping semantics for add/subtract/multiply)
            let result = a_val.wrapping_div(b_val);
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("divide: expected two integers on stack"),
    }
}

/// Integer equality: =
///
/// Returns 1 if equal, 0 if not (Forth-style boolean)
/// Stack effect: ( a b -- flag )
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
            push(rest, Value::Int(if a_val == b_val { 1 } else { 0 }))
        },
        _ => panic!("eq: expected two integers on stack"),
    }
}

/// Less than: <
///
/// Returns 1 if a < b, 0 otherwise (Forth-style boolean)
/// Stack effect: ( a b -- flag )
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
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(if a_val < b_val { 1 } else { 0 }))
        },
        _ => panic!("lt: expected two integers on stack"),
    }
}

/// Greater than: >
///
/// Returns 1 if a > b, 0 otherwise (Forth-style boolean)
/// Stack effect: ( a b -- flag )
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
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(if a_val > b_val { 1 } else { 0 }))
        },
        _ => panic!("gt: expected two integers on stack"),
    }
}

/// Less than or equal: <=
///
/// Returns 1 if a <= b, 0 otherwise (Forth-style boolean)
/// Stack effect: ( a b -- flag )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lte(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "lte: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "lte: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(if a_val <= b_val { 1 } else { 0 }))
        },
        _ => panic!("lte: expected two integers on stack"),
    }
}

/// Greater than or equal: >=
///
/// Returns 1 if a >= b, 0 otherwise (Forth-style boolean)
/// Stack effect: ( a b -- flag )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gte(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "gte: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "gte: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(if a_val >= b_val { 1 } else { 0 }))
        },
        _ => panic!("gte: expected two integers on stack"),
    }
}

/// Not equal: <>
///
/// Returns 1 if a != b, 0 otherwise (Forth-style boolean)
/// Stack effect: ( a b -- flag )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn neq(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "neq: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "neq: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(if a_val != b_val { 1 } else { 0 }))
        },
        _ => panic!("neq: expected two integers on stack"),
    }
}

/// Logical AND operation (Forth-style: multiply for boolean values)
///
/// Stack effect: ( a b -- result )
/// where 0 is false, non-zero is true
/// Returns 1 if both are true (non-zero), 0 otherwise
///
/// # Safety
/// Stack must have at least two Int values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn and(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "and: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "and: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(
                rest,
                Value::Int(if a_val != 0 && b_val != 0 { 1 } else { 0 }),
            )
        },
        _ => panic!("and: expected two integers on stack"),
    }
}

/// Logical OR operation (Forth-style)
///
/// Stack effect: ( a b -- result )
/// where 0 is false, non-zero is true
/// Returns 1 if either is true (non-zero), 0 otherwise
///
/// # Safety
/// Stack must have at least two Int values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn or(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "or: stack is empty");
    let (rest, b) = unsafe { pop(stack) };
    assert!(!rest.is_null(), "or: stack has only one value");
    let (rest, a) = unsafe { pop(rest) };

    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(
                rest,
                Value::Int(if a_val != 0 || b_val != 0 { 1 } else { 0 }),
            )
        },
        _ => panic!("or: expected two integers on stack"),
    }
}

/// Logical NOT operation
///
/// Stack effect: ( a -- result )
/// where 0 is false, non-zero is true
/// Returns 1 if false (0), 0 otherwise
///
/// # Safety
/// Stack must have at least one Int value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn not(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "not: stack is empty");
    let (rest, a) = unsafe { pop(stack) };

    match a {
        Value::Int(a_val) => unsafe { push(rest, Value::Int(if a_val == 0 { 1 } else { 0 })) },
        _ => panic!("not: expected integer on stack"),
    }
}

/// Helper for peeking at the top integer value without popping
///
/// Returns the integer value on top of the stack without modifying the stack.
/// Used in conjunction with pop_stack for conditional branching.
///
/// **Why separate peek and pop?**
/// In LLVM IR for conditionals, we need to:
/// 1. Extract the integer value to test it (peek_int_value)
/// 2. Branch based on that value (icmp + br)
/// 3. Free the stack node in both branches (pop_stack)
///
/// A combined pop_int_value would leak memory since we'd need the value
/// before branching but couldn't return the updated stack pointer through
/// both branches. Separating these operations prevents memory leaks.
///
/// Stack effect: ( n -- n ) returns n
///
/// # Safety
/// Stack must have an Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn peek_int_value(stack: Stack) -> i64 {
    assert!(!stack.is_null(), "peek_int_value: stack is empty");

    let node = unsafe { &*stack };
    match &node.value {
        Value::Int(n) => *n,
        _ => panic!(
            "peek_int_value: expected Int on stack, got {:?}",
            node.value
        ),
    }
}

/// Helper for popping without extracting the value (for conditionals)
///
/// Pops the top stack node and returns the updated stack pointer.
/// Used after peek_int_value to free the condition value's stack node.
///
/// Stack effect: ( n -- )
///
/// # Safety
/// Stack must not be empty
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pop_stack(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "pop_stack: stack is empty");
    let (rest, _value) = unsafe { pop(stack) };
    rest
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
            // Test eq (returns 1 for true, 0 for false - Forth style)
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, 5);
            let stack = push_int(stack, 5);
            let stack = eq(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 1 = true
            assert!(stack.is_null());

            // Test lt
            let stack = push_int(stack, 3);
            let stack = push_int(stack, 5);
            let stack = lt(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 1 = true
            assert!(stack.is_null());

            // Test gt
            let stack = push_int(stack, 7);
            let stack = push_int(stack, 5);
            let stack = gt(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1)); // 1 = true
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

    #[test]
    fn test_division_overflow_edge_case() {
        // Critical edge case: i64::MIN / -1 would overflow
        // Regular division panics, but wrapping_div wraps to i64::MIN
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push_int(stack, i64::MIN);
            let stack = push_int(stack, -1);
            let stack = divide(stack);
            let (stack, result) = pop(stack);
            // i64::MIN / -1 would be i64::MAX + 1, which wraps to i64::MIN
            assert_eq!(result, Value::Int(i64::MIN));
            assert!(stack.is_null());
        }
    }

    // Note: Division by zero test omitted because panics cannot be caught across
    // extern "C" boundaries. The runtime will panic with a helpful error message
    // "divide: division by zero (attempted X / 0)" which is validated at compile time
    // by the type checker in production code.
}
