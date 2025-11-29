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

Currently closures only capture Int and String values. Need to add support for:

1. **Bool** - Same pattern as Int (i64 representation)
2. **Float** - f64 representation
3. **Quotation** - Function pointer + environment
4. **Variant** - Tagged union with fields

Each type requires:
- Runtime: `env_get_<type>` function in `closures.rs`
- CodeGen: Match arm in closure code generation

### Stdlib Improvements

1. **JSON escape handling** - Properly escape special characters in `json-escape-string`
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
