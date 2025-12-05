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
pub unsafe extern "C" fn patch_seq_cond(mut stack: Stack) -> Stack {
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
            let body_wrapper = match body_val {
                Value::Quotation { wrapper, .. } => wrapper,
                _ => panic!("cond: expected body Quotation, got {:?}", body_val),
            };

            // Pop predicate quotation
            let (stack_temp2, pred_val) = pop(stack_temp);
            let pred_wrapper = match pred_val {
                Value::Quotation { wrapper, .. } => wrapper,
                _ => panic!("cond: expected predicate Quotation, got {:?}", pred_val),
            };

            stack = stack_temp2;
            pairs.push((pred_wrapper, body_wrapper));
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

// Public re-export with short name for internal use
pub use patch_seq_cond as cond;
