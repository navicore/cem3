//! Safe integer division, modulo, and shift lowering for specialized
//! codegen. Division emits a branch-based zero check plus an `INT_MIN / -1`
//! wrap handler; shift emits bounds checks that match the runtime's shl/shr
//! semantics for out-of-range counts.

use super::CodeGen;
use super::context::RegisterContext;
use super::types::RegisterType;
use crate::codegen::CodeGenError;
use std::fmt::Write as _;

impl CodeGen {
    /// Emit a safe integer division or modulo with overflow protection.
    ///
    /// Returns ( Int Int -- Int Bool ) where Bool indicates success.
    /// Division by zero returns (0, false).
    /// INT_MIN / -1 uses wrapping semantics (returns INT_MIN, true) to match runtime.
    ///
    /// Note: LLVM's sdiv has undefined behavior for INT_MIN / -1, so we must
    /// handle it explicitly. We match the runtime's wrapping_div behavior.
    pub(super) fn emit_specialized_safe_div(
        &mut self,
        ctx: &mut RegisterContext,
        op: &str, // "sdiv" or "srem"
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap(); // divisor
        let (a, _) = ctx.pop().unwrap(); // dividend

        // Check if divisor is zero
        let is_zero = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = icmp eq i64 %{}, 0", is_zero, b)?;

        // For sdiv: also check for INT_MIN / -1 overflow case.
        // We handle this specially to return INT_MIN (wrapping behavior).
        let (check_overflow, is_overflow) = if op == "sdiv" {
            let is_int_min = self.fresh_temp();
            let is_neg_one = self.fresh_temp();
            let is_overflow = self.fresh_temp();

            writeln!(
                &mut self.output,
                "  %{} = icmp eq i64 %{}, -9223372036854775808",
                is_int_min, a
            )?;
            writeln!(
                &mut self.output,
                "  %{} = icmp eq i64 %{}, -1",
                is_neg_one, b
            )?;
            writeln!(
                &mut self.output,
                "  %{} = and i1 %{}, %{}",
                is_overflow, is_int_min, is_neg_one
            )?;
            (true, is_overflow)
        } else {
            (false, String::new())
        };

        let ok_label = self.fresh_block("div_ok");
        let fail_label = self.fresh_block("div_fail");
        let merge_label = self.fresh_block("div_merge");
        let overflow_label = if check_overflow {
            self.fresh_block("div_overflow")
        } else {
            String::new()
        };

        // First check: division by zero
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            is_zero,
            fail_label,
            if check_overflow {
                &overflow_label
            } else {
                &ok_label
            }
        )?;

        // For sdiv: check overflow case (INT_MIN / -1)
        if check_overflow {
            writeln!(&mut self.output, "{}:", overflow_label)?;
            writeln!(
                &mut self.output,
                "  br i1 %{}, label %{}, label %{}",
                is_overflow, merge_label, ok_label
            )?;
        }

        // Success branch: perform the division
        writeln!(&mut self.output, "{}:", ok_label)?;
        let ok_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            ok_result, op, a, b
        )?;
        writeln!(&mut self.output, "  br label %{}", merge_label)?;

        // Failure branch: return 0 and false
        writeln!(&mut self.output, "{}:", fail_label)?;
        writeln!(&mut self.output, "  br label %{}", merge_label)?;

        // Merge block with phi nodes
        writeln!(&mut self.output, "{}:", merge_label)?;
        let result_phi = self.fresh_temp();
        let success_phi = self.fresh_temp();

        if check_overflow {
            // For sdiv: three incoming edges (ok, fail, overflow).
            // Overflow returns INT_MIN (wrapping behavior) with success=true.
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ %{}, %{} ], [ 0, %{} ], [ -9223372036854775808, %{} ]",
                result_phi, ok_result, ok_label, fail_label, overflow_label
            )?;
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ 1, %{} ], [ 0, %{} ], [ 1, %{} ]",
                success_phi, ok_label, fail_label, overflow_label
            )?;
        } else {
            // For srem: two incoming edges (ok, fail)
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ %{}, %{} ], [ 0, %{} ]",
                result_phi, ok_result, ok_label, fail_label
            )?;
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ 1, %{} ], [ 0, %{} ]",
                success_phi, ok_label, fail_label
            )?;
        }

        // Push result and success flag to context
        // Stack order: result first (deeper), then success (top)
        ctx.push(result_phi, RegisterType::I64);
        ctx.push(success_phi, RegisterType::I64);

        Ok(())
    }

    /// Emit a safe shift operation with bounds checking.
    ///
    /// Returns 0 for negative shift or shift >= 64, otherwise performs the shift.
    /// Matches runtime behavior for shl/shr.
    pub(super) fn emit_specialized_safe_shift(
        &mut self,
        ctx: &mut RegisterContext,
        is_left: bool, // true for shl, false for shr
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap(); // shift count
        let (a, _) = ctx.pop().unwrap(); // value to shift

        let is_negative = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp slt i64 %{}, 0",
            is_negative, b
        )?;

        let is_too_large = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, 64",
            is_too_large, b
        )?;

        // Combine: invalid if negative OR >= 64
        let is_invalid = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i1 %{}, %{}",
            is_invalid, is_negative, is_too_large
        )?;

        // Use a safe shift count (0 if invalid) to avoid LLVM undefined behavior
        let safe_count = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            safe_count, is_invalid, b
        )?;

        // Perform the shift with safe count
        let shift_result = self.fresh_temp();
        let op = if is_left { "shl" } else { "lshr" };
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            shift_result, op, a, safe_count
        )?;

        // Select intermediate result: 0 if invalid, otherwise shift_result.
        let intermediate = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            intermediate, is_invalid, shift_result
        )?;

        // 63-bit clamp: any result outside [I63_MIN, I63_MAX] would lose
        // bit 62 when retagged into the tagged stack slot. Map out-of-range
        // to 0. (0 is in range, so the existing invalid->0 mapping is
        // preserved.)
        let fits_min = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, -4611686018427387904",
            fits_min, intermediate
        )?;
        let fits_max = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sle i64 %{}, 4611686018427387903",
            fits_max, intermediate
        )?;
        let fits = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i1 %{}, %{}",
            fits, fits_min, fits_max
        )?;
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 %{}, i64 0",
            result, fits, intermediate
        )?;

        ctx.push(result, RegisterType::I64);
        Ok(())
    }
}
