# Foreign Function Interface (FFI) Guide

## Overview

Seq's FFI system enables calling external C libraries from Seq programs. FFI is
purely a compiler/linker concern - the runtime remains free of external
dependencies, preserving Seq's minimal footprint.

## Quick Start

### Built-in FFI: libedit

The compiler includes a BSD-licensed libedit binding for readline-style input:

```seq
include ffi:libedit

: main ( -- Int )
  "prompt> " readline
  "You entered: " swap string-concat write_line
  0
;
```

Build with:
```bash
seqc build myprogram.seq -o myprogram
```

### External FFI: SQLite

For libraries not bundled with the compiler, use `--ffi-manifest`:

```seq
include ffi:sqlite

: main ( -- Int )
  ":memory:" db-open drop
  "CREATE TABLE test (id INT)" db-exec drop
  db-close drop
  0
;
```

Build with:
```bash
seqc --ffi-manifest sqlite.toml myprogram.seq -o myprogram
```

## Include Syntax

FFI bindings are accessed via the `ffi:` prefix in include statements:

```seq
include ffi:libedit    # Built-in manifest (ships with compiler)
include ffi:sqlite     # Must provide --ffi-manifest sqlite.toml
```

The library name after `ffi:` must match the `name` field in the manifest.

## Writing FFI Manifests

FFI bindings are declared in TOML files. Here's a complete example:

```toml
[[library]]
name = "mylib"
link = "mylib"           # Passed to linker as -lmylib

[[library.function]]
c_name = "my_function"   # C function name
seq_name = "my-func"     # Seq word name
stack_effect = "( Int String -- Int )"
args = [
  { type = "int", pass = "int" },
  { type = "string", pass = "c_string" }
]
[library.function.return]
type = "int"
```

### Type Mappings

| Manifest Type | C Type        | Seq Type | Notes                    |
|---------------|---------------|----------|--------------------------|
| `int`         | `int`/`long`  | `Int`    | 64-bit on Seq side       |
| `string`      | `char*`       | `String` | Null-terminated          |
| `ptr`         | `void*`       | `Int`    | Raw pointer as integer   |
| `void`        | `void`        | (nothing)| No return value          |

### Pass Modes

The `pass` field controls how arguments are passed to C:

| Mode       | Description                                      |
|------------|--------------------------------------------------|
| `c_string` | Convert Seq String to null-terminated `char*`    |
| `ptr`      | Pass raw pointer value (Int on stack)            |
| `int`      | Pass as C integer                                |
| `by_ref`   | Allocate storage, pass pointer (for out params)  |

### Memory Ownership

The `ownership` field on returns controls memory management:

| Mode           | Description                          | Codegen                    |
|----------------|--------------------------------------|----------------------------|
| `caller_frees` | C malloc'd it, we must free          | Generates `free()` call    |
| `static`       | Library owns memory, don't free      | Just copy, no free         |
| `borrowed`     | Only valid during call               | Copy immediately           |

## Advanced Features

### Out Parameters (`by_ref`)

Some C functions return values via pointer parameters (out params). Use `by_ref`
pass mode:

```toml
[[library.function]]
c_name = "sqlite3_open"
seq_name = "db-open"
stack_effect = "( String -- Int Int )"   # db handle + return code
args = [
  { type = "string", pass = "c_string" },
  { type = "ptr", pass = "by_ref" }       # Out parameter
]
[library.function.return]
type = "int"
```

For `by_ref` arguments:
1. Compiler allocates local storage
2. Passes pointer to that storage to C function
3. After call, reads value and pushes onto stack

**Important**: The `by_ref` value is an opaque handle owned by the C library.
You must:
- Only pass it to functions from the same library
- Never attempt to free it manually
- Always use the library's cleanup function (e.g., `db-close`)

### Fixed Value Arguments

For arguments that should always be a constant (like NULL callbacks):

```toml
args = [
  { type = "ptr", pass = "ptr" },
  { type = "string", pass = "c_string" },
  { type = "ptr", value = "null" },    # Always passes NULL
  { type = "ptr", value = "null" },
  { type = "ptr", value = "null" }
]
```

