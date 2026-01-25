#!/usr/bin/env python3
"""Pingpong Benchmark - Python implementation
Output format: BENCH:pingpong:<test>:<result>:<time_ms>

Uses asyncio queues for channel-like behavior.
Note: Python asyncio is single-threaded cooperative multitasking.
"""

import asyncio
import time

ITERATIONS = 100_000


async def pong(ping_queue, pong_queue, count):
    for _ in range(count):
        val = await ping_queue.get()
        await pong_queue.put(val)


async def ping(ping_queue, pong_queue, count):
    for i in range(count):
        await ping_queue.put(i)
        await pong_queue.get()


async def main():
    ping_queue = asyncio.Queue()
    pong_queue = asyncio.Queue()

    start = time.perf_counter_ns()

    # Start pong task
    pong_task = asyncio.create_task(pong(ping_queue, pong_queue, ITERATIONS))

    # Run ping
    await ping(ping_queue, pong_queue, ITERATIONS)

    # Wait for pong to finish
    await pong_task

    elapsed = (time.perf_counter_ns() - start) // 1_000_000

    print(f"BENCH:pingpong:roundtrip-100k:{ITERATIONS}:{elapsed}")


if __name__ == "__main__":
    asyncio.run(main())
