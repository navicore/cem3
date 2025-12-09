# Seq Development Roadmap

## Runtime Diagnostics

The `kill -3` (SIGQUIT) diagnostics feature currently reports:
- Active strand count (global atomic - accurate)
- Open channel count (global registry - accurate)

### Future Investigation: Per-Thread Memory Stats

**Problem**: Arena and pool memory are thread-local. The signal handler runs on its own thread, so it can only report its own (empty) stats, not the worker threads where strands actually execute.

**Research needed**:
- Feasibility of aggregating stats across all worker threads
- Performance impact of any cross-thread coordination
- Whether this violates our core value: *we can't slow the system down to monitor it*

**Potential approaches** (all need investigation):
1. Global atomic counters updated on each allocation (adds overhead to hot path)
2. Periodic sampling from a dedicated monitoring thread (adds latency to stats)
3. On-demand iteration over thread-local storage (may not be possible safely)
4. Accept the limitation and document that memory stats require external tools (pmap, heaptrack, etc.)

**Decision**: Deferred until we have concrete use cases that justify the complexity and potential performance cost.
