# Observability Guide

Seq provides three layers of runtime observability for compiled programs, from zero-cost live inspection to compile-time instrumentation. All features are gated behind the `diagnostics` Cargo feature (enabled by default).

## Quick Reference

| Tool | Trigger | Overhead | What You Get |
|------|---------|----------|--------------|
| SIGQUIT dump | `kill -3 <pid>` | Zero until triggered | Live snapshot: strands, memory, registry |
| Watchdog | `SEQ_WATCHDOG_SECS=N` | Near-zero (periodic scan) | Alerts when strands run too long |
| At-exit report | `SEQ_REPORT=1` | Near-zero | Wall clock, strands, memory, channels |
| Instrumentation | `--instrument` + `SEQ_REPORT=words` | ~1 atomic per word call | Per-word call counts |

---

## SIGQUIT Diagnostic Dump

Send `SIGQUIT` to a running Seq process to dump runtime statistics to stderr without stopping it. This works like a JVM thread dump.

```bash
kill -3 <pid>
```

Output includes:

- **Strand statistics** — active, total spawned, total completed, peak (high-water mark)
- **Active strand details** — strand IDs and how long each has been running
- **Memory statistics** — arena bytes across all threads
- **Warnings** — lost strands (panic/abort), registry overflow

Example output:

```
=== Seq Runtime Diagnostics ===
Timestamp: SystemTime { ... }

[Strands]
  Active:    3
  Spawned:   150 (total)
  Completed: 147 (total)
  Peak:      12 (high-water mark)

[Active Strand Details]
  Registry capacity: 1024 slots
  3 strand(s) tracked:
    [ 1] Strand #1        running for 42s
    [ 2] Strand #148      running for 3s
    [ 3] Strand #150      running for 0s

[Memory]
  Tracked threads: 4
  Arena bytes:     2.50 MB (across all threads)

=== End Diagnostics ===
```

The signal handler runs on a dedicated thread using `signal-hook`'s iterator API, so all I/O operations happen outside signal context.

---

## Watchdog

The watchdog detects strands that run too long without yielding, helping catch infinite loops and runaway computation in production.

### Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `SEQ_WATCHDOG_SECS` | `0` (disabled) | Threshold in seconds for "stuck" strand |
| `SEQ_WATCHDOG_INTERVAL` | `5` | How often to check (seconds) |
| `SEQ_WATCHDOG_ACTION` | `warn` | What to do: `warn` (dump diagnostics) or `exit` (terminate) |

### Usage

```bash
# Warn if any strand runs longer than 30 seconds
SEQ_WATCHDOG_SECS=30 ./my-program

# Check every 10 seconds instead of every 5
SEQ_WATCHDOG_SECS=30 SEQ_WATCHDOG_INTERVAL=10 ./my-program

# Exit the process if a strand is stuck (for unattended services)
SEQ_WATCHDOG_SECS=60 SEQ_WATCHDOG_ACTION=exit ./my-program
```

When the watchdog triggers with `warn` action, it dumps the same diagnostics as `kill -3`. With `exit` action, it dumps diagnostics then terminates the process.

The watchdog runs on a dedicated thread and scans the strand registry periodically. It piggybacks on existing strand tracking infrastructure, adding no overhead to the hot path.

---

## At-Exit Report (`SEQ_REPORT`)

Batch programs exit before anyone can send `kill -3`. The at-exit report dumps KPIs automatically when the program finishes, controlled by the `SEQ_REPORT` environment variable.

### Configuration

| Value | Format | Destination |
|-------|--------|-------------|
| unset or `0` | — | No report (zero cost) |
| `1` | Human-readable | stderr |
| `json` | JSON | stderr |
| `json:/path/to/file` | JSON | File |
| `words` | Human-readable + word counts | stderr (requires `--instrument`) |

### Usage

