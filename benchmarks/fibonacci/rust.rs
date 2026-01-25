// Fibonacci Benchmark - Rust implementation
// Output format: BENCH:fibonacci:<test>:<result>:<time_ms>

use std::time::Instant;

fn fib_naive(n: i64) -> i64 {
    if n < 2 {
        n
    } else {
        fib_naive(n - 1) + fib_naive(n - 2)
    }
}

fn fib_fast(n: i64) -> i64 {
    if n < 2 {
        return n;
    }
    let mut a: i64 = 0;
    let mut b: i64 = 1;
    for _ in 1..n {
        let tmp = a + b;
        a = b;
        b = tmp;
    }
    b
}

fn bench<F>(name: &str, n: i64, expected: i64, f: F)
where
    F: Fn(i64) -> i64,
{
    let start = Instant::now();
    let result = f(n);
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:fibonacci:{}:{}:{}", name, result, elapsed);
    if result != expected {
        println!("ERROR: expected {}, got {}", expected, result);
    }
}

fn bench_repeated<F>(name: &str, n: i64, iterations: i32, expected: i64, f: F)
where
    F: Fn(i64) -> i64,
{
    let start = Instant::now();
    let mut result = 0;
    for _ in 0..iterations {
        result = f(n);
    }
    let elapsed = start.elapsed().as_millis();
    println!("BENCH:fibonacci:{}:{}:{}", name, result, elapsed);
    if result != expected {
        println!("ERROR: expected {}, got {}", expected, result);
    }
}

fn main() {
    // Naive recursive tests
    bench("fib-naive-30", 30, 832040, fib_naive);
    bench("fib-naive-35", 35, 9227465, fib_naive);

    // Iterative tests
    bench("fib-fast-30", 30, 832040, fib_fast);
    bench("fib-fast-50", 50, 12586269025, fib_fast);
    bench("fib-fast-70", 70, 190392490709135, fib_fast);

    // Repeated runs
    bench_repeated("fib-naive-20-x1000", 20, 1000, 6765, fib_naive);
    bench_repeated("fib-fast-20-x1000", 20, 1000, 6765, fib_fast);
}
