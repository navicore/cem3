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

## Feasibility Assessment

This section provides an honest analysis of what's achievable and what requires
additional work.

### What's Straightforward (Low Risk)

#### 1. Compiler-Level TCO for Word Calls

When the compiler sees a word definition where the last operation is a call:
```seq
: foo ( -- ) setup-stuff bar ;  # bar is in tail position
```

The compiler can emit `musttail call` in LLVM IR. This covers:
- Direct recursion (`factorial-acc` calling itself)
- Mutual recursion (`even?` and `odd?` calling each other)
- SeqLisp's eval loop and similar interpreter patterns

**Risk**: Low. The architecture already supports this.

#### 2. Quotations via `call` Word

In `patch_seq_call` (runtime/src/quotations.rs), the quotation branch is already
in tail position:
```rust
Value::Quotation(fn_ptr) => {
    let fn_ref: unsafe extern "C" fn(Stack) -> Stack =
        std::mem::transmute(fn_ptr);
    fn_ref(stack)  // Already tail position - no cleanup after
}
```

No runtime changes needed - just needs the compiler to emit `musttail` when
calling `patch_seq_call` in tail position.

**Risk**: Low. The code structure already supports this.

#### 3. Mutual Recursion

Both #1 and #2 handle mutual recursion naturally. With `musttail`, each tail
call reuses the caller's frame regardless of which function is called.

**Risk**: Low. Falls out naturally from the above.

### What Requires Work (Medium Risk)

#### 4. Closures via `call` Word

The closure branch in `patch_seq_call` has a problem:
```rust
Value::Closure { fn_ptr, env } => {
    let env_ptr = Box::into_raw(env);
    let env_data = env_slice.as_ptr();
    let env_len = env_slice.len();

    let result_stack = fn_ref(stack, env_data, env_len);

    // THIS BLOCKS MUSTTAIL - cleanup happens after the call
    let _ = Box::from_raw(env_ptr);
    result_stack
}
```

The closure owns its environment (`Box<[Value]>`). We must free it after the
call returns, which prevents `musttail`.

**Solutions**:

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Transfer ownership to callee | Zero overhead | Requires LLVM IR changes |
| B | Use `Arc<[Value]>` | Simple, automatic cleanup | Slight ref-count overhead |
| C | Don't TCO closures | No runtime changes | Incomplete TCO story |

**Recommendation**: Option B (Arc) provides the best balance. The overhead is
negligible for the closure use cases (spawn, capture).

**Risk**: Medium. Requires runtime changes but is well-understood.

### Why Phased Delivery Works

The key insight is that most tail-recursive patterns don't need closures:

1. **SeqLisp eval loop** - Uses word recursion, not closure recursion
2. **Accumulator patterns** (`factorial-acc`) - Pure quotations, no captures
3. **Mutual recursion** (`even?`/`odd?`) - Word calls, no closures

Closures are primarily for:
- `spawn` (concurrency, not recursion)
- Capturing local state in combinators

So Phase 1-2 deliver value for the common cases, while Phase 3 completes the
story.

## Component Analysis

### Runtime Function Calls

Calls to runtime functions (`patch_seq_add`, etc.) are typically NOT in tail
position because they're followed by more operations. However, some could be:

```seq
: print-and-exit ( msg -- )
    write_line    # Could be tail call to runtime
;
```

**Decision**: Initially, only optimize Seq-to-Seq calls. Runtime calls can be
added later with minimal risk.

### Quotations and Closures

See Feasibility Assessment above. Summary:
- Quotations: Ready for TCO now
- Closures: Need Arc refactor (Phase 3)

### Mutual Recursion

```seq
: even? ( n -- bool )
    dup 0 = if drop 1 else 1 - odd? then
;

: odd? ( n -- bool )
    dup 0 = if drop 0 else 1 - even? then
;
```

With `musttail`, mutual recursion works correctly - each tail call reuses frames.

### The `cond` Combinator

The multi-way `cond` combinator evaluates quotations dynamically. Since
quotations can be TCO'd in Phase 2, `cond` will benefit automatically when its
final branch ends in a quotation call.

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

### Phase 1: Compiler-Level TCO for Word Calls

**Goal**: Tail-recursive word definitions execute in constant stack space.

**Scope**:
- Add `TailPosition` tracking to codegen
- Detect tail position in sequential code
- Propagate tail position through `if/then/else` branches
- Emit `musttail call` for word calls in tail position
- Add `tailcc` calling convention to all Seq functions

**Deliverables**:
- `count-down 1000000` completes without stack overflow
- `even? 1000000` (mutual recursion) completes without overflow
- Unit tests for tail position detection
- IR verification tests for `musttail` emission

**Risk**: Low. Architecture is ready.

**Files to modify**:
- `crates/compiler/src/codegen.rs` - Emit musttail, add tailcc
- `crates/compiler/src/parser.rs` - May need AST annotation (optional)

### Phase 2: Quotation TCO via `call` Word

**Goal**: Dynamic quotation calls in tail position get TCO.

**Scope**:
- Compiler emits `musttail` when `call` word is in tail position
- Quotation branch in `patch_seq_call` already supports this
- No runtime changes needed

**Deliverables**:
- `[ recurse ] call` patterns get TCO
- Combinators like `cond` benefit when final branch is a call
- SeqLisp quotation-heavy code benefits

**Risk**: Low. Runtime already structured correctly.

**Files to modify**:
- `crates/compiler/src/codegen.rs` - Recognize `call` as TCO-eligible

### Phase 3: Closure TCO via Arc Refactor

**Goal**: Closures (quotations with captured environments) get TCO.

**Scope**:
- Change `Value::Closure { env: Box<[Value]> }` to `Arc<[Value]>`
- Remove explicit cleanup in `patch_seq_call` closure branch
- Update closure creation in compiler
- Update spawn trampoline for Arc

**Deliverables**:
- Recursive closures execute in constant stack space
- All `call` invocations (quotations and closures) are TCO-eligible
- Complete TCO story

**Risk**: Medium. Requires coordinated runtime + compiler changes.

**Files to modify**:
- `crates/runtime/src/value.rs` - Change Closure env type
- `crates/runtime/src/quotations.rs` - Remove Box cleanup, update spawn
- `crates/compiler/src/codegen.rs` - Update closure creation

## Design Decisions

1. **TCO is always-on**
   - No opt-in required - compiler applies TCO whenever possible
   - Rationale: Coroutine stacks are limited (8MB), recursion is idiomatic
   - Trade-off: Shorter stack traces in errors (acceptable)
   - Future: May add `--no-tco` debug flag if needed

2. **No `@tailrec` annotation**
   - Compiler silently optimizes when possible
   - No warnings for non-tail-recursive code
   - Rationale: Keep language simple, avoid annotation clutter

3. **Debug features deferred**
   - Focus on correct, fast execution first
   - Escape hatches and diagnostics are future work

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
