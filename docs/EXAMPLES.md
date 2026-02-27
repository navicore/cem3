# Examples

> **Note**: This file is auto-generated from README files in the `examples/` directory.
> Run `just gen-docs` to regenerate, or edit the source README files.


## Basics

Getting started with Seq - the simplest programs to verify your setup.

### hello-world.seq

The canonical first program:

```seq
: main ( -- Int ) "Hello, World!" io.write-line 0 ;
```

### cond.seq

Demonstrates the `cond` combinator for multi-way branching - a cleaner alternative to nested if/else.


## Language Features

Core Seq language concepts demonstrated through focused examples.

### Stack Effects (stack-effects.seq)

Stack effect declarations and how the type checker enforces them:

```seq
: square ( Int -- Int ) dup i.* ;
```

### Quotations (quotations.seq)

Anonymous code blocks that can be passed around and called:

```seq
: apply-twice ( Int { Int -- Int } -- Int )
  dup rot swap call swap call ;

5 [ 2 i.* ] apply-twice  # Result: 20
```

### Closures (closures.seq)

Quotations that capture values from their environment:

```seq
: make-adder ( Int -- { Int -- Int } )
  { i.+ } ;

10 make-adder  # Creates a closure that adds 10
5 swap call    # Result: 15
```

### Control Flow (control-flow.seq)

Conditionals, pattern matching, and loops:

```seq
: fizzbuzz ( Int -- String )
  dup 15 i.mod 0 i.= if drop "FizzBuzz"
  else dup 3 i.mod 0 i.= if drop "Fizz"
  else dup 5 i.mod 0 i.= if drop "Buzz"
  else int->string
  then then then ;
```

### Recursion (recursion.seq)

Tail-recursive algorithms with guaranteed TCO:

```seq
: factorial-acc ( Int Int -- Int )
  over 0 i.<= if nip
  else swap dup rot i.* swap 1 i.- swap factorial-acc
  then ;

: factorial ( Int -- Int ) 1 factorial-acc ;
```

### Strands (strands.seq)

Lightweight concurrent execution:

```seq
[ "Hello from strand!" io.write-line ] strand.spawn
```

### Include Demo (main.seq, http_simple.seq)

Demonstrates the module include system for code organization.


## Programming Paradigms

Seq is flexible enough to express multiple programming paradigms. These examples demonstrate different approaches to structuring programs.

### Object-Oriented (oop/)

**shapes.seq** - OOP patterns using unions and pattern matching:

- Encapsulation: data bundled in union variants
- Polymorphism: pattern matching dispatches to correct implementation
- Factory functions as constructors
- Type checks via `variant.tag` (like `instanceof`)

```seq
union Shape {
  Circle { radius: Float }
  Rectangle { width: Float, height: Float }
}

: shape.area ( Shape -- Float )
  match
    Circle { >radius } -> dup f.* 3.14159 f.*
    Rectangle { >width >height } -> f.*
  end ;
```

### Actor Model (actor/)

**actor_counters.seq** - CSP/Actor demonstration with hierarchical aggregation:

```
Company (aggregate)
  └── Region (aggregate)
        └── District (aggregate)
              └── Store (counter)
```

Features:
- Independent strands communicate via channels
- HTTP interface for queries and updates
- Request-response pattern with response channels

**counter.seq** - Simple generator pattern using weaves.

**sensor-classifier.seq** - Stream processing with structured data.

### Functional (functional/)

**lists.seq** - Higher-order functions and list processing:

```seq
# Built-in higher-order functions
list-of 1 lv 2 lv 3 lv 4 lv 5 lv
  [ 2 i.* ] list.map       # (2 4 6 8 10)
  [ 2 i.mod 0 i.= ] list.filter  # keep evens
  0 [ i.+ ] list.fold      # sum

# Functional pipelines
list-of 1 lv 2 lv 3 lv 4 lv 5 lv 6 lv 7 lv 8 lv 9 lv 10 lv
  keep-odds      # filter to 1,3,5,7,9
  square-each    # map to 1,9,25,49,81
  sum            # fold to 165
```

Features:
- **map**: Transform each element with a quotation
- **filter**: Keep elements matching a predicate
- **fold**: Reduce list to single value with accumulator
- Composable operations for data pipelines

