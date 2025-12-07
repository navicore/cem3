# LTO Investigation

This document records our investigation into Link-Time Optimization (LTO) for
cross-boundary inlining between Seq-generated code and the Rust runtime.

## Background

Seq compiles to LLVM IR that calls into a Rust runtime library. Each stack
operation (push, pop, add, etc.) is a function call:

```llvm
define ptr @seq_add_nums(ptr %stack) {
  %0 = call ptr @patch_seq_push_int(ptr %stack, i64 1)
  %1 = call ptr @patch_seq_push_int(ptr %0, i64 2)
  %2 = call ptr @patch_seq_add(ptr %1)
  ret ptr %2
}
```

We investigated whether LTO could inline these runtime calls to reduce overhead.

## Investigation (December 2024)

### Approach

1. Compile runtime to LLVM bitcode: `RUSTFLAGS="--emit=llvm-bc" cargo build`
2. Link Seq IR with runtime bitcode: `llvm-link seq.bc runtime.bc`
3. Run LLVM optimizer: `opt -O3 linked.bc`

### Findings

#### LTO Does Work (With Configuration)

We successfully got LLVM to inline runtime functions. Requirements:

| Requirement | Details |
|------------|---------|
| LLVM version match | Rust 1.91 uses LLVM 21; must use matching llvm tools |
| Function attributes | Must match `target-cpu`, `frame-pointer`, `probe-stack` |
| Calling convention | `tailcc` in Seq functions blocks inlining into C-convention runtime |
| Exception handling | Functions need matching `personality` attributes |

Example of successful inlining remark:
```
'patch_seq_push_int' inlined into 'seq_add_nums' with (cost=50, threshold=250)
```

#### But It Doesn't Help

Aggressive inlining actually **increased** code size:

| Metric | Before | After |
|--------|--------|-------|
| Stack frame | 64 bytes | 144 bytes |
| Saved registers | 4 | 6 |
| Code complexity | Simple calls | Inlined pool logic |

The pool allocator has cost ~410 (above default threshold 250). When inlined,
it brings in:
- Thread-local storage access
- Conditional pool vs heap allocation paths
- Reference counting logic
- Exception handling blocks

### Why Inlining Doesn't Simplify

LLVM cannot perform the optimizations that would actually help:

1. **Constant folding across calls**: `1 2 add` cannot become `3` because LLVM
   doesn't understand the stack semantics

2. **Eliminate redundant push/pop**: `push 1; pop` cannot be removed because
   LLVM sees opaque function calls, not stack operations

3. **Pool allocation is irreducible**: The thread-local pool with fallback to
   heap cannot be simplified - the branches are semantically necessary

### What Would Actually Help

| Optimization | Impact | Implementation |
|-------------|--------|----------------|
| Compile-time evaluation | `1 2 add` → `push 3` | Partial evaluator in compiler |
| Unboxed primitives | Keep ints in registers | Major runtime redesign |
| Inline caching | Fast type dispatch | Runtime modification |
| Tracing JIT | Dynamic optimization | Different architecture entirely |

## Decision

**We are not pursuing LTO.** The current architecture is appropriate because:

1. **Pooled allocation is already fast** - ~10x faster than malloc, consistent
   performance

2. **Function call overhead is minimal** - A few nanoseconds per operation is
   acceptable for a high-level language

3. **Clean separation** - Runtime as a separate library is easier to maintain,
   test, and reason about

4. **Predictable performance** - Current overhead is consistent rather than
   varying based on inlining heuristics

5. **Complexity cost** - LTO would require:
   - Matching LLVM versions between Rust and clang
   - Emitting compatible function attributes
   - Managing bitcode in the build system
   - Debugging across inlined boundaries

The effort-to-benefit ratio does not justify the added complexity.

## Future Considerations

If performance becomes critical, consider:

1. **Partial evaluation** - Fold constants at compile time
2. **Specialized builtins** - Generate inline IR for common patterns like
   `dup add` (double) or `swap drop` (nip)
3. **Profile-guided optimization** - Focus optimization on hot paths

## Reproduction

To reproduce this investigation:

```bash
# Build runtime with bitcode
RUSTFLAGS="--emit=llvm-bc" cargo build -p seq-runtime --release

# Find the bitcode
ls target/release/deps/seq_runtime*.bc

# Create test program
echo ': add-nums 1 2 add ;
: main add-nums drop ;' > /tmp/test.seq
./target/release/seqc /tmp/test.seq --output /tmp/test --keep-ir

# Link (adjust LLVM path and bitcode filename)
llvm-link /tmp/test.ll target/release/deps/seq_runtime-*.bc -o /tmp/linked.bc

# Optimize with remarks
opt -O3 -pass-remarks=inline -pass-remarks-missed=inline /tmp/linked.bc -o /tmp/opt.bc
```

Note: You'll likely see "conflicting attributes" unless you modify the Seq IR
to include matching target attributes.
