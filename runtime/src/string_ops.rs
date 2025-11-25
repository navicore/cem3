//! String operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//!
//! # Design Decision: split Return Value
//!
//! `split` uses Option A (push parts + count):
//! - "a b c" " " split → "a" "b" "c" 3
//!
//! This is the simplest approach, requiring no new types.
//! The count allows the caller to know how many parts were pushed.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Split a string on a delimiter
///
/// Stack effect: ( str delim -- Variant )
///
/// Returns a Variant containing the split parts as fields.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_split(stack: Stack) -> Stack {
    use crate::value::VariantData;

    assert!(!stack.is_null(), "string_split: stack is empty");

    let (stack, delim_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_split: need two strings");
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, delim_val) {
        (Value::String(s), Value::String(d)) => {
            // Split and collect into Value::String instances
            let fields: Vec<Value> = s
                .as_str()
                .split(d.as_str())
                .map(|part| Value::String(global_string(part.to_owned())))
                .collect();

            // Create a Variant with tag 0 and the split parts as fields
            let variant = Value::Variant(Box::new(VariantData::new(0, fields)));

            unsafe { push(stack, variant) }
        }
        _ => panic!("string_split: expected two strings on stack"),
    }
}

/// Check if a string is empty
///
/// Stack effect: ( str -- bool )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_empty(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_empty: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let is_empty = s.as_str().is_empty();
            let result = if is_empty { 1 } else { 0 };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_empty: expected String on stack"),
    }
}

/// Check if a string contains a substring
///
/// Stack effect: ( str substring -- bool )
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_contains(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_contains: stack is empty");

    let (stack, substring_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_contains: need two strings");
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, substring_val) {
        (Value::String(s), Value::String(sub)) => {
            let contains = s.as_str().contains(sub.as_str());
            let result = if contains { 1 } else { 0 };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_contains: expected two strings on stack"),
    }
}

/// Check if a string starts with a prefix
///
/// Stack effect: ( str prefix -- bool )
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_starts_with(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_starts_with: stack is empty");

    let (stack, prefix_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_starts_with: need two strings");
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, prefix_val) {
        (Value::String(s), Value::String(prefix)) => {
            let starts = s.as_str().starts_with(prefix.as_str());
            let result = if starts { 1 } else { 0 };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_starts_with: expected two strings on stack"),
    }
}

/// Concatenate two strings
///
/// Stack effect: ( str1 str2 -- result )
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_concat(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_concat: stack is empty");

    let (stack, str2_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_concat: need two strings");
    let (stack, str1_val) = unsafe { pop(stack) };

    match (str1_val, str2_val) {
        (Value::String(s1), Value::String(s2)) => {
            let result = format!("{}{}", s1.as_str(), s2.as_str());
            unsafe { push(stack, Value::String(global_string(result))) }
        }
        _ => panic!("string_concat: expected two strings on stack"),
    }
}

/// Get the length of a string
///
/// Stack effect: ( str -- int )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_length(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_length: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let len = s.as_str().len() as i64;
            unsafe { push(stack, Value::Int(len)) }
        }
        _ => panic!("string_length: expected String on stack"),
    }
}

/// Trim whitespace from both ends of a string
///
/// Stack effect: ( str -- trimmed )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_trim(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_trim: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let trimmed = s.as_str().trim();
            unsafe { push(stack, Value::String(global_string(trimmed.to_owned()))) }
        }
        _ => panic!("string_trim: expected String on stack"),
    }
}

/// Convert a string to uppercase
///
/// Stack effect: ( str -- upper )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_upper(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_to_upper: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let upper = s.as_str().to_uppercase();
            unsafe { push(stack, Value::String(global_string(upper))) }
        }
        _ => panic!("string_to_upper: expected String on stack"),
    }
}

/// Convert a string to lowercase
///
/// Stack effect: ( str -- lower )
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_lower(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_to_lower: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let lower = s.as_str().to_lowercase();
            unsafe { push(stack, Value::String(global_string(lower))) }
        }
        _ => panic!("string_to_lower: expected String on stack"),
    }
}

