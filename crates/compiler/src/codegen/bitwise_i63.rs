//! IR helpers for emitting 63-bit-aware bitwise operations.
//!
//! Seq's `Int` is a signed 63-bit integer (the low bit of the tagged
//! stack slot is the type tag). The compiler inlines `shl`/`shr`/
//! `popcount`/`clz`/`ctz` as LLVM IR through two paths — `inline/ops.rs`
//! (the standard tagged-stack convention) and `specialization/codegen_ops.rs`
//! (the register-based fast path). Both paths apply identical 63-bit
//! adjustments, so the IR shapes live here as shared helpers.
//!
//! Keeping the constants and the IR templates in one place means the two
//! codegen paths can never drift if the encoding changes; see
//! `docs/design/TAGGED_INT_BITWISE.md` for the contract.

use super::CodeGen;
use super::error::CodeGenError;
use std::fmt::Write;

/// Minimum value representable in a 63-bit signed integer (-2^62) as an
/// IR literal.
pub(in crate::codegen) const I63_MIN_LIT: &str = "-4611686018427387904";
/// Maximum value representable in a 63-bit signed integer (2^62 - 1) as
/// an IR literal.
pub(in crate::codegen) const I63_MAX_LIT: &str = "4611686018427387903";
/// Mask covering the 63 value-bits of a Seq Int — every bit except the
/// i64 sign-extension bit at position 63.
pub(in crate::codegen) const I63_MASK_LIT: &str = "9223372036854775807";

impl CodeGen {
    /// Clamp `val` to the 63-bit signed Int range, returning the
    /// unmodified value when it fits and `0` otherwise. Used to keep
    /// `shl`/`shr` honest against the tagged-pointer encoding: any
    /// out-of-range result would silently lose bit 62 when retagged.
    pub(in crate::codegen) fn emit_clamp_to_i63(
        &mut self,
        val: &str,
    ) -> Result<String, CodeGenError> {
        let fits_min = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, {}",
            fits_min, val, I63_MIN_LIT
        )?;
        let fits_max = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sle i64 %{}, {}",
            fits_max, val, I63_MAX_LIT
        )?;
        let fits = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i1 %{}, %{}",
            fits, fits_min, fits_max
        )?;
        let clamped = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 %{}, i64 0",
            clamped, fits, val
        )?;
        Ok(clamped)
    }

    /// Population count over the 63 value-bits of `val`. The i64
    /// sign-extension bit (position 63) is masked off before counting,
    /// so `popcount(-1) = 63`.
    pub(in crate::codegen) fn emit_popcount_i63(
        &mut self,
        val: &str,
    ) -> Result<String, CodeGenError> {
        let masked = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i64 %{}, {}",
            masked, val, I63_MASK_LIT
        )?;
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @llvm.ctpop.i64(i64 %{})",
            result, masked
        )?;
        Ok(result)
    }

    /// Count leading zeros relative to the 63-bit Int width. The raw
    /// `llvm.ctlz.i64` count is one larger than the 63-bit count for any
    /// non-negative value (and also one larger for `0`); for negatives
    /// it is already `0`. Saturating `raw - 1` collapses both cases:
    /// `clz(0) = 63`, `clz(1) = 62`, `clz(-1) = 0`.
    pub(in crate::codegen) fn emit_clz_i63(&mut self, val: &str) -> Result<String, CodeGenError> {
        let raw = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @llvm.ctlz.i64(i64 %{}, i1 false)",
            raw, val
        )?;
        let raw_is_zero = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 0",
            raw_is_zero, raw
        )?;
        let minus_one = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = sub i64 %{}, 1", minus_one, raw)?;
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            result, raw_is_zero, minus_one
        )?;
        Ok(result)
    }

    /// Count trailing zeros relative to the 63-bit Int width. Any
    /// non-zero 63-bit value has at least one set bit at position ≤ 62,
    /// so `llvm.cttz.i64` already returns a value in `[0, 62]`; only
    /// `v == 0` needs special handling to yield 63 instead of 64.
    pub(in crate::codegen) fn emit_ctz_i63(&mut self, val: &str) -> Result<String, CodeGenError> {
        let raw = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @llvm.cttz.i64(i64 %{}, i1 false)",
            raw, val
        )?;
        let val_is_zero = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 0",
            val_is_zero, val
        )?;
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 63, i64 %{}",
            result, val_is_zero, raw
        )?;
        Ok(result)
    }
}
