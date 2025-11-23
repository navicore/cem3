# Session Status: 2025-11-23

## Summary

This session continued from previous work on closure implementation and concurrency. We fixed two critical bugs (stdout concurrency, loop yielding) but discovered a **blocking architectural bug** that makes the language unusable for real-world concurrent applications.

## Completed Work ✅

### 1. Fixed Stdout Concurrency RefCell Panic

**Problem:** Multiple May coroutines writing to stdout caused RefCell panics:
```
thread '<unnamed>' panicked at library/std/src/io/stdio.rs:860:20:
RefCell already borrowed
```

**Root Cause:** Rust's `std::io::stdout()` uses RefCell internally, which panics on concurrent access from multiple coroutines on the same thread.

**Solution:** Bypass Rust's std::io entirely:
- Use `may::sync::Mutex` to serialize access (yields instead of blocking)
- Use `libc::write()` directly to fd 1 instead of Rust's stdout()

**Files Modified:**
- `runtime/src/io.rs` - Changed `write_line()` implementation
- `Cargo.toml`, `runtime/Cargo.toml` - Added `libc = "0.2"` dependency

**Status:** ✅ FIXED - All 232 tests pass, concurrent stdout writes work correctly

### 2. Fixed Loop Yielding (Cooperative Scheduling)

**Problem:** All loop constructs (`forever`, `while`, `until`, `times`) were implemented as tight Rust loops that never yielded to May's scheduler, violating cooperative multitasking principles.

**Root Cause:** Loops ran continuously without calling `may::coroutine::yield_now()`, starving other strands.

**Solution:** Added explicit yielding after each iteration in all loop constructs.

**Files Modified:**
- `runtime/src/quotations.rs`:
  - `forever` (line 318)
  - `while_loop` (line 271)
  - `until_loop` (line 403)
  - `times` (line 195)

**Status:** ✅ FIXED - Simple concurrent loops work correctly, strands properly yield

### 3. Stack Size Configuration

**Changes:**
- Increased May coroutine stack from 32KB (default) to 8MB (0x100000 words)
- Discovered May has internal maximum around 64MB
- Located in `runtime/src/scheduler.rs:56-64`

**Status:** ✅ CONFIGURED - But stack size is NOT the root cause of remaining issue

## Blocking Issue ❌

### Critical Bug: Spawn + Channel + Forever Loop Crash

**Status:** BLOCKS production use
**Severity:** Critical
**Documentation:** `docs/BUG_SPAWN_CHANNEL_CRASH.md`

**Symptom:**
```seq
: worker ( Int -- )
  receive tcp-close ;

: accept-loop ( Int -- Int )
  [
    dup tcp-accept
    make-channel dup
    [ worker ] spawn drop
    send
  ]
  forever
;
```

This pattern (spawn + channel + send/receive in forever loop):
- ✅ First iteration succeeds completely
- ❌ Second iteration crashes with stack overflow

**What Works in Isolation:**
- ✅ tcp_accept in forever loop (tested 3+ connections)
- ✅ spawn with closures in forever loop (tested 3+ connections)
- ✅ make_channel in forever loop (tested 3+ connections)

**What Fails:**
- ❌ Only when ALL are combined: spawn + channel + send + receive + forever

