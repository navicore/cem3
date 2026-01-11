# seq-lsp

Language Server Protocol (LSP) implementation for the Seq programming language. Provides IDE features like autocompletion, hover information, and diagnostics.

## Part of the Seq Workspace

This crate is part of the [Seq programming language](https://github.com/navicore/patch-seq) project.

### Related Crates

| Crate | Description |
|-------|-------------|
| [seq-compiler](https://crates.io/crates/seq-compiler) | Compiler and CLI |
| [seq-runtime](https://crates.io/crates/seq-runtime) | Runtime library |
| [seq-lsp](https://crates.io/crates/seq-lsp) | Language Server Protocol implementation (this crate) |
| [seq-repl](https://crates.io/crates/seq-repl) | Interactive TUI REPL |
| [vim-line](https://crates.io/crates/vim-line) | Vim-style line editor |

## Installation

```bash
cargo install seq-lsp
```

This installs the `seq-lsp` binary.

## Editor Integration

### VS Code

Install the Seq extension from the VS Code marketplace, or configure the LSP manually.

### Neovim

Add to your LSP configuration:

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "seq",
  callback = function()
    vim.lsp.start({
      name = "seq-lsp",
      cmd = { "seq-lsp" },
    })
  end,
})
```

## License

MIT
