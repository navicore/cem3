//! Stub module for report operations when the "diagnostics" feature is disabled.
//!
//! These are no-op functions that ensure linking works regardless of feature flags.

/// No-op at-exit report when diagnostics is disabled
///
/// # Safety
/// Always safe to call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_report() {
    // No-op: diagnostics feature not enabled
}

/// No-op report init when diagnostics is disabled
///
/// # Safety
/// Always safe to call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_report_init(
    _counters: *const u64,
    _names: *const *const u8,
    _count: i64,
) {
    // No-op: diagnostics feature not enabled
}
