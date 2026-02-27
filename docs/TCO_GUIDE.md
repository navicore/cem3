# Tail Call Optimization (TCO) Guide

## Overview

This guide describes tail call optimization in seqc, the Seq compiler. TCO is
a critical optimization for functional and recursive programming styles,
allowing recursive functions to execute in constant stack space.

## Motivation

### The Problem

Without TCO, recursive functions consume stack space for each call:

```seq
: factorial ( Int -- Int )
    dup 1 i.<= if
        drop 1
    else
        dup 1 i.- factorial i.*   # Each call adds a stack frame
    then
;
```

Calling `1000 factorial` would create 1000 stack frames, risking stack overflow.

### Why TCO Matters for Seq

1. **Concatenative languages favor recursion** - Without built-in loop constructs,
   recursion is the natural way to express iteration in Seq

2. **SeqLisp and embedded languages** - Languages implemented in Seq (like
   SeqLisp) have recursive interpreters. Without TCO, both the interpreter
   recursion AND user program recursion compound

3. **Coroutine stack limits** - Strands have fixed stacks (128KB by default,
   configurable via `SEQ_STACK_SIZE`). TCO reduces pressure on these stacks

4. **Differentiating feature** - Many compiled languages lack guaranteed TCO.
   Making it a first-class feature positions Seq for functional programming use
   cases

## Background: How TCO Works

### Traditional Call
```
caller:
    push return_address
    push arguments
    jump to callee

callee:
    ... do work ...
    pop arguments
    pop return_address
    jump to return_address
```

### Tail Call (Optimized)
When a call is the *last* thing before returning, we can reuse the current frame:
```
caller:
    # Don't push return address - reuse caller's
    overwrite arguments in place
    jump to callee  # callee will return directly to our caller
```

### LLVM Support

Seq uses LLVM's `musttail` calling convention — guaranteed TCO (compiler error
if the optimization is impossible):

```llvm
%result = musttail call ptr @seq_bar(ptr %stack)
ret ptr %result
```

All Seq word functions share a uniform `ptr -> ptr` signature, which is ideal
for `musttail`: return value is directly from the call with no cleanup between.

## Tail Position in Seq

A call is in tail position when it's the last operation before the function
returns. In Seq's word-based model:

**Tail position:**
```seq
: foo ( -- )
    setup-stuff
    bar           # Last word - tail position
;
```

**NOT tail position:**
```seq
: foo ( -- Int )
    something
    bar           # Not tail - result used by i.+
    i.+
;
```

### Conditionals

Both branches must end in tail position for the call to be a tail call:
```seq
: factorial ( Int -- Int )
    dup 1 i.<= if
        drop 1        # Base case - not a call, that's fine
    else
        dup 1 i.-
        factorial     # Tail position in else branch
        i.*           # PROBLEM: i.* comes after factorial
    then
;
```

This common pattern is NOT tail-recursive. To enable TCO, use accumulator style:
```seq
: factorial-acc ( Int Int -- Int )
    over 1 i.<= if
        nip           # Return accumulator
    else
        swap dup 1 i.- swap
        rot i.*       # Multiply first
        factorial-acc # NOW in tail position
    then
;

: factorial ( Int -- Int )
    1 factorial-acc
;
```

### Match Expressions

Match expressions work the same as conditionals - each arm body is independently
checked for tail position. If a match arm's **last statement** is a recursive
call, it gets TCO:

```seq
: process-list ( SexprList -- Int )
  match
    SNil ->
      0                    # Base case, no recursion
    SCons { >head >tail } ->
      swap do-something    # Not tail position (head on stack)
      process-list         # Last statement - gets TCO ✓
  end
;
```

Each arm is evaluated independently, so different arms can have different
tail call behavior:

```seq
: eval-expr ( Expr -- Value )
  match
    Literal { >value } ->
      # value on stack
                           # No recursion needed
    BinOp { >left >op >right } ->
      # Stack: ( left op right )
      rot eval-expr        # NOT tail - result used below
      swap eval-expr
      apply-op
    Call { >func >args } ->
      # Stack: ( func args )
      eval-args
      eval-expr            # Last statement - gets TCO ✓
  end
;
```

The key insight: TCO applies to the **last statement in each arm body**,
regardless of how many statements precede it.

## Mutual Recursion

Mutual recursion works correctly — each tail call reuses the caller's frame
regardless of which function is called:

```seq
: even? ( Int -- Bool )
    dup 0 i.= if drop true else 1 i.- odd? then
;

: odd? ( Int -- Bool )
    dup 0 i.= if drop false else 1 i.- even? then
;
```

`1000000 even?` runs in constant stack space despite bouncing between two words.

## Design Decisions

1. **TCO is always-on**
   - No opt-in required — compiler applies TCO whenever possible
   - Rationale: Coroutine stacks are fixed-size (128KB default), recursion is idiomatic
   - Trade-off: Shorter stack traces in errors (acceptable)

2. **No `@tailrec` annotation**
   - Compiler silently optimizes when possible
   - No warnings for non-tail-recursive code
   - Rationale: Keep language simple, avoid annotation clutter

## Appendix: LLVM IR Examples

### Before TCO
```llvm
define ptr @seq_factorial(ptr %stack) {
entry:
    ; ... check n <= 1 ...
    br i1 %cond, label %base, label %recurse

base:
    ; return 1
    %result1 = call ptr @patch_seq_push_int(ptr %stack2, i64 1)
    ret ptr %result1

recurse:
    ; recursive case
    %n_minus_1 = call ptr @patch_seq_subtract(ptr %stack3)
    %rec_result = call ptr @seq_factorial(ptr %n_minus_1)  ; NOT tail call
    %final = call ptr @patch_seq_multiply(ptr %rec_result)
    ret ptr %final
}
```

### After TCO (Accumulator Style)
```llvm
define ptr @seq_factorial_acc(ptr %stack) {
entry:
    ; ... check n <= 1 ...
    br i1 %cond, label %base, label %recurse

base:
    ; return accumulator (already on stack)
    ret ptr %stack2

recurse:
    ; compute new acc, new n
    %new_stack = call ptr @patch_seq_multiply(ptr %stack3)
    %prepared = call ptr @patch_seq_subtract(ptr %new_stack)
    %result = musttail call ptr @seq_factorial_acc(ptr %prepared)
    ret ptr %result
}
```

## References

- [LLVM Language Reference: Tail Calls](https://llvm.org/docs/LangRef.html#call-instruction)
- [LLVM Tail Call Optimization](https://llvm.org/docs/CodeGenerator.html#tail-call-optimization)
- [Wikipedia: Tail Call](https://en.wikipedia.org/wiki/Tail_call)