Fixed value arguments don't consume stack values - they're compiled as constants.
Supported values: `null`, `NULL`, or integer literals.

### Multiple Manifests

You can load multiple FFI manifests:

```bash
seqc --ffi-manifest db.toml --ffi-manifest crypto.toml program.seq -o program
```

## Safety Model

FFI is inherently unsafe - you're calling into C code that can do anything.
Seq's approach:

1. **Opt-in boundary**: Using `include ffi:*` is the explicit safety boundary
2. **Stack effects enforced**: Type checker validates declared effects
3. **Memory managed by codegen**: Ownership annotations prevent leaks
4. **Linker flag validation**: Only safe characters allowed in link names

If you don't use FFI, your Seq program has full memory safety.

### Security Considerations

- **Trust your manifests**: Malicious manifests could link arbitrary libraries
- **Validate external manifests**: Review manifests before using `--ffi-manifest`
- **Linker injection prevented**: Link names can only contain alphanumeric,
  dash, underscore, and dot characters

## Built-in Libraries

### `ffi:libedit`

BSD-licensed readline alternative. Provides:

| Word           | Stack Effect           | Description                    |
|----------------|------------------------|--------------------------------|
| `readline`     | `( String -- String )` | Read line with prompt          |
| `add-history`  | `( String -- )`        | Add line to history            |
| `read-history` | `( String -- Int )`    | Load history from file         |
| `write-history`| `( String -- Int )`    | Save history to file           |

## Examples

### Example: Interactive REPL

```seq
include ffi:libedit

: repl ( -- )
  "seq> " readline
  dup string-length 0 > if
    dup add-history
    process-input      # Your processing here
    repl
  else
    drop
  then
;

: main ( -- Int )
  "Welcome to Seq REPL" write_line
  repl
  0
;
```

### Example: Persistent History

```seq
include ffi:libedit

: repl ( -- )
  "seq> " readline
  dup string-length 0 > if
    dup add-history
    process-input
    repl
  else
    drop
  then
;

: main ( -- Int )
  # Load history at startup (ignore error if file doesn't exist)
  "/tmp/.myapp_history" read-history drop

  "Welcome to Seq REPL" write_line
  repl

  # Save history on exit
  "/tmp/.myapp_history" write-history drop
  0
;
```

**Note:** File paths are passed directly to the C library. Shell expansions
like `~` are not performed - path resolution is your application's responsibility.
A future `std:os` module could provide environment variable access for building
paths like `$HOME/.myapp_history`.

### Example: SQLite Database

See `examples/ffi/sqlite/` for a complete SQLite example demonstrating:
- `by_ref` out parameters for database handles
- Fixed `null` values for unused callbacks
- Proper handle cleanup with `db-close`

## Troubleshooting

### "Unknown word: readline"

Ensure you have `include ffi:libedit` at the top of your file.

### "Unknown FFI library: sqlite"

You need to provide the manifest: `--ffi-manifest path/to/sqlite.toml`

### Linker errors

Install the C library's development package:
- macOS: `brew install <library>`
- Ubuntu: `apt install lib<library>-dev`
- Fedora: `dnf install <library>-devel`

### "Invalid character in linker flag"

Link names can only contain: `a-z`, `A-Z`, `0-9`, `-`, `_`, `.`

This prevents command injection attacks via malicious manifests.

## Future Work

- **Struct passing**: Pass and return C structs
- **Platform-specific bindings**: Conditional compilation per target

### Callbacks (Shelved)

FFI callbacks (C functions calling back into Seq) were explored but shelved for now.

**Why shelved:**
- Most useful callback patterns (qsort comparators, iteration handlers) pass pointers
  to the callback, requiring low-level memory operations to interpret them
- Those low-level operations (`ptr-read-i64`, etc.) are too invasive for Seq's design
- Many C APIs have non-callback alternatives (e.g., SQLite's prepared statement API
  works without callbacks)

Callbacks may be revisited when there's a concrete use case that justifies the
complexity.

See [ROADMAP.md](ROADMAP.md) for the full FFI roadmap.
