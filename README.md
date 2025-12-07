# Seq - Concatenative Language

A concatenative, stack-based programming language with static typing, tail call optimization, and CSP-style concurrency.

## Status

**Compiler:** Functional (compiles .seq source to native executables via LLVM IR)
**Runtime:** Strands (green threads), channels, TCP I/O, arena allocation
**Tail Call Optimization:** Guaranteed TCO for recursive functions via LLVM `musttail`
**Type System:** Row polymorphic stack effects with type inference
**Standard Library:** JSON, YAML, HTTP, math utilities
**Editor Support:** LSP server with diagnostics and completions

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
- Bitwise: `band`, `bor`, `bxor`, `bnot`, `shl`, `shr`, `popcount`, `clz`, `ctz`
- Numeric literals: decimal, hex (`0xFF`), binary (`0b1010`)
- Comparisons: `=`, `<`, `>`, `<=`, `>=`, `<>`
- Conditionals: `if`/`else`/`then`
- Quotations: First-class functions with `call`, `times`, `while`, `until`
- Closures: Captured environments with type-driven inference

**Tail Call Optimization:**
- Guaranteed TCO via LLVM's `musttail` and `tailcc` calling convention
- Recursive functions execute in constant stack space (100k+ calls tested)
- Mutual recursion fully supported
- Quotation calls (`call` word) are TCO-eligible
- Closures use Arc-based environments for efficient tail calls

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
- `hackers-delight/*.seq` - Bit manipulation puzzles (rightmost bits, power of 2, popcount, branchless ops)

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
- `docs/TCO_DESIGN.md` - Tail call optimization implementation
- `docs/TYPE_SYSTEM_GUIDE.md` - Type system and stack effects
- `docs/language-guide.md` - Language syntax and semantics
- `docs/ROADMAP.md` - Development phases and milestones

## License

MIT License
