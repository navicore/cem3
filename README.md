[![CI - macOS](https://github.com/navicore/patch-seq/actions/workflows/ci-macos.yml/badge.svg)](https://github.com/navicore/patch-seq/actions/workflows/ci-macos.yml)
[![CI - Linux](https://github.com/navicore/patch-seq/actions/workflows/ci-linux.yml/badge.svg)](https://github.com/navicore/patch-seq/actions/workflows/ci-linux.yml)
[![Release with Auto Version](https://github.com/navicore/patch-seq/actions/workflows/release.yml/badge.svg)](https://github.com/navicore/patch-seq/actions/workflows/release.yml)

[![seq-compiler](https://img.shields.io/crates/v/seq-compiler.svg?label=seq-compiler)](https://crates.io/crates/seq-compiler)
[![seq-repl](https://img.shields.io/crates/v/seq-repl.svg?label=seq-repl)](https://crates.io/crates/seq-repl)
[![seq-lsp](https://img.shields.io/crates/v/seq-lsp.svg?label=seq-lsp)](https://crates.io/crates/seq-lsp)
[![seq-runtime](https://img.shields.io/crates/v/seq-runtime.svg?label=seq-runtime)](https://crates.io/crates/seq-runtime)
[![vim-line](https://img.shields.io/crates/v/vim-line.svg?label=vim-line)](https://crates.io/crates/vim-line)

[![seq-compiler docs](https://img.shields.io/docsrs/seq-compiler?label=seq-compiler%20docs)](https://docs.rs/seq-compiler)
[![seq-runtime docs](https://img.shields.io/docsrs/seq-runtime?label=seq-runtime%20docs)](https://docs.rs/seq-runtime)
[![vim-line docs](https://img.shields.io/docsrs/vim-line?label=vim-line%20docs)](https://docs.rs/vim-line)

# Seq - Concatenative Language

A concatenative, stack-based programming language that compiles to native executables. Seq combines the elegance of stack-based programming with a sophisticated type system, guaranteed tail call optimization, and CSP-style concurrency.

## Project Status

**Stable as of 1.1.0.** The language and standard library are stable and used by the creators for their own projects. That said, Seq is a niche experimental language - adopt it with eyes open. Future versions follow strict semantic versioning: major version increments indicate breaking changes to the language or standard library. Minor and patch versions add features and fixes without breaking existing code.

```seq
: factorial ( Int -- Int )
  dup 1 i.<= if
    drop 1
  else
    dup 1 i.- factorial i.*
  then
;

: main ( -- ) 10 factorial int->string io.write-line ;
```

## Why Seq?

**Stack-based simplicity.** No variable declarations, no argument lists - values flow through the stack. Code reads left-to-right as a pipeline of transformations.

**Strongly typed with effect tracking.** Stack effects aren't just comments - they're enforced by the compiler. The type system tracks not only what goes on and off the stack, but also side effects like yielding from generators:

```seq
: counter ( Ctx Int -- Ctx | Yield Int )   # Yields integers, takes a context
  dup yield drop
  1 i.+ counter
;
```

**Guaranteed tail call optimization.** Recursive functions run in constant stack space via LLVM's `musttail`. Write elegant recursive algorithms without stack overflow concerns.

**CSP-style concurrency.** Lightweight strands (green threads) communicate through channels. No shared memory, no locks - just message passing.

## Installation

### Prerequisites

**clang** is required to compile Seq programs (used to compile LLVM IR to native executables):
- macOS: `xcode-select --install`
- Ubuntu/Debian: `apt install clang`
- Fedora: `dnf install clang`

### Install from crates.io

```bash
cargo install seq-compiler  # installs seqc (compiler)
cargo install seq-repl      # installs seqr (interactive REPL)
cargo install seq-lsp       # installs seq-lsp (optional, for editor support)
```

### Build from source

```bash
cargo build --release
```

### Virtual Environments

Create isolated environments to manage multiple Seq versions or pin a specific version for a project:

```bash
seqc venv myenv
source myenv/bin/activate
```

This copies the `seqc`, `seqr`, and `seq-lsp` binaries into `myenv/bin/`, completely isolated from your system installation. Unlike Python's venv (which uses symlinks), Seq copies binaries so your project won't break if the system Seq is updated.

**Activate/deactivate:**
```bash
source myenv/bin/activate   # Prepends myenv/bin to PATH, shows (myenv) in prompt
deactivate                  # Restores original PATH
```

Supports bash, zsh, fish (`activate.fish`), and csh/tcsh (`activate.csh`).

## Quick Start

**Compile and run a program:**
```bash
seqc build examples/hello-world.seq
./hello-world
```

**Check version:**
```bash
seqc --version
```

**Run tests:**
```bash
cargo test --all
```

## Learn Seq

**New to concatenative programming?** Start with the [Glossary](docs/GLOSSARY.md) - it explains concepts like stack effects, quotations, row polymorphism, and CSP in plain terms for working programmers.

**Learn by doing:** Work through [seqlings](https://github.com/navicore/seqlings) - hands-on exercises that teach the language step by step, covering stack operations, arithmetic, control flow, quotations, and more. Each exercise includes hints and automatic verification.

## Interactive REPL

The `seqr` REPL provides an interactive environment for exploring Seq:

```bash
seqr
```

**Stack persists across lines:**
```
seqr> 1 2
stack: 1 2
seqr> i.+
stack: 3
seqr> 5
stack: 3 5
seqr> : square ( Int -- Int ) dup i.* ;
Defined.
seqr> square
stack: 3 25
```

**Commands:**
- `:clear`  - Reset session
- `:edit`   - Open in $EDITOR
- `:pop`    - Remove last expression (undo)
- `:quit`   - Exit
- `:show`   - Show current file
- `:stack`  - Show current stack

**Editing:**
- Vi-mode editing (Esc for normal mode, i for insert)
- **Shift+Enter** - Insert newline for multiline input
- **Tab** - Trigger completions
- **F1/F2/F3** - Toggle IR pane views (Stack/AST/LLVM)

## Language Features

### Stack Operations & Arithmetic

```seq
dup drop swap over rot nip tuck pick 2dup 3drop   # Stack manipulation
i.+ i.- i.* i./ i.%                               # Integer arithmetic
f.+ f.- f.* f./                                   # Float arithmetic
i.= i.< i.> i.<= i.>= i.<>                        # Comparisons
band bor bxor bnot shl shr popcount               # Bitwise operations
```

Numeric literals support decimal, hex (`0xFF`), and binary (`0b1010`).

### Algebraic Data Types

Define sum types with `union` and pattern match with `match`:

```seq
union Option { None, Some { value: Int } }

: unwrap-or ( Option Int -- Int )
  swap match
    None ->
    Some { >value } -> nip
  end
;
```

### Quotations & Higher-Order Programming

Quotations are first-class anonymous functions:

```seq
[ dup i.* ] 5 swap call    # Square 5 → 25
my-list [ 2 i.* ] list.map # Double each element
```

### Concurrency

Strands (green threads) communicate through channels:

```seq
make-channel
[ 42 swap chan.send ] strand.spawn
chan.receive    # Receives 42
```

Weaves provide generator-style coroutines with bidirectional communication:

```seq
[ my-generator ] strand.weave
initial-value strand.resume   # Yields values back and forth
```

### Standard Library

Import modules with `include std:module`:

| Module | Purpose |
|--------|---------|
| `std:json` | JSON parsing and serialization |
| `std:yaml` | YAML parsing and serialization |
| `std:http` | HTTP request/response utilities |
| `std:math` | Mathematical functions |
| `std:stack-utils` | Stack manipulation utilities |

## Examples

The `examples/` directory contains programs demonstrating various features:

| Directory | What it shows |
|-----------|---------------|
| `hello-world.seq` | Basic I/O |
| `recursion/` | Tail-recursive algorithms (fibonacci, factorial) |
| `json/` | JSON parsing with the standard library |
| `http/` | HTTP routing and TCP servers |
| `weave/` | Generator patterns and coroutines |
| `csp/` | Channel-based actor patterns |
| `hackers-delight/` | Bit manipulation puzzles |
| `lisp/` | A Lisp interpreter in Seq |

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

## Configuration

**Environment Variables:**

| Variable | Default | Description |
|----------|---------|-------------|
| `SEQ_STACK_SIZE` | 1048576 (1MB) | Coroutine stack size in bytes. Increase if you hit stack overflow in deeply nested (non-tail) calls. |
| `SEQ_YIELD_INTERVAL` | 0 (disabled) | Yield to scheduler every N tail calls. Prevents tight recursive loops from starving other strands. |
| `SEQ_WATCHDOG_SECS` | 0 (disabled) | Detect strands running longer than N seconds. See `crates/runtime/src/watchdog.rs` for details. |

Examples:
```bash
SEQ_STACK_SIZE=2097152 ./my-program      # 2MB stacks
SEQ_YIELD_INTERVAL=10000 ./my-program    # Yield every 10K tail calls
SEQ_WATCHDOG_SECS=30 ./my-program        # Warn if strand runs >30s
```

## License

MIT License
