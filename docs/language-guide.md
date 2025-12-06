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
produces one integer. The compiler verifies stack effects at compile time.

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
| Int | `42`, `-1`, `0xFF`, `0b1010` | 64-bit signed, hex/binary literals |
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

Since a word is just a named sequence of operations, any contiguous sequence
can be extracted into a new word without changing meaning:

```seq
# Given words a, b, c, d in sequence:
a b c d

# Define a new word for "b c":
: bc  b c ;

# This is equivalent:
a bc d
```

A concrete example:

```seq
# Four words in sequence
read parse transform write

# Extract middle two into a word
: process  parse transform ;
read process write          # Same behavior
```

## Comments

Comments start with `#` and continue to end of line:

```seq
# Whole-line comment

5 square  # Inline comment after code
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

## Variants (Sum Types)

Variants are tagged unions - the primary way to build data structures:

```seq
# Create a variant with tag 1 and one field
42 1 make-variant-1     # (Tag1 42)

# Create with two fields
"key" 100 2 make-variant-2   # (Tag2 "key" 100)

# Inspect variants
variant-tag             # Get the tag number
0 variant-field-at      # Get field 0
1 variant-field-at      # Get field 1
```

### Building ADTs

Define constructors and accessors for your types:

```seq
# Option type: None (tag 0) or Some (tag 1)
: none ( -- Variant )  0 make-variant-0 ;
: some ( a -- Variant )  1 make-variant-1 ;
: none? ( Variant -- Int )  variant-tag 0 = ;
: some? ( Variant -- Int )  variant-tag 1 = ;
: unwrap ( Variant -- a )  0 variant-field-at ;

# Usage
42 some        # Create Some(42)
dup some? if
    unwrap    # Get the 42
then
```

### Cons Lists

The standard pattern for lists:

```seq
: nil ( -- Variant )  0 make-variant-0 ;
: cons ( head tail -- Variant )  1 make-variant-2 ;
: nil? ( Variant -- Int )  variant-tag 0 = ;
: car ( Variant -- head )  0 variant-field-at ;
: cdr ( Variant -- tail )  1 variant-field-at ;

# Build a list: (1 2 3)
1  2  3 nil cons cons cons
```

### State as Variant

When you need to thread multiple values through recursion, pack them:

```seq
# Tokenizer state: (Input Position CurrentToken TokenList)
: make-state ( String Int String Variant -- Variant )
    100 make-variant-4 ;

: state-input ( Variant -- String )  0 variant-field-at ;
: state-pos ( Variant -- Int )  1 variant-field-at ;

# Initialize and loop
"input" 0 "" nil make-state
process-loop
```

## String Operations

| Word | Effect | Description |
|------|--------|-------------|
| `string-concat` | `( a b -- ab )` | Concatenate |
| `string-length` | `( s -- Int )` | Character count |
| `string-empty` | `( s -- Int )` | True if empty |
| `string-equal` | `( a b -- Int )` | Compare |
| `string-char-at` | `( s i -- Int )` | Char code at index |
| `string-substring` | `( s start len -- s )` | Extract substring |
| `string-split` | `( s delim -- Variant )` | Split into list |
| `string-chomp` | `( s -- s )` | Remove trailing newline |
| `string-trim` | `( s -- s )` | Remove whitespace |
| `string->int` | `( s -- Int )` | Parse integer |
| `int->string` | `( Int -- s )` | Format integer |

## Bitwise Operations

For low-level bit manipulation:

| Word | Effect | Description |
|------|--------|-------------|
| `band` | `( Int Int -- Int )` | Bitwise AND |
| `bor` | `( Int Int -- Int )` | Bitwise OR |
| `bxor` | `( Int Int -- Int )` | Bitwise XOR |
| `bnot` | `( Int -- Int )` | Bitwise NOT (one's complement) |
| `shl` | `( Int Int -- Int )` | Shift left |
| `shr` | `( Int Int -- Int )` | Logical shift right (zero-fill) |
| `popcount` | `( Int -- Int )` | Count 1-bits |
| `clz` | `( Int -- Int )` | Count leading zeros |
| `ctz` | `( Int -- Int )` | Count trailing zeros |
| `int-bits` | `( -- Int )` | Push 64 (bit width of Int) |

### Numeric Literals

```seq
42          # Decimal
0xFF        # Hexadecimal (case insensitive: 0xff, 0XFF)
0b1010      # Binary (case insensitive: 0B1010)
```

### Shift Behavior

- Shift by 0 returns the original value
- Shift by 63 is the maximum valid shift
- Shift by 64 or more returns 0
- Shift by negative amount returns 0
- Right shift is *logical* (zero-fill), not arithmetic (sign-extending)

```seq
1 63 shl    # -9223372036854775808 (i64::MIN, high bit set)
-1 1 shr    # 9223372036854775807 (i64::MAX, logical shift fills with 0)
```

## Recursion and Tail Call Optimization

Seq has no loop keywords. Iteration is recursion:

```seq
# Count down
: countdown ( Int -- )
    dup 0 > if
        dup int->string write_line
        1 - countdown
    else
        drop
    then
