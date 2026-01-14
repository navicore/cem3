//! Stub module for HTTP client operations when the "http" feature is disabled.
//!
//! These functions provide the same FFI interface but panic with helpful messages
//! instructing users to enable the http feature.

use seq_core::stack::Stack;

const FEATURE_MSG: &str = "http feature not enabled. Rebuild with: cargo build --features http";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_get(_stack: Stack) -> Stack {
    panic!("http.get requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_post(_stack: Stack) -> Stack {
    panic!("http.post requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_put(_stack: Stack) -> Stack {
    panic!("http.put requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_http_delete(_stack: Stack) -> Stack {
    panic!("http.delete requires {}", FEATURE_MSG);
}
