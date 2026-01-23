# Programming Paradigms

Seq is flexible enough to express multiple programming paradigms. These examples demonstrate different approaches to structuring programs.

## Object-Oriented (oop/)

**shapes.seq** - OOP patterns using unions and pattern matching:

- Encapsulation: data bundled in union variants
- Polymorphism: pattern matching dispatches to correct implementation
- Factory functions as constructors
- Type checks via `variant.tag` (like `instanceof`)

```seq
union Shape {
  Circle { radius: Float }
  Rectangle { width: Float, height: Float }
}

: shape.area ( Shape -- Float )
  match
    Circle { >radius } -> dup f.* 3.14159 f.*
    Rectangle { >width >height } -> f.*
  end ;
```

## Actor Model (actor/)

**actor_counters.seq** - CSP/Actor demonstration with hierarchical aggregation:

```
Company (aggregate)
  └── Region (aggregate)
        └── District (aggregate)
              └── Store (counter)
```

Features:
- Independent strands communicate via channels
- HTTP interface for queries and updates
- Request-response pattern with response channels

**counter.seq** - Simple generator pattern using weaves.

**sensor-classifier.seq** - Stream processing with structured data.

## Functional (functional/)

*Coming soon* - Pure functional patterns, composition, immutability.

## Logic (logic/)

*Coming soon* - Backtracking, unification patterns.

## Dataflow (dataflow/)

*Coming soon* - Reactive and stream-based patterns.
