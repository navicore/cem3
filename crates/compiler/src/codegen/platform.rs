//! Platform detection and FFI type helpers.

use crate::ffi::{FfiArg, FfiReturn, FfiType};

/// Get the target triple for the current platform
pub fn get_target_triple() -> &'static str {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "arm64-apple-macosx14.0.0"
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "x86_64-apple-darwin"
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "x86_64-unknown-linux-gnu"
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "aarch64-unknown-linux-gnu"
    }

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64")
    )))]
    {
        "unknown"
    }
}

/// Get the LLVM IR return type for an FFI function
pub fn ffi_return_type(return_spec: &Option<FfiReturn>) -> &'static str {
    match return_spec {
        Some(spec) => match spec.return_type {
            FfiType::Int => "i64",
            FfiType::String => "ptr",
            FfiType::Ptr => "ptr",
            FfiType::Void => "void",
        },
        None => "void",
    }
}

/// Get the LLVM IR argument types for an FFI function
pub fn ffi_c_args(args: &[FfiArg]) -> String {
    if args.is_empty() {
        return String::new();
    }

    args.iter()
        .map(|arg| match arg.arg_type {
            FfiType::Int => "i64",
            FfiType::String => "ptr",
            FfiType::Ptr => "ptr",
            FfiType::Void => "void",
        })
        .collect::<Vec<_>>()
        .join(", ")
}
