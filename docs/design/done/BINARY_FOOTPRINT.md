# Binary Footprint

Status: shipped · 2026-04-30

Companion to `NO_DEAD_CODE.md`. Once the link drops every function
and data item unreferenced from `seq_main`, the question becomes:
what *is* in the binary, and is each piece justified?

This document records the breakdown for `hello-world.seq` on Linux
x86\_64, rustc 1.95.0, with `-Wl,--gc-sections` (the default since
NO\_DEAD\_CODE shipped). The shape on macOS aarch64 is similar
modulo a small `ring` asm residue documented in NO\_DEAD\_CODE.

## Headline

| Build                                       | Size  |
|---------------------------------------------|------:|
| Pure Rust hello world, `--release` stripped | ~500K |
| Seq hello world, dead-stripped + binary stripped | **711K** |
| Seq hello world, dead-stripped (default `seqc build`) | 6.4M |
| Seq hello world without dead-strip (historic) | 15M |

The headline number is the 711K. That is the actual code and
data the runtime contributes once DWARF and the symbol table are
removed. The cost over a Rust hello world is roughly 200–300K.

## What's in the 6.4M default build

ELF section sizes for the default `seqc build` output:

| Category                              | Size   | Notes |
|---------------------------------------|-------:|-------|
| `.debug_*` (DWARF)                    | ~5.7M  | design choice: enables `.seq:line` panic backtraces |
| `.text` (machine code)                |   503K | runtime + program |
| `.rodata` (constant tables)           |    98K | float-formatting tables, etc. |
| `.eh_frame` / `.gcc_except_table`     |    72K | unwinder metadata |
| `.data.rel.ro`, `.got`, `.bss`, etc.  |    50K | normal ELF metadata |
| Symbol table + string table           |   ~2M  | non-.debug names, removed by `strip` |

Two things dominate: DWARF (5.7M) and the symbol table (~2M). Both
are `strip`-able at the cost of debuggability, which is why the
default keeps them.

`seqc` passes `-g` to clang on every build. The intent is that
when a Seq program panics, the backtrace resolves to `.seq:line`
positions. Stripping DWARF saves the user roughly 5.4M but loses
that resolution.

## What's in the 711K stripped binary

Top symbols cluster into four groups. Sizes are approximate.

### Backtrace symbolization machinery (~250K)

The largest single category, and the one most likely to surprise.

- `std::backtrace_rs::symbolize::gimli::*` — gimli is a DWARF parser
- `addr2line::*` — translates addresses to file:line via DWARF
- `miniz_oxide::inflate::core::decompress` — for compressed DWARF sections
- `rustc_demangle::demangle` — turns `_ZN3std...` into `std::...`

When a Seq program panics, std's panic handler reads the
in-process binary's own DWARF to print the backtrace. That requires
shipping the DWARF *parser* (gimli/addr2line) inside the binary —
not just the DWARF data. `miniz_oxide` is here as the DWARF
decompressor, not because anything in the program touches `flate2`
(the no-dead-code test confirms `flate2` itself is absent).

Most pure-Rust `--release` binaries skip this because their panic
handler is wired to abort or print a brief message. Seq runtimes
keep full backtrace machinery because panics from generated code
need to point back at `.seq:line` for the message to be actionable
to the developer.

This category disappears entirely under `panic = "abort"`, at the
cost of useful panic backtraces.

### Seq runtime essentials (~150–250K)

The always-on infrastructure NO\_DEAD\_CODE explicitly preserves:

- `may::scheduler::init_scheduler` — green-thread runtime
- `seq_runtime::diagnostics::dump_diagnostics` — SIGQUIT diagnostic dump
- `patch_seq_report` — `SEQ_REPORT` KPI emitter at exit
- `signal_hook_registry::register_unchecked_impl` — SIGQUIT/watchdog plumbing
- `crossbeam_utils::atomic::atomic_cell::lock::LOCKS` — channel internals
- `parking_lot::raw_rwlock::*` — sync primitives, transitively via `may`

These are reachable from `seq_main` itself, so dead-strip cannot
remove them — and shouldn't. They are what makes Seq a Seq program
rather than a thin wrapper around `printf`.

### Float formatting (~30–50K)

- `core::num::flt2dec::dragon::format_shortest` and `format_exact`
- `zmij::POW10_SIGNIFICANDS` (10K, `.rodata`)

These come along because `std::fmt` includes float-to-string
converters. A hello world that prints "Hello, World!" doesn't use
them, but they are monomorphized in via `core::fmt`'s formatter
machinery and remain reachable via virtual dispatch tables.

### libstd / libcore baseline (~200–300K)

- Allocator
- Panic infrastructure (the `core` parts; the symbolizer is the
  `std` part above)
- Common slice and string operations: `stable::quicksort` (two
  monomorphizations), `to_lowercase`, `BTreeMap::insert`

This is "being a Rust binary." Pure Rust `--release` carries most
of it.

## Tradeoff levers

These are documented for completeness, not as recommendations. The
defaults are deliberate.

| Want to drop                          | How                                            | Cost                                        |
|---------------------------------------|------------------------------------------------|---------------------------------------------|
| 5.4M DWARF                            | Stop passing `-g` in `crates/compiler/src/lib.rs` | Lose `.seq:line` resolution in backtraces |
| ~250K backtrace symbolizer            | `panic = "abort"` in workspace release profile | Lose backtraces entirely on panic           |
| ~30–50K float formatting              | Avoid `Display` for `f64` in runtime           | Probably not worth it — small slice         |
| Seq runtime essentials                | Don't                                          | These *are* the runtime                     |

## Re-running the analysis

```bash
BIN=/tmp/hello-ds
./target/release/seqc build examples/basics/hello-world.seq -o "$BIN"

ls -lh "$BIN"
size -A "$BIN" | sort -k2 -rn | head -15
nm --print-size --size-sort --reverse-sort "$BIN" 2>/dev/null | head -30

cp "$BIN" "$BIN.stripped"
strip "$BIN.stripped"
ls -lh "$BIN.stripped"
"$BIN.stripped"
```

If `bloaty` is available it gives a far better breakdown:

```bash
bloaty "$BIN" -d compileunits -n 25
bloaty "$BIN" -d sections,symbols -n 25
```
