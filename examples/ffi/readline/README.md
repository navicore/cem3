# GNU Readline FFI Example

This example demonstrates using GNU Readline for line editing and history.

## License Warning

**GNU Readline is GPL-3.0 licensed.** Any binary you create that links to
libreadline must be distributed under a GPL-compatible license.

If you need a permissively-licensed alternative, use `include ffi:libedit`
instead. libedit is BSD-licensed and provides the same API.

## Building

Once `--ffi-manifest` is implemented:

```bash
seqc --ffi-manifest readline.toml -o readline-demo readline-demo.seq
```

Or use the embedded manifest (accepting GPL for your binary):

```bash
seqc -o readline-demo readline-demo.seq
```

## Dependencies

- **macOS**: `brew install readline`
- **Ubuntu/Debian**: `apt install libreadline-dev`
- **Fedora**: `dnf install readline-devel`
