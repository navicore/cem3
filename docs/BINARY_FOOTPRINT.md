# Binary Footprint

Compiled Seq programs include a runtime — green threads, channels, an
arena allocator, signal handling, panic backtrace machinery. This
document explains what's in the binary you ship, why each piece is
there, and which size choices you can adjust.

## Sizes

For `examples/basics/hello-world.seq` on Linux x86\_64, rustc 1.95:

| Build                                            | Size  |
|--------------------------------------------------|------:|
| Default `seqc build`                             | 6.4M  |
| `seqc build` then `strip` (no DWARF, no symbols) | ~700K |
| Pure Rust `--release` hello world (for comparison) | ~500K |

The dominant cost in the default build is **DWARF debug info**
(~5.4M). It is intentional, and there's a tradeoff documented below.
The actual code and data the runtime contributes is ~700K — roughly
200K above a comparable Rust hello world.

Programs that don't reference HTTP, regex, crypto, or compression do
not pay for those subsystems' code. The runtime archive contains all
of them; the link removes what's unreferenced. See
[Batteries Included](BATTERIES_INCLUDED.md#binary-contents).

## What's in the binary

Excluding DWARF and the symbol table, the ~700K of code splits into
four groups:

### Backtrace symbolizer (~250K)

Rust's `std::panic` machinery includes an in-process DWARF parser
(`gimli`, `addr2line`, `rustc_demangle`, plus a deflate decompressor
for compressed DWARF sections). When a Seq program panics, this is
what reads the binary's own debug info to print a `.seq:line` trace.
A pure Rust `--release` binary often skips this — Seq keeps it so
panics from generated code can point back at the source.

This category disappears entirely under `panic = "abort"`, at the
cost of useful panic backtraces.

### Seq runtime essentials (~150–250K)

The always-on infrastructure that defines a Seq program:

- `may` — green-thread scheduler
- arena allocator
- channels (via `crossbeam`)
- signal handlers (SIGQUIT diagnostic dump, watchdog)
- the at-exit `SEQ_REPORT` KPI emitter
- sync primitives (`parking_lot`) used by the above

These are reachable from `seq_main` itself, so they stay in every
binary. They are what makes a Seq program a Seq program rather than a
thin wrapper around `printf`.

### Float formatting (~30–50K)

`std::fmt`'s float-to-string machinery
(`core::num::flt2dec::dragon::*`, the `POW10_SIGNIFICANDS` table).
Pulled in via `std::fmt`'s monomorphized formatters. Present even in
programs that don't print floats, because the formatter trait
machinery touches it.

### Rust standard library baseline (~200–300K)

The allocator, panic infrastructure (the `core` parts), and common
slice/string operations that any Rust binary carries. Not Seq-specific
— this is the cost of being a Rust-built program at all.

## DWARF and the backtrace tradeoff

`seqc build` passes `-g` to clang on every build. This embeds DWARF
debug sections that account for ~5.4M of the default 6.4M binary
size.

The DWARF is what lets a panic from generated Seq code resolve back
to a source line in your `.seq` file rather than a hex address. It's
metadata only — no runtime cost — but it is large.

If a deployment target is size-constrained and you don't need
`.seq:line` resolution in panic traces, `strip` removes DWARF and the
symbol table:

```bash
seqc build prog.seq -o prog
strip prog
```

A `strip`'d binary still runs and still panics; it just prints
addresses where it would otherwise print source locations. There is
no recommended default here — the tradeoff between debuggability and
artifact size depends on what you're shipping.

Programs that go further and don't want backtrace machinery at all
can configure the workspace's release profile with
`panic = "abort"`. That removes the ~250K backtrace symbolizer
category in addition to the DWARF data. Panics then terminate the
process without a backtrace.

## Inspecting your own binaries

Standard Linux binutils are enough for a quick breakdown:

```bash
size -A ./prog | sort -k2 -rn | head -20
nm --print-size --size-sort --reverse-sort ./prog | head -30
```

`bloaty` (where available) gives a much better view, especially the
`compileunits` mode that buckets bytes by source file:

```bash
bloaty ./prog -d compileunits -n 25
```

On macOS the equivalents are `size -m`, `nm`, and `bloaty` (via
Homebrew). The shape of a Seq macOS binary is similar to Linux modulo
a small `ring`-crate residue that survives `ld64`'s `-dead_strip` —
it's a few KB of unused AES and SHA assembly kernels and is inert at
runtime.
