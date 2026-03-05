# Seq Development Roadmap

## Core Values

**The fast path stays fast.** Observability is opt-in and zero-cost when disabled. We can't slow the system down to monitor it.

Inspired by the Tokio ecosystem (tokio-console, tracing, metrics, tower), we aspire to rich runtime visibility while respecting performance.

---

## Current (v4.0)

### Union Type Safety

Compile-time safety for union types (RFC #345). The compiler auto-generates
type-safe constructors, predicates (`is-Get?`), and field accessors
(`Get-response_chan`) for all union variants. See [MIGRATION_4.0.md](/MIGRATION_4.0.md).

### Error Handling Standardization (v3.0)

All fallible operations return `(value Bool)` instead of panicking.
Division, TCP, regex, and other operations now consistently use this pattern.
See [MIGRATION_3.0.md](/MIGRATION_3.0.md).

---

## Foundation (Complete)

These features are stable and documented:

| Feature | Details |
|---------|---------|
| **Naming conventions** | Dot for namespaces, hyphen for compounds, arrow for conversions |
| **OS module** | `os.getenv`, `os.home-dir`, `os.path-*`, `args.count`, `args.at`, `os.exit`, `os.name`, `os.arch` |
| **FFI** | Manifest-based C bindings, string marshalling, out parameters. Examples: libedit, SQLite |
| **Runtime observability** | SIGQUIT diagnostics, watchdog timer, strand/channel/memory stats |
| **Yield safety valve** | Automatic yields in tight loops to prevent strand starvation |
| **LSP server** | Language server with completions, hover, diagnostics. Powers TUI and editor integrations |
| **TUI REPL** | Default REPL with split-pane IR visualization, Vi editing, tab completion |
| **Union types & match** | `union` definitions, `match` expressions, exhaustiveness checking, named bindings |
| **Closures** | Lexical capture with Arc-shared environments, TCO-compatible |
| **Channels as values** | First-class `Channel` type on the stack (no ID-based lookup) |
| **Weaves** | Generator/coroutine pattern with bidirectional communication |
| **Lists & Maps** | First-class collection types with `list.map`, `list.filter`, `list.fold`, `map.*` |
| **Symbols** | `:foo` syntax for lightweight identifiers, used for variant tags |
| **Lint tool** | `seqc lint` with TOML-based syntactic pattern matching |

---

## Future

### Strand Visibility

**Strand lifecycle events** (opt-in):
- Parent-child relationships for debugging actor hierarchies
- Blocked strand detection (who's waiting on what)
- Optional compile-time flag to enable

### Metrics & Tracing

**Metrics export**:
- Prometheus-compatible endpoint
- Strand pool utilization
- Message throughput sampling
- Configurable sampling rates to control overhead

**Structured tracing**:
- Integration with tracing ecosystem
- Span-based request tracking across strands
- Correlation IDs for distributed debugging

### Visual Tooling

**Seq console** (inspired by tokio-console):
- Real-time strand visualization
- Channel flow graphs
- Actor hierarchy browser
- Historical replay for post-mortem debugging

**OpenTelemetry integration**:
- Distributed tracing across services
- Standard observability pipeline integration

### FFI Phase 3

- Struct passing
- Platform-specific bindings
- Callback support (C -> Seq) - *shelved*: most useful callback patterns require low-level memory operations; many C APIs have non-callback alternatives

### Type System Research

**Goal**: Achieve the safety benefits of generics without sacrificing point-free composability or adding syntactic overhead.

Seq's philosophy: type safety through inference, not annotation.

**Current state**:
- Row-polymorphic stack effects provide implicit type threading
- Union types with nominal typing and auto-generated accessors (v4.0)
- `(value Bool)` error handling pattern (v3.0)

**Research directions**:

1. **Inferred variant types** - Compiler tracks that `Make-Ok` produces a specific union type
2. **Flow typing through combinators** - If `result-bind` receives `IntResult`, infer the quotation expects `Int`
3. **Structural typing for conventions** - Recognize Result-like patterns at compile time
4. **Constructor argument refinement** - `42 Make-Ok` infers `IntResult` from the `Int` argument

**Key question**: How far can we push implicit typing before explicit annotations become necessary?

**Constraint**: Must not compromise point-free style or add syntactic noise.

---

## Design Documents

- [FFI Design](design/ffi.md)
- [FFI Callbacks](design/ffi-callbacks.md) *(shelved)*
- [ADT Design](design/ADT_DESIGN.md)
- [Lint Design](design/LINT_DESIGN.md)
- [SON Design](design/SON_DESIGN.md)
- [Tagged Stack Codegen](design/tagged-stack-codegen.md) *(historical - superseded by current 40-byte StackValue design)*
