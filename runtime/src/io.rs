//! I/O Operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # Safety Contract
//!
//! **IMPORTANT:** These functions are designed to be called ONLY by compiler-generated code,
//! not by end users or arbitrary C code. The compiler is responsible for:
//!
//! - Ensuring stack has correct types (verified by type checker)
//! - Passing valid, null-terminated C strings to `push_string`
//! - Never calling these functions directly from user code
//!
//! # String Handling
//!
//! String literals from the compiler must be valid UTF-8 C strings (null-terminated).
//! Currently, each string literal is allocated as an owned `String`. See
//! `docs/STRING_INTERNING_DESIGN.md` for discussion of future optimizations
//! (interning, static references, etc.).

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::ffi::CStr;
use std::io;
use std::sync::LazyLock;

/// Coroutine-aware stdout mutex.
/// Uses may::sync::Mutex which yields the coroutine when contended instead of blocking the OS thread.
/// By serializing access to stdout, we prevent RefCell borrow panics that occur when multiple
/// coroutines on the same thread try to access stdout's internal RefCell concurrently.
static STDOUT_MUTEX: LazyLock<may::sync::Mutex<()>> = LazyLock::new(|| may::sync::Mutex::new(()));

/// Valid exit code range for Unix compatibility
const EXIT_CODE_MIN: i64 = 0;
const EXIT_CODE_MAX: i64 = 255;

/// Write a string to stdout followed by a newline
///
/// Stack effect: ( str -- )
///
/// # Safety
/// Stack must have a String value on top
///
/// # Concurrency
/// Uses may::sync::Mutex to serialize stdout writes from multiple strands.
/// When the mutex is contended, the strand yields to the scheduler (doesn't block the OS thread).
/// This prevents RefCell borrow panics when multiple strands write concurrently.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_write_line(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "write_line: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            // Acquire coroutine-aware mutex (yields if contended, doesn't block)
            // This serializes access to stdout
            let _guard = STDOUT_MUTEX.lock().unwrap();

            // Write directly to fd 1 using libc to avoid Rust's std::io::stdout() RefCell.
            // Rust's standard I/O uses RefCell which panics on concurrent access from
            // multiple coroutines on the same thread.
            let str_slice = s.as_str();
            let newline = b"\n";
            unsafe {
                libc::write(
                    1,
                    str_slice.as_ptr() as *const libc::c_void,
                    str_slice.len(),
                );
                libc::write(1, newline.as_ptr() as *const libc::c_void, newline.len());
            }

            rest
        }
        _ => panic!("write_line: expected String on stack, got {:?}", value),
    }
}

/// Read a line from stdin (preserves newline characters)
///
/// Returns the line including trailing newline (\n or \r\n).
/// Returns empty string "" at EOF.
/// Use `string-chomp` to remove trailing newlines if needed.
///
/// Stack effect: ( -- str )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_read_line(stack: Stack) -> Stack {
    use std::io::BufRead;

    let stdin = io::stdin();
    let mut line = String::new();

    stdin
        .lock()
        .read_line(&mut line)
        .expect("read_line: failed to read from stdin (I/O error or EOF)");

    // Preserve newlines - callers can use string-chomp if needed
    unsafe { push(stack, Value::String(line.into())) }
}

/// Read a line from stdin with explicit EOF detection
///
/// Returns the line and a status flag:
/// - ( line 1 ) on success (line includes trailing newline)
/// - ( "" 0 ) at EOF
///
/// Stack effect: ( -- String Int )
///
/// The `+` suffix indicates this returns a result pattern (value + status).
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_read_line_plus(stack: Stack) -> Stack {
    use std::io::BufRead;

    let stdin = io::stdin();
    let mut line = String::new();

    let bytes_read = stdin
        .lock()
        .read_line(&mut line)
        .expect("read_line_safe: failed to read from stdin");

    // bytes_read == 0 means EOF
    let status = if bytes_read > 0 { 1i64 } else { 0i64 };

    let stack = unsafe { push(stack, Value::String(line.into())) };
    unsafe { push(stack, Value::Int(status)) }
}