### Logic (logic/)

*Coming soon* - Backtracking, unification patterns.

### Dataflow (dataflow/)

*Coming soon* - Reactive and stream-based patterns.


## Data Formats & Structures

Working with structured data in Seq.

### JSON (json/)

**json_tree.seq** - Parse and traverse JSON:

```seq
include std:json

: main ( -- Int )
  "{\"name\": \"Alice\", \"age\": 30}" json-parse
  "name" json.get json.as-string io.write-line
  0 ;
```

### YAML (yaml/)

YAML parsing with support for:
- Multiline strings
- Nested structures
- Anchors and aliases

### SON (son/)

**serialize.seq** - Seq Object Notation, Seq's native serialization format optimized for stack-based data.

### Zipper (zipper/)

**zipper-demo.seq** - Functional list navigation with O(1) cursor movement:

```seq
include std:zipper

{ 1 2 3 4 5 } list->zipper
zipper.right zipper.right  # Move to element 3
100 zipper.set             # Replace with 100
zipper.to-list             # { 1 2 100 4 5 }
```

### Encoding (encoding.seq)

Base64, hex, and other encoding/decoding operations.


### JSON Examples

Practical examples demonstrating JSON parsing and serialization in Seq.

#### json_tree.seq - JSON Tree Viewer

An interactive tool that reads JSON from files, command-line, or stdin, parses it, and displays the structure.

##### Usage

```bash
### Build
cargo build --release
./target/release/seqc build examples/json/json_tree.seq -o json_tree

### Read from a JSON file (preferred)
./json_tree config.json
./json_tree data/users.json

### Or with command-line JSON string
./json_tree '42'
./json_tree 'true'
./json_tree '"hello world"'
./json_tree '[42]'

### Or with piped input
echo '42' | ./json_tree

### Or interactive (type JSON, press Enter)
./json_tree
```

##### Example Output

```
$ ./json_tree '[42]'
=== JSON Tree Viewer ===

Input: [42]

Type: 4
Value:
  [42]
```

Type codes: 0=null, 1=bool, 2=number, 3=string, 4=array, 5=object

#### Current JSON Support

Works:
- Primitives: `null`, `true`, `false`
- Numbers: `42`, `-3.14`, `1e10`
- Strings: `"hello"`, `"hello world"`, `"say \"hi\""` (with escape sequences)
- Arrays: `[]`, `[1]`, `[1, 2]`, `[1, 2, 3]`, nested arrays, any length
- Objects: `{}`, `{"a": 1}`, `{"a": 1, "b": 2}`, nested objects, any number of pairs
- Complex nested structures: `[{"name": "John", "age": 30}, {"name": "Jane"}]`

Serialization limits (parsing works for any size):
- Arrays: up to 3 elements display fully, 4+ show as `[...]`
- Objects: up to 2 pairs display fully, 3+ show as `{...}`

#### Technical Notes

##### Why Serialization Has Size Limits

The serializer (`json-serialize-array`, `json-serialize-object`) uses nested if/else
chains to handle different sizes (0, 1, 2, 3 elements). This is because Seq currently
lacks:

1. **Loops** - No `for i in 0..count` construct
2. **Variant fold/map** - No way to iterate over variant fields from Seq

Possible solutions:
- Add a `variant-fold` runtime primitive: `( variant init quot -- result )`
- Add counted loops to the language

##### Why Parsing Has No Size Limits

Parsing uses recursive descent with the functional builders (`array-with`, `obj-with`).
Each recursive call builds up the collection incrementally. The stack usage is
proportional to nesting depth, not collection size, so `[1,2,3,...,1000]` works fine
but deeply nested structures could overflow.


### YAML Examples

Examples demonstrating the YAML parsing library implemented in Seq.

#### Overview

The YAML library (`std:yaml`) is written entirely in Seq, using only the
existing language primitives. This validates that the builtin/stdlib balance
allows building complex parsers without language changes.

#### Primitives Used

