#!/usr/bin/env python3
"""Primes Benchmark - Python implementation
Output format: BENCH:primes:<test>:<result>:<time_ms>
"""

import time
import math


def is_prime(n):
    if n < 2:
        return False
    if n == 2:
        return True
    if n % 2 == 0:
        return False
    for i in range(3, int(math.sqrt(n)) + 1, 2):
        if n % i == 0:
            return False
    return True


def count_primes(limit):
    count = 0
    for n in range(2, limit + 1):
        if is_prime(n):
            count += 1
    return count


def main():
    # count-primes-10k
    start = time.perf_counter_ns()
    result = count_primes(10000)
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:primes:count-10k:{result}:{elapsed}")

    # count-primes-100k
    start = time.perf_counter_ns()
    result = count_primes(100000)
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:primes:count-100k:{result}:{elapsed}")


if __name__ == "__main__":
    main()
