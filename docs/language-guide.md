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
data [ transform ] list.map [ predicate ] list.filter init [ combine ] list.fold
```

The connection runs deeper than syntax. Rust's `FnOnce` trait means "callable
once, consumes self." Seq's stack semantics mean "pop consumes the value." Both
enforce *linear* dataflow - resources used exactly once. Rust tracks this in the
type system; Seq tracks it through the stack.

## Language Heritage

Seq belongs to the concatenative language family. If you know Forth or Factor,
you'll feel at home:

| Feature | Forth | Factor | Seq |
|---------|-------|--------|-----|
| Word definition | `: name ... ;` | `:: name ( ) ... ;` | `: name ( ) ... ;` |
| Stack effects | `( a -- b )` comment | `( a -- b )` checked | `( a -- b )` checked |
| Quotations | `' word execute` | `[ ... ]` | `[ ... ]` |
| Conditionals | `if else then` | `if else then` | `if else then` |

**Syntactically**, Seq is ~80% Forth, ~15% Factor - a Forth programmer reads
Seq immediately; a Factor programmer feels at home with the quotations and
type annotations.

**Semantically**, Seq is novel:

- **Row-polymorphic type system** - Forth is untyped; Factor has optional
  inference. Seq statically verifies stack effects with full type checking.

- **CSP concurrency** - Neither Forth nor Factor has built-in green threads
  with channels. Seq's `spawn`, `send`, and `receive` enable actor-style
  concurrency.

- **LLVM compilation** - Seq compiles to native binaries via LLVM, not
  threaded code or a VM.

Seq wears familiar Forth clothes while offering modern type safety and
concurrency. It's a new language built on proven concatenative foundations.

## The Stack

Everything in Seq operates on an implicit stack. Literals push values; words
consume and produce values:

```seq
1 2 i.+    # Push 1, push 2, add consumes both, pushes 3
```

The stack replaces variables. Instead of:

```
let x = 1
let y = 2
let z = x + y
```

You write:

```seq
1 2 i.+
```

The stack *is* your working memory.

## Words

Words are the building blocks. A word is a named sequence of operations:

```seq
: square ( Int -- Int )
  dup i.*
;
```

The `( Int -- Int )` is the *stack effect* - this word consumes one integer and
produces one integer. Stack effects are **required** on all word definitions - the compiler verifies that the body matches the declared effect.

Calling a word is just writing its name:

```seq
5 square    # Result: 25
```

## Quotations

Quotations are deferred code - blocks that can be passed around and executed later:

```seq
[ 2 i.* ]    # Pushes a quotation onto the stack
```

Quotations enable higher-order programming:

```seq
5 [ 2 i.* ] call    # Result: 10
```

They're essential for combinators like `list.map`, `list.filter`, and control flow.

## Control Flow

Conditionals use stack-based syntax:

```seq
condition if
  then-branch
else
  else-branch
then
```

The condition is popped from the stack and must be a `Bool` (produced by comparisons, `true`/`false` literals, or logical operations):

```seq
: abs ( Int -- Int )
  dup 0 i.< if
    0 swap i.-    # negate: 0 - n
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

### Numeric Literals

Integers can be written in decimal, hexadecimal, or binary:

```seq
42          # Int (decimal)
-123        # Int (negative)
0xFF        # Int (hexadecimal, case insensitive: 0xff, 0XFF)
0b1010      # Int (binary, case insensitive: 0B1010)
```

Floats use decimal notation with a decimal point:

```seq
3.14        # Float
-0.5        # Float (negative)
```

## Stack Operations

The fundamental stack manipulators:

| Word | Effect | Description |
|------|--------|-------------|
| `dup` | `( ..a T -- ..a T T )` | Duplicate top |
| `drop` | `( ..a T -- ..a )` | Discard top |
| `swap` | `( ..a T U -- ..a U T )` | Exchange top two |
| `over` | `( ..a T U -- ..a T U T )` | Copy second to top |
| `rot` | `( ..a T U V -- ..a U V T )` | Rotate third to top |
| `nip` | `( ..a T U -- ..a U )` | Drop second |
| `tuck` | `( ..a T U -- ..a U T U )` | Copy top below second |

Master these and you can express any data flow without variables.

## Composition

The key insight: in Seq, *juxtaposition is composition*.

```seq
: double  2 i.* ;
: square  dup i.* ;
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
| `io.write-line` | `( String -- )` | Print string to stdout with newline |
| `io.read-line` | `( -- String Bool )` | Read line from stdin with success flag |

### Line Ending Normalization

All line-reading operations (`io.read-line`, `file.for-each-line+`)
normalize line endings to `\n`. Windows-style `\r\n` is converted to `\n`.
This ensures Seq programs behave consistently across operating systems.

