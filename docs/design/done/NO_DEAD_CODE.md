# No Dead Code in Compiled Binaries

Status: shipped · 2026-04-30 (Linux clean, macOS with documented residue)

## Intent

A Seq compiler should produce binaries that contain **exactly the
code reachable from the source program, and nothing else.** The
runtime archive holds machinery for HTTP, TLS, regex, compression,
crypto, and more — but a `hello world` source file references none of
these, and a correct compiler output should reflect that.

This is a values statement, not a size optimization. Linking unused
code into a static-compiled artifact is a dynamic-language reflex
("ship everything, just in case") that doesn't belong in a typed,
statically-compiled language. The Seq source declares its
capabilities; the binary should match the declaration. Anything else
is the toolchain quietly lying about what the program is.

Today every Seq binary is the same ~2.0 MB regardless of what its
source touches. **Size is the symptom; the bug is that the binary's
contents are not justified by the source.** A correct binary is one
whose every byte traces back to a transitive use from `main`.

## Constraints

- **No user-facing feature flags.** Users do not opt in or out of
  HTTP, crypto, regex, compression, etc. The compiler reads source;
  the source determines what's in the binary. The existing
  `crypto`/`http`/`regex`/`compression` Cargo features in
  `crates/runtime/Cargo.toml` are a runtime-build internal at most —
  they must never appear in `seqc`'s CLI surface.
- **No source annotation.** A program does not need to say
  `#[uses(http)]` or similar. The typechecker already knows which
  FFI builtins each word references; that's the ground truth and
  must remain the ground truth.
- **No silent feature gates.** A capability that isn't compiled in
  must be unreachable, not present-but-panicking. If the source
  doesn't reference HTTP, no HTTP code in the binary; and because
  the source doesn't reference it, no path can call it. (This rules
  out the current `*_stub.rs` panic-at-runtime pattern as the
  long-term answer.)
- **No language change.** Source semantics, FFI surface, and runtime
  API are unchanged. What changes is what `seqc build` puts in the
  output file.
- **Preserve always-on infrastructure.** `may` (scheduler), arena,
  channels, signal handlers, `SEQ_REPORT`, watchdog — these are
  reachable from `seq_main` itself, so they stay. That is not dead
  code; that is the runtime.
- **Out of scope:** binary size as a marketing number. Dynamic
  linking. Source-level capability declarations. Custom linker
  scripts. `no_std` rewrites of the runtime.

## Approach

The compiler already produces, per program, a precise set of FFI
symbols it calls. The toolchain just needs to enforce that set as the
binary's reachability boundary.

The principled mechanism is **whole-program dead-code elimination at
the final link**. Given the user's IR plus the runtime as bitcode
(not just opaque object code), LLVM can walk the call graph from
`seq_main` and discard everything else.

Implementation paths (an implementation concern, not a values one):

- **Cross-language LTO.** Build the runtime with
  `-Clinker-plugin-lto` so the staticlib carries LLVM bitcode.
  Final link: `clang -flto=thin -fuse-ld=lld <user.ll>
  libseq_runtime.a`. LLVM does whole-program DCE starting from
  `seq_main`. This is the principled answer.
- **Section-level GC as a stepping stone.** Build runtime with
  `-ffunction-sections -fdata-sections`; final link with
  `-Wl,--gc-sections` (Linux) / `-Wl,-dead_strip` (macOS). Coarser
  but a meaningful step in the right direction without an `lld`
  dependency.

Both are **automatic from the user's perspective.** They write
source; `seqc build` produces a binary whose contents match the
source. No flags. No opt-in.

This is not necessarily cheap to land, and may take more than one
PR — toolchain plumbing (`lld` availability across host platforms,
runtime build flags, CI matrix, packaging) is real work.
**Document the value now; build it when we build it.** Recording the
position is the point: every future architectural decision (e.g.
"should we add a built-in `xml` library?") is shaped by it. The
answer to that question is yes, *provided* the unused case
statically eliminates to nothing.

## Domain events

- **Source compiles** → the typechecker records the set of FFI
  builtins the program references (this set already exists
  internally).
- **Final link runs** → LLVM (or, in the stepping-stone, the
  system linker) walks from `seq_main`, keeps the closure of
  reachable code, drops the rest.
