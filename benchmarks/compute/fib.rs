// Fibonacci benchmark - naive recursive implementation
// Compile: rustc -O -o fib_rust fib.rs

fn fib(n: i64) -> i64 {
    if n < 2 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

fn main() {
    let result = fib(40);
    println!("{}", result);

    // Expected: 102334155
    std::process::exit(if result == 102334155 { 0 } else { 1 });
}
