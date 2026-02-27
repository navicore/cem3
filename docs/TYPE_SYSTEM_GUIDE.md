# Seq Type System Guide

## Overview

Seq has a **static type system** with **row polymorphism** that verifies your programs at compile time. The type checker ensures:

- **Stack safety**: Operations receive the correct types
- **No stack underflow**: Operations never pop from an empty stack
- **Branch compatibility**: Conditionals produce consistent stack effects
- **Type correctness**: String operations get Strings, Int operations get Ints, etc.

All type checking happens at **compile time** - there's zero runtime overhead.

---

## Stack Effect Declarations

### Basic Syntax

Words declare their **stack effect** - how they transform the stack:

```seq
: square ( Int -- Int )
  dup i.* ;
```

Stack effect format: `( inputs -- outputs )`
- **Before `--`**: Types expected on stack (top on right)
- **After `--`**: Types produced on stack (top on right)

### Reading Stack Effects

Stack effects are read **bottom-to-top**, with the **rightmost type** being the **top of stack**:

```seq
: example ( Int String -- Bool )
  # Expects:  Bottom [ Int String ] Top
  # Produces: Bottom [ Bool ] Top
  ...
;
```

When this word is called:
- String must be on **top** of stack
- Int must be **below** the String
- After execution, a Bool will be on top

### Examples

```seq
# Takes nothing, produces an Int
: forty-two ( -- Int )
  42 ;

# Takes two Ints, produces one Int
: add-numbers ( Int Int -- Int )
  i.+ ;

# Takes String, produces nothing (prints it)
: print ( String -- )
  io.write-line ;

# Takes Int and String, produces String (e.g., "Value: 42")
: format ( Int String -- String )
  swap int->string swap string.concat ;
```

---

## Row Polymorphism

### The Problem

How do we type `dup`? It should work for **any** type:

```seq
42 dup       # Works: Int Int
"hi" dup     # Works: String String
```

But it also needs to work with **any stack depth**:

```seq
# With empty stack
42 dup            # ( -- Int Int )

# With existing values on stack
10 20 dup         # ( Int -- Int Int Int )
"a" "b" dup       # ( String -- String String String )
```

### The Solution: Row Variables

Row variables represent "the rest of the stack":

```seq
: dup ( ..a T -- ..a T T )
  # ..a = whatever is already on the stack
  # T = type on top
  # Result: same stack, but top duplicated
  ...
;
```

**Components:**
- `..a` - Row variable (rest of stack)
- `T` - Type variable (polymorphic over any type)
- Stack effect says: "Give me a stack with some stuff (..a) and a value (T) on top, I'll give you the same stack with that value duplicated"

### Row Polymorphism in Action

All stack operations use row polymorphism:

```seq
# Duplicate top value
: dup ( ..a T -- ..a T T )

# Remove top value
: drop ( ..a T -- ..a )

# Swap top two values
: swap ( ..a T U -- ..a U T )

# Copy second value to top
: over ( ..a T U -- ..a T U T )

# Rotate three values
: rot ( ..a T U V -- ..a U V T )
```

Built-in operations also use row polymorphism:

```seq
# Add: works at any stack depth
: i.+ ( ..a Int Int -- ..a Int )

# Print: works at any stack depth
: io.write-line ( ..a String -- ..a )
```

### Implicit Row Polymorphism

**All stack effects in Seq are implicitly row-polymorphic.** You don't need to write `..rest` explicitly - the compiler adds it automatically:

```seq
# What you write:
: double ( Int -- Int )
  dup i.+ ;

# What the compiler understands:
: double ( ..rest Int -- ..rest Int )
  dup i.+ ;
```

This means:
- `( -- Int )` is treated as `( ..rest -- ..rest Int )` - pushes Int onto any stack
- `( Int -- )` is treated as `( ..rest Int -- ..rest )` - consumes Int from any stack
- `( Int Int -- Int )` is treated as `( ..rest Int Int -- ..rest Int )` - works at any depth

**Why this matters:** You can call `double` from any stack state:

```seq
# With one value on stack:
42 double           # 42 → 84

# With extra values below:
10 20 30 double     # 10 20 30 → 10 20 60
```

The values 10 and 20 are untouched—`double` only operates on the top. Without implicit row polymorphism, `double` would only work with exactly one Int on the stack—you couldn't compose operations freely.

