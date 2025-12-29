# Lint Suppression Mechanism

This document describes the Option B lint suppression mechanism implemented for Patch-Seq.

## Syntax

Use the `@allow:<lint-id>` annotation before a word call to suppress specific lint warnings:

```seq
: intentional-drop ( Int -- )
  # Suppress the unchecked-chan-receive lint for this specific call
  @allow:unchecked-chan-receive chan.receive drop
;
```

## Features

- **Explicit**: You must specify which lint you're suppressing (`@allow:unchecked-chan-receive`)
- **Scoped**: The suppression applies only to the next statement
- **Compositional**: You can chain multiple suppressions for the same statement
- **Friction by design**: The prefix syntax makes suppression slightly awkward to discourage overuse

## Multiple Suppressions

You can suppress multiple lints for the same statement by chaining annotations:

```seq
: example ( -- )
  @allow:lint-a @allow:lint-b word
;
```

## Available Lint IDs

Check `crates/compiler/src/lints.toml` for the full list of lint IDs. Common ones include:

- `unchecked-chan-receive` - Dropping channel receive success flag
- `unchecked-map-get` - Dropping map get success flag
- `prefer-nip` - Using `swap drop` instead of `nip`
- `redundant-dup-drop` - No-op `dup drop` sequence

## Design Rationale

This implements **Option B** from issue #135. Key design decisions:

1. **Stack annotation word syntax**: Uses `@allow:` prefix rather than comments
2. **Friction as a feature**: Slightly awkward to prevent desensitization
3. **Explicit lint IDs**: Forces programmer to understand what they're suppressing
4. **Not a substitute for type system**: Suppressions are temporary workarounds until the type system can enforce the invariant (see issue #134)

## Implementation

- **Parser**: Recognizes `@allow:<lint-id>` tokens and passes metadata to next statement
- **AST**: `Statement::WordCall` now carries `suppressed_lints: Vec<String>`
- **Linter**: Checks suppression list before reporting diagnostics

## Future Work

- Track suppression metrics in CI (count per file)
- Enhance type system to reduce need for suppressions (issue #134)
- Consider warning on unused suppressions
