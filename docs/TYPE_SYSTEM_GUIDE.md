# cem3 Type System Guide

## Overview

cem3 has a **static type system** with **row polymorphism** that verifies your programs at compile time. The type checker ensures:

- **Stack safety**: Operations receive the correct types
- **No stack underflow**: Operations never pop from an empty stack
- **Branch compatibility**: Conditionals produce consistent stack effects
- **Type correctness**: String operations get Strings, Int operations get Ints, etc.

All type checking happens at **compile time** - there's zero runtime overhead.

---

## Stack Effect Declarations

### Basic Syntax

Words declare their **stack effect** - how they transform the stack:

```cem
: square ( Int -- Int )
  dup multiply ;
```

Stack effect format: `( inputs -- outputs )`
- **Before `--`**: Types expected on stack (top on right)
- **After `--`**: Types produced on stack (top on right)

### Reading Stack Effects

Stack effects are read **bottom-to-top**, with the **rightmost type** being the **top of stack**:

```cem
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

```cem
# Takes nothing, produces an Int
: forty-two ( -- Int )
  42 ;

# Takes two Ints, produces one Int
: add-numbers ( Int Int -- Int )
  add ;

# Takes String, produces nothing (prints it)
: print ( String -- )
  write_line ;

# Takes Int and String, produces String
: format ( Int String -- String )
  swap int->string swap ;
```

---

## Row Polymorphism

### The Problem

How do we type `dup`? It should work for **any** type:

```cem
42 dup       # Works: Int Int
"hi" dup     # Works: String String
```

But it also needs to work with **any stack depth**:

```cem
# With empty stack
42 dup            # ( -- Int Int )

# With existing values on stack
10 20 dup         # ( Int -- Int Int Int )
"a" "b" dup       # ( String -- String String String )
```

### The Solution: Row Variables

Row variables represent "the rest of the stack":

```cem
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

```cem
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

```cem
# Add: works at any stack depth
: add ( ..a Int Int -- ..a Int )

# Print: works at any stack depth
: write_line ( ..a String -- ..a )
```

### Why This Matters

Row polymorphism enables **stack operation composition**:

```cem
: swap-and-add ( Int Int Int -- Int )
  swap add ;

# Type checker verifies:
# 1. swap: ( ..a Int Int -- ..a Int Int )
#    With ..a = Int, we get: ( Int Int Int -- Int Int Int )
# 2. add: ( ..a Int Int -- ..a Int )
#    With ..a = Int, we get: ( Int Int Int -- Int Int )
# Result: ( Int Int Int -- Int Int ) ✓
```

---

## Types in cem3

### Concrete Types

- **Int**: Integer numbers (64-bit signed)
- **String**: Text strings
- **Bool**: Not a separate type - represented as Int (0 = false, non-zero = true)

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

```cem
: bad ( Int -- )
  write_line ;  # ERROR: write_line expects String, got Int
```

**Error message:**
```
write_line: stack type mismatch.
Expected Cons { rest: RowVar("a"), top: String },
got Cons { rest: RowVar("a"), top: Int }
```

**Fix:** Convert Int to String first:
```cem
: good ( Int -- )
  int->string write_line ;
```

### Stack Underflow

```cem
: bad ( -- )
  drop ;  # ERROR: can't drop from empty stack
```

**Error message:**
```
drop: stack type mismatch.
Expected Cons { rest: RowVar("a"), top: Var("T") },
got Empty
```

**Fix:** Ensure stack has a value first:
```cem
: good ( Int -- )
  drop ;
```

### Branch Incompatibility

```cem
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
then=Cons { rest: RowVar("a"), top: Int },
else=Cons { rest: RowVar("a"), top: String }
```

**Fix:** Both branches must produce the same types:
```cem
: good ( Int -- String )
  0 > if
    "positive"
  else
    "non-positive"
  then ;
```

### Unbalanced If/Then

```cem
: bad ( Int Int -- Int )
  > if
    100    # Pushes Int
  then ;   # ERROR: else branch leaves stack unchanged
```

**Error message:**
```
if branches have incompatible stack effects:
then=Cons { rest: RowVar("a"), top: Int },
else=RowVar("a")
```

**Fix:** Provide else branch OR don't push in then:
```cem
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

```cem
: helper ( Int -- String ) int->string ;
: main ( -- ) 42 helper write_line ;
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
After write_line: ( )                   # Applied write_line's effect
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

```cem
# Good - clear intent
: double ( Int -- Int )
  2 multiply ;

# Discouraged - unclear what it does
: double
  2 multiply ;
```

### 2. Use Descriptive Row Variable Names

```cem
# Okay
: dup ( ..a T -- ..a T T ) ... ;

# Better - shows it's the rest of stack
: dup ( ..rest T -- ..rest T T ) ... ;
```

### 3. Check Both Branches

When using conditionals, ensure **both branches** produce the same effect:

```cem
: abs ( Int -- Int )
  dup 0 < if
    -1 multiply    # Negate
  else
    # Leave unchanged - implicit "do nothing"
  then ;
```

### 4. Use int->string for Conversions

```cem
: print-number ( Int -- )
  int->string write_line ;
```

---

## Examples

### Simple Math

```cem
: square ( Int -- Int )
  dup multiply ;

: pythagorean ( Int Int -- Int )
  # a^2 + b^2
  swap square    # ( Int Int -- Int )
  swap square    # ( Int Int -- Int )
  add ;          # ( Int Int -- Int )
```

### String Operations

```cem
: greet ( String -- )
  "Hello, " swap   # Would need concat - not yet implemented
  write_line ;

: print-number ( Int -- )
  int->string write_line ;
```

### Conditionals

```cem
: max ( Int Int -- Int )
  2dup > if
    drop    # Keep first
  else
    nip     # Keep second
  then ;

: describe ( Int -- String )
  0 > if
    "positive"
  else
    "non-positive"
  then ;
```

### Stack Shuffling

```cem
: rot-sum ( Int Int Int -- Int )
  # Sum three numbers after rotation
  rot add add ;

: under ( Int Int Int -- Int Int )
  # Like over but deeper
  rot swap ;
```

---

## Current Limitations

### No Quotations Yet

First-class functions (quotations) are not yet implemented:

```cem
# Not yet supported:
: map ( List [T -- U] -- List )
  ...
;
```

This is planned for a future phase.

### No User-Defined Types Yet

Currently only built-in types (Int, String) are supported:

```cem
# Not yet supported:
type Option T = Some T | None ;
```

Algebraic data types are planned for a future phase.

### No Type Inference

All word effects must be **explicitly declared**. The checker verifies but doesn't infer:

```cem
# Must declare effect:
: double ( Int -- Int )
  2 multiply ;

# Can't omit effect (discouraged):
: double
  2 multiply ;
```

---

## Summary

cem3's type system provides:

- ✅ **Stack safety** - no underflows, no type mismatches
- ✅ **Row polymorphism** - stack operations work at any depth
- ✅ **Zero runtime cost** - all checking at compile time
- ✅ **Clear error messages** - tells you exactly what's wrong
- ✅ **Compile-time guarantees** - if it type checks, stack operations are safe

The type system is **simple but powerful** - it catches bugs early without getting in your way.

**Happy concatenative programming!** 🎉
