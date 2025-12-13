# Migration Guide: 0.8.x to 0.9.0

Version 0.9.0 introduces a consistent naming convention for all built-in operations. This is a **breaking change** that requires updating existing Seq code.

## Naming Convention

The new naming convention uses three delimiters:

| Delimiter | Usage | Example |
|-----------|-------|---------|
| `.` (dot) | Module/namespace prefix | `io.write-line`, `tcp.listen` |
| `-` (hyphen) | Compound words within names | `home-dir`, `field-at` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |

**Core primitives remain unnamespaced:** `dup`, `swap`, `drop`, `add`, `subtract`, `=`, `<`, `>`, `and`, `or`, `not`, `if`, `call`, etc.

## Quick Reference

### I/O Operations

| Old Name | New Name |
|----------|----------|
| `write-line` | `io.write-line` |
| `read-line` | `io.read-line` |
| `read-line+` | `io.read-line+` |

### Command-Line Arguments

| Old Name | New Name |
|----------|----------|
| `arg-count` | `args.count` |
| `arg-at` | `args.at` |

### Channel Operations

| Old Name | New Name |
|----------|----------|
| `make-channel` | `chan.make` |
| `send` | `chan.send` |
| `send-safe` | `chan.send-safe` |
| `receive` | `chan.receive` |
| `receive-safe` | `chan.receive-safe` |
| `close-channel` | `chan.close` |
| `yield` | `chan.yield` |

### TCP Operations

| Old Name | New Name |
|----------|----------|
| `tcp-listen` | `tcp.listen` |
| `tcp-accept` | `tcp.accept` |
| `tcp-read` | `tcp.read` |
| `tcp-write` | `tcp.write` |
| `tcp-close` | `tcp.close` |

### OS Operations

| Old Name | New Name |
|----------|----------|
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

| Old Name | New Name |
|----------|----------|
| `file-slurp` | `file.slurp` |
| `file-slurp-safe` | `file.slurp-safe` |
| `file-exists?` | `file.exists?` |
| `file-for-each-line+` | `file.for-each-line+` |

### String Operations

| Old Name | New Name |
|----------|----------|
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

| Old Name | New Name |
|----------|----------|
| `int-to-string` | `int->string` |
| `string-to-int` | `string->int` |
| `int-to-float` | `int->float` |
| `float-to-int` | `float->int` |
| `float-to-string` | `float->string` |
| `string-to-float` | `string->float` |
| `char-to-string` | `char->string` |

### List Operations

| Old Name | New Name |
|----------|----------|
| `list-map` | `list.map` |
| `list-filter` | `list.filter` |
| `list-fold` | `list.fold` |
| `list-each` | `list.each` |
| `list-length` | `list.length` |
| `list-empty?` | `list.empty?` |

### Map Operations

| Old Name | New Name |
|----------|----------|
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

| Old Name | New Name |
|----------|----------|
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

### Float Operations

| Old Name | New Name |
|----------|----------|
| `f-add` | `f.add` |
| `f-subtract` | `f.subtract` |
| `f-multiply` | `f.multiply` |
| `f-divide` | `f.divide` |
| `f-=` | `f.=` |
| `f-<` | `f.<` |
| `f->` | `f.>` |
| `f-<=` | `f.<=` |
| `f->=` | `f.>=` |
| `f-<>` | `f.<>` |

## Unchanged Operations

These operations keep their original names (no namespace prefix):

**Stack:** `dup`, `swap`, `over`, `rot`, `nip`, `tuck`, `drop`, `pick`, `roll`

**Arithmetic:** `add`, `subtract`, `multiply`, `divide`

**Comparison:** `=`, `<`, `>`, `<=`, `>=`, `<>`

**Boolean:** `and`, `or`, `not`

**Bitwise:** `band`, `bor`, `bxor`, `bnot`, `shl`, `shr`, `popcount`, `clz`, `ctz`, `int-bits`

**Control Flow:** `call`, `times`, `while`, `until`, `spawn`, `cond`, `if`, `else`, `then`, `match`, `end`

## Automated Migration

### Using sed (macOS/BSD)

```bash
# Run from your project root
find . -name "*.seq" -exec sed -i '' \
  -e 's/\bwrite-line\b/io.write-line/g' \
  -e 's/\bread-line\b/io.read-line/g' \
  -e 's/\bmake-channel\b/chan.make/g' \
  -e 's/\bsend-safe\b/chan.send-safe/g' \
  -e 's/\breceive-safe\b/chan.receive-safe/g' \
  -e 's/\bclose-channel\b/chan.close/g' \
  -e 's/\bsend\b/chan.send/g' \
  -e 's/\breceive\b/chan.receive/g' \
  -e 's/\byield\b/chan.yield/g' \
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
```

### Using sed (Linux/GNU)

```bash
# Same as above but without the '' after -i
find . -name "*.seq" -exec sed -i \
  -e 's/\bwrite-line\b/io.write-line/g' \
  # ... rest of patterns ...
  {} \;
```

## Example: Before and After

### Before (0.8.x)

```seq
: greet ( String -- )
  "Hello, " swap string-concat write-line ;

: main ( -- Int )
  "Enter your name: " write-line
  read-line string-trim
  dup string-empty? if
    drop "World"
  then
  greet
  0 ;
```

### After (0.9.0)

```seq
: greet ( String -- )
  "Hello, " swap string.concat io.write-line ;

: main ( -- Int )
  "Enter your name: " io.write-line
  io.read-line string.trim
  dup string.empty? if
    drop "World"
  then
  greet
  0 ;
```

## Rationale

The new naming convention provides:

1. **Clear namespacing** - Operations are grouped by module (`io.`, `tcp.`, `os.`, etc.)
2. **Consistency** - All compound names use the same delimiter patterns
3. **Discoverability** - Related operations share a common prefix
4. **Reduced collisions** - Module prefixes prevent name conflicts with user-defined words

Core stack operations and arithmetic remain unnamespaced because:
- They are fundamental primitives used in nearly every program
- They have well-established names in concatenative programming
- Namespacing them would add unnecessary verbosity

## Getting Help

If you encounter issues during migration:

1. The compiler will report undefined words with suggestions for misspelled names
2. Check this guide for the correct new name
3. Report issues at https://github.com/navicore/patch-seq/issues
