//! Terminal Operations for Seq
//!
//! These functions provide low-level terminal control for building
//! interactive applications (vim-style editors, menus, etc.).
//!
//! # Signal Safety
//!
//! When raw mode is enabled, signal handlers are installed for SIGINT and
//! SIGTERM that restore terminal state before the process exits. This ensures
//! the terminal isn't left in a broken state if the program is killed.
//!
//! # Safety Contract
//!
//! These functions are designed to be called ONLY by compiler-generated code.
//! The compiler is responsible for ensuring correct stack types.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::sync::atomic::{AtomicBool, Ordering};

/// Track whether raw mode is currently enabled
static RAW_MODE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Saved terminal settings (for restoration when exiting raw mode)
static mut SAVED_TERMIOS: Option<libc::termios> = None;

/// Saved signal handlers (for restoration when exiting raw mode)
static mut SAVED_SIGINT_ACTION: Option<libc::sigaction> = None;
static mut SAVED_SIGTERM_ACTION: Option<libc::sigaction> = None;

/// Enable or disable raw terminal mode
///
/// Stack effect: ( Bool -- )
///
/// When enabled:
/// - Input is not line-buffered (characters available immediately)
/// - Echo is disabled
/// - Ctrl+C doesn't generate SIGINT (read as byte 3)
///
/// # Safety
/// Stack must have a Bool value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_raw_mode(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "terminal_raw_mode: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::Bool(enable) => {
            if enable {
                enable_raw_mode();
            } else {
                disable_raw_mode();
            }
            rest
        }
        _ => panic!("terminal_raw_mode: expected Bool on stack, got {:?}", value),
    }
}

/// Read a single character from stdin (blocking)
///
/// Stack effect: ( -- Int )
///
/// Returns:
/// - 0-255: The byte value read
/// - -1: EOF or error
///
/// In raw mode, this returns immediately when a key is pressed.
/// In cooked mode, this waits for Enter.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_read_char(stack: Stack) -> Stack {
    let mut buf = [0u8; 1];
    let result = unsafe { libc::read(0, buf.as_mut_ptr() as *mut libc::c_void, 1) };

    let char_value = if result == 1 {
        buf[0] as i64
    } else {
        -1 // EOF or error
    };

    unsafe { push(stack, Value::Int(char_value)) }
}

/// Read a single character from stdin (non-blocking)
///
/// Stack effect: ( -- Int )
///
/// Returns:
/// - 0-255: The byte value read
/// - -1: No input available, EOF, or error
///
/// This function returns immediately even if no input is available.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_read_char_nonblock(stack: Stack) -> Stack {
    // Save current flags
    let flags = unsafe { libc::fcntl(0, libc::F_GETFL) };

    // Set non-blocking
    unsafe { libc::fcntl(0, libc::F_SETFL, flags | libc::O_NONBLOCK) };

    let mut buf = [0u8; 1];
    let result = unsafe { libc::read(0, buf.as_mut_ptr() as *mut libc::c_void, 1) };

    // Restore original flags
    unsafe { libc::fcntl(0, libc::F_SETFL, flags) };

    let char_value = if result == 1 {
        buf[0] as i64
    } else {
        -1 // No input, EOF, or error
    };

    unsafe { push(stack, Value::Int(char_value)) }
}

/// Get terminal width (columns)
///
/// Stack effect: ( -- Int )
///
/// Returns the number of columns in the terminal, or 80 if unknown.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_width(stack: Stack) -> Stack {
    let width = get_terminal_size().0;
    unsafe { push(stack, Value::Int(width)) }
}

/// Get terminal height (rows)
///
/// Stack effect: ( -- Int )
///
/// Returns the number of rows in the terminal, or 24 if unknown.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_height(stack: Stack) -> Stack {
    let height = get_terminal_size().1;
    unsafe { push(stack, Value::Int(height)) }
}

/// Flush stdout
///
/// Stack effect: ( -- )
///
/// Ensures all buffered output is written to the terminal.
/// Useful after writing escape sequences or partial lines.
///
/// # Safety
/// Always safe to call
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_terminal_flush(stack: Stack) -> Stack {
    unsafe { libc::fsync(1) };
    stack
}

// ============================================================================
// Internal helper functions
// ============================================================================

