//! Seq Runtime: A clean concatenative language foundation
//!
//! Key design principles:
//! - Value: What the language talks about (Int, Bool, Variant, etc.)
//! - StackValue: 40-byte tagged stack entry (discriminant + 4 payload slots)
//! - Stack: Contiguous array of StackValue entries for efficient operations

pub mod arena;
pub mod args;
pub mod arithmetic;
pub mod channel;
pub mod closures;
pub mod cond;
pub mod diagnostics;
pub mod file;
pub mod float_ops;
pub mod io;
pub mod list_ops;
pub mod map_ops;
pub mod memory_stats;
pub mod os;
pub mod quotations;
pub mod scheduler;
pub mod seqstring;
pub mod serialize;
pub mod stack;
pub mod string_ops;
pub mod tagged_stack;
pub mod tcp;
pub mod tcp_test;
pub mod test;
pub mod time_ops;
pub mod value;
pub mod variant_ops;
pub mod watchdog;

// Re-export key types and functions
pub use stack::{
    DISC_BOOL, DISC_CHANNEL, DISC_CLOSURE, DISC_FLOAT, DISC_INT, DISC_MAP, DISC_QUOTATION,
    DISC_STRING, DISC_VARIANT, Stack, clone_stack, clone_stack_segment, clone_stack_value,
    drop_stack_value, drop_top, is_empty, patch_seq_2dup as two_dup, patch_seq_3drop as three_drop,
    patch_seq_clone_value as clone_value, patch_seq_drop_op as drop_op, patch_seq_dup as dup,
    patch_seq_nip as nip, patch_seq_over as over, patch_seq_pick_op as pick_op,
    patch_seq_push_value as push_value, patch_seq_rot as rot,
    patch_seq_set_stack_base as set_stack_base, patch_seq_stack_dump as stack_dump,
    patch_seq_swap as swap, patch_seq_tuck as tuck, peek, peek_sv, pop, pop_sv, push, push_sv,
    stack_value_to_value, value_to_stack_value,
};
pub use value::{ChannelData, MapKey, Value, VariantData};

// Serialization types (for persistence/exchange with external systems)
pub use serialize::{SerializeError, TypedMapKey, TypedValue, ValueSerialize};

// Arithmetic operations (exported for LLVM linking)
pub use arithmetic::{
    patch_seq_add as add, patch_seq_divide as divide, patch_seq_eq as eq, patch_seq_gt as gt,
    patch_seq_gte as gte, patch_seq_lt as lt, patch_seq_lte as lte, patch_seq_multiply as multiply,
    patch_seq_neq as neq, patch_seq_push_bool as push_bool, patch_seq_push_int as push_int,
    patch_seq_subtract as subtract,
};

// Float operations (exported for LLVM linking)
pub use float_ops::{
    patch_seq_f_add as f_add, patch_seq_f_divide as f_divide, patch_seq_f_eq as f_eq,
    patch_seq_f_gt as f_gt, patch_seq_f_gte as f_gte, patch_seq_f_lt as f_lt,
    patch_seq_f_lte as f_lte, patch_seq_f_multiply as f_multiply, patch_seq_f_neq as f_neq,
    patch_seq_f_subtract as f_subtract, patch_seq_float_to_int as float_to_int,
    patch_seq_float_to_string as float_to_string, patch_seq_int_to_float as int_to_float,
    patch_seq_push_float as push_float,
};

// I/O operations (exported for LLVM linking)
pub use io::{
    patch_seq_exit_op as exit_op, patch_seq_push_string as push_string,
    patch_seq_read_line as read_line, patch_seq_read_line_plus as read_line_plus,
    patch_seq_write_line as write_line,
};

// Scheduler operations (exported for LLVM linking)
pub use scheduler::{
    patch_seq_scheduler_init as scheduler_init, patch_seq_scheduler_run as scheduler_run,
    patch_seq_scheduler_shutdown as scheduler_shutdown, patch_seq_spawn_strand as spawn_strand,
    patch_seq_strand_spawn as strand_spawn, patch_seq_wait_all_strands as wait_all_strands,
    patch_seq_yield_strand as yield_strand,
};

// Channel operations (exported for LLVM linking)
pub use channel::{
    patch_seq_chan_receive as receive, patch_seq_chan_receive_safe as receive_safe,
    patch_seq_chan_send as send, patch_seq_chan_send_safe as send_safe,
    patch_seq_close_channel as close_channel, patch_seq_make_channel as make_channel,
};

