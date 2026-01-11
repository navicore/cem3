//! Arithmetic operations for Seq
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

use crate::error::set_runtime_error;
use crate::stack::{DISC_INT, Stack, peek_sv, pop, pop_two, push};
use crate::value::Value;

/// Push an integer literal onto the stack (for compiler-generated code)
///
/// Stack effect: ( -- n )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_int(stack: Stack, value: i64) -> Stack {
    unsafe { push(stack, Value::Int(value)) }
}

/// Push a boolean literal onto the stack (for compiler-generated code)
///
/// Stack effect: ( -- bool )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_bool(stack: Stack, value: bool) -> Stack {
    unsafe { push(stack, Value::Bool(value)) }
}

/// Add two integers
///
/// Stack effect: ( a b -- a+b )
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_add(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "add") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(a_val.wrapping_add(b_val)))
        },
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
pub unsafe extern "C" fn patch_seq_subtract(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "subtract") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(a_val.wrapping_sub(b_val)))
        },
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
pub unsafe extern "C" fn patch_seq_multiply(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "multiply") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Int(a_val.wrapping_mul(b_val)))
        },
        _ => panic!("multiply: expected two integers on stack"),
    }
}

/// Divide two integers (a / b)
///
/// Stack effect: ( a b -- a/b )
///
/// # Error Handling
/// - Division by zero: Sets runtime error and returns 0
/// - Type mismatch: Sets runtime error and returns 0
///
/// # Safety
/// Stack must have at least two values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_divide(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "divide") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(0)) => {
            // Division by zero - set error and return 0
            set_runtime_error(format!(
                "divide: division by zero (attempted {} / 0)",
                a_val
            ));
            unsafe { push(rest, Value::Int(0)) }
        }
        (Value::Int(a_val), Value::Int(b_val)) => {
            // Use wrapping_div to handle i64::MIN / -1 overflow edge case
            unsafe { push(rest, Value::Int(a_val.wrapping_div(b_val))) }
        }
        _ => {
            // Type error - should not happen with type-checked code
            set_runtime_error("divide: expected two integers on stack");
            unsafe { push(rest, Value::Int(0)) }
        }
    }
}