```bash
# Human-readable report on stderr
SEQ_REPORT=1 ./my-program

# JSON report on stderr (pipe to jq, etc.)
SEQ_REPORT=json ./my-program 2>report.json

# JSON report written directly to a file
SEQ_REPORT=json:/tmp/report.json ./my-program

# Human report with per-word call counts (requires --instrument)
SEQ_REPORT=words ./my-program
```

### Example Output

```
=== SEQ REPORT ===
Wall clock:      127 ms
Strands spawned: 42
Strands done:    42
Peak strands:    8
Worker threads:  4
Arena current:   0 bytes
Arena peak:      524288 bytes
Messages sent:   100
Messages recv:   100
==================
```

### Metrics

| Metric | Description |
|--------|-------------|
| Wall clock | Total time from scheduler init to program exit |
| Strands spawned | Total number of strands created |
| Strands done | Total number of strands that completed |
| Peak strands | Maximum concurrent strands at any point |
| Worker threads | Number of OS threads with active arenas |
| Arena current | Current arena memory across all threads |
| Arena peak | Peak arena memory across all threads |
| Messages sent | Total channel send operations |
| Messages recv | Total channel receive operations |
| Word counts | Per-word call counts (only with `--instrument`) |

---

## Instrumentation (`--instrument`)

The `--instrument` compiler flag bakes per-word atomic counters into the binary. Each time a word is called, its counter increments. This is useful for profiling which words are hot paths.

### Usage

```bash
# Compile with instrumentation
seqc build --instrument my-program.seq

# Run with word-count report
SEQ_REPORT=words ./my-program
```

### How It Works

When `--instrument` is passed:

1. The compiler emits a global array of `i64` counters (one per word)
2. Each word's entry point gets a single `atomicrmw add monotonic` instruction
3. At program startup, the counter array and word name table are registered with the runtime
4. At exit, `SEQ_REPORT` reads the counters and includes them in the report

**Overhead:** One atomic increment per word call (`lock xadd` on x86). This is the cheapest possible atomic operation. When `--instrument` is not passed, no counters or atomics are emitted — zero cost.

**Tail recursion:** A word that tail-recurses 1M times will show 1M calls. This accurately reflects work done, since each tail call re-enters the function.

### Example Output

With `SEQ_REPORT=words`:

```
=== SEQ REPORT ===
Wall clock:      42 ms
Strands spawned: 1
...

--- Word Call Counts ---
  main                           1
  process-item                   1000
  helper                         5000
  recursive-worker               1000000
==================
```

Word counts are sorted by call count (descending), making it easy to spot hot words.

---

## Disabling Diagnostics

All observability features depend on the `diagnostics` Cargo feature. Disable it to eliminate strand registry operations, signal handler setup, and `SystemTime::now()` syscalls on every spawn:

```toml
# In Cargo.toml
seq-runtime = { version = "...", default-features = false }
```

When disabled:
- `kill -3` has no effect (no signal handler installed)
- Watchdog is not compiled
- `SEQ_REPORT` still works for basic metrics (wall clock, memory) but strand registry data is unavailable

In practice, benchmarking shows the diagnostics overhead is negligible compared to May's coroutine spawn syscalls. The feature is primarily useful for production deployments where live debugging capability is needed.

## Environment Variables Summary

| Variable | Default | Description |
|----------|---------|-------------|
| `SEQ_REPORT` | unset (disabled) | At-exit KPI report format and destination |
| `SEQ_WATCHDOG_SECS` | `0` (disabled) | Stuck-strand detection threshold (seconds) |
| `SEQ_WATCHDOG_INTERVAL` | `5` | Watchdog check frequency (seconds) |
| `SEQ_WATCHDOG_ACTION` | `warn` | Watchdog action: `warn` or `exit` |

## See Also

- [Architecture](ARCHITECTURE.md) — runtime configuration and concurrency design
- [Testing Guide](TESTING_GUIDE.md) — writing and running Seq tests