// String operations (exported for LLVM linking)
pub use io::patch_seq_int_to_string as int_to_string;
pub use string_ops::{
    patch_seq_json_escape as json_escape, patch_seq_string_chomp as string_chomp,
    patch_seq_string_concat as string_concat, patch_seq_string_contains as string_contains,
    patch_seq_string_empty as string_empty, patch_seq_string_length as string_length,
    patch_seq_string_split as string_split, patch_seq_string_starts_with as string_starts_with,
    patch_seq_string_to_int as string_to_int, patch_seq_string_to_lower as string_to_lower,
    patch_seq_string_to_upper as string_to_upper, patch_seq_string_trim as string_trim,
};

// Quotation operations (exported for LLVM linking)
pub use quotations::{
    patch_seq_call as call, patch_seq_peek_is_quotation as peek_is_quotation,
    patch_seq_peek_quotation_fn_ptr as peek_quotation_fn_ptr,
    patch_seq_push_quotation as push_quotation, patch_seq_spawn as spawn, patch_seq_times as times,
    patch_seq_until_loop as until_loop, patch_seq_while_loop as while_loop,
};

// Closure operations (exported for LLVM linking)
pub use closures::{
    patch_seq_create_env as create_env, patch_seq_env_get as env_get,
    patch_seq_env_get_int as env_get_int, patch_seq_env_set as env_set,
    patch_seq_make_closure as make_closure, patch_seq_push_closure as push_closure,
};

// Conditional combinator (exported for LLVM linking)
pub use cond::patch_seq_cond as cond;

// TCP operations (exported for LLVM linking)
pub use tcp::{
    patch_seq_tcp_accept as tcp_accept, patch_seq_tcp_close as tcp_close,
    patch_seq_tcp_listen as tcp_listen, patch_seq_tcp_read as tcp_read,
    patch_seq_tcp_write as tcp_write,
};

// OS operations (exported for LLVM linking)
pub use os::{
    patch_seq_current_dir as current_dir, patch_seq_exit as exit, patch_seq_getenv as getenv,
    patch_seq_home_dir as home_dir, patch_seq_os_arch as os_arch, patch_seq_os_name as os_name,
    patch_seq_path_exists as path_exists, patch_seq_path_filename as path_filename,
    patch_seq_path_is_dir as path_is_dir, patch_seq_path_is_file as path_is_file,
    patch_seq_path_join as path_join, patch_seq_path_parent as path_parent,
};

// Variant operations (exported for LLVM linking)
pub use variant_ops::{
    patch_seq_make_variant_0 as make_variant_0, patch_seq_make_variant_1 as make_variant_1,
    patch_seq_make_variant_2 as make_variant_2, patch_seq_make_variant_3 as make_variant_3,
    patch_seq_make_variant_4 as make_variant_4, patch_seq_unpack_variant as unpack_variant,
    patch_seq_variant_field_at as variant_field_at,
    patch_seq_variant_field_count as variant_field_count, patch_seq_variant_tag as variant_tag,
};

// Command-line argument operations (exported for LLVM linking)
pub use args::{
    patch_seq_arg_at as arg_at, patch_seq_arg_count as arg_count, patch_seq_args_init as args_init,
};

// File operations (exported for LLVM linking)
pub use file::{
    patch_seq_file_exists as file_exists,
    patch_seq_file_for_each_line_plus as file_for_each_line_plus,
    patch_seq_file_slurp as file_slurp, patch_seq_file_slurp_safe as file_slurp_safe,
};

// List operations (exported for LLVM linking)
pub use list_ops::{
    patch_seq_list_each as list_each, patch_seq_list_empty as list_empty,
    patch_seq_list_filter as list_filter, patch_seq_list_fold as list_fold,
    patch_seq_list_length as list_length, patch_seq_list_map as list_map,
};

// Map operations (exported for LLVM linking)
pub use map_ops::{
    patch_seq_make_map as make_map, patch_seq_map_empty as map_empty, patch_seq_map_get as map_get,
    patch_seq_map_get_safe as map_get_safe, patch_seq_map_has as map_has,
    patch_seq_map_keys as map_keys, patch_seq_map_remove as map_remove,
    patch_seq_map_set as map_set, patch_seq_map_size as map_size,
    patch_seq_map_values as map_values,
};

// Test framework operations (exported for LLVM linking)
pub use test::{
    patch_seq_test_assert as test_assert, patch_seq_test_assert_eq as test_assert_eq,
    patch_seq_test_assert_eq_str as test_assert_eq_str,
    patch_seq_test_assert_not as test_assert_not, patch_seq_test_fail as test_fail,
    patch_seq_test_fail_count as test_fail_count, patch_seq_test_finish as test_finish,
    patch_seq_test_has_failures as test_has_failures, patch_seq_test_init as test_init,
    patch_seq_test_pass_count as test_pass_count,
};

// Time operations (exported for LLVM linking)
pub use time_ops::{
    patch_seq_time_nanos as time_nanos, patch_seq_time_now as time_now,
    patch_seq_time_sleep_ms as time_sleep_ms,
};
