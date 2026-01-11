//! Inline Operation Code Generation (NaN-boxing mode)
//!
//! This module contains helper functions for generating inline LLVM IR
//! for common operations in NaN-boxing mode.
//!
//! Key differences from non-nanbox mode:
//! - %Value is i64 (8 bytes) - the entire value is NaN-boxed
//! - No slot0/slot1 layout - values are encoded in a single i64
//! - Stack operations use 8-byte offsets instead of 40-byte
//!
//! NaN-boxing encoding (from runtime/nanbox.rs):
//! - NANBOX_BASE = 0xFFF8_0000_0000_0000
//! - TAG_SHIFT = 47
//! - PAYLOAD_MASK = 0x0000_7FFF_FFFF_FFFF
//! - Formula: NANBOX_BASE | (tag << 47) | payload
//!
//! Tags: Int=0, Bool=1, String=2, Symbol=3, Variant=4, Map=5, Quotation=6, Closure=7

use super::super::{CodeGen, CodeGenError, VirtualValue};
use std::fmt::Write as _;

/// NaN-boxing constants (must match runtime/nanbox.rs)
const NANBOX_BASE: u64 = 0xFFF8_0000_0000_0000;
const TAG_SHIFT: u32 = 47;
const PAYLOAD_MASK: u64 = 0x0000_7FFF_FFFF_FFFF;

/// Tags for NaN-boxed values
#[allow(dead_code)]
const TAG_INT: u64 = 0;
const TAG_BOOL: u64 = 1;
#[allow(dead_code)]
const TAG_STRING: u64 = 2;
#[allow(dead_code)]
const TAG_SYMBOL: u64 = 3;
#[allow(dead_code)]
const TAG_VARIANT: u64 = 4;
#[allow(dead_code)]
const TAG_MAP: u64 = 5;
#[allow(dead_code)]
const TAG_QUOTATION: u64 = 6;
#[allow(dead_code)]
const TAG_CLOSURE: u64 = 7;

impl CodeGen {
    /// Encode an integer into NaN-boxed format (compile-time constant)
    #[inline]
    fn nanbox_int(n: i64) -> u64 {
        let payload = (n as u64) & PAYLOAD_MASK;
        NANBOX_BASE | (TAG_INT << TAG_SHIFT) | payload
    }

    /// Encode a boolean into NaN-boxed format (compile-time constant)
    #[inline]
    fn nanbox_bool(b: bool) -> u64 {
        let payload = if b { 1 } else { 0 };
        NANBOX_BASE | (TAG_BOOL << TAG_SHIFT) | payload
    }

    /// Generate LLVM IR to encode an i64 SSA value as NaN-boxed Int at runtime
    fn codegen_nanbox_int_runtime(&mut self, int_var: &str) -> Result<String, CodeGenError> {
        // payload = int_var & PAYLOAD_MASK
        let masked = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i64 %{}, {}",
            masked, int_var, PAYLOAD_MASK
        )?;

