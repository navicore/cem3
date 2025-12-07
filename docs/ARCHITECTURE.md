# Seq Architecture

Seq is a concatenative (stack-based) programming language with static typing,
row polymorphism, and green-thread concurrency.

## Core Design Principles

1. **Values are independent of stack structure** - A value can be duplicated,
   shuffled, or stored without corruption. The stack is just a linked list of
   pointers to values.

2. **Functional style** - Operations produce new values rather than mutating.
   `array-with` returns a new array, it doesn't modify the original.

3. **Static typing with inference** - Stack effects are checked at compile time.
   Row polymorphism (`..rest`) allows generic stack-polymorphic functions.

4. **Concatenative composition** - Functions compose by juxtaposition.
   `f g` means "do f, then g". No explicit argument passing.

## Project Structure

```
seq/
├── compiler/           # Rust - seqc compiler
│   ├── src/
│   │   ├── ast.rs          # AST and builtin definitions
│   │   ├── parser.rs       # Forth-style parser
│   │   ├── typechecker.rs  # Row-polymorphic type inference
│   │   ├── builtins.rs     # Type signatures for builtins
│   │   ├── codegen.rs      # LLVM IR generation
│   │   └── unification.rs  # Type unification
│   └── tests/
├── runtime/            # Rust - libseq_runtime.a
│   ├── src/
│   │   ├── stack.rs        # Stack operations (push, pop, dup, swap, rot, etc.)
│   │   ├── value.rs        # Value types (Int, Float, String, Variant)
│   │   ├── arithmetic.rs   # Math operations
│   │   ├── comparison.rs   # Comparison operations
│   │   ├── strings.rs      # String operations
│   │   ├── variant_ops.rs  # Variant creation and field access
│   │   ├── io.rs           # I/O operations
│   │   ├── file.rs         # File operations
│   │   ├── args.rs         # Command-line argument access
│   │   ├── scheduler.rs    # May coroutine scheduler
│   │   ├── arena.rs        # Arena allocation for temporaries
│   │   └── pool.rs         # Stack node pooling
│   └── tests/
├── lsp/                # Rust - seq-lsp language server
│   └── src/
│       ├── main.rs         # LSP server entry point (tower-lsp)
│       └── diagnostics.rs  # Document analysis and error reporting
├── stdlib/             # Seq standard library
│   ├── json.seq            # JSON parsing and serialization
│   └── yaml.seq            # YAML parsing and serialization
├── examples/           # Example programs
│   └── json/
│       ├── json_tree.seq   # JSON viewer tool
│       └── README.md
└── docs/               # Documentation
```

## Value Types

Values are defined in `runtime/src/value.rs`:

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    String(SeqString),           // Reference-counted string
    Variant(Box<VariantData>),   // Tagged union with N fields
}

pub struct VariantData {
    pub tag: u32,
    pub fields: Box<[Value]>,
}
```

## Stack Model

The stack is a singly-linked list of nodes:

```rust
pub struct StackNode {
    pub value: Value,
    pub next: Stack,  // *mut StackNode
}
```

Key operations:
- `push(stack, value) -> stack'` - Add value to top
- `pop(stack) -> (stack', value)` - Remove and return top
- `dup`, `drop`, `swap`, `rot`, `over`, `pick`, `roll` - Stack shuffling

## Type System

### Stack Effects

Every function has a stack effect: `( input -- output )`

```seq
: add ( Int Int -- Int ) ... ;
: dup ( T -- T T ) ... ;
: swap ( A B -- B A ) ... ;
```

### Row Polymorphism

The `..rest` syntax captures "everything else on the stack":

```seq
: my-dup ( ..rest T -- ..rest T T )
  dup
;
```

This means `my-dup` works regardless of what's below the top value.

### Type Inference

Types are inferred at compile time. The type checker:
1. Assigns fresh type variables to unknowns
2. Collects constraints from operations
3. Unifies constraints to solve for types
4. Reports errors if unification fails

## Variants (Algebraic Data Types)

Variants are tagged unions with N fields:

```seq
# Create: field1 field2 ... fieldN count tag make-variant
42 "hello" 2 1 make-variant    # Tag 1 with fields [42, "hello"]

# Access
variant-tag           # ( Variant -- Int )
variant-field-count   # ( Variant -- Int )
0 variant-field-at    # ( Variant Int -- Value )

# Functional append
value variant-append  # ( Variant Value -- Variant' )
```

### JSON Tags

The JSON library uses these variant tags:
- Tag 0: JsonNull (0 fields)
- Tag 1: JsonBool (1 field: Int 0/1)
- Tag 2: JsonNumber (1 field: Float)
- Tag 3: JsonString (1 field: String)
- Tag 4: JsonArray (N fields: elements)
- Tag 5: JsonObject (2N fields: key1 val1 key2 val2 ...)

## Control Flow

### Conditionals

```seq
condition if
  # then-branch
else
  # else-branch
then
```

Conditions are integers: 0 = false, non-zero = true.

Both branches must have the same stack effect.

### Recursion

Words can call themselves:

```seq
: factorial ( Int -- Int )
  dup 1 <= if
    drop 1
  else
    dup 1 - factorial *
  then
;
```

Tail calls are optimized via LLVM's `musttail` - deep recursion won't overflow.
See `docs/TCO_DESIGN.md` for details.

## Concurrency (Strands)

Seq uses May coroutines for cooperative concurrency:

```seq
# Spawn a strand (green thread)
[ ... code ... ] spawn    # ( Quotation -- Int ) returns strand ID

# Channels for communication
make-channel              # ( -- Int ) returns channel ID
value channel-id send     # ( Value Int -- )
channel-id receive        # ( Int -- Value )

# Cooperative yield
yield                     # Let other strands run
```

**Note:** Current implementation has known issues with heavy concurrent workloads.

### Why May (Not Tokio)

Seq uses the `may` crate for stackful coroutines (fibers) rather than Rust's
async/await ecosystem (Tokio, async-std). Key reasons:

1. **No async coloring** - With may, a Seq `spawn` creates a fiber that can
   call blocking operations (channel send/receive, I/O) and implicitly yield.
   No `async`/`await` syntax pollution spreading through the call stack.

2. **Erlang/Go mental model** - Fits Seq's concatenative style naturally.
   `[ code ] spawn` creates a lightweight fiber. Thousands can run concurrently
   with message passing via channels. This matches how Go goroutines and Erlang
   processes work - simple synchronous-looking code that yields cooperatively.

3. **Simpler FFI** - LLVM-generated code calls synchronous Rust functions.
   No async runtime ceremony or `Future` plumbing required.

4. **M:N scheduling** - Like Tokio, may multiplexes many fibers across a small
   thread pool. We get lightweight concurrency without one OS thread per fiber.

### M:N Threading: Best of Both Worlds

Early concurrency implementations had to choose between two models:

| Model | Mapping | Pros | Cons |
|-------|---------|------|------|
| Green threads (early Java) | M:1 | Cheap, fast switch | Single CPU only |
| Native OS threads | 1:1 | Multi-CPU | Expensive (~1MB stack), slow switch |

May provides **M:N scheduling** - many lightweight coroutines distributed across
all CPU cores:

- **Lightweight** - Strands use ~4KB stack (grows as needed), not 1MB
- **Multi-core** - Work-stealing scheduler spreads load across all CPUs
- **Fast context switch** - Cooperative yield, no kernel involvement
- **No blocking** - When one strand waits on I/O, others run on that core

This means Seq programs get the programming simplicity of green threads (spawn
thousands of concurrent tasks cheaply) with the performance of native threads
(utilizing all available CPUs). Write sequential code that scales.

### Tradeoff: libc for stdout

May's implicit yields can occur inside any function call. Rust's `stdout()`
uses an internal `RefCell` that panics if one coroutine holds a borrow, yields,
and another coroutine on the same OS thread tries to borrow. This is because
`RefCell` tracks borrows per-thread, not per-coroutine.

We bypass this by calling `libc::write(1, ...)` directly, protected by
`may::sync::Mutex` (which yields the coroutine when contended rather than
blocking the OS thread). This is a small price for may's cleaner programming
model.

See `runtime/src/io.rs` for the implementation.

## Memory Management