### Handling EOF with io.read-line

The `io.read-line` word returns a success flag, making EOF handling explicit:

```seq
io.read-line    # ( -- String Bool )
                # Success: ( "line\n" true )
                # EOF:     ( "" false )
```

Example - reading all lines until EOF:

```seq
: process-input ( -- )
    io.read-line if
        string.chomp    # Remove trailing newline
        process-line    # Your processing word
        process-input   # Recurse for next line
    else
        drop            # Drop empty string at EOF
    then
;
```

## Algebraic Data Types (ADTs)

Seq provides compile-time safe algebraic data types with `union` definitions and `match` expressions.

Seq's `union` is similar to Rust's `enum` - each variant can carry multiple named fields. This differs from C++'s `std::variant`, where each alternative holds only a single type.

| Feature | C++ `std::variant` | Rust `enum` | Seq `union` |
|---------|-------------------|-------------|-------------|
| Multiple fields per variant | No (single type) | Yes | Yes (max 12) |
| Named fields | No | Yes | Yes |
| Exhaustive matching | `std::visit` | `match` | `match` |

### Union Definitions

Define sum types with typed fields:

```seq
union Option { Some { value: Int }, None }

union Message {
  Get { response-chan: Int }
  Increment { amount: Int }
  Report { op: Int, delta: Int, total: Int }
}
```

The compiler automatically generates typed constructors:
- `Make-Some: ( Int -- Option )`
- `Make-None: ( -- Option )`
- `Make-Get: ( Int -- Message )`
- `Make-Report: ( Int Int Int -- Message )`

### Compile-Time Safety

The compiler catches common errors:

**Field type validation** - Only valid types allowed:
```seq
union Bad { Foo { x: Unknown } }  # Error: Unknown type 'Unknown'
```
Valid field types: `Int`, `Float`, `Bool`, `String`, or another defined union.

**Variant arity limit** - Maximum 12 fields per variant:
```seq
union TooBig { V { a: Int, b: Int, c: Int, d: Int, e: Int, f: Int,
                   g: Int, h: Int, i: Int, j: Int, k: Int, l: Int, m: Int } }
# Error: Variant 'V' has 13 fields, maximum is 12.
# Consider using a Map or grouping fields into nested union types.
```

### Pattern Matching

Use `match` to destructure variants. The compiler requires **exhaustive** matching:

```seq
: describe ( Option -- String )
  match
    Some { >value } -> drop "has value"   # drop the extracted value
    None -> "empty"
  end
;
```

Non-exhaustive matches are compile errors:
```seq
: bad ( Option -- String )
  match
    Some -> "has value"
    # Error: Non-exhaustive match on 'Option'. Missing variants: None
  end
;
```

### Stack-Based Matching

All fields are pushed to stack in declaration order:

```seq
: handle ( Message -- )
  match
    Get ->              # ( response-chan )
      send-response
    Increment ->        # ( amount )
      do-increment
    Report ->           # ( op delta total )
      drop nip          # extract delta
      process
  end
;
```

### Named Bindings

Request specific fields by name using `>` prefix (indicating stack extraction, not variable binding):

```seq
: handle ( Message -- )
  match
    Get { >response-chan } ->
      # response-chan is now on stack
      send-response
    Increment { >amount } ->
      # amount is now on stack
      do-increment
    Report { >delta } ->     # only 'delta' pushed to stack
      process
  end
;
```

The `>` prefix makes clear these are stack extractions, not local variables. Both styles compile to identical code. Mix them freely.

### ADTs with Row Polymorphism

ADTs and row polymorphism are orthogonal:

```seq
union Option { Some { value: Int }, None }

# Row polymorphic - extra stack values pass through
: unwrap-or ( ..a Option Int -- ..a Int )
  swap match                    # swap so Option is on top for match
    Some { >value } -> nip      # remove default, keep extracted value
    None ->                     # keep default
  end
;

"hello" 42 Make-Some 0 unwrap-or   # ( "hello" 42 )
```

## Low-Level Variants

For dynamic use cases, low-level primitives create tagged values at runtime. A variant is a value with a **symbol tag** (like `:Some` or `:Nil`) and zero or more **fields**.

### Creating Variants

The `variant.make-N` words take N values from the stack plus a symbol tag:

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `variant.make-0` | `( Symbol -- Variant )` | Tag only, no fields |
| `variant.make-1` | `( T Symbol -- Variant )` | One field + tag |
| `variant.make-2` | `( T U Symbol -- Variant )` | Two fields + tag |

The tag is always the **last** argument (top of stack):

```seq
:None variant.make-0              # Creates: (None)
42 :Some variant.make-1           # Creates: (Some 42)
"x" 10 :Point variant.make-2      # Creates: (Point "x" 10)
```

