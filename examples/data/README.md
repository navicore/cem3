# Data Formats & Structures

Working with structured data in Seq.

## JSON (json/)

**json_tree.seq** - Parse and traverse JSON:

```seq
include std:json

: main ( -- Int )
  "{\"name\": \"Alice\", \"age\": 30}" json.parse
  "name" json.get json.as-string io.write-line
  0 ;
```

## YAML (yaml/)

YAML parsing with support for:
- Multiline strings
- Nested structures
- Anchors and aliases

## SON (son/)

**serialize.seq** - Seq Object Notation, Seq's native serialization format optimized for stack-based data.

## Zipper (zipper/)

**zipper-demo.seq** - Functional list navigation with O(1) cursor movement:

```seq
include std:zipper

{ 1 2 3 4 5 } list->zipper
zipper.right zipper.right  # Move to element 3
100 zipper.set             # Replace with 100
zipper.to-list             # { 1 2 100 4 5 }
```

## Encoding (encoding.seq)

Base64, hex, and other encoding/decoding operations.
