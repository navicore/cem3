//! Signal handling API for Seq
//!
//! Provides Unix signal handling with a safe, flag-based approach:
//! - Signals are trapped and set atomic flags (no code runs in signal context)
//! - User code polls for signals at safe points
//! - Fits Seq's explicit, predictable style
//!
//! # Example
//! ```seq
//! signal.SIGINT signal.trap
//! signal.SIGTERM signal.trap
//!
//! : main-loop ( -- )
//!   signal.SIGINT signal.received? if
//!     "Shutting down..." io.write-line
//!     return
//!   then
//!   do-work
//!   main-loop
//! ;
//! ```
//!
//! # Safety
//!
//! Signal handlers execute in an interrupt context with severe restrictions.
//! This module uses only async-signal-safe operations (atomic flag setting).
//! All Seq code execution happens outside the signal handler, when the user
//! explicitly checks for received signals.
//!
//! # Thread Safety and Concurrent Access
//!
//! This module is designed to be safe for concurrent use from multiple strands:
//!
//! - **Handler installation** (`signal.trap`, `signal.default`, `signal.ignore`):
//!   Protected by a mutex to ensure only one strand modifies handlers at a time.
//!   Concurrent calls will serialize safely.
//!
//! - **Flag operations** (`signal.received?`, `signal.pending?`, `signal.clear`):
//!   Use lock-free atomic operations with appropriate memory ordering:
//!   - `signal.received?`: Atomic swap with Acquire ordering (read-modify-write)
//!   - `signal.pending?`: Atomic load with Acquire ordering (read-only)
//!   - `signal.clear`: Atomic store with Release ordering (write-only)
//!
//!   Multiple strands can safely check the same signal. However, `signal.received?`
//!   clears the flag atomically, so if two strands both call it, only one will
//!   observe `true`. Use `signal.pending?` if you need non-destructive reads.
//!
//! - **Signal handler**: Executes outside the strand context (in OS interrupt
//!   context) and only performs a single atomic store. This is async-signal-safe.
//!
//! This module uses `sigaction()` instead of the deprecated `signal()` function
//! for well-defined behavior in multithreaded environments.
//!
//! # Platform Support
//!
//! - Unix: Full signal support using sigaction()
//! - Windows: Stub implementations (signals not supported, all operations no-op)

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::sync::atomic::{AtomicBool, Ordering};

/// Maximum signal number we support (covers all standard Unix signals)
const MAX_SIGNAL: usize = 32;

/// Atomic flags for each signal - set by signal handler, cleared by user code
static SIGNAL_FLAGS: [AtomicBool; MAX_SIGNAL] = [
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
    AtomicBool::new(false),
];

/// Mutex to protect signal handler installation from concurrent access
#[cfg(unix)]
static SIGNAL_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Signal handler that just sets the atomic flag
/// This is async-signal-safe: only uses atomic operations
#[cfg(unix)]
extern "C" fn flag_signal_handler(sig: libc::c_int) {
    let sig_num = sig as usize;
    if sig_num < MAX_SIGNAL {
        SIGNAL_FLAGS[sig_num].store(true, Ordering::Release);
    }
}

// ============================================================================
// Signal Constants - Platform-correct values from libc
// ============================================================================

/// Get SIGINT constant (Ctrl+C interrupt)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigint(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGINT as i64)) }
}

/// Get SIGTERM constant (termination request)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigterm(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGTERM as i64)) }
}

/// Get SIGHUP constant (hangup)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sighup(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGHUP as i64)) }
}

/// Get SIGPIPE constant (broken pipe)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigpipe(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGPIPE as i64)) }
}

/// Get SIGUSR1 constant (user signal 1)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigusr1(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGUSR1 as i64)) }
}

/// Get SIGUSR2 constant (user signal 2)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigusr2(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGUSR2 as i64)) }
}

/// Get SIGCHLD constant (child status change)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigchld(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGCHLD as i64)) }
}

/// Get SIGALRM constant (alarm clock)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigalrm(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGALRM as i64)) }
}

