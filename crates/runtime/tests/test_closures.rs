//! Unit tests for closure runtime operations
//!
//! These tests verify the low-level closure creation and access functions
//! to catch regressions in the FFI boundary.

use seq_runtime::Value;
use seq_runtime::closures::{create_env, env_get, env_get_int, env_get_string, env_set};

#[test]
fn test_create_env_valid_size() {
    unsafe {
        let env = create_env(3);
        assert!(!env.is_null());
        assert_eq!((&(*env)).len(), 3);

        // Verify all values are initialized to Int(0)
        for i in 0..3 {
            match &(*env)[i] {
                Value::Int(0) => {}
                other => panic!("Expected Int(0), got {:?}", other),
            }
        }
    }
}

#[test]
fn test_env_set_and_get_int() {
    unsafe {
        let env = create_env(5);

        // Set some Int values
        env_set(env, 0, Value::Int(42));
        env_set(env, 2, Value::Int(-100));
        env_set(env, 4, Value::Int(999));

        // Get them back
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 0), 42);
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 2), -100);
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 4), 999);

        // Verify untouched slots are still Int(0)
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 1), 0);
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 3), 0);
    }
}

#[test]
fn test_env_set_and_get_string() {
    use seq_runtime::seqstring::global_string;

    unsafe {
        let env = create_env(3);

        let str1 = global_string("hello".to_string());
        let str2 = global_string("world".to_string());

        env_set(env, 0, Value::String(str1.clone()));
        env_set(env, 2, Value::String(str2.clone()));

        // Get them back
        let ptr1 = env_get_string((*env).as_ptr(), (&(*env)).len(), 0);
        let ptr2 = env_get_string((*env).as_ptr(), (&(*env)).len(), 2);

        assert_eq!(ptr1, str1);
        assert_eq!(ptr2, str2);
    }
}

#[test]
fn test_env_get_generic() {
    use seq_runtime::seqstring::global_string;

    unsafe {
        let env = create_env(4);

        let test_string = global_string("test".to_string());

        env_set(env, 0, Value::Int(123));
        env_set(env, 1, Value::String(test_string.clone()));
        env_set(env, 2, Value::Bool(true));

        // Generic get returns clones
        match env_get((*env).as_ptr(), (&(*env)).len(), 0) {
            Value::Int(123) => {}
            other => panic!("Expected Int(123), got {:?}", other),
        }

        match env_get((*env).as_ptr(), (&(*env)).len(), 1) {
            Value::String(ptr) if ptr == test_string => {}
            other => panic!("Expected String, got {:?}", other),
        }

        match env_get((*env).as_ptr(), (&(*env)).len(), 2) {
            Value::Bool(true) => {}
            other => panic!("Expected Bool(true), got {:?}", other),
        }
    }
}

#[test]
fn test_env_set_overwrites_previous_value() {
    unsafe {
        let env = create_env(2);

        // Set initial value
        env_set(env, 0, Value::Int(42));
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 0), 42);

        // Overwrite it
        env_set(env, 0, Value::Int(999));
        assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), 0), 999);
    }
}

// Note: We don't test panic behavior for FFI functions as they use
// extern "C" which cannot unwind. The functions will still panic at runtime
// if called incorrectly, but we can't test that behavior with #[should_panic].

#[test]
fn test_create_env_size_zero() {
    unsafe {
        let env = create_env(0);
        assert!(!env.is_null());
        assert_eq!((&(*env)).len(), 0);
    }
}

#[test]
fn test_large_environment() {
    unsafe {
        let size = 100;
        let env = create_env(size);

        // Fill with sequential values
        for i in 0..size {
            env_set(env, i, Value::Int(i as i64));
        }

        // Verify all values
        for i in 0..size {
            assert_eq!(env_get_int((*env).as_ptr(), (&(*env)).len(), i), i as i64);
        }
    }
}

#[test]
fn test_env_clone_on_access() {
    use seq_runtime::seqstring::global_string;

    unsafe {
        let env = create_env(2);
        let original = global_string("original".to_string());

        env_set(env, 0, Value::String(original.clone()));

        // env_get returns a clone
        let cloned = env_get((*env).as_ptr(), (&(*env)).len(), 0);

        match cloned {
            Value::String(ptr) => assert_eq!(ptr, original),
            _ => panic!("Expected String"),
        }

        // Original still in env
        let still_there = env_get_string((*env).as_ptr(), (&(*env)).len(), 0);
        assert_eq!(still_there, original);
    }
}
