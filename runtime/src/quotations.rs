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
}
