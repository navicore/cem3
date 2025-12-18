//! Profile May coroutine spawning
//! Run from project root: cargo build --release -p seq-runtime &&
//!   rustc -O -L target/release/deps benchmarks/skynet/may_profile.rs -o may_profile

use std::time::Instant;
use std::sync::atomic::{AtomicUsize, Ordering};

fn main() {
    // Test: spawn 100K coroutines that just increment a counter
    let n = 100_000;
    let counter = std::sync::Arc::new(AtomicUsize::new(0));

    may::config().set_workers(4);

    let start = Instant::now();

    // Use may's go! macro for spawning
    let handles: Vec<_> = (0..n).map(|_| {
        let c = counter.clone();
        may::go!(move || {
            c.fetch_add(1, Ordering::Relaxed);
        })
    }).collect();

    // Wait for all to complete
    for h in handles {
        h.join().ok();
    }

    let elapsed = start.elapsed();
    let count = counter.load(Ordering::Relaxed);

    println!("May coroutine spawn ({} coroutines): {:?}", n, elapsed);
    println!("Counter: {} (should be {})", count, n);
    println!("Per spawn: {:?}", elapsed / n as u32);
}
