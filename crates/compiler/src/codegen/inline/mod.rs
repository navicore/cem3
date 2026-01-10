//! Inline Operation Code Generation
//!
//! This submodule contains all inline code generation for stack operations,
//! arithmetic, comparisons, and loops. These generate LLVM IR directly
//! instead of calling runtime functions.

mod dispatch;
mod ops;

// Re-export for use by parent module (the functions are pub(in crate::codegen))
