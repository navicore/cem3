# Tail Call Optimization (TCO) Design

## Overview

This document describes the design for implementing tail call optimization in
seqc, the Seq compiler. TCO is a critical optimization for functional and
recursive programming styles, allowing recursive functions to execute in
constant stack space.

## Motivation

### The Problem

Without TCO, recursive functions consume stack space for each call:

```seq
: factorial ( n -- result )
    dup 1 <= if
        drop 1
    else
        dup 1 - factorial *   # Each call adds a stack frame
    then
;
```

Calling `1000 factorial` would create 1000 stack frames, risking stack overflow.

### Why TCO Matters for Seq

1. **Concatenative languages favor recursion** - Without loop constructs beyond
   `while`/`times`, recursion is natural for many algorithms

2. **SeqLisp and embedded languages** - Languages implemented in Seq (like
   SeqLisp) have recursive interpreters. Without TCO, both the interpreter
   recursion AND user program recursion compound

3. **Coroutine stack limits** - May coroutines have finite stacks (currently
   8MB). TCO reduces pressure on these stacks

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

LLVM provides three mechanisms:

1. **`tail call`** - Hint to optimizer (may or may not optimize)
   ```llvm
   %result = tail call ptr @seq_bar(ptr %stack)
   ret ptr %result
   ```

2. **`musttail call`** - Guaranteed TCO (compiler error if impossible)
   ```llvm
   %result = musttail call ptr @seq_bar(ptr %stack)
   ret ptr %result
   ```

3. **`tailcc` calling convention** - Designed for tail calls
   ```llvm
   define tailcc ptr @seq_foo(ptr %stack) {
     %result = musttail call tailcc ptr @seq_bar(ptr %stack)
     ret ptr %result
   }
   ```

## Current Architecture Analysis

### Function Signature (Already TCO-Friendly)

All Seq functions follow this pattern:
```llvm
define ptr @seq_word(ptr %stack) {
    ; ... operations ...
    %final = call ptr @seq_other_word(ptr %current_stack)
    ret ptr %final
}
```

This is ideal for TCO because:
- Uniform signature: `ptr -> ptr`
- Return value is directly from the call
- No cleanup between call and return

### Tail Position in Seq

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
: foo ( -- n )
    something
    bar           # Not tail - result used by +
    +
;
```

### Conditionals

Both branches must end in tail position for the call to be a tail call:
```seq
: factorial ( n -- result )
    dup 1 <= if
        drop 1        # Base case - not a call, that's fine
    else
        dup 1 -
        factorial     # Tail position in else branch
        *             # PROBLEM: * comes after factorial
    then
;
```

This common pattern is NOT tail-recursive. To enable TCO, use accumulator style:
```seq
: factorial-acc ( n acc -- result )
    over 1 <= if
        nip           # Return accumulator
    else
        swap dup 1 - swap
        rot *         # Multiply first
        factorial-acc # NOW in tail position
    then
;

: factorial ( n -- result )
    1 factorial-acc
;
```

## Implementation Design

### Phase 1: Tail Position Detection

Add tail position tracking to the AST or during codegen:

```rust
enum TailPosition {
    Tail,       // This is the last operation
    NonTail,    // More operations follow
}

