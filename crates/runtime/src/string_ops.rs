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

use crate::error::set_runtime_error;
use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::sync::Arc;

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

            // Create a Variant with :List tag and the split parts as fields
            let variant = Value::Variant(Arc::new(VariantData::new(
                global_string("List".to_string()),
                fields,
            )));

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
            unsafe { push(stack, Value::Bool(is_empty)) }
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
            unsafe { push(stack, Value::Bool(contains)) }
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
            unsafe { push(stack, Value::Bool(starts)) }
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

/// Get the length of a string in Unicode characters (code points)
///
/// Stack effect: ( str -- int )
///
/// Note: This returns character count, not byte count.
/// For UTF-8 byte length (e.g., HTTP Content-Length), use `string-byte-length`.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_length(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_length: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let len = s.as_str().chars().count() as i64;
            unsafe { push(stack, Value::Int(len)) }
        }
        _ => panic!("string_length: expected String on stack"),
    }
}

/// Get the byte length of a string (UTF-8 encoded)
///
/// Stack effect: ( str -- int )
///
/// Use this for HTTP Content-Length headers and buffer allocation.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_byte_length(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_byte_length: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let len = s.as_str().len() as i64;
            unsafe { push(stack, Value::Int(len)) }
        }
        _ => panic!("string_byte_length: expected String on stack"),
    }
}

/// Get the Unicode code point at a character index
///
/// Stack effect: ( str int -- int )
///
/// Returns the code point value at the given character index.
/// Returns -1 if index is out of bounds.
///
/// # Safety
/// Stack must have a String and Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_char_at(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_char_at: stack is empty");

    let (stack, index_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_char_at: need string and index");
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, index_val) {
        (Value::String(s), Value::Int(index)) => {
            let result = if index < 0 {
                -1
            } else {
                s.as_str()
                    .chars()
                    .nth(index as usize)
                    .map(|c| c as i64)
                    .unwrap_or(-1)
            };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_char_at: expected String and Int on stack"),
    }
}

/// Extract a substring using character indices
///
/// Stack effect: ( str start len -- str )
///
/// Arguments:
/// - str: The source string
/// - start: Starting character index
/// - len: Number of characters to extract
///
/// Edge cases:
/// - Start beyond end: returns empty string
/// - Length extends past end: clamps to available
///
/// # Safety
/// Stack must have String, Int, Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_substring(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_substring: stack is empty");

    let (stack, len_val) = unsafe { pop(stack) };
    assert!(
        !stack.is_null(),
        "string_substring: need string, start, len"
    );
    let (stack, start_val) = unsafe { pop(stack) };
    assert!(
        !stack.is_null(),
        "string_substring: need string, start, len"
    );
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, start_val, len_val) {
        (Value::String(s), Value::Int(start), Value::Int(len)) => {
            let result = if start < 0 || len < 0 {
                String::new()
            } else {
                s.as_str()
                    .chars()
                    .skip(start as usize)
                    .take(len as usize)
                    .collect()
            };
            unsafe { push(stack, Value::String(global_string(result))) }
        }
        _ => panic!("string_substring: expected String, Int, Int on stack"),
    }
}

/// Convert a Unicode code point to a single-character string
///
/// Stack effect: ( int -- str )
///
/// Creates a string containing the single character represented by the code point.
/// Panics if the code point is invalid.
///
/// # Safety
/// Stack must have an Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_char_to_string(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "char_to_string: stack is empty");

    let (stack, code_point_val) = unsafe { pop(stack) };

    match code_point_val {
        Value::Int(code_point) => {
            let result = if !(0..=0x10FFFF).contains(&code_point) {
                // Invalid code point - return empty string
                String::new()
            } else {
                match char::from_u32(code_point as u32) {
                    Some(c) => c.to_string(),
                    None => String::new(), // Invalid code point (e.g., surrogate)
                }
            };
            unsafe { push(stack, Value::String(global_string(result))) }
        }
        _ => panic!("char_to_string: expected Int on stack"),
    }
}

