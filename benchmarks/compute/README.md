# Compute Benchmarks

Pure computation benchmarks comparing Seq, Rust, and Go performance.
No I/O, no concurrency - just raw number crunching.

## Benchmarks

### Fibonacci (Recursive)

Calculates `fib(40)` using naive recursive implementation.

**Why naive recursion?** The exponential O(2^n) algorithm is intentional - it stress-tests
function call overhead and stack operations, which are key performance indicators for
interpreted languages. An iterative or memoized version would mostly measure loop overhead.

**Tests:** Function call overhead, recursion depth, integer arithmetic

**Expected result:** 102,334,155

### Sum of Squares

Calculates sum of `i^2` for `i` in `1..1,000,000`.

**Tests:** Loop iteration, integer multiplication, accumulation

**Expected result:** 333,333,833,333,500,000

**Note:** The limit of 1M is safe for i64. Limits above ~3M risk overflow.

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

Results from MacBook Pro M-series:

| Benchmark | Seq | Rust | Go | Seq/Rust |
|-----------|-----|------|-----|----------|
| fib(40) | 2500ms | 168ms | 224ms | 15x |
| sum_squares | 54ms | 2ms | 2ms | 29x |
| primes(100k) | 93ms | 3ms | 3ms | 29x |

## Interpreting Results

For an interpreted language, 15-50x slower than native code is typical:
- **10-20x**: Good - efficient interpreter or JIT
- **20-50x**: Expected - standard interpreter overhead
- **>50x**: Investigate - potential inefficiency in codegen

## Notes

- All implementations use equivalent algorithms (no SIMD, no parallelism)
- Rust compiled with `-O` (optimizations enabled)
- Go compiled with default optimizations
- Seq compiled with `seqc build` (LLVM backend with optimizations)
