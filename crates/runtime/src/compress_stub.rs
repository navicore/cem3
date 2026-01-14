//! Stub module for compression operations when the "compression" feature is disabled.
//!
//! These functions provide the same FFI interface but panic with helpful messages
//! instructing users to enable the compression feature.

use seq_core::stack::Stack;

const FEATURE_MSG: &str =
    "compression feature not enabled. Rebuild with: cargo build --features compression";

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gzip(_stack: Stack) -> Stack {
    panic!("compress.gzip requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gzip_level(_stack: Stack) -> Stack {
    panic!("compress.gzip-level requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_gunzip(_stack: Stack) -> Stack {
    panic!("compress.gunzip requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_zstd(_stack: Stack) -> Stack {
    panic!("compress.zstd requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_zstd_level(_stack: Stack) -> Stack {
    panic!("compress.zstd-level requires {}", FEATURE_MSG);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_compress_unzstd(_stack: Stack) -> Stack {
    panic!("compress.unzstd requires {}", FEATURE_MSG);
}
