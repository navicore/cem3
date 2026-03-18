# Performance Analysis: Foundation Choices

## Intent

Seq is slower than Python in channels (#306, 400x), strand spawn (#307, 20x),
and collections (#305, 50-100x). Compute is 13-32x slower than Go/Rust.
The goal is: close to Go, much faster than Python. We need to understand
which foundation choices are load-bearing walls vs which can be replaced.

## Current Numbers (from benchmarks/)

| Benchmark | Seq | Go | Rust | Python |
|-----------|-----|-----|------|--------|
| fib(40) | 2200ms | 224ms | 168ms | ~30s |
| primes(100k) | 84ms | 3ms | 3ms | ~2s |
| pingpong(1M) | 269ms | 206ms | 3875ms | n/a |
| skynet(1M) | 4779ms | 170ms | 2ms | 266ms |
| fanout(100k) | 551ms | 249ms | 29ms | 230ms |
| collection build-100k | 15,888ms | ~0ms | ~0ms | ~0ms |

Compute is competitive with Python. Concurrency and collections are not.

## Foundation Choices — Diagnosis

### 1. 40-byte StackValue (biggest single cost)

Every value — including bare integers — occupies 40 bytes (5 x u64). This means:
- `dup` copies 40 bytes, not 8
- A list of 100k ints is 4MB, not 800KB
- Only ~1.6 values per cache line (vs 8 with 8-byte values)
- Every stack shuffle (rot, pick, roll) moves 40 bytes per element

NaN-boxing (#188) was considered and closed. The 40-byte layout was chosen
because it keeps Variant, Closure, and WeaveCtx inline on the stack (no
heap indirection for the common case). This is a real benefit for
polymorphic code but a severe penalty for numeric-heavy code.

**Verdict**: The virtual stack optimization (top 4 in SSA registers) mitigates
this for straight-line integer code. The real cost shows up in collections
and deep stack manipulation. A NaN-boxed representation would be 5x better
for cache but would require heap-allocating Variants, Closures, and WeaveCtx.
This is the right tradeoff if collections and numeric throughput matter more
than variant-heavy actor code.

### 2. May coroutine library (concurrency floor)

Issue #110 profiling showed May alone is 9.6x slower than Go for skynet, even
in pure Rust — before any Seq overhead. May uses mmap/munmap with guard pages
per coroutine stack. Go uses segmented stacks with minimal syscalls.

**Verdict**: May sets a floor we can't optimize past. For long-lived actors
(spawn once, message forever), the cost is amortized. For spawn-heavy patterns,
we're stuck. Replacing May is a large effort but would unlock Go-competitive
concurrency. Alternatives: `async-std`/`tokio` (async coloring problem),
custom stackful coroutines (hard), or accept the floor and optimize around it.

### 3. Immutable collections (collection floor)

`list.push` allocates a new list every time. 100k pushes = 100k allocations.
Python's `list.append` is amortized O(1) in-place mutation over a C array.

**Verdict**: This is fixable without changing the language semantics. When
refcount == 1, mutate in place (copy-on-write). This is what Clojure's
transients and Swift's value types do. The language already promises functional
semantics — the runtime can optimize under the hood.

### 4. No loop primitive

Every iteration is a tail call. Even with musttail, each iteration pays:
function prologue, stack parameter passing, jump. A counted loop would be
a single LLVM basic block with a phi node and a branch.

**Verdict**: Adding `times` or `loop` to the language (or recognizing
tail-recursive patterns and lowering them to loops in codegen) would close
much of the 13-32x compute gap. This is pure compiler optimization work.

### 5. Channel implementation

Channels use `may::sync::mpmc` which is mutex-based. Each send/receive
acquires a lock, copies a 40-byte Value, and yields. Go channels are
lock-free for the fast path.

**Verdict**: Buffered ring-buffer channels with lock-free fast path would
help significantly. Combined with smaller values (point 1), channel
throughput should improve 10-100x.

## What's Already Right

- **LLVM backend**: Native code generation is the correct foundation. No
  interpreter overhead for the hot path.
- **musttail TCO**: Guaranteed, correct, and enables recursive style.
- **Arena allocator**: Fast allocation for short-lived strings.
- **Virtual stack**: SSA registers for top-of-stack eliminates many 40-byte
  copies for sequential integer code.
- **Static typing**: Enables specialization that dynamic languages can't do.

## Ranked Opportunities (effort vs impact)

| Change | Impact | Effort | Breaks |
|--------|--------|--------|--------|
| COW collections (refcount==1 mutation) | 50-100x collections | Medium | Nothing |
| Loop lowering in codegen | 2-5x compute | Medium | Nothing |
| Buffered/lock-free channels | 10-50x channels | Medium | Nothing |
| NaN-boxing (8-byte values) | 2-5x everything | Very large | Stack layout, codegen, FFI, runtime |
| Replace May | 5-30x spawn | Very large | Scheduler, channels, weaves |

## Checkpoints

1. **COW collections**: build-100k benchmark under 100ms (currently 15,888ms)
2. **Loop lowering**: fib(40) under 500ms, sum_squares under 10ms
3. **Buffered channels**: fanout under 100ms, pingpong under 100ms
4. **Overall goal**: No benchmark where Python wins