/// Get SIGCONT constant (continue)
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigcont(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(libc::SIGCONT as i64)) }
}

// Non-Unix stubs for signal constants (return 0)
// Safety: Stack pointer must be valid for all functions below.

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigint(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigterm(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sighup(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigpipe(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigusr1(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigusr2(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigchld(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigalrm(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_sigcont(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Int(0)) }
}

/// Install a signal handler using sigaction (thread-safe)
///
/// Uses sigaction() instead of signal() for:
/// - Well-defined semantics across platforms
/// - Thread safety with strands
/// - SA_RESTART to automatically restart interrupted syscalls
#[cfg(unix)]
fn install_signal_handler(sig_num: libc::c_int) -> Result<(), std::io::Error> {
    use std::mem::MaybeUninit;

    let _guard = SIGNAL_MUTEX
        .lock()
        .expect("signal: mutex poisoned during handler installation");

    unsafe {
        let mut action: libc::sigaction = MaybeUninit::zeroed().assume_init();
        // Use sa_handler (not sa_sigaction) since we're not using SA_SIGINFO
        action.sa_sigaction = flag_signal_handler as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut action.sa_mask);

        let result = libc::sigaction(sig_num, &action, std::ptr::null_mut());
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Restore default signal handler using sigaction (thread-safe)
#[cfg(unix)]
fn restore_default_handler(sig_num: libc::c_int) -> Result<(), std::io::Error> {
    use std::mem::MaybeUninit;

    let _guard = SIGNAL_MUTEX
        .lock()
        .expect("signal: mutex poisoned during handler restoration");

    unsafe {
        let mut action: libc::sigaction = MaybeUninit::zeroed().assume_init();
        // Use SIG_DFL to restore default handler
        action.sa_sigaction = libc::SIG_DFL as libc::sighandler_t;
        action.sa_flags = 0;
        libc::sigemptyset(&mut action.sa_mask);

        let result = libc::sigaction(sig_num, &action, std::ptr::null_mut());
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Ignore a signal using sigaction (thread-safe)
#[cfg(unix)]
fn ignore_signal(sig_num: libc::c_int) -> Result<(), std::io::Error> {
    use std::mem::MaybeUninit;

    let _guard = SIGNAL_MUTEX
        .lock()
        .expect("signal: mutex poisoned during ignore");

    unsafe {
        let mut action: libc::sigaction = MaybeUninit::zeroed().assume_init();
        // Use SIG_IGN to ignore the signal
        action.sa_sigaction = libc::SIG_IGN as libc::sighandler_t;
        action.sa_flags = 0;
        libc::sigemptyset(&mut action.sa_mask);

        let result = libc::sigaction(sig_num, &action, std::ptr::null_mut());
        if result != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Trap a signal: install handler that sets flag instead of default behavior
///
/// Stack effect: ( signal-num -- )
///
/// After trapping, the signal will set an internal flag instead of its default
/// action (which might be to terminate the process). Use `signal.received?` to
/// check and clear the flag.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_trap(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.trap: invalid signal number {}", n);
                }
                n as libc::c_int
            }
            _ => panic!(
                "signal.trap: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        // Install our flag-setting handler using sigaction
        if let Err(e) = install_signal_handler(sig_num) {
            panic!(
                "signal.trap: failed to install handler for signal {}: {}",
                sig_num, e
            );
        }
        stack
    }
}

/// Check if a signal was received and clear the flag
///
/// Stack effect: ( signal-num -- received? )
///
/// Returns true if the signal was received since the last check, false otherwise.
/// This atomically clears the flag, so the signal must be received again to return true.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_received(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.received?: invalid signal number {}", n);
                }
                n as usize
            }
            _ => panic!(
                "signal.received?: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        // Atomically swap the flag to false and return the old value
        let was_set = SIGNAL_FLAGS[sig_num].swap(false, Ordering::Acquire);
        push(stack, Value::Bool(was_set))
    }
}

/// Check if a signal is pending without clearing the flag
///
/// Stack effect: ( signal-num -- pending? )
///
/// Returns true if the signal was received, false otherwise.
/// Unlike `signal.received?`, this does NOT clear the flag.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_pending(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.pending?: invalid signal number {}", n);
                }
                n as usize
            }
            _ => panic!(
                "signal.pending?: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        let is_set = SIGNAL_FLAGS[sig_num].load(Ordering::Acquire);
        push(stack, Value::Bool(is_set))
    }
}

/// Restore the default handler for a signal
///
/// Stack effect: ( signal-num -- )
///
/// Restores the system default behavior for the signal.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_default(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.default: invalid signal number {}", n);
                }
                n as libc::c_int
            }
            _ => panic!(
                "signal.default: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        if let Err(e) = restore_default_handler(sig_num) {
            panic!(
                "signal.default: failed to restore default handler for signal {}: {}",
                sig_num, e
            );
        }
        stack
    }
}

