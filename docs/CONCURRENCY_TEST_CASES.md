# Concurrency Test Cases

This document contains the test cases used to isolate the spawn+channel crash bug.
All tests are available in `/tmp/` for reproduction.

## Test 1: Simple TCP Accept Loop ✅ WORKS

**File:** `/tmp/test_tcp_accept_loop.seq`

```seq
: accept-loop ( Int -- Int )
  [
    dup tcp-accept
    "Connection accepted" write_line
    tcp-close
  ]
  forever
;

: main ( -- Int )
  "Starting simple accept loop on port 8081" write_line
  8081 tcp-listen
  accept-loop
;
```

**Result:** Successfully handles 3+ connections in a row
**Proves:** Basic forever loop with tcp_accept works correctly

## Test 2: TCP Accept + Make Channel ✅ WORKS

**File:** `/tmp/test2_channel_only.seq`

```seq
: test-loop ( Int -- Int )
  [
    dup tcp-accept
    "Connection accepted" write_line
    make-channel
    "Channel created" write_line
    drop  # drop channel
    tcp-close
    "Socket closed" write_line
  ]
  forever
;

: main ( -- Int )
  "Test 2: tcp_accept + make_channel (no spawn)" write_line
  "Starting on port 8081..." write_line
  8081 tcp-listen test-loop
;
```

**Result:** Successfully handles 3+ connections in a row
**Proves:** make_channel in forever loop works correctly

## Test 3: TCP Accept + Spawn ✅ WORKS

**File:** `/tmp/test3_spawn_only.seq`

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
    drop  # drop strand_id
  ]
  forever
;

: main ( -- Int )
  "Test 3: tcp_accept + spawn (no channel)" write_line
  "Starting on port 8081..." write_line
  8081 tcp-listen test-loop
;
```

**Result:** Successfully handles 3+ connections in a row
**Proves:**
- Spawn with closures in forever loop works correctly
- Closure capture mechanism works
- Worker strands execute successfully

## Test 4: Full Pattern - Spawn + Channel ❌ CRASHES

**File:** `/tmp/test4_full_pattern.seq`

```seq
: worker ( Int -- )
  receive
  "Worker received socket" write_line
  tcp-close
  "Worker closed socket" write_line
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
  "Test 4: Full pattern (spawn + channel + send/receive)" write_line
  "Starting on port 8081..." write_line
  8081 tcp-listen test-loop
;
```

**Result:**
- First connection succeeds completely (all messages print)
- Crashes at start of second iteration
- Stack overflow at first `dup` of second iteration

**Proves:**
- Bug is NOT in individual components (spawn, channel, forever)
- Bug is in the INTERACTION between spawn + channel + send in forever loop
- Bug manifests AFTER successful first iteration

## Test Runner Scripts

All tests have corresponding shell scripts in `/tmp/`:
- `run_test2.sh` - Runs test2, sends 3 connections
- `run_test3.sh` - Runs test3, sends 3 connections
- `run_test4.sh` - Runs test4, crashes on second connection

**Usage:**
```bash
cd /Users/navicore/git/navicore/cem3
./target/release/seqc --output /tmp/testN /tmp/testN_*.seq
/tmp/run_testN.sh
```

## Stack Trace from Crash

```
   0: std::backtrace_rs::backtrace::libunwind::trace
   1: std::backtrace_rs::backtrace::trace_unsynchronized
   2: std::backtrace::Backtrace::create
   3: generator::stack::sys::overflow::signal_handler
   4: __os_lock_handoff_lock
   5: _dup        <-- SECOND dup (abnormal)
   6: _dup        <-- First dup (normal - start of loop body)
   7: _seq_quot_0 <-- Forever loop quotation
   8: _forever
   9: _seq_test-loop
  10: _seq_main
