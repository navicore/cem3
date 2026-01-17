# seq-compiler

Compiler for the Seq programming language. Converts `.seq` source code to LLVM IR and links it with the runtime to produce native executables.

## Part of the Seq Workspace

This crate is part of the [Seq programming language](https://github.com/navicore/patch-seq) project.

### Related Crates

| Crate | Description |
|-------|-------------|
| [seq-compiler](https://crates.io/crates/seq-compiler) | Compiler and CLI (this crate) |
| [seq-runtime](https://crates.io/crates/seq-runtime) | Runtime library linked into compiled programs |
| [seq-lsp](https://crates.io/crates/seq-lsp) | Language Server Protocol implementation |
| [seq-repl](https://crates.io/crates/seq-repl) | Interactive TUI REPL |
| [vim-line](https://crates.io/crates/vim-line) | Vim-style line editor |

## Installation

```bash
cargo install seq-compiler
```

This installs the `seqc` binary for compiling Seq programs.

## Usage

```bash
# Compile a Seq program
seqc build program.seq -o program

# Run the compiled program
./program
```

## License

MIT