/// Signal handler that restores terminal state and re-raises the signal
///
/// This is called when SIGINT or SIGTERM is received while in raw mode.
/// It restores the terminal to its original state, then re-raises the signal
/// with the default handler so the process exits with the correct status.
extern "C" fn signal_handler(sig: libc::c_int) {
    // Restore terminal state (safe to call even if already restored)
    unsafe {
        if let Some(ref saved) = SAVED_TERMIOS {
            libc::tcsetattr(0, libc::TCSANOW, saved);
        }
    }

    // Restore default signal handler and re-raise
    unsafe {
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

/// Install signal handlers for SIGINT and SIGTERM
fn install_signal_handlers() {
    unsafe {
        let mut new_action: libc::sigaction = std::mem::zeroed();
        new_action.sa_sigaction = signal_handler as usize;
        libc::sigemptyset(&mut new_action.sa_mask);
        new_action.sa_flags = 0;

        // Save and replace SIGINT handler
        let mut old_sigint: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGINT, &new_action, &mut old_sigint) == 0 {
            SAVED_SIGINT_ACTION = Some(old_sigint);
        }

        // Save and replace SIGTERM handler
        let mut old_sigterm: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(libc::SIGTERM, &new_action, &mut old_sigterm) == 0 {
            SAVED_SIGTERM_ACTION = Some(old_sigterm);
        }
    }
}

/// Restore original signal handlers
fn restore_signal_handlers() {
    unsafe {
        if let Some(ref action) = SAVED_SIGINT_ACTION {
            libc::sigaction(libc::SIGINT, action, std::ptr::null_mut());
        }
        SAVED_SIGINT_ACTION = None;

        if let Some(ref action) = SAVED_SIGTERM_ACTION {
            libc::sigaction(libc::SIGTERM, action, std::ptr::null_mut());
        }
        SAVED_SIGTERM_ACTION = None;
    }
}

fn enable_raw_mode() {
    if RAW_MODE_ENABLED.load(Ordering::SeqCst) {
        return; // Already in raw mode
    }

    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();

        // Get current terminal settings
        if libc::tcgetattr(0, &mut termios) != 0 {
            return; // Failed to get settings
        }

        // Save for later restoration
        SAVED_TERMIOS = Some(termios);

        // Modify for raw mode:
        // - Turn off ICANON (canonical mode) - no line buffering
        // - Turn off ECHO - don't echo typed characters
        // - Turn off ISIG - don't generate signals for Ctrl+C, Ctrl+Z
        // - Turn off IEXTEN - disable implementation-defined input processing
        termios.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);

        // Input flags:
        // - Turn off IXON - disable Ctrl+S/Ctrl+Q flow control
        // - Turn off ICRNL - don't translate CR to NL
        termios.c_iflag &= !(libc::IXON | libc::ICRNL);

        // Output flags:
        // - Turn off OPOST - disable output processing
        termios.c_oflag &= !libc::OPOST;

        // Set VMIN and VTIME for blocking read of 1 character
        termios.c_cc[libc::VMIN] = 1;
        termios.c_cc[libc::VTIME] = 0;

        // Apply settings
        if libc::tcsetattr(0, libc::TCSANOW, &termios) == 0 {
            RAW_MODE_ENABLED.store(true, Ordering::SeqCst);
            // Install signal handlers AFTER successfully entering raw mode
            install_signal_handlers();
        }
    }
}

fn disable_raw_mode() {
    if !RAW_MODE_ENABLED.load(Ordering::SeqCst) {
        return; // Not in raw mode
    }

    // Restore signal handlers BEFORE restoring terminal
    restore_signal_handlers();

    unsafe {
        if let Some(ref saved) = SAVED_TERMIOS {
            libc::tcsetattr(0, libc::TCSANOW, saved);
        }
        SAVED_TERMIOS = None;
        RAW_MODE_ENABLED.store(false, Ordering::SeqCst);
    }
}

fn get_terminal_size() -> (i64, i64) {
    unsafe {
        let mut winsize: libc::winsize = std::mem::zeroed();
        if libc::ioctl(1, libc::TIOCGWINSZ, &mut winsize) == 0 {
            let cols = if winsize.ws_col > 0 {
                winsize.ws_col as i64
            } else {
                80
            };
            let rows = if winsize.ws_row > 0 {
                winsize.ws_row as i64
            } else {
                24
            };
            (cols, rows)
        } else {
            (80, 24) // Default fallback
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_terminal_flush as terminal_flush;
pub use patch_seq_terminal_height as terminal_height;
pub use patch_seq_terminal_raw_mode as terminal_raw_mode;
pub use patch_seq_terminal_read_char as terminal_read_char;
pub use patch_seq_terminal_read_char_nonblock as terminal_read_char_nonblock;
pub use patch_seq_terminal_width as terminal_width;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_terminal_size() {
        // Should return reasonable values (not panic)
        let (width, height) = get_terminal_size();
        assert!(width > 0);
        assert!(height > 0);
    }

    #[test]
    fn test_terminal_width_stack() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = terminal_width(stack);
            let (_, value) = pop(stack);
            match value {
                Value::Int(w) => assert!(w > 0),
                _ => panic!("expected Int"),
            }
        }
    }

    #[test]
    fn test_terminal_height_stack() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = terminal_height(stack);
            let (_, value) = pop(stack);
            match value {
                Value::Int(h) => assert!(h > 0),
                _ => panic!("expected Int"),
            }
        }
    }

    #[test]
    fn test_raw_mode_toggle() {
        // Test that we can toggle raw mode without crashing
        // Note: This may not work in all test environments
        enable_raw_mode();
        disable_raw_mode();
        // Should be back to normal
        assert!(!RAW_MODE_ENABLED.load(Ordering::SeqCst));
    }
}