/// Find the first occurrence of a substring
///
/// Stack effect: ( str needle -- int )
///
/// Returns the character index of the first occurrence of needle in str.
/// Returns -1 if not found.
///
/// # Safety
/// Stack must have two Strings on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_find(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_find: stack is empty");

    let (stack, needle_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "string_find: need string and needle");
    let (stack, str_val) = unsafe { pop(stack) };

    match (str_val, needle_val) {
        (Value::String(haystack), Value::String(needle)) => {
            let haystack_str = haystack.as_str();
            let needle_str = needle.as_str();

            // Find byte position then convert to character position
            let result = match haystack_str.find(needle_str) {
                Some(byte_pos) => {
                    // Count characters up to byte_pos
                    haystack_str[..byte_pos].chars().count() as i64
                }
                None => -1,
            };
            unsafe { push(stack, Value::Int(result)) }
        }
        _ => panic!("string_find: expected two Strings on stack"),
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
            unsafe { push(stack, Value::Bool(equal)) }
        }
        _ => panic!("string_equal: expected two strings on stack"),
    }
}

/// Compare two symbols for equality
///
/// Stack effect: ( Symbol Symbol -- Bool )
///
/// Optimization (Issue #166): If both symbols are interned (capacity=0),
/// we use O(1) pointer comparison instead of O(n) string comparison.
///
/// # Safety
/// Stack must have two Symbol values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_symbol_equal(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "symbol_equal: stack is empty");

    let (stack, sym2_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "symbol_equal: need two symbols");
    let (stack, sym1_val) = unsafe { pop(stack) };

    match (sym1_val, sym2_val) {
        (Value::Symbol(s1), Value::Symbol(s2)) => {
            // Fast path: both interned symbols -> O(1) pointer comparison
            let equal = if s1.is_interned() && s2.is_interned() {
                s1.as_ptr() == s2.as_ptr()
            } else {
                // Fallback: string comparison for runtime-created symbols
                s1.as_str() == s2.as_str()
            };
            unsafe { push(stack, Value::Bool(equal)) }
        }
        _ => panic!("symbol_equal: expected two symbols on stack"),
    }
}

/// Escape a string for JSON output
///
/// Stack effect: ( str -- str )
///
/// Escapes special characters according to JSON spec:
/// - `"` → `\"`
/// - `\` → `\\`
/// - newline → `\n`
/// - carriage return → `\r`
/// - tab → `\t`
/// - backspace → `\b`
/// - form feed → `\f`
/// - Control characters (0x00-0x1F) → `\uXXXX`
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_json_escape(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "json_escape: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::String(s) => {
            let input = s.as_str();
            let mut result = String::with_capacity(input.len() + 16);

            for ch in input.chars() {
                match ch {
                    '"' => result.push_str("\\\""),
                    '\\' => result.push_str("\\\\"),
                    '\n' => result.push_str("\\n"),
                    '\r' => result.push_str("\\r"),
                    '\t' => result.push_str("\\t"),
                    '\x08' => result.push_str("\\b"), // backspace
                    '\x0C' => result.push_str("\\f"), // form feed
                    // Control characters (0x00-0x1F except those handled above)
                    // RFC 8259 uses uppercase hex in examples for Unicode escapes
                    c if c.is_control() => {
                        result.push_str(&format!("\\u{:04X}", c as u32));
                    }
                    c => result.push(c),
                }
            }

            unsafe { push(stack, Value::String(global_string(result))) }
        }
        _ => panic!("json_escape: expected String on stack"),
    }
}

