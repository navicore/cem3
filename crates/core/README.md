# seq-core

Core runtime primitives for stack-based concatenative languages.

This crate provides the language-agnostic foundation that can be shared across multiple stack-based languages (Seq, actor languages, etc.).

## Features

- **Value System**: Core `Value` enum supporting Int, Float, Bool, String, Symbol, Variant, Map, Quotation, Closure, Channel, and WeaveCtx
- **Stack Operations**: Efficient 40-byte tagged stack entries with LLVM-compatible layout
- **Memory Management**: Thread-local arena allocation for fast value creation
- **Channels**: CSP-style MPMC channels built on May green threads
- **Error Handling**: Thread-local FFI-safe error propagation
- **Serialization**: SON (Seq Object Notation) for value serialization

## Usage

```toml
[dependencies]
seq-core = "0.19"
```

```rust
use seq_core::{Value, Stack, push, pop, alloc_stack};
use seq_core::seqstring::global_string;

// Create a stack
let stack = alloc_stack();

// Push values
let stack = unsafe { push(stack, Value::Int(42)) };
let stack = unsafe { push(stack, Value::String(global_string("hello".to_string()))) };

// Pop values
let (stack, value) = unsafe { pop(stack) };
```

## Architecture

```
seq-core/
├── error.rs         # Thread-local FFI-safe error handling
├── memory_stats.rs  # Cross-thread memory statistics
├── arena.rs         # Thread-local bump allocation
├── seqstring.rs     # Arena/global string allocation
├── tagged_stack.rs  # 40-byte stack value layout
├── value.rs         # Core Value enum
├── stack.rs         # Stack operations and value conversion
└── son.rs           # Seq Object Notation serialization
```

## Building Other Languages

seq-core is designed to be the foundation for multiple languages:

- **Seq**: Stack-based concatenative language (uses seq-runtime which builds on seq-core)
- **Actor Languages**: Build actor primitives on top of channels and green threads

See [seq-actor](https://github.com/navicore/seq-actor) for an example of building actor primitives on seq-core.

## License

MIT
