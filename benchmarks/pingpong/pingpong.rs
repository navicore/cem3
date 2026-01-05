// Ping-Pong Benchmark - Rust version
//
// Two threads exchange messages N times.
// Tests: channel round-trip latency, context switch overhead.
// Build: rustc -O -o pingpong_rust pingpong.rs

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const ITERATIONS: i64 = 1_000_000;

fn main() {
    let (ping_tx, ping_rx) = mpsc::channel();
    let (pong_tx, pong_rx) = mpsc::channel();

    let start = Instant::now();

    // Spawn pong thread
    let pong_handle = thread::spawn(move || {
        for _ in 0..ITERATIONS {
            let val: i32 = ping_rx.recv().unwrap();
            pong_tx.send(val).unwrap();
        }
    });

    // Ping in main thread
    for i in 0..ITERATIONS {
        ping_tx.send(i as i32).unwrap();
        let _ = pong_rx.recv().unwrap();
    }

    pong_handle.join().unwrap();

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    println!("{} round trips in {} ms", ITERATIONS, elapsed_ms);

    // Calculate throughput
    let total_messages = ITERATIONS * 2;
    let msgs_per_sec = if elapsed_ms > 0 {
        total_messages * 1000 / elapsed_ms
    } else {
        0
    };
    println!("Throughput: {} msg/sec", msgs_per_sec);
}
