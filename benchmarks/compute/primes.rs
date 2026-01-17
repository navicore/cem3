// Prime counting benchmark
// Compile: rustc -O -o primes_rust primes.rs

fn is_prime(n: i64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 {
        return true;
    }
    let mut divisor = 2;
    while divisor * divisor <= n {
        if n % divisor == 0 {
            return false;
        }
        divisor += 1;
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
    let result = count_primes(100_000);
    println!("{}", result);

    // Expected: 9592
    std::process::exit(if result == 9592 { 0 } else { 1 });
}
