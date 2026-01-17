//! Profile May coroutine spawn and channel overhead
//!
//! Run with: cargo run --release

use may::coroutine;
use may::sync::mpmc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

fn main() {
    println!("=== May Coroutine & Channel Profile ===\n");

    // Configure May like seq-runtime does
    may::config().set_stack_size(0x100000); // 1MB stack
    may::config().set_workers(4);

    let n = 100_000;

    // Test 1: May coroutine spawn
    let counter = Arc::new(AtomicUsize::new(0));
    let start = Instant::now();
    for _ in 0..n {
        let c = counter.clone();
        unsafe {
            coroutine::spawn(move || {
                c.fetch_add(1, Ordering::Relaxed);
            });
        }
    }
    let spawn_time = start.elapsed();
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("May spawn ({} coroutines): {:?}", n, spawn_time);
    println!("Per spawn: {:?}", spawn_time / n as u32);

    // Test 2: Channel creation
    let start = Instant::now();
    let channels: Vec<_> = (0..n).map(|_| mpmc::channel::<i64>()).collect();
    let chan_create_time = start.elapsed();
    println!("\nChannel creation ({}): {:?}", n, chan_create_time);
    println!("Per channel: {:?}", chan_create_time / n as u32);
    drop(channels);

    // Test 3: Channel send/receive (in same thread, no blocking)
    let (tx, rx) = mpmc::channel::<i64>();
    let start = Instant::now();
    for i in 0..n {
        tx.send(i as i64).unwrap();
    }
    let send_time = start.elapsed();
    println!("\nChannel send ({}): {:?}", n, send_time);
    println!("Per send: {:?}", send_time / n as u32);

    let start = Instant::now();
    for _ in 0..n {
        rx.recv().unwrap();
    }
    let recv_time = start.elapsed();
    println!("Channel receive ({}): {:?}", n, recv_time);
    println!("Per receive: {:?}", recv_time / n as u32);

    // Test 5: Mutex + HashMap lookup (simulates Seq's channel registry)
    let registry: Mutex<HashMap<u64, (mpmc::Sender<i64>, mpmc::Receiver<i64>)>> =
        Mutex::new(HashMap::new());

    // Pre-populate registry
    for i in 0..1000 {
        let (tx, rx) = mpmc::channel::<i64>();
        registry.lock().unwrap().insert(i, (tx, rx));
    }

    // Measure lock + lookup + clone pattern (what Seq does per send/receive)
    let n_ops = 1_000_000;
    let start = Instant::now();
    for i in 0..n_ops {
        let key = (i % 1000) as u64;
        let guard = registry.lock().unwrap();
        let (tx, _rx) = guard.get(&key).unwrap();
        let _tx_clone = tx.clone();
        drop(guard);
    }
    let registry_time = start.elapsed();
    println!("\n=== Seq Channel Registry Overhead ===");
    println!("Mutex lock + HashMap lookup + clone ({} ops): {:?}", n_ops, registry_time);
    println!("Per operation: {:?}", registry_time / n_ops as u32);

    // With 2M ops (1M sends + 1M receives), estimate:
    let registry_2m = registry_time * 2;
    println!("Estimated for 2M ops (skynet): {:?}", registry_2m);

    // Test 4: Skynet-like pattern estimates
    let num_channels = 111_111;

    let start = Instant::now();
    let _channels: Vec<_> = (0..num_channels).map(|_| mpmc::channel::<i64>()).collect();
    let skynet_chan_time = start.elapsed();
    println!("\n=== Skynet-Scale Estimates ===");
    println!("Channel creation ({} channels): {:?}", num_channels, skynet_chan_time);

    // Estimate total
    let spawn_1m = spawn_time * 10;
    let send_1m = send_time * 10;
    let recv_1m = recv_time * 10;

    println!("\nEstimated costs for 1M nodes:");
    println!("  May spawn (1M):         {:?}", spawn_1m);
    println!("  Channel creates (111K): {:?}", skynet_chan_time);
    println!("  Channel sends (1M):     {:?}", send_1m);
    println!("  Channel recvs (1M):     {:?}", recv_1m);
    println!("  Registry overhead (2M): {:?}", registry_2m);

    let total = spawn_1m + skynet_chan_time + send_1m + recv_1m + registry_2m;
    println!("\nTotal estimated: {:?}", total);
    println!("Skynet actual:   4779ms");
    println!("Accounted: {:.1}% of skynet", total.as_secs_f64() / 4.779 * 100.0);
    println!("\nRemaining ~{:.0}ms = other Seq overhead",
             4779.0 - total.as_secs_f64() * 1000.0);
}