**When to use explicit row variables:**
- Use explicit `..a`, `..rest` when you need to **name** the row variable
- Useful when multiple row variables must **match** (e.g., in quotation types)
- Example: `( ..a T -- ..a T T )` makes it clear both sides share the same `..a`

### Why This Matters

Row polymorphism enables **stack operation composition**:

```seq
: swap-and-add ( Int Int Int -- Int Int )
  swap i.+ ;

# Type checker verifies:
# 1. swap: ( ..a Int Int -- ..a Int Int )
#    With ..a = Int, we get: ( Int Int Int -- Int Int Int )
# 2. i.+: ( ..a Int Int -- ..a Int )
#    With ..a = Int, we get: ( Int Int Int -- Int Int )
# Result: ( Int Int Int -- Int Int ) ✓
```

### Row Polymorphism vs Traditional Generics

If you're familiar with generics from languages like Java, Rust, or TypeScript, row polymorphism may seem similar—but it solves a different problem.

**Traditional Generics** parameterize over individual types:

```typescript
// TypeScript: generic over one type T
function identity<T>(x: T): T {
  return x;
}

// Rust: generic over type T
fn identity<T>(x: T) -> T { x }
```

This lets `identity` work with any single type. But what if you need to abstract over *multiple* types at once—without knowing how many?

**Row Polymorphism** parameterizes over *sequences* of types:

```seq
# Seq: polymorphic over the entire stack prefix
: dup ( ..a T -- ..a T T )
```

The `..a` isn't a single type—it's zero or more types. This is essential for stack-based languages where operations must work regardless of what's "below" them on the stack.

**Comparison table:**

| Feature | Traditional Generics | Row Polymorphism |
|---------|---------------------|------------------|
| Abstraction unit | Single type (`T`) | Sequence of types (`..a`) |
| Fixed arity | Function has fixed param count | Stack depth is variable |
| Composition | Explicit argument passing | Implicit stack threading |
| Use case | Collections, containers | Stack operations, concatenative code |

**Why generics alone aren't enough:**

Consider typing `swap` with only traditional generics:

```typescript
// TypeScript - can type swap for exactly 2 args:
function swap<T, U>(a: T, b: U): [U, T] {
  return [b, a];
}
```

But this doesn't let `swap` ignore extra values. In Seq:

```seq
: swap ( ..a T U -- ..a U T )
```

The `..a` means "whatever else is on the stack stays unchanged." You can't express this with traditional generics—you'd need a separate `swap2`, `swap3`, etc. for each stack depth.

**Row polymorphism is generics extended to type sequences.** Where `T` abstracts over a single type, `..a` abstracts over zero or more types in a specific order—each potentially different. The compiler tracks that `..a` bound to `Int, String, Float` stays exactly `Int, String, Float`.

---

## Types in Seq

### Concrete Types

- **Int**: Integer numbers (64-bit signed)
- **Float**: Floating-point numbers (64-bit)
- **String**: Text strings
- **Bool**: A distinct type tracked by the type checker. Literals are `true` and `false`. Comparison and logical operations produce `Bool`; `if` requires `Bool`.

### Type Variables

- **T, U, V, ...**: Polymorphic type variables (uppercase)
- Can unify with any concrete type
- Example: `dup` works for `T` where T can be Int, String, etc.

### Row Variables

- **..a, ..b, ..rest**: Row variables (lowercase with `..` prefix)
- Represent "rest of stack"
- Enable polymorphism over stack depth

---

## Type Errors Explained

### Type Mismatch

```seq
: bad ( Int -- )
  io.write-line ;  # ERROR: io.write-line expects String, got Int
```

**Error message:**
```
io.write-line: stack type mismatch.
Expected (..a String), got (..a Int): Type mismatch: cannot unify String with Int
```

**Fix:** Convert Int to String first:
```seq
: good ( Int -- )
  int->string io.write-line ;
```

### Stack Underflow

```seq
: bad ( -- )
  drop ;  # ERROR: can't drop from empty stack
```

**Error message:**
```
drop: stack type mismatch.
Expected (..a T), got (): stack underflow
```

**Fix:** Ensure stack has a value first:
```seq
: good ( Int -- )
  drop ;
```

### Branch Incompatibility

```seq
: bad ( Int -- ? )
  0 > if
    42          # Produces: Int
  else
    "hello"     # Produces: String - ERROR!
  then ;
```

**Error message:**
```
if branches have incompatible stack effects:
then=(..a Int), else=(..a String): Type mismatch: cannot unify Int with String
```