/// Ignore a signal entirely
///
/// Stack effect: ( signal-num -- )
///
/// The signal will be ignored - it won't terminate the process or set any flag.
/// Useful for SIGPIPE in network servers.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_ignore(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.ignore: invalid signal number {}", n);
                }
                n as libc::c_int
            }
            _ => panic!(
                "signal.ignore: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        if let Err(e) = ignore_signal(sig_num) {
            panic!("signal.ignore: failed to ignore signal {}: {}", sig_num, e);
        }
        stack
    }
}

/// Clear the flag for a signal without checking it
///
/// Stack effect: ( signal-num -- )
///
/// Useful for resetting state without caring about the previous value.
///
/// # Safety
/// Stack must have an Int (signal number) on top
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_clear(stack: Stack) -> Stack {
    unsafe {
        let (stack, sig_val) = pop(stack);
        let sig_num = match sig_val {
            Value::Int(n) => {
                if n < 0 || n as usize >= MAX_SIGNAL {
                    panic!("signal.clear: invalid signal number {}", n);
                }
                n as usize
            }
            _ => panic!(
                "signal.clear: expected Int (signal number), got {:?}",
                sig_val
            ),
        };

        SIGNAL_FLAGS[sig_num].store(false, Ordering::Release);
        stack
    }
}

