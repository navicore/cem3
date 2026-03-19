//! Inline Operation Dispatch
//!
//! This module contains the main `try_codegen_inline_op` function that dispatches
//! to appropriate inline implementations for stack, arithmetic, and other operations.
//!
//! Layout-dependent operations use helpers from `layout.rs` to support
//! both 40-byte StackValue and 8-byte tagged pointer representations.

use super::super::{CodeGen, CodeGenError};
use std::fmt::Write as _;

impl CodeGen {
    /// Try to generate inline code for a tagged stack operation.
    /// Returns Some(result_var) if the operation was inlined, None otherwise.
    pub(in crate::codegen) fn try_codegen_inline_op(
        &mut self,
        stack_var: &str,
        name: &str,
    ) -> Result<Option<String>, CodeGenError> {
        match name {
            // drop: ( a -- )
            // Must call runtime to properly drop heap values
            "drop" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // dup: ( a -- a a )
            "dup" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Get pointer to top value
                let top_ptr = self.emit_stack_gep(stack_var, -1)?;

                // Optimization: use fast path if we know top is trivially copyable
                let use_fast_path = self.prev_stmt_is_trivial_literal
                    || self.is_trivially_copyable_at_current_stmt();

                if use_fast_path {
                    let val = self.emit_load_value(&top_ptr)?;
                    self.emit_store_value(stack_var, &val)?;
                } else {
                    // General path: call clone_value for heap types
                    writeln!(
                        &mut self.output,
                        "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                        top_ptr, stack_var
                    )?;
                }

                // Increment SP
                let result_var = self.emit_stack_gep(stack_var, 1)?;
                Ok(Some(result_var))
            }

            // swap: ( a b -- b a )
            "swap" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.emit_stack_gep(stack_var, -1)?;
                let ptr_a = self.emit_stack_gep(stack_var, -2)?;
                let val_a = self.emit_load_value(&ptr_a)?;
                let val_b = self.emit_load_value(&ptr_b)?;
                self.emit_store_value(&ptr_a, &val_b)?;
                self.emit_store_value(&ptr_b, &val_a)?;
                Ok(Some(stack_var.to_string()))
            }

