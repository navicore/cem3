# Seq Development Roadmap

## Core Values

**The fast path stays fast.** Observability is opt-in and zero-cost when disabled. We can't slow the system down to monitor it.

Inspired by the Tokio ecosystem (tokio-console, tracing, metrics, tower), we aspire to rich runtime visibility while respecting performance.

---

## Runtime Observability

### Current: SIGQUIT Diagnostics

The `kill -3` (SIGQUIT) feature reports:
- Active strand count (global atomic - accurate)
- Open channel count (global registry - accurate)

Zero overhead until signaled.

### Near-term: Strand & Channel Visibility

**Strand lifecycle events** (opt-in):
- Spawn/exit tracing with strand IDs
- Parent-child relationships for debugging actor hierarchies
- Optional compile-time flag to enable

**Channel diagnostics**:
- Queue depth visibility (backpressure detection)
- Send/receive counts per channel
- Blocked strand detection (who's waiting on what)

### Medium-term: Metrics & Tracing

**Metrics export**:
- Prometheus-compatible endpoint
- Strand pool utilization
- Message throughput sampling
- Configurable sampling rates to control overhead

**Structured tracing**:
- Integration with tracing ecosystem
- Span-based request tracking across strands
- Correlation IDs for distributed debugging

### Long-term: Visual Tooling

**Seq console** (inspired by tokio-console):
- Real-time strand visualization
- Channel flow graphs
- Actor hierarchy browser
- Historical replay for post-mortem debugging

**OpenTelemetry integration**:
- Distributed tracing across services
- Standard observability pipeline integration

---

## Memory Diagnostics

### Future Investigation: Per-Thread Memory Stats

**Problem**: Arena and pool memory are thread-local. The signal handler runs on its own thread, so it can only report its own (empty) stats, not the worker threads where strands actually execute.

**Research needed**:
- Feasibility of aggregating stats across all worker threads
- Performance impact of any cross-thread coordination
- Whether this violates our core value

**Potential approaches** (all need investigation):
1. Global atomic counters updated on each allocation (adds overhead to hot path)
2. Periodic sampling from a dedicated monitoring thread (adds latency to stats)
3. On-demand iteration over thread-local storage (may not be possible safely)
4. Accept the limitation and document that memory stats require external tools (pmap, heaptrack, etc.)

**Decision**: Deferred until we have concrete use cases that justify the complexity and potential performance cost.
