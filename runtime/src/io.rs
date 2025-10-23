//! I/O Operations for cem3
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
use std::io::{self, Write};

/// Valid exit code range for Unix compatibility
const EXIT_CODE_MIN: i64 = 0;
const EXIT_CODE_MAX: i64 = 255;

/// Write a string to stdout followed by a newline
///
/// Stack effect: ( str -- )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn write_line(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "write_line: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            println!("{}", s);
            io::stdout()
                .flush()
                .expect("write_line: failed to flush stdout (stdout may be closed or redirected)");
            rest
        }
        _ => panic!("write_line: expected String on stack, got {:?}", value),
    }
}

/// Read a line from stdin (strips trailing newline)
///
/// Stack effect: ( -- str )
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn read_line(stack: Stack) -> Stack {
    use std::io::BufRead;

    let stdin = io::stdin();
    let mut line = String::new();

    stdin
        .lock()
        .read_line(&mut line)
        .expect("read_line: failed to read from stdin (I/O error or EOF)");

    // Strip trailing newline(s)
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }

    unsafe { push(stack, Value::String(line)) }
}

/// Convert an integer to a string
///
/// Stack effect: ( Int -- String )
///
/// # Safety
/// Stack must have an Int value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn int_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "int_to_string: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(n) => unsafe { push(rest, Value::String(n.to_string())) },
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
pub unsafe extern "C" fn push_string(stack: Stack, c_str: *const i8) -> Stack {
    assert!(!c_str.is_null(), "push_string: null string pointer");

    let s = unsafe {
        CStr::from_ptr(c_str)
            .to_str()
            .expect("push_string: invalid UTF-8 in string literal")
            .to_owned()
    };

    unsafe { push(stack, Value::String(s)) }
}

/// Exit the program with a status code
///
/// Stack effect: ( exit_code -- )
///
/// # Safety
/// Stack must have an Int on top. Never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn exit_op(stack: Stack) -> ! {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use std::ffi::CString;

    #[test]
    fn test_write_line() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String("Hello, World!".to_string()));
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
            assert_eq!(value, Value::String("Test".to_string()));
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
            assert_eq!(value, Value::String(String::new()));
            assert!(stack.is_null());

            // Write empty string should work without panic
            let stack = push(stack, Value::String(String::new()));
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
            assert_eq!(value, Value::String("Hello, 世界! 🌍".to_string()));
            assert!(stack.is_null());
        }
    }
}
