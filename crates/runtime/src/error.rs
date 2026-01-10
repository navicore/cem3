//! Runtime Error Handling
//!
//! Provides thread-local error state for FFI functions to report errors
//! without panicking across the FFI boundary.
//!
//! # Usage
//!
//! FFI functions can set an error instead of panicking:
//! ```ignore
//! if divisor == 0 {
//!     set_runtime_error("divide: division by zero");
//!     return stack; // Return unchanged stack
//! }
//! ```
//!
//! Callers can check for errors:
//! ```ignore
//! if patch_seq_has_error() {
//!     let error = patch_seq_take_error();
//!     // Handle error...
//! }
//! ```

use std::cell::RefCell;
use std::ffi::CString;
use std::ptr;

thread_local! {
    /// Thread-local storage for the last runtime error message
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };

    /// Cached C string for FFI access (avoids allocation on every get)
    static ERROR_CSTRING: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// Set the last runtime error message
///
/// Note: This clears any cached CString to prevent stale pointer access.
pub fn set_runtime_error(msg: impl Into<String>) {
    // Clear cached CString first to prevent stale pointers
    ERROR_CSTRING.with(|cs| *cs.borrow_mut() = None);
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = Some(msg.into());
    });
}

/// Take (and clear) the last runtime error message
pub fn take_runtime_error() -> Option<String> {
    LAST_ERROR.with(|e| e.borrow_mut().take())
}

/// Check if there's a pending runtime error
pub fn has_runtime_error() -> bool {
    LAST_ERROR.with(|e| e.borrow().is_some())
}

/// Clear any pending runtime error
pub fn clear_runtime_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
    ERROR_CSTRING.with(|e| *e.borrow_mut() = None);
}

/// Format a panic payload into an error message
pub fn format_panic_payload(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

// FFI-safe error access functions

/// Check if there's a pending runtime error (FFI-safe)
#[unsafe(no_mangle)]
pub extern "C" fn patch_seq_has_error() -> bool {
    has_runtime_error()
}

/// Get the last error message as a C string pointer (FFI-safe)
///
/// Returns null if no error is pending.
///
/// # WARNING: Pointer Lifetime
/// The returned pointer is only valid until the next call to `set_runtime_error`,
/// `get_error`, `take_error`, or `clear_error`. Callers must copy the string
/// immediately if they need to retain it.
#[unsafe(no_mangle)]
pub extern "C" fn patch_seq_get_error() -> *const i8 {
    LAST_ERROR.with(|e| {
        let error = e.borrow();
        match &*error {
            Some(msg) => {
                // Cache the CString so the pointer remains valid
                ERROR_CSTRING.with(|cs| {
                    // Replace null bytes with '?' to preserve error content
                    let safe_msg: String = msg
                        .chars()
                        .map(|c| if c == '\0' { '?' } else { c })
                        .collect();
                    let cstring = CString::new(safe_msg).expect("null bytes already replaced");
                    let ptr = cstring.as_ptr();
                    *cs.borrow_mut() = Some(cstring);
                    ptr
                })
            }
            None => ptr::null(),
        }
    })
}

/// Take (and clear) the last error, returning it as a C string (FFI-safe)
///
/// Returns null if no error is pending.
///
/// # WARNING: Pointer Lifetime
/// The returned pointer is only valid until the next call to `set_runtime_error`,
/// `get_error`, `take_error`, or `clear_error`. Callers must copy the string
/// immediately if they need to retain it.
#[unsafe(no_mangle)]
pub extern "C" fn patch_seq_take_error() -> *const i8 {
    let msg = take_runtime_error();
    match msg {
        Some(s) => ERROR_CSTRING.with(|cs| {
            // Replace null bytes with '?' to preserve error content
            let safe_msg: String = s.chars().map(|c| if c == '\0' { '?' } else { c }).collect();
            let cstring = CString::new(safe_msg).expect("null bytes already replaced");
            let ptr = cstring.as_ptr();
            *cs.borrow_mut() = Some(cstring);
            ptr
        }),
        None => ptr::null(),
    }
}

/// Clear any pending error (FFI-safe)
#[unsafe(no_mangle)]
pub extern "C" fn patch_seq_clear_error() {
    clear_runtime_error();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_and_take_error() {
        clear_runtime_error();
        assert!(!has_runtime_error());

        set_runtime_error("test error");
        assert!(has_runtime_error());

        let error = take_runtime_error();
        assert_eq!(error, Some("test error".to_string()));
        assert!(!has_runtime_error());
    }

    #[test]
    fn test_clear_error() {
        set_runtime_error("another error");
        assert!(has_runtime_error());

        clear_runtime_error();
        assert!(!has_runtime_error());
        assert!(take_runtime_error().is_none());
    }

    #[test]
    fn test_format_panic_payload() {
        let payload: Box<dyn std::any::Any + Send> = Box::new("panic message");
        assert_eq!(format_panic_payload(&payload), "panic message");

        let payload: Box<dyn std::any::Any + Send> = Box::new("owned panic".to_string());
        assert_eq!(format_panic_payload(&payload), "owned panic");
    }
}
