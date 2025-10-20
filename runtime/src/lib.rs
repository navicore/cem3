//! cem3 Runtime: A clean concatenative language foundation
//!
//! Key design principles:
//! - Value: What the language talks about (Int, Bool, Variant, etc.)
//! - StackNode: Implementation detail (contains Value + next pointer)
//! - Variant fields: Stored in arrays, NOT linked via next pointers

pub mod stack;
pub mod value;

pub use stack::{
    Stack, StackNode, drop, dup, is_empty, nip, over, peek, pick, pop, push, rot, swap, tuck,
};
pub use value::{Value, VariantData};
