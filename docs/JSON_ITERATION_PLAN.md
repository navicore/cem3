# JSON Implementation Iteration Plan

## Current Status (Working)

The following JSON features work correctly:

- **Primitives**: `null`, `true`, `false` parse and serialize
- **Strings**: `"hello"` parses and serializes (basic - no escape sequences yet)
- **Numbers**: `42`, `3.14`, `-5`, `1e10` parse and serialize (standalone only)
- **Empty containers**: `[]` and `{}` parse and serialize
- **Whitespace**: Handled correctly in all positions

## Current Limitations

### 1. Numbers Inside Arrays/Objects Don't Work

**Problem**: `json-parse-number` consumes the entire remaining string instead of stopping at the number boundary.

**Example**:
```
"[1]" → parses as: [ then tries to parse "1]" as number → fails
"[true]" → parses as: [ then parses "true" → works (keyword parser stops correctly)
```

**Root Cause**: The number parser was originally designed for standalone values where `string->float` could consume the whole string. Inside arrays, we need to find where the number ends (first non-digit/non-number-char) and only parse that substring.

### 2. Multi-Element Arrays Not Implemented

**Problem**: After parsing first element, we need to check for `,` or `]`, then potentially parse more elements. This requires complex stack management to track the count of elements while preserving `str` and `pos`.

### 3. Objects with Key-Value Pairs Not Implemented

**Problem**: Similar to arrays, but also need to parse string keys and `:` separator.

### 4. String Escape Sequences Not Implemented

**Problem**: `\"`, `\\`, `\n`, `\t`, `\uXXXX` are not handled.

## The Core Technical Challenge

### Stack Depth Management

Seq's stack primitives (`rot`, `swap`, `over`, `pick`) work on at most 3-4 items. Complex operations like number parsing inside arrays require juggling 5+ values:

- `str` (original string - needed for return)
- `startpos` (where number starts)
- `endpos` (where number ends)
- `len` (endpos - startpos for substring)
- `numstr` (extracted number string)
- `float` (parsed value)
- `jsonval` (wrapped as JSON number)

Moving values around at this depth is extremely difficult with standard Forth-style operations.

## Proposed Solutions

### Option A: Add `roll` Builtin (Recommended)

Add a `roll` operation to the runtime that can rotate items at arbitrary depth:

```seq
# n roll: rotate n items, bringing nth item to top
# Example: a b c d 3 roll → b c d a
```

**Implementation**:
1. Add to `runtime/src/stack.rs`
2. Add signature to `compiler/src/builtins.rs`
3. Add codegen in `compiler/src/codegen.rs`
4. Add to builtin list in `compiler/src/ast.rs`

**Pros**: Clean, reusable, matches standard Forth
**Cons**: Requires runtime changes

### Option B: Parser State Variant

Pack parser state into a variant instead of keeping `str` and `pos` as separate stack items:

```seq
# Create parser state: ( String Int -- ParserState )
: make-parser-state  2 100 make-variant ;  # tag 100 for parser state

# Unpack: ( ParserState -- String Int )
: unpack-parser-state
  dup 0 variant-field-at  # str
  swap 1 variant-field-at  # pos
;
```

**Pros**: Reduces stack depth by 1, no runtime changes
**Cons**: Extra allocation, slightly slower

### Option C: Specialized Number Extraction Builtin

Add a builtin specifically for extracting a number substring:

```seq
# extract-json-number: ( String Int -- String Int String )
# Input: full string, start position
# Output: full string, end position, number substring
```

**Pros**: Solves the immediate problem efficiently
**Cons**: Very specialized, doesn't help with other stack challenges

## Recommended Iteration Path

### Phase 1: Fix Number Parsing in Arrays

**Goal**: Make `[1]`, `[3.14]`, `[-5]` work

**Approach**: Implement Option A (add `roll` builtin) or Option B (parser state variant)

**Test cases**:
```seq
"[1]" json-parse drop json-serialize  # Should output: [1]
"[42]" json-parse drop json-serialize # Should output: [42]
"[-3.14]" json-parse drop json-serialize # Should output: [-3.14]
```

### Phase 2: Multi-Element Arrays

**Goal**: Make `[1, 2, 3]` work

**Approach**:
1. After parsing first element, check for `,` or `]`
2. If `,`, recursively parse more elements
3. Track count, then build variant with `count 4 make-variant`

**Challenge**: Elements accumulate on stack below `str`/`pos`. Need to move them above for `make-variant`.

**Test cases**:
```seq
"[1, 2]" json-parse drop json-serialize
"[true, false, null]" json-parse drop json-serialize
"[[1], [2]]" json-parse drop json-serialize  # Nested arrays
```

### Phase 3: Array Serialization with Elements

**Goal**: Serialize arrays with actual elements, not just `[]`

**Approach**: Loop through variant fields, serialize each, join with `,`

### Phase 4: Objects

**Goal**: Make `{"key": "value"}` work

**Approach**: Similar to arrays but parse key-value pairs

### Phase 5: String Escapes

**Goal**: Handle `\"`, `\\`, `\n`, etc.

## Lessons Learned

### Stack Tracing is Critical

When writing complex stack manipulations, trace each operation manually:

```
# Example trace for rot:
# (a b c) rot → (b c a)  -- brings 3rd item to top
```

### The `rot rot` Pattern

To move the top item below the next two:
```seq
# (a b c) → (c a b)
rot rot  # First rot: (b c a), Second rot: (c a b)
```

### Be Careful with `swap` after `rot`

A common mistake:
```seq
# Want: (a b c) → (b c a)
# Wrong: rot swap  -- this gives (b a c), not (b c a)
# Right: just rot
```

### Type Checker and Nested Ifs

The Seq type checker requires all branches of an `if` to have the same stack effect. When you have nested `if`s, trace each complete path from function entry to exit to ensure they all produce the same stack shape.

## Key Code Locations

- **Main JSON library**: `stdlib/json.seq`
- **Number parsing**: `json-parse-number` function (line ~472)
- **Array parsing**: `json-parse-array-contents` function (line ~533)
- **Serialization**: `json-serialize` and `json-serialize-array` functions

## Testing Strategy

Create incremental test files:

```bash
# Test standalone values (should all pass now)
./target/release/seqc --output /tmp/test /tmp/test_primitives.seq && /tmp/test

# Test arrays with non-number elements (should pass now)
./target/release/seqc --output /tmp/test /tmp/test_arrays_bool.seq && /tmp/test

# Test arrays with numbers (will fail until Phase 1 complete)
./target/release/seqc --output /tmp/test /tmp/test_arrays_num.seq && /tmp/test
```

## Notes on Seq Stack Operations

For reference, here's what the standard operations do:

| Operation | Stack Effect | Description |
|-----------|-------------|-------------|
| `dup` | `( a -- a a )` | Duplicate top |
| `drop` | `( a -- )` | Remove top |
| `swap` | `( a b -- b a )` | Swap top two |
| `over` | `( a b -- a b a )` | Copy second to top |
| `rot` | `( a b c -- b c a )` | Rotate top three |
| `nip` | `( a b -- b )` | Remove second |
| `tuck` | `( a b -- b a b )` | Copy top below second |
| `pick` | `( ... n -- ... x )` | Copy nth item to top (0-indexed) |

The `pick` operation helps but can only copy, not move or remove items at depth.
