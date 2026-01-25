#!/usr/bin/env python3
"""Fanout Benchmark - Python implementation
Output format: BENCH:fanout:<test>:<result>:<time_ms>

1 producer, N consumer workers using asyncio.
"""

import asyncio
import time

NUM_MESSAGES = 100_000
NUM_WORKERS = 10


async def worker(work_queue, done_queue):
    count = 0
    while True:
        val = await work_queue.get()
        if val < 0:  # sentinel
            await done_queue.put(count)
            break
        count += 1
        await asyncio.sleep(0)  # yield


async def producer(work_queue, count):
    for i in range(count):
        await work_queue.put(i)


async def main():
    work_queue = asyncio.Queue()
    done_queue = asyncio.Queue()

    # Spawn workers
    workers = [
        asyncio.create_task(worker(work_queue, done_queue))
        for _ in range(NUM_WORKERS)
    ]

    start = time.perf_counter_ns()

    # Produce messages
    await producer(work_queue, NUM_MESSAGES)

    # Send sentinels
    for _ in range(NUM_WORKERS):
        await work_queue.put(-1)

    # Collect results
    total = 0
    for _ in range(NUM_WORKERS):
        total += await done_queue.get()

    # Wait for workers
    await asyncio.gather(*workers)

    elapsed = (time.perf_counter_ns() - start) // 1_000_000

    print(f"BENCH:fanout:throughput-100k:{total}:{elapsed}")


if __name__ == "__main__":
    asyncio.run(main())
