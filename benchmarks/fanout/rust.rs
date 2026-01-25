// Fanout Benchmark - Rust implementation
// Output format: BENCH:fanout:<test>:<result>:<time_ms>
//
// 1 producer, N consumer workers using threads and channels.

use std::sync::mpsc;
use std::thread;
use std::time::Instant;

const NUM_MESSAGES: i32 = 100_000;
const NUM_WORKERS: usize = 10;

fn worker(work_rx: mpsc::Receiver<i32>, done_tx: mpsc::Sender<i32>) {
    let mut count = 0;
    loop {
        match work_rx.recv() {
            Ok(val) if val < 0 => {
                done_tx.send(count).unwrap();
                return;
            }
            Ok(_) => {
                count += 1;
                thread::yield_now();
            }
            Err(_) => return,
        }
    }
}

fn main() {
    let (work_tx, work_rx) = mpsc::channel::<i32>();
    let (done_tx, done_rx) = mpsc::channel();

    // Spawn workers - need to share receiver
    // Use crossbeam for MPMC, or spawn with individual channels
    // For simplicity, use a single-consumer approach with work stealing
    let mut handles = Vec::with_capacity(NUM_WORKERS);

    // Create individual channels for each worker
    let mut worker_txs: Vec<mpsc::Sender<i32>> = Vec::with_capacity(NUM_WORKERS);
    for _ in 0..NUM_WORKERS {
        let (wtx, wrx) = mpsc::channel();
        let dtx = done_tx.clone();
        worker_txs.push(wtx);
        handles.push(thread::spawn(move || {
            worker(wrx, dtx);
        }));
    }
    drop(done_tx);

    let start = Instant::now();

    // Produce messages (round-robin to workers)
    for i in 0..NUM_MESSAGES {
        worker_txs[i as usize % NUM_WORKERS].send(i).unwrap();
    }

    // Send sentinels
    for tx in &worker_txs {
        tx.send(-1).unwrap();
    }

    // Collect results
    let mut total = 0;
    for _ in 0..NUM_WORKERS {
        total += done_rx.recv().unwrap();
    }

    // Wait for workers
    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed().as_millis();

    println!("BENCH:fanout:throughput-100k:{}:{}", total, elapsed);
}
