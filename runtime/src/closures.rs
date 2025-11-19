//! Closure support for cem3
//!
//! Provides runtime functions for creating and managing closures (quotations with captured environments).
//!
//! A closure consists of:
//! - Function pointer (the compiled quotation code)
//! - Environment (boxed array of captured values)
//!
//! Note: These extern "C" functions use Value and slice pointers, which aren't technically FFI-safe,
//! but they work correctly when called from LLVM-generated code (not actual C interop).

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
///
/// # Safety
/// - env must be a valid pointer to a closure environment
/// - index must be in bounds [0, size)
// Allow improper_ctypes_definitions: Called from LLVM IR (not C), both sides understand layout
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_get(env: *const [Value], index: i32) -> Value {
    if env.is_null() {
        panic!("env_get: null environment pointer");
    }

    let env_slice = unsafe { &*env };
    let idx = index as usize;

    if idx >= env_slice.len() {
        panic!(
            "env_get: index {} out of bounds for environment of size {}",
            index,
            env_slice.len()
        );
    }

    // Clone the value from the environment
    env_slice[idx].clone()
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

        // Get values
        unsafe {
            assert_eq!(env_get(env, 0), Value::Int(42));
            assert_eq!(env_get(env, 1), Value::Bool(true));
            assert_eq!(env_get(env, 2), Value::Int(99));
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
}
