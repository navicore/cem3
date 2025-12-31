# Migration Guide: Pre-0.9 to 0.18.x

This guide covers all breaking changes from early Seq versions (0.8.x and earlier) to the current stable release (0.18.x). If you're migrating from any pre-0.18 version, this single document has everything you need.

## Summary of Breaking Changes

| Version | Change | Impact |
|---------|--------|--------|
| 0.9.0 | Built-in operations namespaced | `write-line` → `io.write-line` |
| 0.15.0 | Arithmetic/comparison namespaced | `add` → `i.add`, `=` → `i.=` |
| 0.16.0 | Bool type replaces Int for flags | `1`/`0` → `true`/`false` |
| 0.18.0 | Concurrency renamed | `spawn` → `strand.spawn` |

## Quick Reference: What Changed

### Arithmetic Operations

| Old | New (verbose) | New (terse) |
|-----|---------------|-------------|
| `add` | `i.add` | `i.+` |
| `subtract` | `i.subtract` | `i.-` |
| `multiply` | `i.multiply` | `i.*` |
| `divide` | `i.divide` | `i./` |
| (new) | `i.modulo` | `i.%` |

### Comparison Operations

| Old | New (symbol) | New (verbose) |
|-----|--------------|---------------|
| `=` | `i.=` | `i.eq` |
| `<` | `i.<` | `i.lt` |
| `>` | `i.>` | `i.gt` |
| `<=` | `i.<=` | `i.lte` |
| `>=` | `i.>=` | `i.gte` |
| `<>` | `i.<>` | `i.neq` |

### Float Operations

| Old | New (verbose) | New (terse) |
|-----|---------------|-------------|
| `f-add` | `f.add` | `f.+` |
| `f-subtract` | `f.subtract` | `f.-` |
| `f-multiply` | `f.multiply` | `f.*` |
| `f-divide` | `f.divide` | `f./` |
| `f-=` | `f.=` | `f.eq` |
| `f-<` | `f.<` | `f.lt` |
| `f->` | `f.>` | `f.gt` |
| `f-<=` | `f.<=` | `f.lte` |
| `f->=` | `f.>=` | `f.gte` |
| `f-<>` | `f.<>` | `f.neq` |

### Concurrency Operations

| Old | New |
|-----|-----|
| `spawn` | `strand.spawn` |
| (new) | `strand.weave` |
| (new) | `strand.resume` |
| (new) | `strand.weave-cancel` |

### Bool Type (Replaces Int for Boolean Values)

Seq now has a proper `Bool` type with `true` and `false` literals. Previously, integers `1` and `0` were used for boolean values.

**Stack effect changes:**

| Operation | Old Signature | New Signature |
|-----------|---------------|---------------|
| `i.=`, `i.<`, `i.>`, etc. | `( Int Int -- Int )` | `( Int Int -- Bool )` |
| `f.=`, `f.<`, `f.>`, etc. | `( Float Float -- Int )` | `( Float Float -- Bool )` |
| `and`, `or` | `( Int Int -- Int )` | `( Bool Bool -- Bool )` |
| `not` | `( Int -- Int )` | `( Bool -- Bool )` |
| `string.empty?` | `( String -- Int )` | `( String -- Bool )` |
| `string.contains` | `( String String -- Int )` | `( String String -- Bool )` |
| `string.starts-with` | `( String String -- Int )` | `( String String -- Bool )` |
| `list.empty?` | `( List -- Int )` | `( List -- Bool )` |
| `map.has?` | `( Map String -- Int )` | `( Map String -- Bool )` |
| `map.empty?` | `( Map -- Int )` | `( Map -- Bool )` |
| `file.exists?` | `( String -- Int )` | `( String -- Bool )` |
| `os.path-exists` | `( String -- Int )` | `( String -- Bool )` |
| `os.path-is-file` | `( String -- Int )` | `( String -- Bool )` |
| `os.path-is-dir` | `( String -- Int )` | `( String -- Bool )` |
| `chan.send-safe` | `( a Chan -- Int )` | `( a Chan -- Bool )` |
| `chan.receive-safe` | `( Chan -- a Int )` | `( Chan -- a Bool )` |
| `file.slurp-safe` | `( String -- String Int )` | `( String -- String Bool )` |

