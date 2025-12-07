# Algebraic Data Types (ADTs) Design Document

## Motivation

The current `Variant` type provides runtime flexibility but no compile-time safety:
- `variant-field-at` can access wrong index
- No type checking on field types
- Stack manipulation bugs (e.g., `make-report-msg` consuming wrong values)

We spent hours debugging stack position errors that ADTs would catch at compile time.

## Goals

1. **Compile-time safety** for structured data
2. **Preserve row polymorphism** for stack effects (orthogonal concerns)
3. **Support both point-free and named binding styles**
4. **Incremental adoption** - existing `Variant` code keeps working

## Design

### Union Type Definitions

```seq
union Message {
  Get { response-chan: Int }
  Increment { response-chan: Int }
  Report { op: Int, delta: Int, total: Int }
}
```

Compiler generates typed constructors:
- `Make-Get: ( Int -- Message )`
- `Make-Increment: ( Int -- Message )`
- `Make-Report: ( Int Int Int -- Message )`

### Pattern Matching

Two styles, both compile to identical code:

#### Stack-Based (Point-Free Purist)

All fields pushed to stack in declaration order:

```seq
: handle ( Message -- )
  match
    Get ->              # ( response-chan )
      send-response
    Increment ->        # ( response-chan )
      do-increment send-response
    Report ->           # ( op delta total )
      drop swap drop    # extract delta
      aggregate-add
  end
;
```

#### Named Bindings (Pragmatic)

Only requested fields, in specified order:

```seq
: handle ( Message -- )
  match
    Get { chan } ->         # ( chan )
      chan send-response
    Increment { chan } ->   # ( chan )
      do-increment chan send-response
    Report { delta } ->     # ( delta )
      delta aggregate-add
  end
;
```

### Key Properties

| Property | Stack-Based | Named Bindings |
|----------|-------------|----------------|
| Runtime cost | Zero | Zero (compiles to same code) |
| Type checking | Full | Full |
| Exhaustiveness | Required | Required |
| Field validation | N/A | Compiler verifies names |
| Stack after match | All fields, declaration order | Requested fields, specified order |

### Coexistence

Both styles can be used in the same codebase, even different arms of the same match:

```seq
# Legal - mix styles per function
: simple-handler ( Message -- )
  match
    Get -> send-response    # point-free
  end
;

: complex-handler ( Message -- )
  match
    Report { delta total } -> delta total process
  end
;
```

### Row Polymorphism Preserved

ADTs and row polymorphism are orthogonal:

```seq
union Option { Some { value: Int }, None }

# Row polymorphic in ..a, works with ADT
: unwrap-or ( ..a Option Int -- ..a Int )
  match
    Some { value } -> drop value
    None ->          # use default already on stack
  end
;

# Extra stack values pass through
"hello" Make-Some 42 0 unwrap-or   # ( "hello" 42 )
```

## Implementation Plan

### Phase 1: Parser + AST
- Add `union` keyword and syntax
- Add `match` expression syntax
- New AST nodes: `UnionDef`, `MatchExpr`, `Pattern`
- **CI passes, no behavioral changes**

### Phase 2: Type System
- Track union definitions in type environment
- `Union` type representation
- Field type tracking per variant
- **CI passes, no behavioral changes**

### Phase 3: Typed Constructors
- Generate `Make-*` constructors from union definitions
- Type check constructor arguments
- Old `make-variant-N` continues working
- **CI passes**

### Phase 4: Match Expression (Stack-Based)
- Implement stack-based pattern matching
- Exhaustiveness checking
- Type inference for stack after each arm
- **CI passes**

### Phase 5: Named Bindings
- Add `{ field1 field2 }` syntax
- Compile to stack operations
- Field name validation
- **CI passes**

### Phase 6: Migration
- Migrate examples one at a time
- Each example is separate PR
- Document patterns and idioms
- **CI passes throughout**

## Backwards Compatibility

- Existing `Variant` type remains for dynamic use cases
- `make-variant-N` functions unchanged
- `variant-field-at` unchanged
- Old code compiles and runs without modification
- Migration is opt-in, per-module

## Open Questions

1. **Syntax bikeshedding**: `union` vs `type` vs `data`?
2. **Generic unions**: `Option[T]` with type parameters?
3. **Nested patterns**: `Some { Point { x y } }` - needed?
4. **Guards**: `match ... when condition ->` - needed?

## References

- Rust enums: https://doc.rust-lang.org/book/ch06-00-enums.html
- Factor's approach: https://docs.factorcode.org/
- PureScript row polymorphism: https://github.com/purescript/documentation

---

*Created during actor_counters.seq debugging session, December 2024*
*After spending hours debugging `make-report-msg` stack corruption*
