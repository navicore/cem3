# Seq Benchmarks

Benchmark suite comparing Seq performance against Rust and Go.

Includes two categories:
- **Concurrency benchmarks**: Test strand/goroutine performance (spawn, channels, context switching)
- **Compute benchmarks**: Test pure computation (loops, recursion, arithmetic)

## CI Integration

**Benchmarks must be run within 24 hours of any commit.** The `just ci` command
will fail if `LATEST_RUN.txt` is stale or missing.

**Why 24 hours?** Performance regressions are easier to diagnose when caught early.
The 24-hour window ensures benchmarks are run at least once per development session,
catching issues before they're buried under unrelated commits. Cloud CI runners have
inconsistent performance, so local benchmarking is the source of truth.

```bash
# If CI fails with "Benchmarks are stale", run:
just bench

# Then commit the updated LATEST_RUN.txt
git add benchmarks/LATEST_RUN.txt
git commit -m "Update benchmark run timestamp"
```

## Prerequisites

**Required:**
- **Rust/Cargo**: For building seqc and Rust benchmarks
- **Go**: For Go benchmarks (`brew install go` or https://go.dev)

**Optional (recommended):**
- **hyperfine**: For statistical benchmarking with summary table (`cargo install hyperfine`)
- **jq**: For robust JSON parsing of hyperfine output (`brew install jq` or `dnf install jq`)
- **bc**: For ratio calculations in summary (usually pre-installed)

**Linux only:**
- If running skynet (1M strands), you may need to increase memory map limits:
  ```bash
  sudo sysctl -w vm.max_map_count=2000000
  ```

## Quick Start

```bash
# Run all benchmarks (from project root)
just bench

# Run specific categories
./benchmarks/run.sh concurrency  # skynet, pingpong, fanout
./benchmarks/run.sh compute      # fib, sum_squares, primes

# Run individual benchmarks
./benchmarks/run.sh skynet
./benchmarks/run.sh pingpong
./benchmarks/run.sh fanout
./benchmarks/run.sh fib
./benchmarks/run.sh sum_squares
./benchmarks/run.sh primes
```

## Concurrency Benchmarks

### Skynet (Spawn Overhead)

Spawns 1,000,000 strands/goroutines in a 10-ary tree. Each leaf returns its ID, parents sum children.

**Tests:** spawn throughput, message passing, work-stealing efficiency

**Expected result:** 499,999,500,000 (sum of 0..999,999)

### Ping-Pong (Latency)

Two strands exchange 1,000,000 messages back and forth.

**Tests:** channel round-trip latency, context switch overhead

**Key metric:** Messages per second

### Fan-Out (Throughput)

1 producer sends 1,000,000 messages to N concurrent worker strands.
Workers receive from a shared MPMC (multi-producer, multi-consumer) channel.

**Tests:** channel throughput, work distribution, concurrent receive performance

**Configuration:** 100 workers, 1,000,000 messages

**Key metric:** Throughput (msg/sec)

## Compute Benchmarks

Pure computation benchmarks with no concurrency, testing interpreter/runtime overhead.

### Fibonacci (fib)

Naive recursive Fibonacci calculation: `fib(40)`.

**Tests:** function call overhead, recursion depth, stack operations

**Expected result:** 102,334,155

### Sum of Squares (sum_squares)

Sum of squares from 1 to 1,000,000: `1² + 2² + 3² + ... + 1000000²`

**Tests:** loop iteration, integer arithmetic

**Expected result:** 333,333,833,333,500,000

### Prime Counting (primes)

Count primes up to 100,000 using trial division.

**Tests:** nested loops, modulo operations, conditionals, recursion

**Expected result:** 9,592 primes

## Sample Results

Results from a MacBook Pro M-series:

### Concurrency Benchmarks

| Benchmark | Seq | Rust | Go | Notes |
|-----------|-----|------|-----|-------|
| Pingpong | 269ms | 3875ms | 206ms | Rust std::thread is slow for this |
| Fanout | 551ms | 29ms | 249ms | Rust thread pool excels here |
| Skynet | 4779ms | 2ms | 170ms | Rust uses threshold-based parallelism |

### Compute Benchmarks

| Benchmark | Seq | Rust | Go | Seq/Rust |
|-----------|-----|------|-----|----------|
| fib(40) | 2500ms | 168ms | 224ms | 15x |
| sum_squares | 54ms | 2ms | 2ms | 29x |
| primes | 93ms | 3ms | 3ms | 29x |

**Notes:**
- Seq pingpong and fanout are competitive with Go for message-passing workloads
- Rust std::thread pingpong is slow because OS thread context switches are expensive
- Compute benchmarks show ~15-30x overhead for Seq vs native code (expected for interpreter)

## Interpreting Results

### Concurrency (Seq vs Go)

| Result | Meaning |
|--------|---------|
| Seq within 2x of Go | Excellent - competitive performance |
| Seq 2-5x slower | Good - expected for young runtime |
| Seq >5x slower | Investigate - may indicate bottleneck |

### Compute (Seq vs Rust)

| Result | Meaning |
|--------|---------|
| Seq within 10x of Rust | Good - reasonable interpreter overhead |
| Seq 10-50x slower | Expected - typical for interpreted vs compiled |
| Seq >50x slower | Investigate - may indicate inefficient codegen |

### What affects performance?

- **Skynet:** Tests raw spawn overhead. Go's runtime is highly optimized for this.
- **Ping-Pong:** Tests channel ops in isolation. Should be comparable to Go.
- **Fan-Out:** Tests scheduler fairness under contention. MPMC channels enable concurrent receives.
- **Compute:** Tests raw interpreter overhead. Rust is the baseline for optimal native code.

### Why Rust concurrency results vary

Rust benchmarks use `std::thread` (OS threads) and `std::sync::mpsc` channels:
- **Pingpong:** OS thread context switches are ~10-100x slower than green threads
- **Fanout:** Rust thread pool handles this well despite OS thread overhead
- **Skynet:** Uses threshold-based parallelism (parallel at high levels, sequential at leaves)

This demonstrates why green threads (Go, Seq) excel at fine-grained concurrency.

### Spawn Overhead vs Message Passing

Skynet results are **not representative of real actor system performance**. Here's why:

| Benchmark | Pattern | Seq System Time | vs Go |
|-----------|---------|-----------------|-------|
| Pingpong | 2 strands, 1M messages | 3ms (1%) | 1.2x slower |
| Skynet | 100k strands, minimal work | 18,000ms (300%) | 35x slower |

**Root cause:** May's coroutine library uses mmap/munmap syscalls with guard pages for each strand stack. Go uses segmented stacks with minimal syscalls.

**Practical implications:**
- **Long-lived actors:** Spawn once, message forever → syscall cost amortized → competitive with Go
- **Spawn-heavy patterns:** Pay full syscall cost per strand → 30x+ overhead

**For actor systems:** If you spawn 1M actors at startup (one-time ~60s cost), then send millions of messages, performance will be competitive with Go. Skynet is a synthetic benchmark that specifically stress-tests spawn overhead.

## Technical Notes

### MPMC Channels

Seq uses May's MPMC (multi-producer, multi-consumer) channels. Key behaviors:
- Unbounded queue (sends never block)
- Multiple strands can receive concurrently from the same channel
- Each message is delivered to exactly one receiver (work-stealing semantics)
- Workers should `chan.yield` after receiving to enable fair distribution

### Sentinel-Based Shutdown

The fanout benchmark uses sentinel values (-1) to signal workers to stop, rather than channel close. This ensures workers can drain all messages before exiting.

## Manual Testing

```bash
# Build and run Seq benchmark manually
../target/release/seqc build skynet/skynet.seq -o skynet/skynet
./skynet/skynet

# Build and run Go benchmark manually
cd skynet && go build -o skynet_go skynet.go && ./skynet_go

# Build and run Rust benchmark manually
rustc -O -o skynet/skynet_rust skynet/skynet.rs && ./skynet/skynet_rust

# Compute benchmarks
../target/release/seqc build compute/fib.seq -o compute/fib_seq && ./compute/fib_seq
rustc -O -o compute/fib_rust compute/fib.rs && ./compute/fib_rust
cd compute && go build -o fib_go fib.go && ./fib_go
```

## Runtime Tuning

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `SEQ_STACK_SIZE` | 131072 (128KB) | Coroutine stack size in bytes |
| `SEQ_POOL_CAPACITY` | 10000 | Coroutine pool size (reduces allocations) |

### Cargo Features

The `seq-runtime` crate has a `diagnostics` feature (enabled by default):

```toml
# Disable diagnostics for maximum performance
[dependencies]
seq-runtime = { version = "...", default-features = false }
```

When disabled:
- No strand registry overhead (O(n) scans on spawn/complete)
- No SIGQUIT signal handler
- No `SystemTime::now()` syscalls per spawn

Note: In benchmarks, the diagnostics overhead is negligible compared to spawn syscall overhead.

## Adding New Benchmarks

1. Create a new directory under `benchmarks/` (or use `compute/` for pure computation)
2. Add `name.seq`, `name.rs`, and `name.go` files
3. Update `run.sh` to include the new benchmark in the appropriate category
