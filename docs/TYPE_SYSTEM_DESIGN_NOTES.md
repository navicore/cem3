# Type System Design Notes - Phase 8.5

## Review of cem2's Type System

### What Worked ✅

1. **Row Polymorphism Implementation**
   - `StackType` enum with three variants:
     - `Empty`: Known empty stack
     - `Cons {rest, top}`: Type on top of rest (recursive)
     - `RowVar(String)`: Row variable for "rest of stack"
   - This is the **key innovation** for concatenative type systems
   - Example: `dup: (..a T -- ..a T T)` where `..a` is a RowVar

2. **Stack as Recursive Type**
   - Stacks are cons-lists of types
   - Clean operations: `push()`, `pop()`, `from_vec()`
   - Natural representation for stack-based semantics

3. **Effect Signatures**
   - `Effect { inputs: StackType, outputs: StackType }`
   - Represents stack transformations: `(inputs -- outputs)`
   - Can be composed: output of first matches input of second

4. **Bidirectional Type Checking**
   - Words declare their effects upfront
   - Type checker verifies body matches declaration
   - Standard approach for typed languages

5. **Unification Algorithm**
   - Hindley-Milner style unification
   - Type variables: `Var(String)`
   - Substitutions: `HashMap<String, Type>`
   - Stack substitutions: `HashMap<String, StackType>` for row vars
   - Row variables unify with any stack type

6. **Type System Structure**
   - `Type` enum: Int, Bool, String, Var, Named, Quotation
   - Named types support type arguments: `Option<T>`
   - Environment tracks word signatures

### What Didn't Work ❌

1. **CRITICAL: Quotation Types Are Unsound**
   - All quotations inferred as `[ -- ]` regardless of contents
   - From cem2 code (checker.rs:99-100):
     ```
     // KNOWN LIMITATION: Currently all quotations have type [ -- ] regardless
     // of their actual contents. This is a soundness hole in the type system.
     ```
   - Result: **Any** quotation can be passed where any other is expected

2. **Quotation Unification Unimplemented**
   - From unification.rs:69-83:
     ```
     // TODO(#10): Implement effect unification
     //
     // KNOWN LIMITATION: Any two quotation types unify successfully, even if
     // they have incompatible effects. This compounds the soundness hole from
     // quotation type inference.
     //
     // Example that should fail but won't:
     //   : takes-int-to-int ( [Int -- Int] -- ) drop ;
     //   : main ( -- ) [ "hello" write_line ] takes-int-to-int ;
     ```
   - Any two quotation types unify, even with incompatible effects

3. **No Type Inference for Quotations**
   - Quotation bodies aren't type-checked
   - Must be explicitly typed, but even then types aren't verified

4. **Linear Types Not Enforced**
   - String marked as "Linear - not Copy" in comments
   - But no runtime or compile-time enforcement
   - Values can be duplicated/dropped without errors

### Key Insights

**Foundation is Excellent:**
The row polymorphism design is **exactly right**. `StackType` with `Cons` and `RowVar`
is the correct representation. Unification works beautifully.

**Main Problem: Quotations**
The type system falls apart at quotations (first-class functions). This is the hard
part of concatenative type systems and cem2 never solved it.

**Why Quotations Are Hard:**
- Quotations capture the current stack state
- They can be passed around and called later
- Their type depends on what's on the stack when they're created
- Need to track "stack at quotation definition" vs "stack at quotation call"

## Recommendations for cem3

### Phase 1: Basic Type Inference (No Quotations)
Start simple, get it working:
1. Implement row polymorphism for basic stack operations
2. Type literals (Int, String, Bool)
3. Type built-in operations
4. Infer types for user-defined words
5. **Skip quotations initially** - mark as `TODO` like cem2 did

### Phase 2: Quotation Types (Research Required)
This is the hard part. Options:
1. **Simple approach**: Quotations must be explicitly typed, verify body matches
2. **Full inference**: Type quotation bodies, handle captured stack state
3. **Research Kitten/Factor**: See how they solve this

### Phase 3: Linear Types (If Needed)
Decide if we need linear types or if Rust's ownership is enough.

## Design Decisions

### Keep from cem2:
- ✅ Row polymorphism with `RowVar`
- ✅ Stack as recursive cons-list
- ✅ Effect signatures `(inputs -- outputs)`
- ✅ Bidirectional type checking
- ✅ Unification with substitutions