**Code changes required:**

```seq
# Before: Using 1/0 as booleans
: is-positive ( Int -- Int )
  0 i.> ;

: main ( -- Int )
  5 is-positive if "yes" then  # Worked because if accepted Int
  0 ;

# After: Using true/false
: is-positive ( Int -- Bool )
  0 i.> ;

: main ( -- Int )
  5 is-positive if "yes" then  # if now expects Bool
  0 ;
```

**Conditional expressions:**

```seq
# Before
1 if "true branch" then    # 1 = true
0 if "never" then          # 0 = false

# After
true if "true branch" then
false if "never" then
```

**Boolean operations:**

```seq
# Before
1 1 and  # Returns 1
1 0 or   # Returns 1
1 not    # Returns 0

# After
true true and   # Returns true
true false or   # Returns true
true not        # Returns false
```

**Checking safe operation results:**

```seq
# Before
chan.receive-safe
1 i.= if
  # success - value is on stack
then

# After
chan.receive-safe
if
  # success - value is on stack
then
```

### I/O Operations

| Old | New |
|-----|-----|
| `write-line` | `io.write-line` |
| `read-line` | `io.read-line` |
| `read-line+` | `io.read-line+` |

### Channel Operations

| Old | New |
|-----|-----|
| `make-channel` | `chan.make` |
| `send` | `chan.send` |
| `send-safe` | `chan.send-safe` |
| `receive` | `chan.receive` |
| `receive-safe` | `chan.receive-safe` |
| `close-channel` | `chan.close` |
| `yield` | `chan.yield` |

### Command-Line Arguments

| Old | New |
|-----|-----|
| `arg-count` | `args.count` |
| `arg-at` | `args.at` |

### TCP Operations

| Old | New |
|-----|-----|
| `tcp-listen` | `tcp.listen` |
| `tcp-accept` | `tcp.accept` |
| `tcp-read` | `tcp.read` |
| `tcp-write` | `tcp.write` |
| `tcp-close` | `tcp.close` |

### OS Operations

| Old | New |
|-----|-----|
| `getenv` | `os.getenv` |
| `home-dir` | `os.home-dir` |
| `current-dir` | `os.current-dir` |
| `path-exists` | `os.path-exists` |
| `path-is-file` | `os.path-is-file` |
| `path-is-dir` | `os.path-is-dir` |
| `path-join` | `os.path-join` |
| `path-parent` | `os.path-parent` |
| `path-filename` | `os.path-filename` |

### File Operations

| Old | New |
|-----|-----|
| `file-slurp` | `file.slurp` |
| `file-slurp-safe` | `file.slurp-safe` |
| `file-exists?` | `file.exists?` |
| `file-for-each-line+` | `file.for-each-line+` |

### String Operations

| Old | New |
|-----|-----|
| `string-concat` | `string.concat` |
| `string-length` | `string.length` |
| `string-byte-length` | `string.byte-length` |
| `string-char-at` | `string.char-at` |
| `string-substring` | `string.substring` |
| `string-find` | `string.find` |
| `string-split` | `string.split` |
| `string-contains` | `string.contains` |
| `string-starts-with` | `string.starts-with` |
| `string-empty?` | `string.empty?` |
| `string-trim` | `string.trim` |
| `string-chomp` | `string.chomp` |
| `string-to-upper` | `string.to-upper` |
| `string-to-lower` | `string.to-lower` |
| `string-equal?` | `string.equal?` |
| `json-escape` | `string.json-escape` |

### Type Conversions

| Old | New |
|-----|-----|
| `int-to-string` | `int->string` |
| `string-to-int` | `string->int` |
| `int-to-float` | `int->float` |
| `float-to-int` | `float->int` |
| `float-to-string` | `float->string` |
| `string-to-float` | `string->float` |
| `char-to-string` | `char->string` |

### List Operations

| Old | New |
|-----|-----|
| `list-map` | `list.map` |
| `list-filter` | `list.filter` |
| `list-fold` | `list.fold` |
| `list-each` | `list.each` |
| `list-length` | `list.length` |
| `list-empty?` | `list.empty?` |

### Map Operations