/// Modulo (remainder) of two integers (a % b)
///
/// Stack effect: ( a b -- a%b )
///
/// # Error Handling
/// - Division by zero: Sets runtime error and returns 0
/// - Type mismatch: Sets runtime error and returns 0
///
/// # Safety
/// Stack must have at least two values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_modulo(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "modulo") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(0)) => {
            // Division by zero - set error and return 0
            set_runtime_error(format!(
                "modulo: division by zero (attempted {} % 0)",
                a_val
            ));
            unsafe { push(rest, Value::Int(0)) }
        }
        (Value::Int(a_val), Value::Int(b_val)) => {
            // Use wrapping_rem to handle i64::MIN % -1 overflow edge case
            unsafe { push(rest, Value::Int(a_val.wrapping_rem(b_val))) }
        }
        _ => {
            // Type error - should not happen with type-checked code
            set_runtime_error("modulo: expected two integers on stack");
            unsafe { push(rest, Value::Int(0)) }
        }
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
pub unsafe extern "C" fn patch_seq_eq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "=") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Bool(a_val == b_val))
        },
        _ => panic!("=: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_lt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "<") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Bool(a_val < b_val)) },
        _ => panic!("<: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_gt(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, ">") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe { push(rest, Value::Bool(a_val > b_val)) },
        _ => panic!(">: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_lte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "<=") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Bool(a_val <= b_val))
        },
        _ => panic!("<=: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_gte(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, ">=") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Bool(a_val >= b_val))
        },
        _ => panic!(">=: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_neq(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "<>") };
    match (a, b) {
        (Value::Int(a_val), Value::Int(b_val)) => unsafe {
            push(rest, Value::Bool(a_val != b_val))
        },
        _ => panic!("<>: expected two integers on stack"),
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
pub unsafe extern "C" fn patch_seq_and(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "and") };
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
pub unsafe extern "C" fn patch_seq_or(stack: Stack) -> Stack {
    let (rest, a, b) = unsafe { pop_two(stack, "or") };
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
pub unsafe extern "C" fn patch_seq_not(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "not: stack is empty");
    let (rest, a) = unsafe { pop(stack) };

    match a {
        Value::Int(a_val) => unsafe { push(rest, Value::Int(if a_val == 0 { 1 } else { 0 })) },
        _ => panic!("not: expected integer on stack"),
    }
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
/// Shifts value left by count bits. Negative count or count >= 64 returns 0.
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_shl(stack: Stack) -> Stack {
    let (rest, value, count) = unsafe { pop_two(stack, "shl") };
    match (value, count) {
        (Value::Int(v), Value::Int(c)) => {
            // Use checked_shl to avoid undefined behavior for out-of-range shifts
            // Negative counts become large u32 values, which correctly return None
            let result = if c < 0 {
                0
            } else {
                v.checked_shl(c as u32).unwrap_or(0)
            };
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("shl: expected two integers on stack"),
    }
}

/// Logical shift right (zero-fill)
///
/// Stack effect: ( value count -- result )
/// Shifts value right by count bits, filling with zeros.
/// Negative count or count >= 64 returns 0.
///
/// # Safety
/// Stack must have two Int values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_shr(stack: Stack) -> Stack {
    let (rest, value, count) = unsafe { pop_two(stack, "shr") };
    match (value, count) {
        (Value::Int(v), Value::Int(c)) => {
            // Use checked_shr to avoid undefined behavior for out-of-range shifts
            // Cast to u64 for logical (zero-fill) shift behavior
            let result = if c < 0 {
                0
            } else {
                (v as u64).checked_shr(c as u32).unwrap_or(0) as i64
            };
            unsafe { push(rest, Value::Int(result)) }
        }
        _ => panic!("shr: expected two integers on stack"),
    }
}

/// Population count (count number of 1 bits)
///
/// Stack effect: ( n -- count )
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_popcount(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "popcount: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => unsafe { push(rest, Value::Int(v.count_ones() as i64)) },
        _ => panic!("popcount: expected integer on stack"),
    }
}

/// Count leading zeros
///
/// Stack effect: ( n -- count )
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_clz(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "clz: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => unsafe { push(rest, Value::Int(v.leading_zeros() as i64)) },
        _ => panic!("clz: expected integer on stack"),
    }
}

/// Count trailing zeros
///
/// Stack effect: ( n -- count )
///
/// # Safety
/// Stack must have one Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_ctz(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "ctz: stack is empty");
    let (rest, a) = unsafe { pop(stack) };
    match a {
        Value::Int(v) => unsafe { push(rest, Value::Int(v.trailing_zeros() as i64)) },
        _ => panic!("ctz: expected integer on stack"),
    }
}

/// Push the bit width of Int (64)
///
/// Stack effect: ( -- 64 )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_bits(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(64)) }
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
pub unsafe extern "C" fn patch_seq_peek_int_value(stack: Stack) -> i64 {
    assert!(!stack.is_null(), "peek_int_value: stack is empty");

    // Stack points to next push location, so top is at stack - 1
    let sv = unsafe { peek_sv(stack) };
    if sv.slot0 == DISC_INT {
        sv.slot1 as i64
    } else {
        panic!(
            "peek_int_value: expected Int on stack, got discriminant {}",
            sv.slot0
        )
    }
}

/// Peek at a bool value on top of stack without popping (for pattern matching)
///
/// # Safety
/// Stack must have a Bool value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_peek_bool_value(stack: Stack) -> bool {
    use crate::stack::DISC_BOOL;
    assert!(!stack.is_null(), "peek_bool_value: stack is empty");

    // Stack points to next push location, so top is at stack - 1
    let sv = unsafe { peek_sv(stack) };
    if sv.slot0 == DISC_BOOL {
        sv.slot1 != 0
    } else {
        panic!(
            "peek_bool_value: expected Bool on stack, got discriminant {}",
            sv.slot0
        )
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
pub unsafe extern "C" fn patch_seq_pop_stack(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "pop_stack: stack is empty");
    let (rest, _value) = unsafe { pop(stack) };
    rest
}

// Public re-exports with short names for internal use
pub use patch_seq_add as add;
pub use patch_seq_and as and;
pub use patch_seq_band as band;
pub use patch_seq_bnot as bnot;
pub use patch_seq_bor as bor;
pub use patch_seq_bxor as bxor;
pub use patch_seq_clz as clz;
pub use patch_seq_ctz as ctz;
pub use patch_seq_divide as divide;
pub use patch_seq_eq as eq;
pub use patch_seq_gt as gt;
pub use patch_seq_gte as gte;
pub use patch_seq_int_bits as int_bits;
pub use patch_seq_lt as lt;
pub use patch_seq_lte as lte;
pub use patch_seq_multiply as multiply;
pub use patch_seq_neq as neq;
pub use patch_seq_not as not;
pub use patch_seq_or as or;
pub use patch_seq_popcount as popcount;
pub use patch_seq_push_bool as push_bool;
pub use patch_seq_push_int as push_int;
pub use patch_seq_shl as shl;
pub use patch_seq_shr as shr;
pub use patch_seq_subtract as subtract;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 5);
            let stack = push_int(stack, 3);
            let stack = add(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(8));
        }
    }

    #[test]
    fn test_subtract() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 10);
            let stack = push_int(stack, 3);
            let stack = subtract(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(7));
        }
    }

    #[test]
    fn test_multiply() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 4);
            let stack = push_int(stack, 5);
            let stack = multiply(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(20));
        }
    }

    #[test]
    fn test_divide() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 20);
            let stack = push_int(stack, 4);
            let stack = divide(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5));
        }
    }

    #[test]
    fn test_comparisons() {
        unsafe {
            // Test eq (returns true/false Bool)
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 5);
            let stack = push_int(stack, 5);
            let stack = eq(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));

            // Test lt
            let stack = push_int(stack, 3);
            let stack = push_int(stack, 5);
            let stack = lt(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));

            // Test gt
            let stack = push_int(stack, 7);
            let stack = push_int(stack, 5);
            let stack = gt(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_overflow_wrapping() {
        // Test that arithmetic uses wrapping semantics (defined overflow behavior)
        unsafe {
            // Test add overflow: i64::MAX + 1 should wrap
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, i64::MAX);
            let stack = push_int(stack, 1);
            let stack = add(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MIN)); // Wraps to minimum

            // Test multiply overflow
            let stack = push_int(stack, i64::MAX);
            let stack = push_int(stack, 2);
            let stack = multiply(stack);
            let (_stack, result) = pop(stack);
            // Result is well-defined (wrapping)
            assert!(matches!(result, Value::Int(_)));

            // Test subtract underflow
            let stack = push_int(stack, i64::MIN);
            let stack = push_int(stack, 1);
            let stack = subtract(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(i64::MAX)); // Wraps to maximum
        }
    }

    #[test]
    fn test_negative_division() {
        unsafe {
            // Test negative dividend
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, -10);
            let stack = push_int(stack, 3);
            let stack = divide(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-3)); // Truncates toward zero

            // Test negative divisor
            let stack = push_int(stack, 10);
            let stack = push_int(stack, -3);
            let stack = divide(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-3));

            // Test both negative
            let stack = push_int(stack, -10);
            let stack = push_int(stack, -3);
            let stack = divide(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3));
        }
    }

    #[test]
    fn test_division_overflow_edge_case() {
        // Critical edge case: i64::MIN / -1 would overflow
        // Regular division panics, but wrapping_div wraps to i64::MIN
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, i64::MIN);
            let stack = push_int(stack, -1);
            let stack = divide(stack);
            let (_stack, result) = pop(stack);
            // i64::MIN / -1 would be i64::MAX + 1, which wraps to i64::MIN
            assert_eq!(result, Value::Int(i64::MIN));
        }
    }

    #[test]
    fn test_and_true_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 1);
            let stack = push_int(stack, 1);
            let stack = and(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
        }
    }

    #[test]
    fn test_and_true_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 1);
            let stack = push_int(stack, 0);
            let stack = and(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_and_false_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 0);
            let stack = push_int(stack, 0);
            let stack = and(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_or_true_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 1);
            let stack = push_int(stack, 1);
            let stack = or(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
        }
    }

    #[test]
    fn test_or_true_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 1);
            let stack = push_int(stack, 0);
            let stack = or(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
        }
    }

    #[test]
    fn test_or_false_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 0);
            let stack = push_int(stack, 0);
            let stack = or(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_not_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 1);
            let stack = not(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_not_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 0);
            let stack = not(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
        }
    }

    #[test]
    fn test_and_nonzero_values() {
        // Forth-style: any non-zero is true
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 42);
            let stack = push_int(stack, -5);
            let stack = and(stack);
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
        }
    }

    #[test]
    fn test_divide_by_zero_sets_error() {
        unsafe {
            crate::error::clear_runtime_error();
            let stack = crate::stack::alloc_test_stack();
            let stack = push_int(stack, 42);
            let stack = push_int(stack, 0);
            let stack = divide(stack);

            // Should have set an error
            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("division by zero"));

            // Should have pushed 0 as sentinel
            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }
}
