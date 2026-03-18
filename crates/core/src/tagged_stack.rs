//! Tagged Stack Implementation
//!
//! Selects between two implementations based on the `tagged-ptr` feature flag:
//! - Default: 40-byte StackValue (5 x u64 struct)
//! - `tagged-ptr`: 8-byte StackValue (tagged u64 pointer)

#[cfg(not(feature = "tagged-ptr"))]
#[path = "tagged_stack_old.rs"]
mod imp;

#[cfg(feature = "tagged-ptr")]
#[path = "tagged_stack_new.rs"]
mod imp;

pub use imp::*;
