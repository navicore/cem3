//! Regular expression operations for Seq
//!
//! These functions are exported with C ABI for LLVM codegen to call.
//! Uses Rust's regex crate - fast, safe, no catastrophic backtracking.
//!
//! # API
//!
//! ```seq
//! # Match check
//! "hello world" "wo.ld" regex.match?      # ( String String -- Bool )
//!
//! # Find first match
//! "a1 b2 c3" "[a-z][0-9]" regex.find      # ( String String -- String Bool )
//!
//! # Find all matches
//! "a1 b2 c3" "[a-z][0-9]" regex.find-all  # ( String String -- List )
//!
//! # Replace first occurrence
//! "hello world" "world" "Seq" regex.replace
//! # ( String pattern replacement -- String )
//!
//! # Replace all occurrences
//! "a1 b2 c3" "[0-9]" "X" regex.replace-all
//! # ( String pattern replacement -- String )
//!
//! # Capture groups
//! "2024-01-15" "(\d+)-(\d+)-(\d+)" regex.captures
//! # ( String pattern -- List Bool ) returns ["2024", "01", "15"] true on match
//!
//! # Split by pattern
//! "a1b2c3" "[0-9]" regex.split            # ( String pattern -- List )
//! ```

use seq_core::seqstring::global_string;
use seq_core::stack::{Stack, pop, push};
use seq_core::value::{Value, VariantData};

use regex::Regex;
use std::sync::Arc;

/// Helper to create a List variant from a vector of values
fn make_list(items: Vec<Value>) -> Value {
    Value::Variant(Arc::new(VariantData::new(
        global_string("List".to_string()),
        items,
    )))
}

/// Check if a pattern matches anywhere in the string
///
/// Stack effect: ( String pattern -- Bool )
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_match(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.match?: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val) {
        (Value::String(text), Value::String(pattern)) => {
            let result = match Regex::new(pattern.as_str()) {
                Ok(re) => re.is_match(text.as_str()),
                Err(_) => false, // Invalid regex returns false
            };
            unsafe { push(stack, Value::Bool(result)) }
        }
        _ => panic!("regex.match?: expected two Strings on stack"),
    }
}

