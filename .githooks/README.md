# Git Hooks

This directory contains git hooks for the project.

## Setup

Enable the hooks by pointing git to this directory:

```bash
git config core.hooksPath .githooks
```

This is a per-repo setting and needs to be run once after cloning.

## Available Hooks

### pre-commit

Prevents accidentally committing binary executables (Mach-O on macOS, ELF on Linux).

**What it catches:**
- Compiled binaries without extensions
- Executables accidentally staged with `git add .`

**What it allows:**
- Scripts (Python, shell, etc.)
- Object files, libraries (add to .gitignore separately if needed)
- Any non-executable binary

**If blocked:**
```
BLOCKED: my_test_binary
  Type: Mach-O 64-bit executable arm64
  Remove with: git reset HEAD "my_test_binary"
```

To bypass in exceptional cases (not recommended):
```bash
git commit --no-verify
```
