//! Inline Operation Dispatch (NaN-boxing mode)
//!
//! This module contains the main `try_codegen_inline_op_nanbox` function that dispatches
//! to appropriate inline implementations for stack, arithmetic, and other operations
//! in NaN-boxing mode.
//!
//! Key differences from non-nanbox mode:
//! - %Value is i64 (8 bytes) instead of { i64, i64, i64, i64, i64 } (40 bytes)
//! - getelementptr uses i64 with 8-byte stride
//! - load/store use i64 instead of %Value
//! - memmove size calculations use 8 instead of 40

use super::super::{CodeGen, CodeGenError};
use std::fmt::Write as _;

/// NaN-boxing constants (must match runtime/nanbox.rs)
const NANBOX_BASE: u64 = 0xFFF8_0000_0000_0000;
const TAG_SHIFT: u32 = 47;
const PAYLOAD_MASK: u64 = 0x0000_7FFF_FFFF_FFFF;
const TAG_BOOL: u64 = 1;

impl CodeGen {
    /// Try to generate inline code for a NaN-boxed stack operation.
    /// Returns Some(result_var) if the operation was inlined, None otherwise.
    pub(in crate::codegen) fn try_codegen_inline_op_nanbox(
        &mut self,
        stack_var: &str,
        name: &str,
    ) -> Result<Option<String>, CodeGenError> {
        match name {
            // drop: ( a -- )
            // Must call runtime to properly drop heap values (String, etc.)
            "drop" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

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

                // Get pointer to top value (8-byte element)
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;

                let use_fast_path = self.prev_stmt_is_trivial_literal
                    || self.is_trivially_copyable_at_current_stmt();

                if use_fast_path {
                    // Optimized path: load/store i64 directly
                    let val = self.fresh_temp();
                    writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val, top_ptr)?;
                    writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val, stack_var)?;
                } else {
                    // General path: call clone_value for heap types
                    writeln!(
                        &mut self.output,
                        "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                        top_ptr, stack_var
                    )?;
                }

                // Increment SP
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // swap: ( a b -- b a )
            "swap" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                // Get pointers (8-byte elements)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Load i64 values
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;

