# cem3 Type System Design

**Date:** 2025-11-15

**Goal:** Reconcile modern type system benefits with idiomatic concatenative style

---

## Design Philosophy

### Core Principle: Types Should **Enhance**, Not **Fight** Concatenative Style

**Bad type system for concatenative:**
```ml
// Feels like ML bolted onto Forth
dup<T>(value: T): (T, T)
```

**Good type system for concatenative:**
```cem
: dup ( ..a T -- ..a T T )
  # Stack-centric, composition-friendly
;
```

**Why this matters:**
- Concatenative = composition by juxtaposition
- Types must respect "threading" semantics
- Row polymorphism is not optional, it's fundamental
- Stack effects should read like documentation

---

## Key Design Decisions

### 1. Stack Effect Notation

**Syntax:** `( inputs -- outputs )`

**Examples:**
```cem
: dup   ( ..a T -- ..a T T )
: swap  ( ..a T U -- ..a U T )
: over  ( ..a T U -- ..a T U T )
: drop  ( ..a T -- ..a )
: nip   ( ..a T U -- ..a U )
```

**Rationale:**
- ✅ Familiar to Forth/Factor programmers
- ✅ Reads left-to-right (stack bottom → top)
- ✅ Double dash `--` visually separates before/after
- ✅ Matches cem2 (less porting work)
- ✅ Natural for concatenative thinking

**Alternatives rejected:**
- `(A -> B)` - Too ML-like, doesn't emphasize stack
- `'a 'S -> ...` - Cryptic, implicit row variable

### 2. Row Variables

**Syntax:** `..name` (double-dot prefix)

**Examples:**
```cem
: dup   ( ..a T -- ..a T T )         # One row variable
: swap  ( ..a T U -- ..a U T )       # Same row variable both sides
: if    ( ..a Bool -- ..b )          # Different row variables!
```

**Semantics:**
- `..a` represents "rest of stack below these values"
- Can be Empty, Cons, or unify with any StackType
- Same variable name = must unify (stack preserved)
- Different names = may differ (branches, etc.)

**Why `..` prefix:**
- ✅ Clearly distinct from type variables
- ✅ Suggests "variable number of things"
- ✅ Consistent with Factor
- ✅ Visually lightweight

**Alternatives rejected:**
- `'S` (Cat) - Looks like type variable, implicit position
- `*a` - Conflicts with multiplication operator
- `~a` - Less intuitive

### 3. Type Variables

**Syntax:** Uppercase names `T`, `U`, `V`, etc.

**Examples:**
```cem
: dup    ( ..a T -- ..a T T )
: swap   ( ..a T U -- ..a U T )
: pick   ( ..a T Int -- ..a T T )    # KNOWN LIMITATION (see below)
```

**Scoping:**
- Type variables are scoped to the stack effect
- Same name = must unify
- Different names = independent

**Capitalization convention:**
```
..a, ..b, ..rest  - Row variables (lowercase after ..)
T, U, V, Item     - Type variables (uppercase/capitalized)
int, bool         - Concrete types (lowercase) [future]
```

**Rationale:**
- ✅ Clear visual distinction from row variables
- ✅ Familiar from ML, Haskell, Rust
- ✅ Consistent with cem2

### 4. Quotation Types

**Syntax:** `[inputs -- outputs]` (square brackets)

**Examples:**
```cem
: call   ( ..a [..a -- ..b] -- ..b )
: map    ( ..a List<T> [T -- U] -- ..a List<U> )
: filter ( ..a List<T> [T -- Bool] -- ..a List<T> )
: while  ( ..a [..a -- ..b Bool] [..b -- ..a] -- ..c )
```

