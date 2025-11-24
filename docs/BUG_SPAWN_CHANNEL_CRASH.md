# Critical Bug: Stack Overflow with Spawn + Channel + Forever Loop

**Status:** BLOCKING - System is unusable for concurrent request handling patterns
**Severity:** Critical
**Date Discovered:** 2025-11-23
**Affects:** Phase 9+ (concurrency with channels and spawn)

## Executive Summary

The runtime crashes with a stack overflow when combining `spawn`, `make_channel`, `send`, and `receive` within a `forever` loop. The crash occurs **after the first successful iteration**, at the start of the second iteration. This makes the language **unusable for HTTP servers and similar concurrent request-handling patterns**.

Individual components (tcp_accept, spawn, channels) work correctly in isolation. The bug only manifests when all components are combined in the specific pattern used for concurrent request handling.

## Reproduction

### Minimal Failing Test Case

```seq
: worker ( Int -- )
  receive
  "Worker received socket" write_line
  tcp-close
;

: test-loop ( Int -- Int )
  [
    dup tcp-accept
    "Connection accepted" write_line
    make-channel dup
    "Channel created" write_line
    [ worker ] spawn
    "Spawned worker" write_line
    drop  # drop strand_id
    send
    "Sent socket to worker" write_line
  ]
  forever
;

: main ( -- Int )
  8081 tcp-listen test-loop
;
```

**Result:** First connection succeeds completely, server crashes at start of second iteration.

### Test Files Created

All test files are in `/tmp/`:
- `test_tcp_accept_loop.seq` - Simple tcp_accept forever loop ✅ WORKS
- `test2_channel_only.seq` - tcp_accept + make_channel (no spawn) ✅ WORKS
- `test3_spawn_only.seq` - tcp_accept + spawn (no channel) ✅ WORKS
- `test4_full_pattern.seq` - Full pattern with spawn + channel ❌ CRASHES

## Observed Behavior

### What Happens

1. Server starts successfully
2. First connection:
   - ✅ "Connection accepted" prints
   - ✅ "Channel created" prints
   - ✅ "Spawned worker" prints
   - ✅ "Sent socket to worker" prints
   - ✅ Worker receives and closes socket
3. Second iteration begins:
   - ❌ Crashes at first `dup` in forever loop quotation
   - Stack overflow signal handler triggered

### Stack Trace

```
   0: std::backtrace_rs::backtrace::libunwind::trace
   1: std::backtrace_rs::backtrace::trace_unsynchronized
   2: std::backtrace::Backtrace::create
   3: generator::stack::sys::overflow::signal_handler
   4: __os_lock_handoff_lock
   5: _dup        <-- Second dup (suspicious)
   6: _dup        <-- First dup (normal)
   7: _seq_quot_0 <-- Forever loop body
   8: _forever
   9: _seq_test-loop
  10: _seq_main
```

**Key observation:** Two `_dup` calls in the trace suggests abnormal recursion or stack frame corruption.

## What We've Tested

### Working Patterns ✅

1. **Simple tcp_accept loop (3 connections)**
   ```seq
   : accept-loop ( Int -- Int )
     [
       dup tcp-accept
       "Connection accepted" write_line
       tcp-close
     ]
     forever
   ;
   ```
   Result: All 3 connections succeeded

2. **tcp_accept + spawn (3 connections)**
   ```seq
   : worker ( Int -- )
     "Worker started" write_line
     tcp-close
   ;

   : test-loop ( Int -- Int )
     [
       dup tcp-accept
       "Connection accepted" write_line
       [ worker ] spawn
       "Spawned worker" write_line
       drop
     ]
     forever
   ;
   ```
   Result: All 3 connections succeeded

3. **tcp_accept + make_channel (3 connections)**
   ```seq
   : test-loop ( Int -- Int )
     [
       dup tcp-accept
       "Connection accepted" write_line
       make-channel
       "Channel created" write_line
       drop  # drop channel
       tcp-close
     ]
     forever
   ;
   ```
   Result: All 3 connections succeeded

### Failing Pattern ❌

**Only fails with ALL of:**
- `spawn` (with closure)
- `make_channel`
- `send`
- `receive`
- `forever` loop

Removing ANY component makes it work.

## Stack Size Investigation

Attempted stack sizes:
- 16MB (0x200000 words) - Still crashes
- 64MB (0x800000 words) - Exceeds May's internal limit, panics with `ExceedsMaximumSize(67059712)`

**Conclusion:** This is NOT a simple "need more stack" problem. May has internal limits around ~64MB, and even 16MB doesn't help.

## Loop Yielding Status