### Inspecting Variants

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `variant.tag` | `( Variant -- Symbol )` | Get the tag symbol |
| `variant.field-at` | `( Variant Int -- T )` | Get field by index (0-based) |

```seq
"x" 10 :Point variant.make-2    # ( Point )
dup variant.tag                 # ( Point :Point )
drop 0 variant.field-at         # ( "x" )
```

### Cons Lists (Lisp-Style Linked Lists)

A **cons list** is a classic linked list from Lisp. Each node is either:
- **Nil**: empty list (no fields)
- **Cons**: a pair of (first-element, rest-of-list)

The names come from Lisp heritage:
- `cons` = "construct" a pair
- `car` = "contents of address register" = first element
- `cdr` = "contents of decrement register" = rest of list

Here's the complete pattern:

```seq
# Constructors
: nil ( -- List )  :Nil variant.make-0 ;
: cons ( T List -- List )  :Cons variant.make-2 ;

# Predicates
: nil? ( List -- Bool )  variant.tag :Nil symbol.= ;

# Accessors
: car ( List -- T )  0 variant.field-at ;
: cdr ( List -- List )  1 variant.field-at ;
```

Building a list works from right to left - start with `nil`, then prepend each element:

```seq
nil                   # ()           - empty list
3 swap cons           # (3)          - prepend 3
2 swap cons           # (2 3)        - prepend 2
1 swap cons           # (1 2 3)      - prepend 1
```

The `swap` is needed because `cons` expects `( T List )` but we have `( List T )`.

In the REPL, the raw stack output for nested variants looks cryptic. Peek at elements without destroying the list:

```seq
dup car           # 1 - first element
dup cdr car       # 2 - second element
dup cdr cdr car   # 3 - third element
```

See `examples/data/cons-list.seq` for a complete example with length, reverse, and printing.

## Safety Philosophy

Seq aspires to Rust's core principle: **if it compiles, it tends to run correctly**. The compiler statically eliminates entire categories of bugs that cause runtime failures in other languages.

### What the Compiler Guarantees

| Guarantee | What It Prevents |
|-----------|------------------|
| **No null** | NullPointerException, segfaults from nil access |
| **Exhaustive pattern matching** | Forgetting to handle error cases or union variants |
| **Stack effect verification** | Stack underflow, type mismatches, arity errors |
| **Explicit numeric types** | Silent precision loss, integer overflow surprises |
| **No shared mutable state** | Data races between strands |

### No Null

Seq has no null. There's no implicit "absence of value" that can appear in any type.

When you need to represent optional or fallible values, use union types:

```seq
union Option { None, Some { value: Int } }
union Result { Ok { value: Int }, Err { message: String } }
```

If a function returns a union type, the compiler requires callers to handle all variants via exhaustive `match`. You cannot forget the error case:

```seq
: maybe-parse ( String -- Option )  ... ;

: use-it ( String -- Int )
  maybe-parse match
    None -> 0                    # must handle this
    Some { >value } ->           # value extracted to stack, returned as result
  end
;
```

This is opt-in. Seq doesn't enforce a pervasive `Result` convention across the standard library - union types are used case by case where they make sense. The compiler's role is to ensure that *if* you use a union type, callers must handle all variants.

### What the Compiler Does Not Catch

Seq is not Rust. Some things remain the programmer's responsibility:

| Not Checked | Why |
|-------------|-----|
| Array bounds | Lists are dynamically sized; bounds checked at runtime |
| Integer overflow | Wraps silently (like C, unlike Rust debug builds) |
| Resource exhaustion | Stack overflow from non-tail recursion, OOM |
| Logic errors | The compiler verifies types, not intent |

The philosophy: eliminate the bugs that are both common and mechanically detectable. Stack effects catch most "wrong number of arguments" bugs. Exhaustive matching catches "forgot the error case." No null catches "didn't check for absence." Explicit numerics catch "mixed up int and float."

What remains are bugs that require understanding intent - and those are for tests and code review.

## Value Semantics

Seq has straightforward value semantics with no ownership tracking or move semantics.

### No Borrowing, No Moves

Unlike Rust, Seq has no borrow checker or ownership system. Unlike C++11+, there are no move constructors or rvalue references. Values are simply copied when needed:

```seq
5 dup    # Copies the integer - both stack positions hold 5
```

This simplicity comes from two design choices:
1. **Values are immutable** - you don't mutate values, you create new ones
2. **Sharing via reference counting** - complex types use Arc internally for O(1) copying

### Copying Behavior by Type

