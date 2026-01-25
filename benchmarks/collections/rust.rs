// Collections Benchmark - Rust implementation
// Output format: BENCH:collections:<test>:<result>:<time_ms>

use std::time::Instant;

const NUM_ELEMENTS: i64 = 100_000;

fn main() {
    // Build
    let start = Instant::now();
    let data: Vec<i64> = (0..NUM_ELEMENTS).collect();
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:collections:build-100k:{}:{}", data.len(), elapsed);

    // Map (double each)
    let start = Instant::now();
    let mapped: Vec<i64> = data.iter().map(|x| x * 2).collect();
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:collections:map-double:{}:{}", mapped.len(), elapsed);

    // Filter (keep evens)
    let start = Instant::now();
    let filtered: Vec<i64> = data.iter().filter(|x| *x % 2 == 0).copied().collect();
    let elapsed = start.elapsed().as_millis();
    println!(
        "BENCH:collections:filter-evens:{}:{}",
        filtered.len(),
        elapsed
    );

    // Fold (sum)
    let start = Instant::now();
    let total: i64 = data.iter().sum();
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:collections:fold-sum:{}:{}", total, elapsed);

    // Chain (map -> filter -> fold)
    let start = Instant::now();
    let result: i64 = data
        .iter()
        .map(|x| x * 3)
        .filter(|x| x % 2 == 0)
        .sum();
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:collections:chain:{}:{}", result, elapsed);
}
