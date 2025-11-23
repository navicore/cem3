//! Quotation operations for Seq
//!
//! Quotations are deferred code blocks (first-class functions).
//! A quotation is represented as a function pointer stored as usize.

use crate::scheduler::strand_spawn;
use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// Type alias for closure registry entries
type ClosureEntry = (usize, Box<[Value]>);

/// Global registry for closure environments in spawned strands
/// Maps closure_spawn_id -> (fn_ptr, env)
/// Cleaned up when the trampoline retrieves and executes the closure
static SPAWN_CLOSURE_REGISTRY: LazyLock<Mutex<HashMap<i64, ClosureEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Trampoline function for spawning closures
///
/// This function is passed to strand_spawn when spawning a closure.
/// It expects the closure_spawn_id on the stack, retrieves the closure data
/// from the registry, and calls the closure function with the environment.
///
/// Stack effect: ( closure_spawn_id -- ... )
/// The closure function determines the final stack state.
///
/// # Safety
/// This function is safe to call, but internally uses unsafe operations
/// to transmute function pointers and call the closure function.
extern "C" fn closure_spawn_trampoline(stack: Stack) -> Stack {
    unsafe {
        // Pop closure_spawn_id from stack
        let (stack, closure_spawn_id_val) = pop(stack);
        let closure_spawn_id = match closure_spawn_id_val {
            Value::Int(id) => id,
            _ => panic!(
                "closure_spawn_trampoline: expected Int (closure_spawn_id), got {:?}",
                closure_spawn_id_val
            ),
        };

        // Retrieve closure data from registry
        let (fn_ptr, env) = {
            let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
            registry.remove(&closure_spawn_id).unwrap_or_else(|| {
                panic!(
                    "closure_spawn_trampoline: no data for closure_spawn_id {}",
                    closure_spawn_id
                )
            })
        };

        // Call closure function with empty stack and environment
        // Closure signature: fn(Stack, *const Value, usize) -> Stack
        let env_ptr = env.as_ptr();
        let env_len = env.len();

        let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
            std::mem::transmute(fn_ptr);

        // Call closure and return result (environment is dropped here)
        fn_ref(stack, env_ptr, env_len)
    }
}

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

/// Call a quotation or closure
///
/// Pops a quotation or closure from the stack and executes it.
/// For stateless quotations, calls the function with just the stack.
/// For closures, calls the function with both the stack and captured environment.
/// The function takes the current stack and returns a new stack.
///
/// Stack effect: ( ..a quot -- ..b )
/// where the quotation has effect ( ..a -- ..b )
///
/// # Safety
/// - Stack must not be null
/// - Top of stack must be a Quotation or Closure value
/// - Function pointer must be valid
/// - Quotation signature: Stack -> Stack
/// - Closure signature: Stack, *const [Value] -> Stack
#[unsafe(no_mangle)]
pub unsafe extern "C" fn call(stack: Stack) -> Stack {
    unsafe {
        let (stack, value) = pop(stack);

        match value {
            Value::Quotation(fn_ptr) => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("call: quotation function pointer is null");
                }

                // SAFETY: fn_ptr was created by the compiler's codegen and stored via push_quotation.
                // The compiler guarantees that quotation literals produce valid function pointers
                // with the signature: unsafe extern "C" fn(Stack) -> Stack.
                // We've verified fn_ptr is non-null above.
                let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);
                fn_ref(stack)
            }
            Value::Closure { fn_ptr, env } => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("call: closure function pointer is null");
                }

                // Convert Box<[Value]> to raw parts (data pointer + length)
                // LLVM IR can't handle Rust's fat pointers, so we pass them separately
                let env_ptr = Box::into_raw(env);
                let env_slice = &*env_ptr;
                let env_data = env_slice.as_ptr();
                let env_len = env_slice.len();

                // SAFETY: fn_ptr was created by the compiler's codegen for a closure.
                // The compiler guarantees that closure functions have the signature:
                // unsafe extern "C" fn(Stack, *const Value, usize) -> Stack.
                // We pass the environment as (data, len) since LLVM can't handle fat pointers.
                let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                    std::mem::transmute(fn_ptr);
                let result_stack = fn_ref(stack, env_data, env_len);

                // Clean up environment (convert back to Box and drop)
                let _ = Box::from_raw(env_ptr);

                result_stack
            }
            _ => panic!(
                "call: expected Quotation or Closure on stack, got {:?}",
                value
            ),
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

        // Validate function pointer is not null
        if fn_ptr == 0 {
            panic!("times: quotation function pointer is null");
        }

        // SAFETY: fn_ptr was created by the compiler's codegen and stored via push_quotation.
        // The compiler guarantees that quotation literals produce valid function pointers.
        // We've verified fn_ptr is non-null above.
        let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);

        // Execute quotation n times
        // IMPORTANT: Yield after each iteration to maintain cooperative scheduling
        stack = stack_temp2;
        for _ in 0..count {
            stack = fn_ref(stack);
            may::coroutine::yield_now();
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

        // Validate function pointers are not null
        if cond_ptr == 0 {
            panic!("while: condition quotation function pointer is null");
        }
        if body_ptr == 0 {
            panic!("while: body quotation function pointer is null");
        }

        // SAFETY: Both fn_ptrs were created by the compiler's codegen and stored via push_quotation.
        // The compiler guarantees that quotation literals produce valid function pointers.
        // We've verified both ptrs are non-null above.
        let cond_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(cond_ptr);
        let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(body_ptr);

        // Loop while condition is true
        // IMPORTANT: Yield after each iteration to maintain cooperative scheduling
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

            // Yield to scheduler after each iteration
            may::coroutine::yield_now();
        }

        stack
    }
}