fn codegen_statement(&mut self, stmt: &Statement, position: TailPosition) {
    match stmt {
        Statement::WordCall(name) => {
            if position == TailPosition::Tail {
                self.emit_tail_call(name);
            } else {
                self.emit_regular_call(name);
            }
        }
        // ...
    }
}
```

### Phase 2: Propagate Through Control Flow

For `if/then/else`:
```rust
fn codegen_conditional(&mut self, cond: &Conditional, position: TailPosition) {
    // ... emit condition check ...

    // Both branches inherit the tail position
    for stmt in &cond.then_branch {
        let pos = if is_last { position } else { NonTail };
        self.codegen_statement(stmt, pos);
    }

    for stmt in &cond.else_branch {
        let pos = if is_last { position } else { NonTail };
        self.codegen_statement(stmt, pos);
    }
}
```

### Phase 3: Emit Tail Calls

For identified tail calls:
```rust
fn emit_tail_call(&mut self, name: &str) {
    writeln!(self.output,
        "  %{} = musttail call ptr @seq_{}(ptr %{})",
        result_var, name, stack_var
    );
    writeln!(self.output, "  ret ptr %{}", result_var);
}
```

### Phase 4: Calling Convention (Optional Enhancement)

For maximum TCO guarantee, switch to `tailcc`:
```llvm
define tailcc ptr @seq_foo(ptr %stack) {
    %result = musttail call tailcc ptr @seq_bar(ptr %stack)
    ret ptr %result
}
```

This requires updating:
- All function definitions
- All call sites (including runtime calls)

## Edge Cases and Considerations

### 1. Runtime Function Calls

Calls to runtime functions (`patch_seq_add`, etc.) are typically NOT in tail
position because they're followed by more operations. However, some could be:

```seq
: print-and-exit ( msg -- )
    write_line    # Could be tail call to runtime
;
```

**Decision:** Initially, only optimize Seq-to-Seq calls. Runtime calls can be
added later.

### 2. Quotations and Closures

Quotation calls via `call` word:
```seq
[ something ] call
```

The `call` invokes `patch_seq_call` in the runtime, which dynamically invokes
the quotation. TCO here requires runtime support.

**Decision:** Phase 1 focuses on direct word calls. Dynamic calls are future work.

### 3. Mutual Recursion

```seq
: even? ( n -- bool )
    dup 0 = if drop 1 else 1 - odd? then
;

: odd? ( n -- bool )
    dup 0 = if drop 0 else 1 - even? then
;
```

With `musttail`, mutual recursion works correctly - each tail call reuses frames.

### 4. The `cond` Combinator

The multi-way `cond` combinator evaluates quotations dynamically. TCO within
`cond` requires runtime changes.

**Decision:** Defer to future work.

## Testing Strategy

### Unit Tests

1. **Tail position detection**
   ```rust
   #[test]
   fn test_last_word_is_tail_position() { ... }

   #[test]
   fn test_word_before_operation_not_tail() { ... }

   #[test]
   fn test_both_if_branches_checked() { ... }
   ```

2. **IR verification**
   ```rust
   #[test]
   fn test_tail_call_emits_musttail() {
       let ir = compile(": foo bar ;");
       assert!(ir.contains("musttail call"));
   }
   ```

### Integration Tests

1. **Deep recursion without overflow**
   ```seq
   : count-down ( n -- )
       dup 0 > if
           1 - count-down
       else
           drop
       then
   ;

   : main ( -- )
       1000000 count-down  # Should not overflow
   ;
   ```

2. **Mutual recursion stress test**
   ```seq
   1000000 even?  # Should not overflow
   ```

### Benchmarks

Compare stack depth and performance:
- Recursive factorial with/without TCO
- Deep mutual recursion
- SeqLisp recursive programs

## Implementation Phases

### Phase 1: Basic TCO (MVP)
- Detect tail position for word calls
- Emit `tail call` hint (not `musttail` yet)
- Handle simple sequential code

### Phase 2: Control Flow
- Propagate tail position through `if/then/else`
- Handle nested conditionals

### Phase 3: Guaranteed TCO
- Switch to `musttail call`
- Add `tailcc` calling convention
- Verify no regressions

### Phase 4: Extended TCO
- Runtime function tail calls
- Quotation/closure tail calls (requires runtime changes)
- `cond` combinator support

## Open Questions

1. **Should TCO be opt-in or always-on?**
   - Recommendation: Always-on. It's always beneficial when applicable.

2. **How to handle non-tail-recursive code?**
   - Could emit warnings for recursive calls not in tail position
   - Could provide a `@tailrec` annotation that errors if TCO not possible

3. **Debugging implications?**
   - TCO removes stack frames, making backtraces shorter
   - Could add a debug mode that disables TCO

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
