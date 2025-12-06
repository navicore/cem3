# Seq Language Guide

A concatenative language where composition is the fundamental operation.

## Why Concatenative?

If you've written Rust like this:

```rust
data.iter()
    .map(transform)
    .filter(predicate)
    .fold(init, combine)
```

You've already experienced the appeal of concatenative thinking: data flows
through a pipeline, each step consuming its input and producing output for the
next. No intermediate variables, no naming - just composition.

Seq takes this idea to its logical conclusion. Where Rust uses method chaining
as syntactic sugar over function application, Seq makes composition the *only*
mechanism:

```seq
data [ transform ] list-map [ predicate ] list-filter init [ combine ] list-fold
```

The connection runs deeper than syntax. Rust's `FnOnce` trait means "callable
once, consumes self." Seq's stack semantics mean "pop consumes the value." Both
enforce *linear* dataflow - resources used exactly once. Rust tracks this in the
type system; Seq tracks it through the stack.

## The Stack

Everything in Seq operates on an implicit stack. Literals push values; words
consume and produce values:

```seq
1 2 add    # Push 1, push 2, add consumes both, pushes 3
```

The stack replaces variables. Instead of:

```
let x = 1
let y = 2
let z = x + y
```

You write:

```seq
1 2 add
```

The stack *is* your working memory.

## Words

Words are the building blocks. A word is a named sequence of operations:

```seq
: square ( Int -- Int )
  dup multiply
;
```

The `( Int -- Int )` is the *stack effect* - this word consumes one integer and
produces one integer. Stack effects are documentation and (eventually) type
checking.

Calling a word is just writing its name:

```seq
5 square    # Result: 25
```

## Quotations

Quotations are deferred code - blocks that can be passed around and executed later:

```seq
[ 2 multiply ]    # Pushes a quotation onto the stack
```

Quotations enable higher-order programming:

```seq
5 [ 2 multiply ] call    # Result: 10
```

They're essential for combinators like `list-map`, `list-filter`, and control flow.

## Control Flow

Conditionals use stack-based syntax:

```seq
condition if
  then-branch
else
  else-branch
then
```

The condition is popped from the stack. Non-zero means true:

```seq
: abs ( Int -- Int )
  dup 0 < if
    -1 multiply
  then
;
```

## Values and Types

Seq has these value types:

| Type | Examples | Notes |
|------|----------|-------|
| Int | `42`, `-1`, `0` | 64-bit signed |
| Float | `3.14`, `-0.5` | 64-bit IEEE 754 |
| Bool | `true`, `false` | |
| String | `"hello"` | UTF-8 |
| List | (via variant ops) | Ordered collection |
| Map | (via map ops) | Key-value dictionary |
| Quotation | `[ code ]` | Deferred execution |

## Stack Operations

The fundamental stack manipulators:

| Word | Effect | Description |
|------|--------|-------------|
| `dup` | `( a -- a a )` | Duplicate top |
| `drop` | `( a -- )` | Discard top |
| `swap` | `( a b -- b a )` | Exchange top two |
| `over` | `( a b -- a b a )` | Copy second to top |
| `rot` | `( a b c -- b c a )` | Rotate third to top |
| `nip` | `( a b -- b )` | Drop second |
| `tuck` | `( a b -- b a b )` | Copy top below second |

Master these and you can express any data flow without variables.

## Composition

The key insight: in Seq, *juxtaposition is composition*.

```seq
: double  2 multiply ;
: square  dup multiply ;
: quad    double double ;    # Composition by juxtaposition
```

Writing `double double` doesn't "call double twice" in the applicative sense -
it *composes* two doublings into a single operation.

This is why concatenative code can be refactored so freely:

```seq
# These are equivalent:
a b c
a  b c      # Extract "b c" as a word
a bc        # Same meaning, different factoring
```

## Comments

```seq
# Line comments start with hash

# Stack effects in word definitions:
: word-name ( inputs -- outputs )
  body
;
```

## I/O Operations

Basic console I/O:

| Word | Effect | Description |
|------|--------|-------------|
| `write_line` | `( String -- )` | Print string to stdout with newline |
| `read_line` | `( -- String )` | Read line from stdin (includes newline) |
| `read_line+` | `( -- String Int )` | Read line with EOF detection |

### Handling EOF with read_line+

The `read_line` word panics at EOF, which is fine for simple scripts. For robust input handling, use `read_line+` which returns a status flag:

```seq
read_line+    # ( -- String Int )
              # Success: ( "line\n" 1 )
              # EOF:     ( "" 0 )
```

Example - reading all lines until EOF:

```seq
: process-input ( -- )
    read_line+ if
        string-chomp    # Remove trailing newline
        process-line    # Your processing word
        process-input   # Recurse for next line
    else
        drop            # Drop empty string at EOF
    then
;
```

The `+` suffix convention indicates words that return a result pattern (value + status) instead of panicking on failure.

## What's Next

This guide will grow to cover:
- List operations (`list-map`, `list-filter`, `list-fold`)
- Map operations (`make-map`, `map-get`, `map-set`)
- String operations
- Concurrency with channels and `spawn`
- Error handling patterns
- Idiomatic Seq style

---

*Seq: where composition is not just a pattern, but the foundation.*
