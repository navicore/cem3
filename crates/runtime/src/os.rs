//! OS operations for Seq
//!
//! Provides portable OS interaction primitives: environment variables,
//! paths, and system information.
//!
//! These functions are exported with C ABI for LLVM codegen to call.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Get an environment variable
///
/// Stack effect: ( name -- value success )
///
/// Returns the value and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack must have a String (variable name) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_getenv(stack: Stack) -> Stack {
    unsafe {
        let (stack, name_val) = pop(stack);
        let name = match name_val {
            Value::String(s) => s,
            _ => panic!(
                "getenv: expected String (name) on stack, got {:?}",
                name_val
            ),
        };

        match std::env::var(name.as_str()) {
            Ok(value) => {
                let stack = push(stack, Value::String(global_string(value)));
                push(stack, Value::Int(1)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // failure
            }
        }
    }
}

/// Get the user's home directory
///
/// Stack effect: ( -- path success )
///
/// Returns the path and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_home_dir(stack: Stack) -> Stack {
    unsafe {
        // Try HOME env var first (works on Unix and some Windows configs)
        if let Ok(home) = std::env::var("HOME") {
            let stack = push(stack, Value::String(global_string(home)));
            return push(stack, Value::Int(1));
        }

        // On Windows, try USERPROFILE
        #[cfg(windows)]
        if let Ok(home) = std::env::var("USERPROFILE") {
            let stack = push(stack, Value::String(global_string(home)));
            return push(stack, Value::Int(1));
        }

        // Fallback: return empty string with failure flag
        let stack = push(stack, Value::String(global_string(String::new())));
        push(stack, Value::Int(0))
    }
}

/// Get the current working directory
///
/// Stack effect: ( -- path success )
///
/// Returns the path and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_current_dir(stack: Stack) -> Stack {
    unsafe {
        match std::env::current_dir() {
            Ok(path) => {
                let path_str = path.to_string_lossy().into_owned();
                let stack = push(stack, Value::String(global_string(path_str)));
                push(stack, Value::Int(1)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // failure
            }
        }
    }
}
