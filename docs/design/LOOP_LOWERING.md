# Loop Lowering in Codegen

## Intent

Compute benchmarks show Seq 13-32x slower than Go/Rust. Every "iteration"
is a `musttail` call: function prologue, spill virtual registers to stack
memory, jump. A native loop is a single basic block with a phi node and
a conditional branch — no call overhead, and LLVM can vectorize/unroll.

The goal is to detect self-tail-recursive words and lower them to LLVM
loops instead of `musttail` calls.

## Current Implementation

A word like:
```seq
: sum-to ( Int Int -- Int )
  over 0 i.<= if nip
  else swap dup rot i.+ swap 1 i.- swap sum-to
  then ;
```

Compiles to:
```llvm
define tailcc ptr @seq_sum_to(ptr %stack) {
entry:
  ; ... body code ...
  br i1 %cond, label %if_then, label %if_else

if_then:
  ; base case — return
  ret ptr %result

if_else:
  ; ... compute next args ...
  call void @patch_seq_maybe_yield()
  %r = musttail call tailcc ptr @seq_sum_to(ptr %stack_n)
  ret ptr %r
}
```

Each iteration: spill virtual stack → call `maybe_yield` → `musttail` jump
→ reload from stack memory. LLVM can't see across the call boundary to
optimize the loop body.

## Constraints

- **Only self-tail-recursion** — Mutual recursion stays as `musttail`.
  Detecting mutual loops in a call graph is a separate, harder problem.
- **Must preserve `maybe_yield`** — Tight loops need cooperative yields
  for strand fairness. Insert a yield check every N iterations (e.g., 1024)
  instead of every iteration.
- **Must not break non-loop tail calls** — Words that tail-call other
  words (not themselves) still use `musttail`.
- **Correctness first** — The loop must produce identical stack state.
  Start with the simplest pattern (single `if/else` with self-call in
  one branch) before handling complex control flow.

## Approach

### Pattern Detection (in codegen, not parser)

When emitting a word body, check:
1. Word has exactly one `if/else/then` at the top level
2. One branch contains a self-tail-call as the last statement
3. The other branch does not call self (the base case)

This covers ~90% of recursive loops in practice (factorial, countdown,
sum, fold, fibonacci-acc, etc.).

### Code Generation

Instead of emitting `musttail`, emit a loop:

```llvm
define tailcc ptr @seq_sum_to(ptr %stack) {
entry:
  br label %loop

loop:
  %sp = phi ptr [%stack, %entry], [%sp_next, %continue]
  ; ... body code (condition + branch) ...
  br i1 %cond, label %base, label %continue

continue:
  ; ... compute next iteration's stack state ...
  ; yield check every 1024 iterations
  %iter = phi i64 [0, %loop], [%iter_next, %continue]
  %iter_next = add i64 %iter, 1
  %need_yield = icmp eq i64 0, (and i64 %iter_next, 1023)
  br i1 %need_yield, label %do_yield, label %loop

do_yield:
  call void @patch_seq_maybe_yield()
  br label %loop

base:
  ; ... base case ...
  ret ptr %result
}
```

### Virtual Stack in Loops

The virtual stack (top 4 values in SSA registers) can stay in registers
across loop iterations using phi nodes. No need to spill and reload —
this is where the real speedup comes from.

```llvm
loop:
  %v0 = phi i64 [%init_v0, %entry], [%next_v0, %continue]
  %v1 = phi i64 [%init_v1, %entry], [%next_v1, %continue]
  ; operate on %v0, %v1 directly — no memory loads
```

### Incremental Rollout

1. **Phase 1**: Detect simplest pattern (single if/else, self-call in
   one branch). Emit loop. Keep `musttail` as fallback for everything
   else. Gate behind `--loop-opt` flag.
2. **Phase 2**: Handle match expressions with self-call in one arm.
3. **Phase 3**: Handle multiple self-calls (e.g., both branches recurse
   but with different args — fibonacci pattern). This requires loop
   unrolling or continuation-passing and may not be worth it.

## What This Does NOT Fix

- **Mutual recursion** — `ping`/`pong` patterns stay as `musttail`.
- **Collection iteration overhead** — `list.map` calls a quotation per
  element; that's a different optimization (inline expansion).
- **40-byte value cost** — Stack operations still move 40 bytes per value
  through memory when the virtual stack spills.

## Checkpoints

1. **fib(40) under 500ms** (currently 2200ms) — fibonacci is the classic
   self-recursive benchmark
2. **sum_squares under 10ms** (currently 48ms) — tight arithmetic loop
3. **primes under 20ms** (currently 84ms) — nested loops with modulo
4. **leibniz_pi under 500ms** (currently 2900ms) — 4-value state loop
5. **`--loop-opt` flag** — opt-in initially, default later after validation
6. **All existing tests pass** — no regressions
