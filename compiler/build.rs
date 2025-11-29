//! Build script for seq-compiler
//!
//! Locates the seq-runtime static library so it can be embedded into the compiler.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // OUT_DIR is something like:
    // target/release/build/seq-compiler-xxx/out
    // We need to find libseq_runtime.a in:
    // target/release/libseq_runtime.a or target/release/deps/libseq_runtime-xxx.a

    // Navigate up from OUT_DIR to find target directory
    // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out
    let target_dir = out_dir
        .parent() // build/<pkg>-<hash>/out -> build/<pkg>-<hash>
        .and_then(|p| p.parent()) // -> build
        .and_then(|p| p.parent()) // -> <profile> (release/debug)
        .expect("Could not find target directory");

    // Try to find libseq_runtime.a
    let direct_lib = target_dir.join("libseq_runtime.a");

    let runtime_lib = if direct_lib.exists() {
        direct_lib
    } else {
        // Search in deps directory for libseq_runtime-*.a
        let deps_dir = target_dir.join("deps");
        find_runtime_in_deps(&deps_dir).unwrap_or_else(|| {
            panic!(
                "Runtime library not found.\n\
                 Looked in: {}\n\
                 And deps: {}\n\
                 OUT_DIR was: {}",
                direct_lib.display(),
                deps_dir.display(),
                out_dir.display()
            )
        })
    };

    // Set environment variable for include_bytes! in lib.rs
    println!(
        "cargo:rustc-env=SEQ_RUNTIME_LIB_PATH={}",
        runtime_lib.display()
    );

    // Rerun if the runtime library changes
    println!("cargo:rerun-if-changed={}", runtime_lib.display());
}

fn find_runtime_in_deps(deps_dir: &PathBuf) -> Option<PathBuf> {
    if !deps_dir.exists() {
        return None;
    }

    fs::read_dir(deps_dir).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("libseq_runtime") && name_str.ends_with(".a") {
            Some(entry.path())
        } else {
            None
        }
    })
}
