//! Stack Value Layout Helpers
//!
//! Abstracts the differences between 40-byte StackValue (default) and
//! 8-byte tagged pointer (tagged-ptr feature) for LLVM IR generation.
//!
//! ## 40-byte layout (default)
//!
//! ```text
//! %Value = type { i64, i64, i64, i64, i64 }
//! - slot0: discriminant (0=Int, 1=Float, 2=Bool, ...)
//! - slot1: primary payload
//! - slot2-4: additional payload
//! - GEP stride: 40 bytes per %Value
//! ```
//!
//! ## 8-byte tagged pointer layout (tagged-ptr)
//!
//! ```text
//! %Value = type i64
//! - Odd values: Int (value << 1 | 1)
//! - 0x0: Bool false
//! - 0x2: Bool true
//! - Even > 2: Heap pointer to Box<Value>
//! - GEP stride: 8 bytes per i64
//! ```

use super::{CodeGen, CodeGenError};
use std::fmt::Write as _;

// These helpers are WIP — they'll be wired in as we migrate each codegen pattern.
#[allow(dead_code)]
impl CodeGen {
    // =========================================================================
    // Type definition
    // =========================================================================

    /// Emit the %Value type definition.
    pub(super) fn emit_value_type_def(&self, ir: &mut String) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            writeln!(ir, "; Value type (tagged pointer - 8 bytes)")?;
            writeln!(ir, "%Value = type i64")?;
        } else {
            writeln!(ir, "; Value type (Rust enum - 40 bytes)")?;
            writeln!(ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        }
        writeln!(ir)?;
        Ok(())
    }

    // =========================================================================
    // Stack pointer arithmetic (Pattern 1)
    // =========================================================================

    /// Emit a GEP to offset the stack pointer by N value slots.
    /// Returns the temp variable name holding the resulting pointer.
    pub(super) fn emit_stack_gep(
        &mut self,
        base: &str,
        offset: i64,
    ) -> Result<String, CodeGenError> {
        let tmp = self.fresh_temp();
        if self.tagged_ptr {
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 {}",
                tmp, base, offset
            )?;
        } else {
            writeln!(
                &mut self.output,
                "  %{} = getelementptr %Value, ptr %{}, i64 {}",
                tmp, base, offset
            )?;
        }
        Ok(tmp)
    }

    // =========================================================================
    // Value slot access (Patterns 3, 6)
    // =========================================================================

    /// Load the integer payload from a value at the given stack pointer.
    /// In 40-byte mode: loads from slot1 (offset +8).
    /// In tagged-ptr mode: loads the tagged i64 and extracts via arithmetic shift.
    /// Returns the temp variable name holding the untagged i64 value.
    pub(super) fn emit_load_int_payload(
        &mut self,
        value_ptr: &str,
    ) -> Result<String, CodeGenError> {
        if self.tagged_ptr {
            let tagged = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = load i64, ptr %{}",
                tagged, value_ptr
            )?;
            let val = self.fresh_temp();
            writeln!(&mut self.output, "  %{} = ashr i64 %{}, 1", val, tagged)?;
            Ok(val)
        } else {
            let slot1_ptr = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 1",
                slot1_ptr, value_ptr
            )?;
            let val = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = load i64, ptr %{}",
                val, slot1_ptr
            )?;
            Ok(val)
        }
    }

    /// Store an integer value at the given stack pointer.
    /// In 40-byte mode: writes discriminant 0 to slot0, value to slot1.
    /// In tagged-ptr mode: writes tagged integer (value << 1 | 1).
    pub(super) fn emit_store_int(
        &mut self,
        value_ptr: &str,
        int_var: &str,
    ) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            let shifted = self.fresh_temp();
            writeln!(&mut self.output, "  %{} = shl i64 %{}, 1", shifted, int_var)?;
            let tagged = self.fresh_temp();
            writeln!(&mut self.output, "  %{} = or i64 %{}, 1", tagged, shifted)?;
            writeln!(
                &mut self.output,
                "  store i64 %{}, ptr %{}",
                tagged, value_ptr
            )?;
        } else {
            // Write discriminant 0 (Int) to slot0
            writeln!(&mut self.output, "  store i64 0, ptr %{}", value_ptr)?;
            // Write value to slot1 (offset +8)
            let slot1_ptr = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 1",
                slot1_ptr, value_ptr
            )?;
            writeln!(
                &mut self.output,
                "  store i64 %{}, ptr %{}",
                int_var, slot1_ptr
            )?;
        }
        Ok(())
    }

    /// Store a boolean result at the given stack pointer.
    /// In 40-byte mode: writes discriminant 2 to slot0, 0/1 to slot1.
    /// In tagged-ptr mode: writes 0 (false) or 2 (true).
    /// `bool_var` is an i64 holding 0 or 1.
    pub(super) fn emit_store_bool(
        &mut self,
        value_ptr: &str,
        bool_var: &str,
    ) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            // false = 0, true = 2 → multiply by 2
            let tagged = self.fresh_temp();
            writeln!(&mut self.output, "  %{} = shl i64 %{}, 1", tagged, bool_var)?;
            writeln!(
                &mut self.output,
                "  store i64 %{}, ptr %{}",
                tagged, value_ptr
            )?;
        } else {
            // Write discriminant 2 (Bool) to slot0
            writeln!(&mut self.output, "  store i64 2, ptr %{}", value_ptr)?;
            // Write 0/1 to slot1
            let slot1_ptr = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 1",
                slot1_ptr, value_ptr
            )?;
            writeln!(
                &mut self.output,
                "  store i64 %{}, ptr %{}",
                bool_var, slot1_ptr
            )?;
        }
        Ok(())
    }

    // =========================================================================
    // Array size calculation (Pattern 5)
    // =========================================================================

    /// Return the size of a single Value in bytes (for memmove calculations).
    pub(super) fn value_size_bytes(&self) -> u64 {
        if self.tagged_ptr { 8 } else { 40 }
    }
}
