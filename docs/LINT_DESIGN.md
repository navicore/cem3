# Seq Lint Tool Design

A clippy-inspired lint tool for Seq, usable both as a CLI command and via seq-lsp.

## Goals

1. **Extensible patterns** - adding new lints shouldn't require re-architecting
2. **Fast feedback** - suitable for real-time LSP diagnostics
3. **Embedded defaults** - ships with sensible defaults, user can override
4. **Incremental design** - start syntactic, add type-awareness later

## Prior Art

| Tool | Approach | Extensibility |
|------|----------|---------------|
| Clippy | Lints in Rust, uses rustc internals | Requires recompile |
| Semgrep | YAML config + pattern DSL | Config-driven, no recompile |
| tree-sitter queries | S-expr pattern language | Query files, no recompile |
| ESLint | JS rules + AST selectors | Plugin-based |

We lean toward **Semgrep-style**: patterns expressed as Seq code snippets with wildcards, stored in TOML config.

## Why This Fits Seq

Concatenative languages have a key property: code is a linear sequence of words. Pattern matching is simpler than tree-based languages. Many lints are literally "see this sequence, suggest that."

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ Pattern DB  │────▶│  Lint Engine │◀────│ Seq AST     │
│ (TOML)      │     │              │     │ + Type Info │
└─────────────┘     └──────┬───────┘     └─────────────┘
                           │
                    ┌──────▼───────┐
                    │  Diagnostics │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              ▼                         ▼
        CLI (batch)               LSP (real-time)
```

## Configuration

- Default config embedded in binary via `include_str!("lints.toml")`
- User override via env var or CLI flag pointing to custom TOML file
- Configs merge: user patterns add to or override defaults

---

## Phase 1: Syntactic Patterns

Simple word sequence matching without type information.

### Pattern Language

- **Literals** - exact word matching
- **Wildcards** - `$X` (single word), `$...` (any sequence)

### Config Format

```toml
[[lint]]
id = "redundant-dup-drop"
pattern = "dup drop"
replacement = ""
message = "`dup drop` has no effect"
severity = "warning"

[[lint]]
id = "redundant-swap-swap"
pattern = "swap swap"
replacement = ""
message = "consecutive swaps cancel out"
severity = "warning"

[[lint]]
id = "prefer-nip"
pattern = "swap drop"
replacement = "nip"
message = "prefer `nip` over `swap drop`"
severity = "hint"

[[lint]]
id = "redundant-over-drop"
pattern = "over drop"
replacement = ""
message = "`over drop` has no effect"
severity = "warning"

[[lint]]
id = "redundant-dup-nip"
pattern = "dup nip"
replacement = ""
message = "`dup nip` has no effect"
severity = "warning"
```

### Severity Levels

- `error` - likely a bug
- `warning` - code smell or inefficiency
- `hint` - stylistic suggestion

### Implementation Pieces

1. **`LintConfig`** - parsed TOML structure
2. **`Pattern`** - compiled pattern representation
3. **`Matcher`** - walks AST, finds pattern matches
4. **`Diagnostic`** - LSP-compatible output (file, span, message, severity)

### Integration Points

- **CLI**: `seqc lint <file>` or `seqc lint src/`
- **LSP**: `textDocument/publishDiagnostics` on file open/save/change

---

## Phase 2: Type-Aware Patterns

Add type constraints to patterns, leveraging the existing type checker.

### Extended Pattern Language

Add `where` clause for type constraints:

```toml
[[lint]]
id = "redundant-int-conversion"
pattern = "int->string string->int"
where = "input_type($1) == Int"
replacement = ""
message = "round-trip conversion has no effect"
severity = "warning"

[[lint]]
id = "unused-dup"
pattern = "dup $X"
where = "stack_effect($X).consumes < 2"
message = "duplicated value is never used"
severity = "warning"

[[lint]]
id = "quotation-ignores-arg"
pattern = "[ drop $... ]"
message = "quotation immediately drops its argument"
severity = "hint"
```

### Type Constraint Predicates

- `input_type($X)` - type consumed by word
- `output_type($X)` - type produced by word
- `stack_effect($X).consumes` - number of values consumed
- `stack_effect($X).produces` - number of values produced
- `is_pure($X)` - word has no side effects

---

## Future Considerations

### Patterns Spanning Control Flow

Detecting issues across if/else branches:

```toml
[[lint]]
id = "unbalanced-branches"
pattern = "if $then else $else then"
where = "stack_delta($then) != stack_delta($else)"
message = "branches have different stack effects"
severity = "error"
```

(Note: the type checker already catches this - but lint could give better messages)

### Learning From the Codebase

Sources to mine for idiomatic patterns:
- `stdlib/` - standard library implementations
- `examples/` - example programs
- `../seq-lisp/` - Lisp interpreter implementation

### Pattern Discovery

Could potentially analyze the codebase to find:
- Common idioms to encourage
- Repeated verbose patterns that could be named words
- Unusual patterns that might indicate bugs

---

## Open Questions

1. **Pattern compilation** - use Aho-Corasick for multi-pattern matching, or simpler linear scan?
2. **Incremental matching** - for LSP, only re-lint changed regions?
3. **Auto-fix** - should `seqc lint --fix` apply replacements automatically?
4. **User-defined patterns** - project-local `.seq-lint.toml` in addition to global config?

---

## References

- [Semgrep](https://semgrep.dev/) - pattern-based code analysis
- [Clippy](https://github.com/rust-lang/rust-clippy) - Rust linter
- [tree-sitter queries](https://tree-sitter.github.io/tree-sitter/using-parsers#pattern-matching-with-queries)
