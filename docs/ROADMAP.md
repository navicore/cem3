# Seq Development Roadmap

## Core Values

**The fast path stays fast.** Observability is opt-in and zero-cost when disabled. We can't slow the system down to monitor it.

Inspired by the Tokio ecosystem (tokio-console, tracing, metrics, tower), we aspire to rich runtime visibility while respecting performance.

---

## Recent Milestones

### v0.9.0: Naming Convention Refactor ✅

Established consistent naming scheme for all built-in operations:

| Delimiter | Usage | Example |
|-----------|-------|---------|
| `.` (dot) | Module/namespace prefix | `io.write-line`, `tcp.listen`, `string.concat` |
| `-` (hyphen) | Compound words within names | `home-dir`, `field-at`, `write-line` |
| `->` (arrow) | Type conversions | `int->string`, `float->int` |

See [MIGRATION-0.9.md](/MIGRATION-0.9.md) for the full migration guide.

### OS Module ✅

Full implementation of portable OS primitives:
- Environment: `os.getenv`, `os.home-dir`, `os.current-dir`
- Paths: `os.path-exists`, `os.path-is-file`, `os.path-is-dir`, `os.path-join`, `os.path-parent`, `os.path-filename`
- Args: `args.count`, `args.at`

### FFI Phase 1 & 2 ✅

Foreign function interface for calling C libraries:
- Manifest-based declarative bindings
- String marshalling with ownership semantics
- Out parameters and fixed value arguments
- Security validation on linker flags

---

## Runtime Observability

### Current: SIGQUIT Diagnostics ✅

The `kill -3` (SIGQUIT) feature reports:
- **Strand lifecycle statistics** (lock-free atomics - zero hot-path overhead):
  - Active strand count
  - Total spawned count (monotonic)
  - Total completed count (monotonic)
  - Peak concurrent strands (high-water mark)
  - Automatic leak detection warning (spawned - completed - active > 0)
- **Lock-free strand registry** (per-strand visibility):
  - Individual strand IDs and spawn timestamps
  - Duration each strand has been running (helps detect stuck strands)
  - Configurable capacity via `SEQ_STRAND_REGISTRY_SIZE` (default: 1024)
  - Overflow tracking when registry is full
- **Channel statistics**:
  - Open channel count (global registry - accurate)

Zero overhead until signaled. All counters use lock-free atomics.
Registry uses CAS operations for registration/unregistration.

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

### Cross-Thread Memory Stats ✅

Memory statistics are now visible in SIGQUIT diagnostics, aggregated across all worker threads:

- **Arena bytes**: Total bump allocator usage across all threads
- **Pool nodes**: Free/capacity of stack node pools across all threads
- **Pool allocations**: Lifetime total allocations (monotonic counter)
- **Tracked threads**: Number of threads registered with the memory registry

**Implementation**: Thread registry pattern (similar to StrandRegistry):
- Each thread registers on first arena/pool access via CAS
- Each thread has exclusive slot for its stats (single atomic store per update)
- Diagnostics thread reads all slots during SIGQUIT (no locks, relaxed reads)
- Capacity: 64 threads (configurable via `MAX_THREADS`)

**Performance characteristics**:
- **Registration**: One-time CAS per thread (~20ns)
- **Updates**: Single atomic store per operation (~1-2 cycles, no contention)
- **Reads**: Only during diagnostics, O(64) iteration

This maintains the "fast path stays fast" principle.

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
: get-history-path ( -- String )
  "HOME" os.getenv if
    "/.myapp_history" string.concat
  else
    drop "/tmp/.myapp_history"
  then
;
```

### Phase 1: Environment & Paths ✅ Complete

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `os.getenv` | `( String -- String Int )` | Get env var, returns value and 1 on success, "" and 0 on failure |
| `os.home-dir` | `( -- String Int )` | User's home directory |
| `os.current-dir` | `( -- String Int )` | Current working directory |

### Phase 2: File System ✅ Complete

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `os.path-exists` | `( String -- Int )` | 1 if path exists, 0 otherwise |
| `os.path-is-file` | `( String -- Int )` | 1 if regular file |
| `os.path-is-dir` | `( String -- Int )` | 1 if directory |
| `os.path-join` | `( String String -- String )` | Join two path components |
| `os.path-parent` | `( String -- String Int )` | Parent directory |
| `os.path-filename` | `( String -- String Int )` | Filename component |

### Phase 3: Process & System ✅ Complete

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `args.count` | `( -- Int )` | Number of command line args |
| `args.at` | `( Int -- String )` | Get arg at index |
| `os.exit` | `( Int -- )` | Exit process with code |
| `os.name` | `( -- String )` | "darwin", "linux", "windows", "freebsd", etc. |
| `os.arch` | `( -- String )` | "x86_64", "aarch64", "arm", etc. |

### Implementation Notes

- Implemented in `crates/runtime/src/os.rs`
- C ABI exports: `patch_seq_getenv`, `patch_seq_home_dir`, etc.
- Direct builtins (no `include` required)
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