;

# Process a list
: sum-list ( Variant -- Int )
    dup nil? if
        drop 0
    else
        dup car swap cdr sum-list +
    then
;
```

### Guaranteed Tail Call Optimization

Seq guarantees TCO via LLVM's `musttail` calling convention. Deeply recursive code
won't overflow the stack - you can recurse millions of times safely.

More importantly, Seq's TCO is **branch-aware**. The compiler recognizes tail
position *within* each branch of a conditional, not just at word level. This means
you can write natural recursive code without restructuring for optimization:

```seq
: process-input ( -- )
    read_line+ if
        string-chomp
        process-line
        process-input   # Tail call - even inside a branch
    else
        drop
    then
;
```

In many languages, you'd have to "game" the compiler - inverting conditions,
using continuation-passing style, or adding explicit trampolines to get TCO.
In Seq, the compiler does this analysis for you. Write readable code; get
optimization automatically.

### When TCO Applies

TCO works for user-defined word calls in tail position. It does *not* apply in:

- **main** - entry point uses C calling convention
- **Quotations** `[ ... ]` - use C convention for interop
- **Closures** - signature differs due to captured environment

For hot loops that need guaranteed TCO, use a named word rather than a quotation:

```seq
# TCO works here
: loop ( Int -- )
    dup 0 > if
        1 - loop
    else
        drop
    then
;

# TCO does NOT work here (quotation)
[ dup 0 > ] [ 1 - ] while
```

The `while`, `until`, and `times` combinators handle their own iteration
internally, so this distinction rarely matters in practice.

## Command Line Programs

```seq
: main ( -- )
    arg-count 1 > if
        1 arg              # First argument (0 is program name)
        process-file
    else
        "Usage: prog <file>" write_line
    then
;
```

| Word | Effect | Description |
|------|--------|-------------|
| `arg-count` | `( -- Int )` | Number of arguments |
| `arg` | `( Int -- String )` | Get argument by index |

## File Operations

| Word | Effect | Description |
|------|--------|-------------|
| `file-slurp` | `( path -- String )` | Read entire file |
| `file-exists?` | `( path -- Int )` | Check if file exists |

## Modules

Split code across files with `include`:

```seq
# main.seq
include "parser"
include "eval"

: main ( -- )
    # parser.seq and eval.seq words available here
;
```

The include path is relative to the including file.

## Naming Conventions

| Suffix | Meaning | Example |
|--------|---------|---------|
| `?` | Predicate (returns boolean) | `nil?`, `empty?`, `file-exists?` |
| `+` | Returns result + status | `read_line+`, `map-get-safe` |
| `->` | Conversion | `int->string`, `string->int` |

## Maps

Key-value dictionaries with O(1) lookup:

```seq
make-map                    # ( -- Map )
"name" "Alice" map-set      # ( Map -- Map )
"age" 30 map-set
"name" map-get              # ( Map key -- Map value )
"name" map-has?             # ( Map key -- Map Int )
map-keys                    # ( Map -- Variant ) list of keys
```

## Higher-Order Words

```seq
# Map over a list
my-list [ 2 * ] list-map

# Filter a list
my-list [ 0 > ] list-filter

# Fold (reduce)
my-list 0 [ + ] list-fold

# Execute N times
10 [ "hello" write_line ] times

# Loop while condition true
[ condition ] [ body ] while
```

---

*Seq: where composition is not just a pattern, but the foundation.*
