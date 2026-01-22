# Examples

The `examples/` directory contains programs demonstrating Seq's features, from simple "hello world" to a complete Lisp interpreter. These examples are tested in CI and serve as both documentation and regression tests.

## Quick Reference

| Directory | What It Demonstrates |
|-----------|----------------------|
| `hello-world.seq` | Minimal program |
| `language-features/` | Core language concepts |
| `recursion/` | Tail-recursive algorithms |
| `csp/` | Actor model with HTTP interface |
| `weave/` | Generators and coroutines |
| `lisp/` | Complete Lisp interpreter |
| `http/` | TCP servers and routing |
| `json/` | JSON parsing |
| `yaml/` | YAML parsing |
| `hackers-delight/` | Bit manipulation puzzles |
| `ffi/` | Foreign function interface |

---

## Getting Started

### hello-world.seq

The simplest Seq program:

```seq
: main ( -- ) "Hello, World!" io.write-line ;
```

Build and run:
```bash
seqc build examples/hello-world.seq
./hello-world
```

---

## Language Features

The `language-features/` directory covers core concepts:

### stack-effects.seq
Stack effect declarations and how the type checker enforces them.

### quotations.seq
Anonymous code blocks (quotations) and calling them with `call`.

### closures.seq
Quotations that capture values from their environment.

### control-flow.seq
Conditionals (`if`/`else`/`then`), pattern matching, and the `cond` combinator.

### recursion.seq
Tail-recursive algorithms demonstrating guaranteed TCO.

---

## Concurrency

### csp/actor_counters.seq

A sophisticated CSP/Actor demonstration with 4-tier hierarchical aggregation:

```
Company (aggregate)
  └── Region (aggregate)
        └── District (aggregate)
              └── Store (counter)
```

Features:
- **Actor model**: Independent strands communicate via channels
- **HTTP interface**: RESTful API for queries and updates
- **Hierarchical aggregation**: Stores batch updates, parents aggregate
- **Request-response pattern**: Synchronous queries via response channels

Run it:
```bash
seqc build examples/csp/actor_counters.seq -o /tmp/actors
/tmp/actors &
curl http://localhost:8080/acme/west/seattle/0001
curl -X POST http://localhost:8080/acme/west/seattle/0001/increment
curl http://localhost:8080/acme  # Company total
```

### weave/counter.seq

Generator pattern using weaves:

```seq
: counter-loop ( Ctx Int -- | Yield Int )
  tuck yield rot i.add counter-loop
;
```

The counter yields its current value and receives an increment.

### weave/sensor-classifier.seq

Stream processing with structured data - a generator that classifies sensor readings.

---

## The Lisp Interpreter

The `lisp/` directory contains a complete Lisp interpreter in Seq:

| File | Purpose |
|------|---------|
| `sexpr.seq` | S-expression data types |
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

This example demonstrates:
- **Union types (ADTs)** for the AST
- **Pattern matching** for dispatch
- **Recursive descent** parsing
- **Environment passing** for lexical scope

---

## HTTP and Networking

### http/http_server.seq

A TCP server with HTTP routing:

```seq
include std:http

: handle-request ( TcpStream -- )
  tcp.read-request
  request-path "/" string.equal? if
    "Hello from Seq!" 200 make-response
  else
    "Not Found" 404 make-response
  then
  tcp.write-response
;
```

### http-client.seq

HTTP client requests using the std:http module.

---

## Data Formats

### json/json_tree.seq

Parse JSON and traverse the resulting tree:

```seq
include std:json

: main ( -- )
  "{\"name\": \"Alice\", \"age\": 30}" json.parse
  "name" json.get json.as-string io.write-line
;
```

### yaml/

YAML parsing with multiline strings and nested structures.

### son/serialize.seq

SON (Seq Object Notation) - Seq's native serialization format.

---

## Bit Manipulation

The `hackers-delight/` directory implements algorithms from *Hacker's Delight*:

| File | Algorithm |
|------|-----------|
| `01-rightmost-bits.seq` | Isolate, clear, and propagate rightmost bits |
| `02-power-of-two.seq` | Check and round to powers of two |
| `03-counting-bits.seq` | Population count, leading/trailing zeros |
| `04-branchless.seq` | Branchless min, max, abs, sign |
| `05-swap-reverse.seq` | Bit reversal and byte swapping |

These demonstrate Seq's bitwise operations (`band`, `bor`, `bxor`, `shl`, `shr`, `popcount`, `clz`, `ctz`).

---

## Foreign Function Interface

### ffi/libedit-demo.seq

Call into native libraries (libedit) for readline-style input.

### ffi/sqlite/

SQLite database access through FFI.

---

## Other Examples

### text/log-parser.seq
Log file parsing with string operations.

### text/regex-demo.seq
Regular expression matching.

### crypto.seq
Cryptographic operations (hashing, encoding).

### terminal/terminal-demo.seq
Terminal colors and cursor control.

### os/os-demo.seq
Environment variables, paths, and system info.

### io/compress-demo.seq
Compression and decompression.

---

## Running Examples

Most examples can be built and run directly:

```bash
# Build and run
seqc build examples/json/json_tree.seq
./json_tree

# Or compile to specific output
seqc build examples/lisp/test_eval.seq -o /tmp/lisp-test
/tmp/lisp-test
```

### Script Mode

For quick testing, use script mode to compile and run in one step:

```bash
seqc examples/hello-world.seq
```

Script mode uses `-O0` for fast compilation and caches binaries in `~/.cache/seq/`. Great for iteration during development.

### Shebang Scripts

Scripts with shebangs can run directly:

```bash
chmod +x tests/integration/src/script_mode.seq
./tests/integration/src/script_mode.seq
```

Some examples require the full standard library features:

```bash
# HTTP requires the http feature
seqc build examples/http/http_server.seq
```

---

## Learning Path

If you're new to Seq, we suggest this order:

1. **hello-world.seq** - Verify your setup
2. **language-features/stack-effects.seq** - Understand the type system
3. **language-features/control-flow.seq** - Conditionals and recursion
4. **language-features/quotations.seq** - First-class functions
5. **json/json_tree.seq** - Real-world data processing
6. **csp/actor_counters.seq** - Concurrency patterns
7. **lisp/** - A complete interpreter

For hands-on learning with exercises, see [seqlings](https://github.com/navicore/seqlings).

## See Also

- [Language Guide](language-guide.md) - Core language concepts
- [Weaves Guide](WEAVES_GUIDE.md) - Generators and coroutines
- [Testing Guide](TESTING_GUIDE.md) - Writing and running tests
