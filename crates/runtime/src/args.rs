//! Command-line argument handling for Seq
//!
//! Provides access to command-line arguments passed to the program.
//!
//! # Usage from Seq
//!
//! ```seq
//! arg-count  # ( -- Int ) number of arguments (including program name)
//! 0 arg      # ( Int -- String ) get argument at index
//! ```
//!
//! # Example
//!
//! ```seq
//! : main ( -- Int )
//!   arg-count 1 > if
//!     1 arg write_line  # Print first argument (after program name)
//!   else
//!     "No arguments provided" write_line
//!   then
//!   0
//! ;
//! ```

use crate::stack::{push, Stack};
use crate::value::Value;
use std::ffi::CStr;
use std::sync::OnceLock;

/// Global storage for command-line arguments
static ARGS: OnceLock<Vec<String>> = OnceLock::new();

/// Initialize command-line arguments from C-style argc/argv
///
/// Called once at program startup from main() before any Seq code runs.
///
/// # Safety
/// - argc must accurately reflect the number of pointers in argv
/// - argv must contain argc valid, null-terminated C strings
/// - argv pointers must remain valid for the duration of this call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_args_init(argc: i32, argv: *const *const i8) {
    let args: Vec<String> = (0..argc)
        .map(|i| {
            let ptr = unsafe { *argv.offset(i as isize) };
            if ptr.is_null() {
                String::new()
            } else {
                unsafe { CStr::from_ptr(ptr).to_str().unwrap_or("").to_owned() }
            }
        })
        .collect();

    // Set once - ignore if already set (shouldn't happen in normal use)
    let _ = ARGS.set(args);
}

/// Get the number of command-line arguments
///
/// Stack effect: ( -- Int )
///
/// Returns the total count including the program name (argv\[0\]).
/// A program run with no arguments returns 1.
///
/// # Safety
/// - `stack` must be a valid stack pointer (may be null for empty stack)
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_arg_count(stack: Stack) -> Stack {
    let count = ARGS.get().map(|a| a.len()).unwrap_or(0) as i64;
    unsafe { push(stack, Value::Int(count)) }
}

/// Get command-line argument at index
///
/// Stack effect: ( Int -- String )
///
/// Index 0 is the program name. Returns empty string if index is out of bounds.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with at least one Int value
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_arg_at(stack: Stack) -> Stack {
    use crate::stack::pop;

    assert!(!stack.is_null(), "arg: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(idx) => {
            // Validate index is non-negative
            if idx < 0 {
                panic!("arg: index must be non-negative, got {}", idx);
            }

            let arg = ARGS
                .get()
                .and_then(|args| args.get(idx as usize))
                .cloned()
                .unwrap_or_default();

            unsafe { push(rest, Value::String(arg.into())) }
        }
        _ => panic!("arg: expected Int index on stack, got {:?}", value),
    }
}

// Public re-exports
pub use patch_seq_arg_at as arg_at;
pub use patch_seq_arg_count as arg_count;
pub use patch_seq_args_init as args_init;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::pop;
    use crate::tagged_stack::StackValue;

    #[test]
    fn test_arg_count_no_init() {
        // Before init, should return 0
        // Note: Can't really test this in isolation since OnceLock is global
        // This test mainly verifies the function doesn't crash
        unsafe {
            // Allocate a small stack buffer
            let mut buffer: [StackValue; 16] = std::mem::zeroed();
            let stack = buffer.as_mut_ptr();
            let stack = patch_seq_arg_count(stack);
            let (_, value) = pop(stack);
            // Could be 0 or whatever was set by previous test
            assert!(matches!(value, Value::Int(_)));
        }
    }
}
