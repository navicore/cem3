//! Arithmetic operations for Seq
//!
//! These FFI entry points are exported with C ABI for LLVM codegen to
//! call. They are grouped into per-concern sub-modules and re-exported
//! from here so the flat public surface is unchanged.
//!
//! # Overflow Behavior
//!
//! All arithmetic operations use wrapping semantics for defined behavior:
//! - `add`: i64::MAX + 1 wraps to i64::MIN
//! - `subtract`: i64::MIN - 1 wraps to i64::MAX
//! - `multiply`: overflow wraps around
//! - `divide`: i64::MIN / -1 wraps to i64::MIN (special case)

mod arith;
mod bitwise;
mod compare;
mod peek;

#[cfg(test)]
mod tests;

pub use arith::*;
pub use bitwise::*;
pub use compare::*;
pub use peek::*;

// Short-name aliases used by tests and internal callers.
pub use patch_seq_add as add;
pub use patch_seq_and as and;
pub use patch_seq_band as band;
pub use patch_seq_bnot as bnot;
pub use patch_seq_bor as bor;
pub use patch_seq_bxor as bxor;
pub use patch_seq_clz as clz;
pub use patch_seq_ctz as ctz;
pub use patch_seq_divide as divide;
pub use patch_seq_eq as eq;
pub use patch_seq_gt as gt;
pub use patch_seq_gte as gte;
pub use patch_seq_int_bits as int_bits;
pub use patch_seq_lt as lt;
pub use patch_seq_lte as lte;
pub use patch_seq_modulo as modulo;
pub use patch_seq_multiply as multiply;
pub use patch_seq_neq as neq;
pub use patch_seq_not as not;
pub use patch_seq_or as or;
pub use patch_seq_peek_bool_value as peek_bool_value;
pub use patch_seq_peek_int_value as peek_int_value;
pub use patch_seq_pop_stack as pop_stack;
pub use patch_seq_popcount as popcount;
pub use patch_seq_pow as pow;
pub use patch_seq_push_bool as push_bool;
pub use patch_seq_push_int as push_int;
pub use patch_seq_shl as shl;
pub use patch_seq_shr as shr;
pub use patch_seq_subtract as subtract;
