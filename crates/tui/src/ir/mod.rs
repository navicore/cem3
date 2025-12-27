//! IR visualization modules
//!
//! These modules handle extraction and rendering of various IR representations
//! for the Seq programming language.

pub mod stack_art;
// pub mod effects;    // Phase 1.2
// pub mod typed_ast;  // Phase 2
// pub mod llvm;       // Phase 2

pub use stack_art::*;
