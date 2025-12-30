# Seq Glossary

A guide to concepts in Seq and concatenative programming. Written for working programmers who may not have encountered these ideas in traditional web/enterprise development.

---

## ADT (Algebraic Data Type)

A way to define custom types by combining simpler types. "Algebraic" because you build types using two operations:

- **Sum types** ("or"): A value is one of several variants. Like an enum.
- **Product types** ("and"): A value contains multiple fields together. Like a struct.

In Seq, you define ADTs with `union`:

```seq
# Sum type: a value is Either a Left OR a Right
union Either {
  Left { value: Int }
  Right { value: String }
}

# Option is a common pattern: either Some value or None
union Option {
  None
  Some { value: Int }
}
```

**Why it matters:** ADTs let you model your domain precisely. Instead of using null, magic numbers, or stringly-typed data, you define exactly what shapes your data can take. The compiler then ensures you handle all cases.

**History:** ADTs emerged from the ML family of languages in the 1970s (Robin Milner at Edinburgh). They became central to Haskell, OCaml, and F#. Rust's `enum` and Swift's `enum` with associated values are modern descendants. Most mainstream languages are only now catching up - Java added sealed classes and pattern matching in recent versions.

**Contrast with mainstream:** In Java/C#, you'd use inheritance hierarchies or wrapper classes. In JavaScript, you'd use objects with type fields and hope for the best. ADTs give you the modeling power with compile-time guarantees.

---

## Closure

A function bundled with the variables it captured from its surrounding scope.

```seq
: make-adder ( Int -- [ Int -- Int ] )
  [ i.+ ] ;  # The quotation captures the Int from the stack

5 make-adder    # Returns a closure that adds 5
10 swap call    # Result: 15
```

The quotation `[ i.+ ]` captures the `5` from the stack. When you `call` it later, it still has access to that captured value, even though `make-adder` has returned.

**Why it matters:** Closures enable functional patterns like callbacks, partial application, and higher-order functions. They're the building block for abstracting behavior.

**Contrast with mainstream:** JavaScript closures work similarly. Java has lambdas (with some restrictions). The difference in Seq is that captured values come from the stack, not named variables.

---

## Concatenative Programming

A programming paradigm where programs are built by composing functions in sequence. Each function takes its input from a stack and leaves its output on the stack.

```seq
# This program: take a number, double it, add 1, print it
dup i.+ 1 i.+ int->string io.write-line
```

Each word operates on whatever is on the stack. No variables, no argument lists - just a pipeline of transformations.

**Why it matters:** Concatenative code is highly composable. Any sequence of words can be extracted into a new word. Refactoring is trivial because there are no variable names to coordinate.

**History:** Concatenative programming was pioneered by **Charles Moore** with **Forth** (1970). Moore designed Forth to control radio telescopes at the National Radio Astronomy Observatory - he needed something small, fast, and interactive. Forth became popular in embedded systems, early personal computers, and even spacecraft (it powered the guidance system on several NASA missions). Other concatenative languages include PostScript (the PDF predecessor), Factor, and Joy.

**Contrast with mainstream:** Most languages are "applicative" - you apply functions to arguments: `print(add(1, double(x)))`. Notice how you read this inside-out. Concatenative code reads left-to-right, like a pipeline.

---

## Coroutine

A function that can pause its execution and resume later from where it left off.

Regular functions run to completion - they start, do their work, and return. Coroutines can *yield* control in the middle, let other code run, then continue from exactly where they paused.

```seq
# A coroutine that yields 1, 2, 3
: counter ( Ctx Int -- Ctx | Yield Int )
  1 yield drop
  2 yield drop
  3 yield drop
;
```

**Why it matters:** Coroutines enable cooperative multitasking, generators, and async-like patterns without the complexity of threads.

**History:** Coroutines were first described by **Melvin Conway** in 1963 - yes, the same Conway of "Conway's Law" (organizations design systems mirroring their communication structure). The concept predates threads! Simula (1967) had coroutines, and they were central to early Lisp implementations. Modern languages rediscovered coroutines: Python added generators in 2001, C# added iterators in 2005, and JavaScript added generators in 2015.

