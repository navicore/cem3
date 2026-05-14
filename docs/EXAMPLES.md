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
  dup 15 i.modulo drop 0 i.= if drop "FizzBuzz"
  else dup 3 i.modulo drop 0 i.= if drop "Fizz"
  else dup 5 i.modulo drop 0 i.= if drop "Buzz"
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

### Union Types (unions.seq)

Algebraic data types (sum types) with pattern matching:

```seq
union Option {
  Some { value: Int }
  None
}

: unwrap-or ( Option Int -- Int )
  swap match
    Some { >value } -> nip    # return the value
    None ->                   # return the default
  end
;

42 Make-Some 0 unwrap-or   # Result: 42
Make-None 99 unwrap-or     # Result: 99
```

Covers Option, Result, Message passing, and recursive tree structures.

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
## Built-in higher-order functions
list-of 1 lv 2 lv 3 lv 4 lv 5 lv
  [ 2 i.* ] list.map       # (2 4 6 8 10)
  [ 2 mod 0 i.= ] list.filter  # keep evens
  0 [ i.+ ] list.fold      # sum

## Functional pipelines
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
  "{\"name\": \"Alice\", \"age\": 30}" json.parse
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
./target/release/seqc --output json_tree examples/json/json_tree.seq

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

#### What This Example Reveals We Need

Building this practical example highlighted several missing features that would make Seq more useful for real-world JSON processing:

##### Implemented

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

##### High Priority

1. **Write without newline** (`write` vs `write_line`)
   - Would allow proper indentation output
   - Currently can only output complete lines

##### Medium Priority

2. **Pattern matching / case statement**
   - Would simplify tag-based dispatch
   - Currently requires nested if/else chains

##### Nice to Have

5. **String escape sequences** (`\"`, `\\`, `\n`)
6. **Pretty-print with indentation levels**
7. **JSON path queries** (`$.foo.bar`)

#### Current JSON Support

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

#### Technical Notes

##### Why Serialization Has Size Limits

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
- String operations: `string-find`, `string-substring`, `string-trim`, `string-empty`, `string-length`, `string-char-at`, `string-concat`, `string->float`
- Character conversion: `char->string`
- Variant operations: `make-variant-0`, `make-variant-1`, `variant-tag`, `variant-field-at`, `variant-field-count`, `variant-append`
- Standard stack operations: `dup`, `drop`, `swap`, `over`, `rot`
- Arithmetic and comparison: `add`, `subtract`, `<`, `>`, `=`, `<>`
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
cargo run --release -- examples/yaml/yaml_test.seq -o /tmp/yaml_test
/tmp/yaml_test

cargo run --release -- examples/yaml/yaml_multiline.seq -o /tmp/yaml_multi
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

File I/O, terminal, OS, text processing, compression. The non-network
side of I/O.

For networking examples (TCP, UDP, TLS, DNS, HTTP) see
[`../net/`](../net/README.md).

### Terminal (terminal/)

**terminal-demo.seq** - Terminal colors, cursor control, and formatting using ANSI escape sequences.

### Operating System (os/)

**os-demo.seq** - Environment variables, paths, and system information.

### Text Processing (text/)

**log-parser.seq** - Parsing structured log files with string operations.

**regex-demo.seq** - Regular expression matching and extraction.

### Compression (compress-demo.seq)

Zstd compression and decompression for efficient data storage.


## Networking

Five layers, bottom to top: DNS → TCP → UDP → TLS → HTTP. Each subfolder
has runnable example(s) using the corresponding `net.*` builtins.

The stack is **may-aware end to end**: every IO step yields the
cooperative carrier instead of blocking it. Hostnames resolve through
a dedicated worker pool so `getaddrinfo` runs off the carrier. See
`docs/STDLIB_REFERENCE.md` for the full word reference, or
`docs/design/done/NONBLOCKING_NETWORKING.md` for the design rationale.

### Examples

#### `dns/resolve.seq` — `net.dns.resolve`

Resolves a hostname and prints each IP returned. Demonstrates the
worker-pool-offload path; useful as a one-liner for "what does this
hostname actually resolve to from inside Seq."

```
seqc build dns/resolve.seq -o /tmp/dns-resolve
/tmp/dns-resolve
```

#### `tcp/client.seq` — `net.tcp.connect`