- **Output binary** → contains exactly the runtime machinery the
  source program could possibly execute. A `hello world` does not
  contain TLS code; an HTTP client does. The binary's contents are
  defensible byte-by-byte against the source.
- **A new builtin is added** → it lives in the runtime archive as
  always, but a program that doesn't call it pays nothing for it.
  This is the property that makes "batteries included" honest.

## Checkpoints

1. **Symbol-presence test.** Compile `hello.seq` and assert via
   `nm` / `llvm-objdump` that the binary contains no `http_*` /
   `regex_*` / `sha2_*` / `flate2_*` / `aes_*` / `ureq_*`
   symbols. This is the smallest, sharpest correctness signal —
   if it fails, the toolchain is including code the source did
   not ask for.
2. **Capability-delta test.** A canonical set (`hello`,
   `http-fetch`, `sha256-hash`, `regex-replace`, `gzip-roundtrip`)
   built in identical configuration shows symbol-set deltas
   matching capability deltas. Verifiable, not subjective.
3. **No `seqc` flag for capability selection.** The CLI surface of
   `seqc build` carries no `--features`, `--with-http`,
   `--minimal`. Confirmed by reading the help output.
4. **`BATTERIES_INCLUDED.md` rewritten** so its "Feature Flags &
   Binary Size" section is replaced by a single statement: *the
   binary contains exactly what the source uses; the runtime is
   always built with everything on and the link removes what the
   program doesn't reference.*
5. **No regression in `just ci`.** Existing tests, examples, and
   integration continue to pass under the new link.

## Result

Section/atom-level link-time GC turned out to be sufficient,
without nightly Rust, without bitcode embedding, without an `lld`
dependency. `seqc build` unconditionally passes one extra flag at
the final link:

- macOS: `-Wl,-dead_strip` — ld64 walks atom-level reachability
  from `seq_main`, dropping every function and data item not
  transitively referenced.
- Linux: `-Wl,--gc-sections` — GNU ld / lld do the same via
  section reachability. Effective on the workspace's existing
  `lto = true / codegen-units = 1` release profile.

Measured against `examples/basics/hello-world.seq` on rustc 1.95,
across the eleven canary crates the source does not touch
(`ureq`, `flate2`, `sha2`, `aes_gcm`, `regex_automata`,
`regex_syntax`, `rustls`, `ed25519`, `hmac`, `pbkdf2`, `zstd`):

| Platform           | Baseline leaks | After dead-strip |
|--------------------|---------------:|-----------------:|
| macOS aarch64      |          3,655 |                4 |
| Linux x86\_64      |          3,673 |                0 |

The four macOS residue symbols are hand-written assembly entry
points in the `ring` crate
(`_ring_core_*__aes_gcm_{enc,dec}_kernel`,
`_ring_core_*__sha256_block_data_order_{hw,nohw}`). They survive
ld64's `-dead_strip` because `ring` references them from inline
assembly via symbol lookups the linker cannot see. They are
inert in a `hello world` (no caller exists) but consume a few KB
of code. The integration test
(`crates/compiler/tests/no_dead_code.rs`) is `#[ignore]`d on
macOS for this reason; on Linux it is part of `just ci`.

### Cross-language LTO was attempted and ruled out

Cross-language LTO via `-C linker-plugin-lto` was investigated as
a uniform mechanism for both platforms. On Linux it is plausibly
viable but redundant — `--gc-sections` already drives the leak
count to zero. On macOS it is blocked: rustc emits
`-Wl,-plugin-opt=...` flags under `-C linker-plugin-lto`, and
neither Apple's ld64 nor LLVM's ld64.lld accepts that argument
(it is a GNU/ELF plugin protocol; LLVM's Mach-O linker uses a
different mechanism). Working around it would require a linker
wrapper or upstream toolchain changes — real engineering for no
additional correctness gain.

Cross-language LTO remains a future option if the macOS residue
becomes load-bearing or if a future LLVM/rustc combination makes
the toolchain coupling cheaper. For now, the simpler section-GC
mechanism is the production answer.

### Companion analysis

`docs/design/BINARY_FOOTPRINT.md` documents what is left in the
binary after dead-stripping and why each piece is there.