/// Convert String to Int: ( String -- Int Bool )
/// Returns the parsed int and true on success, or 0 and false on failure.
/// Accepts integers in range [-9223372036854775808, 9223372036854775807] (i64).
/// Trims leading/trailing whitespace before parsing.
/// Leading zeros are accepted (e.g., "007" parses to 7).
///
/// # Error Handling
/// - Empty stack: Sets runtime error, returns unchanged stack
/// - Type mismatch: Sets runtime error, returns 0 and false
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_int(stack: Stack) -> Stack {
    if stack.is_null() {
        set_runtime_error("string->int: stack is empty");
        return stack;
    }
    let (stack, val) = unsafe { pop(stack) };

    match val {
        Value::String(s) => match s.as_str().trim().parse::<i64>() {
            Ok(i) => {
                let stack = unsafe { push(stack, Value::Int(i)) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            Err(_) => {
                let stack = unsafe { push(stack, Value::Int(0)) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => {
            set_runtime_error("string->int: expected String on stack");
            let stack = unsafe { push(stack, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

/// Remove trailing newline characters from a string
///
/// Stack effect: ( str -- str )
///
/// Removes trailing \n or \r\n (handles both Unix and Windows line endings).
/// If the string doesn't end with a newline, returns it unchanged.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_chomp(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "string_chomp: stack is empty");

    let (stack, str_val) = unsafe { pop(stack) };

    match str_val {
        Value::String(s) => {
            let mut result = s.as_str().to_owned();
            if result.ends_with('\n') {
                result.pop();
                if result.ends_with('\r') {
                    result.pop();
                }
            }
            unsafe { push(stack, Value::String(global_string(result))) }
        }
        _ => panic!("string_chomp: expected String on stack"),
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_char_to_string as char_to_string;
pub use patch_seq_json_escape as json_escape;
pub use patch_seq_string_byte_length as string_byte_length;
pub use patch_seq_string_char_at as string_char_at;
pub use patch_seq_string_chomp as string_chomp;
pub use patch_seq_string_concat as string_concat;
pub use patch_seq_string_contains as string_contains;
pub use patch_seq_string_empty as string_empty;
pub use patch_seq_string_equal as string_equal;
pub use patch_seq_string_find as string_find;
pub use patch_seq_string_length as string_length;
pub use patch_seq_string_split as string_split;
pub use patch_seq_string_starts_with as string_starts_with;
pub use patch_seq_string_substring as string_substring;
pub use patch_seq_string_to_int as string_to_int;
pub use patch_seq_string_to_lower as string_to_lower;
pub use patch_seq_string_to_upper as string_to_upper;
pub use patch_seq_string_trim as string_trim;

// ============================================================================
// FFI String Helpers
// ============================================================================

/// Convert a Seq String on the stack to a null-terminated C string.
///
/// The returned pointer must be freed by the caller using free().
/// This peeks the string from the stack (caller pops after use).
///
/// Stack effect: ( String -- ) returns ptr to C string
///
/// # Memory Safety
///
/// The returned C string is a **completely independent copy** allocated via
/// `malloc()`. It has no connection to Seq's memory management:
///
/// - The Seq String on the stack remains valid and unchanged
/// - The returned pointer is owned by the C world and must be freed with `free()`
/// - Even if the Seq String is garbage collected, the C string remains valid
/// - Multiple calls with the same Seq String produce independent C strings
///
/// This design ensures FFI calls cannot cause use-after-free or double-free
/// bugs between Seq and C code.
///
/// # Safety
/// Stack must have a String value on top. The unused second argument
/// exists for future extension (passing output buffer).
#[cfg(not(feature = "nanbox"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_cstring(stack: Stack, _out: *mut u8) -> *mut u8 {
    assert!(!stack.is_null(), "string_to_cstring: stack is empty");

    use crate::stack::{DISC_STRING, peek_sv};

    // Peek the string value (don't pop - caller will pop after we return)
    let sv = unsafe { peek_sv(stack) };
    if sv.slot0 != DISC_STRING {
        panic!(
            "string_to_cstring: expected String on stack, got discriminant {}",
            sv.slot0
        );
    }

    // Extract string data from StackValue slots
    let str_ptr = sv.slot1 as *const u8;
    let len = sv.slot2 as usize;

    // Guard against overflow: len + 1 for null terminator
    let alloc_size = len.checked_add(1).unwrap_or_else(|| {
        panic!(
            "string_to_cstring: string too large for C conversion (len={})",
            len
        )
    });

    // Allocate space for string + null terminator
    let ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
    if ptr.is_null() {
        panic!("string_to_cstring: malloc failed");
    }

    // Copy string data
    unsafe {
        std::ptr::copy_nonoverlapping(str_ptr, ptr, len);
        // Add null terminator
        *ptr.add(len) = 0;
    }

    ptr
}

/// Convert the String on top of stack to a null-terminated C string (NaN-boxed version)
///
/// # Safety
/// Stack must have a String value on top. The unused second argument
/// exists for future extension (passing output buffer).
#[cfg(feature = "nanbox")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_to_cstring(stack: Stack, _out: *mut u8) -> *mut u8 {
    assert!(!stack.is_null(), "string_to_cstring: stack is empty");

    use crate::stack::peek_sv;

    // Peek the string value (don't pop - caller will pop after we return)
    let sv = unsafe { peek_sv(stack) };
    if !sv.0.is_string() {
        panic!("string_to_cstring: expected String on stack");
    }

    // Get the SeqString pointer from the NanBoxedValue
    let seq_str_ptr = unsafe { sv.0.as_string_ptr() };
    let seq_str = unsafe { &*seq_str_ptr };
    let str_ptr = seq_str.as_ptr();
    let len = seq_str.len();

    // Guard against overflow: len + 1 for null terminator
    let alloc_size = len.checked_add(1).unwrap_or_else(|| {
        panic!(
            "string_to_cstring: string too large for C conversion (len={})",
            len
        )
    });

    // Allocate space for string + null terminator
    let ptr = unsafe { libc::malloc(alloc_size) as *mut u8 };
    if ptr.is_null() {
        panic!("string_to_cstring: malloc failed");
    }

    // Copy string data
    unsafe {
        std::ptr::copy_nonoverlapping(str_ptr, ptr, len);
        // Add null terminator
        *ptr.add(len) = 0;
    }

    ptr
}

/// Convert a null-terminated C string to a Seq String and push onto stack.
///
/// The C string is NOT freed by this function.
///
/// Stack effect: ( -- String )
///
/// # Safety
/// cstr must be a valid null-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_cstring_to_string(stack: Stack, cstr: *const u8) -> Stack {
    if cstr.is_null() {
        // NULL string - push empty string
        return unsafe { push(stack, Value::String(global_string(String::new()))) };
    }

    // Get string length
    let len = unsafe { libc::strlen(cstr as *const libc::c_char) };

    // Create Rust string from C string
    let slice = unsafe { std::slice::from_raw_parts(cstr, len) };
    let s = String::from_utf8_lossy(slice).into_owned();

    unsafe { push(stack, Value::String(global_string(s))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_split_simple() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("a b c".to_owned())));
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Should have a Variant with 3 fields: "a", "b", "c"
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::String(global_string("a".to_owned())));
                    assert_eq!(v.fields[1], Value::String(global_string("b".to_owned())));
                    assert_eq!(v.fields[2], Value::String(global_string("c".to_owned())));
                }
                _ => panic!("Expected Variant, got {:?}", result),
            }
        }
    }

    #[test]
    fn test_string_split_empty() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Empty string splits to one empty part
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
                    assert_eq!(v.fields.len(), 1);
                    assert_eq!(v.fields[0], Value::String(global_string("".to_owned())));
                }
                _ => panic!("Expected Variant, got {:?}", result),
            }
        }
    }

    #[test]
    fn test_string_empty_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_empty(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_string_empty_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_empty(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(false));
        }
    }

    #[test]
    fn test_string_contains_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_contains(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_string_contains_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("foo".to_owned())));

            let stack = string_contains(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(false));
        }
    }

    #[test]
    fn test_string_starts_with_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_starts_with(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_string_starts_with_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_starts_with(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(false));
        }
    }

    #[test]
    fn test_http_request_line_parsing() {
        // Real-world use case: Parse "GET /api/users HTTP/1.1"
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("GET /api/users HTTP/1.1".to_owned())),
            );
            let stack = push(stack, Value::String(global_string(" ".to_owned())));

            let stack = string_split(stack);

            // Should have a Variant with 3 fields: "GET", "/api/users", "HTTP/1.1"
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
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
        }
    }

    #[test]
    fn test_path_routing() {
        // Real-world use case: Check if path starts with "/api/"
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("/api/users".to_owned())));
            let stack = push(stack, Value::String(global_string("/api/".to_owned())));

            let stack = string_starts_with(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_string_concat() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("Hello, ".to_owned())));
            let stack = push(stack, Value::String(global_string("World!".to_owned())));

            let stack = string_concat(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Hello, World!".to_owned()))
            );
        }
    }

    #[test]
    fn test_string_length() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("Hello".to_owned())));

            let stack = string_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5));
        }
    }

    #[test]
    fn test_string_length_empty() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_string_trim() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("  Hello, World!  ".to_owned())),
            );

            let stack = string_trim(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Hello, World!".to_owned()))
            );
        }
    }

    #[test]
    fn test_string_to_upper() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("Hello, World!".to_owned())),
            );

            let stack = string_to_upper(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("HELLO, WORLD!".to_owned()))
            );
        }
    }

    #[test]
    fn test_string_to_lower() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("Hello, World!".to_owned())),
            );

            let stack = string_to_lower(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("hello, world!".to_owned()))
            );
        }
    }

    #[test]
    fn test_http_header_content_length() {
        // Real-world use case: Build "Content-Length: 42" header
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("Content-Length: ".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("42".to_owned())));

            let stack = string_concat(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Content-Length: 42".to_owned()))
            );
        }
    }

    #[test]
    fn test_string_equal_true() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_equal(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    #[test]
    fn test_string_equal_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_equal(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(false));
        }
    }

    #[test]
    fn test_string_equal_empty_strings() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_equal(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Bool(true));
        }
    }

    // UTF-8 String Primitives Tests

    #[test]
    fn test_string_length_utf8() {
        // "héllo" has 5 characters but 6 bytes (é is 2 bytes in UTF-8)
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("héllo".to_owned())));

            let stack = string_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5)); // Characters, not bytes
        }
    }

    #[test]
    fn test_string_length_emoji() {
        // Emoji is one code point but multiple bytes
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hi🎉".to_owned())));

            let stack = string_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3)); // 'h', 'i', and emoji
        }
    }

    #[test]
    fn test_string_byte_length_ascii() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));

            let stack = string_byte_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(5)); // Same as char length for ASCII
        }
    }

    #[test]
    fn test_string_byte_length_utf8() {
        // "héllo" has 5 characters but 6 bytes
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("héllo".to_owned())));

            let stack = string_byte_length(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(6)); // Bytes, not characters
        }
    }

    #[test]
    fn test_string_char_at_ascii() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(0));

            let stack = string_char_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(104)); // 'h' = 104
        }
    }

    #[test]
    fn test_string_char_at_utf8() {
        // Get the é character at index 1 in "héllo"
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("héllo".to_owned())));
            let stack = push(stack, Value::Int(1));

            let stack = string_char_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(233)); // 'é' = U+00E9 = 233
        }
    }

    #[test]
    fn test_string_char_at_out_of_bounds() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(10)); // Out of bounds

            let stack = string_char_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-1));
        }
    }

    #[test]
    fn test_string_char_at_negative() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(-1));

            let stack = string_char_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-1));
        }
    }

    #[test]
    fn test_string_substring_simple() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(1)); // start
            let stack = push(stack, Value::Int(3)); // len

            let stack = string_substring(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("ell".to_owned())));
        }
    }

    #[test]
    fn test_string_substring_utf8() {
        // "héllo" - get "éll" (characters 1-3)
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("héllo".to_owned())));
            let stack = push(stack, Value::Int(1)); // start
            let stack = push(stack, Value::Int(3)); // len

            let stack = string_substring(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("éll".to_owned())));
        }
    }

    #[test]
    fn test_string_substring_clamp() {
        // Request more than available - should clamp
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(2)); // start
            let stack = push(stack, Value::Int(100)); // len (way too long)

            let stack = string_substring(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("llo".to_owned())));
        }
    }

    #[test]
    fn test_string_substring_beyond_end() {
        // Start beyond end - returns empty
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::Int(10)); // start (beyond end)
            let stack = push(stack, Value::Int(3)); // len

            let stack = string_substring(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("".to_owned())));
        }
    }

    #[test]
    fn test_char_to_string_ascii() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(65)); // 'A'

            let stack = char_to_string(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("A".to_owned())));
        }
    }

    #[test]
    fn test_char_to_string_utf8() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(233)); // 'é' = U+00E9

            let stack = char_to_string(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("é".to_owned())));
        }
    }

    #[test]
    fn test_char_to_string_newline() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(10)); // '\n'

            let stack = char_to_string(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("\n".to_owned())));
        }
    }

    #[test]
    fn test_char_to_string_invalid() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(-1)); // Invalid

            let stack = char_to_string(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("".to_owned())));
        }
    }

    #[test]
    fn test_string_find_found() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("world".to_owned())));

            let stack = string_find(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(6)); // "world" starts at index 6
        }
    }

    #[test]
    fn test_string_find_not_found() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello world".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("xyz".to_owned())));

            let stack = string_find(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(-1));
        }
    }

    #[test]
    fn test_string_find_first_match() {
        // Should return first occurrence
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("hello".to_owned())));
            let stack = push(stack, Value::String(global_string("l".to_owned())));

            let stack = string_find(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(2)); // First 'l' is at index 2
        }
    }

    #[test]
    fn test_string_find_utf8() {
        // Find in UTF-8 string - returns character index, not byte index
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("héllo wörld".to_owned())),
            );
            let stack = push(stack, Value::String(global_string("wörld".to_owned())));

            let stack = string_find(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(6)); // Character index, not byte index
        }
    }

    // JSON Escape Tests

    #[test]
    fn test_json_escape_quotes() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("hello \"world\"".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("hello \\\"world\\\"".to_owned()))
            );
        }
    }

    #[test]
    fn test_json_escape_backslash() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("path\\to\\file".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("path\\\\to\\\\file".to_owned()))
            );
        }
    }

    #[test]
    fn test_json_escape_newline_tab() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("line1\nline2\ttabbed".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("line1\\nline2\\ttabbed".to_owned()))
            );
        }
    }

    #[test]
    fn test_json_escape_carriage_return() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("line1\r\nline2".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("line1\\r\\nline2".to_owned()))
            );
        }
    }

    #[test]
    fn test_json_escape_control_chars() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            // Test backspace (0x08) and form feed (0x0C)
            let stack = push(
                stack,
                Value::String(global_string("a\x08b\x0Cc".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("a\\bb\\fc".to_owned())));
        }
    }

    #[test]
    fn test_json_escape_unicode_control() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            // Test null character (0x00) - should be escaped as \u0000 (uppercase hex per RFC 8259)
            let stack = push(stack, Value::String(global_string("a\x00b".to_owned())));

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("a\\u0000b".to_owned())));
        }
    }

    #[test]
    fn test_json_escape_mixed_special_chars() {
        // Test combination of multiple special characters
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("Line 1\nLine \"2\"\ttab\r\n".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string(
                    "Line 1\\nLine \\\"2\\\"\\ttab\\r\\n".to_owned()
                ))
            );
        }
    }

    #[test]
    fn test_json_escape_no_change() {
        // Normal string without special chars should pass through unchanged
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("Hello, World!".to_owned())),
            );

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(
                result,
                Value::String(global_string("Hello, World!".to_owned()))
            );
        }
    }

    #[test]
    fn test_json_escape_empty_string() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = json_escape(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(global_string("".to_owned())));
        }
    }

    // string->int tests

    #[test]
    fn test_string_to_int_success() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("42".to_owned())));

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(value, Value::Int(42));
        }
    }

    #[test]
    fn test_string_to_int_negative() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("-99".to_owned())));

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(value, Value::Int(-99));
        }
    }

    #[test]
    fn test_string_to_int_with_whitespace() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("  123  ".to_owned())));

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(value, Value::Int(123));
        }
    }

    #[test]
    fn test_string_to_int_failure() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(
                stack,
                Value::String(global_string("not a number".to_owned())),
            );

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            assert_eq!(value, Value::Int(0));
        }
    }

    #[test]
    fn test_string_to_int_empty() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("".to_owned())));

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            assert_eq!(value, Value::Int(0));
        }
    }

    #[test]
    fn test_string_to_int_leading_zeros() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(global_string("007".to_owned())));

            let stack = string_to_int(stack);

            let (stack, success) = pop(stack);
            let (_stack, value) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            assert_eq!(value, Value::Int(7));
        }
    }

    #[test]
    fn test_string_to_int_type_error() {
        unsafe {
            crate::error::clear_runtime_error();

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42)); // Wrong type - should be String

            let stack = string_to_int(stack);

            // Should have set an error
            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("expected String"));

            // Should return (0, false)
            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            let (_stack, value) = pop(stack);
            assert_eq!(value, Value::Int(0));
        }
    }
}
