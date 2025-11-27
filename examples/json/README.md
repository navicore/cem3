# JSON Examples

Practical examples demonstrating JSON parsing and serialization in Seq.

## json_tree.seq - JSON Tree Viewer

An interactive tool that reads JSON from files, command-line, or stdin, parses it, and displays the structure.

### Usage

```bash
# Build
cargo build --release
./target/release/seqc --output json_tree examples/json/json_tree.seq

# Read from a JSON file (preferred)
./json_tree config.json
./json_tree data/users.json

# Or with command-line JSON string
./json_tree '42'
./json_tree 'true'
./json_tree '"hello world"'
./json_tree '[42]'

# Or with piped input
echo '42' | ./json_tree

# Or interactive (type JSON, press Enter)
./json_tree
```

### Example Output

```
$ ./json_tree '[42]'
=== JSON Tree Viewer ===

Input: [42]

Type: 4
Value:
  [42]
```

Type codes: 0=null, 1=bool, 2=number, 3=string, 4=array, 5=object

## What This Example Reveals We Need

Building this practical example highlighted several missing features that would make Seq more useful for real-world JSON processing:

### Implemented

1. **Command-line arguments** (`arg-count`, `arg`) ✓
   - `arg-count` returns number of arguments (including program name)
   - `arg` takes an index and returns the argument string
   - Example: `./json_tree '[42]'` now works!

2. **File I/O** (`file-slurp`, `file-exists?`) ✓
   - `file-slurp` reads entire file contents as a string
   - `file-exists?` checks if a file exists (returns 1 or 0)
   - Example: `./json_tree config.json` now works!

### High Priority

1. **Write without newline** (`write` vs `write_line`)
   - Would allow proper indentation output
   - Currently can only output complete lines

### Medium Priority

4. **Multi-element array parsing**
   - Currently only single-element arrays work: `[42]`
   - Need: `[1, 2, 3]`

5. **Object key-value parsing**
   - Currently only empty objects work: `{}`
   - Need: `{"key": "value"}`

6. **Pattern matching / case statement**
   - Would simplify tag-based dispatch
   - Currently requires nested if/else chains

### Nice to Have

7. **String escape sequences** (`\"`, `\\`, `\n`)
8. **Pretty-print with indentation levels**
9. **JSON path queries** (`$.foo.bar`)

## Current JSON Support

Works:
- Primitives: `null`, `true`, `false`
- Numbers: `42`, `-3.14`, `1e10`
- Strings: `"hello"` (no escapes)
- Single-element arrays: `[42]`, `["hi"]`
- Empty containers: `[]`, `{}`

Limitations:
- Multi-element arrays: `[1, 2]` - not parsed yet
- Objects with data: `{"a": 1}` - not parsed yet
- String escapes: `"say \"hi\""` - not supported
