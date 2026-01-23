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

**lists.seq** - Higher-order functions and list processing:

```seq
# Built-in higher-order functions
list-of 1 lv 2 lv 3 lv 4 lv 5 lv
  [ 2 i.* ] list.map       # (2 4 6 8 10)
  [ 2 mod 0 i.= ] list.filter  # keep evens
  0 [ i.+ ] list.fold      # sum

# Functional pipelines
list-of 1 lv 2 lv 3 lv 4 lv 5 lv 6 lv 7 lv 8 lv 9 lv 10 lv
  keep-odds      # filter to 1,3,5,7,9
  square-each    # map to 1,9,25,49,81
  sum            # fold to 165
```

Features:
- **map**: Transform each element with a quotation
- **filter**: Keep elements matching a predicate
- **fold**: Reduce list to single value with accumulator
- Composable operations for data pipelines

## Logic (logic/)

*Coming soon* - Backtracking, unification patterns.

## Dataflow (dataflow/)

*Coming soon* - Reactive and stream-based patterns.