**Contrast with mainstream:** JavaScript has `async/await` (a limited form of coroutines). Python has generators with `yield`. Go's goroutines are similar but preemptively scheduled.

See also: [Generator](#generator-weave), [Yield](#yield), [Strand](#strand-green-thread)

---

## CSP (Communicating Sequential Processes)

A concurrency model where independent processes communicate by sending messages through channels, rather than sharing memory.

```seq
make-channel           # Create a channel
[ 42 swap chan.send ]  # Sender process
strand.spawn
chan.receive           # Receiver gets 42
```

The key insight: instead of multiple threads reading/writing shared variables (and needing locks), each process has its own state and communicates through explicit message passing.

**Why it matters:** CSP eliminates entire categories of concurrency bugs (race conditions, deadlocks from lock ordering). It's easier to reason about because communication points are explicit.

**History:** CSP was formalized by **Tony Hoare** in his 1978 paper "Communicating Sequential Processes." Hoare is one of the giants of computer science - he also invented quicksort, developed Hoare logic for program verification, and received the Turing Award in 1980. CSP influenced the Occam language (1983) for parallel computing, and Erlang's actor model is a close relative. Despite CSP's elegance, it remained mostly academic until **Go** (2009) made channels and goroutines first-class features. Go's success finally brought CSP to mainstream programming.

**Contrast with mainstream:** Java uses shared memory + locks. JavaScript is single-threaded with callbacks. Go popularized CSP with goroutines and channels. Seq follows Go's model.

---

## Fiber

See [Strand](#strand-green-thread).

---

## Generator (Weave)

A function that produces a sequence of values on demand, yielding one at a time rather than computing all values upfront.

In Seq, generators are called **weaves**:

```seq
# A generator that yields squares: 1, 4, 9, 16, ...
: squares ( Ctx Int -- Ctx | Yield Int )
  dup dup i.* yield drop   # yield n*n
  1 i.+ squares            # recurse with n+1
;

[ 1 swap squares ] strand.weave
0 strand.resume  # yields 1
0 strand.resume  # yields 4
0 strand.resume  # yields 9
```

**Why it matters:** Generators let you work with infinite or expensive sequences lazily. You only compute values as needed. Great for streaming data, pagination, or any producer/consumer pattern.

**Contrast with mainstream:** Python has generators with `yield`. JavaScript has generator functions (`function*`). Java has `Stream` (less flexible). Seq's weaves are bidirectional - you can send values back to the generator.

---

## Point-Free Programming

Writing functions without explicitly naming their arguments. Also called "tacit programming."

```seq
# Point-free: arguments are implicit on the stack
: double ( Int -- Int ) dup i.+ ;
: quadruple ( Int -- Int ) double double ;

# vs. "pointed" style in other languages:
# def quadruple(x): return double(double(x))
```

In Seq, point-free is the natural style because values live on the stack, not in named variables.

**Why it matters:** Point-free code emphasizes the *transformation* rather than the *data*. It's often more composable and can be easier to reason about once you're fluent.

**Contrast with mainstream:** Haskell programmers sometimes write point-free (using `.` for composition). Most languages require naming arguments. In Seq, you'd have to go out of your way to *not* be point-free.

---

## Quotation

A block of code that isn't executed immediately - it's a value you can pass around and execute later.

```seq
[ 1 i.+ ]           # A quotation that adds 1
dup                 # Now we have two copies of it
call                # Execute one copy
swap call           # Execute the other
```

Quotations are Seq's equivalent of lambdas/anonymous functions, but simpler - they're just deferred code.

**Why it matters:** Quotations enable higher-order programming. You can pass behavior as data, store it, compose it, execute it conditionally or repeatedly.

**Contrast with mainstream:** JavaScript arrow functions `x => x + 1`, Python lambdas `lambda x: x + 1`, Java lambdas `x -> x + 1`. The difference is Seq quotations don't declare parameters - they operate on whatever is on the stack.

---

## Resume

Continuing a paused generator/weave by sending it a value.

```seq
[ my-generator ] strand.weave   # Create weave, get handle
42 strand.resume                # Send 42, get yielded value back
```

Resume is the counterpart to yield. When the generator yields, it pauses. When you resume, you send a value *into* the generator and it continues from where it paused.

**Why it matters:** Bidirectional communication between caller and generator enables powerful patterns like coroutine-based state machines, interactive protocols, and pull-based data processing.

**Contrast with mainstream:** Python's `generator.send(value)`, JavaScript's `iterator.next(value)`. Many languages only support one-way generators that yield out but don't receive values in.

---

## Row Polymorphism

A type system feature that lets functions work with stacks of any depth, as long as they have the right types on top.

```seq
: add-one ( ..a Int -- ..a Int ) 1 i.+ ;
```

The `..a` is a "row variable" representing "whatever else is on the stack." This function works whether the stack has 1 element or 100 - it only cares about the `Int` on top.

**Why it matters:** Without row polymorphism, you'd need different versions of `add-one` for different stack depths, or lose type safety entirely. Row polymorphism gives you both flexibility and safety.

**History:** Row polymorphism was developed in the 1990s for typing extensible records (Mitchell Wand, 1989; Didier Rémy, 1994). It was adapted for stack-based languages by researchers working on typed Forth and later Joy. The key insight: a stack is just a record where fields are positions rather than names. Seq's type system builds on this work to provide safety without sacrificing the flexibility that makes concatenative programming powerful.

**Contrast with mainstream:** Most languages don't have this concept because they don't have stack-based semantics. It's similar to how generics let you write code that works with any type - row polymorphism lets you write code that works with any stack depth.

---

## Stack Effect

A function's type signature in Seq, describing what it takes from the stack and what it leaves.

```seq
: swap ( a b -- b a )     # Takes two values, returns them reversed
: dup  ( a -- a a )       # Takes one value, returns two copies
: drop ( a -- )           # Takes one value, returns nothing
: i.+  ( Int Int -- Int ) # Takes two Ints, returns one Int
```

The part before `--` is input (consumed from stack), after `--` is output (left on stack).

**Why it matters:** Stack effects are the contract of a function. The type checker verifies that functions compose correctly - if you chain `dup` then `i.+`, the types must line up.

**Contrast with mainstream:** Function signatures in other languages like `int add(int a, int b)`. Stack effects describe the *stack transformation* rather than named parameters.

---

## Stack Effect Chaining

The compiler automatically verifies that when you compose functions, the stack types line up correctly.

```seq
# These compose: dup outputs match i.+'s inputs
: double ( Int -- Int ) dup i.+ ;
#          ↑ Int      → ↑ Int Int → ↑ Int

# This would fail:
# : broken ( Int -- Int ) dup concat ;  # ERROR: concat expects strings!
```

The compiler traces the types through each operation, ensuring the pipeline is type-safe.

**Why it matters:** Concatenative code is extremely composable, but that power needs guardrails. Stack effect chaining catches type errors at compile time, even across complex compositions.

---

## Strand (Green Thread)

A lightweight unit of concurrent execution managed by the runtime, not the operating system.

```seq
[ do-work ] strand.spawn   # Start work in a new strand
```

Strands are much cheaper than OS threads (thousands are fine), and they cooperate by yielding at certain points rather than being preemptively interrupted.

**Why it matters:** You can have massive concurrency without the overhead of OS threads. Great for I/O-bound work like servers handling many connections.

**Contrast with mainstream:**
- **OS Threads** (Java, C++): Heavy, limited to hundreds/thousands, preemptively scheduled
- **Goroutines** (Go): Very similar to strands - lightweight, cooperatively scheduled
- **Async/await** (JavaScript, Python): Single-threaded concurrency via callbacks/promises
- **Fibers** (Ruby): Another name for the same concept

Seq's strands are most similar to Go's goroutines, running on top of the [May](https://github.com/Xudong-Huang/may) coroutine library.

---

## Tail Call Optimization (TCO)

A compiler technique that transforms recursive calls into loops, preventing stack overflow.

```seq
# Without TCO, this would overflow the stack for large n
: countdown ( Int -- )
  dup 0 i.> if
    dup int->string io.write-line
    1 i.- countdown   # Recursive call - but with TCO, no stack growth!
  else
    drop
  then ;

1000000 countdown  # Works fine - runs in constant stack space
```

When a function's last action is calling another function (a "tail call"), TCO reuses the current stack frame instead of creating a new one.

**Why it matters:** TCO makes recursion as efficient as iteration. You can write elegant recursive algorithms without worrying about stack overflow.

**History:** TCO was pioneered by **Guy Steele** and **Gerald Sussman** in the development of **Scheme** (1975). They proved that properly tail-recursive functions are equivalent to loops, making recursion a practical tool for iteration. Scheme was the first language to *require* TCO in its specification. This insight influenced functional programming for decades. Most mainstream languages still don't implement TCO - a 50-year-old optimization that remains cutting-edge!

**Contrast with mainstream:** Most languages don't guarantee TCO. Java and Python never do it. JavaScript has it in the spec but most engines don't implement it. Scheme requires it. Seq guarantees TCO using LLVM's `musttail` directive.

---

## Union

See [ADT](#adt-algebraic-data-type).

---

## Weave

Seq's term for a generator/coroutine that can yield values. See [Generator](#generator-weave).

The name evokes how the weave's execution "weaves" back and forth with the caller - yielding out, resuming in, yielding out again.

---

## Word

A named function in Seq. The term comes from Forth, where the dictionary of defined operations are called "words."

```seq
: greet ( -- )                           # Define a word
  "Hello, World!" io.write-line ;

greet                                    # Call the word
```

**Why "word"?** In concatenative languages, a program is literally a sequence of words (tokens). When you write `1 2 i.+`, you're writing three words. User-defined words are indistinguishable from built-in ones in usage.

**History:** The term comes from Forth, where Charles Moore conceived of programming as extending a language. In Forth, you build up a "dictionary" of words - starting with primitives and defining new words in terms of existing ones. Moore saw programming as fundamentally linguistic: you're not writing instructions for a machine, you're teaching the machine new vocabulary. This philosophy influenced Seq's design.

**Contrast with mainstream:** Same as "function," "method," or "procedure" in other languages. Seq uses "word" to honor the Forth tradition and because it emphasizes the linguistic nature of concatenative programming.

---

## Yield

Pausing a generator/weave and sending a value to the caller.

```seq
: fibonacci ( Ctx Int Int -- | Yield Int )
  over yield drop           # Yield current fib number
  tuck i.+ fibonacci        # Compute next and recurse
;

[ 0 1 fibonacci ] strand.weave
0 strand.resume  # yields 0
0 strand.resume  # yields 1
0 strand.resume  # yields 1
0 strand.resume  # yields 2
0 strand.resume  # yields 3
```

When the generator executes `yield`, it:
1. Sends a value to whoever called `strand.resume`
2. Pauses execution
3. Waits for the next `strand.resume` to continue

**Why it matters:** Yield enables lazy evaluation and producer/consumer patterns. The generator only does work when asked.

**Contrast with mainstream:** Python's `yield`, JavaScript's `yield`, C#'s `yield return`. Same concept, different syntax.

---

## Further Reading

- [Language Guide](./language-guide.md) - Full syntax and semantics
- [Type System Guide](./TYPE_SYSTEM_GUIDE.md) - Deep dive into Seq's type system
- [TCO Guide](./TCO_GUIDE.md) - How tail call optimization works
- [Architecture](./ARCHITECTURE.md) - System design and implementation
- [seqlings](https://github.com/navicore/seqlings) - Learn by doing with guided exercises
