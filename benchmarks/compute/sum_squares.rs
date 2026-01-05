// Sum of squares benchmark
// Compile: rustc -O -o sum_squares_rust sum_squares.rs
//
// Note: n=1M is safe for i64. Limits above ~3M risk overflow.

fn sum_squares(n: i64) -> i64 {
    let mut acc: i64 = 0;
    for i in 1..=n {
        acc += i * i;
    }
    acc
}

fn main() {
    let result = sum_squares(1_000_000);
    println!("{}", result);

    // Expected: 333333833333500000
    std::process::exit(if result == 333333833333500000 { 0 } else { 1 });
}
