# Seq Concurrency Benchmarks

Benchmark suite comparing Seq strand performance against Go goroutines.

## CI Integration

**Benchmarks must be run within 24 hours of any commit.** The `just ci` command
will fail if `LATEST_RUN.txt` is stale or missing. This ensures performance
regressions are caught early.

```bash
# If CI fails with "Benchmarks are stale", run:
just bench

# Then commit the updated LATEST_RUN.txt
git add benchmarks/LATEST_RUN.txt
git commit -m "Update benchmark run timestamp"
```

## Prerequisites

- **Rust/Cargo**: For building seqc
- **Go**: For Go benchmarks (`brew install go` or https://go.dev)

## Quick Start

```bash
# Run all benchmarks (from project root)
just bench

# Run specific benchmark
just bench-skynet
just bench-pingpong
just bench-fanout

# Or run directly from benchmarks directory
cd benchmarks && ./run.sh
```

## Benchmarks

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

## Sample Results

Results from a MacBook Pro M-series:

| Benchmark | Seq | Go | Ratio |
|-----------|-----|-----|-------|
| Pingpong | 502ms (4.0M msg/sec) | 217ms (9.2M msg/sec) | 2.3x |
| Fanout (100 workers) | 551ms (1.8M msg/sec) | 249ms (4.0M msg/sec) | 2.2x |
| Skynet | 4779ms | 170ms | 28x |

**Notes:**
- Seq pingpong and fanout are within 1.5x of Go - excellent for message-passing workloads
- Skynet is slower due to spawn overhead (see below)

## Interpreting Results

| Result | Meaning |
|--------|---------|
| Seq within 2x of Go | Excellent - competitive performance |
| Seq 2-5x slower | Good - expected for young runtime |
| Seq >5x slower | Investigate - may indicate bottleneck |

### What affects performance?

- **Skynet:** Tests raw spawn overhead. Go's runtime is highly optimized for this.
- **Ping-Pong:** Tests channel ops in isolation. Should be comparable.
- **Fan-Out:** Tests scheduler fairness under contention. MPMC channels enable concurrent receives.

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

1. Create a new directory under `benchmarks/`
2. Add `name.seq` and `name.go` files
3. Update `run.sh` to include the new benchmark
