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
//!
//! ## Signal Safety
//!
//! Signal handlers can only safely call async-signal-safe functions. Our
//! dump_diagnostics() does I/O and acquires locks, which is NOT safe to call
//! directly from a signal handler. Instead, we spawn a dedicated thread that
//! waits for signals using signal-hook's iterator API, making all the I/O
//! operations safe.

use crate::memory_stats::memory_registry;
use crate::scheduler::{
    ACTIVE_STRANDS, PEAK_STRANDS, TOTAL_COMPLETED, TOTAL_SPAWNED, strand_registry,
};
use std::sync::Once;
use std::sync::atomic::Ordering;

static SIGNAL_HANDLER_INIT: Once = Once::new();

/// Maximum number of individual strands to display in diagnostics output
/// to avoid overwhelming the output for programs with many strands
const STRAND_DISPLAY_LIMIT: usize = 20;

/// Install the SIGQUIT signal handler for diagnostics
///
/// This is called automatically by scheduler_init, but can be called
/// explicitly if needed. Safe to call multiple times (idempotent).
///
/// # Implementation
///
/// Uses a dedicated thread to handle signals safely. The signal-hook iterator
/// API ensures we're not calling non-async-signal-safe functions from within
/// a signal handler context.
pub fn install_signal_handler() {
    SIGNAL_HANDLER_INIT.call_once(|| {
        #[cfg(unix)]
        {
            use signal_hook::consts::SIGQUIT;
            use signal_hook::iterator::Signals;

            // Create signal iterator - this is safe and doesn't block
            let mut signals = match Signals::new([SIGQUIT]) {
                Ok(s) => s,
                Err(_) => return, // Silently fail if we can't register
            };

            // Spawn a dedicated thread to handle signals
            // This thread blocks waiting for signals, then safely calls dump_diagnostics()
            std::thread::Builder::new()
                .name("seq-diagnostics".to_string())
                .spawn(move || {
                    for sig in signals.forever() {
                        if sig == SIGQUIT {
                            dump_diagnostics();
                        }
                    }
                })
                .ok(); // Silently fail if thread spawn fails
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

    // Strand statistics (global atomics - accurate)
    let active = ACTIVE_STRANDS.load(Ordering::Relaxed);
    let total_spawned = TOTAL_SPAWNED.load(Ordering::Relaxed);
    let total_completed = TOTAL_COMPLETED.load(Ordering::Relaxed);
    let peak = PEAK_STRANDS.load(Ordering::Relaxed);

    let _ = writeln!(out, "\n[Strands]");
    let _ = writeln!(out, "  Active:    {}", active);
    let _ = writeln!(out, "  Spawned:   {} (total)", total_spawned);
    let _ = writeln!(out, "  Completed: {} (total)", total_completed);
    let _ = writeln!(out, "  Peak:      {} (high-water mark)", peak);

    // Calculate potential leak indicator
    // If spawned > completed + active, some strands were lost (panic, etc.)
    let expected_completed = total_spawned.saturating_sub(active as u64);
    if total_completed < expected_completed {
        let lost = expected_completed - total_completed;
        let _ = writeln!(
            out,
            "  WARNING: {} strands may have been lost (panic/abort)",
            lost
        );
    }

    // Active strand details from registry
    let registry = strand_registry();
    let overflow = registry.overflow_count.load(Ordering::Relaxed);

    let _ = writeln!(out, "\n[Active Strand Details]");
    let _ = writeln!(out, "  Registry capacity: {} slots", registry.capacity());
    if overflow > 0 {
        let _ = writeln!(
            out,
            "  WARNING: {} strands exceeded registry capacity (not tracked)",
            overflow
        );
    }

    // Get current time for duration calculation
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Collect and sort active strands by spawn time (oldest first)
    let mut strands: Vec<_> = registry.active_strands().collect();
    strands.sort_by_key(|(_, spawn_time)| *spawn_time);

    if strands.is_empty() {
        let _ = writeln!(out, "  (no active strands in registry)");
    } else {
        let _ = writeln!(out, "  {} strand(s) tracked:", strands.len());
        for (idx, (strand_id, spawn_time)) in strands.iter().take(STRAND_DISPLAY_LIMIT).enumerate()
        {
            let duration = now.saturating_sub(*spawn_time);
            let _ = writeln!(
                out,
                "    [{:2}] Strand #{:<8} running for {}s",
                idx + 1,
                strand_id,
                duration
            );
        }
        if strands.len() > STRAND_DISPLAY_LIMIT {
            let _ = writeln!(
                out,
                "    ... and {} more strands",
                strands.len() - STRAND_DISPLAY_LIMIT
            );
        }
    }

    // Memory statistics (cross-thread aggregation)
    let _ = writeln!(out, "\n[Memory]");
    let mem_stats = memory_registry().aggregate_stats();
    let _ = writeln!(out, "  Tracked threads: {}", mem_stats.active_threads);
    let _ = writeln!(
        out,
        "  Arena bytes:     {} (across all threads)",
        format_bytes(mem_stats.total_arena_bytes)
    );
    if mem_stats.overflow_count > 0 {
        let _ = writeln!(
            out,
            "  WARNING: {} threads exceeded registry capacity (memory not tracked)",
            mem_stats.overflow_count
        );
        let _ = writeln!(
            out,
            "           Consider increasing MAX_THREADS in memory_stats.rs (currently 64)"
        );
    }

    // Note: Channel stats are not available with the zero-mutex design.
    // Channels are passed directly as Value::Channel on the stack with no global registry.

    let _ = writeln!(out, "\n=== End Diagnostics ===\n");
}

/// Format bytes as human-readable string
fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.2} KB", bytes as f64 / 1024.0)
    } else {
        format!("{} B", bytes)
    }
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