| Old | New |
|-----|-----|
| `make-map` | `map.make` |
| `map-get` | `map.get` |
| `map-get-safe` | `map.get-safe` |
| `map-set` | `map.set` |
| `map-has?` | `map.has?` |
| `map-remove` | `map.remove` |
| `map-keys` | `map.keys` |
| `map-values` | `map.values` |
| `map-size` | `map.size` |
| `map-empty?` | `map.empty?` |

### Variant Operations

| Old | New |
|-----|-----|
| `variant-field-count` | `variant.field-count` |
| `variant-tag` | `variant.tag` |
| `variant-field-at` | `variant.field-at` |
| `variant-append` | `variant.append` |
| `variant-last` | `variant.last` |
| `variant-init` | `variant.init` |
| `make-variant-0` | `variant.make-0` |
| `make-variant-1` | `variant.make-1` |
| `make-variant-2` | `variant.make-2` |
| `make-variant-3` | `variant.make-3` |
| `make-variant-4` | `variant.make-4` |

## Unchanged Operations

These operations keep their original names:

**Stack:** `dup`, `swap`, `over`, `rot`, `nip`, `tuck`, `drop`, `pick`, `roll`, `2dup`, `3drop`

**Boolean:** `and`, `or`, `not`

**Bitwise:** `band`, `bor`, `bxor`, `bnot`, `shl`, `shr`, `popcount`, `clz`, `ctz`, `int-bits`

**Control Flow:** `call`, `times`, `while`, `until`, `cond`, `if`, `else`, `then`, `match`, `end`, `yield`

## Automated Migration

### Full Migration Script (macOS/BSD)

This script migrates from any pre-0.9 version to current 0.18.x:

