# Compute Benchmarks

Pure computation benchmarks comparing Seq, Rust, and Go performance.
No I/O, no concurrency - just raw number crunching.

## Benchmarks

### Fibonacci (Recursive)

Calculates `fib(40)` using naive recursive implementation.

**Tests:** Function call overhead, recursion depth, integer arithmetic

**Expected result:** 102,334,155

### Sum of Squares

Calculates sum of `i^2` for `i` in `1..1,000,000`.

**Tests:** Loop iteration, integer multiplication, accumulation

**Expected result:** 333,333,833,333,500,000

### Prime Count

Counts primes up to 100,000 using trial division.

**Tests:** Nested loops, modulo operation, conditionals

**Expected result:** 9,592 primes

## Running

```bash
# From project root
just bench-compute

# Or manually
cd benchmarks/compute
./run.sh
```

## Sample Results

| Benchmark | Seq | Rust | Go | Seq/Rust | Seq/Go |
|-----------|-----|------|-----|----------|--------|
| fib(40) | TBD | TBD | TBD | TBD | TBD |
| sum-squares | TBD | TBD | TBD | TBD | TBD |
| primes(100k) | TBD | TBD | TBD | TBD | TBD |

## Notes

- All implementations use equivalent algorithms (no SIMD, no parallelism)
- Rust compiled with `--release` (optimizations enabled)
- Go compiled with default optimizations
- Seq compiled with `seqc build` (LLVM optimizations enabled)
