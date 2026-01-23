# Examples

> **Note**: This file is auto-generated from README files in the `examples/` directory.
> Edit those files instead of this one.

The `examples/` directory contains programs demonstrating Seq's features, organized by category. These examples are tested in CI and serve as both documentation and regression tests.

## Categories

| Directory | Description |
|-----------|-------------|
| [basics/](basics/) | Getting started - hello world and simple programs |
| [language/](language/) | Core language features - quotations, closures, recursion |
| [paradigms/](paradigms/) | Programming paradigms - OOP, actors, functional |
| [data/](data/) | Data formats - JSON, YAML, SON, zipper |
| [io/](io/) | Input/output - HTTP, terminal, files, text processing |
| [projects/](projects/) | Complete applications - Lisp interpreter, crypto, algorithms |
| [ffi/](ffi/) | Foreign function interface - SQLite, libedit |

## Running Examples

```bash
# Build and run
seqc build examples/basics/hello-world.seq -o /tmp/hello
/tmp/hello

# Or use script mode (compile + run in one step)
seqc examples/basics/hello-world.seq
```

## Learning Path

If you're new to Seq, we suggest this order:

1. `basics/hello-world.seq` - Verify your setup
2. `language/stack-effects.seq` - Understand the type system
3. `language/control-flow.seq` - Conditionals and recursion
4. `language/quotations.seq` - First-class functions
5. `data/json/json_tree.seq` - Real-world data processing
6. `paradigms/actor/` - Concurrency patterns
7. `projects/lisp/` - A complete interpreter

---

# Basics

Getting started with Seq - the simplest programs to verify your setup.

## hello-world.seq

The canonical first program:

```seq
: main ( -- Int ) "Hello, World!" io.write-line 0 ;
```

## cond.seq

Demonstrates the `cond` combinator for multi-way branching - a cleaner alternative to nested if/else.

---

# Language Features

Core Seq language concepts demonstrated through focused examples.

## Stack Effects (stack-effects.seq)

Stack effect declarations and how the type checker enforces them:

```seq
: square ( Int -- Int ) dup i.* ;
```

## Quotations (quotations.seq)

Anonymous code blocks that can be passed around and called:

```seq
: apply-twice ( Int { Int -- Int } -- Int )
  dup rot swap call swap call ;

5 [ 2 i.* ] apply-twice  # Result: 20
```

## Closures (closures.seq)

Quotations that capture values from their environment:

```seq
: make-adder ( Int -- { Int -- Int } )
  { i.+ } ;

10 make-adder  # Creates a closure that adds 10
5 swap call    # Result: 15
```

## Control Flow (control-flow.seq)

Conditionals, pattern matching, and loops:

```seq
: fizzbuzz ( Int -- String )
  dup 15 i.mod 0 i.= if drop "FizzBuzz"
  else dup 3 i.mod 0 i.= if drop "Fizz"
  else dup 5 i.mod 0 i.= if drop "Buzz"
  else int->string
  then then then ;
```

## Recursion (recursion.seq)

Tail-recursive algorithms with guaranteed TCO:

```seq
: factorial-acc ( Int Int -- Int )
  over 0 i.<= if nip
  else swap dup rot i.* swap 1 i.- swap factorial-acc
  then ;

: factorial ( Int -- Int ) 1 factorial-acc ;
```

## Strands (strands.seq)

Lightweight concurrent execution:

```seq
[ "Hello from strand!" io.write-line ] strand.spawn
```

## Include Demo (main.seq, http_simple.seq)

Demonstrates the module include system for code organization.

---

# Programming Paradigms

Seq is flexible enough to express multiple programming paradigms. These examples demonstrate different approaches to structuring programs.

## Object-Oriented (oop/)

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

## Actor Model (actor/)

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

## Functional (functional/)

*Coming soon* - Pure functional patterns, composition, immutability.

## Logic (logic/)

*Coming soon* - Backtracking, unification patterns.

## Dataflow (dataflow/)

*Coming soon* - Reactive and stream-based patterns.

---

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

---

# Input/Output

Networking, file I/O, terminal, and text processing.

## HTTP Server (http/)

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

## HTTP Client (http-client.seq)

Making HTTP requests using the std:http module:

```seq
include std:http

"https://api.example.com/data" http.get
http.body io.write-line
```

## Terminal (terminal/)

**terminal-demo.seq** - Terminal colors, cursor control, and formatting using ANSI escape sequences.

## Operating System (os/)

**os-demo.seq** - Environment variables, paths, and system information.

## Text Processing (text/)

**log-parser.seq** - Parsing structured log files with string operations.

**regex-demo.seq** - Regular expression matching and extraction.

## Compression (compress-demo.seq)

Zstd compression and decompression for efficient data storage.

---

# Complete Projects

Larger applications demonstrating Seq's capabilities.

## Lisp Interpreter (lisp/)

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

## Hacker's Delight (hackers-delight/)

Bit manipulation algorithms from the book *Hacker's Delight*:

| File | Algorithm |
|------|-----------|
| `01-rightmost-bits.seq` | Isolate, clear, and propagate rightmost bits |
| `02-power-of-two.seq` | Check and round to powers of two |
| `03-counting-bits.seq` | Population count, leading/trailing zeros |
| `04-branchless.seq` | Branchless min, max, abs, sign |
| `05-swap-reverse.seq` | Bit reversal and byte swapping |

Demonstrates Seq's bitwise operations: `band`, `bor`, `bxor`, `shl`, `shr`, `popcount`, `clz`, `ctz`.

## Cryptography (crypto.seq)

Cryptographic operations including hashing and encoding.

## Shopping Cart (shopping-cart/)

A domain modeling example showing how to structure a typical business application with Seq.

---

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

---

## See Also

- [Language Guide](language-guide.md) - Core language concepts
- [Weaves Guide](WEAVES_GUIDE.md) - Generators and coroutines
- [Testing Guide](TESTING_GUIDE.md) - Writing and running tests
- [seqlings](https://github.com/navicore/seqlings) - Interactive exercises
