//! Encodes the goal of `docs/design/NO_DEAD_CODE.md` as a runnable test.
//!
//! A `hello world` Seq program references no HTTP, regex, crypto, or
//! compression code. The compiled binary should reflect that. This test
//! compiles a `hello.seq` and asserts the resulting binary contains no
//! symbols from the canary set of crates the source does not touch.
//!
//! Currently fails. The test is `#[ignore]`'d so `cargo test` does not
//! run it; invoke explicitly to measure each candidate link strategy:
//!
//! ```text
//! cargo test --release -p seq-compiler --test no_dead_code -- --ignored --nocapture
//! ```
//!
//! When a link strategy makes the test pass, remove `#[ignore]` and wire
//! it into `just ci`.

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

#[test]
#[ignore = "design goal of docs/design/NO_DEAD_CODE.md; currently fails. \
            Run with `cargo test -- --ignored` to measure progress."]
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