        // result = NANBOX_BASE | payload  (tag is 0 for Int, so no shift needed)
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i64 %{}, {}",
            result, masked, NANBOX_BASE
        )?;

        Ok(result)
    }

    /// Generate LLVM IR to encode an i1 SSA value as NaN-boxed Bool at runtime
    fn codegen_nanbox_bool_runtime(&mut self, bool_var: &str) -> Result<String, CodeGenError> {
        // Extend i1 to i64
        let extended = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            extended, bool_var
        )?;

        // result = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT) | payload
        let base_with_tag = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT);
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i64 %{}, {}",
            result, extended, base_with_tag
        )?;

        Ok(result)
    }

    /// Generate LLVM IR to decode a NaN-boxed Int to raw i64 at runtime
    /// For signed integers, we need to sign-extend from 47 bits
    fn codegen_decode_nanbox_int(&mut self, boxed_var: &str) -> Result<String, CodeGenError> {
        // Extract payload: payload = boxed & PAYLOAD_MASK
        let payload = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i64 %{}, {}",
            payload, boxed_var, PAYLOAD_MASK
        )?;

        // Sign extend from bit 46 to bit 63
        // If bit 46 is set, we need to OR with 0xFFFF_8000_0000_0000
        // signbit = (payload >> 46) & 1
        let shifted = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = lshr i64 %{}, 46",
            shifted, payload
        )?;
        let signbit = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = and i64 %{}, 1", signbit, shifted)?;

        // sign_ext = signbit * 0xFFFF_8000_0000_0000 (fills upper bits if negative)
        let sign_ext = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = mul i64 %{}, {}",
            sign_ext, signbit, 0xFFFF_8000_0000_0000_u64 as i64
        )?;

        // result = payload | sign_ext
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i64 %{}, %{}",
            result, payload, sign_ext
        )?;

        Ok(result)
    }

    /// Generate inline code for comparison operations (NaN-box mode).
    /// Returns NaN-boxed Bool.
    pub(in crate::codegen) fn codegen_inline_comparison_nanbox(
        &mut self,
        stack_var: &str,
        icmp_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Spill virtual registers - comparison returns Bool
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointers to values on stack (8-byte elements in nanbox mode)
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Load NaN-boxed values
        let boxed_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            boxed_a, ptr_a
        )?;
        let boxed_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            boxed_b, ptr_b
        )?;

        // Decode to raw integers
        let val_a = self.codegen_decode_nanbox_int(&boxed_a)?;
        let val_b = self.codegen_decode_nanbox_int(&boxed_b)?;

        // Compare
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp {} i64 %{}, %{}",
            cmp_result, icmp_op, val_a, val_b
        )?;

        // Encode result as NaN-boxed Bool
        let result_boxed = self.codegen_nanbox_bool_runtime(&cmp_result)?;

        // Store result back to stack position a
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result_boxed, ptr_a
        )?;

        // SP = SP - 1 (consumed b)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            result_var, stack_var
        )?;

        Ok(Some(result_var))
    }

    /// Generate inline code for binary arithmetic (add/subtract) - NaN-box mode.
    /// Uses virtual registers when both operands are available.
    pub(in crate::codegen) fn codegen_inline_binary_op_nanbox(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
        _adjust_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Try fast path with virtual registers
        if self.virtual_stack.len() >= 2
            && let Some(result) = self.codegen_binary_op_virtual_nanbox(stack_var, llvm_op)?
        {
            return Ok(Some(result));
        }

        // Fall back to memory path
        self.codegen_binary_op_memory_nanbox(stack_var, llvm_op)
    }

    /// Fast path: both operands in virtual registers (NaN-box mode).
    pub(in crate::codegen) fn codegen_binary_op_virtual_nanbox(
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

        // Perform the operation directly on SSA values (raw integers, not boxed)
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, ssa_a, ssa_b
        )?;

        // Push result to virtual stack (remains as raw integer)
        let result = VirtualValue::Int {
            ssa_var: op_result,
            value: 0,
        };
        Ok(Some(self.push_virtual(result, stack_var)?))
    }

    /// Slow path: spill virtual stack and operate on memory (NaN-box mode).
    pub(in crate::codegen) fn codegen_binary_op_memory_nanbox(
        &mut self,
        stack_var: &str,
        llvm_op: &str,
    ) -> Result<Option<String>, CodeGenError> {
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointers to values (8-byte elements in nanbox mode)
        let ptr_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            ptr_b, stack_var
        )?;
        let ptr_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -2",
            ptr_a, stack_var
        )?;

        // Load NaN-boxed values
        let boxed_a = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            boxed_a, ptr_a
        )?;
        let boxed_b = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            boxed_b, ptr_b
        )?;

        // Decode to raw integers
        let val_a = self.codegen_decode_nanbox_int(&boxed_a)?;
        let val_b = self.codegen_decode_nanbox_int(&boxed_b)?;

        // Perform operation
        let op_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            op_result, llvm_op, val_a, val_b
        )?;

        // Encode result as NaN-boxed Int
        let result_boxed = self.codegen_nanbox_int_runtime(&op_result)?;

        // Store result back
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

    /// Generate inline code for unary negation (NaN-box mode).
    pub(in crate::codegen) fn codegen_inline_negate_nanbox(
        &mut self,
        stack_var: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Try virtual register path first
        if let Some(top) = self.virtual_stack.pop() {
            if let VirtualValue::Int { ssa_var, .. } = top {
                // Negate directly
                let neg_result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_result, ssa_var
                )?;

                // Push result back
                let result = VirtualValue::Int {
                    ssa_var: neg_result,
                    value: 0,
                };
                return Ok(Some(self.push_virtual(result, stack_var)?));
            } else {
                // Not an integer - restore and fall through
                self.virtual_stack.push(top);
            }
        }

        // Memory path
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointer to top value (8-byte element)
        let ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            ptr, stack_var
        )?;

        // Load and decode
        let boxed = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = load i64, ptr %{}", boxed, ptr)?;
        let val = self.codegen_decode_nanbox_int(&boxed)?;

        // Negate
        let neg_result = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = sub i64 0, %{}", neg_result, val)?;

        // Encode and store
        let result_boxed = self.codegen_nanbox_int_runtime(&neg_result)?;
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result_boxed, ptr
        )?;

        // Stack pointer unchanged
        Ok(Some(stack_var.to_string()))
    }

    /// Generate inline code for boolean not (NaN-box mode).
    pub(in crate::codegen) fn codegen_inline_not_nanbox(
        &mut self,
        stack_var: &str,
    ) -> Result<Option<String>, CodeGenError> {
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Get pointer to top value
        let ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 -1",
            ptr, stack_var
        )?;

        // Load NaN-boxed Bool
        let boxed = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = load i64, ptr %{}", boxed, ptr)?;

        // Extract payload (0 or 1)
        let payload = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = and i64 %{}, 1", payload, boxed)?;

        // Flip: new_payload = 1 - payload
        let flipped = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = sub i64 1, %{}", flipped, payload)?;

        // Encode as Bool: NANBOX_BASE | (TAG_BOOL << TAG_SHIFT) | flipped
        let base_with_tag = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT);
        let result_boxed = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i64 %{}, {}",
            result_boxed, flipped, base_with_tag
        )?;

        // Store back
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            result_boxed, ptr
        )?;

        Ok(Some(stack_var.to_string()))
    }

    /// Generate code to create a NaN-boxed Int constant on the virtual stack
    pub(in crate::codegen) fn codegen_push_int_nanbox(
        &mut self,
        value: i64,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // Create SSA variable with the raw integer value
        let ssa_var = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = add i64 0, {}", ssa_var, value)?;

        // Push to virtual stack as raw integer (will be boxed when spilled)
        let vv = VirtualValue::Int { ssa_var, value };
        self.push_virtual(vv, stack_var)
    }

    /// Generate code to create a NaN-boxed Bool constant on the virtual stack
    pub(in crate::codegen) fn codegen_push_bool_nanbox(
        &mut self,
        value: bool,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // For bools, we store the boxed value directly
        let boxed = Self::nanbox_bool(value);
        let ssa_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = add i64 0, {}",
            ssa_var, boxed as i64
        )?;

        // Push to virtual stack
        let vv = VirtualValue::Bool { ssa_var };
        self.push_virtual(vv, stack_var)
    }

    // Note: Loop codegen (times, while, until) falls back to runtime calls in nanbox mode.
    // Optimized inline loop codegen can be added in a future phase.
}
