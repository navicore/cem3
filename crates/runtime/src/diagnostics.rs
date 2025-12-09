//! Runtime diagnostics for production debugging
//!
//! Provides a SIGQUIT (kill -3) handler that dumps runtime statistics to stderr,
//! similar to JVM thread dumps. This is useful for debugging production issues
//! without stopping the process.
//!
//! ## Usage
//!
//! Send SIGQUIT to a running Seq process:
//! ```bash
//! kill -3 <pid>
//! ```
//!
//! The process will dump diagnostics to stderr and continue running.

use crate::scheduler::ACTIVE_STRANDS;
use std::sync::Once;
use std::sync::atomic::Ordering;

static SIGNAL_HANDLER_INIT: Once = Once::new();

/// Install the SIGQUIT signal handler for diagnostics
///
/// This is called automatically by scheduler_init, but can be called
/// explicitly if needed. Safe to call multiple times (idempotent).
pub fn install_signal_handler() {
    SIGNAL_HANDLER_INIT.call_once(|| {
        #[cfg(unix)]
        {
            unsafe {
                // SIGQUIT = 3 (same as JVM's kill -3 for thread dumps)
                let _ = signal_hook::low_level::register(signal_hook::consts::SIGQUIT, || {
                    dump_diagnostics();
                });
            }
        }

        #[cfg(not(unix))]
        {
            // Signal handling not supported on non-Unix platforms
            // Diagnostics can still be called directly via dump_diagnostics()
        }
    });
}

/// Dump runtime diagnostics to stderr
///
/// This can be called directly from code or triggered via SIGQUIT.
/// Output goes to stderr to avoid mixing with program output.
pub fn dump_diagnostics() {
    use std::io::Write;

    let mut out = std::io::stderr().lock();

    let _ = writeln!(out, "\n=== Seq Runtime Diagnostics ===");
    let _ = writeln!(out, "Timestamp: {:?}", std::time::SystemTime::now());

    // Strand count (global atomic - accurate)
    let active = ACTIVE_STRANDS.load(Ordering::Relaxed);
    let _ = writeln!(out, "\n[Strands]");
    let _ = writeln!(out, "  Active: {}", active);

    // Channel stats (global registry - accurate if lock available)
    let _ = writeln!(out, "\n[Channels]");
    match get_channel_count() {
        Some(count) => {
            let _ = writeln!(out, "  Open channels: {}", count);
        }
        None => {
            let _ = writeln!(out, "  Open channels: (unavailable - registry locked)");
        }
    }

    let _ = writeln!(out, "\n=== End Diagnostics ===\n");
}

/// Try to get channel count without blocking
/// Returns None if the registry lock is held
fn get_channel_count() -> Option<usize> {
    use crate::channel::channel_count;
    channel_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_diagnostics_runs() {
        // Just verify it doesn't panic
        dump_diagnostics();
    }

    #[test]
    fn test_install_signal_handler_idempotent() {
        // Should be safe to call multiple times
        install_signal_handler();
        install_signal_handler();
        install_signal_handler();
    }
}
