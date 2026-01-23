# Foreign Function Interface

Calling native C libraries from Seq.

## SQLite (sqlite/)

**sqlite-demo.seq** - Database access through FFI:

```seq
include ffi:sqlite

: main ( -- Int )
  "test.db" sqlite.open
  "CREATE TABLE users (id INTEGER, name TEXT)" sqlite.exec
  "INSERT INTO users VALUES (1, 'Alice')" sqlite.exec
  "SELECT * FROM users" sqlite.query
  sqlite.close
  0 ;
```

Requires `sqlite.toml` manifest defining the FFI bindings.

## Libedit (libedit-demo.seq)

Readline-style input using the libedit library for interactive command-line applications.

## Creating FFI Bindings

1. Create a TOML manifest defining the C functions
2. Use `include ffi:name` to load the bindings
3. Call functions with Seq-style names (e.g., `sqlite.open`)

See the [FFI Guide](../../docs/FFI_GUIDE.md) for complete documentation.
