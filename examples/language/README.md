# Language Features

Core Seq language concepts demonstrated through focused examples.

## Stack Effects (stack-effects.seq)

Stack effect declarations and how the type checker enforces them:

```seq
: square ( Int -- Int ) dup i.* ;
```

## Quotations (quotations.seq)

Anonymous code blocks that can be passed around and called:

```seq
: apply-twice ( Int { Int -- Int } -- Int )
  dup rot swap call swap call ;

5 [ 2 i.* ] apply-twice  # Result: 20
```

## Closures (closures.seq)

Quotations that capture values from their environment:

```seq
: make-adder ( Int -- { Int -- Int } )
  { i.+ } ;

10 make-adder  # Creates a closure that adds 10
5 swap call    # Result: 15
```

## Control Flow (control-flow.seq)

Conditionals, pattern matching, and loops:

```seq
: fizzbuzz ( Int -- String )
  dup 15 i.mod 0 i.= if drop "FizzBuzz"
  else dup 3 i.mod 0 i.= if drop "Fizz"
  else dup 5 i.mod 0 i.= if drop "Buzz"
  else int->string
  then then then ;
```

## Recursion (recursion.seq)

Tail-recursive algorithms with guaranteed TCO:

```seq
: factorial-acc ( Int Int -- Int )
  over 0 i.<= if nip
  else swap dup rot i.* swap 1 i.- swap factorial-acc
  then ;

: factorial ( Int -- Int ) 1 factorial-acc ;
```

## Strands (strands.seq)

Lightweight concurrent execution:

```seq
[ "Hello from strand!" io.write-line ] strand.spawn
```

## Include Demo (main.seq, http_simple.seq)

Demonstrates the module include system for code organization.
