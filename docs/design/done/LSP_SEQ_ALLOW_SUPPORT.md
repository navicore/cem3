# LSP Support for `seq:allow` Annotations

Forgejo issue: [#493](https://git.navicore.tech/navicore/patch-seq/issues/493)

## Intent

Make `# seq:allow(lint-id)` annotations discoverable and verifiable from the
editor. Today they are a hidden API: the parser silently accepts them, the
linter silently consumes them, and the LSP says nothing — so users either
copy from examples or read source to find valid IDs, and a typo
(`unckecked-tcp-write`) becomes a silent no-op that the lint they meant to
suppress keeps firing under.

We're not adding any new annotations, changing semantics, or encouraging
suppression — just exposing what already exists so the existing escape
hatch is usable without grep.

## Constraints

- No new annotation syntax. Form stays `# seq:allow(lint-id)`, parsed at
  `crates/compiler/src/parser/cursor.rs:32`.
- No change to how the parser collects allow-list IDs or how the linter
  applies them (`Linter::lint_word` filter at `lint/linter.rs:92`).
- Comments remain a no-completion zone *in general* — only the specific
  `seq:allow(` prefix should open the completion popup. Free-text comments
  must not start spamming completions.
- Single source of truth for the ID list. Adding a new lint to `lints.toml`,
  or registering a new hard-coded analyzer (like `unchecked-error-flag`,
  `unreachable-chan-yield`, `deep-nesting`), must light up in completion
  and pass the unknown-id check automatically. No second list to maintain.
- Out of scope: file-level allow scoping, allow-many-on-one-line, fix-its
  that *insert* `seq:allow`, "dead allow" detection (annotation that
  doesn't suppress anything).

## Approach

Three small pieces, all in already-existing files.

**1. A lint-ID registry in `seq-compiler`.**
Add `seqc::lint::known_lint_ids() -> Vec<(&'static str, &'static str, Severity)>`
that unions:
- the IDs in `lints.toml` (from `LintConfig::default_config()`),
- the three hard-coded ones: `"deep-nesting"` (`lint/linter.rs:71`),
  `"unchecked-error-flag"` (`error_flag_lint/analyzer.rs:349`),
  `"unreachable-chan-yield"` (`chan_yield_lint.rs:31`).

The hard-coded set is small and stable enough to enumerate inline; if it
grows, we'll refactor the analyzers to self-register, but not yet.

**2. Completion in the LSP.**
In `crates/lsp/src/completion.rs`:
- Add `ContextType::LintAllow`.
- In `detect_context`, before the "is in comment" check, detect a line
  prefix matching `(^|\s)#\s*seq:allow\(` with no closing `)` after the
  cursor — same prefix-only logic the comment detector already uses,
  scoped to the `seq:allow(` form.
- For that context, return one `CompletionItem` per `known_lint_ids()`
  entry, with the lint's message as the documentation and severity as
  the detail.

**3. Diagnostic for unknown IDs.**
In `crates/lsp/src/diagnostics.rs::check_document_with_quotations`, after
parsing, walk `program.words[*].allowed_lints` (already populated by the
parser) and emit a `DiagnosticSeverity::WARNING` for any ID not in
`known_lint_ids()`. Range is the word's source span — we don't have a
span for the annotation comment itself, but the word span is close enough
for the editor lightbulb to find it.

## Domain Events

- **Produces**: `LintAllowCompletion` (popup contents inside
  `seq:allow(`), `UnknownLintIdDiagnostic` (typo'd ID).
- **Consumes**: parsed `Program` (for `WordDef.allowed_lints`), lint
  registry (for the ID set), cursor's line prefix (for context detection).
- **Must follow**: the registry is the only place lint IDs are listed for
  human-facing tools. Future lints — including stdlib-side rules added by
  users via the merge path in `LintConfig::merge` — flow through the same
  function. User-merged rules should be included once we wire merge into
  the LSP (currently the LSP only loads defaults at
  `diagnostics.rs:193`); that's a follow-up, not a blocker for #493.

## Checkpoints

- Typing `# seq:allow(` in an LSP-attached `.seq` buffer opens a popup
  listing every ID from `lints.toml` plus `deep-nesting`,
  `unchecked-error-flag`, `unreachable-chan-yield`. Each item shows the
  lint's message.
- `# seq:allow(unckecked-tcp-write)` (typo) above a `: foo ... ;` shows a
  warning diagnostic referencing the word; `# seq:allow(unchecked-tcp-write)`
  (correct) shows none.
- The existing `# seq:allow(deep-nesting)` annotations in
  `stdlib/yaml.seq` and `stdlib/json.seq` do not regress — no new
  diagnostic, lint still suppressed.
- `just test` passes including the existing parser tests at
  `parser/tests.rs:1649` and the error-flag suppression test at
  `error_flag_lint/tests.rs:174`.
- One new test per piece: a completion test asserting `LintAllow` context
  yields a known ID; a diagnostic test asserting a typo'd ID surfaces as
  a warning; a regression test asserting a correct ID does not.
