#!/usr/bin/env python3
"""Fibonacci Benchmark - Python implementation
Output format: BENCH:fibonacci:<test>:<result>:<time_ms>
"""

import time


def fib_naive(n):
    if n < 2:
        return n
    return fib_naive(n - 1) + fib_naive(n - 2)


def fib_fast(n):
    if n < 2:
        return n
    a, b = 0, 1
    for _ in range(n - 1):
        a, b = b, a + b
    return b


def bench(name, n, expected, func):
    start = time.perf_counter_ns()
    result = func(n)
    elapsed_ms = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:fibonacci:{name}:{result}:{elapsed_ms}")
    if result != expected:
        print(f"ERROR: expected {expected}, got {result}")


def bench_repeated(name, n, iterations, expected, func):
    start = time.perf_counter_ns()
    result = 0
    for _ in range(iterations):
        result = func(n)
    elapsed_ms = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:fibonacci:{name}:{result}:{elapsed_ms}")
    if result != expected:
        print(f"ERROR: expected {expected}, got {result}")


def main():
    # Naive recursive tests
    bench("fib-naive-30", 30, 832040, fib_naive)
    bench("fib-naive-35", 35, 9227465, fib_naive)

    # Iterative tests (Python doesn't have TCO)
    bench("fib-fast-30", 30, 832040, fib_fast)
    bench("fib-fast-50", 50, 12586269025, fib_fast)
    bench("fib-fast-70", 70, 190392490709135, fib_fast)

    # Repeated runs
    bench_repeated("fib-naive-20-x1000", 20, 1000, 6765, fib_naive)
    bench_repeated("fib-fast-20-x1000", 20, 1000, 6765, fib_fast)


if __name__ == "__main__":
    main()
