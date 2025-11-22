//! Conditional combinator for multi-way branching
//!
//! Provides `cond` - a concatenative alternative to match/case statements.

use crate::stack::{Stack, pop};
use crate::value::Value;

/// Multi-way conditional combinator
///
/// Takes N predicate/body quotation pairs from the stack, plus a value to test.
/// Tries each predicate in order (last to first on stack). When a predicate
/// returns non-zero, executes its corresponding body and returns.
///
/// Stack effect: ( value [pred1] [body1] [pred2] [body2] ... [predN] [bodyN] count -- result )
///
/// Each predicate quotation has effect: ( value -- value bool )
/// Each body quotation has effect: ( value -- result )
///
/// Example:
/// ```cem
/// : route ( request -- response )
///   [ dup "GET /" = ]           [ drop "Hello" ]
///   [ dup "/api" starts-with ]  [ get-users ]
///   [ drop 1 ]                  [ drop "Not Found" ]
///   3 cond ;
/// ```
///
/// # Safety
/// - Stack must have at least (2*count + 2) values
/// - All predicate/body values must be Quotations
/// - Predicates must return Int (0 or non-zero)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn cond(mut stack: Stack) -> Stack {
    unsafe {
        // Pop count
        let (stack_temp, count_val) = pop(stack);
        let count = match count_val {
            Value::Int(n) if n >= 0 => n as usize,
            Value::Int(n) => panic!("cond: count must be non-negative, got {}", n),
            _ => panic!("cond: expected Int count, got {:?}", count_val),
        };

        if count == 0 {
            panic!("cond: need at least one predicate/body pair");
        }

        // Pop all predicate/body pairs into a vector
        // Stack is [ value pred1 body1 pred2 body2 ... predN bodyN ]
        // We pop from top (bodyN) down to bottom (pred1)
        let mut pairs = Vec::with_capacity(count);
        stack = stack_temp;

        for _ in 0..count {
            // Pop body quotation
            let (stack_temp, body_val) = pop(stack);
            let body_ptr = match body_val {
                Value::Quotation(ptr) => ptr,
                _ => panic!("cond: expected body Quotation, got {:?}", body_val),
            };

            // Pop predicate quotation
            let (stack_temp2, pred_val) = pop(stack_temp);
            let pred_ptr = match pred_val {
                Value::Quotation(ptr) => ptr,
                _ => panic!("cond: expected predicate Quotation, got {:?}", pred_val),
            };

            stack = stack_temp2;
            pairs.push((pred_ptr, body_ptr));
        }

        // Now pairs is in reverse order (last pair at index 0)
        // Reverse it so we try first pair first
        pairs.reverse();

        // Value is now on top of stack
        // For each pair, dup value, run predicate, check result
        for (pred_ptr, body_ptr) in pairs {
            // Cast function pointers
            let pred_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(pred_ptr);
            let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(body_ptr);

            // Execute predicate (keeps value on stack, adds boolean result)
            stack = pred_fn(stack);

            // Pop predicate result
            let (stack_after_pred, pred_result) = pop(stack);

            let matches = match pred_result {
                Value::Int(0) => false,
                Value::Int(_) => true,
                _ => panic!("cond: predicate must return Int, got {:?}", pred_result),
            };

            if matches {
                // Predicate matched! Execute body and return
                stack = body_fn(stack_after_pred);
                return stack;
            }

            // Predicate didn't match, try next pair
            stack = stack_after_pred;
        }

        // No predicate matched!
        panic!("cond: no predicate matched");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::push;
    use crate::quotations::push_quotation;
    use crate::seqstring::global_string;

    // Helper predicates for testing
    #[unsafe(no_mangle)]
    unsafe extern "C" fn is_zero(stack: Stack) -> Stack {
        let (stack, val) = unsafe { pop(stack) };
        let is_zero = match val {
            Value::Int(0) => 1,
            Value::Int(_) => 0,
            _ => panic!("is_zero: expected Int"),
        };
        let stack = unsafe { push(stack, val) }; // Put value back
        unsafe { push(stack, Value::Int(is_zero)) }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn is_positive(stack: Stack) -> Stack {
        let (stack, val) = unsafe { pop(stack) };
        let is_pos = match val {
            Value::Int(n) if n > 0 => 1,
            Value::Int(_) => 0,
            _ => panic!("is_positive: expected Int"),
        };
        let stack = unsafe { push(stack, val) }; // Put value back
        unsafe { push(stack, Value::Int(is_pos)) }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn is_negative(stack: Stack) -> Stack {
        let (stack, val) = unsafe { pop(stack) };
        let is_neg = match val {
            Value::Int(n) if n < 0 => 1,
            Value::Int(_) => 0,
            _ => panic!("is_negative: expected Int"),
        };
        let stack = unsafe { push(stack, val) }; // Put value back
        unsafe { push(stack, Value::Int(is_neg)) }
    }

    // Helper bodies for testing
    #[unsafe(no_mangle)]
    unsafe extern "C" fn return_zero(stack: Stack) -> Stack {
        let (stack, _) = unsafe { pop(stack) }; // Drop input value
        unsafe { push(stack, Value::String(global_string("zero".to_string()))) }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn return_positive(stack: Stack) -> Stack {
        let (stack, _) = unsafe { pop(stack) }; // Drop input value
        unsafe { push(stack, Value::String(global_string("positive".to_string()))) }
    }

    #[unsafe(no_mangle)]
    unsafe extern "C" fn return_negative(stack: Stack) -> Stack {
        let (stack, _) = unsafe { pop(stack) }; // Drop input value
        unsafe { push(stack, Value::String(global_string("negative".to_string()))) }
    }

    #[test]
    fn test_cond_first_match() {
        unsafe {
            let stack = std::ptr::null_mut();

            // Push value to test
            let stack = push(stack, Value::Int(0));

            // Push predicate/body pairs
            let stack = push_quotation(stack, is_zero as usize);
            let stack = push_quotation(stack, return_zero as usize);

            let stack = push_quotation(stack, is_positive as usize);
            let stack = push_quotation(stack, return_positive as usize);

            let stack = push_quotation(stack, is_negative as usize);
            let stack = push_quotation(stack, return_negative as usize);

            // Push count
            let stack = push(stack, Value::Int(3));

            // Call cond
            let stack = cond(stack);

            // Should match first predicate (is_zero)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("zero".to_string())));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_cond_second_match() {
        unsafe {
            let stack = std::ptr::null_mut();

            // Push value to test
            let stack = push(stack, Value::Int(42));

            // Push predicate/body pairs
            let stack = push_quotation(stack, is_zero as usize);
            let stack = push_quotation(stack, return_zero as usize);

            let stack = push_quotation(stack, is_positive as usize);
            let stack = push_quotation(stack, return_positive as usize);

            let stack = push_quotation(stack, is_negative as usize);
            let stack = push_quotation(stack, return_negative as usize);

            // Push count
            let stack = push(stack, Value::Int(3));

            // Call cond
            let stack = cond(stack);

            // Should match second predicate (is_positive)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("positive".to_string())));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_cond_third_match() {
        unsafe {
            let stack = std::ptr::null_mut();

            // Push value to test
            let stack = push(stack, Value::Int(-5));

            // Push predicate/body pairs
            let stack = push_quotation(stack, is_zero as usize);
            let stack = push_quotation(stack, return_zero as usize);

            let stack = push_quotation(stack, is_positive as usize);
            let stack = push_quotation(stack, return_positive as usize);

            let stack = push_quotation(stack, is_negative as usize);
            let stack = push_quotation(stack, return_negative as usize);

            // Push count
            let stack = push(stack, Value::Int(3));

            // Call cond
            let stack = cond(stack);

            // Should match third predicate (is_negative)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("negative".to_string())));
            assert!(stack.is_null());
        }
    }

    // Note: We don't test the "no predicate matches" panic case here because
    // panics in extern "C" functions can't unwind through the test harness.
    // The panic behavior is correct (verified manually), but causes test abort.
    // In production, users should ensure at least one predicate always matches
    // (typically with a catch-all like: [ drop 1 ] [ default-action ])
}
