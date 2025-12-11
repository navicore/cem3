# FFI Design for Seq

## Overview

Seq's FFI system enables calling external C libraries without polluting the core language or runtime. FFI is purely a compiler/linker concern - the runtime remains free of external dependencies.

## Goals

1. **No runtime pollution** - seq-runtime has zero FFI dependencies
2. **Opt-in** - users explicitly `include ffi:*` to access external libraries
3. **Build flexibility** - enable/disable FFI bindings per target (e.g., no readline for musl static builds)
4. **User extensible** - anyone can write a manifest for any C library
5. **Type safe** - stack effects declared and verified at compile time

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Seq Source                               │
│   include ffi:readline                                          │
│   "prompt> " readline                                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Compiler                                  │
│   1. Parse include ffi:readline                                 │
│   2. Load manifest (embedded or user-provided)                  │
│   3. Register words with stack effects                          │
│   4. Generate marshalling LLVM IR                               │
│   5. Add -lreadline to linker flags                             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      LLVM IR Output                              │
│   declare ptr @readline(ptr)          ; External C function     │
│   declare void @free(ptr)             ; For memory cleanup      │
│                                                                  │
│   define ptr @seq_ffi_readline(ptr %stack) {                    │
│     ; pop prompt string from stack                              │
│     ; convert to C string (null-terminated)                     │
│     ; call @readline                                            │
│     ; copy result to Seq string                                 │
│     ; call @free on C string                                    │
│     ; push Seq string to stack                                  │
│     ; return stack                                              │
│   }                                                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                        Linker                                    │
│   clang output.ll -lreadline -o program                         │
└─────────────────────────────────────────────────────────────────┘
```

## Manifest Format

FFI bindings are declared in TOML manifests:

```toml
[[library]]
name = "readline"
link = "readline"           # -lreadline

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
args = [
  { type = "string", pass = "c_string" }
]
return = { type = "string", ownership = "caller_frees" }

[[library.function]]
c_name = "add_history"
seq_name = "add-history"
stack_effect = "( String -- )"
args = [
  { type = "string", pass = "c_string" }
]
return = { type = "void" }
```

### Type Mappings

| Manifest Type | C Type | Seq Type | Notes |
|---------------|--------|----------|-------|
| `int` | `int` / `long` | `Int` | 64-bit on Seq side |
| `string` | `char*` | `String` | Null-terminated |
| `ptr` | `void*` | `Int` | Raw pointer as integer |
| `void` | `void` | (nothing) | No stack effect |

### Argument Passing (`pass`)

| Mode | Description |
|------|-------------|
| `c_string` | Convert Seq String to null-terminated `char*` |
| `ptr` | Pass raw pointer value |
| `int` | Pass as C integer |
| `by_ref` | Pass pointer to value (for out params) |

### Memory Ownership (`ownership`)

| Mode | Description | Codegen |
|------|-------------|---------|
| `caller_frees` | C function malloc'd it, we free | Generate `free()` call after copy |
| `static` | Library owns, don't free | Just copy, no free |
| `borrowed` | Valid only during call | Copy immediately |

## Manifest Discovery

### Built-in FFI

Common bindings (readline, etc.) are embedded in the compiler:
- Location: `crates/compiler/ffi/`
- Loaded automatically for `include ffi:<name>`
- Can be disabled via build flags

### User FFI

Users can provide custom manifests:
- Compiler flag: `--ffi-manifest path/to/bindings.toml`
- Convention: `foo.seq-ffi` alongside `foo.seq`
- Multiple manifests supported

## Safety Model

FFI is inherently unsafe (calling into C code). Seq's approach:

1. **Implicit unsafe** - using `include ffi:*` is the opt-in
2. **No `unsafe` keyword** - would be noise since all FFI is unsafe
3. **Stack effects enforced** - type checker validates declared effects
4. **Memory managed by codegen** - ownership annotations prevent leaks

The safety boundary is the `include` statement. Users who don't use FFI get full memory safety.

## Implementation Phases

### Phase 1: Readline Support

Minimal implementation to prove the design:

1. Manifest parser for TOML format
2. Extend include system for `ffi:` prefix
3. LLVM IR generation for string marshalling
4. Memory management codegen (`caller_frees`)
5. Linker flag injection

Deliverable: `include ffi:readline` works

### Phase 2: Generalization

1. Full type mapping support
2. User manifest discovery (`--ffi-manifest`)
3. Out parameters (`by_ref`)
4. Error handling conventions
5. Documentation and examples

### Phase 3: Advanced Features

1. Struct support (pass/return C structs)
2. Callback support (C calling into Seq)
3. Conditional compilation (platform-specific bindings)
4. Package manager integration (download bindings)

## Examples

### Readline

```seq
include ffi:readline

: repl ( -- )
  "seqlisp> " readline
  dup string-empty not if
    dup add-history
    eval
    repl
  else
    drop
  then
;
```

### SQLite

```toml
# sqlite.ffi.toml
[[library]]
name = "sqlite"
link = "sqlite3"

[[library.function]]
c_name = "sqlite3_open"
seq_name = "db-open"
stack_effect = "( String -- Int Int )"
args = [{ type = "string", pass = "c_string" }]
out_params = [{ index = 1, type = "ptr" }]
return = { type = "int" }

[[library.function]]
c_name = "sqlite3_close"
seq_name = "db-close"
stack_effect = "( Int -- Int )"
args = [{ type = "ptr" }]
return = { type = "int" }

[[library.function]]
c_name = "sqlite3_exec"
seq_name = "db-exec"
stack_effect = "( Int String -- Int )"
args = [
  { type = "ptr" },
  { type = "string", pass = "c_string" },
  { type = "ptr", value = "null" },  # callback
  { type = "ptr", value = "null" },  # callback arg
  { type = "ptr", value = "null" }   # error msg (ignored)
]
return = { type = "int" }
```

```seq
include ffi:sqlite

: main ( -- Int )
  "test.db" db-open
  0 = if
    # db handle is on stack
    dup "CREATE TABLE users (id INT, name TEXT)" db-exec drop
    dup "INSERT INTO users VALUES (1, 'alice')" db-exec drop
    db-close drop
    "Database created!" write_line
    0
  else
    drop "Failed to open database" write_line
    1
  then
;
```

### PCRE (Regular Expressions)

```seq
include ffi:pcre

: main ( -- Int )
  "\\d+" regex-compile    # ( pattern -- regex )
  "abc123def" regex-match # ( regex string -- match? )
  if "Found digits!" else "No match" then
  write_line
  0
;
```

## Build System Integration

### Conditional FFI

```bash
# Desktop build with readline
seqc --ffi readline -o seqlisp seqlisp.seq

# Static musl build without readline
seqc --no-ffi readline -o seqlisp-static seqlisp.seq
```

### Custom Libraries

```bash
# User-provided SQLite bindings
seqc --ffi-manifest ./sqlite.ffi.toml -o myapp myapp.seq
```

## Open Questions

1. **Variadic functions** - how to handle `printf`-style signatures?
2. **Struct layout** - manual specification or auto-detect?
3. **Thread safety** - any special handling for non-thread-safe C libs?
4. **Error conventions** - standardize on Result types for FFI?

## References

- [LuaJIT FFI](https://luajit.org/ext_ffi.html) - similar declarative approach
- [Guile FFI](https://www.gnu.org/software/guile/manual/html_node/Foreign-Function-Interface.html)
- [LLVM C ABI](https://llvm.org/docs/LangRef.html#calling-conventions)