/// Find the first match of a pattern in the string
///
/// Stack effect: ( String pattern -- String Bool )
///
/// Returns the matched text and true on success, empty string and false on no match.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_find(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.find: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val) {
        (Value::String(text), Value::String(pattern)) => {
            match Regex::new(pattern.as_str()) {
                Ok(re) => match re.find(text.as_str()) {
                    Some(m) => {
                        let stack = unsafe {
                            push(stack, Value::String(global_string(m.as_str().to_string())))
                        };
                        unsafe { push(stack, Value::Bool(true)) }
                    }
                    None => {
                        let stack =
                            unsafe { push(stack, Value::String(global_string(String::new()))) };
                        unsafe { push(stack, Value::Bool(false)) }
                    }
                },
                Err(_) => {
                    // Invalid regex
                    let stack = unsafe { push(stack, Value::String(global_string(String::new()))) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("regex.find: expected two Strings on stack"),
    }
}

/// Find all matches of a pattern in the string
///
/// Stack effect: ( String pattern -- List Bool )
///
/// Returns a list of all matched substrings and true on success.
/// Returns empty list and false on invalid regex.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_find_all(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.find-all: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val) {
        (Value::String(text), Value::String(pattern)) => match Regex::new(pattern.as_str()) {
            Ok(re) => {
                let matches: Vec<Value> = re
                    .find_iter(text.as_str())
                    .map(|m| Value::String(global_string(m.as_str().to_string())))
                    .collect();
                let stack = unsafe { push(stack, make_list(matches)) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            Err(_) => {
                // Invalid regex
                let stack = unsafe { push(stack, make_list(vec![])) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("regex.find-all: expected two Strings on stack"),
    }
}

/// Replace the first occurrence of a pattern
///
/// Stack effect: ( String pattern replacement -- String Bool )
///
/// Returns the string with the first match replaced and true on success.
/// Returns original string and false on invalid regex.
///
/// # Safety
/// Stack must have three String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_replace(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.replace: stack is empty");

    let (stack, replacement_val) = unsafe { pop(stack) };
    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val, replacement_val) {
        (Value::String(text), Value::String(pattern), Value::String(replacement)) => {
            match Regex::new(pattern.as_str()) {
                Ok(re) => {
                    let result = re.replace(text.as_str(), replacement.as_str()).into_owned();
                    let stack = unsafe { push(stack, Value::String(global_string(result))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                Err(_) => {
                    // Invalid regex returns original
                    let stack = unsafe {
                        push(
                            stack,
                            Value::String(global_string(text.as_str().to_string())),
                        )
                    };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("regex.replace: expected three Strings on stack"),
    }
}

/// Replace all occurrences of a pattern
///
/// Stack effect: ( String pattern replacement -- String Bool )
///
/// Returns the string with all matches replaced and true on success.
/// Returns original string and false on invalid regex.
///
/// # Safety
/// Stack must have three String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_replace_all(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.replace-all: stack is empty");

    let (stack, replacement_val) = unsafe { pop(stack) };
    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val, replacement_val) {
        (Value::String(text), Value::String(pattern), Value::String(replacement)) => {
            match Regex::new(pattern.as_str()) {
                Ok(re) => {
                    let result = re
                        .replace_all(text.as_str(), replacement.as_str())
                        .into_owned();
                    let stack = unsafe { push(stack, Value::String(global_string(result))) };
                    unsafe { push(stack, Value::Bool(true)) }
                }
                Err(_) => {
                    // Invalid regex returns original
                    let stack = unsafe {
                        push(
                            stack,
                            Value::String(global_string(text.as_str().to_string())),
                        )
                    };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("regex.replace-all: expected three Strings on stack"),
    }
}

/// Extract capture groups from a pattern match
///
/// Stack effect: ( String pattern -- List Bool )
///
/// Returns a list of captured groups (excluding the full match) and true on success.
/// Returns empty list and false if no match or invalid regex.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_captures(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.captures: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val) {
        (Value::String(text), Value::String(pattern)) => {
            match Regex::new(pattern.as_str()) {
                Ok(re) => match re.captures(text.as_str()) {
                    Some(caps) => {
                        // Skip group 0 (full match), collect groups 1..n
                        let groups: Vec<Value> = caps
                            .iter()
                            .skip(1)
                            .map(|m| match m {
                                Some(m) => Value::String(global_string(m.as_str().to_string())),
                                None => Value::String(global_string(String::new())),
                            })
                            .collect();
                        let stack = unsafe { push(stack, make_list(groups)) };
                        unsafe { push(stack, Value::Bool(true)) }
                    }
                    None => {
                        let stack = unsafe { push(stack, make_list(vec![])) };
                        unsafe { push(stack, Value::Bool(false)) }
                    }
                },
                Err(_) => {
                    // Invalid regex
                    let stack = unsafe { push(stack, make_list(vec![])) };
                    unsafe { push(stack, Value::Bool(false)) }
                }
            }
        }
        _ => panic!("regex.captures: expected two Strings on stack"),
    }
}

/// Split a string by a pattern
///
/// Stack effect: ( String pattern -- List Bool )
///
/// Returns a list of substrings split by the pattern and true on success.
/// Returns single-element list with original string and false on invalid regex.
///
/// # Safety
/// Stack must have two String values on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_split(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.split: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };
    let (stack, text_val) = unsafe { pop(stack) };

    match (text_val, pattern_val) {
        (Value::String(text), Value::String(pattern)) => match Regex::new(pattern.as_str()) {
            Ok(re) => {
                let parts: Vec<Value> = re
                    .split(text.as_str())
                    .map(|s| Value::String(global_string(s.to_string())))
                    .collect();
                let stack = unsafe { push(stack, make_list(parts)) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            Err(_) => {
                // Invalid regex returns original as single element
                let parts = vec![Value::String(global_string(text.as_str().to_string()))];
                let stack = unsafe { push(stack, make_list(parts)) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("regex.split: expected two Strings on stack"),
    }
}

/// Check if a pattern is a valid regex
///
/// Stack effect: ( String -- Bool )
///
/// Returns true if the pattern compiles successfully, false otherwise.
///
/// # Safety
/// Stack must have a String value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_valid(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "regex.valid?: stack is empty");

    let (stack, pattern_val) = unsafe { pop(stack) };

    match pattern_val {
        Value::String(pattern) => {
            let is_valid = Regex::new(pattern.as_str()).is_ok();
            unsafe { push(stack, Value::Bool(is_valid)) }
        }
        _ => panic!("regex.valid?: expected String on stack"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seq_core::stack::alloc_stack;

    #[test]
    fn test_regex_match() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };
        let stack = unsafe { push(stack, Value::String(global_string("wo.ld".to_string()))) };

        let stack = unsafe { patch_seq_regex_match(stack) };
        let (_, value) = unsafe { pop(stack) };
        assert_eq!(value, Value::Bool(true));
    }

    #[test]
    fn test_regex_match_no_match() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("hello".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("xyz".to_string()))) };

        let stack = unsafe { patch_seq_regex_match(stack) };
        let (_, value) = unsafe { pop(stack) };
        assert_eq!(value, Value::Bool(false));
    }

    #[test]
    fn test_regex_find() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("a1 b2 c3".to_string()))) };
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("[a-z][0-9]".to_string())),
            )
        };

        let stack = unsafe { patch_seq_regex_find(stack) };
        let (stack, success) = unsafe { pop(stack) };
        let (_, matched) = unsafe { pop(stack) };

        assert_eq!(success, Value::Bool(true));
        if let Value::String(s) = matched {
            assert_eq!(s.as_str(), "a1");
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn test_regex_find_all() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("a1 b2 c3".to_string()))) };
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("[a-z][0-9]".to_string())),
            )
        };

        let stack = unsafe { patch_seq_regex_find_all(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));
        let (_, list_val) = unsafe { pop(stack) };

        if let Value::Variant(v) = list_val {
            assert_eq!(v.fields.len(), 3);
            if let Value::String(s) = &v.fields[0] {
                assert_eq!(s.as_str(), "a1");
            }
            if let Value::String(s) = &v.fields[1] {
                assert_eq!(s.as_str(), "b2");
            }
            if let Value::String(s) = &v.fields[2] {
                assert_eq!(s.as_str(), "c3");
            }
        } else {
            panic!("expected Variant (List)");
        }
    }

    #[test]
    fn test_regex_replace() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("hello world".to_string())),
            )
        };
        let stack = unsafe { push(stack, Value::String(global_string("world".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("Seq".to_string()))) };

        let stack = unsafe { patch_seq_regex_replace(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));
        let (_, result) = unsafe { pop(stack) };

        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "hello Seq");
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn test_regex_replace_all() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("a1 b2 c3".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("[0-9]".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("X".to_string()))) };

        let stack = unsafe { patch_seq_regex_replace_all(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));
        let (_, result) = unsafe { pop(stack) };

        if let Value::String(s) = result {
            assert_eq!(s.as_str(), "aX bX cX");
        } else {
            panic!("expected String");
        }
    }

    #[test]
    fn test_regex_captures() {
        let stack = alloc_stack();
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string("2024-01-15".to_string())),
            )
        };
        let stack = unsafe {
            push(
                stack,
                Value::String(global_string(r"(\d+)-(\d+)-(\d+)".to_string())),
            )
        };

        let stack = unsafe { patch_seq_regex_captures(stack) };
        let (stack, success) = unsafe { pop(stack) };
        let (_, groups) = unsafe { pop(stack) };

        assert_eq!(success, Value::Bool(true));
        if let Value::Variant(v) = groups {
            assert_eq!(v.fields.len(), 3);
            if let Value::String(s) = &v.fields[0] {
                assert_eq!(s.as_str(), "2024");
            }
            if let Value::String(s) = &v.fields[1] {
                assert_eq!(s.as_str(), "01");
            }
            if let Value::String(s) = &v.fields[2] {
                assert_eq!(s.as_str(), "15");
            }
        } else {
            panic!("expected Variant (List)");
        }
    }

    #[test]
    fn test_regex_split() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("a1b2c3".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("[0-9]".to_string()))) };

        let stack = unsafe { patch_seq_regex_split(stack) };
        let (stack, success) = unsafe { pop(stack) };
        assert_eq!(success, Value::Bool(true));
        let (_, result) = unsafe { pop(stack) };

        if let Value::Variant(v) = result {
            assert_eq!(v.fields.len(), 4); // "a", "b", "c", ""
            if let Value::String(s) = &v.fields[0] {
                assert_eq!(s.as_str(), "a");
            }
            if let Value::String(s) = &v.fields[1] {
                assert_eq!(s.as_str(), "b");
            }
            if let Value::String(s) = &v.fields[2] {
                assert_eq!(s.as_str(), "c");
            }
        } else {
            panic!("expected Variant (List)");
        }
    }

    #[test]
    fn test_regex_valid() {
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("[a-z]+".to_string()))) };

        let stack = unsafe { patch_seq_regex_valid(stack) };
        let (_, result) = unsafe { pop(stack) };
        assert_eq!(result, Value::Bool(true));

        // Invalid regex
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("[invalid".to_string()))) };

        let stack = unsafe { patch_seq_regex_valid(stack) };
        let (_, result) = unsafe { pop(stack) };
        assert_eq!(result, Value::Bool(false));
    }

    #[test]
    fn test_invalid_regex_graceful() {
        // Invalid regex should return false, not panic
        let stack = alloc_stack();
        let stack = unsafe { push(stack, Value::String(global_string("test".to_string()))) };
        let stack = unsafe { push(stack, Value::String(global_string("[invalid".to_string()))) };

        let stack = unsafe { patch_seq_regex_match(stack) };
        let (_, result) = unsafe { pop(stack) };
        assert_eq!(result, Value::Bool(false));
    }
}
