//! Seq Core: A clean foundation for stack-based concatenative languages
//!
//! This crate provides the language-agnostic runtime primitives that can be
//! shared across multiple stack-based languages (Seq, actor languages, etc.)
//!
//! Key design principles:
//! - Value: What the language talks about (Int, Bool, Variant, etc.)
//! - StackValue: 40-byte tagged stack entry (discriminant + 4 payload slots)
//! - Stack: Contiguous array of StackValue entries for efficient operations
//!
//! # Modules
//!
//! - `error`: Thread-local error handling for FFI safety
//! - `memory_stats`: Cross-thread memory statistics registry
//! - `arena`: Thread-local bump allocation for fast value creation
//! - `seqstring`: Arena or globally-allocated strings
//! - `tagged_stack`: Low-level 40-byte stack value layout
//! - `value`: Core Value enum (Int, Float, Bool, String, Variant, Map, etc.)
//! - `stack`: Stack operations and value conversion
//! - `son`: Seq Object Notation serialization

pub mod arena;
pub mod error;
pub mod memory_stats;
pub mod seqstring;
pub mod son;
pub mod stack;
pub mod tagged_stack;
pub mod value;

// Re-export key types and functions
pub use stack::{
    DISC_BOOL, DISC_CHANNEL, DISC_CLOSURE, DISC_FLOAT, DISC_INT, DISC_MAP, DISC_QUOTATION,
    DISC_STRING, DISC_SYMBOL, DISC_VARIANT, DISC_WEAVECTX, Stack, alloc_stack, alloc_test_stack,
    clone_stack, clone_stack_segment, clone_stack_value, drop_stack_value, drop_top, is_empty,
    patch_seq_2dup as two_dup, patch_seq_clone_value as clone_value, patch_seq_drop_op as drop_op,
    patch_seq_dup as dup, patch_seq_nip as nip, patch_seq_over as over,
    patch_seq_pick_op as pick_op, patch_seq_push_value as push_value, patch_seq_roll as roll,
    patch_seq_rot as rot, patch_seq_set_stack_base as set_stack_base,
    patch_seq_stack_dump as stack_dump, patch_seq_swap as swap, patch_seq_tuck as tuck, peek,
    peek_sv, pop, pop_sv, push, push_sv, stack_value_to_value, value_to_stack_value,
};

pub use value::{ChannelData, MapKey, Value, VariantData, WeaveChannelData, WeaveMessage};

// SON serialization
pub use son::{patch_seq_son_dump as son_dump, patch_seq_son_dump_pretty as son_dump_pretty};

// Error handling
pub use error::{
    clear_runtime_error, has_runtime_error, patch_seq_clear_error as clear_error,
    patch_seq_get_error as get_error, patch_seq_has_error as has_error,
    patch_seq_take_error as take_error, set_runtime_error, take_runtime_error,
};
