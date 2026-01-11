# seq-repl

Interactive TUI REPL for the Seq programming language with vim-style editing and IR visualization.

## Part of the Seq Workspace

This crate is part of the [Seq programming language](https://github.com/navicore/patch-seq) project.

### Related Crates

| Crate | Description |
|-------|-------------|
| [seq-compiler](https://crates.io/crates/seq-compiler) | Compiler and CLI |
| [seq-runtime](https://crates.io/crates/seq-runtime) | Runtime library |
| [seq-lsp](https://crates.io/crates/seq-lsp) | Language Server Protocol implementation |
| [seq-repl](https://crates.io/crates/seq-repl) | Interactive TUI REPL (this crate) |
| [vim-line](https://crates.io/crates/vim-line) | Vim-style line editor |

## Installation

```bash
cargo install seq-repl
```

This installs the `seqr` binary.

## Usage

```bash
# Start the REPL
seqr

# Start with IR panel visible
seqr --show-ir
```

## Features

- Vim-style line editing (normal/insert modes)
- Tab completion via LSP
- LLVM IR visualization panel
- Persistent command history
- Multi-line input support

## License

MIT
