# Seq - Concatenative Language

A concatenative, stack-based programming language with static typing and CSP-style concurrency.

## Status

**Compiler:** Functional (compiles .seq source to native executables via LLVM)
**Runtime:** Strands (green threads), channels, TCP I/O, arena allocation
**Type System:** Row polymorphic stack effects with type inference
**Editor Support:** LSP server with diagnostics (see [patch-seq.nvim](https://github.com/navicore/patch-seq.nvim) for Neovim)
**Missing:** Pattern matching, loops, full standard library

See `docs/ROADMAP.md` for the development plan and `CLAUDE.md` for current phase status.

## What's Different from cem2?

Seq separates **Values** (data) from **StackNodes** (stack structure):

- **Value**: Pure data (Int, Bool, String, Variant, Quotation, Closure)
- **StackNode**: Linked list node (value + next pointer)
- **Variant fields**: Stored in heap arrays, NOT linked via stack pointers

This separation prevents the data corruption bug that plagued cem2 during stack shuffling.

## Philosophy

**Foundation First:** Get the concatenative core bulletproof before adding advanced features.

**No Compromises:** If something doesn't feel clean, we stop and redesign.

**Learn from cem2:** cem2 taught us what happens when you conflate StackCell with Value. Seq does it right from the start.

## Quick Start

**Build:**
```bash
cargo build --release
```

**Compile and run a program:**
```bash
./target/release/seqc examples/hello-world.seq --output /tmp/hello
/tmp/hello
```

**Run tests:**
```bash
cargo test --all
```

## What Works

- **Stack operations:** dup, drop, swap, over, rot, nip, tuck, pick
- **Arithmetic:** +, -, *, / with overflow checking
- **Comparisons:** =, <, >, <=, >=, <> (return Int: 0 or 1)
- **Conditionals:** if/else/then
- **I/O:** write_line, read_line
- **Strings:** concat, split, trim, length, contains, starts-with, to-upper, to-lower
- **Quotations:** First-class functions with `call`, `times`, `while`, `until`, `forever`
- **Closures:** Captured environments with type-driven inference
- **Concurrency:** spawn, make-channel, send, receive, close-channel
- **TCP:** tcp-listen, tcp-accept, tcp-read, tcp-write, tcp-close
- **Type system:** Row polymorphic stack effects with unification

## Examples

Working examples in `examples/`:
- `hello-world.seq` - Basic I/O
- `test-if.seq`, `test-if-else.seq`, `test-nested-if.seq` - Conditionals
- `test-comparison.seq` - All comparison operators
- `test-pick.seq` - Stack manipulation
- `recursion/fibonacci.seq`, `recursion/factorial.seq` - Recursion
- `http/*.seq` - HTTP routing and TCP servers (see `examples/http/README.md`)

## Editor Support

The `seq-lsp` language server provides real-time diagnostics in your editor.

**Install:**
```bash
just install-lsp   # Installs to ~/.local/bin/seq-lsp
```

**Neovim:** Use [patch-seq.nvim](https://github.com/navicore/patch-seq.nvim) with Lazy:
```lua
{ "navicore/patch-seq.nvim", ft = "seq", opts = {} }
```

Features: Parse errors, type errors, undefined word detection, syntax highlighting.

## Documentation

- `docs/ARCHITECTURE.md` - System architecture and design decisions
- `docs/ROADMAP.md` - Development phases and milestones
- `docs/CLEAN_CONCATENATIVE_DESIGN.md` - Core design principles
- `docs/CELL_VS_VALUE_DESIGN.md` - Why we separate Value from StackNode
- `docs/CONCATENATIVE_CORE_INVARIANTS.md` - Invariants that must hold

## License

MIT License
