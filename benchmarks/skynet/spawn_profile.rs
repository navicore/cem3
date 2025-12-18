//! Quick profiling of spawn overhead components
//! Run with: rustc -O spawn_profile.rs && ./spawn_profile

use std::time::Instant;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

// Simulate the registry scan
const REGISTRY_SIZE: usize = 1024;

struct StrandSlot {
    strand_id: AtomicU64,
}

impl StrandSlot {
    const fn new() -> Self {
        Self { strand_id: AtomicU64::new(0) }
    }
}

fn main() {
    let n = 100_000;

    // Test 1: Just atomic increments (baseline)
    let counter = AtomicU64::new(0);
    let start = Instant::now();
    for _ in 0..n {
        counter.fetch_add(1, Ordering::Relaxed);
    }
    let atomic_time = start.elapsed();
    println!("Atomic increments ({}): {:?}", n, atomic_time);

    // Test 2: Registry scan (finding free slot in full registry)
    let slots: Vec<StrandSlot> = (0..REGISTRY_SIZE).map(|i| {
        let s = StrandSlot::new();
        s.strand_id.store(i as u64 + 1, Ordering::Relaxed); // Mark all as used
        s
    }).collect();

    let start = Instant::now();
    let mut found = 0;
    for _ in 0..n {
        for slot in &slots {
            if slot.strand_id.compare_exchange(
                0, 1, Ordering::AcqRel, Ordering::Relaxed
            ).is_ok() {
                found += 1;
                break;
            }
        }
    }
    let registry_time = start.elapsed();
    println!("Registry scans ({} x {} slots): {:?} (found: {})", n, REGISTRY_SIZE, registry_time, found);

    // Test 3: SystemTime call (used in registry)
    let start = Instant::now();
    for _ in 0..n {
        let _ = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs());
    }
    let time_time = start.elapsed();
    println!("SystemTime::now() ({}): {:?}", n, time_time);

    // Test 4: CAS loop for peak update (simulating contention)
    let peak = AtomicUsize::new(0);
    let start = Instant::now();
    for i in 0..n {
        let new_count = i;
        let mut current = peak.load(Ordering::Acquire);
        while new_count > current {
            match peak.compare_exchange_weak(
                current, new_count, Ordering::Release, Ordering::Relaxed
            ) {
                Ok(_) => break,
                Err(c) => current = c,
            }
        }
    }
    let peak_time = start.elapsed();
    println!("Peak CAS loop ({}): {:?}", n, peak_time);

    // Test 5: Thread spawn (for comparison - this is what May replaces)
    let start = Instant::now();
    let handles: Vec<_> = (0..1000).map(|_| {
        std::thread::spawn(|| {})
    }).collect();
    for h in handles {
        h.join().unwrap();
    }
    let thread_time = start.elapsed();
    println!("std::thread::spawn (1000): {:?}", thread_time);

    println!("\n=== Estimates for 100K spawns ===");
    println!("Registry scans alone: {:?}", registry_time);
    println!("This is {:.1}% of typical skynet time (~5s)",
        registry_time.as_secs_f64() / 5.0 * 100.0);
}
