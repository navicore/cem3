//! Build script for seq-compiler
//!
//! Locates the seq-runtime static library so it can be embedded into the compiler.

use std::env;
use std::path::PathBuf;

fn main() {
    // The runtime is built by cargo as a dependency
    // We need to find the .a file in the target directory
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir).parent().unwrap().to_path_buf();

    // Check release first, then debug
    let release_lib = workspace_root.join("target/release/libseq_runtime.a");
    let debug_lib = workspace_root.join("target/debug/libseq_runtime.a");

    let runtime_lib = if release_lib.exists() {
        release_lib
    } else if debug_lib.exists() {
        debug_lib
    } else {
        panic!(
            "Runtime library not found. Please build seq-runtime first:\n\
             cargo build --release -p seq-runtime\n\
             Looked in:\n  {}\n  {}",
            release_lib.display(),
            debug_lib.display()
        );
    };

    // Set environment variable for include_bytes! in lib.rs
    println!(
        "cargo:rustc-env=SEQ_RUNTIME_LIB_PATH={}",
        runtime_lib.display()
    );

    // Rerun if the runtime library changes
    println!("cargo:rerun-if-changed={}", runtime_lib.display());
}
