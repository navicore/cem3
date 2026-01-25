// Primes Benchmark - Rust implementation
// Output format: BENCH:primes:<test>:<result>:<time_ms>

use std::time::Instant;

fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let mut i = 3;
    while i * i <= n {
        if n % i == 0 {
            return false;
        }
        i += 2;
    }
    true
}

fn count_primes(limit: i64) -> i64 {
    let mut count = 0;
    for n in 2..=limit {
        if is_prime(n) {
            count += 1;
        }
    }
    count
}

fn main() {
    // count-primes-10k
    let start = Instant::now();
    let result = count_primes(10000);
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:primes:count-10k:{}:{}", result, elapsed);

    // count-primes-100k
    let start = Instant::now();
    let result = count_primes(100000);
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:primes:count-100k:{}:{}", result, elapsed);
}
