# Weaves: Generators and Coroutines

Weaves are Seq's implementation of generators/coroutines, built on top of the CSP-style strand system. They provide bidirectional communication between a producer and consumer through structured yield/resume semantics.

## Why Weaves?

Generators are useful when you need to:
- Produce values lazily (only compute when needed)
- Maintain state between iterations without explicit data structures
- Transform streams of data with backpressure
- Implement query engines or interpreters that yield results incrementally

Seq's weaves are unique in that they're built on the same strand/channel infrastructure as CSP concurrency. A weave is essentially a strand with a structured protocol for yield and resume.

## Basic Concepts

| Term | Description |
|------|-------------|
| **Weave** | A suspended computation that yields values and receives resume values |
| **Handle** | A `WeaveCtx` value used to resume and communicate with a weave |
| **Yield** | Pause execution, send a value to the caller, wait for a resume value |
| **Resume** | Send a value into a paused weave, receive its next yielded value |

## Core Operations

| Word | Effect | Description |
|------|--------|-------------|
| `strand.weave` | `( Quotation -- WeaveCtx )` | Create a weave from a quotation |
| `strand.resume` | `( WeaveCtx T -- WeaveCtx T Bool )` | Resume with value, get (handle, yielded, has_more) |
| `yield` | `( Ctx T -- Ctx T \| Yield T )` | Yield value, receive resume value |
| `strand.weave-cancel` | `( WeaveCtx -- )` | Cancel a weave and release resources |

## Simple Counter Example

A counter that yields its current value and accepts an increment:

```seq
# Counter - yields current count, receives increment
: counter ( Ctx Int -- | Yield Int )
  tuck           # ( count Ctx count )
  yield          # yield count, receive increment -> ( count Ctx increment )
  rot            # ( Ctx increment count )
  i.add          # ( Ctx new_count )
  counter        # tail recurse
;

: main ( -- )
  # Create weave
  [ counter ] strand.weave        # ( handle )

  # Resume with initial value 10
  10 strand.resume                # ( handle yielded has_more )
  drop                            # ( handle yielded )
  "First: " swap int->string string.concat io.write-line
                                  # ( handle )

  # Resume with increment 5
  5 strand.resume                 # ( handle yielded has_more )
  drop
  "After +5: " swap int->string string.concat io.write-line

  strand.weave-cancel             # Clean up (infinite generator)
;
```

Output:
```
First: 10
After +5: 15
```

## The Ctx Threading Pattern

**Critical:** The weave context (`Ctx`) must be explicitly threaded through your code. This is different from languages where generator state is implicit.

The quotation passed to `strand.weave` receives `(Ctx, first_resume_value)` and must keep the `Ctx` accessible for `yield` calls:

```seq
# WRONG - loses the Ctx
: bad-generator ( Ctx Int -- | Yield Int )
  yield          # Error: Ctx is buried under Int
;

# CORRECT - Ctx is on top for yield
: good-generator ( Ctx Int -- | Yield Int )
  tuck           # ( Int Ctx Int )
  yield          # ( Int Ctx resume_value )
  ...
;
```

## Handling Weave Completion

`strand.resume` returns `( handle value has_more )`. The boolean indicates whether the weave yielded (`true`) or completed (`false`):

```seq
: finite-generator ( Ctx Int -- | Yield Int )
  dup 0 i.<= if
    drop drop    # Done - just return, weave ends
  else
    tuck yield   # Yield current value
    rot 1 i.-    # Decrement
    finite-generator
  then
;

: main ( -- )
  [ finite-generator ] strand.weave
  3 strand.resume    # ( handle 3 true )
  drop drop          # ( handle )
  0 strand.resume    # ( handle 2 true )
  drop drop
  0 strand.resume    # ( handle 1 true )
  drop drop
  0 strand.resume    # ( handle 0 false ) - weave completed!
  if
    "More values" io.write-line
  else
    drop "Weave finished" io.write-line
  then
  drop  # drop handle
;
```

## Resource Management

Weaves hold resources (channels internally). You must either:

1. **Resume to completion** - Keep resuming until `has_more` is false
2. **Cancel explicitly** - Call `strand.weave-cancel` to release resources

```seq
# BAD - resource leak!
[ infinite-generator ] strand.weave
10 strand.resume drop drop drop  # Weave abandoned, resources leak

# GOOD - explicit cancellation
[ infinite-generator ] strand.weave
10 strand.resume drop drop
strand.weave-cancel              # Clean up
```

The linter will warn about immediate drops of weave handles.

## Type System Integration

The `Yield` effect appears in stack effect annotations:

```seq
: my-generator ( Ctx Int -- | Yield Int )
  #                      ^^^^^^^^^^^ Yield effect
  ...
;
```

This tells the type checker that the word participates in generator semantics. The effect propagates through callers.

## Advanced: Structured Data

Weaves work well with union types for rich producer/consumer protocols:

```seq
union SensorReading {
  Reading { temp: Int, status: String }
}

: sensor-processor ( Ctx SensorReading -- | Yield SensorReading )
  # Transform the reading
  dup 0 variant.field-at    # Get temp
  classify-temp             # Classify it
  swap 1 variant.field-at   # Get status
  Make-Reading              # Create new reading

  swap yield                # Yield result, get next reading
  sensor-processor          # Process next
;
```

## Comparison to Other Languages

| Feature | Seq Weaves | Python Generators | JavaScript Generators |
|---------|------------|-------------------|----------------------|
| Bidirectional | Yes (`yield`/`resume`) | Yes (`send()`) | Yes (`next(value)`) |
| Context | Explicit stack threading | Implicit | Implicit |
| Concurrency | Built on strands/channels | Single-threaded | Single-threaded |
| Cancellation | `strand.weave-cancel` | Close iterator | `return()` |
| Type system | Yield effect tracked | Untyped | Untyped |

## When to Use Weaves vs Strands

| Use Case | Mechanism |
|----------|-----------|
| Independent concurrent tasks | `strand.spawn` |
| Producer/consumer with backpressure | Weaves |
| Request/response patterns | Weaves |
| Fire-and-forget parallelism | `strand.spawn` |
| Lazy sequences | Weaves |
| Stream transformations | Weaves |

## See Also

- [Concurrency](language-guide.md#concurrency) - Strands and channels
- [examples/weave/](https://github.com/navicore/patch-seq/tree/main/examples/weave) - Working examples
