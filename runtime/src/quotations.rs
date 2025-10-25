//! Quotation operations for cem3
//!
//! Quotations are deferred code blocks (first-class functions).
//! A quotation is represented as a function pointer stored as usize.

use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Push a quotation (function pointer) onto the stack
///
/// Stack effect: ( -- quot )
///
/// # Safety
/// - Stack pointer must be valid (or null for empty stack)
/// - Function pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn push_quotation(stack: Stack, fn_ptr: usize) -> Stack {
    unsafe { push(stack, Value::Quotation(fn_ptr)) }
}

/// Call a quotation
///
/// Pops a quotation from the stack and executes it.
/// The quotation function takes the current stack and returns a new stack.
///
/// Stack effect: ( ..a quot -- ..b )
/// where the quotation has effect ( ..a -- ..b )
///
/// # Safety
/// - Stack must not be null
/// - Top of stack must be a Quotation value
/// - Function pointer must be valid and have signature: Stack -> Stack
#[unsafe(no_mangle)]
pub unsafe extern "C" fn call(stack: Stack) -> Stack {
    unsafe {
        let (stack, value) = pop(stack);

        match value {
            Value::Quotation(fn_ptr) => {
                // Cast the function pointer back and call it
                let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);
                fn_ref(stack)
            }
            _ => panic!("call: expected Quotation on stack, got {:?}", value),
        }
    }
}

/// Execute a quotation n times
///
/// Pops a count (Int) and a quotation from the stack, then executes
/// the quotation that many times.
///
/// Stack effect: ( ..a quot n -- ..a )
/// where the quotation has effect ( ..a -- ..a )
///
/// # Safety
/// - Stack must have at least 2 values
/// - Top must be Int (the count)
/// - Second must be Quotation
/// - Quotation's effect must preserve stack shape
#[unsafe(no_mangle)]
pub unsafe extern "C" fn times(mut stack: Stack) -> Stack {
    unsafe {
        // Pop count
        let (stack_temp, count_value) = pop(stack);
        let count = match count_value {
            Value::Int(n) => n,
            _ => panic!("times: expected Int count, got {:?}", count_value),
        };

        // Pop quotation
        let (stack_temp2, quot_value) = pop(stack_temp);
        let fn_ptr = match quot_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("times: expected Quotation, got {:?}", quot_value),
        };

        // Cast function pointer
        let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);

        // Execute quotation n times
        stack = stack_temp2;
        for _ in 0..count {
            stack = fn_ref(stack);
        }

        stack
    }
}

/// Loop while a condition is true
///
/// Pops a body quotation and a condition quotation from the stack.
/// Repeatedly executes: condition quotation, check result (Int: 0=false, non-zero=true),
/// if true then execute body quotation, repeat.
///
/// Stack effect: ( ..a cond-quot body-quot -- ..a )
/// where cond-quot has effect ( ..a -- ..a Int )
/// and body-quot has effect ( ..a -- ..a )
///
/// # Safety
/// - Stack must have at least 2 values
/// - Top must be Quotation (body)
/// - Second must be Quotation (condition)
/// - Condition quotation must push exactly one Int
/// - Body quotation must preserve stack shape
#[unsafe(no_mangle)]
pub unsafe extern "C" fn while_loop(mut stack: Stack) -> Stack {
    unsafe {
        // Pop body quotation
        let (stack_temp, body_value) = pop(stack);
        let body_ptr = match body_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("while: expected body Quotation, got {:?}", body_value),
        };

        // Pop condition quotation
        let (stack_temp2, cond_value) = pop(stack_temp);
        let cond_ptr = match cond_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("while: expected condition Quotation, got {:?}", cond_value),
        };

        // Cast function pointers
        let cond_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(cond_ptr);
        let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(body_ptr);

        // Loop while condition is true
        stack = stack_temp2;
        loop {
            // Execute condition quotation
            stack = cond_fn(stack);

            // Pop the condition result
            let (stack_after_cond, cond_result) = pop(stack);
            let is_true = match cond_result {
                Value::Int(n) => n != 0,
                _ => panic!("while: condition must return Int, got {:?}", cond_result),
            };

            if !is_true {
                // Condition is false, exit loop
                stack = stack_after_cond;
                break;
            }

            // Condition is true, execute body
            stack = body_fn(stack_after_cond);
        }

        stack
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arithmetic::push_int;

    // Helper function for testing: a quotation that adds 1
    unsafe extern "C" fn add_one_quot(stack: Stack) -> Stack {
        unsafe {
            let stack = push_int(stack, 1);
            crate::arithmetic::add(stack)
        }
    }

    #[test]
    fn test_push_quotation() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Push a quotation
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr);

            // Verify it's on the stack
            let (stack, value) = pop(stack);
            assert!(matches!(value, Value::Quotation(_)));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_call_quotation() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Push 5, then a quotation that adds 1
            let stack = push_int(stack, 5);
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr);

            // Call the quotation
            let stack = call(stack);

            // Result should be 6
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(6));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_times_combinator() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Push 0, then execute [ 1 add ] 5 times
            let stack = push_int(stack, 0);
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr);
            let stack = push_int(stack, 5);

            // Execute times
            let stack = times(stack);

            // Result should be 5 (0 + 1 + 1 + 1 + 1 + 1)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_times_zero() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Push 10, then execute quotation 0 times
            let stack = push_int(stack, 10);
            let fn_ptr = add_one_quot as usize;
            let stack = push_quotation(stack, fn_ptr);
            let stack = push_int(stack, 0);

            // Execute times
            let stack = times(stack);

            // Result should still be 10 (quotation not executed)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(10));
            assert!(stack.is_null());
        }
    }

    // Helper quotation: dup then check if top value > 0
    // Corresponds to: [ dup 0 > ]
    unsafe extern "C" fn dup_gt_zero_quot(stack: Stack) -> Stack {
        unsafe {
            let stack = crate::stack::dup(stack); // Duplicate the value
            let stack = push_int(stack, 0);
            crate::arithmetic::gt(stack)
        }
    }

    // Helper quotation: subtract 1 from top value
    // Corresponds to: [ 1 subtract ]
    unsafe extern "C" fn subtract_one_quot(stack: Stack) -> Stack {
        unsafe {
            let stack = push_int(stack, 1);
            crate::arithmetic::subtract(stack)
        }
    }

    #[test]
    fn test_while_countdown() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Countdown from 5 to 0 using while
            // [ dup 0 > ] [ dup 1 - ] while
            let stack = push_int(stack, 5);

            // Push condition: dup 0 >
            let cond_ptr = dup_gt_zero_quot as usize;
            let stack = push_quotation(stack, cond_ptr);

            // Push body: 1 subtract
            let body_ptr = subtract_one_quot as usize;
            let stack = push_quotation(stack, body_ptr);

            // Execute while
            let stack = while_loop(stack);

            // Result should be 0
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_while_false_immediately() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Start with 0, so condition is immediately false
            let stack = push_int(stack, 0);

            let cond_ptr = dup_gt_zero_quot as usize;
            let stack = push_quotation(stack, cond_ptr);

            let body_ptr = subtract_one_quot as usize;
            let stack = push_quotation(stack, body_ptr);

            // Execute while
            let stack = while_loop(stack);

            // Result should still be 0 (body never executed)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }
}
