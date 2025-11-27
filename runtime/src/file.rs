//! File I/O Operations for Seq
//!
//! Provides file reading operations for Seq programs.
//!
//! # Usage from Seq
//!
//! ```seq
//! "config.json" file-slurp  # ( String -- String ) read entire file
//! "config.json" file-exists?  # ( String -- Int ) 1 if exists, 0 otherwise
//! ```
//!
//! # Example
//!
//! ```seq
//! : main ( -- Int )
//!   "config.json" file-exists? if
//!     "config.json" file-slurp write_line
//!   else
//!     "File not found" write_line
//!   then
//!   0
//! ;
//! ```

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::fs;
use std::path::Path;

/// Read entire file contents as a string
///
/// Stack effect: ( String -- String )
///
/// Takes a file path, reads the entire file, and returns its contents.
/// Panics if the file cannot be read (doesn't exist, no permission, not UTF-8, etc.)
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with a String value on top
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_slurp(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-slurp: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(path) => {
            let contents = fs::read_to_string(path.as_str()).unwrap_or_else(|e| {
                panic!("file-slurp: failed to read '{}': {}", path.as_str(), e)
            });

            unsafe { push(rest, Value::String(contents.into())) }
        }
        _ => panic!("file-slurp: expected String path on stack, got {:?}", value),
    }
}

/// Check if a file exists
///
/// Stack effect: ( String -- Int )
///
/// Takes a file path and returns 1 if the file exists, 0 otherwise.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with a String value on top
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_exists(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-exists?: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(path) => {
            let exists = if Path::new(path.as_str()).exists() {
                1i64
            } else {
                0i64
            };

            unsafe { push(rest, Value::Int(exists)) }
        }
        _ => panic!(
            "file-exists?: expected String path on stack, got {:?}",
            value
        ),
    }
}

// Public re-exports
pub use patch_seq_file_exists as file_exists;
pub use patch_seq_file_slurp as file_slurp;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_slurp() {
        // Create a temporary file with known contents
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Hello, file!").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            let (stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str().trim(), "Hello, file!"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_exists_true() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_exists(stack);

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_exists_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String("/nonexistent/path/to/file.txt".into()));
            let stack = patch_seq_file_exists(stack);

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_slurp_utf8() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "Hello, 世界! 🌍").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            let (stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str(), "Hello, 世界! 🌍"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_slurp_empty() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            let (stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }
}