/// Convert an integer to a string
///
/// Stack effect: ( Int -- String )
///
/// # Safety
/// Stack must have an Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "int_to_string: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(n) => unsafe { push(rest, Value::String(n.to_string().into())) },
        _ => panic!("int_to_string: expected Int on stack, got {:?}", value),
    }
}

/// Push a C string literal onto the stack (for compiler-generated code)
///
/// Stack effect: ( -- str )
///
/// # Safety
/// The c_str pointer must be valid and null-terminated
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_string(stack: Stack, c_str: *const i8) -> Stack {
    assert!(!c_str.is_null(), "push_string: null string pointer");

    let s = unsafe {
        CStr::from_ptr(c_str)
            .to_str()
            .expect("push_string: invalid UTF-8 in string literal")
            .to_owned()
    };

    unsafe { push(stack, Value::String(s.into())) }
}

/// Push a SeqString value onto the stack
///
/// This is used when we already have a SeqString (e.g., from closures).
/// Unlike push_string which takes a C string, this takes a SeqString by value.
///
/// Stack effect: ( -- String )
///
/// # Safety
/// The SeqString must be valid. This is only called from LLVM-generated code, not actual C code.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_seqstring(
    stack: Stack,
    seq_str: crate::seqstring::SeqString,
) -> Stack {
    unsafe { push(stack, Value::String(seq_str)) }
}

/// Exit the program with a status code
///
/// Stack effect: ( exit_code -- )
///
/// # Safety
/// Stack must have an Int on top. Never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_exit_op(stack: Stack) -> ! {
    assert!(!stack.is_null(), "exit_op: stack is empty");

    let (_rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(code) => {
            // Explicitly validate exit code is in Unix-compatible range
            if !(EXIT_CODE_MIN..=EXIT_CODE_MAX).contains(&code) {
                panic!(
                    "exit_op: exit code must be in range {}-{}, got {}",
                    EXIT_CODE_MIN, EXIT_CODE_MAX, code
                );
            }
            std::process::exit(code as i32);
        }
        _ => panic!("exit_op: expected Int on stack, got {:?}", value),
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_exit_op as exit_op;
pub use patch_seq_int_to_string as int_to_string;
pub use patch_seq_push_seqstring as push_seqstring;
pub use patch_seq_push_string as push_string;
pub use patch_seq_read_line as read_line;
pub use patch_seq_read_line_plus as read_line_plus;
pub use patch_seq_write_line as write_line;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use std::ffi::CString;

    #[test]
    fn test_write_line() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String("Hello, World!".into()));
            let _stack = write_line(stack);
        }
    }

    #[test]
    fn test_push_string() {
        unsafe {
            let stack = std::ptr::null_mut();
            let test_str = CString::new("Test").unwrap();
            let stack = push_string(stack, test_str.as_ptr());

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::String("Test".into()));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_empty_string() {
        unsafe {
            // Empty string should be handled correctly
            let stack = std::ptr::null_mut();
            let empty_str = CString::new("").unwrap();
            let stack = push_string(stack, empty_str.as_ptr());

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::String("".into()));
            assert!(stack.is_null());

            // Write empty string should work without panic
            let stack = push(stack, Value::String("".into()));
            let stack = write_line(stack);
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_unicode_strings() {
        unsafe {
            // Test that Unicode strings are handled correctly
            let stack = std::ptr::null_mut();
            let unicode_str = CString::new("Hello, 世界! 🌍").unwrap();
            let stack = push_string(stack, unicode_str.as_ptr());

            let (stack, value) = pop(stack);
            assert_eq!(value, Value::String("Hello, 世界! 🌍".into()));
            assert!(stack.is_null());
        }
    }
}
