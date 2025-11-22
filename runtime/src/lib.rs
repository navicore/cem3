//! Seq Runtime: A clean concatenative language foundation
//!
//! Key design principles:
//! - Value: What the language talks about (Int, Bool, Variant, etc.)
//! - StackNode: Implementation detail (contains Value + next pointer)
//! - Variant fields: Stored in arrays, NOT linked via next pointers

pub mod arena;
pub mod arithmetic;
pub mod channel;
pub mod closures;
pub mod cond;
pub mod io;
pub mod pool;
pub mod quotations;
pub mod scheduler;
pub mod seqstring;
pub mod stack;
pub mod string_ops;
pub mod tcp;
pub mod tcp_test;
pub mod value;

// Re-export key types and functions
pub use stack::{
    Stack, StackNode, drop, dup, is_empty, nip, over, peek, pick, pop, push, push_value, rot, swap,
    tuck,
};
pub use value::{Value, VariantData};

// Arithmetic operations (exported for LLVM linking)
pub use arithmetic::{
    add, divide, eq, gt, gte, lt, lte, multiply, neq, push_bool, push_int, subtract,
};

// I/O operations (exported for LLVM linking)
pub use io::{exit_op, push_string, read_line, write_line};

// Scheduler operations (exported for LLVM linking)
pub use scheduler::{
    scheduler_init, scheduler_run, scheduler_shutdown, spawn_strand, strand_spawn,
    wait_all_strands, yield_strand,
};

// Channel operations (exported for LLVM linking)
pub use channel::{close_channel, make_channel, receive, send};

// String operations (exported for LLVM linking)
pub use string_ops::{
    string_concat, string_contains, string_empty, string_length, string_split, string_starts_with,
    string_to_lower, string_to_upper, string_trim,
};

// Quotation operations (exported for LLVM linking)
pub use quotations::{call, forever, push_quotation, spawn, times, until_loop, while_loop};

// Closure operations (exported for LLVM linking)
pub use closures::{create_env, env_get, env_get_int, env_set, make_closure, push_closure};

// Conditional combinator (exported for LLVM linking)
pub use cond::cond;

// TCP operations (exported for LLVM linking)
pub use tcp::{tcp_accept, tcp_close, tcp_listen, tcp_read, tcp_write};
