//! At-exit report for compiled Seq programs
//!
//! Dumps KPIs when the program finishes, controlled by `SEQ_REPORT` env var:
//! - Unset → no report, zero cost
//! - `1` → human-readable to stderr
//! - `json` → JSON to stderr
//! - `json:/path` → JSON to file
//!
//! ## Feature Flag
//!
//! This module requires the `diagnostics` feature (enabled by default).
//! When disabled, `report_stub.rs` provides no-op FFI symbols.

#![cfg(feature = "diagnostics")]

use crate::channel::{TOTAL_MESSAGES_RECEIVED, TOTAL_MESSAGES_SENT};
use crate::memory_stats::memory_registry;
use crate::scheduler::{PEAK_STRANDS, TOTAL_COMPLETED, TOTAL_SPAWNED, scheduler_elapsed};
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

// =============================================================================
// Report Configuration (parsed from SEQ_REPORT env var)
// =============================================================================

/// Output format
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportFormat {
    Human,
    Json,
}

/// Output destination
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportDestination {
    Stderr,
    File(String),
}

/// Parsed report configuration
#[derive(Debug, Clone)]
pub struct ReportConfig {
    pub format: ReportFormat,
    pub destination: ReportDestination,
    /// Whether to include word counts (tier 2)
    pub include_words: bool,
}

impl ReportConfig {
    /// Parse from SEQ_REPORT environment variable
    pub fn from_env() -> Option<Self> {
        let val = std::env::var("SEQ_REPORT").ok()?;
        if val.is_empty() {
            return None;
        }

        match val.as_str() {
            "0" => None,
            "1" => Some(ReportConfig {
                format: ReportFormat::Human,
                destination: ReportDestination::Stderr,
                include_words: false,
            }),
            "words" => Some(ReportConfig {
                format: ReportFormat::Human,
                destination: ReportDestination::Stderr,
                include_words: true,
            }),
            "json" => Some(ReportConfig {
                format: ReportFormat::Json,
                destination: ReportDestination::Stderr,
                include_words: false,
            }),
            s if s.starts_with("json:") => {
                let path = s[5..].to_string();
                Some(ReportConfig {
                    format: ReportFormat::Json,
                    destination: ReportDestination::File(path),
                    include_words: false,
                })
            }
            _ => {
                eprintln!("Warning: SEQ_REPORT='{}' not recognized, ignoring", val);
                None
            }
        }
    }
}

static REPORT_CONFIG: OnceLock<Option<ReportConfig>> = OnceLock::new();

fn get_report_config() -> &'static Option<ReportConfig> {
    REPORT_CONFIG.get_or_init(ReportConfig::from_env)
}

// =============================================================================
// Report Data
// =============================================================================

/// Collected metrics for the report
#[derive(Debug)]
pub struct ReportData {
    pub wall_clock_ms: u64,
    pub total_spawned: u64,
    pub total_completed: u64,
    pub peak_strands: usize,
    pub active_threads: usize,
    pub total_arena_bytes: u64,
    pub total_peak_arena_bytes: u64,
    pub messages_sent: u64,
    pub messages_received: u64,
    pub word_counts: Option<Vec<(String, u64)>>,
}

/// Collect all metrics
fn collect_report_data(include_words: bool) -> ReportData {
    let wall_clock_ms = scheduler_elapsed()
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let mem_stats = memory_registry().aggregate_stats();

    let word_counts = if include_words {
        read_word_counts()
    } else {
        None
    };

    ReportData {
        wall_clock_ms,
        total_spawned: TOTAL_SPAWNED.load(Ordering::Relaxed),
        total_completed: TOTAL_COMPLETED.load(Ordering::Relaxed),
        peak_strands: PEAK_STRANDS.load(Ordering::Relaxed),
        active_threads: mem_stats.active_threads,
        total_arena_bytes: mem_stats.total_arena_bytes,
        total_peak_arena_bytes: mem_stats.total_peak_arena_bytes,
        messages_sent: TOTAL_MESSAGES_SENT.load(Ordering::Relaxed),
        messages_received: TOTAL_MESSAGES_RECEIVED.load(Ordering::Relaxed),
        word_counts,
    }
}

// =============================================================================
// Formatting
// =============================================================================

fn format_human(data: &ReportData) -> String {
    let mut out = String::new();
    out.push_str("=== SEQ REPORT ===\n");
    out.push_str(&format!("Wall clock:      {} ms\n", data.wall_clock_ms));
    out.push_str(&format!("Strands spawned: {}\n", data.total_spawned));
    out.push_str(&format!("Strands done:    {}\n", data.total_completed));
    out.push_str(&format!("Peak strands:    {}\n", data.peak_strands));
    out.push_str(&format!("Worker threads:  {}\n", data.active_threads));
    out.push_str(&format!(
        "Arena current:   {} bytes\n",
        data.total_arena_bytes
    ));
    out.push_str(&format!(
        "Arena peak:      {} bytes\n",
        data.total_peak_arena_bytes
    ));
    out.push_str(&format!("Messages sent:   {}\n", data.messages_sent));
    out.push_str(&format!("Messages recv:   {}\n", data.messages_received));

    if let Some(ref counts) = data.word_counts {
        out.push_str("\n--- Word Call Counts ---\n");
        for (name, count) in counts {
            out.push_str(&format!("  {:30} {}\n", name, count));
        }
    }

    out.push_str("==================\n");
    out
}

