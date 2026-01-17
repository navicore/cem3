//! Time operations for Seq
//!
//! Provides timing primitives for performance measurement and delays.
//!
//! # Usage from Seq
//!
//! ```seq
//! time.now      # ( -- Int ) microseconds since epoch
//! time.nanos    # ( -- Int ) nanoseconds (monotonic, for timing)
//! 100 time.sleep-ms  # ( Int -- ) sleep for N milliseconds
//! ```
//!
//! # Example: Measuring execution time
//!
//! ```seq
//! : benchmark ( -- )
//!   time.nanos    # start time
//!   do-work
//!   time.nanos    # end time
//!   swap -        # elapsed nanos
//!   1000000 /     # convert to ms
//!   "Elapsed: " write
//!   int->string write
//!   "ms" write-line
//! ;
//! ```

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Get current time in microseconds since Unix epoch
///
/// Stack effect: ( -- Int )
///
/// Returns wall-clock time. Good for timestamps.
/// For measuring durations, prefer `time.nanos` which uses a monotonic clock.
///
/// # Safety
/// - `stack` must be a valid stack pointer (may be null for empty stack)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_time_now(stack: Stack) -> Stack {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as i64)
        .unwrap_or(0);

    unsafe { push(stack, Value::Int(micros)) }
}

/// Get monotonic nanoseconds for precise timing
///
/// Stack effect: ( -- Int )
///
/// Returns nanoseconds elapsed since the first call to this function.
/// Uses CLOCK_MONOTONIC for thread-independent consistent values.
/// Values start near zero for easier arithmetic.
///
/// # Safety
/// - `stack` must be a valid stack pointer (may be null for empty stack)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_time_nanos(stack: Stack) -> Stack {
    let nanos = elapsed_nanos();
    unsafe { push(stack, Value::Int(nanos)) }
}

/// Get elapsed nanoseconds since program start.
///
/// Thread-safe, consistent across all threads. Uses a lazily-initialized
/// base time to ensure values start near zero.
#[inline]
fn elapsed_nanos() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};

    // Base time is initialized on first call (value 0 means uninitialized)
    static BASE_NANOS: AtomicI64 = AtomicI64::new(0);

    let current = raw_monotonic_nanos();

    // Try to read existing base time
    let base = BASE_NANOS.load(Ordering::Relaxed);
    if base != 0 {
        return current.saturating_sub(base);
    }

    // First call: try to set the base time
    match BASE_NANOS.compare_exchange(0, current, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => 0,                                              // We set the base, elapsed is 0
        Err(actual_base) => current.saturating_sub(actual_base), // Another thread set it
    }
}

/// Get raw monotonic nanoseconds from the system clock.
///
/// On Unix: Uses `clock_gettime(CLOCK_MONOTONIC)` directly to get absolute
/// nanoseconds since boot. This is thread-independent - the same value is
/// returned regardless of which OS thread calls it.
///
/// On Windows: Falls back to `Instant::now()` with a process-wide base time.
#[inline]
#[cfg(unix)]
fn raw_monotonic_nanos() -> i64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: ts is a valid pointer to a timespec struct
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    // Convert to nanoseconds, saturating at i64::MAX
    // Explicit i64 casts for portability (tv_sec/tv_nsec types vary by platform)
    #[allow(clippy::unnecessary_cast)] // Required for 32-bit platforms
    let secs = (ts.tv_sec as i64).saturating_mul(1_000_000_000);
    #[allow(clippy::unnecessary_cast)]
    secs.saturating_add(ts.tv_nsec as i64)
}

/// Windows fallback using Instant with a process-wide base time.
/// Uses OnceLock for thread-safe one-time initialization.
#[inline]
#[cfg(not(unix))]
fn raw_monotonic_nanos() -> i64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static BASE: OnceLock<Instant> = OnceLock::new();
    let base = BASE.get_or_init(Instant::now);
    base.elapsed().as_nanos().try_into().unwrap_or(i64::MAX)
}

/// Sleep for a specified number of milliseconds
///
/// Stack effect: ( Int -- )
///
/// Yields the current strand to the scheduler while sleeping.
/// Uses `may::coroutine::sleep` for cooperative scheduling.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with an Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_time_sleep_ms(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "time.sleep-ms: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Int(ms) => {
            if ms < 0 {
                panic!("time.sleep-ms: duration must be non-negative, got {}", ms);
            }

            // Use may's coroutine-aware sleep for cooperative scheduling
            may::coroutine::sleep(Duration::from_millis(ms as u64));

            rest
        }
        _ => panic!(
            "time.sleep-ms: expected Int duration on stack, got {:?}",
            value
        ),
    }
}

// Public re-exports
pub use patch_seq_time_nanos as time_nanos;
pub use patch_seq_time_now as time_now;
pub use patch_seq_time_sleep_ms as time_sleep_ms;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::pop;
    use std::time::Instant;

    #[test]
    fn test_time_now_returns_positive() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = patch_seq_time_now(stack);
            let (_, value) = pop(stack);

            match value {
                Value::Int(micros) => {
                    // Should be a reasonable timestamp (after year 2020)
                    assert!(micros > 1_577_836_800_000_000); // 2020-01-01
                }
                _ => panic!("Expected Int"),
            }
        }
    }

    #[test]
    fn test_time_nanos_monotonic() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = patch_seq_time_nanos(stack);
            let (_, value1) = pop(stack);

            // Small delay
            std::thread::sleep(Duration::from_micros(100));

            let stack = crate::stack::alloc_test_stack();
            let stack = patch_seq_time_nanos(stack);
            let (_, value2) = pop(stack);

            match (value1, value2) {
                (Value::Int(t1), Value::Int(t2)) => {
                    assert!(t2 > t1, "time.nanos should be monotonically increasing");
                }
                _ => panic!("Expected Int values"),
            }
        }
    }

    #[test]
    fn test_time_nanos_cross_thread() {
        // Verify raw_monotonic_nanos is consistent across threads
        use std::sync::mpsc;
        use std::thread;

        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();

        // Get time on main thread
        let t1 = raw_monotonic_nanos();

        // Spawn thread, get time there
        let handle = thread::spawn(move || {
            let t2 = raw_monotonic_nanos();
            tx1.send(t2).unwrap();
            rx2.recv().unwrap() // wait for main to continue
        });

        let t2 = rx1.recv().unwrap();

        // Get time on main thread again
        let t3 = raw_monotonic_nanos();
        tx2.send(()).unwrap();
        handle.join().unwrap();

        // All times should be monotonically increasing
        assert!(t2 > t1, "t2 ({}) should be > t1 ({})", t2, t1);
        assert!(t3 > t2, "t3 ({}) should be > t2 ({})", t3, t2);
    }

    #[test]
    fn test_time_sleep_ms() {
        unsafe {
            // Push 1ms sleep duration
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(1));

            let start = Instant::now();
            let _stack = patch_seq_time_sleep_ms(stack);
            let elapsed = start.elapsed();

            // Should sleep at least 1ms
            assert!(elapsed >= Duration::from_millis(1));
            // Stack should be empty after popping the duration
        }
    }
}
