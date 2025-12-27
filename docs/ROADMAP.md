# Seq Development Roadmap

## Core Values

**The fast path stays fast.** Observability is opt-in and zero-cost when disabled. We can't slow the system down to monitor it.

Inspired by the Tokio ecosystem (tokio-console, tracing, metrics, tower), we aspire to rich runtime visibility while respecting performance.

---

## Recent (v0.14)

### TUI REPL

Split-pane terminal interface for interactive development:
- Vi mode editing with syntax highlighting
- Real-time IR visualization (stack effects, typed AST, LLVM IR)
- ASCII art stack effect diagrams
- LSP-powered tab completion
- Session file management with `:edit` to open in $EDITOR

Launch with `seqr --tui` or `seqr --tui myfile.seq`.

---

## Foundation (Complete)

These features are stable and documented:

| Feature | Version | Details |
|---------|---------|---------|
| **Naming conventions** | v0.9 | Dot for namespaces, hyphen for compounds, arrow for conversions. See [MIGRATION-0.9.md](/MIGRATION-0.9.md) |
| **OS module** | v0.10 | `os.getenv`, `os.home-dir`, `os.path-*`, `args.count`, `args.at`, `os.exit`, `os.name`, `os.arch` |
| **FFI** | v0.11 | Manifest-based C bindings, string marshalling, out parameters. Examples: libedit, SQLite |
| **Runtime observability** | v0.12 | SIGQUIT diagnostics, watchdog timer, strand/channel/memory stats |
| **Yield safety valve** | v0.13 | Automatic yields in tight loops to prevent strand starvation |

---

## In Progress

### Strand Visibility

**Strand lifecycle events** (opt-in):
- Spawn/exit tracing with strand IDs
- Parent-child relationships for debugging actor hierarchies
- Optional compile-time flag to enable

**Advanced channel diagnostics**:
- Blocked strand detection (who's waiting on what)

---

## Future

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
- Result/Option helpers use duck typing on variant tags
- Users define concrete unions (`IntResult`, `StringResult`) for their use cases

**Research directions**:

1. **Inferred variant types** - Compiler tracks that `Make-Ok` produces a specific union type
2. **Flow typing through combinators** - If `result-bind` receives `IntResult`, infer the quotation expects `Int`
3. **Structural typing for conventions** - Recognize Result-like patterns at compile time
4. **Constructor argument refinement** - `42 Make-Ok` infers `IntResult` from the `Int` argument

**Key question**: How far can we push implicit typing before explicit annotations become necessary?

**Constraint**: Must not compromise point-free style or add syntactic noise.

### CLI & Developer Experience

**Tab completion for Seq-based CLI tools**:
- Word name completion in interactive prompts
- Context-aware completions
- File path completion

**Potential paths**:
1. Custom completion protocol with manual key handling
2. LSP integration for editor/IDE support

---

## Design Documents

- [FFI Design](design/ffi.md)
- [TUI Plan](/.claude/plans/) (internal)