### Improve from cem2:
- ❌ **Don't punt on quotations** - either solve it or explicitly limit usage
- ✅ Better error messages (cem2's are verbose but unclear)
- ✅ Simpler representation if possible

### Open Questions:
1. Do we need linear types, or is Rust's ownership sufficient?
2. How important are quotations? Can we ship without them?
3. Should we support type inference or require all effects declared?

## Survey of Other Concatenative Type Systems

### Factor (factorcode.org)
- **Approach**: Declared stack effects, not inference
- Stack effects must be explicitly declared: `( inputs -- outputs )`
- Compiler verifies code matches declared effects
- **Quotations**: Must be explicitly typed
- **Polymorphism**: Limited - effects must be concrete
- **Key insight**: "very simple and permissive type system"
- Works well for practical programming but not as expressive as full inference

### Kitten (kittenlang.org)
- **Approach**: Hindley-Milner type inference
- Uses row polymorphism for stack types
- Effect types to control side-effects
- **Status**: Development appears stalled/limited
- **Key insight**: Attempted full inference with HM, but implementation incomplete

### Cat Language
- Similar to Kitten's approach
- Row polymorphism on stack types
- "Types describe the effect of a function on a stack"
- Every function requires well-typed stack, generates well-typed stack

### Research Literature
- Stack effects formalized by Jaanus Pöial (early 1990s)
- Two main approaches:
  1. **Stack Effects** - algebraic formalization (Factor, Joy)
  2. **Nested Pairs** - functional programming approach (Okasaki 1993)
- **Common problem**: Quotation typing is hard
  - Most languages either don't type quotations fully
  - Or require explicit type annotations

### Key Takeaways

1. **Factor's approach works**: Declared effects are practical and understandable
2. **Full inference is hard**: Kitten attempted it, implementation incomplete
3. **Quotations are the hard part**: Every language struggles here
4. **Row polymorphism is standard**: All typed concatenative languages use it

## Design Decisions for cem3

### Decision 1: Declared vs Inferred Effects

**Options:**
- **A**: Full inference (like Kitten attempted)
- **B**: Declared effects (like Factor)
- **C**: Hybrid: infer where possible, require declaration for exports

**Recommendation: Option B (Declared Effects)**

**Rationale:**
1. Factor proves this works in practice
2. Simpler implementation - can ship sooner
3. Better error messages - mismatch against declared intent
4. Easier for users to understand
5. Can add inference later if needed

**Example:**
```cem
: dup ( ..a T -- ..a T T )
  # implementation
;
```

### Decision 2: Row Polymorphism

**Decision: YES - Use row polymorphism**

- This is the correct approach (proven by cem2, Factor, Kitten)
- Stack types: `Empty | Cons {rest, top} | RowVar`
- Allow `..a` notation for "rest of stack"

### Decision 3: Quotation Strategy

**Options:**
- **A**: Skip quotations (Phase 1)
- **B**: Simple typing - quotations must be explicitly typed
- **C**: Full inference - type quotation bodies

**Recommendation: Start with A, evolve to B**

**Phase 1**: No quotations at all
- Focus on getting basic row polymorphism working
- Type all built-ins and user words

**Phase 2** (later): Simple quotation typing
- Quotations have type `[ StackIn -- StackOut ]`
- Must be explicitly typed
- Body checked against type
- Don't attempt to infer captured stack state

### Decision 4: Type Syntax

**Stack Effect Declaration:**
```cem
: word-name ( ..rest Int String -- ..rest Bool )
  # body
;
```

Components:
- `..rest` - row variable
- `Int`, `String`, `Bool` - concrete types
- `--` separates inputs from outputs

**Type Grammar:**
```
Type        := BaseType | TypeVar | RowVar
BaseType    := "Int" | "Bool" | "String" | UserType
TypeVar     := UppercaseName
RowVar      := ".." LowercaseName
StackEffect := "(" TypeList "--" TypeList ")"
TypeList    := (Type)*
```

## Implementation Plan

### Phase 1: Basic Types + Row Polymorphism (1-2 weeks)

1. **Extend AST** with type annotations
2. **Implement `Type` enum**: Int, Bool, String, TypeVar
3. **Implement `StackType`**: Empty, Cons, RowVar
4. **Implement `Effect`**: inputs, outputs
5. **Add stack effect parser**: parse `( ..a Int -- ..a Bool )`
6. **Type check** against declared effects
7. **Test** with all built-ins

**Success Criteria:**
- Can declare stack effects for words
- Type checker verifies implementations match declarations
- Row polymorphism works for `dup`, `swap`, etc.

### Phase 2: User-Defined Types (1 week)

1. Add `Type::Named` for user-defined ADTs
2. Support type parameters (generics)
3. Type check variant construction/destructuring

### Phase 3: Quotations (TBD - Research)

**Don't rush this.** Get Phase 1 & 2 solid first.

Options to explore:
- Explicit typing only
- Inference for simple cases
- Research more advanced techniques

## Next Steps

1. Create design document summarizing decisions
2. Get user approval on approach
3. Start Phase 1 implementation
4. Write comprehensive tests