**Investigation:**
- Stack trace shows double `_dup` calls (abnormal recursion)
- Crash at `generator::stack::sys::overflow::signal_handler`
- Happens at start of second iteration, after first completes successfully
- NOT a simple stack size issue (tried 16MB, even 64MB which exceeds May's limit)

**Test Files Created:**
- `/tmp/test_tcp_accept_loop.seq` - Works ✅
- `/tmp/test2_channel_only.seq` - Works ✅
- `/tmp/test3_spawn_only.seq` - Works ✅
- `/tmp/test4_full_pattern.seq` - Crashes ❌

**Impact:**
- HTTP servers: BLOCKED
- Concurrent request handlers: BLOCKED
- Any spawn+channel pattern in loops: BLOCKED
- Real-world concurrent apps: BLOCKED

**Hypotheses:**
1. Closure environment lifecycle bug with channel operations
2. Channel ownership/cloning bug with captured values
3. May coroutine switching corrupts stack pointer threading
4. Forever loop doesn't properly preserve state after spawn+send

## Test Status

**All Unit Tests:** ✅ 232 passing
- Compiler: 103 tests
- Runtime: 129 tests

**Integration Tests:**
- Simple concurrent patterns: ✅ WORKING
- Spawn + channel in forever loop: ❌ CRASHES

## Architecture Status

### What's Production-Ready ✅

- Stack operations (dup, swap, rot, etc.)
- Arithmetic with overflow handling
- Comparisons and booleans
- I/O (now with correct concurrency)
- Variants (multi-field algebraic types)
- Memory management (pooling + arenas, no leaks)
- Simple concurrency (spawn without channels in loops)

### What's NOT Production-Ready ❌

- **Concurrent request handling** (spawn + channel in loops)
- **HTTP servers**
- **Worker pool patterns**
- **Any real-world concurrent application**

## Next Session Priorities

### Immediate: Deep Debug Spawn + Channel Interaction

**Phase 1: Instrumentation**
1. Add debug logging to spawn with closures (closure creation, registry, trampoline)
2. Add debug logging to channel send/receive (channel_id, values, registry state)
3. Add debug logging to forever loop (stack pointer at each iteration)

**Phase 2: Stack Pointer Validation**
- Verify stack pointer integrity through iterations
- Check alignment, depth, value types at each step

**Phase 3: Isolation Testing**
- Spawn with closure + channel (no send)
- Spawn with closure + send (no channel)
- Simple spawn + send with Ints (no tcp_accept)

**Phase 4: Architecture Decision**

If bug is fundamental, consider:
- **Option A:** Redesign closure spawn mechanism (no global registry)
- **Option B:** Redesign channel operations (avoid conflict with spawn)
- **Option C:** Redesign forever loop (explicit state save/restore)
- **Option D:** Replace May with different runtime (Tokio, Crossbeam, custom)

### Testing Protocol

Before any fix is accepted:
1. All 232 tests must pass
2. Test4 pattern must succeed for 100+ connections
3. No memory leaks over 10,000+ iterations
4. No performance degradation

## User Feedback

Key user insights from this session:
1. "this is hilarious you say it is ready for production-ready work" - Correctly called out premature success claims
2. "If we have an overflow crash the code is unusable" - Any crash makes the entire system unusable
3. "we are nothing but a bug machine if we don't yield on blocking operations" - Led to fixing loop yielding
4. "the system is not at all usable then" - Rejected workarounds, demands complete fix

**Philosophy:** The language must work correctly or it's not acceptable. No workarounds, no limitations on patterns. Either fix completely or redesign architecture.

## Files Modified This Session

### Runtime
- `runtime/src/io.rs` - Stdout concurrency fix (libc::write)
- `runtime/src/quotations.rs` - Loop yielding fixes
- `runtime/src/scheduler.rs` - Stack size configuration
- `Cargo.toml`, `runtime/Cargo.toml` - Added libc dependency

### Documentation
- `docs/BUG_SPAWN_CHANNEL_CRASH.md` - Complete bug investigation report
- `docs/SESSION_STATUS.md` - This file

### Test Files (in /tmp/)
- `test_tcp_accept_loop.seq`
- `test2_channel_only.seq`
- `test3_spawn_only.seq`
- `test4_full_pattern.seq`
- `run_test*.sh` scripts

## Conclusion

**Progress:** Fixed 2 critical bugs (stdout, yielding) that were making the system crash

**Regression:** Discovered the fixes don't help the HTTP server - there's a deeper architectural bug

**Status:** System is NOT usable for production concurrent applications until spawn+channel+forever bug is fixed

**Next:** Deep debugging session with instrumentation to find root cause, potentially requiring architecture changes