                // Store swapped
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_b)?;

                Ok(Some(stack_var.to_string()))
            }

            // over: ( a b -- a b a )
            "over" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_a = self.fresh_temp();
                let result_var = self.fresh_temp();

                // Get pointer to a (sp - 2)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Clone a to top
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;

                // SP = SP + 1
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // rot: ( a b c -- b c a )
            "rot" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_c = self.fresh_temp();
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_c, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -3",
                    ptr_a, stack_var
                )?;

                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();

                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_c, ptr_c)?;

                // Store rotated: a->c, b->a, c->b
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_c, ptr_b)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_c)?;

                Ok(Some(stack_var.to_string()))
            }

            // -rot: ( a b c -- c a b )
            "-rot" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_c = self.fresh_temp();
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_c, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -3",
                    ptr_a, stack_var
                )?;

                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();

                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_c, ptr_c)?;

                // Store reverse-rotated: c->a, a->b, b->c
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_c, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_b)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_c)?;

                Ok(Some(stack_var.to_string()))
            }

            // nip: ( a b -- b )
            "nip" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                // Drop a first
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_drop_value(ptr %{})",
                    ptr_a
                )?;

                // Load b and store at a's position
                let val_b = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;

                // SP = SP - 1
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // tuck: ( a b -- b a b )
            "tuck" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;

                // Clone b to top
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, stack_var
                )?;

                // Store: a position <- b, b position <- a
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_b)?;

                // SP = SP + 1
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // Integer arithmetic
            "i.+" | "i.add" => self.codegen_inline_binary_op_nanbox(stack_var, "add", ""),
            "i.-" | "i.sub" => self.codegen_inline_binary_op_nanbox(stack_var, "sub", ""),
            "i.*" | "i.mul" => self.codegen_inline_binary_op_nanbox(stack_var, "mul", ""),
            "i./" | "i.div" => self.codegen_inline_binary_op_nanbox(stack_var, "sdiv", ""),
            "i.mod" => self.codegen_inline_binary_op_nanbox(stack_var, "srem", ""),
            "i.neg" => self.codegen_inline_negate_nanbox(stack_var),

            // Comparisons
            "i.=" | "i.eq" => self.codegen_inline_comparison_nanbox(stack_var, "eq"),
            "i.<>" | "i.ne" => self.codegen_inline_comparison_nanbox(stack_var, "ne"),
            "i.<" | "i.lt" => self.codegen_inline_comparison_nanbox(stack_var, "slt"),
            "i.<=" | "i.le" => self.codegen_inline_comparison_nanbox(stack_var, "sle"),
            "i.>" | "i.gt" => self.codegen_inline_comparison_nanbox(stack_var, "sgt"),
            "i.>=" | "i.ge" => self.codegen_inline_comparison_nanbox(stack_var, "sge"),

            // Boolean operations
            "not" => self.codegen_inline_not_nanbox(stack_var),

            "and" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;

                // Extract payloads (0 or 1)
                let payload_a = self.fresh_temp();
                let payload_b = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = and i64 %{}, 1", payload_a, val_a)?;
                writeln!(&mut self.output, "  %{} = and i64 %{}, 1", payload_b, val_b)?;

                // AND the payloads
                let result_payload = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i64 %{}, %{}",
                    result_payload, payload_a, payload_b
                )?;

                // Encode as NaN-boxed Bool
                let base_with_tag = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT);
                let result_boxed = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = or i64 %{}, {}",
                    result_boxed, result_payload, base_with_tag
                )?;

                // Store result
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    result_boxed, ptr_a
                )?;

                // SP = SP - 1
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            "or" => {
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;

                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;

                // Extract and OR payloads
                let payload_a = self.fresh_temp();
                let payload_b = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = and i64 %{}, 1", payload_a, val_a)?;
                writeln!(&mut self.output, "  %{} = and i64 %{}, 1", payload_b, val_b)?;

                let result_payload = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = or i64 %{}, %{}",
                    result_payload, payload_a, payload_b
                )?;

                // Encode as NaN-boxed Bool
                let base_with_tag = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT);
                let result_boxed = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = or i64 %{}, {}",
                    result_boxed, result_payload, base_with_tag
                )?;

                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    result_boxed, ptr_a
                )?;

                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // roll: ( ... xn ... x0 n -- ... xn-1 ... x0 xn )
            "roll" => {
                // Check if previous statement was an IntLiteral for constant optimization
                if let Some(n) = self.prev_stmt_int_value {
                    if n >= 0 {
                        return self.codegen_roll_constant_nanbox(stack_var, n as usize);
                    }
                }

                // Dynamic roll
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Get pointer to n
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load and decode n
                let boxed_n = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    boxed_n, n_ptr
                )?;

                // Extract payload (n is small positive, no sign extension needed)
                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i64 %{}, {}",
                    n_val, boxed_n, PAYLOAD_MASK
                )?;

                // Pop n
                let popped_sp = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    popped_sp, stack_var
                )?;

                // Calculate offset to item to roll: -(n + 1)
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 1", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to item to roll
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 %{}",
                    src_ptr, popped_sp, neg_offset
                )?;

                // Load value to roll
                let rolled_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    rolled_val, src_ptr
                )?;

                // Use memmove to shift items down (size = n * 8 bytes)
                let src_plus_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    src_plus_one, src_ptr
                )?;

                let size_bytes = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = mul i64 %{}, 8",
                    size_bytes, n_val
                )?;

                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 %{}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    top_ptr, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    rolled_val, top_ptr
                )?;

                Ok(Some(popped_sp))
            }

            // pick: ( ... xn ... x0 n -- ... xn ... x0 xn )
            "pick" => {
                // Check for constant optimization
                if let Some(n) = self.prev_stmt_int_value {
                    if n >= 0 {
                        return self.codegen_pick_constant_nanbox(stack_var, n as usize);
                    }
                }

                // Dynamic pick
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Get pointer to n
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load and decode n
                let boxed_n = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    boxed_n, n_ptr
                )?;

                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = and i64 %{}, {}",
                    n_val, boxed_n, PAYLOAD_MASK
                )?;

                // Calculate offset: -(n + 2) from stack_var
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 2", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to source
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 %{}",
                    src_ptr, stack_var, neg_offset
                )?;

                // Clone to n position (replacing n)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    src_ptr, n_ptr
                )?;

                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // Not an inline-able operation
            _ => Ok(None),
        }
    }

    /// Generate optimized roll code when N is known at compile time (NaN-box mode)
    pub(super) fn codegen_roll_constant_nanbox(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Pop the N value
        let popped_sp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            popped_sp, stack_var
        )?;

        match n {
            0 => Ok(Some(popped_sp)),
            1 => {
                // swap
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_b, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_a, popped_sp
                )?;
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_b)?;
                Ok(Some(popped_sp))
            }
            2 => {
                // rot
                let ptr_c = self.fresh_temp();
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    ptr_c, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -2",
                    ptr_b, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -3",
                    ptr_a, popped_sp
                )?;
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_a, ptr_a)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_b, ptr_b)?;
                writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val_c, ptr_c)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_b, ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_c, ptr_b)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val_a, ptr_c)?;
                Ok(Some(popped_sp))
            }
            _ => {
                // General case with memmove (constant size)
                let neg_offset = -((n + 1) as i64);
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 {}",
                    src_ptr, popped_sp, neg_offset
                )?;

                let rolled_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    rolled_val, src_ptr
                )?;

                let src_plus_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    src_plus_one, src_ptr
                )?;

                // Size in bytes = n * 8 (constant)
                let size_bytes = n * 8;
                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 {}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 -1",
                    top_ptr, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    rolled_val, top_ptr
                )?;

                Ok(Some(popped_sp))
            }
        }
    }

    /// Generate optimized pick code when N is known at compile time (NaN-box mode)
    pub(super) fn codegen_pick_constant_nanbox(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // n position on stack (sp - 1)
        let n_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            n_ptr, stack_var
        )?;

        // Source is at offset -(n + 2) from stack_var
        let neg_offset = -((n + 2) as i64);
        let src_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 {}",
            src_ptr, stack_var, neg_offset
        )?;

        // Clone source to n position
        writeln!(
            &mut self.output,
            "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
            src_ptr, n_ptr
        )?;

        Ok(Some(stack_var.to_string()))
    }
}
