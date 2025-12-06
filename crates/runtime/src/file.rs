//! File I/O Operations for Seq
//!
//! Provides file reading operations for Seq programs.
//!
//! # Usage from Seq
//!
//! ```seq
//! "config.json" file-slurp  # ( String -- String ) read entire file
//! "config.json" file-exists?  # ( String -- Int ) 1 if exists, 0 otherwise
//! "data.txt" [ process-line ] file-for-each-line+  # ( String Quotation -- String Int )
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
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
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

/// Read entire file contents as a string, with error handling
///
/// Stack effect: ( String -- String Int )
///
/// Takes a file path, attempts to read the entire file.
/// Returns (contents 1) on success, or ("" 0) on failure.
/// Failure cases: file not found, permission denied, not valid UTF-8, etc.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with a String value on top
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_slurp_safe(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-slurp-safe: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(path) => match fs::read_to_string(path.as_str()) {
            Ok(contents) => {
                let stack = unsafe { push(rest, Value::String(contents.into())) };
                unsafe { push(stack, Value::Int(1)) }
            }
            Err(_) => {
                let stack = unsafe { push(rest, Value::String("".into())) };
                unsafe { push(stack, Value::Int(0)) }
            }
        },
        _ => panic!(
            "file-slurp-safe: expected String path on stack, got {:?}",
            value
        ),
    }
}

/// Process each line of a file with a quotation
///
/// Stack effect: ( String Quotation -- String Int )
///
/// Opens the file, calls the quotation with each line (including newline),
/// then closes the file.
///
/// Returns:
/// - Success: ( "" 1 )
/// - Error: ( "error message" 0 )
///
/// The quotation should have effect ( String -- ), receiving each line
/// and consuming it.
///
/// # Example
///
/// ```seq
/// "data.txt" [ string-chomp process-line ] file-for-each-line+
/// if
///     "Done processing" write_line
/// else
///     "Error: " swap string-concat write_line
/// then
/// ```
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be a Quotation or Closure
/// - Second on stack must be a String (file path)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_for_each_line_plus(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-for-each-line+: stack is empty");

    // Pop quotation
    let (stack, quot_value) = unsafe { pop(stack) };

    // Pop path
    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!(
            "file-for-each-line+: expected String path, got {:?}",
            path_value
        ),
    };

    // Open file
    let file = match File::open(path.as_str()) {
        Ok(f) => f,
        Err(e) => {
            // Return error: ( "error message" 0 )
            let stack = unsafe { push(stack, Value::String(format!("{}", e).into())) };
            return unsafe { push(stack, Value::Int(0)) };
        }
    };

    // Extract function pointer and optionally closure environment
    let (wrapper, env_data, env_len): (usize, *const Value, usize) = match quot_value {
        Value::Quotation { wrapper, .. } => {
            if wrapper == 0 {
                panic!("file-for-each-line+: quotation wrapper function pointer is null");
            }
            (wrapper, std::ptr::null(), 0)
        }
        Value::Closure { fn_ptr, ref env } => {
            if fn_ptr == 0 {
                panic!("file-for-each-line+: closure function pointer is null");
            }
            (fn_ptr, env.as_ptr(), env.len())
        }
        _ => panic!(
            "file-for-each-line+: expected Quotation or Closure, got {:?}",
            quot_value
        ),
    };

    // Read lines and call quotation/closure for each
    let reader = BufReader::new(file);
    let mut current_stack = stack;

    for line_result in reader.lines() {
        match line_result {
            Ok(mut line_str) => {
                // Add back newline to match read_line behavior
                line_str.push('\n');

                // Push line onto stack
                current_stack = unsafe { push(current_stack, Value::String(line_str.into())) };

                // Call the quotation or closure
                if env_data.is_null() {
                    // Quotation: just stack -> stack
                    let fn_ref: unsafe extern "C" fn(Stack) -> Stack =
                        unsafe { std::mem::transmute(wrapper) };
                    current_stack = unsafe { fn_ref(current_stack) };
                } else {
                    // Closure: stack, env_ptr, env_len -> stack
                    let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                        unsafe { std::mem::transmute(wrapper) };
                    current_stack = unsafe { fn_ref(current_stack, env_data, env_len) };
                }

                // Yield to scheduler for cooperative multitasking
                may::coroutine::yield_now();
            }
            Err(e) => {
                // I/O error mid-file
                let stack = unsafe { push(current_stack, Value::String(format!("{}", e).into())) };
                return unsafe { push(stack, Value::Int(0)) };
            }
        }
    }

    // Success: ( "" 1 )
    let stack = unsafe { push(current_stack, Value::String("".into())) };
    unsafe { push(stack, Value::Int(1)) }
}

// Public re-exports
pub use patch_seq_file_exists as file_exists;
pub use patch_seq_file_for_each_line_plus as file_for_each_line_plus;
pub use patch_seq_file_slurp as file_slurp;
pub use patch_seq_file_slurp_safe as file_slurp_safe;

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

    #[test]
    fn test_file_slurp_safe_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Safe read!").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp_safe(stack);

            let (stack, success) = pop(stack);
            let (stack, contents) = pop(stack);
            assert_eq!(success, Value::Int(1));
            match contents {
                Value::String(s) => assert_eq!(s.as_str().trim(), "Safe read!"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_slurp_safe_not_found() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String("/nonexistent/path/to/file.txt".into()));
            let stack = patch_seq_file_slurp_safe(stack);

            let (stack, success) = pop(stack);
            let (stack, contents) = pop(stack);
            assert_eq!(success, Value::Int(0));
            match contents {
                Value::String(s) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_file_slurp_safe_empty_file() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp_safe(stack);

            let (stack, success) = pop(stack);
            let (stack, contents) = pop(stack);
            assert_eq!(success, Value::Int(1)); // Empty file is still success
            match contents {
                Value::String(s) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }
}
