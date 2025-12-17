# Include System Design

## Overview

Seq supports a simple include system for code reuse. The design prioritizes:
- Minimal filesystem exposure
- Clear provenance (stdlib vs user code)
- Collision detection with good error messages
- Future extensibility to packages

## Syntax

```seq
# Standard library (ships with compiler)
include std:http
include std:math

# FFI bindings (C library wrappers)
include ffi:libedit

# Relative to current file
include "my-utils"
include "lib/helpers"
```

## Rules

1. **`std:` prefix** - References stdlib bundled with compiler
   - Compiler knows where stdlib lives (not user's concern)
   - Example: `include std:http` loads `http.seq` from stdlib

2. **`ffi:` prefix** - References FFI bindings for C libraries
   - Some bindings ship with compiler (e.g., `ffi:libedit`)
   - Others require `--ffi-manifest` flag with a TOML manifest
   - Example: `include ffi:libedit` loads readline-style functions
   - See [FFI_GUIDE.md](FFI_GUIDE.md) for full documentation

3. **Quoted string** - Path relative to the including file
   - No absolute paths allowed
   - Paths can use `..` to reference parent directories
   - Example: `include "lib/foo"` loads `./lib/foo.seq`
   - Example: `include "../src/utils"` from a tests directory
   - Example: `include "../../src/tokenizer"` for deeper nesting

4. **Extension omitted** - Compiler adds `.seq` automatically

5. **Include once** - Files are included at most once per compilation
   - Duplicate includes are silently ignored
   - Prevents diamond dependency issues

## Collision Detection

If the same word is defined in multiple files:

```
Error: Word 'http-ok' is defined multiple times:
  - stdlib/http.seq:45
  - ./my-utils.seq:12

Hint: Rename one of the definitions to avoid collision.
```

All definitions are still global (no namespaces), but collisions are caught at compile time.

## Implementation Notes

### Parser Changes

1. Add `include` as keyword
2. Parse `include std:name` and `include "path"` forms
3. Return include statements as part of AST

### Compilation Pipeline

1. **Resolve includes** - Before parsing main file:
   - Scan for include statements
   - Resolve paths (stdlib vs relative)
   - Load and parse included files
   - Recursively process their includes
   - Track included files to prevent duplicates

2. **Merge programs** - Combine all WordDefs into single Program

3. **Check collisions** - Before type checking:
   - Build map of word name -> definition location
   - Error if any word has multiple definitions

4. **Continue normally** - Type check and codegen as before

### Stdlib Location

The compiler needs to know where stdlib lives. Options:
- Compiled-in path (simplest)
- Environment variable `SEQ_STDLIB`
- Relative to compiler binary

For now: **compiled-in path** or path relative to compiler binary.

### Path Validation

Include paths are validated:

1. **Absolute Path Rejection** - Absolute paths are rejected; all includes must be relative
2. **Empty Path Validation** - Empty include paths are rejected
3. **Canonicalization** - Paths are canonicalized to resolve symlinks and normalize `..` segments
4. **File Must Exist** - The target `.seq` file must exist

## Future Extensions

### User Library Paths

Could add compiler flag: `seqc --lib ./my-libs ...`

Then: `include lib:utils` would search `./my-libs/utils.seq`

### Package System

Could add: `include pkg:json`

Would search `~/.seq/packages/json/` or similar.

### Selective Imports

Could add: `include std:http { http-ok, http-not-found }`

Only imports specified words (helps with collisions).

---

## Examples

### Simple Program

```seq
include std:http

: main ( -- Int )
  "Hello" http-ok write_line
  0
;
```

### With Local Utils

```seq
include std:http
include "utils"

: main ( -- Int )
  get-greeting http-ok write_line
  0
;
```

Where `utils.seq` in same directory:
```seq
: get-greeting ( -- String )
  "Hello from utils!"
;
```

### Collision Error

```seq
include std:http
include "my-http"   # Also defines http-ok
```

```
Error: Word 'http-ok' is defined multiple times:
  - stdlib/http.seq:45
  - ./my-http.seq:3
```
