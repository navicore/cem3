# Seq Development Roadmap

## Current State

Seq is a functional concatenative language with:
- Static typing with row-polymorphic stack effects
- Green thread concurrency (strands) with channels
- Standard library (JSON, YAML, HTTP, math)
- LSP with diagnostics and completions

## Near-Term Goals

### LSP Enhancements

1. ✅ **Hover for word signatures** - Show stack effect when hovering over a word
2. ✅ **Go-to-definition** - Navigate to word definitions (local and included files)

### Closure Capture Support

Closures can capture these types:

- ✅ **Int** - `env_get_int` returns i64
- ✅ **String** - `env_get_string` returns SeqString
- ✅ **Bool** - `env_get_bool` returns i64 (0/1)
- ✅ **Float** - `env_get_float` returns f64
- ✅ **Quotation** - `env_get_quotation` returns function pointer as i64

Each type requires:
- Runtime: `env_get_<type>` function in `closures.rs`
- CodeGen: Match arm in closure code generation

### Stdlib Improvements

1. ✅ **JSON escape handling** - Properly escape special characters in `json-escape-string`
2. ✅ **Integer I/O** - `int->string` and `string->int` conversions

### Safe Error Handling

Non-panicking variants for fallible operations:

- ✅ **File I/O** - `file-slurp-safe` returns `( String -- String Int )`
- ✅ **Channel operations** - `send-safe`, `receive-safe` return success flags
- ✅ **Type conversions** - `string->int`, `string->float` already return success flags

### List Operations

Higher-order combinators for Variants used as lists:

- ✅ **list-map** - `( Variant Quotation -- Variant )` transform each element
- ✅ **list-filter** - `( Variant Quotation -- Variant )` keep elements matching predicate
- ✅ **list-fold** - `( Variant init Quotation -- result )` reduce with accumulator
- ✅ **list-each** - `( Variant Quotation -- )` apply for side effects
- ✅ **list-length** - `( Variant -- Int )` alias for variant-field-count
- ✅ **list-empty?** - `( Variant -- Int )` check if list has no elements

## Medium-Term Goals

### Collections

- Maps/dictionaries

## Long-Term Goals

### Extended Closure Capture

Additional types for closure capture (lower priority since error messages now help users work around limitations):

- ⏳ **Variant** - Compound type, more complex (tagged union with fields)
- ⏳ **Closure** - Nested closures with their own environments

## Non-Goals

- **Pattern matching syntax** - Stack destructuring via existing words is sufficient
- **Let bindings / VALUE** - Not idiomatic for concatenative languages
- **Loop syntax** - `while`, `until`, `times`, `forever` quotation combinators cover this
