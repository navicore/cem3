# Seq Architecture

Seq is a concatenative (stack-based) programming language with static typing,
row polymorphism, and green-thread concurrency.

## Core Design Principles

1. **Values are independent of stack structure** - A value can be duplicated,
   shuffled, or stored without corruption. The stack is a contiguous array of
   40-byte tagged values.

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
│   │   └── tagged_stack.rs # Contiguous 40-byte tagged value stack
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
#[repr(C)]
pub enum Value {
    Int(i64),                    // Discriminant 0
    Float(f64),                  // Discriminant 1
    Bool(bool),                  // Discriminant 2
    String(SeqString),           // Discriminant 3 - reference-counted
    Variant(Arc<VariantData>),   // Discriminant 4 - Arc for O(1) cloning
    Map(Box<HashMap<...>>),      // Discriminant 5 - key-value dictionary
    Quotation { wrapper, impl_ },// Discriminant 6 - function pointers
    Closure { fn_ptr, env },     // Discriminant 7 - function + captured values
}

pub struct VariantData {
    pub tag: u32,
    pub fields: Box<[Value]>,
}
```

Value is exactly 40 bytes with 8-byte alignment, matching the stack entry size.

## Stack Model

The stack is a contiguous array of 40-byte tagged values (`StackValue`):

```rust
#[repr(C)]
pub struct StackValue {
    pub slot0: u64,  // Discriminant (type tag)
    pub slot1: u64,  // Payload slot 1
    pub slot2: u64,  // Payload slot 2
    pub slot3: u64,  // Payload slot 3
    pub slot4: u64,  // Payload slot 4
}

pub struct TaggedStack {
    base: *mut StackValue,  // Heap-allocated array
    sp: usize,              // Stack pointer (next free slot)
    capacity: usize,        // Current allocation size
}
```

This design enables:
- **Inline LLVM IR operations** - Integer arithmetic, comparisons, and boolean ops
  execute directly in generated code without FFI calls
- **Cache-friendly layout** - Contiguous memory access patterns
- **O(1) stack operations** - No linked-list traversal or allocation per push/pop

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
# Create using typed constructors (0-4 fields)
42 "hello" 1 make-variant-2    # Tag 1 with fields [42, "hello"]
5 make-variant-0               # Tag 5 with no fields

# Access
variant-tag           # ( Variant -- Int )
variant-field-count   # ( Variant -- Int )
0 variant-field-at    # ( Variant Int -- Value )

# Functional append (for building dynamic collections)
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

### Runtime Configuration

The scheduler can be tuned via environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `SEQ_STACK_SIZE` | 131072 (128KB) | Coroutine stack size in bytes |
| `SEQ_POOL_CAPACITY` | 10000 | Cached coroutine pool size |
| `SEQ_WATCHDOG_SECS` | 0 (disabled) | Threshold for "stuck strand" detection |
| `SEQ_WATCHDOG_INTERVAL` | 5 | Watchdog check frequency (seconds) |
| `SEQ_WATCHDOG_ACTION` | warn | Action on stuck strand: `warn` or `exit` |

### Diagnostics Feature

The runtime includes optional diagnostics for production debugging:

- **Strand registry** - Tracks active strands with spawn timestamps
- **SIGQUIT handler** - Dumps runtime stats on `kill -3 <pid>`
- **Watchdog** - Detects strands running longer than threshold

These are controlled by the `diagnostics` Cargo feature (enabled by default):

```toml
# In Cargo.toml - disable for minimal overhead
seq-runtime = { version = "...", default-features = false }
```

When disabled, the runtime skips strand registry operations and signal handler
setup, eliminating ~O(1024) scans and `SystemTime::now()` syscalls per spawn.

**Note:** Benchmarking shows the diagnostics overhead is negligible compared to
May's coroutine spawn syscalls. The feature is primarily useful for production
deployments where `kill -3` debugging is needed.

## Memory Management

The tagged stack design eliminates per-operation allocation overhead. The stack
is a single contiguous array that grows/shrinks by adjusting the stack pointer.
Heap types (String, Variant, Closure) use reference counting for correct cleanup.

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

### Inline LLVM IR vs FFI

The tagged stack design enables inline code generation for performance-critical
operations. Integer arithmetic, comparisons, and boolean operations execute
directly in generated LLVM IR without FFI calls to the runtime:

```llvm
; Example: inline integer add
%a = load i64, ptr %slot1_ptr
%b = load i64, ptr %slot1_ptr.1
%result = add i64 %a, %b
store i64 %result, ptr %slot1_ptr
```

Complex operations (string handling, variants, closures) still call into the
Rust runtime for memory safety and code maintainability.

## Compilation Pipeline

1. **Parse** - Tokenize and build AST (`parser.rs`)
2. **Type Check** - Infer and verify stack effects (`typechecker.rs`)
3. **Codegen** - Emit LLVM IR (`codegen.rs`)
4. **Link** - LLVM compiles IR, links with `libseq_runtime.a`

```bash
# Compile a .seq file
./target/release/seqc build myprogram.seq -o myprogram

# Keep IR for inspection
./target/release/seqc build myprogram.seq -o myprogram --keep-ir
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
./target/release/seqc build myprogram.seq -o /tmp/prog
/tmp/prog

# With arguments
/tmp/prog arg1 arg2
```
