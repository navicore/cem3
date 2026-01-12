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

**History:** ADTs emerged from the ML family of languages in the 1970s (Robin Milner at Edinburgh). They became central to Haskell, OCaml, and F#. Rust's `enum` and Swift's `enum` with associated values are modern descendants.

**In other languages:** Java traditionally uses inheritance hierarchies; Java 17+ added sealed classes and pattern matching. C# has similar recent additions. Rust and Swift have ADTs as core features. In JavaScript/TypeScript, discriminated unions with type fields achieve a similar pattern.

**Disambiguation:** "ADT" also stands for "Abstract Data Type" (Barbara Liskov, CLU, 1974) - a different concept about encapsulation and defining types by their operations rather than their representation. Abstract data types influenced object-oriented programming. Seq uses ADT in the *algebraic* sense from ML, not the abstract sense from CLU.

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

**In other languages:** JavaScript closures work similarly. Java 8+ has lambdas (with some restrictions on captured variables). Python has closures with `nonlocal`. The difference in Seq is that captured values come from the stack, not named variables.

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

**In other languages:** Most languages are "applicative" - you apply functions to arguments: `print(add(1, double(x)))`. Notice how you read this inside-out. Concatenative code reads left-to-right. Unix pipes (`cat file | grep pattern | wc -l`) follow a similar compositional style. Elixir's `|>` operator and F#'s pipeline operator provide this within applicative languages.

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

**In other languages:** Python has generators with `yield` and `async/await`. JavaScript has `async/await` and generator functions. C# has `yield return` for iterators and `async/await`. Go's goroutines are similar but preemptively scheduled. Kotlin has coroutines as a library feature.

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

**History:** CSP was formalized by **Tony Hoare** in his 1978 paper "Communicating Sequential Processes." Hoare is one of the giants of computer science - he also invented quicksort, developed Hoare logic for program verification, and received the Turing Award in 1980. CSP influenced the Occam language (1983) for parallel computing, and Erlang's actor model is a close relative. **Go** (2009) made channels and goroutines first-class features, bringing CSP to wide adoption.

**In other languages:** Go has goroutines and channels as core features. Erlang and Elixir use the related Actor model with message-passing processes. Rust has channels in its standard library. Java traditionally uses shared memory with locks; Java 21 added virtual threads. JavaScript is single-threaded and uses callbacks/promises for async work.

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

**In other languages:** Python has generators with `yield` and `send()` for bidirectional communication. JavaScript has generator functions (`function*`) with `next(value)`. C# has `IEnumerable` with `yield return`. Java has `Stream` for lazy sequences. Seq's weaves support bidirectional communication - you can send values back to the generator with each resume.

---

## NaN-Boxing

A memory optimization technique that encodes multiple value types within a single 64-bit word by exploiting the unused bits in IEEE 754 floating-point NaN (Not a Number) representations.

IEEE 754 doubles use 64 bits: 1 sign, 11 exponent, 52 mantissa. When the exponent is all 1s and the mantissa is non-zero, the value is NaN. The "quiet NaN" range (with the top mantissa bit set) provides ~51 bits of payload that hardware ignores - perfect for smuggling in pointers, integers, or type tags.

```
Normal f64:    stored directly (bit pattern < 0xFFF8...)
NaN-boxed Int: 0xFFF8 | (tag << 47) | (47-bit payload)
NaN-boxed Ptr: 0xFFF8 | (tag << 47) | (pointer)
```

**Why it matters:** NaN-boxing shrinks value representation from multi-word tagged unions to a single 64-bit word. This improves cache utilization, reduces memory bandwidth, and enables faster stack operations. JavaScript engines (V8, SpiderMonkey) use variants of this technique for significant performance gains.