| Type | On `dup` | Notes |
|------|----------|-------|
| Int, Float, Bool | Bitwise copy | True value types |
| String | Deep copy | New allocation, independent string |
| Variant | Shallow copy | Arc refcount increment, data shared |
| Map | Deep copy | New HashMap with cloned entries |
| Channel | Shallow copy | Arc increment, shares sender/receiver |
| Quotation | Bitwise copy | Function pointers, no heap data |
| Closure | Shallow copy | Arc increment on captured environment |

### Why This Works

The lack of mutation eliminates the problems that borrowing solves. In Rust, you need the borrow checker because:
- Mutable references could alias
- Data could be freed while references exist
- Race conditions on shared mutable state

Seq sidesteps all of this:
- No mutation of values on the stack
- Reference counting handles lifetimes automatically
- Strands communicate via channels, not shared memory

### Comparison to Other Languages

| Language | Model | Seq Equivalent |
|----------|-------|----------------|
| Java | Primitives by value, objects by reference (shared mutable) | Primitives copy, collections share via Arc (immutable) |
| Rust | Ownership + borrowing, explicit moves | Everything copies, Arc handles sharing |
| C++ | Value types with copy/move constructors | Everything copies, no move optimization |
| Clojure | Persistent immutable data structures | Similar - variants share, maps clone |

Seq's model is closest to functional languages with persistent data structures. The simplicity cost is that large maps are expensive to "modify" (you clone the whole thing). The benefit is that you never think about lifetimes, borrows, or use-after-free.

## Error Handling

Seq uses a simple, type-preserving pattern for fallible operations: `( value Bool )`.

### The Value-Bool Pattern

Operations that can fail return their result plus a Bool success flag:

```seq
"42" string->int    # ( -- 42 true ) on success
"abc" string->int   # ( -- 0 false ) on failure
```

This pattern is used consistently across the standard library:

| Category | Example | Signature |
|----------|---------|-----------|
| Parsing | `string->int` | `( String -- Int Bool )` |
| File I/O | `file.slurp` | `( String -- String Bool )` |
| Environment | `os.getenv` | `( String -- String Bool )` |
| Collections | `map.get` | `( Map Key -- Value Bool )` |
| Encoding | `encoding.base64-decode` | `( String -- String Bool )` |

### Using the Pattern

The idiomatic way to handle fallible operations:

```seq
"42" string->int if
  # Success - the Int is on the stack
  2 i.*    # use it
else
  drop     # discard the failure value
  0        # provide a default
then
```

### Chaining Fallible Operations

For multiple fallible operations, check each result:

```seq
: get-port ( -- Int Bool )
    # Get PORT from environment, parse it, validate range
    # Demonstrates chaining: getenv -> parse -> validate
    "PORT" os.getenv if                     # Check env var exists
      string->int if                        # Parse as integer
        dup 1024 i.>= over 65535 i.<= and if
          true                              # Valid port in range
        else
          drop 8080 false                   # Port out of range
        then
      else
        drop 8080 false                     # PORT is not a number
      then
    else
      drop 8080 false                       # PORT not set
    then
;
```

### Why Not Result/Option Types?

Seq prioritizes compile-time type safety. A generic `Result<T,E>` type would require either losing type information (everything becomes Variant) or generic/parametric types (not supported).

The `( value Bool )` pattern preserves types: the Int stays an Int, the String stays a String.

If you need functional composition patterns (map, bind), you can define your own concrete Result types - see `examples/paradigms/functional/result.seq` for an example.

## String Operations

| Word | Effect | Description |
|------|--------|-------------|
| `string.concat` | `( String String -- String )` | Concatenate |
| `string.length` | `( String -- Int )` | Character count |
| `string.empty?` | `( String -- Bool )` | True if empty |
| `string.equal?` | `( String String -- Bool )` | Compare |
| `string.char-at` | `( String Int -- Int )` | Char code at index |
| `string.substring` | `( String Int Int -- String )` | Extract substring |
| `string.split` | `( String String -- List )` | Split into list |
| `string.chomp` | `( String -- String )` | Remove trailing newline |
| `string.trim` | `( String -- String )` | Remove whitespace |
| `string->int` | `( String -- Int Bool )` | Parse integer (value, success flag) |
| `int->string` | `( Int -- String )` | Format integer |

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
    dup 0 i.> if
        dup int->string io.write-line
        1 i.- countdown
    else
        drop
    then
;

# Process a list
: sum-list ( Variant -- Int )
    dup nil? if
        drop 0
    else
        dup car swap cdr sum-list i.+
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
    io.read-line if
        string.chomp
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
    dup 0 i.> if
        1 i.- loop
    else
        drop
    then
;
```

## Command Line Programs

```seq
: main ( -- )
    args.count 1 i.> if
        1 args.at          # First argument (0 is program name)
        process-file
    else
        "Usage: prog <file>" io.write-line
    then