**Key features:**
- Nested stack effects using square brackets
- Quotations can be polymorphic: `[T -- U]`
- Quotations can have row variables: `[..a -- ..b]`
- This is where cem2 failed (TODO #10) - we'll fix it!

**Rationale:**
- ✅ Square brackets match quotation syntax: `[ code ]`
- ✅ Nested effects are explicit
- ✅ Enables higher-order functions
- ✅ Factor-style, proven to work

### 5. Parametric Types

**Syntax:** `TypeName<Args>`

**Examples:**
```cem
List<Int>           # List of integers
Option<String>      # Optional string
Result<T, Error>    # Result with polymorphic success type
HashMap<String, T>  # Map with string keys, polymorphic values
```

**Definition syntax (future):**
```cem
type List<T> =
  | Nil
  | Cons T (List<T>)
  ;

type Option<T> =
  | None
  | Some T
  ;
```

**Rationale:**
- ✅ Standard syntax (Rust, Java, C++)
- ✅ Clear nesting: `List<Option<Int>>`
- ✅ Consistent with cem2

---

## Type System Features

### Row Polymorphism (CRITICAL)

**What it is:**
Row polymorphism allows operations to work on stacks of varying depths by representing the "rest of stack" as a variable.

**Why it's critical for concatenative:**

**Without row polymorphism:**
```cem
: dup ( T -- T T )  # WRONG! What happened to rest of stack?

# This would mean:
[10, 20, 30]  dup  # Stack becomes [30, 30] - LOST 10, 20!
```

**With row polymorphism:**
```cem
: dup ( ..a T -- ..a T T )  # CORRECT! Preserves rest

# This means:
[10, 20, 30]  dup  # Stack becomes [10, 20, 30, 30] - Preserved 10, 20!
```

**How it works:**
1. `..a` unifies with `[10, 20]`
2. `T` unifies with `30`
3. Result: `..a T T` = `[10, 20, 30, 30]`

**This is what makes concatenative composition work!**

### Unification-Based Inference

**Algorithm:** Hindley-Milner style unification with row variables

**Example:**
```cem
: square ( Int -- Int ) dup * ;

# Type checking:
#   Start: ( Int -- ? )
#   After dup: ( Int -- Int Int )    [dup: ( ..a T -- ..a T T ) with ..a=Empty, T=Int]
#   After *: ( Int -- Int )            [*: ( ..a Int Int -- ..a Int )]
#   Unify with declared output: Int = Int ✓
```

**Substitution maps:**
```rust
type Substitution = HashMap<String, Type>;        // T -> Int
type StackSubstitution = HashMap<String, StackType>; // ..a -> [Int, String]
```

**Effect application:**
```rust
fn apply_effect(effect: &Effect, stack: StackType) -> StackType {
    // 1. Pop values matching effect.inputs
    // 2. Unify, generating substitutions
    // 3. Apply substitutions to effect.outputs
    // 4. Rebuild: remaining_stack + substituted_outputs
}
```

### Quotation Body Type Checking (Fix cem2 TODO #10)

**Problem in cem2:**
```cem
: broken ( Int -- String )
  [ 1 + ]  # Should fail! [Int -- Int] ≠ [Int -- String]
  call
;
```
This incorrectly type-checked because quotation bodies weren't analyzed.

**Solution for cem3:**
1. **Type-check quotation body** with empty initial stack
2. **Infer actual effect** from body
3. **Unify with expected effect** at usage site

**Example:**
```cem
: map ( List<T> [T -- U] -- List<U> ) ... ;

# Usage:
[1, 2, 3]  [ dup * ]  map

# Type checking:
#   1. Infer [ dup * ] type:
#      - Start with ( T -- ? )
#      - After dup: ( T -- T T )
#      - After *: ( T -- T ) [requires T supports *]
#      - Result: [T -- T] where T: Mul
#
#   2. Unify with map's expectation: [T -- U]
#      - T unifies with Int
#      - U unifies with Int
#      - Result: map : ( List<Int> [Int -- Int] -- List<Int> )
```

### Linearity Tracking (Copy vs Linear)

**Purpose:** Prevent resource bugs (double-free, use-after-move)

**Classification:**
```rust
trait Copy:   Int, Bool, Channel, Function      // Can dup without clone
trait Linear: String, List, Variant             // Requires explicit clone
```

**Type checking:**
```cem
: test ( String -- String String )
  dup  # ERROR! String is linear, use 'clone' first
;

: test-fixed ( String -- String String )
  clone dup  # OK! clone: ( ..a String -- ..a String String )
;
```

**Why this matters:**
- Prevents accidental string duplication (performance)
- Foundation for ownership/borrowing (future)
- Makes resource management explicit

**Conservative defaults:**
- Primitives (Int, Bool): Copy
- Heap-allocated (String, List, Variant): Linear
- Type variables: Assume Linear (safe)
- User types: Require trait annotation

---

## Phase 8.5 Implementation Plan

### Phase 1: Foundation (Session 1-2)

**Goal:** Port cem2's type system core

**Tasks:**
1. Create `compiler/src/types.rs` with Type/StackType/Effect enums
2. Create `compiler/src/unification.rs` with unify_types and unify_stack_types
3. Port substitution application logic
4. Add tests for unification

**Code to port:**
- `cem2/compiler/src/ast/types.rs` → `cem3/compiler/src/types.rs`
- `cem2/compiler/src/typechecker/unification.rs` → `cem3/compiler/src/unification.rs`

**Changes needed:**
- Update to current AST structure
- Fix TODOs (quotation checking)
- Add better error messages

**Success criteria:**
- Unification tests pass
- Type/StackType can represent cem3's types
- Effect composition works

### Phase 2: Type Checker (Session 2-3)

**Goal:** Implement type checking with effect application

**Tasks:**
1. Create `compiler/src/type_checker.rs`
2. Port effect application algorithm
3. Implement expression type checking
4. Add word definition checking
5. Integrate with existing compiler pipeline

**Key algorithms:**
```rust
fn check_expr(&self, expr: &Expr, stack: StackType) -> TypeResult<StackType>
fn apply_effect(&self, effect: &Effect, stack: StackType) -> TypeResult<StackType>
fn check_word_def(&mut self, word: &WordDef) -> TypeResult<()>
```

**Success criteria:**
- Basic programs type-check correctly
- Stack underflow detected at compile time
- Type mismatches caught
- Polymorphic operations work (dup, swap, etc.)

### Phase 3: Quotation Types (Session 3-4)

**Goal:** Fix cem2's TODO #10 - quotation body type checking

**Tasks:**
1. Implement quotation body analysis
2. Infer quotation effects
3. Unify quotation effects at call sites
4. Handle polymorphic quotations

**Example to support:**
```cem
: map ( List<T> [T -- U] -- List<U> )
  # Implementation uses call internally
;

: main ( -- )
  [1, 2, 3] [ dup * ] map  # Should infer List<Int>
;
```

**Success criteria:**
- Quotation bodies are type-checked
- Invalid quotations fail at compile time
- Higher-order functions work correctly
- map, filter, each all type-check

### Phase 4: Builtins & Stdlib (Session 4-5)

**Goal:** Add proper type signatures to all builtins and stdlib

**Tasks:**
1. Update `compiler/src/builtins.rs` with full Effect types
2. Test all stack operations (dup, swap, pick, etc.)
3. Test all arithmetic operations
4. Test stdlib functions (math, stack-utils)
5. Fix any type errors in stdlib

**Examples:**
```rust
// builtins.rs
sigs.insert("dup", Effect::new(
    StackType::RowVar("a").push(Type::Var("T")),
    StackType::RowVar("a").push(Type::Var("T")).push(Type::Var("T")),
));

sigs.insert("map", Effect::new(
    StackType::RowVar("a")
        .push(Type::Named { name: "List", args: vec![Type::Var("T")] })
        .push(Type::Quotation(Box::new(Effect::new(
            StackType::Empty.push(Type::Var("T")),
            StackType::Empty.push(Type::Var("U")),
        )))),
    StackType::RowVar("a")
        .push(Type::Named { name: "List", args: vec![Type::Var("U")] }),
));
```

**Success criteria:**
- All 124+ runtime tests still pass
- All operations have correct type signatures
- Stdlib type-checks without errors
- Examples compile and run correctly

### Phase 5: Error Messages (Session 5-6)

**Goal:** Make type errors helpful, not cryptic

**Bad error message:**
```
Type error: cannot unify ( Int String -- Bool ) with ( T U -- U )
```

**Good error message:**
```
Type error in word 'foo' at line 42:
  Expected stack: ( ..rest value -- ..rest Bool )
  Actual stack:   ( ..rest Int String -- ..rest Bool )

  Problem: 'swap' requires 2 values on stack, but only 1 was provided.

  Stack trace:
    1. After 'dup': ( Int -- Int Int )
    2. After '+': ( Int -- Int )
    3. Expected 'swap' input: ( T U -- U T )
    4. Cannot unify ( Int -- ? ) with ( T U -- U T )
```

**Tasks:**
1. Add source location tracking to Type/StackType
2. Implement stack trace collection during checking
3. Add pretty-printing for types and effects
4. Create helpful error messages with context
5. Test error messages for common mistakes

**Success criteria:**
- Errors show source location
- Errors explain what went wrong
- Errors suggest fixes when possible
- Errors show stack state progression

---

## Advanced Features (Post-Phase 8.5)

### Effect System (Kitten-inspired)

**Goal:** Track side effects in types

**Syntax:**
```cem
: pure-double ( ..a Int -- ..a Int )
  2 * ;

: print-double ( ..a Int -- ..a +IO )
  2 * write_line ;

: spawn-worker ( ..a [..b -- ..c] -- ..a +Concurrency )
  spawn ;
```

**Effects to track:**
```
+IO           - I/O operations (read_line, write_line, files)
+Concurrency  - Spawn strands, send/receive
+Unsafe       - FFI calls, pointer operations
+Fail         - Operations that can panic/abort
```

**Effect inference:**
```cem
: main ( -- +IO +Concurrency )
  "Starting server..." write_line       # Requires +IO
  [ handle_connection ] spawn            # Requires +Concurrency
;
```

**Benefits:**
- ✅ Makes side effects visible
- ✅ Prevents accidental I/O in pure code
- ✅ Enables compiler optimizations
- ✅ Better reasoning about code

**Complexity:**
- Moderate - effect polymorphism is tricky
- Need effect subtyping: `+IO` ⊆ `+IO +Fail`
- Need effect inference and checking

### Dependent-ish Types for `pick`

**Problem:** `pick` type depends on runtime value

**Current (incomplete):**
```cem
: pick ( ..a T Int -- ..a T T )  # Wrong! T depends on the Int value
```

**Ideal (dependent types):**
```cem
: pick ( ..a[T0, T1, ..., Tn] Int(n) -- ..a[T0, T1, ..., Tn] Tn )
```

**Practical compromise:**
- Accept that some operations can't be fully typed
- Add runtime checks (already done!)
- Document limitation (already done!)
- Maybe add refinement types later

**Low priority - pick is rarely used directly.**

### Refinement Types

**Goal:** More precise types for common cases

**Examples:**
```cem
NonNegativeInt    # Int where value >= 0
NonEmptyList<T>   # List<T> where length > 0
NonZeroInt        # Int where value != 0 (for division)
```

**Usage:**
```cem
: divide ( ..a Int NonZeroInt -- ..a Int )
  # Guaranteed no division by zero!
;

: head ( ..a NonEmptyList<T> -- ..a T )
  # Guaranteed no empty list error!
;
```

**Benefits:**
- ✅ Catch more errors at compile time
- ✅ Eliminate runtime checks where proven safe
- ✅ Self-documenting constraints

**Complexity:**
- High - requires constraint solving
- SMT solver integration?
- Type inference becomes much harder

**Future consideration - not Phase 8.5.**

---

## Open Questions

### Q1: Syntax for Stack Effect Comments

**Option A: Inline with definition**
```cem
: dup ( ..a T -- ..a T T )
  # implementation
;
```

**Option B: Separate declaration**
```cem
declare dup ( ..a T -- ..a T T )

: dup
  # implementation
;
```

**Option C: Optional annotations**
```cem
: dup   # Type inferred
  # implementation (must be unambiguous)
;

: complex-function ( ..a T U -- ..b V )
  # Type required because inference is ambiguous
;
```

**Recommendation:** Option A (inline) for simplicity, consider Option C later

### Q2: How Strict Should Linearity Be?

**Strict (Rust-style):**
```cem
: test ( String -- String )
  dup  # ERROR! Must clone first
;
```

**Permissive (Copy-on-use):**
```cem
: test ( String -- String )
  dup  # OK, implicitly clones
;
```

**Explicit (Require clone word):**
```cem
: test ( String -- String )
  clone dup  # clone: ( ..a String -- ..a String String )
;
```

**Recommendation:** Start with Explicit (matches cem2), consider warnings for Phase 2

### Q3: Type Annotations - Required or Optional?

**Always required:**
```cem
: dup ( ..a T -- ..a T T )  # MUST annotate every word
  # ...
;
```

**Always inferred:**
```cem
: dup   # Inferred: ( ..a T -- ..a T T )
  # ... (implementation must be unambiguous)
;
```

**Bidirectional (infer where possible):**
```cem
: dup   # Simple, can infer
  # ...
;

: complex ( ..a T U -- ..b V )  # Complex, must annotate
  # ...
;
```

**Recommendation:** Start with Always Required (simpler), add inference in Phase 2

---

## Success Criteria for Phase 8.5

**Must have:**
- ✅ Row polymorphism works correctly
- ✅ Stack effects type-check
- ✅ Quotation bodies are analyzed (fix TODO #10)
- ✅ Type errors are caught at compile time
- ✅ All current tests still pass
- ✅ Stdlib type-checks correctly

**Should have:**
- ✅ Good error messages with source locations
- ✅ Linearity tracked and enforced
- ✅ Polymorphic quotations work (map, filter, etc.)
- ✅ Documentation with type examples

**Nice to have:**
- ⚠️ Effect system (can wait for Phase 9)
- ⚠️ Refinement types (can wait for Phase 10)
- ⚠️ Full type inference (can wait for Phase 9)

---

## Conclusion

**The key insight:** Modern type systems and concatenative style are **compatible** when you:

1. ✅ **Embrace row polymorphism** - Not optional, fundamental
2. ✅ **Use stack-centric notation** - `( -- )`, not `A -> B`
3. ✅ **Make quotations first-class** - Nested effects, full checking
4. ✅ **Track linearity** - Prevents resource bugs
5. ✅ **Consider effects later** - Track I/O, concurrency, etc.

**We're not bolting ML onto Forth. We're building a type system that respects concatenative semantics.**

**Next steps:**
1. Review this design doc together
2. Refine any controversial decisions
3. Start Phase 1: Port cem2's type system foundation
4. Iterate based on what we learn

**Let's build a type system that makes concatenative programming better, not harder.**
