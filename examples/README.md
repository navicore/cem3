# Examples

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