;
```

| Word | Effect | Description |
|------|--------|-------------|
| `args.count` | `( -- Int )` | Number of arguments |
| `args.at` | `( Int -- String )` | Get argument by index |

## Script Mode

For quick iteration and scripting, you can run `.seq` files directly without a separate build step:

```bash
seqc myscript.seq arg1 arg2
```

Script mode compiles with `-O0` for fast startup and caches the binary for subsequent runs. The cache key includes the source content and all transitive includes, so scripts automatically recompile when any dependency changes.

### Shebang Support

Scripts can include a shebang for direct execution:

```seq
#!/usr/bin/env seqc
: main ( -- Int ) "Hello from script!" io.write-line 0 ;
```

```bash
chmod +x myscript.seq
./myscript.seq arg1 arg2    # Arguments passed to main
```

Note that the `main` word in a script must return `Int` (the exit code), unlike compiled programs where `main` returns `( -- )`.

### Cache Location

Compiled binaries are cached in:
- `$XDG_CACHE_HOME/seq/` if `XDG_CACHE_HOME` is set
- `~/.cache/seq/` otherwise

Cache entries are named by their SHA-256 hash. To clear the cache: `rm -rf ~/.cache/seq/`

### When to Use Script Mode

| Use Case | Recommendation |
|----------|----------------|
| Quick testing | Script mode |
| Development iteration | Script mode |
| Production deployment | `seqc build` with `-O3` (default) |
| Performance-critical | `seqc build` with optimizations |

Script mode trades runtime optimization (`-O0`) for faster compilation. For production use, compile with `seqc build` to get full LLVM optimizations.

## File Operations

| Word | Effect | Description |
|------|--------|-------------|
| `file.slurp` | `( String -- String Bool )` | Read entire file. Returns content and success flag |
| `file.spit` | `( String String -- Bool )` | Write content to file. Takes content and path, returns success |
| `file.append` | `( String String -- Bool )` | Append content to file. Takes content and path, returns success |
| `file.exists?` | `( String -- Bool )` | Check if file exists at path |
| `file.delete` | `( String -- Bool )` | Delete a file at path. Returns success |
| `file.size` | `( String -- Int Bool )` | Get file size in bytes. Returns size and success |
| `file.for-each-line+` | `( String [String --] -- String Bool )` | Process file line by line |

## Directory Operations

| Word | Effect | Description |
|------|--------|-------------|
| `dir.exists?` | `( String -- Bool )` | Check if directory exists at path |
| `dir.make` | `( String -- Bool )` | Create a directory at path. Returns success |
| `dir.delete` | `( String -- Bool )` | Delete an empty directory. Returns success |
| `dir.list` | `( String -- List Bool )` | List directory contents. Returns filenames and success |

### Line-by-Line File Processing

For processing files line by line, use `file.for-each-line+`:

```seq
: process-line ( String -- )
    string.chomp
    # ... do something with line
;

: main ( -- )
    "data.txt" [ process-line ] file.for-each-line+
    if
        drop  # drop empty string on success
        "Done!" io.write-line
    else
        # error message is on stack
        "Error: " swap string.concat io.write-line
    then
