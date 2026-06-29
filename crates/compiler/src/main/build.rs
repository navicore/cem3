//! `seqc build` subcommand: compile a .seq file to an executable.

use std::path::{Path, PathBuf};
use std::process;

/// `loop_opt_cadence`: `None` disables loop lowering; `Some(n)` enables it
/// with a yield every `n` iterations. `n` must be a power of two.
pub(crate) fn run_build(
    input: &Path,
    output: &Path,
    keep_ir: bool,
    ffi_manifests: &[PathBuf],
    pure_inline: bool,
    instrument: bool,
    loop_opt_cadence: Option<u32>,
) {
    // Build config with external FFI manifests
    let mut config = if ffi_manifests.is_empty() {
        seqc::CompilerConfig::default()
    } else {
        seqc::CompilerConfig::new().with_ffi_manifests(ffi_manifests.iter().cloned())
    };

    // Enable pure inline test mode if requested
    config.pure_inline_test = pure_inline;

    // Enable per-word instrumentation if requested
    config.instrument = instrument;

    // Enable loop lowering if requested. The yield cadence must be a power of
    // two (it is used as an AND mask); default 1024.
    if let Some(cadence) = loop_opt_cadence {
        if cadence == 0 || !cadence.is_power_of_two() {
            eprintln!(
                "Error: --loop-yield-cadence must be a power of two, got {}",
                cadence
            );
            process::exit(1);
        }
        config.loop_opt = true;
        config.loop_yield_cadence = cadence;
    }

    match seqc::compile_file_with_config(input, output, keep_ir, &config) {
        Ok(_) => {
            println!("Compiled {} -> {}", input.display(), output.display());

            if keep_ir {
                let ir_path = output.with_extension("ll");
                if ir_path.exists() {
                    println!("IR saved to {}", ir_path.display());
                }
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            process::exit(1);
        }
    }
}
