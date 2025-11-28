# JSON Support Plan for Seq

## Overview

Implement JSON parsing and serialization **in Seq** - writing the parser and serializer in the Seq language itself. This exercises and validates the language's capabilities.

## Prerequisites

- ✅ Float support (merged in PR #26) - for JSON numbers
- ✅ String operations (char-at, substring, etc.)
- ✅ Variant support - for JSON value representation
- ✅ Recursion - for nested structures

## Design Decisions

### 1. JSON Value Representation (using Variants)

JSON values stored as Seq Variants with tags:

```seq
# Tag 0: JsonNull (no fields)
# Tag 1: JsonBool (one Int field: 0 or 1)
# Tag 2: JsonNumber (one Float field - JSON numbers are always float)
# Tag 3: JsonString (one String field)
# Tag 4: JsonArray (fields are array elements)
# Tag 5: JsonObject (fields alternate: key1, val1, key2, val2, ...)
```

### 2. Implementation in Seq

The parser will be written as Seq words:

```seq
: skip-whitespace ( str pos -- str newpos )
  # Skip spaces, tabs, newlines
;

: parse-string ( str pos -- str newpos JsonString )
  # Parse "..." string literal
;

: parse-number ( str pos -- str newpos JsonNumber )
  # Parse numeric literal (int or float)
;

: parse-value ( str pos -- str newpos JsonValue )
  # Dispatch based on first character
  # { -> parse-object
  # [ -> parse-array
  # " -> parse-string
  # t/f -> parse-bool
  # n -> parse-null
  # digit/- -> parse-number
;

: parse-array ( str pos -- str newpos JsonArray )
  # Parse [...] recursively calling parse-value
;

: parse-object ( str pos -- str newpos JsonObject )
  # Parse {...} with key:value pairs
;

: json-parse ( String -- JsonValue )
  # Entry point: parse entire JSON string
  0 parse-value
  # Check we consumed entire string
;
```

### 3. Required Language Features

To implement the parser, we need:

1. **String character access**: `string-char-at` ( String Int -- Int )
2. **String slicing**: `string-substring` ( String start end -- String )
3. **Character comparisons**: comparing char codes
4. **Variant construction**: creating tagged variants
5. **Recursion**: for nested structures
6. **Loops**: for arrays/objects with multiple elements

### 4. Serialization in Seq

```seq
: json-serialize ( JsonValue -- String )
  # Pattern match on variant tag
  # Recursively serialize nested values
;
```

## Implementation Phases

### Phase 1: Verify Prerequisites
- Check all needed string ops exist
- Check variant construction works
- Test recursion capability

### Phase 2: JSON Value Construction Helpers
```seq
: json-null ( -- JsonValue ) ... ;
: json-bool ( Int -- JsonValue ) ... ;
: json-number ( Float -- JsonValue ) ... ;
: json-string ( String -- JsonValue ) ... ;
```

### Phase 3: Basic Parsing
- Skip whitespace
- Parse string literals
- Parse numbers
- Parse null/true/false

### Phase 4: Recursive Parsing
- Parse arrays
- Parse objects

### Phase 5: Serialization
- Serialize each JSON type back to string

### Phase 6: Integration
- Use in HTTP server for JSON request/response

## Example Usage

```seq
: main ( -- Int )
  "{\"name\": \"Alice\", \"age\": 30}" json-parse
  # Stack: JsonObject variant

  "name" json-get json-unwrap-string
  # Stack: "Alice"

  write_line
  0
;
```

## Testing Strategy

1. Parse simple values (null, true, 42, "hello")
2. Parse arrays
3. Parse objects
4. Parse nested structures
5. Round-trip: parse then serialize, compare
6. Error cases: invalid JSON

## Notes

- No Rust dependencies for JSON
- Parser written entirely in Seq
- Validates Seq's expressiveness for real-world tasks
- Performance is secondary to correctness initially