/// Execute a quotation forever (infinite loop)
///
/// Pops a quotation from the stack and executes it repeatedly in an infinite loop.
/// This is useful for server accept loops and other continuous operations.
///
/// Stack effect: ( ..a quot -- ..a )
/// where the quotation has effect ( ..a -- ..a )
///
/// # Example
/// ```cem
/// : server-loop ( listener -- )
///   [ dup tcp-accept handle-client ] forever ;
/// ```
///
/// # Safety
/// - Stack must have at least 1 value
/// - Top must be Quotation
/// - Quotation must not cause stack underflow
/// - This never returns! (infinite loop)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn forever(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation
        let (mut stack, quot_value) = pop(stack);
        let fn_ptr = match quot_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("forever: expected Quotation, got {:?}", quot_value),
        };

        // Validate function pointer is not null
        if fn_ptr == 0 {
            panic!("forever: quotation function pointer is null");
        }

        // SAFETY: fn_ptr was created by the compiler's codegen and stored via push_quotation.
        // The compiler guarantees that quotation literals produce valid function pointers.
        // We've verified fn_ptr is non-null above.
        let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);

        // Infinite loop - execute body forever
        // IMPORTANT: Yield after each iteration to maintain cooperative scheduling.
        // Without yielding, this coroutine would monopolize the thread and starve other strands.
        loop {
            stack = body_fn(stack);
            may::coroutine::yield_now();
        }
    }
}

/// Loop until a condition is true
///
/// Pops a condition quotation and a body quotation from the stack.
/// Repeatedly executes: body quotation, then condition quotation, check result (Int: 0=false, non-zero=true),
/// if false then continue loop, if true then exit.
///
/// This is the inverse of `while`: executes body at least once, then checks condition.
///
/// Stack effect: ( ..a body-quot cond-quot -- ..a )
/// where body-quot has effect ( ..a -- ..a )
/// and cond-quot has effect ( ..a -- ..a Int )
///
/// # Safety
/// - Stack must have at least 2 values
/// - Top must be Quotation (condition)
/// - Second must be Quotation (body)
/// - Condition quotation must push exactly one Int
/// - Body quotation must preserve stack shape
#[unsafe(no_mangle)]
pub unsafe extern "C" fn until_loop(mut stack: Stack) -> Stack {
    unsafe {
        // Pop condition quotation
        let (stack_temp, cond_value) = pop(stack);
        let cond_ptr = match cond_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("until: expected condition Quotation, got {:?}", cond_value),
        };

        // Pop body quotation
        let (stack_temp2, body_value) = pop(stack_temp);
        let body_ptr = match body_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("until: expected body Quotation, got {:?}", body_value),
        };

        // Validate function pointers are not null
        if cond_ptr == 0 {
            panic!("until: condition quotation function pointer is null");
        }
        if body_ptr == 0 {
            panic!("until: body quotation function pointer is null");
        }

        // SAFETY: Both fn_ptrs were created by the compiler's codegen and stored via push_quotation.
        // The compiler guarantees that quotation literals produce valid function pointers.
        // We've verified both ptrs are non-null above.
        let cond_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(cond_ptr);
        let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(body_ptr);

        // Loop until condition is true (do-while style)
        // IMPORTANT: Yield after each iteration to maintain cooperative scheduling
        stack = stack_temp2;
        loop {
            // Execute body quotation
            stack = body_fn(stack);

            // Execute condition quotation
            stack = cond_fn(stack);

            // Pop the condition result
            let (stack_after_cond, cond_result) = pop(stack);
            let is_true = match cond_result {
                Value::Int(n) => n != 0,
                _ => panic!("until: condition must return Int, got {:?}", cond_result),
            };

            if is_true {
                // Condition is true, exit loop
                stack = stack_after_cond;
                break;
            }

            // Condition is false, continue loop
            stack = stack_after_cond;

            // Yield to scheduler after each iteration
            may::coroutine::yield_now();
        }

        stack
    }
}