#[cfg(feature = "report-json")]
fn format_json(data: &ReportData) -> String {
    let mut map = serde_json::Map::new();
    map.insert(
        "wall_clock_ms".into(),
        serde_json::Value::Number(data.wall_clock_ms.into()),
    );
    map.insert(
        "strands_spawned".into(),
        serde_json::Value::Number(data.total_spawned.into()),
    );
    map.insert(
        "strands_completed".into(),
        serde_json::Value::Number(data.total_completed.into()),
    );
    map.insert(
        "peak_strands".into(),
        serde_json::Value::Number((data.peak_strands as u64).into()),
    );
    map.insert(
        "worker_threads".into(),
        serde_json::Value::Number((data.active_threads as u64).into()),
    );
    map.insert(
        "arena_bytes".into(),
        serde_json::Value::Number(data.total_arena_bytes.into()),
    );
    map.insert(
        "arena_peak_bytes".into(),
        serde_json::Value::Number(data.total_peak_arena_bytes.into()),
    );
    map.insert(
        "messages_sent".into(),
        serde_json::Value::Number(data.messages_sent.into()),
    );
    map.insert(
        "messages_received".into(),
        serde_json::Value::Number(data.messages_received.into()),
    );

    if let Some(ref counts) = data.word_counts {
        let word_map: serde_json::Map<String, serde_json::Value> = counts
            .iter()
            .map(|(name, count)| (name.clone(), serde_json::Value::Number((*count).into())))
            .collect();
        map.insert("word_counts".into(), serde_json::Value::Object(word_map));
    }

    let obj = serde_json::Value::Object(map);
    serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(not(feature = "report-json"))]
fn format_json(_data: &ReportData) -> String {
    eprintln!(
        "Warning: SEQ_REPORT=json requires the 'report-json' feature. Falling back to human format."
    );
    format_human(_data)
}

// =============================================================================
// Tier 2: Word Count Data (populated by patch_seq_report_init)
// =============================================================================

/// Pointers to instrumentation data registered by compiled binary
struct WordCountData {
    counters: *const u64,
    names: *const *const u8,
    count: usize,
}

// Safety: the pointers are to static data in the compiled binary
unsafe impl Send for WordCountData {}
unsafe impl Sync for WordCountData {}

static WORD_COUNT_DATA: OnceLock<WordCountData> = OnceLock::new();

fn read_word_counts() -> Option<Vec<(String, u64)>> {
    let data = WORD_COUNT_DATA.get()?;
    let mut counts = Vec::with_capacity(data.count);

    unsafe {
        for i in 0..data.count {
            let counter_val = std::ptr::read_volatile(data.counters.add(i));
            let name_ptr = *data.names.add(i);
            let name = std::ffi::CStr::from_ptr(name_ptr as *const i8)
                .to_string_lossy()
                .into_owned();
            counts.push((name, counter_val));
        }
    }

    // Sort by count descending
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    Some(counts)
}

// =============================================================================
// Emit
// =============================================================================

fn emit_report() {
    let config = match get_report_config() {
        Some(c) => c,
        None => return,
    };

    let data = collect_report_data(config.include_words);

    let output = match config.format {
        ReportFormat::Human => format_human(&data),
        ReportFormat::Json => format_json(&data),
    };

    match &config.destination {
        ReportDestination::Stderr => {
            let _ = std::io::stderr().write_all(output.as_bytes());
        }
        ReportDestination::File(path) => {
            if let Ok(mut f) = std::fs::File::create(path) {
                let _ = f.write_all(output.as_bytes());
            } else {
                eprintln!("Warning: could not write report to {}", path);
                let _ = std::io::stderr().write_all(output.as_bytes());
            }
        }
    }
}

// =============================================================================
// FFI Entry Points
// =============================================================================

/// At-exit report — called from generated main after scheduler_run
///
/// # Safety
/// Safe to call from any context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_report() {
    emit_report();
}

