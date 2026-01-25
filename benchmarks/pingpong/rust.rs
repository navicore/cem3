// Pingpong Benchmark - Rust implementation
// Output format: BENCH:pingpong:<test>:<result>:<time_ms>
//
// Two threads exchange messages N times using channels.
// Tests channel round-trip latency.

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const ITERATIONS: i32 = 100_000;

fn main() {
    let (ping_tx, ping_rx) = mpsc::channel();
    let (pong_tx, pong_rx) = mpsc::channel();

    let start = Instant::now();

    // Spawn pong thread
    let pong_handle = thread::spawn(move || {
        for _ in 0..ITERATIONS {
            let val = ping_rx.recv().unwrap();
            pong_tx.send(val).unwrap();
        }
    });

    // Ping in main thread
    for i in 0..ITERATIONS {
        ping_tx.send(i).unwrap();
        pong_rx.recv().unwrap();
    }

    pong_handle.join().unwrap();

    let elapsed = start.elapsed().as_millis();

    println!("BENCH:pingpong:roundtrip-100k:{}:{}", ITERATIONS, elapsed);
}
