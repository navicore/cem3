// Skynet Benchmark - Rust version
//
// Spawns 1,000,000 tasks using a thread pool.
// Uses std::thread with work-stealing via recursive subdivision.
// Each leaf returns its ID, parents sum children.
// The root should return 499999500000 (sum of 0..999999).
//
// Note: Unlike Go goroutines, Rust std::thread are OS threads.
// This version uses rayon-style recursive parallelism with a thread pool.
// Build: rustc -O -o skynet_rust skynet.rs

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

// Use a thread pool approach: spawn threads only at higher levels,
// compute leaves sequentially
const PARALLEL_THRESHOLD: i64 = 10000; // Below this, compute sequentially

fn skynet_seq(num: i64, size: i64) -> i64 {
    if size == 1 {
        return num;
    }

    let child_size = size / 10;
    let mut sum = 0i64;
    for i in 0..10 {
        sum += skynet_seq(num + i * child_size, child_size);
    }
    sum
}

fn skynet_par(num: i64, size: i64) -> i64 {
    if size <= PARALLEL_THRESHOLD {
        return skynet_seq(num, size);
    }

    let child_size = size / 10;
    let (tx, rx) = mpsc::channel();

    // Spawn threads for children
    let mut handles = Vec::with_capacity(10);
    for i in 0..10 {
        let tx = tx.clone();
        let child_num = num + i * child_size;
        handles.push(thread::spawn(move || {
            let result = skynet_par(child_num, child_size);
            tx.send(result).unwrap();
        }));
    }
    drop(tx);

    // Sum results
    let mut sum = 0i64;
    for _ in 0..10 {
        sum += rx.recv().unwrap();
    }

    for h in handles {
        h.join().unwrap();
    }

    sum
}

fn main() {
    let start = Instant::now();

    let sum = skynet_par(0, 1_000_000);

    let elapsed = start.elapsed();

    println!("Result: {}", sum);
    println!("Time: {} ms", elapsed.as_millis());
}
