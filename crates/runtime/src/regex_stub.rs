//! Stub module for regex operations when the "regex" feature is disabled.
//!
//! These functions provide the same FFI interface but panic with helpful messages
//! instructing users to enable the regex feature.

use seq_core::stack::Stack;

const FEATURE_MSG: &str = "regex feature not enabled. Rebuild with: cargo build --features regex";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_match(_stack: Stack) -> Stack {
    panic!("regex.match? requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_find(_stack: Stack) -> Stack {
    panic!("regex.find requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_find_all(_stack: Stack) -> Stack {
    panic!("regex.find-all requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_replace(_stack: Stack) -> Stack {
    panic!("regex.replace requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_replace_all(_stack: Stack) -> Stack {
    panic!("regex.replace-all requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_captures(_stack: Stack) -> Stack {
    panic!("regex.captures requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_split(_stack: Stack) -> Stack {
    panic!("regex.split requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_regex_valid(_stack: Stack) -> Stack {
    panic!("regex.valid? requires {}", FEATURE_MSG);
}
