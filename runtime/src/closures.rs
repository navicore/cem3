//! Closure support for Seq
//!
//! Provides runtime functions for creating and managing closures (quotations with captured environments).
//!
//! A closure consists of:
//! - Function pointer (the compiled quotation code)
//! - Environment (boxed array of captured values)
//!
//! Note: These extern "C" functions use Value and slice pointers, which aren't technically FFI-safe,
//! but they work correctly when called from LLVM-generated code (not actual C interop).

use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Create a closure environment (array of captured values)
///
/// Called from generated LLVM code to allocate space for captured values.
/// Returns a raw pointer to a boxed slice that will be filled with values.
///
/// # Safety
/// - Caller must populate the environment with `env_set` before using
/// - Caller must eventually pass ownership to a Closure value (via `make_closure`)
// Allow improper_ctypes_definitions: Called from LLVM IR (not C), both sides understand layout
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub extern "C" fn create_env(size: i32) -> *mut [Value] {
    if size < 0 {
        panic!("create_env: size cannot be negative: {}", size);
    }

    let mut vec: Vec<Value> = Vec::with_capacity(size as usize);

    // Fill with placeholder values (will be replaced by env_set)
    for _ in 0..size {
        vec.push(Value::Int(0));
    }

    Box::into_raw(vec.into_boxed_slice())
}

/// Set a value in the closure environment
///
/// Called from generated LLVM code to populate captured values.
///
/// # Safety
/// - env must be a valid pointer from `create_env`
/// - index must be in bounds [0, size)
/// - env must not have been passed to `make_closure` yet
// Allow improper_ctypes_definitions: Called from LLVM IR (not C), both sides understand layout
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_set(env: *mut [Value], index: i32, value: Value) {
    if env.is_null() {
        panic!("env_set: null environment pointer");
    }

    let env_slice = unsafe { &mut *env };
    let idx = index as usize;

    if idx >= env_slice.len() {
        panic!(
            "env_set: index {} out of bounds for environment of size {}",
            index,
            env_slice.len()
        );
    }

    env_slice[idx] = value;
}

/// Get a value from the closure environment
///
/// Called from generated closure function code to access captured values.
/// Takes environment as separate data pointer and length (since LLVM can't handle fat pointers).
///
/// # Safety
/// - env_data must be a valid pointer to an array of Values
/// - env_len must match the actual array length
/// - index must be in bounds [0, env_len)
// Allow improper_ctypes_definitions: Called from LLVM IR (not C), both sides understand layout
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_get(env_data: *const Value, env_len: usize, index: i32) -> Value {
    if env_data.is_null() {
        panic!("env_get: null environment pointer");
    }

    let idx = index as usize;

    if idx >= env_len {
        panic!(
            "env_get: index {} out of bounds for environment of size {}",
            index, env_len
        );
    }

    // Clone the value from the environment
    unsafe { (*env_data.add(idx)).clone() }
}

/// Get an Int value from the closure environment
///
/// This is a type-specific helper that avoids passing large Value enums through LLVM IR.
/// Returns primitive i64 instead of Value to avoid FFI issues with by-value enum passing.
///
/// # Safety
/// - env_data must be a valid pointer to an array of Values
/// - env_len must match the actual array length
/// - index must be in bounds [0, env_len)
/// - The value at index must be Value::Int
///
/// # FFI Notes
/// This function is ONLY called from LLVM-generated code, not from external C code.
/// The signature is safe for LLVM IR but would be undefined behavior if called from C
/// with incorrect assumptions about type layout.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_get_int(env_data: *const Value, env_len: usize, index: i32) -> i64 {
    if env_data.is_null() {
        panic!("env_get_int: null environment pointer");
    }

    let idx = index as usize;

    if idx >= env_len {
        panic!(
            "env_get_int: index {} out of bounds for environment of size {}",
            index, env_len
        );
    }

    // Access the value at the index
    let value = unsafe { &*env_data.add(idx) };

    match value {
        Value::Int(n) => *n,
        _ => panic!(
            "env_get_int: expected Int at index {}, got {:?}",
            index, value
        ),
    }
}

/// Get a String value from the environment at the given index
///
/// # Safety
/// - env_data must be a valid pointer to an array of Values
/// - env_len must be the actual length of that array
/// - index must be within bounds
/// - The value at index must be a String
///
/// This function returns a SeqString by-value.
/// This is safe for FFI because it's only called from LLVM-generated code, not actual C code.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_get_string(
    env_data: *const Value,
    env_len: usize,
    index: i32,
) -> crate::seqstring::SeqString {
    if env_data.is_null() {
        panic!("env_get_string: null environment pointer");
    }

    let idx = index as usize;

    if idx >= env_len {
        panic!(
            "env_get_string: index {} out of bounds for environment of size {}",
            index, env_len
        );
    }

    // Access the value at the index
    let value = unsafe { &*env_data.add(idx) };

    match value {
        Value::String(s) => s.clone(),
        _ => panic!(
            "env_get_string: expected String at index {}, got {:?}",
            index, value
        ),
    }
}

