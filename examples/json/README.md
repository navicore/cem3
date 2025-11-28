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

3. **Multi-element arrays (up to 2 elements)** ✓
   - `[1]`, `[1, 2]`, `["a", "b"]`, `[42, "mixed"]`
   - Strings, numbers, booleans all work inside arrays

4. **Strings at any position** ✓
   - Strings now parse correctly whether top-level or inside arrays
   - `"hello"`, `["hello"]`, `["a", "b"]` all work

5. **Multi-element arrays** ✓
   - Arrays with any number of elements: `[1, 2, 3, ...]`
   - Nested arrays: `[[1, 2], [3, 4]]`
   - Mixed content: `[1, "hello", true, null]`

6. **Multi-pair objects** ✓
   - Objects with any number of key-value pairs
   - Nested objects: `{"person": {"name": "John", "age": 30}}`
   - Complex structures: `[{"name": "John"}, {"name": "Jane"}]`

7. **Functional collection builders** ✓
   - `array-with`: `( arr val -- arr' )` - append to array
   - `obj-with`: `( obj key val -- obj' )` - add key-value pair
   - `variant-append`: low-level primitive for building variants

### High Priority

1. **Write without newline** (`write` vs `write_line`)
   - Would allow proper indentation output
   - Currently can only output complete lines

### Medium Priority

2. **Pattern matching / case statement**
   - Would simplify tag-based dispatch
   - Currently requires nested if/else chains

### Nice to Have

5. **String escape sequences** (`\"`, `\\`, `\n`)
6. **Pretty-print with indentation levels**
7. **JSON path queries** (`$.foo.bar`)

## Current JSON Support

Works:
- Primitives: `null`, `true`, `false`
- Numbers: `42`, `-3.14`, `1e10`
- Strings: `"hello"`, `"hello world"` (no escapes)
- Arrays: `[]`, `[1]`, `[1, 2]`, `[1, 2, 3]`, nested arrays, any length
- Objects: `{}`, `{"a": 1}`, `{"a": 1, "b": 2}`, nested objects, any number of pairs
- Complex nested structures: `[{"name": "John", "age": 30}, {"name": "Jane"}]`

Serialization limits (parsing works for any size):
- Arrays: up to 3 elements display fully, 4+ show as `[...]`
- Objects: up to 2 pairs display fully, 3+ show as `{...}`

Limitations:
- String escapes: `"say \"hi\""` - not supported

## Technical Notes

### Why Serialization Has Size Limits

The serializer (`json-serialize-array`, `json-serialize-object`) uses nested if/else
chains to handle different sizes (0, 1, 2, 3 elements). This is because Seq currently
lacks:

1. **Loops** - No `for i in 0..count` construct
2. **Tail-call optimization** - Recursion would blow the stack for large collections
3. **Variant fold/map** - No way to iterate over variant fields from Seq

Possible solutions:
- Add a `variant-fold` runtime primitive: `( variant init quot -- result )`
- Add counted loops to the language
- Implement TCO for recursive serialization

### Why Parsing Has No Size Limits

Parsing uses recursive descent with the functional builders (`array-with`, `obj-with`).
Each recursive call builds up the collection incrementally. The stack usage is
proportional to nesting depth, not collection size, so `[1,2,3,...,1000]` works fine
but deeply nested structures could overflow.
