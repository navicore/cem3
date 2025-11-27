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

**Root Cause**: The number parser uses `string->float` which consumes the whole remaining string. Inside arrays, we need to find where the number ends (first non-digit/non-number-char) and only parse that substring.

### 2. Multi-Element Arrays Not Implemented

**Problem**: After parsing first element, we need to check for `,` or `]`, then potentially parse more elements. This requires complex stack management to track the count of elements while preserving `str` and `pos`.

### 3. Objects with Key-Value Pairs Not Implemented

**Problem**: Similar to arrays, but also need to parse string keys and `:` separator.

### 4. String Escape Sequences Not Implemented

**Problem**: `\"`, `\\`, `\n`, `\t`, `\uXXXX` are not handled.

---

## Recommended Next Step: Pure Seq Approach

**Decision**: Rather than adding runtime builtins, solve this in pure Seq using variants to manage parser state. This keeps the JSON library self-contained and demonstrates Seq's capabilities.

### Why This Approach?

1. **No runtime changes needed** - JSON stays in stdlib only
2. **Proves the language** - Shows Seq can handle real parsing tasks
3. **Cleaner architecture** - JSON doesn't need special compiler support
4. **Educational value** - Good example of using variants for state management

---

## Implementation Plan

### Phase 1: Fix Number Boundary Detection (NEXT)

**Goal**: Make `[1]`, `[42]`, `[-3.14]` parse correctly.

**The Problem**: `json-parse-number` currently uses:
```seq
: json-parse-number ( str pos -- str pos JsonValue Int )
  drop  # drop pos
  dup string->float  # parse ENTIRE string as float
  ...
```

This fails for `"1]"` because `string->float` tries to parse `"1]"` as a number.

**The Solution**: Scan forward to find number boundary, extract substring, then parse.

```seq
# Find where number ends (first char that's not digit/dot/e/E/+/-)
: json-find-number-end ( str pos -- str endpos )
  json-at-end? if
    # Already at end
  else
    json-char-at json-is-number-char? if
      1 json-advance json-find-number-end  # recurse
    else
      # Found boundary
    then
  then
;

# New number parser with boundary detection
: json-parse-number-bounded ( str pos -- str newpos JsonValue Int )
  # Stack: str pos
  over over              # str pos str pos
  json-find-number-end   # str pos str endpos
  # Now extract substring from pos to endpos
  over subtract          # str pos str len
  rot rot                # str str len pos
  swap string-substring  # str numstr
  string->float          # str float success
  0 = if
    drop 0 json-null 0   # Parse failed
  else
    json-number          # str JsonNumber
    # Need to get endpos back... this is the challenge
    ...
  then
;
```

**The Stack Challenge**: After parsing, we need both:
- The original `str` (for further parsing)
- The new position (`endpos`)
- The parsed `JsonValue`
- Success flag

With 4 values and complex intermediate steps, stack management is tricky.

**Solution: Use Parser State Variant**

Pack `(str, pos)` into a single variant to reduce stack depth:

```seq
# Parser state: tag 100, fields: [str, pos]
: make-pstate ( str pos -- PState )
  2 100 make-variant
;

: pstate-str ( PState -- PState str )
  dup 0 variant-field-at
;

: pstate-pos ( PState -- PState pos )
  dup 1 variant-field-at
;

: pstate-advance ( PState n -- PState )
  swap pstate-pos rot add    # str newpos
  swap pstate-str swap       # str str newpos (wrong order, fix below)
  # Actually: unpack, advance, repack
  swap dup 1 variant-field-at  # PState pos
  rot add                       # PState newpos
  swap 0 variant-field-at       # newpos str
  swap make-pstate              # PState'
;
```

This keeps parser state as ONE stack item instead of TWO, making all operations easier.

### Phase 2: Single-Element Arrays

**Goal**: Make `[1]`, `[true]`, `["hello"]` work.

After Phase 1, `json-parse-value` can parse a number and return the correct position. Now:

```seq
: json-parse-array-contents ( PState -- PState JsonArray Int )
  # Parse first element
  json-parse-value        # PState JsonValue success
  0 = if
    drop json-empty-array 0
  else
    # Stack: PState JsonValue
    # Skip whitespace, check for ] or ,
    swap pstate-skip-ws
    pstate-char-at 93 = if   # ]
      # Single element array
      pstate-advance         # PState'
      swap                   # JsonValue PState'
      1 4 make-variant       # JsonArray (1 element, tag 4)
      swap 1                 # PState' JsonArray 1
    else
      # Must be comma for multi-element, or error
      drop json-empty-array 0
    then
  then
;
```

### Phase 3: Multi-Element Arrays

**Goal**: Make `[1, 2, 3]` work.

**Approach**: Recursive helper that accumulates count.

