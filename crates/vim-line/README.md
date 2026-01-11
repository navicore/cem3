# vim-line

A vim-style single-line editor library for Rust TUI applications. Provides modal editing with normal and insert modes, common vim motions, and customizable keybindings.

## Part of the Seq Workspace

This crate is part of the [Seq programming language](https://github.com/navicore/patch-seq) project, but can be used independently in any Rust TUI application.

### Related Crates

| Crate | Description |
|-------|-------------|
| [seq-compiler](https://crates.io/crates/seq-compiler) | Compiler and CLI |
| [seq-runtime](https://crates.io/crates/seq-runtime) | Runtime library |
| [seq-lsp](https://crates.io/crates/seq-lsp) | Language Server Protocol implementation |
| [seq-repl](https://crates.io/crates/seq-repl) | Interactive TUI REPL |
| [vim-line](https://crates.io/crates/vim-line) | Vim-style line editor (this crate) |

## Features

- Modal editing (normal/insert modes)
- Common vim motions: `h`, `l`, `w`, `b`, `e`, `0`, `$`, `^`
- Editing commands: `x`, `dd`, `D`, `C`, `r`, `~`
- Text objects: `iw`, `aw`
- Undo/redo support
- Works with ratatui

## License

MIT
