# Seq Programming Language

Seq is a concatenative, stack-based programming language that compiles to native executables. It combines the elegance of stack-based programming with a sophisticated type system, guaranteed tail call optimization, and CSP-style concurrency.

```seq
: factorial ( Int -- Int )
  dup 1 i.<= if
    drop 1
  else
    dup 1 i.- factorial i.*
  then
;

: main ( -- ) 10 factorial int->string io.write-line ;
```

## Why Seq?

**Stack-based simplicity.** No variable declarations, no argument lists - values flow through the stack. Code reads left-to-right as a pipeline of transformations.

**Strongly typed with effect tracking.** Stack effects aren't just comments - they're enforced by the compiler. The type system tracks not only what goes on and off the stack, but also side effects like yielding from generators.

**Guaranteed tail call optimization.** Recursive functions run in constant stack space via LLVM's `musttail`. Write elegant recursive algorithms without stack overflow concerns.

**CSP-style concurrency.** Lightweight strands (green threads) communicate through channels. No shared memory, no locks - just message passing.

## Project Status

**Stable as of 1.1.0.** The language and standard library are stable and used by the creators for their own projects. That said, Seq is a niche experimental language - adopt it with eyes open. Future versions follow strict semantic versioning: major version increments indicate breaking changes.

## Quick Links

- **New to Seq?** Start with the [Glossary](GLOSSARY.md) for key concepts
- **Learn by doing:** Try [seqlings](https://github.com/navicore/seqlings) - hands-on exercises
- **Reference:** See the [Language Guide](language-guide.md) and [Standard Library](STDLIB_REFERENCE.md)
- **Source code:** [github.com/navicore/patch-seq](https://github.com/navicore/patch-seq)

## Installation

```bash
cargo install seq-compiler  # installs seqc (compiler)
cargo install seq-repl      # installs seqr (interactive REPL)
```

See the [main repository](https://github.com/navicore/patch-seq) for full installation instructions.
