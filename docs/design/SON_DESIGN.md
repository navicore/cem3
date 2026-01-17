# SON: Seq Object Notation

**Status:** Implemented (Core Features)

## Roadmap Issues

- [x] [#147 - List type for Seq](https://github.com/navicore/patch-seq/issues/147) ✅
- [x] [#148 - Symbol/keyword syntax (:name)](https://github.com/navicore/patch-seq/issues/148) ✅
- [x] [#149 - Dynamic variant construction (wrap)](https://github.com/navicore/patch-seq/issues/149) ✅
- [x] [#150 - SON security model](https://github.com/navicore/patch-seq/issues/150) ✅ Trust model documented
- [x] [#151 - Map builder word for SON](https://github.com/navicore/patch-seq/issues/151) ✅

## Motivation

The "JS" in JSON stands for JavaScript - JSON is a subset of JavaScript syntax that
can represent data structures. What would an equivalent look like for Seq?

SON (Seq Object Notation) would be a data serialization format that uses valid Seq
syntax, meaning SON documents are executable Seq code that reconstructs the data
when evaluated.

## Core Insight

In a concatenative language, data literals are already programs. A quotation like
`[1 2 3]` is code that, when called, pushes 1, 2, and 3 onto the stack. This means:

- **No separate parser needed** - SON is just Seq
- **Executable data** - `include` + `call` replaces `JSON.parse()`
- **Homoiconic** - code and data share the same representation

## Basic Examples

### Primitives

```seq
42              # Int
3.14            # Float
"hello world"   # String
true            # Bool
```

### Sequences (as quotations)

```seq
[1 2 3]                     # evaluates to: stack: 1 2 3
["a" "b" "c"]               # evaluates to: stack: "a" "b" "c"
```

### Maps

```seq
# Using builder pattern (recommended)
include "map"
[ map-of "name" "Alice" kv "age" 30 kv ]

# Or using builtins directly
[ map.make "name" "Alice" map.set "age" 30 map.set ]
```

### Lists

```seq
# Using builder pattern (recommended)
include "list"
[ list-of 1 lv 2 lv 3 lv ]

# Or using builtins directly
[ list.make 1 list.push 2 list.push 3 list.push ]
```

### Nested Structures

```seq
include "map"
include "list"

[
  map-of
    "users"
    list-of
      map-of "name" "Alice" kv "age" 30 kv lv
      map-of "name" "Bob" kv "age" 25 kv lv
    kv
    "count" 2 kv
]
```

### Variants (Tagged Unions)

```seq
[:some 42 wrap]             # Some(42)
[:none wrap]                # None
[:ok "success" wrap]        # Ok("success")
[:err "failed" wrap]        # Err("failed")
```

## Usage Pattern

```seq
# Loading SON data
include "map"
include "config.son"

: load-config ( -- config )
  config-data call ;

# In config.son:
: config-data ( -- quot )
  [
    map-of
      "host" "localhost" kv
      "port" 8080 kv
      "debug" true kv
  ] ;
```

## Design Questions

### 1. Quotation vs Immediate

Should SON data be wrapped in a quotation (deferred) or pushed immediately?

```seq
# Option A: Quotation (must call to get values)
: data ( -- quot ) [1 2 3] ;
data call  # stack: 1 2 3

# Option B: Immediate (values pushed at include time)
# Would need different semantics or a dedicated SON loader
```

### 2. Stack Representation

How do we represent "an array" vs "multiple values"?

```seq
[1 2 3]                             # pushes 3 values when called
[list-of 1 lv 2 lv 3 lv]            # pushes 1 list value when called
```

This is actually a feature - SON can represent both!

### 3. Type Preservation

Should SON preserve type information?

```seq
# Untyped (like JSON)
[42 "hello" true]

# Typed (with explicit type constructors)
[42 as-i64 "hello" as-string true as-bool]
```

### 4. Comments in Data

Since SON is valid Seq, comments are free:

```seq
[
  map-of
    # Database configuration
    "host" "prod-db.example.com" kv
    "port" 5432 kv

    # Connection pool settings
    "pool-size" 10 kv
    "timeout-ms" 5000 kv
]
```

### 5. File Extension

- `.son` - clear and memorable
- `.seq` - it's just Seq code anyway
- `.seq.data` - explicit about purpose

## Comparison with JSON

| Aspect | JSON | SON |
|--------|------|-----|
| Requires parser | Yes | No (it's Seq) |
| Comments | No | Yes |
| Trailing commas | No | N/A (no commas) |
| Executable | No | Yes |
| Homoiconic | No | Yes |
| Schema | JSON Schema | Seq types |

## Potential Standard Library

```seq
# SON loading utilities
: son.load ( path -- values )
  include call ;

: son.loads ( string -- values )
  # parse and evaluate SON from string
  ;

# SON generation (serialization)
: son.dump ( values -- string )
  # convert stack values to SON string
  ;
```

## ⚠️ Security Considerations

> **WARNING:** SON is executable Seq code. Loading untrusted SON is equivalent to
> running untrusted code. Only load SON from sources you trust.

Unlike JSON, which is safe to parse from any source, SON files can contain
arbitrary Seq code including:
- File system access (`file-slurp`, `path-exists`, etc.)
- Network operations (`tcp.*`)
- Shell command execution (if enabled)
- Any other side effects available in Seq

### Current Security Model (Trust-Based)

The current approach is trust-based:
- Only load `.son` files from trusted sources
- Never load SON from untrusted network sources
- Treat SON files like any other executable code

```seq
# SAFE: Loading from your own project
include "config.son"

# DANGEROUS: Never do this!
# include "https://untrusted-source.com/data.son"
```

### Future: Sandboxed Evaluation

The loader API is designed to support sandboxed evaluation in the future:

```seq
# Phase 1 (current): Trust model
: son.load ( path -- values )
  include call ;

# Phase 2 (future): Sandboxed version
: son.load-safe ( path -- values )
  # Would restrict available words during evaluation
  # Only allow: literals, map.*, list.*, wrap, call
  # Forbid: io.*, tcp.*, file-*, strand.*, etc.
  ;
```

### Mitigation Options (Future)

1. **Sandboxed evaluation** - restrict available words during SON evaluation
2. **Static subset** - define a "pure data" subset of Seq that's safe to parse
3. **Capability-based** - explicitly grant capabilities to SON loaders

## Future Directions

- Schema validation using Seq's type system
- Streaming SON for large datasets
- Binary SON format for efficiency
- SON-RPC (like JSON-RPC but with SON payloads)

---

*This design emerged from a discussion about stack display formats in the REPL.
The observation that `[]` was confusing (since it's quotation syntax) led to
the insight that Seq's syntax could serve as its own serialization format.*