The YAML parser uses these existing primitives:
- String operations: `string.find`, `string.substring`, `string.trim`, `string.empty?`, `string.length`, `string.char-at`, `string.concat`, `string->float`
- Character conversion: `char->string`
- Variant operations: `variant.make-0`, `variant.make-1`, `variant.tag`, `variant.field-at`, `variant.field-count`, `variant.append`
- Standard stack operations: `dup`, `drop`, `swap`, `over`, `rot`
- Arithmetic and comparison: `i.+`, `i.-`, `i.<`, `i.>`, `i.=`, `i.<>`
- Control flow: `if/else/then`

No new primitives were required.

#### Examples

##### yaml_test.seq
Basic tests for single-line YAML parsing:
- Strings: `name: John`
- Numbers: `age: 42`, `price: 19.99`
- Booleans: `active: true`, `enabled: false`
- Null: `data: null`, `empty: ~`

##### yaml_multiline.seq
Tests for multi-line YAML documents:
- Multiple key-value pairs
- Blank lines (ignored)
- Comments (lines starting with #)

#### Running

```bash
cargo run --release -- build examples/yaml/yaml_test.seq -o /tmp/yaml_test
/tmp/yaml_test

cargo run --release -- build examples/yaml/yaml_multiline.seq -o /tmp/yaml_multi
/tmp/yaml_multi
```

#### Supported YAML Features

- Multi-line documents with multiple key-value pairs
- String values (unquoted)
- Integer and floating-point numbers
- Booleans (true/false)
- Null values (null or ~)
- Comments (# to end of line)
- Blank lines

#### Not Yet Supported

- Nested objects (indentation-based nesting)
- Arrays/lists (- item syntax)
- Multi-line strings (| and > block scalars)
- Quoted strings with escapes
- Anchors and aliases


## Input/Output

Networking, file I/O, terminal, and text processing.

### HTTP Server (http/)

**http_server.seq** - TCP server with HTTP routing:

```seq
include std:http

: handle-request ( TcpStream -- )
  tcp.read-request
  request-path "/" string.equal? if
    "Hello from Seq!" 200 make-response
  else
    "Not Found" 404 make-response
  then
  tcp.write-response ;
```

**test_simple.seq** - Basic HTTP request/response testing.

### HTTP Client (http-client.seq)

Making HTTP requests using the std:http module:

```seq
include std:http

"https://api.example.com/data" http.get
http.body io.write-line
```

### Terminal (terminal/)

**terminal-demo.seq** - Terminal colors, cursor control, and formatting using ANSI escape sequences.

### Operating System (os/)

**os-demo.seq** - Environment variables, paths, and system information.

### Text Processing (text/)

**log-parser.seq** - Parsing structured log files with string operations.

**regex-demo.seq** - Regular expression matching and extraction.

### Compression (compress-demo.seq)

Zstd compression and decompression for efficient data storage.


### HTTP Server Example for Seq

A complete concurrent HTTP server demonstrating Seq's capabilities:
- TCP socket operations
- Concurrent request handling with strands (green threads)
- Channel-based communication (CSP)
- Closure capture for spawned workers
- HTTP routing with pattern matching

#### Prerequisites

Build the compiler:
```bash
cargo build --release
```

#### Running the Server

```bash
./target/release/seqc build examples/http/http_server.seq -o /tmp/http_server
/tmp/http_server
```

The server listens on port 8080 and handles multiple concurrent connections.

#### Testing

In another terminal:

```bash
### Test root endpoint
curl http://localhost:8080/
### Output: Hello from Seq!

### Test health endpoint
curl http://localhost:8080/health
### Output: OK

### Test echo endpoint
curl http://localhost:8080/echo
### Output: Echo!

### Test 404 handling
curl http://localhost:8080/invalid
### Output: 404 Not Found
```

#### How It Works

The server demonstrates several Seq features:

1. **TCP Operations**: `tcp-listen`, `tcp-accept`, `tcp-read`, `tcp-write`, `tcp-close`
2. **Routing**: Uses `cond` combinator for multi-way branching on request paths
3. **Concurrency**: Each connection is handled in a separate strand (green thread)
4. **Channels**: Spawned workers receive socket IDs via channels
5. **Closures**: The `[ worker ]` quotation captures the channel ID when spawned

##### Architecture

```
main
  ├─ tcp-listen (creates listener socket)
  └─ accept-loop (infinite)
       ├─ tcp-accept (waits for connection)
       ├─ chan.make (creates communication channel)
       ├─ strand.spawn [ worker ] (launches handler strand with channel)
       └─ chan.send (passes socket ID to worker via channel)

worker strand
  ├─ chan.receive (gets socket ID from channel)
  └─ handle-connection
       ├─ tcp-read (reads HTTP request)
       ├─ route (pattern matches to response)
       ├─ tcp-write (sends HTTP response)
       └─ tcp-close (cleanup)
```

#### Key Features

**Non-blocking I/O**: All TCP operations cooperate with May's coroutine scheduler, yielding instead of blocking OS threads.

**Efficient Concurrency**: The server can handle thousands of concurrent connections using lightweight strands.

**Stack-based Routing**: HTTP routing is implemented using Seq's `cond` combinator, demonstrating clean concatenative style.

#### Next Steps

This example serves as a foundation for:
- RESTful APIs with JSON
- WebSocket servers
- HTTP client implementations
- More sophisticated routing (path parameters, query strings)

#### References

- [Seq Roadmap](../../docs/ROADMAP.md)
- [Concatenative Design](../../docs/CLEAN_CONCATENATIVE_DESIGN.md)


## Complete Projects

Larger applications demonstrating Seq's capabilities.

### Lisp Interpreter (lisp/)

A complete Lisp interpreter in Seq:

| File | Purpose |
|------|---------|
| `sexpr.seq` | S-expression data types (ADTs) |
| `tokenizer.seq` | Lexical analysis |
| `parser.seq` | Parsing tokens to AST |
| `eval.seq` | Evaluation with environments |
| `test_*.seq` | Test files for each component |

Supported features:
- Numbers and symbols
- Arithmetic: `+`, `-`, `*`, `/`
- `let` bindings
- `if` conditionals
- `lambda` with closures

This project demonstrates:
- **Union types (ADTs)** for the AST
- **Pattern matching** for dispatch
- **Recursive descent** parsing
- **Environment passing** for lexical scope

### Hacker's Delight (hackers-delight/)

Bit manipulation algorithms from the book *Hacker's Delight*:

| File | Algorithm |
|------|-----------|
| `01-rightmost-bits.seq` | Isolate, clear, and propagate rightmost bits |
| `02-power-of-two.seq` | Check and round to powers of two |
| `03-counting-bits.seq` | Population count, leading/trailing zeros |
| `04-branchless.seq` | Branchless min, max, abs, sign |
| `05-swap-reverse.seq` | Bit reversal and byte swapping |

Demonstrates Seq's bitwise operations: `band`, `bor`, `bxor`, `shl`, `shr`, `popcount`, `clz`, `ctz`.

### Cryptography (crypto.seq)

Cryptographic operations including hashing and encoding.

### Shopping Cart (shopping-cart/)

A domain modeling example showing how to structure a typical business application with Seq.


### Hacker's Delight Examples

Bit manipulation puzzles inspired by the classic techniques in low-level programming.

#### Files

| File | Topic |
|------|-------|
| `01-rightmost-bits.seq` | Rightmost bit manipulation (turn off, isolate, propagate) |
| `02-power-of-two.seq` | Power of 2 detection, next power, log2 |
| `03-counting-bits.seq` | Popcount algorithms, parity, leading/trailing zeros |
| `04-branchless.seq` | Branchless abs, sign, min, max |
| `05-swap-reverse.seq` | XOR swap, bit reversal, bit set/clear/toggle |

#### Running

```bash
seqc build examples/hackers-delight/01-rightmost-bits.seq -o /tmp/demo && /tmp/demo
```

#### Bitwise Operations Used

These examples use Seq's bitwise operations:

- `band` - bitwise AND
- `bor` - bitwise OR
- `bxor` - bitwise XOR
- `bnot` - bitwise NOT
- `shl` - shift left
- `shr` - logical shift right
- `popcount` - count 1-bits
- `clz` - count leading zeros
- `ctz` - count trailing zeros
- `int-bits` - bit width (64)

#### Numeric Literals

Seq supports hex and binary literals for bit manipulation:

```seq
0xFF        # hex: 255
0b10101010  # binary: 170
```


### Shopping Cart REST Server

A complete example demonstrating:
- HTTP REST API with multiple endpoints
- SQLite persistence with prepared statements
- Database transactions for checkout
- URL query parameter parsing
- Concurrent request handling with strands

#### Build

```bash
seqc build --ffi-manifest examples/shopping-cart/sqlite.toml \
     examples/shopping-cart/shopping-cart.seq -o shopping-cart
```

#### Run

```bash
./shopping-cart
```

#### API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/` | API info |
| GET | `/products` | List all products |
| GET | `/cart` | View cart contents |
| POST | `/cart/add?product=ID&qty=N` | Add item to cart |
| POST | `/cart/remove?id=N` | Remove item from cart |
| POST | `/cart/checkout` | Process order with transaction |

#### Test with curl

```bash
### List products
curl http://localhost:8080/products

### Add items to cart
curl -X POST "http://localhost:8080/cart/add?product=1&qty=2"
curl -X POST "http://localhost:8080/cart/add?product=4&qty=1"

### View cart
curl http://localhost:8080/cart

### Checkout (uses transaction)
curl -X POST http://localhost:8080/cart/checkout

### Verify stock was updated
curl http://localhost:8080/products
```

#### Database

The server creates `shop.db` with three tables:
- `products` - Product catalog with stock tracking
- `cart_items` - Shopping cart
- `orders` - Completed orders

Check the database directly:
```bash
sqlite3 shop.db "SELECT * FROM products"
sqlite3 shop.db "SELECT * FROM orders"
```

#### Features Demonstrated

##### SQLite FFI
- `db-open`, `db-close` - Connection management
- `db-exec` - Simple SQL execution
- `db-prepare`, `db-step`, `db-finalize` - Prepared statements
- `db-column-int`, `db-column-text` - Result extraction

##### Transactions
The checkout process uses `BEGIN TRANSACTION` / `COMMIT` / `ROLLBACK`:
1. Calculate cart total
2. Begin transaction
3. Update product stock
4. Create order record
5. Clear cart
6. Commit (or rollback on error)

##### HTTP Server
- Route matching by method and path
- Query parameter parsing
- Concurrent request handling with strands


## Foreign Function Interface

Calling native C libraries from Seq.

### SQLite (sqlite/)

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

### Libedit (libedit-demo.seq)

Readline-style input using the libedit library for interactive command-line applications.

### Creating FFI Bindings

1. Create a TOML manifest defining the C functions
2. Use `include ffi:name` to load the bindings
3. Call functions with Seq-style names (e.g., `sqlite.open`)

See the [FFI Guide](../../docs/FFI_GUIDE.md) for complete documentation.


### SQLite FFI Example

This example demonstrates using SQLite via FFI, including the `by_ref` pass mode
for out parameters (used by `sqlite3_open` to return the database handle).

#### Building

```bash
seqc build --ffi-manifest examples/ffi/sqlite/sqlite.toml \
     examples/ffi/sqlite/sqlite-demo.seq \
     -o sqlite-demo
./sqlite-demo
```

#### Dependencies

- **macOS**: SQLite is pre-installed
- **Ubuntu/Debian**: `apt install libsqlite3-dev`
- **Fedora**: `dnf install sqlite-devel`

#### FFI Features Demonstrated

##### `by_ref` Out Parameters

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

**Important: Ownership Semantics**

The `by_ref` pointer value pushed onto the stack is an **opaque handle** owned by
the C library (SQLite in this case). You must:

- Only pass it to functions from the same library (e.g., `db-exec`, `db-close`)
- Never attempt to free it manually
- Always close/release it using the library's cleanup function (`db-close`)
- Not store it beyond its valid lifetime

The compiler treats these as integers for simplicity, but they are NOT arbitrary
integers - they are pointers that must be used according to the C library's API.

##### Fixed Value Arguments

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

---

## See Also

- [Language Guide](language-guide.md) - Core language concepts
- [Weaves Guide](WEAVES_GUIDE.md) - Generators and coroutines
- [Testing Guide](TESTING_GUIDE.md) - Writing and running tests
- [seqlings](https://github.com/navicore/seqlings) - Interactive exercises
