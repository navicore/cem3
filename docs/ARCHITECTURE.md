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
├── stdlib/             # Seq standard library
│   └── json.seq            # JSON parsing and serialization
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

**Note:** No tail-call optimization yet. Deep recursion will overflow.

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

## Memory Management

### Stack Node Pooling

Stack nodes are recycled via a thread-local pool, avoiding malloc/free overhead
for the hot path of push/pop operations.

### Arena Allocation

Temporary strings (from parsing, concatenation) use per-strand arenas that are
bulk-freed when the strand exits.

### Reference Counting

`SeqString` uses atomic reference counting for shared strings.

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

1. **No loops** - Use recursion (but no TCO)
2. **No closures** - Quotations can't capture variables
3. **Serialization size limits** - Arrays > 3 elements, objects > 2 pairs show as `[...]`/`{...}`
4. **No string escapes** - `\"` not supported in strings
5. **roll type checking** - `3 roll` works at runtime but type checker can't fully verify

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
