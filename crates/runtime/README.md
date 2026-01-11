# seq-runtime

Runtime library for the Seq programming language. This library is embedded into compiled Seq programs and provides core operations like stack manipulation, I/O, concurrency primitives, and more.

## Part of the Seq Workspace

This crate is part of the [Seq programming language](https://github.com/navicore/patch-seq) project.

### Related Crates

| Crate | Description |
|-------|-------------|
| [seq-compiler](https://crates.io/crates/seq-compiler) | Compiler and CLI |
| [seq-runtime](https://crates.io/crates/seq-runtime) | Runtime library (this crate) |
| [seq-lsp](https://crates.io/crates/seq-lsp) | Language Server Protocol implementation |
| [seq-repl](https://crates.io/crates/seq-repl) | Interactive TUI REPL |
| [vim-line](https://crates.io/crates/vim-line) | Vim-style line editor |

## Note

This crate is not intended for direct use. It is automatically embedded by the Seq compiler when building Seq programs.

## License

MIT
