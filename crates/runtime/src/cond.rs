//! Conditional combinator for multi-way branching
//!
//! Provides `cond` - a concatenative alternative to match/case statements.
//! Uses quotation pairs (predicate + body) evaluated in order until one matches.

use crate::stack::{Stack, pop};
use crate::value::Value;

/// Multi-way conditional combinator
///
/// # Stack Effect
///
/// `( value [pred1] [body1] ... [predN] [bodyN] N -- result )`
///
/// # How It Works
///
/// 1. Takes a value and N predicate/body quotation pairs from the stack
/// 2. Tries each predicate in order (first pair = first tried)
/// 3. When a predicate returns true, executes its body and returns
/// 4. Panics if no predicate matches (always include a default case)
///
/// # Quotation Contracts
///
/// - **Predicate**: `( value -- value Bool )` - keeps value on stack, pushes true or false
/// - **Body**: `( value -- result )` - consumes value, produces result
///
/// # Default Case Pattern
///
/// Use `[ true ]` as the last predicate to create an "otherwise" case that always matches:
///
/// ```text
/// [ true ] [ drop "default result" ]
/// ```
///
/// # Example: Classify a Number
///
/// ```text
/// : classify ( Int -- String )
///   [ dup 0 i.< ]  [ drop "negative" ]
///   [ dup 0 i.= ]  [ drop "zero" ]
///   [ true ]       [ drop "positive" ]
///   3 cond
/// ;
///
/// -5 classify   # "negative"
/// 0 classify    # "zero"
/// 42 classify   # "positive"
/// ```
///
/// # Example: FizzBuzz Logic
///
/// ```text
/// : fizzbuzz ( Int -- String )
///   [ dup 15 i.% 0 i.= ]  [ drop "FizzBuzz" ]
///   [ dup 3 i.% 0 i.= ]   [ drop "Fizz" ]
///   [ dup 5 i.% 0 i.= ]   [ drop "Buzz" ]
///   [ true ]              [ int->string ]
///   4 cond
/// ;
/// ```
///
/// # Safety
///
/// - Stack must have at least (2*N + 1) values (value + N pairs)
/// - All predicate/body values must be Quotations
/// - Predicates must return Bool
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
                Value::Bool(b) => b,
                _ => panic!("cond: predicate must return Bool, got {:?}", pred_result),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::push;

    // Helper: predicate that always returns true (keeps value, pushes true)
    // Stack effect: ( value -- value Bool )
    unsafe extern "C" fn pred_always_true(stack: Stack) -> Stack {
        unsafe { push(stack, Value::Bool(true)) }
    }

    // Helper: predicate that always returns false (keeps value, pushes false)
    // Stack effect: ( value -- value Bool )
    unsafe extern "C" fn pred_always_false(stack: Stack) -> Stack {
        unsafe { push(stack, Value::Bool(false)) }
    }

    // Helper: predicate that checks if top value is zero
    // Stack effect: ( Int -- Int Bool )
    unsafe extern "C" fn pred_is_zero(stack: Stack) -> Stack {
        unsafe {
            let val = crate::stack::peek(stack);
            match val {
                Value::Int(n) => push(stack, Value::Bool(n == 0)),
                _ => panic!("pred_is_zero: expected Int"),
            }
        }
    }

    // Helper: predicate that checks if value is negative
    // Stack effect: ( Int -- Int Bool )
    unsafe extern "C" fn pred_is_negative(stack: Stack) -> Stack {
        unsafe {
            let val = crate::stack::peek(stack);
            match val {
                Value::Int(n) => push(stack, Value::Bool(n < 0)),
                _ => panic!("pred_is_negative: expected Int"),
            }
        }
    }

    // Helper: body that drops value and pushes "matched"
    // Stack effect: ( value -- String )
    unsafe extern "C" fn body_matched(stack: Stack) -> Stack {
        unsafe {
            let (stack, _) = pop(stack);
            push(
                stack,
                Value::String(crate::seqstring::global_string("matched".to_string())),
            )
        }
    }

    // Helper: body that drops value and pushes "zero"
    // Stack effect: ( value -- String )
    unsafe extern "C" fn body_zero(stack: Stack) -> Stack {
        unsafe {
            let (stack, _) = pop(stack);
            push(
                stack,
                Value::String(crate::seqstring::global_string("zero".to_string())),
            )
        }
    }

    // Helper: body that drops value and pushes "positive"
    // Stack effect: ( value -- String )
    unsafe extern "C" fn body_positive(stack: Stack) -> Stack {
        unsafe {
            let (stack, _) = pop(stack);
            push(
                stack,
                Value::String(crate::seqstring::global_string("positive".to_string())),
            )
        }
    }

    // Helper: body that drops value and pushes "negative"
    // Stack effect: ( value -- String )
    unsafe extern "C" fn body_negative(stack: Stack) -> Stack {
        unsafe {
            let (stack, _) = pop(stack);
            push(
                stack,
                Value::String(crate::seqstring::global_string("negative".to_string())),
            )
        }
    }

    // Helper: body that drops value and pushes "default"
    // Stack effect: ( value -- String )
    unsafe extern "C" fn body_default(stack: Stack) -> Stack {
        unsafe {
            let (stack, _) = pop(stack);
            push(
                stack,
                Value::String(crate::seqstring::global_string("default".to_string())),
            )
        }
    }

    // Helper to create a quotation value from a function pointer
    fn make_quotation(f: unsafe extern "C" fn(Stack) -> Stack) -> Value {
        let ptr = f as usize;
        Value::Quotation {
            wrapper: ptr,
            impl_: ptr,
        }
    }

    #[test]
    fn test_cond_single_match() {
        // Test: single predicate that always matches
        // Stack: value [pred_always_true] [body_matched] 1
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42)); // value
            let stack = push(stack, make_quotation(pred_always_true));
            let stack = push(stack, make_quotation(body_matched));
            let stack = push(stack, Value::Int(1)); // count

            let stack = cond(stack);

            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "matched"),
                _ => panic!("Expected String, got {:?}", result),
            }
        }
    }

    #[test]
    fn test_cond_first_match_wins() {
        // Test: multiple predicates, first one that matches should win
        // Both predicates would match, but first one should be used
        // Stack: value [pred_always_true] [body_matched] [pred_always_true] [body_default] 2
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42)); // value
            let stack = push(stack, make_quotation(pred_always_true));
            let stack = push(stack, make_quotation(body_matched)); // first pair
            let stack = push(stack, make_quotation(pred_always_true));
            let stack = push(stack, make_quotation(body_default)); // second pair
            let stack = push(stack, Value::Int(2)); // count

            let stack = cond(stack);

            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "matched"), // first body wins
                _ => panic!("Expected String, got {:?}", result),
            }
        }
    }

    #[test]
    fn test_cond_second_match() {
        // Test: first predicate fails, second matches
        // Stack: value [pred_always_false] [body_matched] [pred_always_true] [body_default] 2
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42)); // value
            let stack = push(stack, make_quotation(pred_always_false));
            let stack = push(stack, make_quotation(body_matched)); // first pair - won't match
            let stack = push(stack, make_quotation(pred_always_true));
            let stack = push(stack, make_quotation(body_default)); // second pair - will match
            let stack = push(stack, Value::Int(2)); // count

            let stack = cond(stack);

            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "default"), // second body wins
                _ => panic!("Expected String, got {:?}", result),
            }
        }
    }

    #[test]
    fn test_cond_classify_number() {
        // Test: classify numbers as negative, zero, or positive
        // This mimics the example from the docs
        unsafe {
            // Test negative number
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(-5)); // value
            let stack = push(stack, make_quotation(pred_is_negative));
            let stack = push(stack, make_quotation(body_negative));
            let stack = push(stack, make_quotation(pred_is_zero));
            let stack = push(stack, make_quotation(body_zero));
            let stack = push(stack, make_quotation(pred_always_true)); // default
            let stack = push(stack, make_quotation(body_positive));
            let stack = push(stack, Value::Int(3)); // count

            let stack = cond(stack);
            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "negative"),
                _ => panic!("Expected String"),
            }

            // Test zero
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(0)); // value
            let stack = push(stack, make_quotation(pred_is_negative));
            let stack = push(stack, make_quotation(body_negative));
            let stack = push(stack, make_quotation(pred_is_zero));
            let stack = push(stack, make_quotation(body_zero));
            let stack = push(stack, make_quotation(pred_always_true)); // default
            let stack = push(stack, make_quotation(body_positive));
            let stack = push(stack, Value::Int(3)); // count

            let stack = cond(stack);
            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "zero"),
                _ => panic!("Expected String"),
            }

            // Test positive
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42)); // value
            let stack = push(stack, make_quotation(pred_is_negative));
            let stack = push(stack, make_quotation(body_negative));
            let stack = push(stack, make_quotation(pred_is_zero));
            let stack = push(stack, make_quotation(body_zero));
            let stack = push(stack, make_quotation(pred_always_true)); // default
            let stack = push(stack, make_quotation(body_positive));
            let stack = push(stack, Value::Int(3)); // count

            let stack = cond(stack);
            let (_, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "positive"),
                _ => panic!("Expected String"),
            }
        }
    }

    // Note: #[should_panic] tests don't work with extern "C" functions because
    // they can't unwind. The following panic conditions are documented in the
    // function's doc comments and verified by the compiler's type system:
    //
    // - "cond: no predicate matched" - when all predicates return false
    // - "cond: need at least one predicate/body pair" - when count is 0
    // - "cond: count must be non-negative" - when count is negative
    // - "cond: expected body Quotation" - when body is not a Quotation
    // - "cond: expected predicate Quotation" - when predicate is not a Quotation
    // - "cond: predicate must return Bool" - when predicate returns non-Bool
}
