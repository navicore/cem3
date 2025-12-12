# SQLite FFI Example

This example demonstrates using SQLite via FFI, including the `by_ref` pass mode
for out parameters (used by `sqlite3_open` to return the database handle).

## Building

```bash
seqc --ffi-manifest examples/ffi/sqlite/sqlite.toml \
     examples/ffi/sqlite/sqlite-demo.seq \
     -o sqlite-demo
./sqlite-demo
```

## Dependencies

- **macOS**: SQLite is pre-installed
- **Ubuntu/Debian**: `apt install libsqlite3-dev`
- **Fedora**: `dnf install sqlite-devel`

## FFI Features Demonstrated

### `by_ref` Out Parameters

SQLite's `sqlite3_open` returns the database handle via an out parameter:

```c
int sqlite3_open(const char *filename, sqlite3 **ppDb);
```

In the FFI manifest, this is declared as:

```toml
[[library.function]]
c_name = "sqlite3_open"
seq_name = "db-open"
stack_effect = "( String -- Int Int )"
args = [
  { type = "string", pass = "c_string" },
  { type = "ptr", pass = "by_ref" }
]
[library.function.return]
type = "int"
```

The `by_ref` argument doesn't come from the Seq stack - instead:
1. The compiler allocates local storage
2. Passes a pointer to that storage to the C function
3. After the call, reads the value and pushes it onto the stack

Result: `db-open` has stack effect `( String -- Int Int )` where the first Int
is the database handle (from the out param) and the second is the return code.

### Fixed Value Arguments

For `sqlite3_exec`, we pass NULL for unused callback parameters:

```toml
args = [
  { type = "ptr", pass = "ptr" },
  { type = "string", pass = "c_string" },
  { type = "ptr", value = "null" },  # callback
  { type = "ptr", value = "null" },  # callback arg
  { type = "ptr", value = "null" }   # error msg
]
```

Arguments with `value` don't come from the stack - they're compiled as constants.
