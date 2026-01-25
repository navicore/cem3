#!/usr/bin/env python3
"""Skynet Benchmark - Python implementation
Output format: BENCH:skynet:<test>:<result>:<time_ms>

Uses asyncio for cooperative concurrency.
Note: Python asyncio is single-threaded cooperative multitasking,
not true parallelism like Go goroutines or Seq strands.
"""

import asyncio
import time

# Reduced size for Python (asyncio has more overhead)
SIZE = 100_000


async def skynet(num: int, size: int) -> int:
    if size == 1:
        return num

    child_size = size // 10
    tasks = [
        asyncio.create_task(skynet(num + i * child_size, child_size))
        for i in range(10)
    ]
    results = await asyncio.gather(*tasks)
    return sum(results)


async def main():
    start = time.perf_counter_ns()
    result = await skynet(0, SIZE)
    elapsed = (time.perf_counter_ns() - start) // 1_000_000
    print(f"BENCH:skynet:spawn-100k:{result}:{elapsed}")


if __name__ == "__main__":
    asyncio.run(main())