```seq
# Parse remaining elements after first
# Stack: ( PState elem1 count -- PState elem1...elemN count )
: json-parse-more-elements ( PState ... count -- PState ... count )
  # Check for comma
  over pstate-skip-ws pstate-char-at 44 = if   # comma
    # Parse next element
    swap 1 pstate-advance pstate-skip-ws
    json-parse-value
    0 = if
      drop  # failed, return what we have
    else
      # Increment count, continue
      rot 1 add
      json-parse-more-elements  # recurse
    then
  else
    # No comma, we're done (expect ])
  then
;
```

**Challenge**: Elements accumulate BELOW PState on stack. For `make-variant` we need:
```
elem1 elem2 elem3 count tag make-variant
```

Will need careful stack manipulation at the end.

### Phase 4: Array Serialization

**Goal**: `json-serialize-array` outputs actual elements, not just `[]`.

```seq
: json-serialize-array ( JsonArray -- String )
  "[" swap
  dup variant-field-count    # "[" array count
  0 = if
    drop drop "]" string-concat
  else
    # Serialize elements with commas
    json-serialize-array-elements
    "]" string-concat
  then
;

: json-serialize-array-elements ( "[" array -- "[elem,elem,..." )
  dup variant-field-count   # str array count
  0                         # str array count idx
  json-serialize-array-loop
;

: json-serialize-array-loop ( str array count idx -- str' )
  2dup >= if
    # idx >= count, done
    drop drop drop
  else
    # Get element at idx
    2 pick over variant-field-at  # str array count idx elem
    json-serialize               # str array count idx elemstr
    # Concat to result
    4 pick swap string-concat    # str array count idx newstr
    # Add comma if not last
    over 1 add 3 pick < if
      "," string-concat
    then
    # Store back and increment idx
    # ... (complex stack management)
  then
;
```

This is where `pick` at depth 4+ gets painful. May need to track state in a variant.

### Phase 5: Objects

**Goal**: `{"key": "value"}` parses and serializes.

Similar to arrays but:
1. Parse string key
2. Expect `:`
3. Parse value
4. Store as pairs in variant (fields: key1, val1, key2, val2, ...)

### Phase 6: String Escapes

**Goal**: Handle `\"`, `\\`, `\n`, `\t`.

Requires character-by-character scanning in string parser.

---

## Alternative: Add `roll` Builtin (Deferred)

If pure Seq becomes too painful, add Forth-standard `roll`:

```seq
# n roll: rotate n items, bringing nth item to top
# a b c d 3 roll → b c d a
```

**Implementation**:
1. `runtime/src/stack.rs`: Add `patch_seq_roll`
2. `compiler/src/builtins.rs`: Add signature
3. `compiler/src/codegen.rs`: Add call generation

**Deferred** because:
- Want to prove JSON is doable in pure Seq first
- Parser state variant may be sufficient
- Keeps runtime minimal

---

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

### Variants Reduce Stack Pressure

Packing related values into a variant reduces the number of stack items to juggle:
- **Before**: `str pos` = 2 items
- **After**: `PState` = 1 item

This is especially valuable for recursive parsers where state threads through many calls.

---

## Key Code Locations

| File | Purpose |
|------|---------|
| `stdlib/json.seq` | Main JSON library |
| `json-parse-number` (line ~479) | Number parsing (needs fix) |
| `json-parse-array-contents` (line ~525) | Array parsing (stub) |
| `json-serialize` | Main serializer |
| `json-serialize-array` | Array serializer (returns `[]`) |

---

## Testing Strategy

```bash
# Build compiler
cargo build --release

# Test primitives (should pass)
echo 'include std:json
: main ( -- Int )
  "null" json-parse drop json-serialize write_line
  "true" json-parse drop json-serialize write_line
  "42" json-parse drop json-serialize write_line
  0
;' > /tmp/test.seq
./target/release/seqc --output /tmp/test /tmp/test.seq && /tmp/test

# Test arrays with numbers (currently fails)
echo 'include std:json
: main ( -- Int )
  "[1]" json-parse drop json-serialize write_line
  0
;' > /tmp/test.seq
./target/release/seqc --output /tmp/test /tmp/test.seq && /tmp/test
```

---

## Stack Operations Reference

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

**Note**: `pick` can only *copy*, not *move* or *remove* items at depth.

---

## Summary: What to Do Next

1. **Implement Parser State Variant** (`make-pstate`, `pstate-str`, `pstate-pos`, `pstate-advance`)
2. **Rewrite `json-parse-number`** to find number boundary first, then parse substring
3. **Update all parser functions** to use PState instead of `str pos`
4. **Test `[1]`** - single element array with number
5. **Add multi-element support** with comma handling
6. **Implement array serialization** that outputs actual elements

The parser state variant is the key architectural change that makes everything else tractable.