/// Spawn a quotation or closure as a new strand (green thread)
///
/// Pops a quotation or closure from the stack and spawns it as a new strand.
/// - For Quotations: The quotation executes concurrently with an empty initial stack
/// - For Closures: The closure executes with its captured environment
///
/// Returns the strand ID.
///
/// Stack effect: ( ..a quot -- ..a strand_id )
/// where the quotation has effect ( -- )
///
/// # Safety
/// - Stack must have at least 1 value
/// - Top must be Quotation or Closure
/// - Function must be safe to execute on any thread
#[unsafe(no_mangle)]
pub unsafe extern "C" fn spawn(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation or closure
        let (stack, value) = pop(stack);

        match value {
            Value::Quotation(fn_ptr) => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("spawn: quotation function pointer is null");
                }

                // SAFETY: fn_ptr was created by the compiler's codegen and stored via push_quotation.
                // The compiler guarantees that quotation literals produce valid function pointers.
                // We've verified fn_ptr is non-null above.
                let fn_ref: extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);

                // Spawn the strand with null initial stack
                let strand_id = strand_spawn(fn_ref, std::ptr::null_mut());

                // Push strand ID back onto stack
                push(stack, Value::Int(strand_id))
            }
            Value::Closure { fn_ptr, env } => {
                // Validate function pointer is not null
                if fn_ptr == 0 {
                    panic!("spawn: closure function pointer is null");
                }

                // We need to pass the closure data to the spawned strand.
                // We use a registry with a unique ID (separate from strand_id).
                use std::sync::atomic::{AtomicI64, Ordering};
                static NEXT_CLOSURE_SPAWN_ID: AtomicI64 = AtomicI64::new(1);
                let closure_spawn_id = NEXT_CLOSURE_SPAWN_ID.fetch_add(1, Ordering::Relaxed);

                // Store closure data in registry
                {
                    let mut registry = SPAWN_CLOSURE_REGISTRY.lock().unwrap();
                    registry.insert(closure_spawn_id, (fn_ptr, env));
                }

                // Create initial stack with the closure_spawn_id
                let initial_stack = push(std::ptr::null_mut(), Value::Int(closure_spawn_id));

                // Spawn strand with trampoline
                let strand_id = strand_spawn(closure_spawn_trampoline, initial_stack);

                // Note: The trampoline will retrieve and remove the closure data from the registry

                // Push strand ID back onto stack
                push(stack, Value::Int(strand_id))
            }
            _ => panic!("spawn: expected Quotation or Closure, got {:?}", value),
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

    // Helper quotation: check if top value <= 0
    // Corresponds to: [ dup 0 <= ]
    unsafe extern "C" fn dup_lte_zero_quot(stack: Stack) -> Stack {
        unsafe {
            let stack = crate::stack::dup(stack);
            let stack = push_int(stack, 0);
            crate::arithmetic::lte(stack)
        }
    }

    #[test]
    fn test_until_countdown() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Countdown from 5 to 0 using until
            // [ 1 subtract ] [ dup 0 <= ] until
            let stack = push_int(stack, 5);

            // Push body: subtract 1
            let body_ptr = subtract_one_quot as usize;
            let stack = push_quotation(stack, body_ptr);

            // Push condition: dup 0 <=
            let cond_ptr = dup_lte_zero_quot as usize;
            let stack = push_quotation(stack, cond_ptr);

            // Execute until
            let stack = until_loop(stack);

            // Result should be 0
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_until_executes_at_least_once() {
        unsafe {
            let stack: Stack = std::ptr::null_mut();

            // Start with 0, so condition is immediately true, but body should execute once
            let stack = push_int(stack, 0);

            // Push body: subtract 1
            let body_ptr = subtract_one_quot as usize;
            let stack = push_quotation(stack, body_ptr);

            // Push condition: dup 0 <=  (will be true after first iteration)
            let cond_ptr = dup_lte_zero_quot as usize;
            let stack = push_quotation(stack, cond_ptr);

            // Execute until
            let stack = until_loop(stack);

            // Result should be -1 (body executed once)
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-1));
            assert!(stack.is_null());
        }
    }

    // Helper quotation for spawn test: does nothing, just completes
    unsafe extern "C" fn noop_quot(_stack: Stack) -> Stack {
        std::ptr::null_mut()
    }

    #[test]
    fn test_spawn_quotation() {
        unsafe {
            // Initialize scheduler
            crate::scheduler::scheduler_init();

            let stack: Stack = std::ptr::null_mut();

            // Push a quotation
            let fn_ptr = noop_quot as usize;
            let stack = push_quotation(stack, fn_ptr);

            // Spawn it
            let stack = spawn(stack);

            // Should have strand ID on stack
            let (stack, result) = pop(stack);
            match result {
                Value::Int(strand_id) => {
                    assert!(strand_id > 0, "Strand ID should be positive");
                }
                _ => panic!("Expected Int (strand ID), got {:?}", result),
            }
            assert!(stack.is_null());

            // Wait for strand to complete
            crate::scheduler::wait_all_strands();
        }
    }
}
