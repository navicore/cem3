//! Build script for seq-compiler
//!
//! Locates the seq-runtime static library so it can be embedded into the compiler.

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    // Verify that seq-runtime version matches compiler version
    verify_runtime_version();
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

/// Verify that the seq-runtime version matches the seq-compiler version
/// by parsing the Cargo.toml files
fn verify_runtime_version() {
    let compiler_version = env!("CARGO_PKG_VERSION");

    // Read and parse the compiler's Cargo.toml to get the runtime dependency version
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read compiler/Cargo.toml");

    // Extract the exact version requirement for seq-runtime
    // Looking for: seq-runtime = { path = "../runtime", version = "=X.Y.Z" }
    let runtime_version = cargo_toml
        .lines()
        .find(|line| line.contains("seq-runtime") && line.contains("version"))
        .and_then(|line| {
            // Extract version string between quotes after "version = "
            line.split("version = \"")
                .nth(1)
                .and_then(|s| s.split('"').next())
        })
        .expect("Could not find seq-runtime version in Cargo.toml");

    // Remove the '=' prefix from exact version
    let runtime_version = runtime_version.trim_start_matches('=');

    if compiler_version != runtime_version {
        panic!(
            "\n\n\
            ╔══════════════════════════════════════════════════════════════╗\n\
            ║ VERSION MISMATCH ERROR                                       ║\n\
            ╠══════════════════════════════════════════════════════════════╣\n\
            ║ seq-compiler version: {:<39}║\n\
            ║ seq-runtime version:  {:<39}║\n\
            ║                                                              ║\n\
            ║ The embedded runtime MUST match the compiler version.       ║\n\
            ║ This ensures published crates.io packages are trustworthy.  ║\n\
            ║                                                              ║\n\
            ║ Update compiler/Cargo.toml to pin seq-runtime to:           ║\n\
            ║ version = \"={:<46}║\n\
            ╚══════════════════════════════════════════════════════════════╝\n",
            compiler_version, runtime_version, compiler_version
        );
    }

    println!(
        "cargo:warning=✓ Version verified: seq-compiler {} with seq-runtime {}",
        compiler_version, runtime_version
    );
}
