// Fan-Out Benchmark - Rust version
//
// 1 producer sends N messages to a shared channel.
// M workers compete to receive and process messages.
// Tests: channel contention, work distribution, scheduler fairness.
// Build: rustc -O -o fanout_rust fanout.rs

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const NUM_MESSAGES: i64 = 1_000_000;
const NUM_WORKERS: usize = 100;

fn main() {
    let (work_tx, work_rx) = mpsc::channel::<Option<i32>>();
    let (done_tx, done_rx) = mpsc::channel();

    // Wrap receiver in Arc<Mutex> for sharing among workers
    let work_rx = std::sync::Arc::new(std::sync::Mutex::new(work_rx));

    let start = Instant::now();

    // Spawn workers
    let mut handles = Vec::with_capacity(NUM_WORKERS);
    for _ in 0..NUM_WORKERS {
        let rx = work_rx.clone();
        let done = done_tx.clone();
        handles.push(thread::spawn(move || {
            let mut count = 0;
            loop {
                let msg = {
                    let guard = rx.lock().unwrap();
                    guard.recv().ok()
                };
                match msg {
                    Some(Some(_)) => count += 1,
                    Some(None) | None => break,
                }
            }
            done.send(count).unwrap();
        }));
    }
    drop(done_tx); // Close our copy

    // Producer: send messages
    for i in 0..NUM_MESSAGES {
        work_tx.send(Some(i as i32)).unwrap();
    }
    // Send sentinel to each worker
    for _ in 0..NUM_WORKERS {
        work_tx.send(None).unwrap();
    }

    // Collect results
    let mut total = 0i64;
    for _ in 0..NUM_WORKERS {
        total += done_rx.recv().unwrap();
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();
    let elapsed_ms = elapsed.as_millis() as i64;

    println!("Processed: {} messages", total);
    println!("Time: {} ms", elapsed_ms);
    println!("Workers: {}", NUM_WORKERS);

    // Calculate throughput
    let msgs_per_sec = if elapsed_ms > 0 {
        NUM_MESSAGES * 1000 / elapsed_ms
    } else {
        0
    };
    println!("Throughput: {} msg/sec", msgs_per_sec);
}
