# JSON Examples

Practical examples demonstrating JSON parsing and serialization in Seq.

## json_tree.seq - JSON Tree Viewer

An interactive tool that reads JSON from stdin, parses it, and displays the structure.

### Usage

```bash
# Build
cargo build --release
./target/release/seqc --output json_tree examples/json/json_tree.seq

# Run with piped input
echo '42' | ./json_tree
echo 'true' | ./json_tree
echo '"hello"' | ./json_tree
echo '[42]' | ./json_tree
echo '[]' | ./json_tree

# Or interactive (type JSON, press Enter)
./json_tree
```

### Example Output

```
$ echo '[42]' | ./json_tree
=== JSON Tree Viewer ===
Paste JSON and press Enter:

Input: [42]

Type: 4
Value:
  [42]
```

Type codes: 0=null, 1=bool, 2=number, 3=string, 4=array, 5=object

## What This Example Reveals We Need

Building this practical example highlighted several missing features that would make Seq more useful for real-world JSON processing:

### High Priority

1. **Command-line arguments** (`args`, `arg-count`)
   - Would allow: `./json_tree config.json`
   - Currently must use stdin piping

2. **File I/O** (`file-read`, `file-slurp`, `file-exists?`)
   - Would allow reading JSON files directly
   - Essential for any file-processing tool

3. **Write without newline** (`write` vs `write_line`)
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