**The tradeoff:** Integers are limited to ~47-51 bits (depending on tag space), not full 64-bit. This breaks operations expecting i64::MIN/MAX (like `1 63 shl`). Languages using NaN-boxing either accept this limit (JavaScript's 53-bit integers via f64 mantissa) or fall back to heap-allocated "BigInt" for large values.

**History:** The technique emerged from Lisp implementations in the 1970s-80s, where tagged pointers were common. Modern popularization came from LuaJIT (Mike Pall, 2005) and JavaScript engines. WebKit's JavaScriptCore, Mozilla's SpiderMonkey, and early V8 all use NaN-boxing or similar "pointer tagging" schemes.

**In other languages:** LuaJIT uses NaN-boxing extensively. JavaScript engines use it for the `number` type. Ruby's CRuby uses tagged pointers (a related technique). OCaml uses tagged integers with the low bit. Seq explored NaN-boxing but currently uses 40-byte tagged values to preserve full 64-bit integer range.

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

**In other languages:** Haskell supports point-free style using `.` for composition. APL and J are famously point-free. Most languages require naming arguments explicitly. In Seq, point-free is the default style since values live on the stack.

---

## Pattern Matching

A control flow mechanism that branches based on the *structure* of data, often extracting values in the process.

Seq has two forms of pattern matching:

### 1. `match` - ADT Destructuring

Used with union types to branch on variants and extract fields:

```seq
union Option {
  None
  Some { value: Int }
}

: describe ( Option -- )
  match
    None -> "Nothing here" io.write-line
    Some { >value } ->
      int->string "Got: " swap string.concat io.write-line
  end
;
```

The compiler verifies **exhaustiveness** - you must handle all variants. If you add a variant to the union, all `match` expressions must be updated.

### 2. `cond` - Predicate-Based Dispatch

Used for conditional branching based on boolean tests:

```seq
: classify ( Int -- String )
  cond
    [ dup 0 i.< ] -> drop "negative"
    [ dup 0 i.= ] -> drop "zero"
    [ true ]      -> drop "positive"
  end
;
```

Each branch has a quotation that produces a Bool. The first true branch executes. This is closer to Lisp's `cond` than structural pattern matching.

**Why it matters:** Pattern matching replaces chains of if/else with declarative structure. The compiler can check exhaustiveness, catching bugs when data structures change.

**History:** Pattern matching originated in ML (1970s) and became central to Haskell, OCaml, and Erlang. It's now spreading widely - Rust has `match`, Scala has `case`, Swift has `switch` with associated values, Python added structural pattern matching in 3.10, and Java added pattern matching in recent versions.

**In other languages:** Rust has `match` with exhaustiveness checking. Haskell and OCaml have pattern matching as a core feature. Scala has `case` classes and `match`. Elixir inherits pattern matching from Erlang. Python 3.10+ has `match`/`case`. Java 21 has pattern matching for `switch`. JavaScript does not have pattern matching natively.

See also: [ADT](#adt-algebraic-data-type)

---

## Quotation

A block of code that isn't executed immediately - it's a value you can pass around and execute later. Also known as an **anonymous function** or **lambda** in other languages.

```seq
[ 1 i.+ ]           # A quotation that adds 1
dup                 # Now we have two copies of it
call                # Execute one copy
swap call           # Execute the other
```

Quotations are Seq's equivalent of lambdas/anonymous functions, but simpler - they're just deferred code.

**Why it matters:** Quotations enable higher-order programming. You can pass behavior as data, store it, compose it, execute it conditionally or repeatedly.

**History:** The `[ ]` quotation syntax comes from **Factor** (2003, Slava Pestov), a modern concatenative language that refined many ideas from Forth. Factor demonstrated that concatenative languages could have rich type systems, garbage collection, and modern tooling. Joy (1990s, Manfred von Thun) also used quotations extensively and influenced Factor's design.

**In other languages:** JavaScript has arrow functions `x => x + 1`. Python has `lambda x: x + 1`. Java 8+ has lambdas `x -> x + 1`. Ruby has blocks and procs. The difference is Seq quotations don't declare parameters - they operate on whatever is on the stack.

---

## Resume

Continuing a paused generator/weave by sending it a value.

```seq
[ my-generator ] strand.weave   # Create weave, get handle
42 strand.resume                # Send 42, get yielded value back
```

Resume is the counterpart to yield. When the generator yields, it pauses. When you resume, you send a value *into* the generator and it continues from where it paused.

**Why it matters:** Bidirectional communication between caller and generator enables powerful patterns like coroutine-based state machines, interactive protocols, and pull-based data processing.

**In other languages:** Python has `generator.send(value)`. JavaScript has `iterator.next(value)`. Lua coroutines have `coroutine.resume(co, value)`. Some languages only support one-way generators that yield out but don't receive values in.

---

## Single Assignment / Immutable Bindings

A language property where variables can only be bound once - you can't reassign them after the initial binding.

```erlang
% Erlang example - single assignment
X = 5,
X = 6.  % ERROR! X is already bound
```

This leads to a characteristic style where you thread transformed values through new names:

```erlang
X1 = transform(X),
X2 = transform(X1),
X3 = transform(X2).
```

**How Seq relates:** Seq takes this further - there are no variable names at all. Values live on the stack and flow through transformations without being named. The "juggling" that Erlang programmers do with variable names (`X1`, `X2`, `X3`) becomes stack manipulation in Seq (`dup`, `swap`, `rot`, `over`).

```seq
# No names to juggle - values flow through the stack
transform transform transform
```

Both approaches enforce thinking about data flow rather than state mutation. The stack is essentially single-assignment taken to its logical conclusion: values don't even need names, they just flow.

**Why it matters:** Understanding single assignment helps explain why concatenative programming feels different. You're not mutating variables - you're transforming values. Stack manipulation is the mechanism for managing those transformations without names.

**History:** Single assignment was central to **Erlang** (1986, Ericsson) where immutability enables reliable concurrent systems. Haskell enforces immutability at the type level. Clojure brought immutable data structures to the JVM. The concept traces back to declarative and logic programming (Prolog).

**In other languages:** Erlang and Elixir enforce single assignment. Haskell variables are immutable by default. Rust has immutable bindings by default (`let` vs `let mut`). JavaScript's `const` and Java's `final` provide opt-in immutability. Most languages allow reassignment by default.

---

## Row Polymorphism

A type system feature that lets functions work with stacks of any depth, as long as they have the right types on top.

```seq
: add-one ( ..a Int -- ..a Int ) 1 i.+ ;
```

The `..a` is a "row variable" representing "whatever else is on the stack." This function works whether the stack has 1 element or 100 - it only cares about the `Int` on top.

**Why it matters:** Without row polymorphism, you'd need different versions of `add-one` for different stack depths, or lose type safety entirely. Row polymorphism gives you both flexibility and safety.

**History:** Row polymorphism was developed in the 1990s for typing extensible records (Mitchell Wand, 1989; Didier Rémy, 1994). It was adapted for stack-based languages by researchers working on typed Forth and later Joy. The key insight: a stack is just a record where fields are positions rather than names. Seq's type system builds on this work to provide safety without sacrificing the flexibility that makes concatenative programming powerful.

**In other languages:** PureScript and some ML variants have row polymorphism for extensible records. TypeScript's mapped types and excess property checks address similar problems differently. Most languages don't need this concept because they don't have stack-based semantics - it's analogous to how generics let you write code that works with any type.

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

The compiler verifies that when you compose words, the stack types line up:

```seq
# These compose: dup outputs match i.+'s inputs
: double ( Int -- Int ) dup i.+ ;
#          ↑ Int      → ↑ Int Int → ↑ Int

# This would fail:
# : broken ( Int -- Int ) dup concat ;  # ERROR: concat expects strings!
```

**Why it matters:** Stack effects are the contract of a function. The compiler traces types through each operation, catching errors at compile time. This makes concatenative code both highly composable and type-safe.

**In other languages:** Function signatures like `int add(int a, int b)` in C/Java describe named parameters. Stack effects describe the *stack transformation* rather than named parameters. Forth uses stack effect comments by convention; Seq makes them part of the type system.

---

## Strand (Green Thread)

A lightweight unit of concurrent execution managed by the runtime, not the operating system.

```seq
[ do-work ] strand.spawn   # Start work in a new strand
```

Strands are much cheaper than OS threads (thousands are fine), and they cooperate by yielding at certain points rather than being preemptively interrupted.

**Why it matters:** You can have massive concurrency without the overhead of OS threads. Great for I/O-bound work like servers handling many connections.

**In other languages:**
- **Go** has goroutines - very similar to strands, lightweight and cooperatively scheduled
- **Erlang/Elixir** has processes - lightweight, isolated, message-passing
- **Java** has OS threads (heavy) and virtual threads (Java 21+, lightweight)
- **JavaScript/Python** use async/await for single-threaded concurrency via callbacks/promises
- **Ruby** has fibers - another name for the same concept

Seq's strands run on top of the [May](https://github.com/Xudong-Huang/may) coroutine library.

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

**History:** TCO was pioneered by **Guy Steele** and **Gerald Sussman** in the development of **Scheme** (1975). They proved that properly tail-recursive functions are equivalent to loops, making recursion a practical tool for iteration. Scheme was the first language to *require* TCO in its specification. This insight influenced functional programming for decades.

**In other languages:** Scheme requires TCO by specification. Haskell, OCaml, and F# implement it. Scala has `@tailrec` annotation for verified tail recursion. JavaScript includes TCO in the ES6 spec, though Safari is currently the only major browser implementing it. Java and Python do not implement TCO. Seq guarantees TCO using LLVM's `musttail` directive.

---

## Union

See [ADT](#adt-algebraic-data-type).

---

## Variant

A tagged value - an instance of a union type. Not to be confused with variables, records, or the loosely-typed "Variant" from COM/Visual Basic.

```seq
union Shape {
  Circle { radius: Int }
  Rectangle { width: Int, height: Int }
}

10 Make-Circle           # Creates a Variant with tag=Circle, one field
5 10 Make-Rectangle      # Creates a Variant with tag=Rectangle, two fields
```

A Variant carries:
- A **tag** identifying which union case it is (Circle vs Rectangle)
- Zero or more **fields** containing the associated data

You create Variants using generated constructors (`Make-Circle`, `Make-Rectangle`) and inspect them with `match`:

```seq
: area ( Shape -- Int )
  match
    Circle { >radius } -> dup i.*           # πr² simplified to r²
    Rectangle { >width >height } -> i.*     # w × h
  end
;
```

**Why "variant"?** The term comes from type theory - a "variant type" is a type that can be one of several variants. Each variant is a tagged alternative. The value "varies" in which case it represents.

**Common confusion:**
- **Not a variable** - Variables hold changing values; Variants are immutable tagged data
- **Not a record/struct** - Records have fixed fields; Variants are one-of-many alternatives
- **Not COM Variant** - COM's Variant is a loosely-typed container; Seq's Variants are statically typed

**In other languages:** Rust calls these `enum` values. Haskell calls them "data constructors." OCaml calls them "variant constructors." TypeScript's discriminated unions achieve similar patterns. The term "variant" is common in ML-family languages.

See also: [ADT](#adt-algebraic-data-type), [Pattern Matching](#pattern-matching)

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

**In other languages:** Equivalent to "function," "method," or "procedure." Seq uses "word" to honor the Forth tradition and because it emphasizes the linguistic nature of concatenative programming - a program is a sentence of words.

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
2. Pauses execution at exactly that point
3. Waits for the next `strand.resume` to continue *from that exact location*

The key insight: the yield point is both the **pause point** and the **resumption point**. When the generator resumes, it continues with all its local state intact - stack values, recursion depth, everything. This makes generators/coroutines inherently **stateful**.

**Example: Game avatars.** Lua coroutines are famously used in MMOs to model player characters. Each avatar is a coroutine that knows its coordinates, inventory, and capabilities. The game loop resumes each avatar, it does a tick of work (move, attack, etc.), yields back, and suspends - all its state preserved until next tick. No external state management needed; the coroutine *is* the state.

**Why it matters:** Yield enables lazy evaluation and producer/consumer patterns. The generator only does work when asked. More powerfully, the stateful nature lets you model complex behaviors (state machines, protocol handlers, simulations) as straightforward sequential code rather than callbacks or explicit state objects.

**In other languages:** Python has `yield`. JavaScript has `yield` in generator functions. C# has `yield return`. Lua has `coroutine.yield()`. The concept is the same across languages - pause execution and emit a value.

---

## Further Reading

- [Language Guide](./language-guide.md) - Full syntax and semantics
- [Type System Guide](./TYPE_SYSTEM_GUIDE.md) - Deep dive into Seq's type system
- [TCO Guide](./TCO_GUIDE.md) - How tail call optimization works
- [Architecture](./ARCHITECTURE.md) - System design and implementation
- [seqlings](https://github.com/navicore/seqlings) - Learn by doing with guided exercises
