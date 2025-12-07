# YAML Examples

Examples demonstrating the YAML parsing library implemented in Seq.

## Overview

The YAML library (`std:yaml`) is written entirely in Seq, using only the
existing language primitives. This validates that the builtin/stdlib balance
allows building complex parsers without language changes.

## Primitives Used

The YAML parser uses these existing primitives:
- String operations: `string-find`, `string-substring`, `string-trim`, `string-empty`, `string-length`, `string-char-at`, `string-concat`, `string->float`
- Character conversion: `char->string`
- Variant operations: `make-variant-0`, `make-variant-1`, `variant-tag`, `variant-field-at`, `variant-field-count`, `variant-append`
- Standard stack operations: `dup`, `drop`, `swap`, `over`, `rot`
- Arithmetic and comparison: `add`, `subtract`, `<`, `>`, `=`, `<>`
- Control flow: `if/else/then`

No new primitives were required.

## Examples

### yaml_test.seq
Basic tests for single-line YAML parsing:
- Strings: `name: John`
- Numbers: `age: 42`, `price: 19.99`
- Booleans: `active: true`, `enabled: false`
- Null: `data: null`, `empty: ~`

### yaml_multiline.seq
Tests for multi-line YAML documents:
- Multiple key-value pairs
- Blank lines (ignored)
- Comments (lines starting with #)

## Running

```bash
cargo run --release -- examples/yaml/yaml_test.seq -o /tmp/yaml_test
/tmp/yaml_test

cargo run --release -- examples/yaml/yaml_multiline.seq -o /tmp/yaml_multi
/tmp/yaml_multi
```

## Supported YAML Features

- Multi-line documents with multiple key-value pairs
- String values (unquoted)
- Integer and floating-point numbers
- Booleans (true/false)
- Null values (null or ~)
- Comments (# to end of line)
- Blank lines

## Not Yet Supported

- Nested objects (indentation-based nesting)
- Arrays/lists (- item syntax)
- Multi-line strings (| and > block scalars)
- Quoted strings with escapes
- Anchors and aliases
