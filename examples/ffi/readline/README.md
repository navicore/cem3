# GNU Readline FFI Example

This example demonstrates using GNU Readline for line editing and history,
loaded via an external FFI manifest with `--ffi-manifest`.

## License Warning

**GNU Readline is GPL-3.0 licensed.** Any binary you create that links to
libreadline must be distributed under a GPL-compatible license.

If you need a permissively-licensed alternative, use `include ffi:libedit`
instead. libedit is BSD-licensed and provides the same API.

## Building

Build using the external manifest:

```bash
seqc --ffi-manifest examples/ffi/readline/readline.toml \
     examples/ffi/readline/readline-demo.seq \
     -o readline-demo
./readline-demo
```

## Dependencies

- **macOS**: `brew install readline`
- **Ubuntu/Debian**: `apt install libreadline-dev`
- **Fedora**: `dnf install readline-devel`
