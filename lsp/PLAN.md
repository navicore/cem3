# LSP Include-Aware Completion Plan

## Goal
Provide completions for words defined in included modules, with context-aware filtering to avoid nonsensical suggestions.

## Architecture

### 1. Document State Tracking

The LSP server needs to track per-document state:

```rust
struct DocumentState {
    /// Document content
    content: String,
    /// File path (for resolving relative includes)
    file_path: Option<PathBuf>,
    /// Parsed includes from this document
    includes: Vec<Include>,
    /// Words available from includes (cached)
    included_words: Vec<IncludedWord>,
    /// Words defined in this document
    local_words: Vec<LocalWord>,
}

struct IncludedWord {
    name: String,
    effect: Option<Effect>,
    source_module: String,  // e.g., "std:json" or "utils"
}

struct LocalWord {
    name: String,
    effect: Option<Effect>,
    line: u32,  // For go-to-definition later
}
```

### 2. Include Resolution

Reuse compiler's resolver logic but adapted for LSP:
- Need to find stdlib path (check common locations or use env var)
- Parse included files to extract word definitions
- Don't need full compilation, just parsing

```rust
fn resolve_includes(doc_path: &Path, content: &str) -> Vec<IncludedWord> {
    // 1. Parse document to get Include statements
    // 2. Resolve each include path
    // 3. Parse included file
    // 4. Extract word names and effects
    // 5. Recurse for nested includes (with cycle detection)
}
```

### 3. Context-Aware Filtering

Filter completions based on cursor context:

| Context | Show | Don't Show |
|---------|------|------------|
| After `include ` | Module names | Words, keywords |
| After `include std:` | Stdlib module names | Everything else |
| Inside string `"..."` | Nothing | Everything |
| Inside comment `#...` | Nothing | Everything |
| After `:` (word def start) | Nothing | Everything |
| After `( ` (stack effect) | Type names | Words |
| Normal code context | Words, builtins, keywords | Module names |

### 4. Caching Strategy

Re-parsing on every keystroke is expensive. Cache at two levels:

1. **Per-document cache**: Invalidate when document changes
2. **Per-include cache**: Invalidate based on file mtime (or just session-based)

For simplicity, start with session-based caching:
- Parse includes once when document opens
- Re-parse includes when document is saved (did_save event)
- Clear cache when document closes

### 5. Stdlib Path Discovery

Try these locations in order:
1. `SEQ_STDLIB_PATH` environment variable
2. Relative to `seq-lsp` binary: `../stdlib/`
3. Common install locations: `~/.local/share/seq/stdlib/`

## Implementation Steps

### Phase 1: Document Path Tracking
- Store URI → file path mapping
- Extract file path from URI in did_open

### Phase 2: Include Parsing
- Add function to parse document and extract Include statements
- Add function to resolve include paths
- Add function to parse a .seq file and extract WordDefs

### Phase 3: Word Extraction
- Extract word name and effect from WordDef
- Format effect for display (reuse existing format_effect)
- Track source module for documentation

### Phase 4: Caching
- Add included_words cache to DocumentState
- Populate on did_open
- Refresh on did_save

### Phase 5: Context Filtering
- Detect string context (count unescaped quotes)
- Detect comment context (# to end of line)
- Detect word definition context (: name ... ;)
- Detect stack effect context (inside parens after :)
- Detect include context (already implemented)

### Phase 6: Integration
- Merge included words with builtins in completion response
- Add source module to documentation
- Test with real .seq files

## Files to Modify

- `lsp/src/main.rs` - Document state, path tracking
- `lsp/src/completion.rs` - Context filtering, include word completions
- `lsp/src/lib.rs` (new) - Shared types if needed
- `lsp/src/includes.rs` (new) - Include resolution logic

## Testing Plan

1. Manual testing with neovim
2. Unit tests for context detection
3. Unit tests for include path resolution

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Slow on large includes | Cache aggressively, parse lazily |
| Circular includes | Track visited files, limit depth |
| Invalid include syntax | Graceful degradation, show what we can |
| Missing stdlib | Log warning, continue without stdlib completions |
| Path resolution fails | Fall back to builtin-only completions |

## Future Enhancements (Not This PR)

- Hover to show word signatures from includes
- Go-to-definition for included words
- Diagnostics for undefined words from includes
- Auto-import suggestions
