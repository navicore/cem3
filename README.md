# Seq - Concatenative Language

A concatenative, stack-based programming language with static typing and CSP-style concurrency.

## Status

**Compiler:** Functional (compiles .seq source to native executables via LLVM IR)
**Runtime:** Strands (green threads), channels, TCP I/O, arena allocation
**Type System:** Row polymorphic stack effects with type inference
**Standard Library:** JSON, YAML, HTTP, math utilities
**Editor Support:** LSP server with diagnostics and completions
**Missing:** Pattern matching, loops

See `docs/ROADMAP.md` for the development plan.

## Installation

### Prerequisites

**clang** is required to compile Seq programs (used to compile LLVM IR to native executables):
- macOS: `xcode-select --install`
- Ubuntu/Debian: `apt install clang`
- Fedora: `dnf install clang`

### Install from crates.io

```bash
cargo install seq-compiler  # installs seqc
cargo install seq-lsp       # installs seq-lsp (optional, for editor support)
```

### Build from source

```bash
cargo build --release
```

## Quick Start

**Compile and run a program:**
```bash
seqc examples/hello-world.seq --output /tmp/hello
/tmp/hello
```

**Check version:**
```bash
seqc --version
```

**Run tests:**
```bash
cargo test --all
```

## What Works

**Core Language:**
- Stack operations: `dup`, `drop`, `swap`, `over`, `rot`, `nip`, `tuck`, `pick`
- Arithmetic: `+`, `-`, `*`, `/` with overflow checking
- Comparisons: `=`, `<`, `>`, `<=`, `>=`, `<>`
- Conditionals: `if`/`else`/`then`
- Quotations: First-class functions with `call`, `times`, `while`, `until`, `forever`
- Closures: Captured environments with type-driven inference

**I/O and Strings:**
- Console: `write_line`, `read_line`
- Files: `file-read`, `file-write`, `file-exists?`
- Strings: `concat`, `split`, `trim`, `length`, `contains`, `starts-with`, `to-upper`, `to-lower`

**Concurrency:**
- Strands: `spawn` (green threads)
- Channels: `make-channel`, `send`, `receive`, `close-channel`
- TCP: `tcp-listen`, `tcp-accept`, `tcp-read`, `tcp-write`, `tcp-close`

**Standard Library** (via `include std:module`):
- `std:json` - JSON parsing and serialization
- `std:yaml` - YAML parsing and serialization
- `std:http` - HTTP request/response utilities
- `std:math` - Mathematical functions
- `std:stack-utils` - Stack manipulation utilities

## Examples

See `examples/` for working programs:
- `hello-world.seq` - Basic I/O
- `recursion/fibonacci.seq`, `recursion/factorial.seq` - Recursion
- `json/json_tree.seq` - JSON parsing with the stdlib
- `http/*.seq` - HTTP routing and TCP servers

## Editor Support

The `seq-lsp` language server provides IDE features in your editor.

**Install:**
```bash
cargo install seq-lsp
```

**Neovim:** Use [patch-seq.nvim](https://github.com/navicore/patch-seq.nvim) with Lazy:
```lua
{ "navicore/patch-seq.nvim", ft = "seq", opts = {} }
```

**Features:**
- Real-time diagnostics (parse errors, type errors, undefined words)
- Autocompletion for builtins, local words, and included modules
- Context-aware completions (stack effects, include statements)
- Syntax highlighting

## Documentation

- `docs/ARCHITECTURE.md` - System architecture and design decisions
- `docs/ROADMAP.md` - Development phases and milestones
- `docs/CLEAN_CONCATENATIVE_DESIGN.md` - Core design principles
- `docs/CELL_VS_VALUE_DESIGN.md` - Why we separate Value from StackNode
- `docs/CONCATENATIVE_CORE_INVARIANTS.md` - Invariants that must hold

## License

MIT License
