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

---

## Type System Research

### Implicit Type Safety Without Generics

**Goal**: Achieve the safety benefits of generics without sacrificing point-free composability or adding syntactic overhead.

Seq's philosophy: type safety through inference, not annotation.

**Current state**:
- Row-polymorphic stack effects provide implicit type threading
- Result/Option helpers use duck typing on variant tags
- Users define concrete unions (`IntResult`, `StringResult`) for their use cases

**Research directions**:

1. **Inferred variant types**
   - Compiler tracks that `Make-Ok` produces a specific union type
   - Result flowing through `result-bind` maintains type identity without annotation

2. **Flow typing through combinators**
   - If `result-bind` receives `IntResult`, infer the quotation expects `Int`
   - Type errors caught at compile time without explicit type parameters

3. **Structural typing for conventions**
   - Recognize "union where tag 0 has one field, tag 1 has String field" as Result-like
   - Helpers already work this way at runtime - make type checker aware

4. **Constructor argument refinement**
   ```seq
   42 Make-Ok         # Inferred: IntResult (from Int argument)
   "hi" Make-Ok       # Inferred: StringResult (from String argument)
   ```

5. **Stack as type evidence**
   - The chain of stack effects *is* the type proof
   - If `safe-divide` returns `IntResult`, the next word knows what it has

**Key question**: How far can we push implicit typing before explicit annotations become necessary?

**Constraint**: Must not compromise point-free style or add syntactic noise. The concatenative feel must be preserved.

---

## Foreign Function Interface (FFI)

**Design document**: [docs/design/ffi.md](design/ffi.md)

### Vision

Enable calling external C libraries without polluting seq-runtime. FFI is purely a compiler/linker concern.

```seq
include ffi:libedit
include ffi:sqlite

: main ( -- Int )
  "mydata.db" db-open drop
  "SELECT * FROM users" db-exec
  0
;
```

### Key Principles

1. **No runtime pollution** - seq-runtime has zero FFI dependencies
2. **Opt-in via include** - `include ffi:*` is the safety boundary
3. **Declarative bindings** - TOML manifests describe C functions
4. **Codegen handles marshalling** - compiler generates LLVM IR for type conversion and memory management
5. **Build flexibility** - enable/disable per target (e.g., no libedit for musl static)

### Implementation Phases

**Phase 1: Readline** ✅ Complete
- Manifest parser
- `include ffi:` support
- String marshalling codegen
- Memory ownership (`caller_frees`)

**Phase 2: Generalization** ✅ Complete
- BSD-licensed `ffi:libedit` as default (no GPL concerns)
- User-provided manifests (`--ffi-manifest`)
- Full type mapping (int, ptr, string, void)
- Out parameters (`by_ref` pass mode)
- Fixed value arguments (`value = "null"`)
- Security validation on linker flags
- SQLite example demonstrating all features

**Phase 3: Advanced**
- Struct passing
- Platform-specific bindings
- Callback support (C → Seq) - *shelved*: most useful callback patterns require low-level memory operations that are too invasive for Seq's design; many C APIs have non-callback alternatives (e.g., SQLite prepared statements)

### Example Manifest

```toml
[[library]]
name = "libedit"
link = "edit"

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
args = [{ type = "string", pass = "c_string" }]
return = { type = "string", ownership = "caller_frees" }
```

### Use Cases

- **libedit**: Line editing for interactive CLI programs
- **SQLite**: Embedded database access
- **PCRE**: Regular expressions
- **libcurl**: HTTP client
- **zlib**: Compression

---

## Standard Library: OS Module

### Vision

Provide portable OS interaction primitives as runtime built-ins, following the same pattern as TCP/HTTP (Rust implementation with C ABI exports).

```seq
include std:os

: get-history-path ( -- String )
  "HOME" getenv if
    "/.myapp_history" string-concat
  else
    drop "/tmp/.myapp_history"
  then
;
```

### Phase 1: Environment & Paths (Priority)

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `getenv` | `( String -- String Int )` | Get env var, returns value and 1 on success, "" and 0 on failure |
| `home-dir` | `( -- String )` | User's home directory |
| `current-dir` | `( -- String )` | Current working directory |

These unblock the persistent history path use case and are simple to implement.

### Phase 2: File System

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `path-exists` | `( String -- Int )` | 1 if path exists, 0 otherwise |
| `path-is-file` | `( String -- Int )` | 1 if regular file |
| `path-is-dir` | `( String -- Int )` | 1 if directory |

### Phase 3: Process & System

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `exit` | `( Int -- )` | Exit process with code |
| `args` | `( -- ... Int )` | Push command line args and count |
| `os-name` | `( -- String )` | "darwin", "linux", "windows" |
| `arch` | `( -- String )` | "x86_64", "aarch64" |

### Implementation Notes

- Implemented in `crates/runtime/src/os.rs`
- C ABI exports: `patch_seq_getenv`, `patch_seq_home_dir`, etc.
- Compiler built-in recognition for `include std:os`
- Cross-platform via Rust's `std::env` and `std::fs`

---

## CLI & Developer Experience

### Tab Completion

**Goal**: Enable tab completion for interactive CLI programs built with Seq.

Now that FFI supports libedit, tab completion for Seq-based CLI tools becomes achievable:

**Near-term: Basic completion**:
- Word name completion in interactive prompts
- Prefix matching for partially typed input
- Integration with libedit via FFI

**Medium-term: Context-aware completion**:
- Application-specific completions
- File path completion
- Custom completion vocabularies

**Potential implementation paths**:
1. **libedit's built-in completion** - Requires FFI callback support (currently shelved)
2. **Custom completion protocol** - Seq-side lookup with manual key handling
3. **LSP integration** - For editor/IDE support of Seq source files
