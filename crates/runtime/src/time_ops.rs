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
/// Returns nanoseconds from system boot (CLOCK_MONOTONIC).
/// Uses raw clock_gettime for consistent values across all threads -
/// critical for timing when coroutines migrate between OS threads.
///
/// Note: Saturates at i64::MAX (~292 years of uptime) to prevent overflow.
///
/// # Safety
/// - `stack` must be a valid stack pointer (may be null for empty stack)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_time_nanos(stack: Stack) -> Stack {
    let nanos = monotonic_nanos();
    unsafe { push(stack, Value::Int(nanos)) }
}

/// Get raw monotonic nanoseconds from the system clock.
///
/// On Unix: Uses `clock_gettime(CLOCK_MONOTONIC)` directly to get absolute
/// nanoseconds since boot. This is thread-independent - the same value is
/// returned regardless of which OS thread calls it.
///
/// On Windows: Falls back to `Instant::now()` with a process-wide base time.
/// This has a one-time initialization cost but is still thread-safe.
#[inline]
#[cfg(unix)]
fn monotonic_nanos() -> i64 {
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
fn monotonic_nanos() -> i64 {
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
    #[cfg(not(feature = "nanbox"))]
    use crate::stack::pop;
    use std::time::Instant;

    // Unix timestamps in microseconds exceed the 44-bit NaN-boxing range
    #[test]
    #[cfg(not(feature = "nanbox"))]
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

    // Monotonic nanosecond counters can exceed the 44-bit NaN-boxing range
    #[test]
    #[cfg(not(feature = "nanbox"))]
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

    // Monotonic nanosecond counters can exceed the 44-bit NaN-boxing range
    #[test]
    #[cfg(not(feature = "nanbox"))]
    fn test_time_nanos_cross_thread() {
        // Verify monotonic_nanos is consistent across threads
        use std::sync::mpsc;
        use std::thread;

        let (tx1, rx1) = mpsc::channel();
        let (tx2, rx2) = mpsc::channel();

        // Get time on main thread
        let t1 = monotonic_nanos();

        // Spawn thread, get time there
        let handle = thread::spawn(move || {
            let t2 = monotonic_nanos();
            tx1.send(t2).unwrap();
            rx2.recv().unwrap() // wait for main to continue
        });

        let t2 = rx1.recv().unwrap();

        // Get time on main thread again
        let t3 = monotonic_nanos();
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