            // over: ( a b -- a b a )
            "over" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_a = self.emit_stack_gep(stack_var, -2)?;
                // Clone the value from ptr_a to stack_var (current SP)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Increment SP
                let result_var = self.emit_stack_gep(stack_var, 1)?;
                Ok(Some(result_var))
            }

            // Integer arithmetic
            "i.add" | "i.+" => self.codegen_inline_binary_op(stack_var, "add", "sub"),
            "i.subtract" | "i.-" => self.codegen_inline_binary_op(stack_var, "sub", "add"),
            "i.multiply" | "i.*" => self.codegen_inline_binary_op(stack_var, "mul", "div"),

            // Integer comparisons
            "i.=" | "i.eq" => self.codegen_inline_comparison(stack_var, "eq"),
            "i.<>" | "i.neq" => self.codegen_inline_comparison(stack_var, "ne"),
            "i.<" | "i.lt" => self.codegen_inline_comparison(stack_var, "slt"),
            "i.>" | "i.gt" => self.codegen_inline_comparison(stack_var, "sgt"),
            "i.<=" | "i.lte" => self.codegen_inline_comparison(stack_var, "sle"),
            "i.>=" | "i.gte" => self.codegen_inline_comparison(stack_var, "sge"),

            // Float arithmetic
            "f.add" | "f.+" => self.codegen_inline_float_binary_op(stack_var, "fadd"),
            "f.subtract" | "f.-" => self.codegen_inline_float_binary_op(stack_var, "fsub"),
            "f.multiply" | "f.*" => self.codegen_inline_float_binary_op(stack_var, "fmul"),
            "f.divide" | "f./" => self.codegen_inline_float_binary_op(stack_var, "fdiv"),

            // Float comparisons
            "f.=" | "f.eq" => self.codegen_inline_float_comparison(stack_var, "oeq"),
            "f.<>" | "f.neq" => self.codegen_inline_float_comparison(stack_var, "one"),
            "f.<" | "f.lt" => self.codegen_inline_float_comparison(stack_var, "olt"),
            "f.>" | "f.gt" => self.codegen_inline_float_comparison(stack_var, "ogt"),
            "f.<=" | "f.lte" => self.codegen_inline_float_comparison(stack_var, "ole"),
            "f.>=" | "f.gte" => self.codegen_inline_float_comparison(stack_var, "oge"),

            // and: ( a b -- a&&b )
            "and" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

                // AND and convert to 0 or 1
                let and_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i64 %{}, %{}",
                    and_result, val_a, val_b
                )?;
                let bool_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp ne i64 %{}, 0",
                    bool_result, and_result
                )?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, bool_result
                )?;

                self.emit_store_bool(&ptr_a, &zext)?;
                let result_var = self.emit_stack_gep(stack_var, -1)?;
                Ok(Some(result_var))
            }

            // or: ( a b -- a||b )
            "or" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

                let or_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = or i64 %{}, %{}",
                    or_result, val_a, val_b
                )?;
                let bool_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = icmp ne i64 %{}, 0",
                    bool_result, or_result
                )?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, bool_result
                )?;

                self.emit_store_bool(&ptr_a, &zext)?;
                let result_var = self.emit_stack_gep(stack_var, -1)?;
                Ok(Some(result_var))
            }

            // not: ( a -- !a )
            "not" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let (top_ptr, val) = self.emit_load_top_int(stack_var)?;

                let is_zero = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = icmp eq i64 %{}, 0", is_zero, val)?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, is_zero
                )?;

                self.emit_store_bool(&top_ptr, &zext)?;
                Ok(Some(stack_var.to_string()))
            }

            // Bitwise operations
            "band" => self.codegen_inline_int_bitwise_binary(stack_var, "and"),
            "bor" => self.codegen_inline_int_bitwise_binary(stack_var, "or"),
            "bxor" => self.codegen_inline_int_bitwise_binary(stack_var, "xor"),
            "shl" => self.codegen_inline_shift(stack_var, true),
            "shr" => self.codegen_inline_shift(stack_var, false),

            // bnot: ( a -- ~a )
            "bnot" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let (top_ptr, val) = self.emit_load_top_int(stack_var)?;

                let not_result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, -1", not_result, val)?;

                self.emit_store_int_result_in_place(&top_ptr, &not_result)?;
                Ok(Some(stack_var.to_string()))
            }

            // Intrinsics
            "popcount" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctpop.i64"),
            "clz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctlz.i64"),
            "ctz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.cttz.i64"),

            // rot: ( a b c -- b c a )
            "rot" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_c = self.emit_stack_gep(stack_var, -1)?;
                let ptr_b = self.emit_stack_gep(stack_var, -2)?;
                let ptr_a = self.emit_stack_gep(stack_var, -3)?;

                let val_a = self.emit_load_value(&ptr_a)?;
                let val_b = self.emit_load_value(&ptr_b)?;
                let val_c = self.emit_load_value(&ptr_c)?;

                self.emit_store_value(&ptr_a, &val_b)?;
                self.emit_store_value(&ptr_b, &val_c)?;
                self.emit_store_value(&ptr_c, &val_a)?;

                Ok(Some(stack_var.to_string()))
            }

            // nip: ( a b -- b )
            "nip" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_nip(ptr %{})",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // tuck: ( a b -- b a b )
            "tuck" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.emit_stack_gep(stack_var, -1)?;
                let ptr_a = self.emit_stack_gep(stack_var, -2)?;

                let val_a = self.emit_load_value(&ptr_a)?;
                let val_b = self.emit_load_value(&ptr_b)?;

                // Clone b to the new top position
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, stack_var
                )?;

                // Result: b a b
                self.emit_store_value(&ptr_a, &val_b)?;
                self.emit_store_value(&ptr_b, &val_a)?;

                let result_var = self.emit_stack_gep(stack_var, 1)?;
                Ok(Some(result_var))
            }

            // 2dup: ( a b -- a b a b )
            "2dup" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.emit_stack_gep(stack_var, -1)?;
                let ptr_a = self.emit_stack_gep(stack_var, -2)?;

                // Clone a to stack_var
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Clone b to stack_var + 1
                let new_ptr = self.emit_stack_gep(stack_var, 1)?;
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, new_ptr
                )?;

                let result_var = self.emit_stack_gep(stack_var, 2)?;
                Ok(Some(result_var))
            }

            // 3drop: ( a b c -- )
            "3drop" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let drop1 = self.fresh_temp();
                let drop2 = self.fresh_temp();
                let drop3 = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                    drop1, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                    drop2, drop1
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                    drop3, drop2
                )?;
                Ok(Some(drop3))
            }

            // pick: ( ... xn ... x1 x0 n -- ... xn ... x1 x0 xn )
            "pick" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Optimize for constant N
                if let Some(n) = self.prev_stmt_int_value
                    && n >= 0
                {
                    return self.codegen_pick_constant(stack_var, n as usize);
                }

                // Dynamic N: read from stack
                let n_ptr = self.emit_stack_gep(stack_var, -1)?;
                let n_val = self.emit_load_int_payload(&n_ptr)?;

                // Calculate offset: -(n + 2) from stack_var
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 2", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to the item to copy (dynamic GEP)
                let src_ptr = self.fresh_temp();
                if self.tagged_ptr {
                    writeln!(
                        &mut self.output,
                        "  %{} = getelementptr i64, ptr %{}, i64 %{}",
                        src_ptr, stack_var, neg_offset
                    )?;
                } else {
                    writeln!(
                        &mut self.output,
                        "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                        src_ptr, stack_var, neg_offset
                    )?;
                }

                // Clone value from src to n_ptr (replacing n)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    src_ptr, n_ptr
                )?;

                Ok(Some(stack_var.to_string()))
            }

            // roll: ( ... xn xn-1 ... x1 x0 n -- ... xn-1 ... x1 x0 xn )
            "roll" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Optimize for constant N
                if let Some(n) = self.prev_stmt_int_value
                    && n >= 0
                {
                    return self.codegen_roll_constant(stack_var, n as usize);
                }

                // Dynamic N: read from stack
                let n_ptr = self.emit_stack_gep(stack_var, -1)?;
                let n_val = self.emit_load_int_payload(&n_ptr)?;

                // Pop n: new SP is stack_var - 1
                let popped_sp = self.emit_stack_gep(stack_var, -1)?;

                // Calculate offset to xn: -(n + 1) from popped_sp
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 1", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Dynamic GEP to xn
                let src_ptr = self.fresh_temp();
                if self.tagged_ptr {
                    writeln!(
                        &mut self.output,
                        "  %{} = getelementptr i64, ptr %{}, i64 %{}",
                        src_ptr, popped_sp, neg_offset
                    )?;
                } else {
                    writeln!(
                        &mut self.output,
                        "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                        src_ptr, popped_sp, neg_offset
                    )?;
                }

                // Load the value to roll
                let rolled_val = self.emit_load_value(&src_ptr)?;

                // memmove: shift items down
                let src_plus_one = self.emit_stack_gep(&src_ptr, 1)?;

                // Size in bytes = n * value_size
                let value_size = self.value_size_bytes();
                let size_bytes = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = mul i64 %{}, {}",
                    size_bytes, n_val, value_size
                )?;

                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 %{}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top
                let top_ptr = self.emit_stack_gep(&popped_sp, -1)?;
                self.emit_store_value(&top_ptr, &rolled_val)?;

                Ok(Some(popped_sp))
            }

            // >aux: ( T -- ) move top of main stack to aux slot
            ">aux" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let slot_idx = self.current_aux_sp;
                let slot_name = &self.current_aux_slots[slot_idx].clone();

                let top_ptr = self.emit_stack_gep(stack_var, -1)?;
                let val = self.emit_load_value(&top_ptr)?;

                // Store to aux slot
                self.emit_store_value(slot_name, &val)?;

                self.current_aux_sp += 1;

                // Decrement main stack SP
                let result_var = top_ptr.clone();
                Ok(Some(result_var))
            }

            // aux>: ( -- T ) move top of aux stack to main stack
            "aux>" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                debug_assert!(
                    self.current_aux_sp > 0,
                    "aux>: aux stack underflow (typechecker should have caught this)"
                );
                self.current_aux_sp -= 1;
                let slot_idx = self.current_aux_sp;
                let slot_name = &self.current_aux_slots[slot_idx].clone();

                // Load from aux slot
                let val = self.emit_load_value(slot_name)?;

                // Store to main stack at current SP
                self.emit_store_value(stack_var, &val)?;

                // Increment main stack SP
                let result_var = self.emit_stack_gep(stack_var, 1)?;
                Ok(Some(result_var))
            }

            // Not an inline-able operation
            _ => Ok(None),
        }
    }

    /// Generate optimized roll code when N is known at compile time (Issue #192)
    pub(super) fn codegen_roll_constant(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        // Pop the N value from stack
        let popped_sp = self.emit_stack_gep(stack_var, -1)?;

        match n {
            0 => Ok(Some(popped_sp)),
            1 => {
                // 1 roll = swap
                let ptr_b = self.emit_stack_gep(&popped_sp, -1)?;
                let ptr_a = self.emit_stack_gep(&popped_sp, -2)?;
                let val_a = self.emit_load_value(&ptr_a)?;
                let val_b = self.emit_load_value(&ptr_b)?;
                self.emit_store_value(&ptr_a, &val_b)?;
                self.emit_store_value(&ptr_b, &val_a)?;
                Ok(Some(popped_sp))
            }
            2 => {
                // 2 roll = rot
                let ptr_c = self.emit_stack_gep(&popped_sp, -1)?;
                let ptr_b = self.emit_stack_gep(&popped_sp, -2)?;
                let ptr_a = self.emit_stack_gep(&popped_sp, -3)?;
                let val_a = self.emit_load_value(&ptr_a)?;
                let val_b = self.emit_load_value(&ptr_b)?;
                let val_c = self.emit_load_value(&ptr_c)?;
                self.emit_store_value(&ptr_a, &val_b)?;
                self.emit_store_value(&ptr_b, &val_c)?;
                self.emit_store_value(&ptr_c, &val_a)?;
                Ok(Some(popped_sp))
            }
            _ => {
                // n >= 3: Use memmove with constant offsets
                let neg_offset = -((n + 1) as i64);
                let src_ptr = self.emit_stack_gep(&popped_sp, neg_offset)?;

                let rolled_val = self.emit_load_value(&src_ptr)?;

                // memmove: shift items down
                let src_plus_one = self.emit_stack_gep(&src_ptr, 1)?;
                let size_bytes = n as u64 * self.value_size_bytes();
                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 {}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top
                let top_ptr = self.emit_stack_gep(&popped_sp, -1)?;
                self.emit_store_value(&top_ptr, &rolled_val)?;

                Ok(Some(popped_sp))
            }
        }
    }

    /// Generate optimized pick code when N is known at compile time (Issue #192)
    pub(super) fn codegen_pick_constant(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        // Destination: replace n at top of stack (sp - 1)
        let n_ptr = self.emit_stack_gep(stack_var, -1)?;

        // Source offset: -(n + 2) from stack_var
        let neg_offset = -((n + 2) as i64);
        let src_ptr = self.emit_stack_gep(stack_var, neg_offset)?;

        // Clone the value from src to dest
        writeln!(
            &mut self.output,
            "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
            src_ptr, n_ptr
        )?;

        Ok(Some(stack_var.to_string()))
    }
}