**Fix:** Both branches must produce the same types:
```seq
: good ( Int -- String )
  0 > if
    "positive"
  else
    "non-positive"
  then ;
```

### Unbalanced If/Then

```seq
: bad ( Int Int -- Int )
  > if
    100    # Pushes Int
  then ;   # ERROR: else branch leaves stack unchanged
```

**Error message:**
```
if branches have incompatible stack effects:
then=(..a Int), else=(..a): branches must produce identical stack effects
```

**Fix:** Provide else branch OR don't push in then:
```seq
: good ( Int Int -- Int )
  > if
    100
  else
    0
  then ;
```

---

## Type Checking Process

The type checker works in two passes:

### Pass 1: Collect Signatures

```seq
: helper ( Int -- String ) int->string ;
: main ( -- ) 42 helper io.write-line ;
```

First, the checker collects all word signatures:
- `helper: ( Int -- String )`
- `main: ( -- )`

### Pass 2: Verify Bodies

For each word, the checker:

1. **Starts with declared input stack**
2. **Processes each statement**, tracking stack changes
3. **Verifies result matches declared output**

Example for `main`:

```
Start:        ( -- )                    # Empty stack
After 42:     ( Int )                   # Pushed Int
After helper: ( String )                # Applied helper's effect
After io.write-line: ( )                   # Applied io.write-line's effect
Result:       ( )                       # Matches declared output ✓
```

### Unification

When applying an effect like `add: ( ..a Int Int -- ..a Int )` to current stack `( Int Int Int )`:

1. **Unify effect input with current stack:**
   - Effect input: `..a Int Int`
   - Current stack: `Int Int Int`
   - Unification: `..a = Int` (row variable binds to Int)

2. **Apply substitution to effect output:**
   - Effect output: `..a Int`
   - Substitute `..a = Int`: `Int Int`
   - Result stack: `( Int Int )`

This is how the type checker proves stack safety!

---

## Best Practices

### 1. Always Declare Effects

Even though the checker can infer types, **always declare effects** for clarity:

```seq
# Good - clear intent
: double ( Int -- Int )
  2 i.* ;

# Discouraged - unclear what it does
: double
  2 i.* ;
```

### 2. Use Descriptive Row Variable Names

```seq
# Okay
: dup ( ..a T -- ..a T T ) ... ;

# Better - shows it's the rest of stack
: dup ( ..rest T -- ..rest T T ) ... ;
```

### 3. Check Both Branches

When using conditionals, ensure **both branches** produce the same effect:

```seq
: abs ( Int -- Int )
  dup 0 < if
    -1 i.*    # Negate
  else
    # Leave unchanged - implicit "do nothing"
  then ;
```

### 4. Use int->string for Conversions

```seq
: print-number ( Int -- )
  int->string io.write-line ;
```

---

## Examples

### Simple Math

```seq
: square ( Int -- Int )
  dup i.* ;

: pythagorean ( Int Int -- Int )
  # a^2 + b^2
  swap square    # ( a b -- b a^2 )
  swap square    # ( b a^2 -- a^2 b^2 )
  i.+ ;          # ( a^2 b^2 -- sum )
```

### String Operations

```seq
: greet ( String -- )
  "Hello, " swap string.concat io.write-line ;

: print-number ( Int -- )
  int->string io.write-line ;
```

### Conditionals

```seq
: max ( Int Int -- Int )
  2dup i.> if
    drop    # Keep first
  else
    nip     # Keep second
  then ;

: describe ( Int -- String )
  0 i.> if
    "positive"
  else
    "non-positive"
  then ;
```

### Stack Shuffling

```seq
: rot-sum ( Int Int Int -- Int )
  # Sum three numbers after rotation
  rot i.+ i.+ ;

: under ( Int Int Int -- Int Int )
  # Like over but deeper
  rot swap ;
```

---

## Summary

Seq's type system provides:

- ✅ **Stack safety** - no underflows, no type mismatches
- ✅ **Row polymorphism** - stack operations work at any depth
- ✅ **Implicit row polymorphism** - all effects are automatically row-polymorphic
- ✅ **Zero runtime cost** - all checking at compile time
- ✅ **Clear error messages** - tells you exactly what's wrong
- ✅ **Compile-time guarantees** - if it type checks, stack operations are safe

The type system is **simple but powerful** - it catches bugs early without getting in your way.

**Happy concatenative programming!** 🎉