All loop constructs properly call `may::coroutine::yield_now()` after each iteration:
- `forever` (quotations.rs:318)
- `while_loop` (quotations.rs:271)
- `until_loop` (quotations.rs:403)
- `times` (quotations.rs:195)

This was implemented to fix cooperative scheduling violations. Simple concurrent tests confirm yielding works correctly.

## LLVM IR Analysis

### Forever Loop Body (test4_full_pattern.seq)

```llvm
define ptr @seq_quot_0(ptr %stack) {
entry:
  %8 = call ptr @dup(ptr %stack)
  %9 = call ptr @tcp_accept(ptr %8)
  %10 = getelementptr inbounds [20 x i8], ptr @.str.2, i32 0, i32 0
  %11 = call ptr @push_string(ptr %9, ptr %10)
  %12 = call ptr @write_line(ptr %11)
  %13 = call ptr @make_channel(ptr %12)
  %14 = call ptr @dup(ptr %13)
  %15 = getelementptr inbounds [16 x i8], ptr @.str.3, i32 0, i32 0
  %16 = call ptr @push_string(ptr %14, ptr %15)
  %17 = call ptr @write_line(ptr %16)
  %21 = ptrtoint ptr @seq_quot_1 to i64
  %22 = call ptr @push_closure(ptr %17, i64 %21, i32 1)  ; <-- Creates closure
  %23 = call ptr @spawn(ptr %22)                          ; <-- Spawns with closure
  %24 = getelementptr inbounds [15 x i8], ptr @.str.4, i32 0, i32 0
  %25 = call ptr @push_string(ptr %23, ptr %24)
  %26 = call ptr @write_line(ptr %25)
  %27 = call ptr @drop_op(ptr %26)
  %28 = call ptr @send(ptr %27)                           ; <-- Sends through channel
  %29 = getelementptr inbounds [22 x i8], ptr @.str.5, i32 0, i32 0
  %30 = call ptr @push_string(ptr %28, ptr %29)
  %31 = call ptr @write_line(ptr %30)
  ret ptr %31
}
```

The IR looks correct. Stack threading is proper. No obvious issues in generated code.

## Hypotheses

### Hypothesis 1: Closure Environment Lifecycle Bug

**Theory:** When `spawn` creates a closure and passes it to a new strand, the closure environment might not be properly handled when combined with channel operations in the parent strand.

**Evidence:**
- Spawn alone works ✅
- Channels alone work ✅
- Only fails when both used together
- Crash happens at start of next iteration, suggesting state corruption

**Key code paths:**
1. `push_closure()` (closures.rs:226) - Creates closure from stack values
2. `spawn()` with Closure variant (quotations.rs:451-479) - Uses `SPAWN_CLOSURE_REGISTRY`
3. `closure_spawn_trampoline` - Retrieves closure from registry and executes

**Potential issue:** The closure registry interaction with channel operations might cause stack state corruption that only manifests on the next iteration.

### Hypothesis 2: Channel Ownership/Cloning Bug

**Theory:** When a channel_id is captured by a closure for spawn, and then used for `send` in the parent strand, there might be value ownership/cloning issues.

**Evidence:**
- `send()` (channel.rs:145) clones the value before sending: `let global_value = value.clone()`
- Channel IDs are `Value::Int`, which should be trivial to clone
- But the socket_id (also an Int) goes through multiple operations

**Key sequence:**
```
make_channel -> channel_id on stack
dup channel_id
push_closure (captures one copy of channel_id)
spawn (moves closure to new strand)
drop (strand_id)
send (uses other copy of channel_id)
```

### Hypothesis 3: May Coroutine Switching + Value Lifecycle

**Theory:** When spawn creates a new May coroutine while channel operations are in progress, the stack pointer threading might get corrupted across coroutine switches.

**Evidence:**
- May uses stackful coroutines (generator library)
- Stack overflow handler is triggered (generator::stack::sys::overflow::signal_handler)
- Two `_dup` calls in trace suggests recursion or frame corruption

**Potential issue:** The LLVM-generated stack threading (`ptr %stack` passed through all functions) might interact badly with May's coroutine stack swapping when spawn+channel are combined.

### Hypothesis 4: Forever Loop State Corruption

**Theory:** The `forever` implementation doesn't properly preserve stack state when the loop body does spawn+send.

**Evidence:**
- Working version: `tcp-accept -> spawn -> next iteration` ✅
- Failing version: `tcp-accept -> spawn -> send -> next iteration` ❌
- Crash is at `dup` (first operation of next iteration)