Minimal TCP client: connect to `example.com:80`, send an HTTP/1.0
request, read the response, close. Plain TCP — no framing helpers —
to show what `net.tcp.*` looks like on its own.

```
seqc build tcp/client.seq -o /tmp/tcp-client
/tmp/tcp-client
```

#### `tcp/server.seq` — plain TCP echo server

Not all networking is HTTP. Accepts a TCP connection, echoes whatever
the client sent back, closes. Each connection runs in its own strand
(green thread) so the server handles concurrent clients cooperatively.

```
seqc build tcp/server.seq -o /tmp/tcp-server
/tmp/tcp-server &
echo hello | nc localhost 9000
```

#### `tcp/http-routing.seq` — HTTP server on top of `net.tcp.*`

The companion to `tcp/server.seq`: same accept-loop shape, with
HTTP/1.1 request parsing and a `cond`-driven router on top. Doubles as
a tutorial on concatenative programming (the source is heavily
commented). Lives under `tcp/` because `net.http.*` is client-only —
the server is hand-rolled over `net.tcp.read` / `net.tcp.write`.

```
seqc build tcp/http-routing.seq -o /tmp/http-routing
/tmp/http-routing &
curl http://localhost:8080/
curl http://localhost:8080/health
curl http://localhost:8080/echo
curl http://localhost:8080/invalid   # 404
```

#### `udp/echo.seq` — `net.udp.bind` + `net.udp.send-to` + `net.udp.receive-from`

Single-program UDP loopback: bind two sockets, send a datagram from
one to the other, receive it, print. Mirrors the round-trip pattern
the integration test uses.

```
seqc build udp/echo.seq -o /tmp/udp-echo
/tmp/udp-echo
```

#### `tls/client.seq` — `net.tls.client`

The TCP client above with one extra step: after `net.tcp.connect`
returns the Socket, `net.tls.client` upgrades it in place to a
TLS-wrapped Socket. Subsequent `net.tcp.read` / `net.tcp.write` calls
dispatch through rustls transparently — the rest of the code looks
exactly like the plain-TCP version.

```
seqc build tls/client.seq -o /tmp/tls-client
/tmp/tls-client
```

#### `http/client.seq` — `net.http.get` / `.post` / `.put` / `.delete`

High-level HTTP/1.1 client: hand `net.http.get` a URL, get a response
Map back (`status`, `body`, `ok`, `error`). The client handles DNS
resolution, SSRF validation against the resolved IPs, connection
pooling keyed on `(scheme, host, port)`, TLS for `https://`, and
HTTP/1.1 framing — all the layers below are still there, just
composed into one builtin. Exercises GET, POST, PUT, DELETE against
httpbin.org.

```
seqc build http/client.seq -o /tmp/http-client
/tmp/http-client
```

### Reading order

For a layered tour, read top-to-bottom: `dns/resolve.seq` →
`tcp/client.seq` → `tcp/server.seq` → `tls/client.seq` →
`http/client.seq`. The HTTP-routing server (`tcp/http-routing.seq`)
is the "everything on top of TCP" deep dive once the rest clicks.

### What's not here yet

