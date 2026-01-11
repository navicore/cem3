//! Inline Operation Code Generation (NaN-boxing mode)
//!
//! This submodule contains all inline code generation for stack operations,
//! arithmetic, comparisons, and loops in NaN-boxing mode.
//!
//! Key differences from non-nanbox mode:
//! - %Value is i64 (8 bytes) instead of { i64, i64, i64, i64, i64 } (40 bytes)
//! - Values are NaN-boxed: floats stored directly, other types encoded in quiet NaN space
//! - No slot0/slot1 layout - the entire i64 is the value
//! - Stack pointer arithmetic uses 8-byte offsets instead of 40-byte

mod dispatch;
mod ops;

// Re-export for use by parent module (the functions are pub(in crate::codegen))