// Stub implementations for non-Unix platforms
// Safety: Stack pointer must be valid for all functions below.

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_trap(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    // No-op on non-Unix - signals not supported
    stack
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_default(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    stack
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_ignore(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    stack
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_received(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    // Always return false on non-Unix - signals not supported
    unsafe { push(stack, Value::Bool(false)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_pending(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    // Always return false on non-Unix - signals not supported
    unsafe { push(stack, Value::Bool(false)) }
}

/// # Safety
/// Stack pointer must be valid.
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_clear(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    // No-op on non-Unix
    stack
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_signal_flag_operations() {
        // Use index 3 (SIGQUIT) to avoid conflicts with actual signal tests
        // Tests use: SIGUSR1 (10 Linux, 30 macOS), SIGUSR2 (12 Linux, 31 macOS)
        // Index 3 is safe on both platforms
        const TEST_IDX: usize = 3;

        // Clear flag first (other tests might have set it)
        SIGNAL_FLAGS[TEST_IDX].store(false, Ordering::Release);

        // Now test that flag is false
        assert!(!SIGNAL_FLAGS[TEST_IDX].load(Ordering::Acquire));

        // Set flag manually (simulating signal receipt)
        SIGNAL_FLAGS[TEST_IDX].store(true, Ordering::Release);
        assert!(SIGNAL_FLAGS[TEST_IDX].load(Ordering::Acquire));

        // Swap should return old value and set new
        let was_set = SIGNAL_FLAGS[TEST_IDX].swap(false, Ordering::Acquire);
        assert!(was_set);
        assert!(!SIGNAL_FLAGS[TEST_IDX].load(Ordering::Acquire));

        // Second swap should return false
        let was_set = SIGNAL_FLAGS[TEST_IDX].swap(false, Ordering::Acquire);
        assert!(!was_set);

        // Clean up
        SIGNAL_FLAGS[TEST_IDX].store(false, Ordering::Release);
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn test_signal_handler_installation() {
        // Test that we can install a handler for SIGUSR1 (safe for testing)
        let result = install_signal_handler(libc::SIGUSR1);
        assert!(result.is_ok(), "Failed to install SIGUSR1 handler");

        // Test that we can restore the default handler
        let result = restore_default_handler(libc::SIGUSR1);
        assert!(result.is_ok(), "Failed to restore SIGUSR1 default handler");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn test_signal_delivery() {
        // Install handler for SIGUSR1
        install_signal_handler(libc::SIGUSR1).expect("Failed to install handler");

        // Clear any pending flag
        SIGNAL_FLAGS[libc::SIGUSR1 as usize].store(false, Ordering::Release);

        // Send signal to self
        unsafe {
            libc::kill(libc::getpid(), libc::SIGUSR1);
        }

        // Give a tiny bit of time for signal delivery (should be immediate)
        std::thread::sleep(std::time::Duration::from_millis(1));

        // Check that the flag was set
        let received = SIGNAL_FLAGS[libc::SIGUSR1 as usize].swap(false, Ordering::Acquire);
        assert!(received, "Signal was not received");

        // Restore default handler
        restore_default_handler(libc::SIGUSR1).expect("Failed to restore handler");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn test_invalid_signal_fails() {
        // SIGKILL and SIGSTOP cannot be caught
        let result = install_signal_handler(libc::SIGKILL);
        assert!(result.is_err(), "SIGKILL should not be catchable");

        let result = install_signal_handler(libc::SIGSTOP);
        assert!(result.is_err(), "SIGSTOP should not be catchable");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn test_signal_ignore() {
        // Test that we can set a signal to be ignored
        let result = ignore_signal(libc::SIGUSR2);
        assert!(result.is_ok(), "Failed to ignore SIGUSR2");

        // Restore default
        let result = restore_default_handler(libc::SIGUSR2);
        assert!(result.is_ok(), "Failed to restore SIGUSR2 default handler");
    }

    #[cfg(unix)]
    #[test]
    #[serial]
    fn test_multiple_signals_independent() {
        // Install handlers for two different signals
        install_signal_handler(libc::SIGUSR1).expect("Failed to install SIGUSR1");
        install_signal_handler(libc::SIGUSR2).expect("Failed to install SIGUSR2");

        // Clear flags
        SIGNAL_FLAGS[libc::SIGUSR1 as usize].store(false, Ordering::Release);
        SIGNAL_FLAGS[libc::SIGUSR2 as usize].store(false, Ordering::Release);

        // Send only SIGUSR1
        unsafe {
            libc::kill(libc::getpid(), libc::SIGUSR1);
        }
        std::thread::sleep(std::time::Duration::from_millis(1));

        // Only SIGUSR1 should be set
        assert!(SIGNAL_FLAGS[libc::SIGUSR1 as usize].load(Ordering::Acquire));
        assert!(!SIGNAL_FLAGS[libc::SIGUSR2 as usize].load(Ordering::Acquire));

        // Cleanup
        restore_default_handler(libc::SIGUSR1).expect("Failed to restore SIGUSR1");
        restore_default_handler(libc::SIGUSR2).expect("Failed to restore SIGUSR2");
    }

    #[cfg(unix)]
    #[test]
    fn test_signal_constants_valid() {
        // Verify signal constants are in valid range
        assert!(libc::SIGINT > 0 && (libc::SIGINT as usize) < MAX_SIGNAL);
        assert!(libc::SIGTERM > 0 && (libc::SIGTERM as usize) < MAX_SIGNAL);
        assert!(libc::SIGHUP > 0 && (libc::SIGHUP as usize) < MAX_SIGNAL);
        assert!(libc::SIGPIPE > 0 && (libc::SIGPIPE as usize) < MAX_SIGNAL);
        assert!(libc::SIGUSR1 > 0 && (libc::SIGUSR1 as usize) < MAX_SIGNAL);
        assert!(libc::SIGUSR2 > 0 && (libc::SIGUSR2 as usize) < MAX_SIGNAL);
    }
}