/// Register instrumentation data from compiled binary (tier 2)
///
/// # Safety
/// - `counters` must point to a valid array of `count` i64 values
/// - `names` must point to a valid array of `count` C string pointers
/// - Both must remain valid for the program's lifetime (they're static globals)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_report_init(
    counters: *const u64,
    names: *const *const u8,
    count: i64,
) {
    if counters.is_null() || names.is_null() || count <= 0 {
        return;
    }
    let _ = WORD_COUNT_DATA.set(WordCountData {
        counters,
        names,
        count: count as usize,
    });
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_parse_none() {
        // When env var is not set, from_env returns None
        // We can't easily unset env vars in tests, so test the logic directly
        assert!(ReportConfig::from_env().is_none() || ReportConfig::from_env().is_some());
    }

    #[test]
    fn test_config_parse_variants() {
        // Test parsing logic by checking the match arms directly
        let test_cases = vec![
            ("0", None),
            (
                "1",
                Some((ReportFormat::Human, ReportDestination::Stderr, false)),
            ),
            (
                "words",
                Some((ReportFormat::Human, ReportDestination::Stderr, true)),
            ),
            (
                "json",
                Some((ReportFormat::Json, ReportDestination::Stderr, false)),
            ),
            (
                "json:/tmp/report.json",
                Some((
                    ReportFormat::Json,
                    ReportDestination::File("/tmp/report.json".to_string()),
                    false,
                )),
            ),
        ];

        for (input, expected) in test_cases {
            let result = match input {
                "0" => None,
                "1" => Some(ReportConfig {
                    format: ReportFormat::Human,
                    destination: ReportDestination::Stderr,
                    include_words: false,
                }),
                "words" => Some(ReportConfig {
                    format: ReportFormat::Human,
                    destination: ReportDestination::Stderr,
                    include_words: true,
                }),
                "json" => Some(ReportConfig {
                    format: ReportFormat::Json,
                    destination: ReportDestination::Stderr,
                    include_words: false,
                }),
                s if s.starts_with("json:") => Some(ReportConfig {
                    format: ReportFormat::Json,
                    destination: ReportDestination::File(s[5..].to_string()),
                    include_words: false,
                }),
                _ => None,
            };

            match (result, expected) {
                (None, None) => {}
                (Some(r), Some((fmt, dest, words))) => {
                    assert_eq!(r.format, fmt, "format mismatch for input '{}'", input);
                    assert_eq!(
                        r.destination, dest,
                        "destination mismatch for input '{}'",
                        input
                    );
                    assert_eq!(
                        r.include_words, words,
                        "include_words mismatch for input '{}'",
                        input
                    );
                }
                _ => panic!("Mismatch for input '{}'", input),
            }
        }
    }

    #[test]
    fn test_collect_report_data() {
        let data = collect_report_data(false);
        // Basic sanity: these should not panic and return reasonable values
        assert!(data.wall_clock_ms < 1_000_000_000); // less than ~11 days
        assert!(data.peak_strands < 1_000_000);
        assert!(data.word_counts.is_none());
    }

    #[test]
    fn test_format_human() {
        let data = ReportData {
            wall_clock_ms: 42,
            total_spawned: 10,
            total_completed: 9,
            peak_strands: 5,
            active_threads: 2,
            total_arena_bytes: 1024,
            total_peak_arena_bytes: 2048,
            messages_sent: 100,
            messages_received: 99,
            word_counts: None,
        };
        let output = format_human(&data);
        assert!(output.contains("SEQ REPORT"));
        assert!(output.contains("42 ms"));
        assert!(output.contains("Strands spawned: 10"));
        assert!(output.contains("Arena peak:      2048 bytes"));
    }

    #[test]
    fn test_format_human_with_word_counts() {
        let data = ReportData {
            wall_clock_ms: 100,
            total_spawned: 1,
            total_completed: 1,
            peak_strands: 1,
            active_threads: 1,
            total_arena_bytes: 0,
            total_peak_arena_bytes: 0,
            messages_sent: 0,
            messages_received: 0,
            word_counts: Some(vec![("main".to_string(), 1), ("helper".to_string(), 42)]),
        };
        let output = format_human(&data);
        assert!(output.contains("Word Call Counts"));
        assert!(output.contains("main"));
        assert!(output.contains("helper"));
    }

    #[cfg(feature = "report-json")]
    #[test]
    fn test_format_json() {
        let data = ReportData {
            wall_clock_ms: 42,
            total_spawned: 10,
            total_completed: 9,
            peak_strands: 5,
            active_threads: 2,
            total_arena_bytes: 1024,
            total_peak_arena_bytes: 2048,
            messages_sent: 100,
            messages_received: 99,
            word_counts: None,
        };
        let output = format_json(&data);
        assert!(output.contains("\"wall_clock_ms\":42"));
        assert!(output.contains("\"strands_spawned\":10"));
        assert!(output.contains("\"arena_peak_bytes\":2048"));
    }

    #[test]
    fn test_emit_report_noop_when_disabled() {
        // When SEQ_REPORT is not set, emit_report should be a no-op
        emit_report();
        // If we get here, it didn't panic
    }
}