/// Check if two strings are equal
///
/// Stack effect: ( str1 str2 -- bool )
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_equal(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_equal: stack is empty");

    let (stack, str2_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_equal: need two strings");
    let (stack, str1_val) = unsafe { pop(stack) };

    match (str1_val, str2_val) {
        (Value::String(s1), Value::String(s2)) => {
            let equal = s1.as_str() == s2.as_str();
            let result = if equal { 1 } else { 0 };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_equal: expected two strings on stack"),
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_string_concat as string_concat;
pub use patch_seq_string_contains as string_contains;
pub use patch_seq_string_empty as string_empty;
pub use patch_seq_string_equal as string_equal;
pub use patch_seq_string_length as string_length;
pub use patch_seq_string_split as string_split;
pub use patch_seq_string_starts_with as string_starts_with;
pub use patch_seq_string_to_lower as string_to_lower;
pub use patch_seq_string_to_upper as string_to_upper;
pub use patch_seq_string_trim as string_trim;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_split_simple() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("a b c".to_owned())));
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Should have a Variant with 3 fields: "a", "b", "c"
            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 0);
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::String(global_string("a".to_owned())));
                    assert_eq!(v.fields[1], Value::String(global_string("b".to_owned())));
                    assert_eq!(v.fields[2], Value::String(global_string("c".to_owned())));
                }
                _ => panic!("Expected Variant, got {:?}", result),
            }

            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_split_empty() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("".to_owned())));
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Empty string splits to one empty part
            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 0);
                    assert_eq!(v.fields.len(), 1);
                    assert_eq!(v.fields[0], Value::String(global_string("".to_owned())));
                }
                _ => panic!("Expected Variant, got {:?}", result),
            }

            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_empty_true() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_empty(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_empty_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_empty(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_contains_true() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_contains(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_contains_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("foo".to_owned())));

            let stack = string_contains(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_starts_with_true() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_starts_with(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_starts_with_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_starts_with(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_http_request_line_parsing() {
        // Real-world use case: Parse "GET /api/users HTTP/1.1"
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("GET /api/users HTTP/1.1".to_owned())),
            );
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Should have a Variant with 3 fields: "GET", "/api/users", "HTTP/1.1"
            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 0);
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::String(global_string("GET".to_owned())));
                    assert_eq!(
                        v.fields[1],
                        Value::String(global_string("/api/users".to_owned()))
                    );
                    assert_eq!(
                        v.fields[2],
                        Value::String(global_string("HTTP/1.1".to_owned()))
                    );
                }
                _ => panic!("Expected Variant, got {:?}", result),
            }

            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_path_routing() {
        // Real-world use case: Check if path starts with "/api/"
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("/api/users".to_owned())));
            let stack = push(stack, Value::String(global_string("/api/".to_owned())));

            let stack = string_starts_with(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_concat() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("Hello, ".to_owned())));
            let stack = push(stack, Value::String(global_string("World!".to_owned())));

            let stack = string_concat(stack);

            let (stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Hello, World!".to_owned()))
            );
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_length() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("Hello".to_owned())));

            let stack = string_length(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_length_empty() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_length(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_trim() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("  Hello, World!  ".to_owned())),
            );

            let stack = string_trim(stack);

            let (stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Hello, World!".to_owned()))
            );
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_to_upper() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("Hello, World!".to_owned())),
            );

            let stack = string_to_upper(stack);

            let (stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("HELLO, WORLD!".to_owned()))
            );
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_to_lower() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("Hello, World!".to_owned())),
            );

            let stack = string_to_lower(stack);

            let (stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("hello, world!".to_owned()))
            );
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_http_header_content_length() {
        // Real-world use case: Build "Content-Length: 42" header
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(
                stack,
                Value::String(global_string("Content-Length: ".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("42".to_owned())));

            let stack = string_concat(stack);

            let (stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Content-Length: 42".to_owned()))
            );
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_equal_true() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_equal(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_equal_false() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_equal(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_string_equal_empty_strings() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = push(stack, Value::String(global_string("".to_owned())));
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_equal(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }
}
