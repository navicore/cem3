# String Primitives Plan

## Goal
Add minimal runtime primitives to enable writing text parsers (CSV, JSON, etc.) in pure Seq.

## Philosophy
- Add **primitives** to the runtime (Rust)
- Build **parsers** in the stdlib (Seq)
- Keep the runtime minimal, let the language do the work
- **Commit to proper UTF-8 support** - characters, not bytes

## Decision: UTF-8 Character Semantics

All string operations use **code point** (character) semantics, not byte semantics.

```seq
"héllo" string-length     # → 5 (characters, not 6 bytes)
"héllo" 1 string-char-at  # → 233 (U+00E9 = é)
```

This is a **breaking change** to `string-length` (was byte-based). We fix existing
code rather than accumulate technical debt.

## Proposed Primitives

### 1. `string-char-at` ( String Int -- Int )
Get Unicode code point at character index.

```seq
"hello" 0 string-char-at  # → 104 (ASCII 'h')
"héllo" 1 string-char-at  # → 233 (U+00E9 = é)
"hello" 5 string-char-at  # → -1 (out of bounds)
```

### 2. `string-substring` ( String Int Int -- String )
Extract substring: `string start length -> result`

```seq
"hello" 1 3 string-substring  # → "ell"
"hello" 0 5 string-substring  # → "hello"
"hello" 2 10 string-substring # → "llo" (clamp to end)
```

**Edge cases:**
- Start beyond end: return empty string
- Length extends past end: clamp to available

### 3. `char->string` ( Int -- String )
Convert code point to single-character string.

```seq
65 char->string   # → "A"
104 char->string  # → "h"
10 char->string   # → "\n" (newline)
```

**Note:** Enables building strings character by character.

### 4. `string-find` ( String String -- Int )
Find first occurrence of substring, returns -1 if not found.

```seq
"hello world" "world" string-find  # → 6
"hello world" "xyz" string-find    # → -1
"hello" "l" string-find            # → 2 (first match)
```

**Use case:** Locating delimiters, checking for substrings.

## Implementation Checklist

For each primitive:

### Runtime (`runtime/src/string_ops.rs`)
```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_string_char_at(stack: Stack) -> Stack {
    // Pop index (Int), pop string (String)
    // Get character at index
    // Push result (Int) - code point or -1
}
```

### Runtime exports (`runtime/src/lib.rs`)
```rust
pub use string_ops::patch_seq_string_char_at;
```

### Type signatures (`compiler/src/builtins.rs`)
```rust
// string-char-at: ( ..a String Int -- ..a Int )
sigs.insert(
    "string-char-at".to_string(),
    Effect::new(
        StackType::RowVar("a".to_string())
            .push(Type::String)
            .push(Type::Int),
        StackType::RowVar("a".to_string()).push(Type::Int),
    ),
);
```

### Codegen (`compiler/src/codegen.rs`)
1. Add declaration:
   ```rust
   writeln!(&mut ir, "declare ptr @patch_seq_string_char_at(ptr)").unwrap();
   ```
2. Add to name mapping:
   ```rust
   "string-char-at" => "patch_seq_string_char_at".to_string(),
   ```

## What This Enables

### In stdlib (pure Seq):

**CSV Parser** (`stdlib/csv.seq`):
```seq
: csv-parse-field ( String Int -- String Int )
  # Parse one field starting at index
  # Returns field value and next index
  ...
;
```

**JSON Tokenizer** (`stdlib/json.seq`):
```seq
: json-skip-whitespace ( String Int -- Int )
  # Skip spaces, tabs, newlines
  ...
;

: json-parse-string ( String Int -- String Int )
  # Parse "..." string literal
  ...
;
```

**Number Parser** (needed for JSON):
```seq
: string->int ( String -- Int )
  # Parse integer from string
  # "123" -> 123
  ...
;
```

## Implementation Order

1. **Phase 1: Core primitives** (this PR)
   - `string-char-at`
   - `string-substring`
   - `char->string`
   - `string-find`

2. **Phase 2: Basic parsing stdlib**
   - `string->int` (in Seq using primitives)
   - `std:string` - additional string utilities

3. **Phase 3: CSV support**
   - `csv-parse-line`
   - `csv-parse-field`

4. **Phase 4: JSON support**
   - JSON tokenizer
   - JSON parser (returns Variants)

## Testing Strategy

Each runtime function needs:
1. Unit tests in Rust (`string_ops.rs`)
2. Integration test compiling Seq program
3. Edge case coverage (empty strings, bounds, UTF-8)

## Estimated Effort

- Phase 1 (primitives): 1 session
- Phase 2 (string stdlib): 1 session
- Phase 3 (CSV): 1 session
- Phase 4 (JSON): 2-3 sessions (recursive parsing is tricky)

## Resolved Decisions

1. **UTF-8 handling**: ✅ Use Unicode code points (characters), not bytes
   - `string-length` returns character count
   - `string-char-at` returns code point value
   - Add `string-byte-length` for when bytes needed (HTTP Content-Length)

2. **Error handling**: Return -1 for out of bounds (allows safe iteration patterns)

3. **Breaking change**: Fix `string-length` now while language is young

## Additional Primitive Needed

### `string-byte-length` ( String -- Int )
Get byte count (needed for HTTP Content-Length, buffer allocation).

```seq
"hello" string-byte-length  # → 5
"héllo" string-byte-length  # → 6 (é is 2 bytes in UTF-8)
```
