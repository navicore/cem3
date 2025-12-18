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
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// Thread-local monotonic clock base for consistent nanosecond timing
thread_local! {
    static CLOCK_BASE: Instant = Instant::now();
}

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
/// Returns nanoseconds from an arbitrary starting point (process start).
/// Uses a monotonic clock - values always increase, unaffected by system
/// clock changes. Perfect for measuring elapsed time.
///
/// Note: Saturates at i64::MAX (~292 years of uptime) to prevent overflow.
///
/// # Safety
/// - `stack` must be a valid stack pointer (may be null for empty stack)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_time_nanos(stack: Stack) -> Stack {
    let nanos = CLOCK_BASE.with(|base| base.elapsed().as_nanos().try_into().unwrap_or(i64::MAX));
    unsafe { push(stack, Value::Int(nanos)) }
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
    use std::ptr;

    #[test]
    fn test_time_now_returns_positive() {
        unsafe {
            let stack = ptr::null_mut();
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
            let stack = ptr::null_mut();
            let stack = patch_seq_time_nanos(stack);
            let (_, value1) = pop(stack);

            // Small delay
            std::thread::sleep(Duration::from_micros(100));

            let stack = ptr::null_mut();
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
    fn test_time_sleep_ms() {
        unsafe {
            // Push 1ms sleep duration
            let stack = ptr::null_mut();
            let stack = push(stack, Value::Int(1));

            let start = Instant::now();
            let stack = patch_seq_time_sleep_ms(stack);
            let elapsed = start.elapsed();

            // Should sleep at least 1ms
            assert!(elapsed >= Duration::from_millis(1));
            // Stack should be empty after popping the duration
            assert!(stack.is_null());
        }
    }
}