/// Create a closure value from a function pointer and environment
///
/// Takes ownership of the environment (converts raw pointer back to Box).
///
/// # Safety
/// - fn_ptr must be a valid function pointer (will be transmuted when called)
/// - env must be a valid pointer from `create_env`, fully populated via `env_set`
/// - env ownership is transferred to the Closure value
// Allow improper_ctypes_definitions: Called from LLVM IR (not C), both sides understand layout
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn make_closure(fn_ptr: u64, env: *mut [Value]) -> Value {
    if fn_ptr == 0 {
        panic!("make_closure: null function pointer");
    }

    if env.is_null() {
        panic!("make_closure: null environment pointer");
    }

    // Take ownership of the environment
    let env_box = unsafe { Box::from_raw(env) };

    Value::Closure {
        fn_ptr: fn_ptr as usize,
        env: env_box,
    }
}

/// Create closure from function pointer and stack values (all-in-one helper)
///
/// Pops `capture_count` values from stack (top-down order), creates environment,
/// makes closure, and pushes it onto the stack.
///
/// This is a convenience function for LLVM codegen that handles the entire
/// closure creation process in one call.
///
/// # Safety
/// - fn_ptr must be a valid function pointer
/// - stack must have at least `capture_count` values
#[unsafe(no_mangle)]
pub unsafe extern "C" fn push_closure(mut stack: Stack, fn_ptr: u64, capture_count: i32) -> Stack {
    if fn_ptr == 0 {
        panic!("push_closure: null function pointer");
    }

    if capture_count < 0 {
        panic!(
            "push_closure: capture_count cannot be negative: {}",
            capture_count
        );
    }

    let count = capture_count as usize;

    // Pop values from stack (captures are in top-down order)
    let mut captures: Vec<Value> = Vec::with_capacity(count);
    for _ in 0..count {
        let (new_stack, value) = unsafe { pop(stack) };
        captures.push(value);
        stack = new_stack;
    }

    // Create closure value
    let closure = Value::Closure {
        fn_ptr: fn_ptr as usize,
        env: captures.into_boxed_slice(),
    };

    // Push onto stack
    unsafe { push(stack, closure) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_env() {
        let env = create_env(3);
        assert!(!env.is_null());

        // Clean up
        unsafe {
            let _ = Box::from_raw(env);
        }
    }

    #[test]
    fn test_env_set_and_get() {
        let env = create_env(3);

        // Set values
        unsafe {
            env_set(env, 0, Value::Int(42));
            env_set(env, 1, Value::Bool(true));
            env_set(env, 2, Value::Int(99));
        }

        // Get values (convert to data pointer + length)
        unsafe {
            let env_slice = &*env;
            let env_data = env_slice.as_ptr();
            let env_len = env_slice.len();
            assert_eq!(env_get(env_data, env_len, 0), Value::Int(42));
            assert_eq!(env_get(env_data, env_len, 1), Value::Bool(true));
            assert_eq!(env_get(env_data, env_len, 2), Value::Int(99));
        }

        // Clean up
        unsafe {
            let _ = Box::from_raw(env);
        }
    }

    #[test]
    fn test_make_closure() {
        let env = create_env(2);

        unsafe {
            env_set(env, 0, Value::Int(5));
            env_set(env, 1, Value::Int(10));

            let closure = make_closure(0x1234, env);

            match closure {
                Value::Closure { fn_ptr, env } => {
                    assert_eq!(fn_ptr, 0x1234);
                    assert_eq!(env.len(), 2);
                    assert_eq!(env[0], Value::Int(5));
                    assert_eq!(env[1], Value::Int(10));
                }
                _ => panic!("Expected Closure value"),
            }
        }
    }

    // Note: We don't test panic behavior for FFI functions as they use
    // extern "C" which cannot unwind. The functions will still panic at runtime
    // if called incorrectly, but we can't test that behavior with #[should_panic].

    #[test]
    fn test_push_closure() {
        use crate::stack::{pop, push};
        use crate::value::Value;

        // Create a stack with some values
        let mut stack = std::ptr::null_mut();
        stack = unsafe { push(stack, Value::Int(10)) };
        stack = unsafe { push(stack, Value::Int(5)) };

        // Create a closure that captures both values
        let fn_ptr = 0x1234;
        stack = unsafe { push_closure(stack, fn_ptr, 2) };

        // Pop the closure
        let (stack, closure_value) = unsafe { pop(stack) };

        // Verify it's a closure with correct captures
        match closure_value {
            Value::Closure { fn_ptr: fp, env } => {
                assert_eq!(fp, fn_ptr as usize);
                assert_eq!(env.len(), 2);
                assert_eq!(env[0], Value::Int(5)); // Top of stack
                assert_eq!(env[1], Value::Int(10)); // Second from top
            }
            _ => panic!("Expected Closure value, got {:?}", closure_value),
        }

        // Stack should be empty now
        assert!(stack.is_null());
    }

    #[test]
    fn test_push_closure_zero_captures() {
        use crate::stack::pop;
        use crate::value::Value;

        // Create empty stack
        let stack = std::ptr::null_mut();

        // Create a closure with no captures
        let fn_ptr = 0x5678;
        let stack = unsafe { push_closure(stack, fn_ptr, 0) };

        // Pop the closure
        let (stack, closure_value) = unsafe { pop(stack) };

        // Verify it's a closure with no captures
        match closure_value {
            Value::Closure { fn_ptr: fp, env } => {
                assert_eq!(fp, fn_ptr as usize);
                assert_eq!(env.len(), 0);
            }
            _ => panic!("Expected Closure value, got {:?}", closure_value),
        }

        // Stack should be empty
        assert!(stack.is_null());
    }
}
