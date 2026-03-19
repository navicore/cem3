//! Inline Operation Code Generation
//!
//! This module contains helper functions for generating inline LLVM IR
//! for common operations like comparisons, arithmetic, and loops.
//! These are called by try_codegen_inline_op in the main module.
//!
//! Layout-dependent operations use helpers from `layout.rs` to support
//! both 40-byte StackValue and 8-byte tagged pointer representations.

use super::super::{CodeGen, CodeGenError, VirtualValue};
use std::fmt::Write as _;

impl CodeGen {
    /// Generate inline code for comparison operations.
    /// Result is a Bool value at position -2, consuming both operands.
    pub(in crate::codegen) fn codegen_inline_comparison(
        &mut self,
        stack_var: &str,
        icmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189) - comparison returns Bool, not Int
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load two integer operands
        let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

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

        // Store result as Bool
        self.emit_store_bool(&ptr_a, &zext)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

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

        // Load two integer operands
        let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

        // Perform the operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result in place at ptr_a
        self.emit_store_int_result_in_place(&ptr_a, &op_result)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

        Ok(Some(result_var))
    }

    /// Generate inline code for float binary operations (f.add, f.subtract, etc.)
    pub(in crate::codegen) fn codegen_inline_float_binary_op(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load operands as doubles
        let (_ptr_a, slot1_a, val_a, val_b) = self.emit_load_two_float_operands(stack_var)?;

        // Perform the float operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} double %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result
        self.emit_store_float_result(&slot1_a, &op_result)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

        Ok(Some(result_var))
    }

    /// Generate inline code for float comparison operations.
    /// Result is a Bool value.
    pub(in crate::codegen) fn codegen_inline_float_comparison(
        &mut self,
        stack_var: &str,
        fcmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load operands as doubles
        let (ptr_a, _slot1_a, val_a, val_b) = self.emit_load_two_float_operands(stack_var)?;

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

        // Store result as Bool
        self.emit_store_bool(&ptr_a, &zext)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

        Ok(Some(result_var))
    }

    /// Generate inline code for integer bitwise binary operations.
    pub(in crate::codegen) fn codegen_inline_int_bitwise_binary(
        &mut self,
        stack_var: &str,
        llvm_op: &str, // "and", "or", "xor"
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Load two integer operands
        let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

        // Perform the bitwise operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Store result in place
        self.emit_store_int_result_in_place(&ptr_a, &op_result)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

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

        // Load operands from memory
        let (ptr_a, val_a, val_b) = self.emit_load_two_int_operands(stack_var)?;

        // Perform bounds-checked shift
        let op_result = self.codegen_shift_compute(&val_a, &val_b, is_left)?;

        // Store result in place
        self.emit_store_int_result_in_place(&ptr_a, &op_result)?;

        // SP = SP - 1 (consumed b)
        let result_var = self.emit_stack_gep(stack_var, -1)?;

        Ok(Some(result_var))
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

        // Load top value
        let (top_ptr, val) = self.emit_load_top_int(stack_var)?;

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

        // Store result in place
        self.emit_store_int_result_in_place(&top_ptr, &result)?;

        // SP unchanged
        Ok(Some(stack_var.to_string()))
    }
}