```

**Key Observation:** Two `_dup` calls in the stack trace is abnormal and suggests recursion or stack frame corruption.

## LLVM IR Comparison

### Test 3 (Working) - spawn without channel

```llvm
define ptr @seq_quot_0(ptr %stack) {
entry:
  %8 = call ptr @dup(ptr %stack)
  %9 = call ptr @tcp_accept(ptr %8)
  ; ... print "Connection accepted" ...
  %19 = ptrtoint ptr @seq_quot_1 to i64
  %20 = call ptr @push_closure(ptr %17, i64 %19, i32 1)
  %21 = call ptr @spawn(ptr %20)
  ; ... print "Spawned worker" ...
  %27 = call ptr @drop_op(ptr %26)
  ret ptr %27
}
```

### Test 4 (Crashing) - spawn with channel

```llvm
define ptr @seq_quot_0(ptr %stack) {
entry:
  %8 = call ptr @dup(ptr %stack)
  %9 = call ptr @tcp_accept(ptr %8)
  ; ... print "Connection accepted" ...
  %13 = call ptr @make_channel(ptr %12)
  %14 = call ptr @dup(ptr %13)              ; <-- Dup the channel_id
  ; ... print "Channel created" ...
  %21 = ptrtoint ptr @seq_quot_1 to i64
  %22 = call ptr @push_closure(ptr %17, i64 %21, i32 1)  ; <-- Captures channel_id
  %23 = call ptr @spawn(ptr %22)
  ; ... print "Spawned worker" ...
  %27 = call ptr @drop_op(ptr %26)
  %28 = call ptr @send(ptr %27)             ; <-- Sends socket through channel
  ; ... print "Sent socket to worker" ...
  ret ptr %31
}
```

**Difference:** Test 4 adds:
1. `make_channel` - creates channel
2. `dup` channel_id (one copy for closure, one for send)
3. Closure captures the channel_id
4. `send` uses the other copy of channel_id

The IR looks correct - proper stack threading, correct operations. The bug is in the runtime behavior, not the generated code.

## Reproduction Commands

```bash
# Compile and run test 4 (crashing case)
cd /Users/navicore/git/navicore/cem3
cargo build --release
./target/release/seqc --output /tmp/test4 /tmp/test4_full_pattern.seq

# Run server
/tmp/test4 &
SERVER_PID=$!
sleep 2

# Send first connection (succeeds)
echo "test" | nc -w 1 localhost 8081

# Send second connection (server crashes)
sleep 1
echo "test" | nc -w 1 localhost 8081

# Clean up
kill -9 $SERVER_PID 2>/dev/null || true
```

## Variations to Test

### Variation 1: Channel without TCP

Does the bug occur without tcp_accept? Replace socket_id with simple Int:

```seq
: worker ( Int -- )
  receive drop
;

: test-loop ( Int -- Int )
  [
    make-channel dup
    [ worker ] spawn drop
    42 swap send  # Send simple Int instead of socket
  ]
  forever
;
```

### Variation 2: Spawn without Send

Does spawn+channel crash without actually sending?

```seq
: worker ( Int -- )
  receive tcp-close
;

: test-loop ( Int -- Int )
  [
    dup tcp-accept
    make-channel dup
    [ worker ] spawn drop
    drop drop  # Drop socket and channel without sending
  ]
  forever
;
```

### Variation 3: Non-Forever Loop

Does the bug occur in a finite loop instead of forever?

```seq
: handle-n-connections ( Int Int -- Int )
  [
    # Same body as test4
    dup tcp-accept
    make-channel dup
    [ worker ] spawn drop
    send
    # Decrement counter
    swap 1 - swap
    over 0 >
  ]
  while
  drop
;
```

## Expected Behavior

All three variations should work without crashing. If any crash, it narrows down the root cause:
- Variation 1 crash → Bug is in channel+spawn interaction (not TCP-specific)
- Variation 2 crash → Bug is in spawn with captured channel (not send-specific)
- Variation 3 works → Bug is specific to forever loop implementation
- Variation 3 crashes → Bug is in spawn+channel generally (not forever-specific)