;
```

The quotation receives each line (including trailing newline) and must consume it.
Returns `("" true)` on success, `("error message" false)` on failure. Empty files succeed
without calling the quotation.

Line endings are normalized to `\n` regardless of platform - Windows-style `\r\n`
becomes `\n`. This ensures consistent behavior when processing files across
different operating systems.

This is safer than slurp-and-split for large files - lines are processed one at a time
rather than loading the entire file into memory.

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

## Naming Convention

Seq uses a consistent naming scheme for all built-in operations:

| Delimiter | Usage | Example |
|-----------|-------|---------|
| `.` (dot) | Module/namespace prefix | `io.write-line`, `tcp.listen`, `string.concat` |
| `-` (hyphen) | Compound words within names | `home-dir`, `field-at`, `write-line` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |

### Words Are Just Names

In Seq, a *word* is any contiguous sequence of non-whitespace characters. There are
no operators - the `.` in `io.write-line` is part of the word's name, not syntax
for "calling a method on an object."

```seq
io.write-line    # This is ONE word, not "io" followed by "write-line"
string.concat    # This is ONE word, not a method call on a string object
```

If you come from object-oriented languages, this may feel strange at first. In OO,
`foo.bar` means "send the `bar` message to `foo`." In Seq, `io.write-line` is simply
a name that includes a dot - exactly like `write-line` is a name that includes a
hyphen. The dot is a naming convention for grouping related operations, not a
dereferencing or method dispatch operator.

Concatenative languages work differently: there are no objects receiving messages.
There is only the stack. Words consume values from the stack and push results back.
`io.write-line` doesn't operate "on" an io object - it pops a string and writes it.

### Module Prefixes

Operations are grouped by functionality:

| Prefix | Domain | Examples |
|--------|--------|----------|
| `io.` | Console I/O | `io.write-line`, `io.read-line` |
| `file.` | File operations | `file.slurp`, `file.spit`, `file.exists?` |
| `dir.` | Directory operations | `dir.list`, `dir.make`, `dir.exists?` |
| `string.` | String manipulation | `string.concat`, `string.trim` |
| `list.` | List operations | `list.map`, `list.filter` |
| `map.` | Hash maps | `map.make`, `map.get`, `map.set` |
| `chan.` | Channels | `chan.make`, `chan.send`, `chan.receive` |
| `tcp.` | Networking | `tcp.listen`, `tcp.accept` |
| `os.` | Operating system | `os.getenv`, `os.home-dir` |
| `args.` | Command-line args | `args.count`, `args.at` |
| `variant.` | Variant introspection | `variant.tag`, `variant.field-at` |
| `i.` | Integer operations | `i.+`, `i.-`, `i.*`, `i./`, `i.=`, `i.<` |
| `f.` | Float operations | `f.+`, `f.-`, `f.*`, `f./`, `f.=`, `f.<` |

### Suffixes

| Suffix | Meaning | Example |
|--------|---------|---------|
| `?` | Predicate (returns boolean) | `nil?`, `string.empty?`, `file.exists?` |
| `+` | Returns result + status | `file.for-each-line+` |

### Core Primitives (No Prefix)

Fundamental operations remain unnamespaced for conciseness:

- **Stack:** `dup`, `swap`, `over`, `rot`, `nip`, `tuck`, `drop`, `pick`, `roll`
- **Boolean:** `and`, `or`, `not`
- **Bitwise:** `band`, `bor`, `bxor`, `bnot`, `shl`, `shr`, `popcount`, `clz`, `ctz`
- **Control:** `call`, `spawn`, `cond`

### Type-Prefixed Arithmetic and Comparison

Integer and float operations use explicit type prefixes:

- **Integer arithmetic:** `i.add`, `i.subtract`, `i.multiply`, `i.divide` (or terse: `i.+`, `i.-`, `i.*`, `i./`, `i.%`)
- **Integer comparison:** `i.=`, `i.<`, `i.>`, `i.<=`, `i.>=`, `i.<>` (or verbose: `i.eq`, `i.lt`, `i.gt`, `i.lte`, `i.gte`, `i.neq`)
- **Float arithmetic:** `f.add`, `f.subtract`, `f.multiply`, `f.divide` (or terse: `f.+`, `f.-`, `f.*`, `f./`)
- **Float comparison:** `f.=`, `f.<`, `f.>`, `f.<=`, `f.>=`

This is a deliberate design choice, not a limitation. **Implicit type conversions are harmful.**

Many languages silently convert between numeric types, leading to subtle bugs:
- JavaScript's `"5" + 3` yields `"53"` but `"5" - 3` yields `2`
- C silently converts between numeric types - promoting integers, truncating floats to integers, and losing precision when narrowing - without warning by default
- Python 2's `/` behaved differently for int vs float operands

Seq rejects this entirely. When you write `i.+`, you know both operands are integers and the result is an integer. When you need to mix types, you convert explicitly:

```seq
42 int->float 3.14 f.+    # Explicit: convert int to float, then add
```

The code states exactly what happens. No implicit coercion, no surprises, no "wat" moments. The few extra characters buy certainty about program behavior.

Note that explicit conversions can still lose precision - `int->float` loses precision for integers beyond 2^53, and `float->int` truncates the fractional part. The point isn't that conversions are lossless; it's that you asked for it, and it's visible in the code.

### Rationale

The naming convention provides:

1. **Discoverability** - Related operations share a prefix. Wondering what you can do with strings? Look for `string.*`
2. **No collisions** - `length` could mean string length, list length, or map size. `string.length`, `list.length`, and `map.size` are unambiguous
3. **Clean primitives** - Core stack operations like `dup` and `swap` appear in nearly every word; namespacing them would add noise
4. **Familiar patterns** - The `.` delimiter echoes method syntax from other languages; `->` for conversions is intuitive

## Maps

Key-value dictionaries with O(1) lookup:

```seq
map.make                    # ( -- Map )
"name" "Alice" map.set      # ( Map K V -- Map )
"age" 30 map.set
"name" map.get              # ( Map K -- V Bool )
"name" map.has?             # ( Map K -- Map Bool )
map.keys                    # ( Map -- List )
```

## SON (Seq Object Notation)

SON is Seq's native data serialization format - it's valid Seq code that reconstructs data when evaluated. This makes SON ideal for configuration files, data exchange, and debugging.

### Format Overview

| Type | SON Format | Example |
|------|------------|---------|
| Int | literal | `42`, `-123` |
| Float | literal | `3.14`, `42.0` |
| Bool | literal | `true`, `false` |
| String | quoted | `"hello"`, `"line\nbreak"` |
| Symbol | colon prefix | `:my-symbol`, `:None` |
| List | builder pattern | `list-of 1 lv 2 lv 3 lv` |
| Map | builder pattern | `map-of "key" "value" kv` |
| Variant | wrap-N | `:Point 10 20 wrap-2` |

### Using SON

Include the SON module to access builder words:

```seq
include std:son

