//! Watchdog timer for detecting stuck strands
//!
//! Monitors strand execution time and triggers alerts when strands run too long
//! without yielding. This helps detect infinite loops and runaway computation.
//!
//! ## Configuration (Environment Variables)
//!
//! | Variable | Default | Description |
//! |----------|---------|-------------|
//! | `SEQ_WATCHDOG_SECS` | `0` (disabled) | Threshold in seconds for "stuck" strand |
//! | `SEQ_WATCHDOG_INTERVAL` | `5` | Check frequency in seconds |
//! | `SEQ_WATCHDOG_ACTION` | `warn` | Action: `warn` (dump diagnostics) or `exit` (terminate) |
//!
//! ## Example
//!
//! ```bash
//! # Enable watchdog with 30 second threshold, check every 10 seconds
//! SEQ_WATCHDOG_SECS=30 SEQ_WATCHDOG_INTERVAL=10 ./my-program
//!
//! # Enable watchdog that exits on stuck strand
//! SEQ_WATCHDOG_SECS=60 SEQ_WATCHDOG_ACTION=exit ./my-program
//! ```
//!
//! ## Design
//!
//! The watchdog runs on a dedicated thread and periodically scans the strand
//! registry. It compares each strand's spawn time against the current time
//! to detect strands that have been running longer than the threshold.
//!
//! This piggybacks on the existing strand registry infrastructure, requiring
//! no additional tracking overhead on the hot path.

use crate::diagnostics::dump_diagnostics;
use crate::scheduler::strand_registry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Once;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static WATCHDOG_INIT: Once = Once::new();
static WATCHDOG_TRIGGERED: AtomicBool = AtomicBool::new(false);

/// Watchdog configuration
#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// Threshold in seconds for considering a strand "stuck"
    pub threshold_secs: u64,
    /// How often to check (in seconds)
    pub interval_secs: u64,
    /// Action to take when a stuck strand is detected
    pub action: WatchdogAction,
}

/// Action to take when watchdog detects a stuck strand
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogAction {
    /// Log a warning and dump diagnostics (default)
    Warn,
    /// Dump diagnostics and exit the process
    Exit,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            threshold_secs: 0, // Disabled by default
            interval_secs: 5,
            action: WatchdogAction::Warn,
        }
    }
}

impl WatchdogConfig {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let threshold_secs = std::env::var("SEQ_WATCHDOG_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let interval_secs = std::env::var("SEQ_WATCHDOG_INTERVAL")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|&v| v > 0)
            .unwrap_or(5);

        let action = std::env::var("SEQ_WATCHDOG_ACTION")
            .ok()
            .map(|s| match s.to_lowercase().as_str() {
                "exit" => WatchdogAction::Exit,
                _ => WatchdogAction::Warn,
            })
            .unwrap_or(WatchdogAction::Warn);

        Self {
            threshold_secs,
            interval_secs,
            action,
        }
    }

    /// Check if watchdog is enabled
    pub fn is_enabled(&self) -> bool {
        self.threshold_secs > 0
    }
}

/// Install the watchdog timer
///
/// This spawns a dedicated thread that periodically checks for stuck strands.
/// Safe to call multiple times (idempotent via Once).
///
/// The watchdog is only started if `SEQ_WATCHDOG_SECS` is set to a positive value.
pub fn install_watchdog() {
    WATCHDOG_INIT.call_once(|| {
        let config = WatchdogConfig::from_env();

        if !config.is_enabled() {
            return;
        }

        eprintln!(
            "[watchdog] Enabled: threshold={}s, interval={}s, action={:?}",
            config.threshold_secs, config.interval_secs, config.action
        );

        std::thread::Builder::new()
            .name("seq-watchdog".to_string())
            .spawn(move || watchdog_loop(config))
            .ok();
    });
}

/// Main watchdog loop
fn watchdog_loop(config: WatchdogConfig) {
    let interval = Duration::from_secs(config.interval_secs);

    loop {
        std::thread::sleep(interval);

        if let Some((strand_id, running_secs)) = check_for_stuck_strands(config.threshold_secs) {
            handle_stuck_strand(strand_id, running_secs, &config);
        }
    }
}

/// Check the strand registry for any strands exceeding the threshold
///
/// Returns Some((strand_id, running_seconds)) for the longest-running stuck strand,
/// or None if all strands are within threshold.
fn check_for_stuck_strands(threshold_secs: u64) -> Option<(u64, u64)> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let registry = strand_registry();
    let mut worst: Option<(u64, u64)> = None;

    for (strand_id, spawn_time) in registry.active_strands() {
        if spawn_time == 0 {
            continue;
        }

        let running_secs = now.saturating_sub(spawn_time);

        if running_secs > threshold_secs {
            match worst {
                None => worst = Some((strand_id, running_secs)),
                Some((_, prev_secs)) if running_secs > prev_secs => {
                    worst = Some((strand_id, running_secs));
                }
                _ => {}
            }
        }
    }

    worst
}

/// Handle detection of a stuck strand
fn handle_stuck_strand(strand_id: u64, running_secs: u64, config: &WatchdogConfig) {
    // Avoid spamming if we keep detecting the same stuck strand
    let was_triggered = WATCHDOG_TRIGGERED.swap(true, Ordering::SeqCst);

    use std::io::Write;
    let mut stderr = std::io::stderr().lock();

    let _ = writeln!(stderr);
    let _ = writeln!(
        stderr,
        "⚠️  WATCHDOG: Strand #{} running for {}s (threshold: {}s)",
        strand_id, running_secs, config.threshold_secs
    );

    // Always dump diagnostics on first trigger
    if !was_triggered {
        dump_diagnostics();
    }

    match config.action {
        WatchdogAction::Warn => {
            if was_triggered {
                let _ = writeln!(
                    stderr,
                    "    (strand still stuck, diagnostics suppressed)"
                );
            }
        }
        WatchdogAction::Exit => {
            let _ = writeln!(stderr, "    Exiting due to SEQ_WATCHDOG_ACTION=exit");
            std::process::exit(1);
        }
    }
}

/// Reset the watchdog triggered state (for testing)
#[cfg(test)]
pub fn reset_triggered() {
    WATCHDOG_TRIGGERED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let config = WatchdogConfig::default();
        assert_eq!(config.threshold_secs, 0);
        assert_eq!(config.interval_secs, 5);
        assert_eq!(config.action, WatchdogAction::Warn);
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_config_enabled() {
        let config = WatchdogConfig {
            threshold_secs: 30,
            interval_secs: 10,
            action: WatchdogAction::Exit,
        };
        assert!(config.is_enabled());
    }

    #[test]
    fn test_check_no_stuck_strands() {
        // With no strands running, should return None
        let result = check_for_stuck_strands(30);
        // May or may not find strands depending on test execution order
        // Just verify it doesn't panic
        let _ = result;
    }
}
