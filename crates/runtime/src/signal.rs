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

/// Signal handler that just sets the atomic flag
/// This is async-signal-safe: only uses atomic operations
#[cfg(unix)]
extern "C" fn flag_signal_handler(sig: libc::c_int) {
    let sig_num = sig as usize;
    if sig_num < MAX_SIGNAL {
        SIGNAL_FLAGS[sig_num].store(true, Ordering::SeqCst);
    }
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

        // Install our flag-setting handler
        libc::signal(sig_num, flag_signal_handler as libc::sighandler_t);
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
        let was_set = SIGNAL_FLAGS[sig_num].swap(false, Ordering::SeqCst);
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

        let is_set = SIGNAL_FLAGS[sig_num].load(Ordering::SeqCst);
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

        libc::signal(sig_num, libc::SIG_DFL);
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

        libc::signal(sig_num, libc::SIG_IGN);
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

        SIGNAL_FLAGS[sig_num].store(false, Ordering::SeqCst);
        stack
    }
}

// Stub implementations for non-Unix platforms
#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_trap(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    // No-op on non-Unix - signals not supported
    stack
}

#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_default(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    stack
}

#[cfg(not(unix))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_signal_ignore(stack: Stack) -> Stack {
    let (stack, _) = unsafe { pop(stack) };
    stack
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_flag_operations() {
        // Test flag is initially false
        assert!(!SIGNAL_FLAGS[2].load(Ordering::SeqCst));

        // Set flag manually (simulating signal receipt)
        SIGNAL_FLAGS[2].store(true, Ordering::SeqCst);
        assert!(SIGNAL_FLAGS[2].load(Ordering::SeqCst));

        // Swap should return old value and set new
        let was_set = SIGNAL_FLAGS[2].swap(false, Ordering::SeqCst);
        assert!(was_set);
        assert!(!SIGNAL_FLAGS[2].load(Ordering::SeqCst));

        // Second swap should return false
        let was_set = SIGNAL_FLAGS[2].swap(false, Ordering::SeqCst);
        assert!(!was_set);
    }
}
