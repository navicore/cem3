//! Inline Operation Code Generation
//!
//! This module contains helper functions for generating inline LLVM IR
//! for common operations like comparisons, arithmetic, and loops.
//! These are called by try_codegen_inline_op in the main module.

use super::super::{CodeGen, CodeGenError, VirtualValue};
use crate::ast::Statement;
use std::fmt::Write as _;

impl CodeGen {
    /// Generate inline code for comparison operations.
    /// Returns Value::Bool (discriminant 2 at slot0, 0/1 at slot1).
    pub(in crate::codegen) fn codegen_inline_comparison(
        &mut self,
        stack_var: &str,
        icmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189) - comparison returns Bool, not Int
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values are at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Compare
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp {} i64 %{}, %{}",
            cmp_result, icmp_op, val_a, val_b
        )?;

        // Convert i1 to i64
        let zext = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            zext, cmp_result
        )?;

        // Store result as Value::Bool (discriminant 2 at slot0, 0/1 at slot1)
        writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
        writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for binary arithmetic (add/subtract).
    /// Issue #189: Uses virtual registers when both operands are available.
    /// Issue #215: Split into fast/slow path helpers to reduce function size.
    pub(in crate::codegen) fn codegen_inline_binary_op(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
        _adjust_op: &str, // No longer needed, kept for compatibility
    ) -> Result<Option<String>, CodeGenError> {
        // Try fast path with virtual registers
        if self.virtual_stack.len() >= 2
            && let Some(result) = self.codegen_binary_op_virtual(stack_var, llvm_op)?
        {
            return Ok(Some(result));
        }

        // Fall back to memory path
        self.codegen_binary_op_memory(stack_var, llvm_op)
    }

    /// Fast path: both operands in virtual registers (Issue #215: extracted helper).
    /// Returns None if operands aren't both integers, leaving virtual_stack unchanged.
    pub(in crate::codegen) fn codegen_binary_op_virtual(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        let val_b = self.virtual_stack.pop().unwrap();
        let val_a = self.virtual_stack.pop().unwrap();

        // Both must be integers for this optimization
        let (ssa_a, ssa_b) = match (&val_a, &val_b) {
            (VirtualValue::Int { ssa_var: a, .. }, VirtualValue::Int { ssa_var: b, .. }) => {
                (a.clone(), b.clone())
            }
            _ => {
                // Not both integers - restore and signal fallback needed
                self.virtual_stack.push(val_a);
                self.virtual_stack.push(val_b);
                return Ok(None);
            }
        };

        // Perform the operation directly on SSA values
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, ssa_a, ssa_b
        )?;

        // Push result to virtual stack
        let result = VirtualValue::Int {
            ssa_var: op_result,
            value: 0, // We don't track constant values through operations yet
        };
        Ok(Some(self.push_virtual(result, stack_var)?))
    }

    /// Slow path: spill virtual stack and operate on memory (Issue #215: extracted helper).
    pub(in crate::codegen) fn codegen_binary_op_memory(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get pointers to slot1 (actual value, offset 8 bytes from Value start)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load actual values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Perform the operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result (discriminant already 0 from original push)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for float binary operations (f.add, f.subtract, etc.)
    /// Values are stored as f64 bits in slot1, discriminant 1 (Float).
    pub(in crate::codegen) fn codegen_inline_float_binary_op(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load operands as doubles (Issue #215: extracted helper)
        let (_ptr_a, slot1_a, val_a, val_b) = self.codegen_float_load_operands(stack_var)?;

        // Perform the float operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} double %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Bitcast result back to i64
        let result_bits = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast double %{} to i64",
            result_bits, op_result
        )?;

        // Store result at slot1 (discriminant 1 already at slot0)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result_bits, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Load two float operands from stack (Issue #215: extracted helper).
    /// Returns (ptr_a, slot1_a, val_a, val_b) where vals are doubles.
    pub(in crate::codegen) fn codegen_float_load_operands(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String, String, String), CodeGenError> {
        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1 as i64 (raw bits)
        let bits_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_a, slot1_a
        )?;
        let bits_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            bits_b, slot1_b
        )?;

        // Bitcast to double
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_a, bits_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 %{} to double",
            val_b, bits_b
        )?;

        Ok((ptr_a, slot1_a, val_a, val_b))
    }

    /// Generate inline code for float comparison operations.
    /// Returns Value::Bool (discriminant 2 at slot0, 0/1 at slot1).
    pub(in crate::codegen) fn codegen_inline_float_comparison(
        &mut self,
        stack_var: &str,
        fcmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load operands as doubles (Issue #215: reuse helper)
        let (ptr_a, slot1_a, val_a, val_b) = self.codegen_float_load_operands(stack_var)?;

        // Compare using fcmp
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = fcmp {} double %{}, %{}",
            cmp_result, fcmp_op, val_a, val_b
        )?;

        // Convert i1 to i64
        let zext = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            zext, cmp_result
        )?;

        // Store result as Value::Bool (discriminant 2 at slot0, 0/1 at slot1)
        writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
        writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for integer bitwise binary operations.
    /// Returns tagged int (discriminant 0).
    pub(in crate::codegen) fn codegen_inline_int_bitwise_binary(
        &mut self,
        stack_var: &str,
        llvm_op: &str, // "and", "or", "xor"
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointers to Value slots
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        // Perform the bitwise operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result (discriminant stays 0 for Int, just update slot1)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for shift operations with proper edge case handling.
    /// Matches runtime behavior: returns 0 for negative shift or shift >= 64.
    /// For shr, uses logical (not arithmetic) shift to match runtime.
    pub(in crate::codegen) fn codegen_inline_shift(
        &mut self,
        stack_var: &str,
        is_left: bool, // true for shl, false for shr
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load operands from memory (Issue #215: extracted helper)
        let (slot1_a, val_a, val_b) = self.codegen_shift_load_operands(stack_var)?;

        // Perform bounds-checked shift (Issue #215: extracted helper)
        let op_result = self.codegen_shift_compute(&val_a, &val_b, is_left)?;

        // Store result (discriminant stays 0 for Int, just update slot1)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            op_result, slot1_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Load two operands for shift operation (Issue #215: extracted helper).
    /// Returns (slot1_a, val_a, val_b) where slot1_a is the store target.
    pub(in crate::codegen) fn codegen_shift_load_operands(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String, String), CodeGenError> {
        // Get pointers to Value slots (b = shift count, a = value to shift)
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Get slot1 pointers (values at offset 8)
        let slot1_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_a, ptr_a
        )?;
        let slot1_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_b, ptr_b
        )?;

        // Load values from slot1
        let val_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_a, slot1_a
        )?;
        let val_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val_b, slot1_b
        )?;

        Ok((slot1_a, val_a, val_b))
    }

    /// Perform bounds-checked shift operation (Issue #215: extracted helper).
    /// Returns the result SSA variable name.
    pub(in crate::codegen) fn codegen_shift_compute(
        &mut self,
        val_a: &str,
        val_b: &str,
        is_left: bool,
    ) -> Result<String, CodeGenError> {
        // Check if shift count is negative
        let is_neg = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp slt i64 %{}, 0",
            is_neg, val_b
        )?;

        // Check if shift count >= 64
        let is_overflow = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, 64",
            is_overflow, val_b
        )?;

        // Combine: is_invalid = is_neg OR is_overflow
        let is_invalid = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i1 %{}, %{}",
            is_invalid, is_neg, is_overflow
        )?;

        // Use a safe shift count (clamped to 0 if invalid) to avoid LLVM UB
        let safe_count = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            safe_count, is_invalid, val_b
        )?;

        // Perform the shift operation with safe count
        let shift_result = self.fresh_temp();
        let op = if is_left { "shl" } else { "lshr" };
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            shift_result, op, val_a, safe_count
        )?;

        // Select final result: 0 if invalid, otherwise shift_result
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            op_result, is_invalid, shift_result
        )?;

        Ok(op_result)
    }

    /// Generate inline code for integer unary intrinsic operations.
    /// Used for popcount, clz, ctz which use LLVM intrinsics.
    pub(in crate::codegen) fn codegen_inline_int_unary_intrinsic(
        &mut self,
        stack_var: &str,
        intrinsic: &str, // "llvm.ctpop.i64", "llvm.ctlz.i64", "llvm.cttz.i64"
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointer to top Value
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;

        // Get pointer to slot1 (value at offset 8)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;

        // Load value from slot1
        let val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            val, slot1_ptr
        )?;

        // Call the intrinsic
        let result = self.fresh_temp();
        if intrinsic == "llvm.ctpop.i64" {
            writeln!(
                &mut self.output,
                "  %{} = call i64 @{}(i64 %{})",
                result, intrinsic, val
            )?;
        } else {
            // clz and ctz have a second parameter: is_poison_on_zero (false)
            writeln!(
                &mut self.output,
                "  %{} = call i64 @{}(i64 %{}, i1 false)",
                result, intrinsic, val
            )?;
        }

        // Store result (discriminant stays 0 for Int)
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result, slot1_ptr
        )?;

        // SP unchanged
        Ok(Some(stack_var.to_string()))
    }

    /// Generate inline code for `while` loop: [cond] [body] while
    ///
    /// LLVM structure:
    /// ```text
    /// while_cond:
    ///   <execute cond_body>
    ///   %cond = load condition from stack
    ///   %sp = pop condition
    ///   br i1 %cond, label %while_body, label %while_end
    /// while_body:
    ///   <execute loop_body>
    ///   br label %while_cond
    /// while_end:
    ///   ...
    /// ```
    pub(in crate::codegen) fn codegen_inline_while(
        &mut self,
        stack_var: &str,
        cond_body: &[Statement],
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        // Issue #264: Spill virtual stack before loop to ensure values are in memory.
        let spilled_stack = self.spill_virtual_stack(stack_var)?;

        let preloop_block = self.fresh_block("while_preloop");
        let cond_block = self.fresh_block("while_cond");
        let body_block = self.fresh_block("while_body");
        let end_block = self.fresh_block("while_end");

        // Use named variables for phi nodes to avoid SSA ordering issues
        let loop_stack_phi = format!("{}_stack", cond_block);
        let loop_stack_next = format!("{}_stack_next", cond_block);

        // Jump to preloop block (needed for phi node predecessor)
        writeln!(&mut self.output, "  br label %{}", preloop_block)?;
        writeln!(&mut self.output, "{}:", preloop_block)?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for stack pointer at loop entry - use preloop_block as predecessor
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{}_end ]",
            loop_stack_phi, spilled_stack, preloop_block, loop_stack_next, body_block
        )?;

        // Execute condition body and get result
        let cond_stack = self.codegen_statements(cond_body, &loop_stack_phi, false)?;
        let (popped_stack, cond_val) = self.codegen_peek_pop_bool(&cond_stack)?;

        // Branch: continue if condition is true (ne 0)
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cond_bool, cond_val
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;
        let body_end_stack = self.codegen_statements(loop_body, &popped_stack, false)?;

        // Create landing block for phi node
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;

        // Store result for phi and loop back
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(popped_stack)
    }

    /// Generate inline code for `until` loop: [cond] [body] until
    ///
    /// Like while but executes body first, then checks condition.
    /// Continues until condition is TRUE (opposite of while).
    pub(in crate::codegen) fn codegen_inline_until(
        &mut self,
        stack_var: &str,
        cond_body: &[Statement],
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        // Issue #264: Spill virtual stack before loop to ensure values are in memory.
        let spilled_stack = self.spill_virtual_stack(stack_var)?;

        let preloop_block = self.fresh_block("until_preloop");
        let body_block = self.fresh_block("until_body");
        let cond_block = self.fresh_block("until_cond");
        let end_block = self.fresh_block("until_end");

        // Use named variables for phi nodes
        let loop_stack_phi = format!("{}_stack", body_block);
        let loop_stack_next = format!("{}_stack_next", body_block);

        // Jump to preloop block (needed for phi node predecessor)
        writeln!(&mut self.output, "  br label %{}", preloop_block)?;
        writeln!(&mut self.output, "{}:", preloop_block)?;
        writeln!(&mut self.output, "  br label %{}", body_block)?;

        // Body block with phi - use preloop_block as predecessor
        writeln!(&mut self.output, "{}:", body_block)?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{}_end ]",
            loop_stack_phi, spilled_stack, preloop_block, loop_stack_next, cond_block
        )?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack_phi, false)?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;
        let cond_stack = self.codegen_statements(cond_body, &body_end_stack, false)?;

        // Peek/pop condition and get value (Issue #215: extracted helper)
        let (popped_stack, cond_val) = self.codegen_peek_pop_bool(&cond_stack)?;

        // Create landing block for phi
        let cond_end_block = format!("{}_end", cond_block);
        writeln!(&mut self.output, "  br label %{}", cond_end_block)?;
        writeln!(&mut self.output, "{}:", cond_end_block)?;
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, popped_stack
        )?;

        // Branch: if condition is TRUE, exit; if FALSE, continue loop
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cond_bool, cond_val
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, end_block, body_block
        )?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(popped_stack)
    }

    /// Peek and pop a boolean value from stack (Issue #215: extracted helper).
    /// Returns (popped_stack, cond_val) where cond_val is the i64 condition.
    pub(in crate::codegen) fn codegen_peek_pop_bool(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String), CodeGenError> {
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let cond_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            cond_val, slot1_ptr
        )?;
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_stack, stack_var
        )?;
        Ok((popped_stack, cond_val))
    }

    /// Generate inline code for `times` loop: n [body] times
    ///
    /// Pops count from stack, executes body that many times.
    #[allow(dead_code)] // Reserved for future dynamic count support
    pub(in crate::codegen) fn codegen_inline_times(
        &mut self,
        stack_var: &str,
        loop_body: &[Statement],
    ) -> Result<String, CodeGenError> {
        let cond_block = self.fresh_block("times_cond");
        let body_block = self.fresh_block("times_body");
        let end_block = self.fresh_block("times_end");

        // Pop count from stack (Issue #215: extracted helper)
        let (count_val, init_stack) = self.codegen_times_pop_count(stack_var)?;

        // Jump to condition
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block with phi nodes (Issue #215: extracted helper)
        let (counter, loop_stack) = self.codegen_times_condition(
            &cond_block,
            &body_block,
            &end_block,
            &count_val,
            &init_stack,
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack, false)?;

        // Create landing block and loop back
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;
        writeln!(
            &mut self.output,
            "  %{}_next = sub i64 %{}, 1",
            counter, counter
        )?;
        writeln!(
            &mut self.output,
            "  %{}_body_end = getelementptr i8, ptr %{}, i64 0",
            body_block, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(loop_stack)
    }

    /// Pop count value from stack for times loop (Issue #215: extracted helper).
    /// Returns (count_val, init_stack).
    pub(in crate::codegen) fn codegen_times_pop_count(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String), CodeGenError> {
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let count_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            count_val, slot1_ptr
        )?;
        let init_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            init_stack, stack_var
        )?;
        Ok((count_val, init_stack))
    }

    /// Generate condition block with phi nodes for times loop (Issue #215: extracted helper).
    /// Returns (counter, loop_stack).
    pub(in crate::codegen) fn codegen_times_condition(
        &mut self,
        cond_block: &str,
        body_block: &str,
        end_block: &str,
        count_val: &str,
        init_stack: &str,
    ) -> Result<(String, String), CodeGenError> {
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for counter and stack
        let counter = self.fresh_temp();
        let loop_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = phi i64 [ %{}, %entry ], [ %{}_next, %{}_end ]",
            counter, count_val, counter, body_block
        )?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}_body_end, %{}_end ]",
            loop_stack, init_stack, body_block, body_block
        )?;

        // Check if counter > 0
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sgt i64 %{}, 0",
            cond_bool, counter
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        Ok((counter, loop_stack))
    }

    /// Generate inline code for `times` loop with literal count: [body] n times
    ///
    /// The count is known at compile time, so we don't need to pop it from stack.
    pub(in crate::codegen) fn codegen_inline_times_literal(
        &mut self,
        stack_var: &str,
        loop_body: &[Statement],
        count: i64,
    ) -> Result<String, CodeGenError> {
        // If count is 0 or negative, skip the loop entirely
        if count <= 0 {
            return Ok(stack_var.to_string());
        }

        // Issue #264: Spill virtual stack before loop to ensure values are in memory.
        // Without this, virtual register values get re-materialized each iteration
        // instead of using the accumulated result from the previous iteration.
        let spilled_stack = self.spill_virtual_stack(stack_var)?;

        let preloop_block = self.fresh_block("times_preloop");
        let cond_block = self.fresh_block("times_cond");
        let body_block = self.fresh_block("times_body");
        let end_block = self.fresh_block("times_end");

        // Use named variables for phi nodes to avoid SSA ordering issues
        let counter_phi = format!("{}_counter", cond_block);
        let counter_next = format!("{}_counter_next", cond_block);
        let loop_stack_phi = format!("{}_stack", cond_block);
        let loop_stack_next = format!("{}_stack_next", cond_block);

        // Jump to preloop block (needed for phi node predecessor)
        writeln!(&mut self.output, "  br label %{}", preloop_block)?;
        writeln!(&mut self.output, "{}:", preloop_block)?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // Condition block
        writeln!(&mut self.output, "{}:", cond_block)?;

        // Phi for counter and stack - use preloop_block as predecessor (not %entry)
        writeln!(
            &mut self.output,
            "  %{} = phi i64 [ {}, %{} ], [ %{}, %{}_end ]",
            counter_phi, count, preloop_block, counter_next, body_block
        )?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{}_end ]",
            loop_stack_phi, spilled_stack, preloop_block, loop_stack_next, body_block
        )?;

        // Check if counter > 0
        let cond_bool = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sgt i64 %{}, 0",
            cond_bool, counter_phi
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cond_bool, body_block, end_block
        )?;

        // Body block
        writeln!(&mut self.output, "{}:", body_block)?;

        // Execute loop body
        let body_end_stack = self.codegen_statements(loop_body, &loop_stack_phi, false)?;

        // Create landing block
        let body_end_block = format!("{}_end", body_block);
        writeln!(&mut self.output, "  br label %{}", body_end_block)?;
        writeln!(&mut self.output, "{}:", body_end_block)?;

        // Decrement counter and create stack alias for phi
        writeln!(
            &mut self.output,
            "  %{} = sub i64 %{}, 1",
            counter_next, counter_phi
        )?;
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i8, ptr %{}, i64 0",
            loop_stack_next, body_end_stack
        )?;
        writeln!(&mut self.output, "  br label %{}", cond_block)?;

        // End block
        writeln!(&mut self.output, "{}:", end_block)?;

        Ok(loop_stack_phi)
    }
}
