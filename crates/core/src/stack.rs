//! Stack Operations
//!
//! Selects between two implementations based on the `tagged-ptr` feature flag:
//! - Default: 40-byte StackValue with slot-based encoding
//! - `tagged-ptr`: 8-byte tagged pointer with Box<Value> for heap types

#[cfg(not(feature = "tagged-ptr"))]
#[path = "stack_old.rs"]
mod imp;

#[cfg(feature = "tagged-ptr")]
#[path = "stack_new.rs"]
mod imp;

pub use imp::*;
