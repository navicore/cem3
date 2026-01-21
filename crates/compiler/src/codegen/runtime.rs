//! Runtime function declarations for LLVM IR.
//!
//! All runtime functions are declared here in a single data-driven table.
//! This eliminates ~500 lines of duplicate writeln! calls and ensures
//! consistency between the FFI and non-FFI code paths.

use super::error::CodeGenError;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::LazyLock;

/// A runtime function declaration for LLVM IR.
pub struct RuntimeDecl {
    /// LLVM declaration string (e.g., "declare ptr @patch_seq_add(ptr)")
    pub decl: &'static str,
    /// Optional category comment (e.g., "; Stack operations")
    pub category: Option<&'static str>,
}

/// All runtime function declarations, organized by category.
/// Each entry generates a single `declare` statement in the LLVM IR.
pub static RUNTIME_DECLARATIONS: LazyLock<Vec<RuntimeDecl>> = LazyLock::new(|| {
    vec![
        // Core push operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_int(ptr, i64)",
            category: Some("; Runtime function declarations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_string(ptr, ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_symbol(ptr, ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_interned_symbol(ptr, ptr)",
            category: None,
        },
        // I/O operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_write(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_write_line(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_read_line(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_read_line_plus(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_read_n(ptr)",
            category: None,
        },
        // Type conversions
        RuntimeDecl {
            decl: "declare ptr @patch_seq_int_to_string(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_symbol_to_string(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_to_symbol(ptr)",
            category: None,
        },
        // Integer arithmetic
        RuntimeDecl {
            decl: "declare ptr @patch_seq_add(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_subtract(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_multiply(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_divide(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_modulo(ptr)",
            category: None,
        },
        // Integer comparisons
        RuntimeDecl {
            decl: "declare ptr @patch_seq_eq(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_lt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_gt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_lte(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_gte(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_neq(ptr)",
            category: None,
        },
        // Boolean operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_and(ptr)",
            category: Some("; Boolean operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_or(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_not(ptr)",
            category: None,
        },
        // Bitwise operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_band(ptr)",
            category: Some("; Bitwise operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_bor(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_bxor(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_bnot(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_shl(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_shr(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_popcount(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_clz(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_ctz(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_int_bits(ptr)",
            category: None,
        },
        // LLVM intrinsics
        RuntimeDecl {
            decl: "declare i64 @llvm.ctpop.i64(i64)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @llvm.ctlz.i64(i64, i1)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @llvm.cttz.i64(i64, i1)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @llvm.memmove.p0.p0.i64(ptr, ptr, i64, i1)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @llvm.trap() noreturn nounwind",
            category: None,
        },
        // Stack operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_dup(ptr)",
            category: Some("; Stack operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_drop_op(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_swap(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_over(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_rot(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_nip(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @patch_seq_clone_value(ptr, ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tuck(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_2dup(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_3drop(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_pick_op(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_roll(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_value(ptr, %Value)",
            category: None,
        },
        // Quotation operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_quotation(ptr, i64, i64)",
            category: Some("; Quotation operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_call(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_peek_is_quotation(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_peek_quotation_fn_ptr(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_spawn(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_weave(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_resume(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_weave_cancel(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_yield(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_cond(ptr)",
            category: None,
        },
        // Closure operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_create_env(i32)",
            category: Some("; Closure operations"),
        },
        RuntimeDecl {
            decl: "declare void @patch_seq_env_set(ptr, i32, %Value)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare %Value @patch_seq_env_get(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_env_get_int(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_env_get_bool(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare double @patch_seq_env_get_float(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_env_get_quotation(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_env_get_string(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_env_push_string(ptr, ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare %Value @patch_seq_make_closure(i64, ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_closure(ptr, i64, i32)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_seqstring(ptr, ptr)",
            category: None,
        },
        // Concurrency operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_channel(ptr)",
            category: Some("; Concurrency operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_chan_send(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_chan_receive(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_close_channel(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_yield_strand(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @patch_seq_maybe_yield()",
            category: None,
        },
        // Scheduler operations
        RuntimeDecl {
            decl: "declare void @patch_seq_scheduler_init()",
            category: Some("; Scheduler operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_scheduler_run()",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @patch_seq_strand_spawn(ptr, ptr)",
            category: None,
        },
        // Command-line argument operations
        RuntimeDecl {
            decl: "declare void @patch_seq_args_init(i32, ptr)",
            category: Some("; Command-line argument operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_arg_count(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_arg_at(ptr)",
            category: None,
        },
        // File operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_file_slurp(ptr)",
            category: Some("; File operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_file_exists(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_file_for_each_line_plus(ptr)",
            category: None,
        },
        // List operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_make(ptr)",
            category: Some("; List operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_push(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_get(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_set(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_map(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_filter(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_fold(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_each(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_length(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_list_empty(ptr)",
            category: None,
        },
        // Map operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_map(ptr)",
            category: Some("; Map operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_get(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_set(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_has(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_remove(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_keys(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_values(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_size(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_map_empty(ptr)",
            category: None,
        },
        // TCP operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tcp_listen(ptr)",
            category: Some("; TCP operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tcp_accept(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tcp_read(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tcp_write(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_tcp_close(ptr)",
            category: None,
        },
        // OS operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_getenv(ptr)",
            category: Some("; OS operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_home_dir(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_current_dir(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_exists(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_is_file(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_is_dir(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_join(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_parent(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_path_filename(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_exit(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_os_name(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_os_arch(ptr)",
            category: None,
        },
        // Signal handling
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_trap(ptr)",
            category: Some("; Signal handling"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_received(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_pending(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_default(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_ignore(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_signal_clear(ptr)",
            category: None,
        },
        // Terminal operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_raw_mode(ptr)",
            category: Some("; Terminal operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_read_char(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_read_char_nonblock(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_width(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_height(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_terminal_flush(ptr)",
            category: None,
        },
        // String operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_concat(ptr)",
            category: Some("; String operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_length(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_byte_length(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_char_at(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_substring(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_char_to_string(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_find(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_split(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_contains(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_starts_with(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_empty(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_trim(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_chomp(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_to_upper(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_to_lower(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_equal(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_json_escape(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_to_int(ptr)",
            category: None,
        },
        // Encoding operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_base64_encode(ptr)",
            category: Some("; Encoding operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_base64_decode(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_base64url_encode(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_base64url_decode(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_hex_encode(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_hex_decode(ptr)",
            category: None,
        },
        // Crypto operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_sha256(ptr)",
            category: Some("; Crypto operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_hmac_sha256(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_constant_time_eq(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_random_bytes(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_random_int(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_uuid4(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_aes_gcm_encrypt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_aes_gcm_decrypt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_pbkdf2_sha256(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_ed25519_keypair(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_ed25519_sign(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_crypto_ed25519_verify(ptr)",
            category: None,
        },
        // HTTP client operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_http_get(ptr)",
            category: Some("; HTTP client operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_http_post(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_http_put(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_http_delete(ptr)",
            category: None,
        },
        // Symbol operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_symbol_equal(ptr)",
            category: Some("; Symbol operations"),
        },
        // Variant operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_field_count(ptr)",
            category: Some("; Variant operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_tag(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_field_at(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_append(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_last(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_variant_init(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_variant_0(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_variant_1(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_variant_2(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_variant_3(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_make_variant_4(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_unpack_variant(ptr, i64)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_symbol_eq_cstr(ptr, ptr)",
            category: None,
        },
        // Float operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_push_float(ptr, double)",
            category: Some("; Float operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_add(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_subtract(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_multiply(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_divide(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_eq(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_lt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_gt(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_lte(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_gte(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_f_neq(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_int_to_float(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_float_to_int(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_float_to_string(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_string_to_float(ptr)",
            category: None,
        },
        // Test framework operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_init(ptr)",
            category: Some("; Test framework operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_finish(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_has_failures(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_assert(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_assert_not(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_assert_eq(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_assert_eq_str(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_fail(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_pass_count(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_test_fail_count(ptr)",
            category: None,
        },
        // Time operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_time_now(ptr)",
            category: Some("; Time operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_time_nanos(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_time_sleep_ms(ptr)",
            category: None,
        },
        // Stack introspection
        RuntimeDecl {
            decl: "declare ptr @patch_seq_stack_dump(ptr)",
            category: Some("; Stack introspection"),
        },
        // SON serialization
        RuntimeDecl {
            decl: "declare ptr @patch_seq_son_dump(ptr)",
            category: Some("; SON serialization"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_son_dump_pretty(ptr)",
            category: None,
        },
        // Regex operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_match(ptr)",
            category: Some("; Regex operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_find(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_find_all(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_replace(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_replace_all(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_captures(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_split(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_regex_valid(ptr)",
            category: None,
        },
        // Compression operations
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_gzip(ptr)",
            category: Some("; Compression operations"),
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_gzip_level(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_gunzip(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_zstd(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_zstd_level(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_compress_unzstd(ptr)",
            category: None,
        },
        // Helpers for conditionals
        RuntimeDecl {
            decl: "declare i64 @patch_seq_peek_int_value(ptr)",
            category: Some("; Helpers for conditionals"),
        },
        RuntimeDecl {
            decl: "declare i1 @patch_seq_peek_bool_value(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @patch_seq_pop_stack(ptr)",
            category: None,
        },
        // Tagged stack operations
        RuntimeDecl {
            decl: "declare ptr @seq_stack_new_default()",
            category: Some("; Tagged stack operations"),
        },
        RuntimeDecl {
            decl: "declare void @seq_stack_free(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare ptr @seq_stack_base(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare i64 @seq_stack_sp(ptr)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @seq_stack_set_sp(ptr, i64)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @seq_stack_grow(ptr, i64)",
            category: None,
        },
        RuntimeDecl {
            decl: "declare void @patch_seq_set_stack_base(ptr)",
            category: None,
        },
    ]
});

/// Mapping from Seq word names to their C runtime symbol names.
/// This centralizes all the name transformations in one place:
/// - Symbolic operators (=, <, >) map to descriptive names (eq, lt, gt)
/// - Hyphens become underscores for C compatibility
/// - Special characters get escaped (?, +, ->)
/// - Reserved words get suffixes (drop -> drop_op)
pub static BUILTIN_SYMBOLS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    HashMap::from([
        // I/O operations
        ("io.write", "patch_seq_write"),
        ("io.write-line", "patch_seq_write_line"),
        ("io.read-line", "patch_seq_read_line"),
        ("io.read-line+", "patch_seq_read_line_plus"),
        ("io.read-n", "patch_seq_read_n"),
        ("int->string", "patch_seq_int_to_string"),
        ("symbol->string", "patch_seq_symbol_to_string"),
        ("string->symbol", "patch_seq_string_to_symbol"),
        // Command-line arguments
        ("args.count", "patch_seq_arg_count"),
        ("args.at", "patch_seq_arg_at"),
        // Integer Arithmetic
        ("i.add", "patch_seq_add"),
        ("i.subtract", "patch_seq_subtract"),
        ("i.multiply", "patch_seq_multiply"),
        ("i.divide", "patch_seq_divide"),
        ("i.modulo", "patch_seq_modulo"),
        // Terse integer arithmetic aliases
        ("i.+", "patch_seq_add"),
        ("i.-", "patch_seq_subtract"),
        ("i.*", "patch_seq_multiply"),
        ("i./", "patch_seq_divide"),
        ("i.%", "patch_seq_modulo"),
        // Integer comparison (symbol form)
        ("i.=", "patch_seq_eq"),
        ("i.<", "patch_seq_lt"),
        ("i.>", "patch_seq_gt"),
        ("i.<=", "patch_seq_lte"),
        ("i.>=", "patch_seq_gte"),
        ("i.<>", "patch_seq_neq"),
        // Integer comparison (verbose form)
        ("i.eq", "patch_seq_eq"),
        ("i.lt", "patch_seq_lt"),
        ("i.gt", "patch_seq_gt"),
        ("i.lte", "patch_seq_lte"),
        ("i.gte", "patch_seq_gte"),
        ("i.neq", "patch_seq_neq"),
        // Boolean
        ("and", "patch_seq_and"),
        ("or", "patch_seq_or"),
        ("not", "patch_seq_not"),
        // Bitwise
        ("band", "patch_seq_band"),
        ("bor", "patch_seq_bor"),
        ("bxor", "patch_seq_bxor"),
        ("bnot", "patch_seq_bnot"),
        ("shl", "patch_seq_shl"),
        ("shr", "patch_seq_shr"),
        ("popcount", "patch_seq_popcount"),
        ("clz", "patch_seq_clz"),
        ("ctz", "patch_seq_ctz"),
        ("int-bits", "patch_seq_int_bits"),
        // Stack operations
        ("dup", "patch_seq_dup"),
        ("swap", "patch_seq_swap"),
        ("over", "patch_seq_over"),
        ("rot", "patch_seq_rot"),
        ("nip", "patch_seq_nip"),
        ("tuck", "patch_seq_tuck"),
        ("2dup", "patch_seq_2dup"),
        ("3drop", "patch_seq_3drop"),
        ("drop", "patch_seq_drop_op"),
        ("pick", "patch_seq_pick_op"),
        ("roll", "patch_seq_roll"),
        // Channel operations (errors are values, not crashes)
        ("chan.make", "patch_seq_make_channel"),
        ("chan.send", "patch_seq_chan_send"),
        ("chan.receive", "patch_seq_chan_receive"),
        ("chan.close", "patch_seq_close_channel"),
        ("chan.yield", "patch_seq_yield_strand"),
        // Quotation operations
        ("call", "patch_seq_call"),
        ("strand.spawn", "patch_seq_spawn"),
        ("strand.weave", "patch_seq_weave"),
        ("strand.resume", "patch_seq_resume"),
        ("strand.weave-cancel", "patch_seq_weave_cancel"),
        ("yield", "patch_seq_yield"),
        ("cond", "patch_seq_cond"),
        // TCP operations
        ("tcp.listen", "patch_seq_tcp_listen"),
        ("tcp.accept", "patch_seq_tcp_accept"),
        ("tcp.read", "patch_seq_tcp_read"),
        ("tcp.write", "patch_seq_tcp_write"),
        ("tcp.close", "patch_seq_tcp_close"),
        // OS operations
        ("os.getenv", "patch_seq_getenv"),
        ("os.home-dir", "patch_seq_home_dir"),
        ("os.current-dir", "patch_seq_current_dir"),
        ("os.path-exists", "patch_seq_path_exists"),
        ("os.path-is-file", "patch_seq_path_is_file"),
        ("os.path-is-dir", "patch_seq_path_is_dir"),
        ("os.path-join", "patch_seq_path_join"),
        ("os.path-parent", "patch_seq_path_parent"),
        ("os.path-filename", "patch_seq_path_filename"),
        ("os.exit", "patch_seq_exit"),
        ("os.name", "patch_seq_os_name"),
        ("os.arch", "patch_seq_os_arch"),
        // Signal handling
        ("signal.trap", "patch_seq_signal_trap"),
        ("signal.received?", "patch_seq_signal_received"),
        ("signal.pending?", "patch_seq_signal_pending"),
        ("signal.default", "patch_seq_signal_default"),
        ("signal.ignore", "patch_seq_signal_ignore"),
        ("signal.clear", "patch_seq_signal_clear"),
        // Terminal operations
        ("terminal.raw-mode", "patch_seq_terminal_raw_mode"),
        ("terminal.read-char", "patch_seq_terminal_read_char"),
        (
            "terminal.read-char?",
            "patch_seq_terminal_read_char_nonblock",
        ),
        ("terminal.width", "patch_seq_terminal_width"),
        ("terminal.height", "patch_seq_terminal_height"),
        ("terminal.flush", "patch_seq_terminal_flush"),
        // String operations
        ("string.concat", "patch_seq_string_concat"),
        ("string.length", "patch_seq_string_length"),
        ("string.byte-length", "patch_seq_string_byte_length"),
        ("string.char-at", "patch_seq_string_char_at"),
        ("string.substring", "patch_seq_string_substring"),
        ("char->string", "patch_seq_char_to_string"),
        ("string.find", "patch_seq_string_find"),
        ("string.split", "patch_seq_string_split"),
        ("string.contains", "patch_seq_string_contains"),
        ("string.starts-with", "patch_seq_string_starts_with"),
        ("string.empty?", "patch_seq_string_empty"),
        ("string.trim", "patch_seq_string_trim"),
        ("string.chomp", "patch_seq_string_chomp"),
        ("string.to-upper", "patch_seq_string_to_upper"),
        ("string.to-lower", "patch_seq_string_to_lower"),
        ("string.equal?", "patch_seq_string_equal"),
        ("string.json-escape", "patch_seq_json_escape"),
        ("string->int", "patch_seq_string_to_int"),
        // Encoding operations
        ("encoding.base64-encode", "patch_seq_base64_encode"),
        ("encoding.base64-decode", "patch_seq_base64_decode"),
        ("encoding.base64url-encode", "patch_seq_base64url_encode"),
        ("encoding.base64url-decode", "patch_seq_base64url_decode"),
        ("encoding.hex-encode", "patch_seq_hex_encode"),
        ("encoding.hex-decode", "patch_seq_hex_decode"),
        // Crypto operations
        ("crypto.sha256", "patch_seq_sha256"),
        ("crypto.hmac-sha256", "patch_seq_hmac_sha256"),
        ("crypto.constant-time-eq", "patch_seq_constant_time_eq"),
        ("crypto.random-bytes", "patch_seq_random_bytes"),
        ("crypto.random-int", "patch_seq_random_int"),
        ("crypto.uuid4", "patch_seq_uuid4"),
        ("crypto.aes-gcm-encrypt", "patch_seq_crypto_aes_gcm_encrypt"),
        ("crypto.aes-gcm-decrypt", "patch_seq_crypto_aes_gcm_decrypt"),
        ("crypto.pbkdf2-sha256", "patch_seq_crypto_pbkdf2_sha256"),
        ("crypto.ed25519-keypair", "patch_seq_crypto_ed25519_keypair"),
        ("crypto.ed25519-sign", "patch_seq_crypto_ed25519_sign"),
        ("crypto.ed25519-verify", "patch_seq_crypto_ed25519_verify"),
        // HTTP client operations
        ("http.get", "patch_seq_http_get"),
        ("http.post", "patch_seq_http_post"),
        ("http.put", "patch_seq_http_put"),
        ("http.delete", "patch_seq_http_delete"),
        // Regex operations
        ("regex.match?", "patch_seq_regex_match"),
        ("regex.find", "patch_seq_regex_find"),
        ("regex.find-all", "patch_seq_regex_find_all"),
        ("regex.replace", "patch_seq_regex_replace"),
        ("regex.replace-all", "patch_seq_regex_replace_all"),
        ("regex.captures", "patch_seq_regex_captures"),
        ("regex.split", "patch_seq_regex_split"),
        ("regex.valid?", "patch_seq_regex_valid"),
        // Compression operations
        ("compress.gzip", "patch_seq_compress_gzip"),
        ("compress.gzip-level", "patch_seq_compress_gzip_level"),
        ("compress.gunzip", "patch_seq_compress_gunzip"),
        ("compress.zstd", "patch_seq_compress_zstd"),
        ("compress.zstd-level", "patch_seq_compress_zstd_level"),
        ("compress.unzstd", "patch_seq_compress_unzstd"),
        // Symbol operations
        ("symbol.=", "patch_seq_symbol_equal"),
        // File operations
        ("file.slurp", "patch_seq_file_slurp"),
        ("file.exists?", "patch_seq_file_exists"),
        ("file.for-each-line+", "patch_seq_file_for_each_line_plus"),
        // List operations
        ("list.make", "patch_seq_list_make"),
        ("list.push", "patch_seq_list_push"),
        ("list.get", "patch_seq_list_get"),
        ("list.set", "patch_seq_list_set"),
        ("list.map", "patch_seq_list_map"),
        ("list.filter", "patch_seq_list_filter"),
        ("list.fold", "patch_seq_list_fold"),
        ("list.each", "patch_seq_list_each"),
        ("list.length", "patch_seq_list_length"),
        ("list.empty?", "patch_seq_list_empty"),
        // Map operations
        ("map.make", "patch_seq_make_map"),
        ("map.get", "patch_seq_map_get"),
        ("map.set", "patch_seq_map_set"),
        ("map.has?", "patch_seq_map_has"),
        ("map.remove", "patch_seq_map_remove"),
        ("map.keys", "patch_seq_map_keys"),
        ("map.values", "patch_seq_map_values"),
        ("map.size", "patch_seq_map_size"),
        ("map.empty?", "patch_seq_map_empty"),
        // Variant operations
        ("variant.field-count", "patch_seq_variant_field_count"),
        ("variant.tag", "patch_seq_variant_tag"),
        ("variant.field-at", "patch_seq_variant_field_at"),
        ("variant.append", "patch_seq_variant_append"),
        ("variant.last", "patch_seq_variant_last"),
        ("variant.init", "patch_seq_variant_init"),
        ("variant.make-0", "patch_seq_make_variant_0"),
        ("variant.make-1", "patch_seq_make_variant_1"),
        ("variant.make-2", "patch_seq_make_variant_2"),
        ("variant.make-3", "patch_seq_make_variant_3"),
        ("variant.make-4", "patch_seq_make_variant_4"),
        // wrap-N aliases for dynamic variant construction (SON)
        ("wrap-0", "patch_seq_make_variant_0"),
        ("wrap-1", "patch_seq_make_variant_1"),
        ("wrap-2", "patch_seq_make_variant_2"),
        ("wrap-3", "patch_seq_make_variant_3"),
        ("wrap-4", "patch_seq_make_variant_4"),
        // Float arithmetic
        ("f.add", "patch_seq_f_add"),
        ("f.subtract", "patch_seq_f_subtract"),
        ("f.multiply", "patch_seq_f_multiply"),
        ("f.divide", "patch_seq_f_divide"),
        // Terse float arithmetic aliases
        ("f.+", "patch_seq_f_add"),
        ("f.-", "patch_seq_f_subtract"),
        ("f.*", "patch_seq_f_multiply"),
        ("f./", "patch_seq_f_divide"),
        // Float comparison (symbol form)
        ("f.=", "patch_seq_f_eq"),
        ("f.<", "patch_seq_f_lt"),
        ("f.>", "patch_seq_f_gt"),
        ("f.<=", "patch_seq_f_lte"),
        ("f.>=", "patch_seq_f_gte"),
        ("f.<>", "patch_seq_f_neq"),
        // Float comparison (verbose form)
        ("f.eq", "patch_seq_f_eq"),
        ("f.lt", "patch_seq_f_lt"),
        ("f.gt", "patch_seq_f_gt"),
        ("f.lte", "patch_seq_f_lte"),
        ("f.gte", "patch_seq_f_gte"),
        ("f.neq", "patch_seq_f_neq"),
        // Float type conversions
        ("int->float", "patch_seq_int_to_float"),
        ("float->int", "patch_seq_float_to_int"),
        ("float->string", "patch_seq_float_to_string"),
        ("string->float", "patch_seq_string_to_float"),
        // Test framework operations
        ("test.init", "patch_seq_test_init"),
        ("test.finish", "patch_seq_test_finish"),
        ("test.has-failures", "patch_seq_test_has_failures"),
        ("test.assert", "patch_seq_test_assert"),
        ("test.assert-not", "patch_seq_test_assert_not"),
        ("test.assert-eq", "patch_seq_test_assert_eq"),
        ("test.assert-eq-str", "patch_seq_test_assert_eq_str"),
        ("test.fail", "patch_seq_test_fail"),
        ("test.pass-count", "patch_seq_test_pass_count"),
        ("test.fail-count", "patch_seq_test_fail_count"),
        // Time operations
        ("time.now", "patch_seq_time_now"),
        ("time.nanos", "patch_seq_time_nanos"),
        ("time.sleep-ms", "patch_seq_time_sleep_ms"),
        // SON serialization
        ("son.dump", "patch_seq_son_dump"),
        ("son.dump-pretty", "patch_seq_son_dump_pretty"),
        // Stack introspection
        ("stack.dump", "patch_seq_stack_dump"),
    ])
});

/// Emit all runtime function declarations to the IR string.
pub fn emit_runtime_decls(ir: &mut String) -> Result<(), CodeGenError> {
    for decl in RUNTIME_DECLARATIONS.iter() {
        if let Some(cat) = decl.category {
            writeln!(ir, "{}", cat)?;
        }
        writeln!(ir, "{}", decl.decl)?;
    }
    writeln!(ir)?;
    Ok(())
}