# Build a list
list-of 1 lv 2 lv 3 lv          # ( -- List )

# Build a map
map-of "name" "Alice" kv        # ( -- Map )
       "age" 30 kv

# Build a variant (fields before tag)
:None wrap-0                    # ( -- Variant ) no fields
42 :Some wrap-1                 # ( -- Variant ) one field
10 20 :Point wrap-2             # ( -- Variant ) two fields
```

### Serializing Values

Use `son.dump` to convert any value to its SON string representation:

```seq
include std:son

# Serialize primitives
42 son.dump                     # "42"
true son.dump                   # "true"
"hello" son.dump                # "\"hello\""

# Serialize complex structures
list-of 1 lv 2 lv son.dump      # "list-of 1 lv 2 lv"

# Pretty-print with indentation
list-of 1 lv 2 lv son.dump-pretty
# list-of
#   1 lv
#   2 lv
```

### Loading SON Files

SON files define words that return data structures:

```seq
# config.son
include std:son

: config ( -- Map )
  map-of
    "debug" true kv
    "port" 8080 kv
;
```

```seq
# main.seq
include std:son
include "config.son"   # adds the 'config' word

: main ( -- )
  config              # call to get the Map
  "port" map.get      # get the port value
;
```

### Stack Display

The REPL uses SON format when displaying stack contents via `stack.dump`:

```
stack: list-of 1 lv 2 lv map-of "name" "Alice" kv :None wrap-0 true 42
```

This makes it easy to copy values from the REPL output directly into Seq code.

## Zipper: Functional List Navigation

The `std:zipper` module provides a zipper data structure for efficient cursor-based navigation and editing of immutable lists. A zipper maintains a focus element with left and right context, enabling O(1) movement and modification.

```seq
include std:zipper

# Create a zipper from a list
list-of 1 lv 2 lv 3 lv 4 lv 5 lv
zipper.from-list               # focus at 1

# Navigate
zipper.right zipper.right      # focus at 3

# Modify
99 zipper.set                  # replace focus with 99

# Convert back
zipper.to-list                 # [1, 2, 99, 4, 5]
```

Key operations:
- **Navigation**: `zipper.left`, `zipper.right`, `zipper.start`, `zipper.end`
- **Query**: `zipper.focus`, `zipper.index`, `zipper.length`
- **Modification**: `zipper.set`, `zipper.insert-left`, `zipper.insert-right`, `zipper.delete`

See `STDLIB_REFERENCE.md` for the complete API.

## Higher-Order Words

```seq
# Map over a list
my-list [ 2 i.* ] list.map

# Filter a list
my-list [ 0 i.> ] list.filter

# Fold (reduce)
my-list 0 [ i.+ ] list.fold
```

## Concurrency

Seq supports massive concurrency through **strands** - lightweight green threads
built on a coroutine runtime. Thousands of strands can run on a single OS thread,
cooperatively yielding during I/O operations.

### Strands

Spawn a quotation as a new strand:

```seq
[ "Hello from strand!" io.write-line ] strand.spawn drop   # drop strand ID
```

Strands are cheap - spawn thousands of them. They're ideal for:
- Handling concurrent connections
- Parallel processing pipelines
- Actor-style architectures

### Channels (CSP-Style Communication)

Strands communicate through channels, following the CSP (Communicating Sequential
Processes) model - similar to Go channels or Erlang message passing.

| Word | Effect | Description |
|------|--------|-------------|
| `chan.make` | `( -- Channel )` | Create channel |
| `chan.send` | `( T Channel -- Bool )` | Send value, returns true on success |
| `chan.receive` | `( Channel -- T Bool )` | Receive value, returns (value, success) |
| `chan.close` | `( Channel -- )` | Close the channel |

Channel operations return status flags rather than panicking. Always check the boolean result:

### Producer-Consumer Example

```seq
: send-messages ( Channel Int -- )
    dup 0 i.> if
        over "message" swap chan.send drop  # send returns Bool, drop it
        1 i.- send-messages
    else
        drop chan.close
    then
