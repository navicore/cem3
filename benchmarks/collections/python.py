#!/usr/bin/env python3
"""Collections Benchmark - Python implementation
Output format: BENCH:collections:<test>:<result>:<time_ms>
"""

import time
from functools import reduce

NUM_ELEMENTS = 100_000


def main():
    # Build
    start = time.perf_counter_ns()
    data = list(range(NUM_ELEMENTS))
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:collections:build-100k:{len(data)}:{elapsed}")

    # Map (double each)
    start = time.perf_counter_ns()
    mapped = list(map(lambda x: x * 2, data))
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:collections:map-double:{len(mapped)}:{elapsed}")

    # Filter (keep evens)
    start = time.perf_counter_ns()
    filtered = list(filter(lambda x: x % 2 == 0, data))
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:collections:filter-evens:{len(filtered)}:{elapsed}")

    # Fold (sum)
    start = time.perf_counter_ns()
    total = reduce(lambda acc, x: acc + x, data, 0)
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:collections:fold-sum:{total}:{elapsed}")

    # Chain (map -> filter -> fold)
    start = time.perf_counter_ns()
    result = reduce(
        lambda acc, x: acc + x,
        filter(lambda x: x % 2 == 0, map(lambda x: x * 3, data)),
        0,
    )
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:collections:chain:{result}:{elapsed}")


if __name__ == "__main__":
    main()