Seq's stack-based execution creates unique memory allocation patterns that
don't fit well with general-purpose allocators. Every `push` allocates a node,
every `pop` frees one. Standard malloc/free would dominate execution time.

### Stack Node Pooling

**Problem:** A tight loop doing `dup drop` thousands of times would spend more
time in malloc/free than doing actual work.

**Solution:** Thread-local free list of pre-allocated `StackNode`s.

- Fast path (~10ns): Pop node from free list, return node to free list
- Slow path (~100ns): Fall back to malloc/free when pool exhausted
- Bounded size (1024 nodes max) prevents unbounded memory growth
- Pre-allocate 256 nodes on first use to amortize startup cost

See `runtime/src/pool.rs` for implementation.

### Arena Allocation

**Problem:** String operations (concatenation, substring, parsing) create many
short-lived intermediate strings. Reference counting each one adds overhead.

**Solution:** Thread-local bump allocator (via `bumpalo` crate).

- Allocation is a pointer bump (~5ns vs ~100ns for malloc)
- No individual deallocation - entire arena reset at once
- Reset when strand exits or when arena exceeds 10MB threshold
- 20x faster than global allocator for allocation-heavy workloads

**Thread-local vs strand-local:** The arena is per-OS-thread, not per-strand.
If may migrates a strand between threads (rare), some memory stays in the old
arena until another strand on that thread exits. This is acceptable - the
common case (strand stays on one thread) is fast, and the 10MB auto-reset
prevents unbounded growth in the rare migration case.

See `runtime/src/arena.rs` for implementation.

### Reference Counting

`SeqString` uses atomic reference counting for strings that escape the arena:

- Strings passed through channels are cloned to the global allocator
- Strings stored in closures use reference counting
- Arena strings are fast for local computation; refcounted strings are safe
  for sharing across strands

This hybrid approach gives us arena speed for the common case (local string
manipulation) and correctness for cross-strand communication.

### LTO Investigation

We investigated Link-Time Optimization to inline runtime functions into
Seq-generated code. While technically possible (requires matching LLVM versions
and function attributes), it doesn't help performance because:

- Pool allocation logic is complex and cannot be simplified by inlining
- LLVM cannot fold constants across stack operations (`1 2 add` cannot become `3`)
- Aggressive inlining actually increases code size and register pressure

The current design with pooled allocation and separate runtime is appropriate.
See `docs/LTO_INVESTIGATION.md` for the full analysis.

## Compilation Pipeline

1. **Parse** - Tokenize and build AST (`parser.rs`)
2. **Type Check** - Infer and verify stack effects (`typechecker.rs`)
3. **Codegen** - Emit LLVM IR (`codegen.rs`)
4. **Link** - LLVM compiles IR, links with `libseq_runtime.a`

```bash
# Compile a .seq file
./target/release/seqc --output myprogram myprogram.seq

# Keep IR for inspection
./target/release/seqc --output myprogram myprogram.seq --keep-ir
cat myprogram.ll
```

## Standard Library

### Include System

```seq
include std:json    # Loads stdlib/json.seq
include foo         # Loads ./foo.seq
```

### JSON (`stdlib/json.seq`)

Parsing:
```seq
"[1, 2, 3]" json-parse    # ( String -- JsonValue Int )
```

Serialization:
```seq
json-value json-serialize  # ( JsonValue -- String )
```

Functional builders:
```seq
json-empty-array 1 json-number array-with 2 json-number array-with
# Result: [1, 2]

json-empty-object "name" json-string "John" json-string obj-with
# Result: {"name": "John"}
```

## Current Limitations

1. **No loop keywords** - Use recursion (with TCO) or combinators (`times`, `while`, `until`)
2. **Serialization size limits** - Arrays > 3 elements, objects > 2 pairs show as `[...]`/`{...}`
3. **No string escapes** - `\"` not supported in strings
4. **roll type checking** - `3 roll` works at runtime but type checker can't fully verify

## Building

```bash
cargo build --release
cargo test --all
cargo clippy --all
```

## Running Programs

```bash
# Compile and run
./target/release/seqc --output /tmp/prog myprogram.seq
/tmp/prog

# With arguments
/tmp/prog arg1 arg2
```
