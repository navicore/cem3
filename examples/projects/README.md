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
