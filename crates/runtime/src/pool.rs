//! Stack Node Pool - DEPRECATED
//!
//! This module is no longer needed with the tagged stack implementation.
//! The tagged stack uses a contiguous array instead of linked-list nodes,
//! so individual node allocation/deallocation is not required.
//!
//! This stub is kept for compatibility during migration.

use crate::tagged_stack::StackValue;

/// Deprecated: Stack uses contiguous array now
#[allow(dead_code)]
pub fn pool_free(_ptr: *mut StackValue) {
    // No-op: contiguous array doesn't need individual node freeing
}
