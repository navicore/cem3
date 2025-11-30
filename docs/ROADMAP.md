# Seq Development Roadmap

## Current State

Seq is a functional concatenative language with:
- Static typing with row-polymorphic stack effects
- Green thread concurrency (strands) with channels
- Standard library (JSON, YAML, HTTP, math)
- LSP with diagnostics and completions

## Near-Term Goals

### LSP Enhancements

1. **Hover for word signatures** - Show stack effect when hovering over a word
2. **Go-to-definition** - Navigate to word definitions

### Closure Capture Support

Closures can capture these types:

- ✅ **Int** - `env_get_int` returns i64
- ✅ **String** - `env_get_string` returns SeqString
- ✅ **Bool** - `env_get_bool` returns i64 (0/1)
- ✅ **Float** - `env_get_float` returns f64
- ✅ **Quotation** - `env_get_quotation` returns function pointer as i64
- ⏳ **Variant** - Compound type, more complex (tagged union with fields)
- ⏳ **Closure** - Nested closures with their own environments

Each type requires:
- Runtime: `env_get_<type>` function in `closures.rs`
- CodeGen: Match arm in closure code generation

### Stdlib Improvements

1. ✅ **JSON escape handling** - Properly escape special characters in `json-escape-string`
2. **Integer I/O** - `int->string` and `string->int` conversions

## Medium-Term Goals

### Spawn with Captured Data

Currently `spawn` only works with empty-effect quotations. Allowing captured data would enable:
```seq
42 [ dup * write-int ] spawn  # Currently not possible
```

This requires solving ownership/copying semantics for spawned strands.

### Collections

- Lists/arrays with stack-friendly operations
- Maps/dictionaries

### Error Handling

Move from panic-based to Result-based error handling for:
- File I/O
- Channel operations
- Type conversions

## Non-Goals

- **Pattern matching syntax** - Stack destructuring via existing words is sufficient
- **Let bindings / VALUE** - Not idiomatic for concatenative languages
- **Loop syntax** - `while`, `until`, `times`, `forever` quotation combinators cover this
