//! Native Rust skynet using May - baseline comparison
//!
//! Run with: cargo run --release --bin skynet_native

use may::coroutine;
use may::sync::mpmc;
use std::time::Instant;

fn skynet(result_chan: mpmc::Sender<i64>, num: i64, size: i64) {
    if size == 1 {
        // Leaf: send num to result channel
        result_chan.send(num).unwrap();
    } else {
        // Branch: spawn 10 children, collect results
        let (tx, rx) = mpmc::channel::<i64>();
        let child_size = size / 10;

        // Spawn 10 children
        for i in 0..10 {
            let child_tx = tx.clone();
            let child_num = num * 10 + i;
            unsafe {
                coroutine::spawn(move || {
                    skynet(child_tx, child_num, child_size);
                });
            }
        }

        // Receive and sum 10 results
        let mut sum = 0i64;
        for _ in 0..10 {
            sum += rx.recv().unwrap();
        }

        // Send sum to parent
        result_chan.send(sum).unwrap();
    }
}

fn main() {
    println!("=== Native Rust Skynet (May) ===\n");

    // Configure May like seq-runtime does
    may::config().set_stack_size(0x100000); // 1MB stack
    may::config().set_workers(4);

    let size = 100_000; // Same as Seq skynet

    let start = Instant::now();

    // Create result channel
    let (tx, rx) = mpmc::channel::<i64>();

    // Spawn root
    unsafe {
        coroutine::spawn(move || {
            skynet(tx, 0, size);
        });
    }

    // Wait for result
    let result = rx.recv().unwrap();
    let elapsed = start.elapsed();

    println!("Result: {}", result);
    println!("Time: {:?}", elapsed);
    println!("\nExpected result: {}", (0i64..size).sum::<i64>());

    // Compare to Seq
    let seq_time = 4779.0; // ms
    println!("\nSeq skynet: {}ms", seq_time);
    println!("Native Rust: {:.1}ms", elapsed.as_secs_f64() * 1000.0);
    println!("Seq overhead: {:.1}x", seq_time / (elapsed.as_secs_f64() * 1000.0));
}