**Forever implementation (quotations.rs:305-327):**
```rust
pub unsafe extern "C" fn forever(stack: Stack) -> Stack {
    unsafe {
        let (mut stack, quot_value) = pop(stack);
        let fn_ptr = match quot_value {
            Value::Quotation(ptr) => ptr,
            _ => panic!("forever: expected Quotation"),
        };

        let body_fn: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(fn_ptr);

        loop {
            stack = body_fn(stack);  // <-- Stack threading
            may::coroutine::yield_now();
        }
    }
}
```

This looks correct - it properly threads the stack pointer. But maybe `send()` or `spawn()` with closures returns an invalid stack pointer?

## Memory Management Status

All memory management is working correctly for simpler patterns:
- ✅ Stack node pooling (~10x faster than malloc)
- ✅ Arena allocation for strings (~20x faster)
- ✅ Automatic cleanup on strand exit (no leaks)
- ✅ Channel send/receive with arena strings (clones to global)

The issue is NOT a general memory leak - it's specific to this spawn+channel pattern.

## Relevant Source Files

### Runtime

- `runtime/src/scheduler.rs:56-64` - Coroutine stack size configuration
- `runtime/src/quotations.rs:305-327` - forever loop implementation
- `runtime/src/quotations.rs:428-483` - spawn implementation with closure support
- `runtime/src/closures.rs:226-256` - push_closure implementation
- `runtime/src/channel.rs:104-152` - send implementation
- `runtime/src/channel.rs:174-220` - receive implementation

### Test Files

- `/tmp/test4_full_pattern.seq` - Minimal reproducer
- `/tmp/test4.ll` - LLVM IR for failing pattern
- `/tmp/run_test4.sh` - Test runner script

## Next Steps for Deep Debugging

### Phase 1: Instrument Spawn + Channel Interaction

1. **Add debug logging to spawn with closures:**
   - Log when closure is created
   - Log when closure is stored in SPAWN_CLOSURE_REGISTRY
   - Log when closure is retrieved by trampoline
   - Log when closure environment is accessed

2. **Add debug logging to send/receive:**
   - Log channel_id being used
   - Log value being sent/received
   - Log registry state before/after operations

3. **Add debug logging to forever:**
   - Log stack pointer at start of each iteration
   - Log stack pointer after body execution
   - Log stack depth at each step

### Phase 2: Validate Stack Pointer Integrity

Create instrumented versions of key functions that verify:
- Stack pointer is not null when it shouldn't be
- Stack pointer is properly aligned
- Stack depth matches expectations
- Values on stack are of expected types

### Phase 3: Isolate Closure Registry Interaction

Test variations:
1. Spawn with closure + channel (but no send) - does it crash?
2. Spawn with closure + send (but socket_id instead of channel) - does it crash?
3. Simple spawn + send with just Ints (no tcp_accept) - does it crash?

### Phase 4: Consider Architecture Changes

If the bug is fundamental to how spawn+channel interact, consider:

**Option A: Redesign closure spawn mechanism**
- Avoid global registry, pass environment directly somehow
- Use different May coroutine spawning strategy

**Option B: Redesign channel operations**
- Different channel implementation that doesn't conflict with spawn
- Or use different CSP primitives

**Option C: Redesign forever loop**
- Explicitly save/restore stack state
- Add validation checks between iterations

**Option D: Abandon May, use different concurrency runtime**
- Tokio (async/await)
- Crossbeam channels + OS threads
- Custom coroutine implementation

## Testing Protocol

Before any fix is accepted:

1. **All existing tests must pass** (146 tests)
2. **Test4 pattern must succeed for 100+ connections**
3. **No memory leaks over 10,000+ iterations**
4. **Performance must not degrade significantly**

## Impact Assessment

**Blocking issues:**
- ❌ HTTP servers (concurrent request handling)
- ❌ Any pattern that spawns workers with channels in a loop
- ❌ Real-world concurrent applications

**Still working:**
- ✅ Simple concurrent patterns (spawn without channels in loops)
- ✅ One-shot channel operations (not in loops)
- ✅ All sequential code
- ✅ Basic CSP patterns outside of forever loops

**Conclusion:** The language is currently **NOT usable for production concurrent applications** until this bug is fixed.

## Historical Context

This bug was discovered during Phase 9 (Memory Management) while testing the HTTP server example. Previous phases had implemented:
- Phase 7: TCP networking (working)
- Phase 8: Closures (working in isolation)
- Phase 9: Arena allocation (working)

The bug was initially thought to be a stack size issue, then a yielding issue (fixed), but is now understood to be a fundamental interaction problem between spawn+closure+channel in loops.

## Priority

**This is the #1 blocking issue** for the language. No workarounds are acceptable. The language must either:
1. Fix this bug completely, OR
2. Redesign the architecture to avoid the problem

Partial solutions or limitations on usage patterns are not acceptable.
