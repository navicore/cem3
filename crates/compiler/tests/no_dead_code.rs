//! Asserts the binary produced by `seqc build` contains no symbols
//! from crates the source program does not reference. Encodes the
//! goal of `docs/design/done/NO_DEAD_CODE.md` as a runnable test.
//!
//! On Linux, `-Wl,--gc-sections` drives this to zero and the test
//! passes. On macOS, `-Wl,-dead_strip` leaves a small residue of
//! `ring` asm kernels that survive linker reachability analysis
//! because they are referenced via inline assembly the linker cannot
//! see; the test is `#[ignore]`d on macOS for that reason. The
//! residue is documented in `docs/design/done/NO_DEAD_CODE.md`. To
//! inspect it manually:
//!
//! ```text
//! cargo test --release -p seq-compiler --test no_dead_code -- --ignored --nocapture
//! ```

use std::fs;
use std::process::Command;

use tempfile::TempDir;

const SEQC: &str = env!("CARGO_BIN_EXE_seqc");

const HELLO_SOURCE: &str = ": main ( -- )\n  \"Hello, World!\" io.write-line ;\n";

// Symbol substrings that prove the binary contains code from crates a
// `hello world` program does not reference. Verified present in current
// release-profile binaries (2026-04-29). Substring match rather than
// prefix because `rustc` mangles the crate name into the middle of
// symbols (`_ZN4ureq...`, `_ZN6flate2...`).
const FORBIDDEN_FOR_HELLO: &[&str] = &[
    "ureq",
    "flate2",
    "sha2",
    "aes_gcm",
    "regex_automata",
    "regex_syntax",
    "rustls",
    "ed25519",
    "hmac",
    "pbkdf2",
    "zstd",
];

// Release-only: the empirical zero-leaks result depends on the
// workspace's release profile (`lto = true`, `codegen-units = 1`)
// being applied to the runtime archive `seqc` links against. Under
// `cargo test` (debug), `seqc` embeds a debug runtime with codegen-
// units=256 and no LTO, where `-Wl,--gc-sections` leaves a much
// larger residue and the assertion fails in confusing ways. The
// `just check-binary-contents` recipe forces release.
//
// macOS is also gated: 4 hand-written `ring` asm kernels are
// referenced via inline assembly the linker cannot prove dead. See
// docs/design/done/NO_DEAD_CODE.md.
#[test]
#[cfg_attr(
    any(target_os = "macos", debug_assertions),
    ignore = "release-only; macOS additionally leaves 4 ring asm stragglers — see docs/design/done/NO_DEAD_CODE.md"
)]
fn hello_world_binary_contains_no_unreferenced_capabilities() {
    let tmp = TempDir::new().expect("create tempdir");
    let src = tmp.path().join("hello.seq");
    let bin = tmp.path().join("hello");
    fs::write(&src, HELLO_SOURCE).expect("write hello.seq");

    let build = Command::new(SEQC)
        .args(["build", src.to_str().unwrap(), "-o", bin.to_str().unwrap()])
        .output()
        .expect("invoke seqc");
    assert!(
        build.status.success(),
        "seqc build failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr),
    );

    let nm = Command::new("nm")
        .arg(&bin)
        .output()
        .expect("invoke nm (install binutils on Linux)");
    assert!(
        nm.status.success(),
        "nm failed:\n{}",
        String::from_utf8_lossy(&nm.stderr),
    );
    let symbols = String::from_utf8_lossy(&nm.stdout);

    let leaks: Vec<(&str, usize)> = FORBIDDEN_FOR_HELLO
        .iter()
        .filter_map(|needle| match symbols.matches(needle).count() {
            0 => None,
            n => Some((*needle, n)),
        })
        .collect();

    if !leaks.is_empty() {
        let total: usize = leaks.iter().map(|(_, c)| c).sum();
        let detail = leaks
            .iter()
            .map(|(n, c)| format!("  {n}: {c} symbols"))
            .collect::<Vec<_>>()
            .join("\n");
        panic!(
            "hello.seq binary contains {total} symbol(s) from {n} crate(s) the source does not reference:\n{detail}",
            n = leaks.len(),
        );
    }
}