```bash
#!/bin/bash
# migrate-to-0.18.sh - Full Seq migration script
# Usage: ./migrate-to-0.18.sh [directory]

DIR="${1:-.}"

find "$DIR" -name "*.seq" -exec sed -i '' \
  -e 's/\badd\b/i.add/g' \
  -e 's/\bsubtract\b/i.subtract/g' \
  -e 's/\bmultiply\b/i.multiply/g' \
  -e 's/\bdivide\b/i.divide/g' \
  -e 's/\bspawn\b/strand.spawn/g' \
  -e 's/\bwrite-line\b/io.write-line/g' \
  -e 's/\bread-line+\b/io.read-line+/g' \
  -e 's/\bread-line\b/io.read-line/g' \
  -e 's/\bmake-channel\b/chan.make/g' \
  -e 's/\bsend-safe\b/chan.send-safe/g' \
  -e 's/\breceive-safe\b/chan.receive-safe/g' \
  -e 's/\bclose-channel\b/chan.close/g' \
  -e 's/\bchan\.yield\b/chan.yield/g' \
  -e 's/\bsend\b/chan.send/g' \
  -e 's/\breceive\b/chan.receive/g' \
  -e 's/\btcp-listen\b/tcp.listen/g' \
  -e 's/\btcp-accept\b/tcp.accept/g' \
  -e 's/\btcp-read\b/tcp.read/g' \
  -e 's/\btcp-write\b/tcp.write/g' \
  -e 's/\btcp-close\b/tcp.close/g' \
  -e 's/\bgetenv\b/os.getenv/g' \
  -e 's/\bhome-dir\b/os.home-dir/g' \
  -e 's/\bcurrent-dir\b/os.current-dir/g' \
  -e 's/\bpath-exists\b/os.path-exists/g' \
  -e 's/\bpath-is-file\b/os.path-is-file/g' \
  -e 's/\bpath-is-dir\b/os.path-is-dir/g' \
  -e 's/\bpath-join\b/os.path-join/g' \
  -e 's/\bpath-parent\b/os.path-parent/g' \
  -e 's/\bpath-filename\b/os.path-filename/g' \
  -e 's/\bfile-slurp-safe\b/file.slurp-safe/g' \
  -e 's/\bfile-slurp\b/file.slurp/g' \
  -e 's/\bfile-exists?\b/file.exists?/g' \
  -e 's/\bfile-for-each-line+\b/file.for-each-line+/g' \
  -e 's/\bstring-concat\b/string.concat/g' \
  -e 's/\bstring-length\b/string.length/g' \
  -e 's/\bstring-byte-length\b/string.byte-length/g' \
  -e 's/\bstring-char-at\b/string.char-at/g' \
  -e 's/\bstring-substring\b/string.substring/g' \
  -e 's/\bstring-find\b/string.find/g' \
  -e 's/\bstring-split\b/string.split/g' \
  -e 's/\bstring-contains\b/string.contains/g' \
  -e 's/\bstring-starts-with\b/string.starts-with/g' \
  -e 's/\bstring-empty?\b/string.empty?/g' \
  -e 's/\bstring-trim\b/string.trim/g' \
  -e 's/\bstring-chomp\b/string.chomp/g' \
  -e 's/\bstring-to-upper\b/string.to-upper/g' \
  -e 's/\bstring-to-lower\b/string.to-lower/g' \
  -e 's/\bstring-equal?\b/string.equal?/g' \
  -e 's/\bjson-escape\b/string.json-escape/g' \
  -e 's/\bint-to-string\b/int->string/g' \
  -e 's/\bstring-to-int\b/string->int/g' \
  -e 's/\bint-to-float\b/int->float/g' \
  -e 's/\bfloat-to-int\b/float->int/g' \
  -e 's/\bfloat-to-string\b/float->string/g' \
  -e 's/\bstring-to-float\b/string->float/g' \
  -e 's/\bchar-to-string\b/char->string/g' \
  -e 's/\blist-map\b/list.map/g' \
  -e 's/\blist-filter\b/list.filter/g' \
  -e 's/\blist-fold\b/list.fold/g' \
  -e 's/\blist-each\b/list.each/g' \
  -e 's/\blist-length\b/list.length/g' \
  -e 's/\blist-empty?\b/list.empty?/g' \
  -e 's/\bmake-map\b/map.make/g' \
  -e 's/\bmap-get-safe\b/map.get-safe/g' \
  -e 's/\bmap-get\b/map.get/g' \
  -e 's/\bmap-set\b/map.set/g' \
  -e 's/\bmap-has?\b/map.has?/g' \
  -e 's/\bmap-remove\b/map.remove/g' \
  -e 's/\bmap-keys\b/map.keys/g' \
  -e 's/\bmap-values\b/map.values/g' \
  -e 's/\bmap-size\b/map.size/g' \
  -e 's/\bmap-empty?\b/map.empty?/g' \
  -e 's/\bvariant-field-count\b/variant.field-count/g' \
  -e 's/\bvariant-tag\b/variant.tag/g' \
  -e 's/\bvariant-field-at\b/variant.field-at/g' \
  -e 's/\bvariant-append\b/variant.append/g' \
  -e 's/\bvariant-last\b/variant.last/g' \
  -e 's/\bvariant-init\b/variant.init/g' \
  -e 's/\bmake-variant-0\b/variant.make-0/g' \
  -e 's/\bmake-variant-1\b/variant.make-1/g' \
  -e 's/\bmake-variant-2\b/variant.make-2/g' \
  -e 's/\bmake-variant-3\b/variant.make-3/g' \
  -e 's/\bmake-variant-4\b/variant.make-4/g' \
  -e 's/\bf-add\b/f.add/g' \
  -e 's/\bf-subtract\b/f.subtract/g' \
  -e 's/\bf-multiply\b/f.multiply/g' \
  -e 's/\bf-divide\b/f.divide/g' \
  -e 's/\bf-=\b/f.=/g' \
  -e 's/\bf-<\b/f.</g' \
  -e 's/\bf->\b/f.>/g' \
  -e 's/\bf-<=\b/f.<=/g' \
  -e 's/\bf->=\b/f.>=/g' \
  -e 's/\bf-<>\b/f.<>/g' \
  -e 's/\barg-count\b/args.count/g' \
  -e 's/\barg-at\b/args.at/g' \
  {} \;

echo "Migration complete. Review changes with: git diff"
```

### Manual Steps Required

#### 1. Comparison Operators

The bare comparison operators (`=`, `<`, `>`, `<=`, `>=`, `<>`) cannot be reliably migrated with sed because they're single characters that appear in many contexts (strings, comments, stack effects).

