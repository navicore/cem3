// Skynet Benchmark - Rust implementation
// Output format: BENCH:skynet:<test>:<result>:<time_ms>
//
// Uses hybrid approach: threads at high levels, sequential at leaves.
// 100,000 virtual "strands" (but not 100k OS threads).
// Expected result: sum of 0..99999 = 4999950000

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const PARALLEL_THRESHOLD: i64 = 1000;

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

    let sum = skynet_par(0, 100_000);

    let elapsed = start.elapsed().as_millis();

    println!("BENCH:skynet:spawn-100k:{}:{}", sum, elapsed);
}
