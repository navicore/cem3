//! cem3 Runtime: A clean concatenative language foundation
//!
//! Key design principles:
//! - Value: What the language talks about (Int, Bool, Variant, etc.)
//! - StackNode: Implementation detail (contains Value + next pointer)
//! - Variant fields: Stored in arrays, NOT linked via next pointers

pub mod arithmetic;
pub mod io;
pub mod stack;
pub mod value;

// Re-export key types and functions
pub use stack::{
    Stack, StackNode, drop, dup, is_empty, nip, over, peek, pick, pop, push, rot, swap, tuck,
};
pub use value::{Value, VariantData};

// Arithmetic operations (exported for LLVM linking)
pub use arithmetic::{add, divide, eq, gt, lt, multiply, push_bool, push_int, subtract};

// I/O operations (exported for LLVM linking)
pub use io::{exit_op, push_string, read_line, write_line};
