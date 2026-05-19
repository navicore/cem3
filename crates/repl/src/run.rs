//! Subprocess helpers for the REPL. Runs a compiled Seq program with a
//! bounded timeout so the REPL can't hang on a blocked strand.

use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

/// Default execution timeout in seconds (can be overridden via SEQ_REPL_TIMEOUT)
/// Set higher now that weaves don't block (issue #287 fix) - this is a safety net
const DEFAULT_TIMEOUT_SECS: u64 = 10;

/// Result of running a command with timeout
pub(crate) enum RunResult {
    /// Command completed successfully
    Success { stdout: String },
    /// Command failed with non-zero exit
    Failed { stderr: String, status: ExitStatus },
    /// Command timed out and was killed
    Timeout { timeout_secs: u64 },
    /// Command failed to start
    Error(String),
}

/// Run a compiled program with a timeout
///
/// This prevents the REPL from hanging indefinitely when a program blocks
/// (e.g., creating a weave without resuming it).
pub(crate) fn run_with_timeout(path: &Path) -> RunResult {
    let timeout_secs = std::env::var("SEQ_REPL_TIMEOUT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_TIMEOUT_SECS);

    let timeout = Duration::from_secs(timeout_secs);

    // Spawn the child process
    //
    // stdin is redirected to /dev/null so the child can't share fd 0
    // with the REPL's raw-mode TTY. Without this, keystrokes typed
    // while a slow child is alive (e.g. crypto first-call init) land
    // in the child's stdin buffer and vanish when it exits — the
    // REPL appears to freeze until Ctrl-C. See issue #491.
    let mut child = match Command::new(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return RunResult::Error(format!("Failed to start: {}", e)),
    };

    let start = Instant::now();
    // 50ms is short enough that the REPL feels responsive at the upper bound of
    // a timeout check, and long enough that we don't pin a core polling a child.
    let poll_interval = Duration::from_millis(50);

    // Poll for completion with timeout
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // Process exited - read the stream we'll actually keep.
                let stream = if status.success() {
                    drain_pipe(child.stdout.take())
                } else {
                    drain_pipe(child.stderr.take())
                };

                if status.success() {
                    return RunResult::Success { stdout: stream };
                } else {
                    return RunResult::Failed {
                        stderr: stream,
                        status,
                    };
                }
            }
            Ok(None) => {
                // Still running - check timeout
                if start.elapsed() >= timeout {
                    // Timeout - kill the process
                    let _ = child.kill();
                    let _ = child.wait(); // Reap the zombie
                    return RunResult::Timeout { timeout_secs };
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return RunResult::Error(format!("Wait error: {}", e));
            }
        }
    }
}

/// Read all remaining bytes from an optional child pipe into a String;
/// errors and `None` collapse to an empty string.
fn drain_pipe<R: Read>(pipe: Option<R>) -> String {
    pipe.map(|mut s| {
        let mut buf = String::new();
        let _ = s.read_to_string(&mut buf);
        buf
    })
    .unwrap_or_default()
}