;

: producer ( Channel -- )
    10 send-messages
;

: consumer ( Channel -- )
    dup chan.receive if
        io.write-line
        consumer        # loop via recursion
    else
        drop drop       # channel closed, drop message and channel
    then
;

: main ( -- )
    chan.make
    dup [ producer ] strand.spawn drop
    consumer
;
```

### TCP Networking

Build network servers with strand-per-connection:

| Word | Effect | Description |
|------|--------|-------------|
| `tcp.listen` | `( Int -- Int )` | Listen on port, return listener |
| `tcp.accept` | `( Int -- Int )` | Accept connection, return socket |
| `tcp.read` | `( Int -- String )` | Read from socket |
| `tcp.write` | `( String Int -- )` | Write to socket |
| `tcp.close` | `( Int -- )` | Close socket |

### Concurrent Server Pattern

```seq
: handle-client ( Int -- )
    dup tcp.read      # read request
    process-request   # your logic here
    over tcp.write    # write response
    tcp.close
;

: accept-loop ( Int -- )
    dup tcp.accept                    # ( listener client )
    [ handle-client ] strand.spawn drop      # spawn handler
    accept-loop                       # tail call - runs forever, no stack growth
;

: main ( -- )
    8080 tcp.listen
    "Listening on :8080" io.write-line
    accept-loop
;
```

Each connection runs in its own strand. The recursive `accept-loop` runs forever
without growing the stack - TCO converts the tail call into a jump. No callbacks,
no async/await, just sequential code that scales.

### Why Strands?

Traditional threading has problems:
- OS threads are expensive (~1MB stack each)
- Context switching is slow
- Shared memory requires careful locking

Strands solve these:
- Lightweight (128KB coroutine stack per strand, fixed, configurable via `SEQ_STACK_SIZE`)
- Cooperative scheduling (fast context switch)
- Message passing via channels (no shared state)

Write code that reads sequentially, runs concurrently.

## Understanding Type Errors

Seq's type system tracks two orthogonal concepts:

| Concept | What It Is | Example |
|---------|------------|---------|
| Stack Effect | A word's declared transformation | `( Int Int -- Int )` |
| Stack Type | The actual stack state at a point | `(..rest Float Float)` |

A **stack effect** describes what a word *does* - its inputs and outputs.
A **stack type** describes what *is* - the current stack contents.

Type errors occur when your stack type doesn't satisfy a word's input requirements:

```
i.divide: stack type mismatch. Expected (..a$0 Int Int), got (..rest Float Float): Type mismatch: cannot unify Int with Float
```

Here, `i.divide` has stack effect `( Int Int -- Int )`. The compiler checks:
"Does the current stack type have two `Int` values on top?" Your stack type
`(..rest Float Float)` has two `Float` values instead - mismatch.

### Reading the Error

The format `(..name Type Type ...)` represents a stack state:

| Component | Meaning |
|-----------|---------|
| `(...)` | Stack contents, left-to-right = bottom-to-top |
| `..a` or `..rest` | "The rest of the stack" (row variable) |
| `Int`, `Float`, etc. | Concrete types at those positions |
| `a$0`, `a$5`, etc. | Freshened variable names (the number is just a counter) |

So `(..a$0 Int Int)` means: "any stack with two `Int` values on top."

### Visual Breakdown

```
i.divide: stack type mismatch. Expected (..a$0 Int Int), got (..rest Float Float)
                                         │     │   │           │      │     │
                                         │     │   └── top     │      │     └── top
                                         │     └── 2nd         │      └── 2nd
                                         └── rest of stack     └── rest of stack

Translation:
  i.divide expects: ( ..a Int Int -- ..a Int )  ← two Ints in, one Int out
  You provided:   ( ..rest Float Float )      ← two Floats
  Problem:        Int ≠ Float
```

### Row Variables Enable Polymorphism

The `..a` notation (row variable) is what makes words like `dup` work on any
stack depth:

```seq
: dup ( ..a T -- ..a T T )
```

This says: "Whatever is on the stack (`..a`), plus some value of type `T` on
top, I'll duplicate that `T`, leaving the rest untouched."

Row variables let the type checker verify stack effects without knowing the
full stack contents - only the parts each word actually touches.

### Common Type Errors

| Error | Cause | Fix |
|-------|-------|-----|
| `Expected Int, got Float` | Wrong numeric type | Use `f.divide` for floats |
| `Expected String, got Int` | Need conversion | Use `int->string` |
| `stack underflow` | Not enough values | Check stack effect, add values |
| `cannot unify T with U` | Type variables don't match | Ensure consistent types |

---

*Seq: where composition is not just a pattern, but the foundation.*
