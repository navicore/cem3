// Leibniz formula for π benchmark
// π/4 = 1 - 1/3 + 1/5 - 1/7 + 1/9 - ...
// Build: rustc -O -o leibniz_pi_rust leibniz_pi.rs

const ITERATIONS: i64 = 100_000_000;

fn leibniz_pi(n: i64) -> f64 {
    let mut sum: f64 = 0.0;
    let mut sign: f64 = 1.0;
    for k in 0..n {
        sum += sign / (2.0 * k as f64 + 1.0);
        sign = -sign;
    }
    sum * 4.0
}

fn main() {
    let pi = leibniz_pi(ITERATIONS);
    println!("{:.15}", pi);

    // Verify result is close to π (converges slowly)
    // With 100M iterations, we get about 8 decimal places
    let expected = std::f64::consts::PI;
    let error = (pi - expected).abs();

    // Should be accurate to ~1e-8 with 100M iterations
    if error < 1e-7 {
        std::process::exit(0);
    } else {
        eprintln!("Error too large: {}", error);
        std::process::exit(1);
    }
}