- mTLS, ALPN selection, peer-cert inspection (planned follow-ups —
  see issue #483 for the test anchors).
- Per-request timeouts (planned — see issue #484).
- A header-bag API for custom HTTP request headers.


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

### Shamir's Secret Sharing (sss.seq)

A tutorial implementation of [Shamir's Secret Sharing](https://en.wikipedia.org/wiki/Shamir%27s_secret_sharing) over GF(256), the same finite field used by AES. A secret is split into N shares such that any K can reconstruct it, but K-1 shares reveal nothing.

Demonstrates:
- **GF(256) finite field arithmetic** — addition (XOR), peasant multiplication, Fermat inverse
- **Polynomial evaluation** via Horner's method
- **Lagrange interpolation** to reconstruct secrets from share subsets
- **Packed accumulators** — encoding two byte values in one Int for `list.fold`
- **Deep stack management** — `pick`/`roll` patterns for 4+ item stacks
- **Cryptographic randomness** — `crypto.random-int` for polynomial coefficients

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
seqc examples/hackers-delight/01-rightmost-bits.seq -o /tmp/demo && /tmp/demo
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
- `int-bits` - bit width (63 — Seq Int is signed 63-bit)

#### Numeric Literals

Seq supports hex and binary literals for bit manipulation:

```seq
0xFF        # hex: 255
0b10101010  # binary: 170
```


### Seq → OSC → Csound (live-coding POC)

This directory is the Phase C POC from
[`docs/design/LIVE_CODING_CSOUND_POC.md`](../../../docs/design/LIVE_CODING_CSOUND_POC.md):
prove that Seq can drive an external audio engine (Csound) over OSC well
enough to support live-coding music.

#### What's here

| File | Purpose |
|---|---|
| `osc.seq` | OSC 1.0 encoder, written in Seq. Library, no `main`. |
| `test_osc.seq` | Byte-exact unit tests for the encoder. |
| `test_osc_loopback.seq` | End-to-end UDP round-trip tests (no audio). |
| `live.csd` | Csound orchestra: OSC listener on port 7770 + kick instrument. |
| `tone.seq` | One-shot driver — sends a single `/kick 220.0` message. |
| `live.seq` | 8-beat metronome driver — sends 8 evenly-spaced kicks. |

#### Audible run

The encoder/loopback tests run in CI (`just ci`). The audible parts
below need a working Csound install on your machine.

##### 1. Install Csound

macOS (Homebrew):

```sh
brew install csound
```

Linux (Debian/Ubuntu):

```sh
sudo apt-get install csound
```

Verify:

```sh
csound --version
```

##### 2. Start the listener

In one terminal, from the repo root:

```sh
csound -odac examples/projects/live-coding-csound/live.csd
```

`-odac` writes audio to your default output device. You should see
Csound print its banner, list the instruments, and sit waiting (last
line will say something like `SECTION 1:` followed by no further
output). Leave this terminal running.

##### 3. Send one kick (`tone.seq`)

In a second terminal, build and run the one-shot driver:

```sh
just build
target/examples/projects-live-coding-csound-tone
```

You should hear a single short percussive tone at 220 Hz. The Seq
process exits immediately; Csound stays up so you can run again.

##### 4. Run the metronome (`live.seq`)

```sh
target/examples/projects-live-coding-csound-live
```

You should hear 8 evenly-spaced kicks over roughly 4 seconds (120 BPM).

##### 5. Live-coding loop

Edit `live.seq` (e.g. change `bpm-ms` from `500` to `250` for 240 BPM,
or `beats` from `8` to `16`), then run `just build` and re-execute
the binary. Csound keeps running between Seq runs, so the iteration
cycle is just edit → build → re-run.

To stop everything: `Ctrl+C` in the Csound terminal.

#### Troubleshooting

**Nothing audible after `tone`/`live`.** Check the Csound terminal:
when an OSC message lands, Csound prints something like
`new alloc for instr 2:` and `ihold:` lines. If those are absent,
the message isn't reaching Csound. Verify the port matches (Csound
listens on 7770; Seq sends to 7770).

**`csound: command not found`.** Install per step 1 above.

**`Address already in use` from Csound.** Another process holds port
7770. `lsof -i :7770` to find it; kill the holder or change the port
in *both* `live.csd` and the `7770` literal in `tone.seq` / `live.seq`.

**Audio cuts out / glitches.** Csound's default audio backend can be
finicky. Try `csound -odac0 live.csd` to force the system default
device, or pass `-+rtaudio=...` to pick a specific backend (CoreAudio
on macOS, ALSA/JACK on Linux).

#### How this fits the design doc

- **Checkpoint 1 (UDP loopback)** — covered by `test_osc_loopback.seq`,
  runs in CI.
- **Checkpoint 2 (OSC fixture test)** — covered by `test_osc.seq`,
  runs in CI.
- **Checkpoint 3 (Csound responds, one tone)** — `tone.seq` + this
  README. Manual verification.
- **Checkpoint 4 (a bar of music)** — `live.seq` + this README.
  Manual verification.
- **Checkpoint 5 (POC writeup decides spinout question)** — once
  you've run the metronome a few times and exercised the
  edit-build-rerun loop, the design doc gets a final block recording
  what worked, what surfaced as a Seq language gap, and whether the
  whole thing is worth spinning into its own repo.


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
seqc --ffi-manifest examples/ffi/sqlite/sqlite.toml \
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