**You must manually update these:**

```seq
# Before
5 3 = if "equal" then
x 0 > if "positive" then
a b <= if "a not greater" then

# After
5 3 i.= if "equal" then
x 0 i.> if "positive" then
a b i.<= if "a not greater" then
```

**Tip:** Search for these patterns in your code:
```bash
grep -n ' = \| < \| > \| <= \| >= \| <> ' *.seq
```

#### 2. Bool Type Changes

Stack effect signatures that used `Int` for boolean results must be updated to `Bool`. The compiler will report type errors.

**Update your word signatures:**

```seq
# Before
: is-valid ( Int -- Int )
  0 i.> ;

# After
: is-valid ( Int -- Bool )
  0 i.> ;
```

**Update literal boolean values:**

```seq
# Before
1 if "yes" then    # Using 1 as true
0                  # Using 0 as false

# After
true if "yes" then
false
```

**Update safe operation checks:**

```seq
# Before
file.slurp-safe
swap drop           # Get the success flag
1 i.= if            # Check if 1
  # handle content
then

# After
file.slurp-safe
swap drop           # Get the success flag (now Bool)
if                  # if expects Bool directly
  # handle content
then
```

**Tip:** Search for patterns that may need Bool updates:
```bash
grep -n '1 i\.= if\|0 i\.= if\|-- Int )' *.seq
```

### Linux/GNU sed

For Linux, remove the `''` after `-i`:

```bash
find "$DIR" -name "*.seq" -exec sed -i \
  -e 's/\badd\b/i.add/g' \
  # ... rest of patterns ...
  {} \;
```

## Example: Before and After

### Before (pre-0.9)

```seq
: factorial ( Int -- Int )
  dup 1 <= if
    drop 1
  else
    dup 1 subtract factorial multiply
  then
;

: main ( -- Int )
  "Enter a number: " write-line
  read-line string-trim string-to-int
  dup 0 < if
    drop "Must be non-negative" write-line 1
  else
    factorial dup int-to-string write-line 0
  then
;
```

### After (0.18.x)

```seq
: factorial ( Int -- Int )
  dup 1 i.<= if
    drop 1
  else
    dup 1 i.- factorial i.*
  then
;

: main ( -- Int )
  "Enter a number: " io.write-line
  io.read-line string.trim string->int
  dup 0 i.< if
    drop "Must be non-negative" io.write-line 1
  else
    factorial dup int->string io.write-line 0
  then
;
```

### Concurrency Example

#### Before (pre-0.18)

```seq
: worker ( Chan -- )
  "Working..." write-line
  42 swap send
;

: main ( -- Int )
  make-channel
  dup [ worker ] spawn drop
  receive
  int-to-string write-line
  0
;
```

#### After (0.18.x)

```seq
: worker ( Chan -- )
  "Working..." io.write-line
  42 swap chan.send
;

: main ( -- Int )
  chan.make
  dup [ worker ] strand.spawn drop
  chan.receive
  int->string io.write-line
  0
;
```

## Naming Convention Rationale

The new naming uses three delimiters:

| Delimiter | Usage | Example |
|-----------|-------|---------|
| `.` (dot) | Module/namespace prefix | `io.write-line`, `i.add` |
| `-` (hyphen) | Compound words within names | `home-dir`, `field-at` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |

**Why namespace arithmetic?**

1. **Type clarity** - `i.+` is clearly integer addition, `f.+` is float addition
2. **Extensibility** - Room for future numeric types without collision
3. **Consistency** - All operations follow the same `module.operation` pattern

**Why keep stack operations unnamespaced?**

Stack operations (`dup`, `swap`, `drop`, etc.) are fundamental primitives used constantly. Namespacing them would add noise without benefit.

## Troubleshooting

### "Unknown word" errors

The compiler reports unknown words. Check this guide for the new name.

### Comparison operators in strings

If your strings contain `=`, `<`, `>` characters, the migration script won't affect them (they're inside quotes). Only bare operators in code need updating.

### Mixed old/new code

The compiler won't accept mixed naming. All code must use the new names.

## Getting Help

- Report issues: https://github.com/navicore/patch-seq/issues
- The compiler suggests corrections for misspelled words
