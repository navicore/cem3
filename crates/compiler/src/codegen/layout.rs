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

    /// Emit a GEP with a dynamic (runtime) offset in an SSA variable.
    /// Returns the temp variable name holding the resulting pointer.
    pub(super) fn emit_dynamic_stack_gep(
        &mut self,
        base: &str,
        offset_var: &str,
    ) -> Result<String, CodeGenError> {
        let tmp = self.fresh_temp();
        if self.tagged_ptr {
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 %{}",
                tmp, base, offset_var
            )?;
        } else {
            writeln!(
                &mut self.output,
                "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                tmp, base, offset_var
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
            // Arithmetic shift right preserves sign for negative integers
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
            // Assumes value fits in 63-bit signed range (-(2^62) to 2^62-1).
            // Values outside this range will silently overflow the shl.
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
    // Loading two operands from stack (binary ops pattern)
    // =========================================================================

    /// Load two integer payloads from the top two stack positions.
    /// Returns (ptr_a, val_a, val_b) where ptr_a points to where the result
    /// should be stored (position -2, consuming both operands).
    pub(super) fn emit_load_two_int_operands(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String, String), CodeGenError> {
        let ptr_b = self.emit_stack_gep(stack_var, -1)?;
        let ptr_a = self.emit_stack_gep(stack_var, -2)?;
        let val_a = self.emit_load_int_payload(&ptr_a)?;
        let val_b = self.emit_load_int_payload(&ptr_b)?;
        Ok((ptr_a, val_a, val_b))
    }

    /// Load two float operands from the top two stack positions as doubles.
    /// Returns (ptr_a, val_a_double, val_b_double) for float binary ops.
    /// Use `emit_store_float_result` with ptr_a to store the result back.
    pub(super) fn emit_load_two_float_operands(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String, String), CodeGenError> {
        let ptr_b = self.emit_stack_gep(stack_var, -1)?;
        let ptr_a = self.emit_stack_gep(stack_var, -2)?;

        if self.tagged_ptr {
            Err(CodeGenError::Logic(
                "tagged-ptr float load not yet implemented".to_string(),
            ))
        } else {
            // 40-byte mode: access slot1 for float bits
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
            Ok((ptr_a, val_a, val_b))
        }
    }

    /// Store a float result (as double) at the value position ptr_a.
    /// In 40-byte mode: computes slot1 from ptr_a and stores bits there.
    /// In tagged-ptr mode: not yet implemented (floats are heap-boxed).
    pub(super) fn emit_store_float_result(
        &mut self,
        ptr_a: &str,
        double_var: &str,
    ) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            return Err(CodeGenError::Logic(
                "tagged-ptr float store not yet implemented".to_string(),
            ));
        }
        // Compute slot1 from ptr_a (offset +8 in 40-byte mode)
        let slot1 = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1, ptr_a
        )?;
        let bits = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = bitcast double %{} to i64",
            bits, double_var
        )?;
        writeln!(&mut self.output, "  store i64 %{}, ptr %{}", bits, slot1)?;
        Ok(())
    }

    /// Load one integer payload from the top of the stack.
    /// Returns (top_ptr, val) where top_ptr points to the value for in-place update.
    pub(super) fn emit_load_top_int(
        &mut self,
        stack_var: &str,
    ) -> Result<(String, String), CodeGenError> {
        let top_ptr = self.emit_stack_gep(stack_var, -1)?;
        let val = self.emit_load_int_payload(&top_ptr)?;
        Ok((top_ptr, val))
    }

    /// Store an integer result in place at the top of stack (for unary ops).
    /// In 40-byte mode: writes to slot1 (discriminant unchanged).
    /// In tagged-ptr mode: writes tagged int directly.
    /// `top_ptr` is the pointer to the value slot, `result_var` is the untagged i64.
    pub(super) fn emit_store_int_result_in_place(
        &mut self,
        top_ptr: &str,
        result_var: &str,
    ) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            self.emit_store_int(top_ptr, result_var)?;
        } else {
            // In 40-byte mode, slot1 is at offset +8 from the Value start
            let slot1_ptr = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = getelementptr i64, ptr %{}, i64 1",
                slot1_ptr, top_ptr
            )?;
            writeln!(
                &mut self.output,
                "  store i64 %{}, ptr %{}",
                result_var, slot1_ptr
            )?;
        }
        Ok(())
    }

    // =========================================================================
    // Float storage (for virtual stack spill)
    // =========================================================================

    /// Store a float value (as i64 bits) at the given stack pointer.
    /// In 40-byte mode: writes discriminant 1 to slot0, bits to slot1.
    /// In tagged-ptr mode: writes the bits as a heap-boxed Value via runtime call.
    /// `bits_var` is an i64 holding the f64 bit pattern.
    ///
    /// Note: In tagged-ptr mode, floats are heap-allocated. For the spill path
    /// we store the raw bits and let the runtime box them. This is a placeholder —
    /// the spill path for tagged-ptr floats will need a runtime helper call.
    pub(super) fn emit_store_float_bits(
        &mut self,
        value_ptr: &str,
        bits_var: &str,
    ) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            return Err(CodeGenError::Logic(
                "tagged-ptr float spill not yet implemented".to_string(),
            ));
        }
        // Write discriminant 1 (Float) to slot0
        writeln!(&mut self.output, "  store i64 1, ptr %{}", value_ptr)?;
        // Write bits to slot1 (offset +8)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, value_ptr
        )?;
        writeln!(
            &mut self.output,
            "  store i64 %{}, ptr %{}",
            bits_var, slot1_ptr
        )?;
        Ok(())
    }

    // =========================================================================
    // Full value load/store (Pattern 4 — for swap, rot, tuck, etc.)
    // =========================================================================

    /// Load a full opaque value from a stack pointer.
    /// Returns the temp variable holding the value.
    /// In 40-byte mode: `load %Value`. In tagged-ptr mode: `load i64`.
    pub(super) fn emit_load_value(&mut self, ptr: &str) -> Result<String, CodeGenError> {
        let val = self.fresh_temp();
        if self.tagged_ptr {
            writeln!(&mut self.output, "  %{} = load i64, ptr %{}", val, ptr)?;
        } else {
            writeln!(&mut self.output, "  %{} = load %Value, ptr %{}", val, ptr)?;
        }
        Ok(val)
    }

    /// Store a full opaque value to a stack pointer.
    /// In 40-byte mode: `store %Value`. In tagged-ptr mode: `store i64`.
    pub(super) fn emit_store_value(&mut self, ptr: &str, val: &str) -> Result<(), CodeGenError> {
        if self.tagged_ptr {
            writeln!(&mut self.output, "  store i64 %{}, ptr %{}", val, ptr)?;
        } else {
            writeln!(&mut self.output, "  store %Value %{}, ptr %{}", val, ptr)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn codegen_default() -> CodeGen {
        CodeGen::new()
    }

    fn codegen_tagged() -> CodeGen {
        let mut cg = CodeGen::new();
        cg.tagged_ptr = true;
        cg
    }

    #[test]
    fn test_value_type_def_default() {
        let cg = codegen_default();
        let mut ir = String::new();
        cg.emit_value_type_def(&mut ir).unwrap();
        assert!(ir.contains("{ i64, i64, i64, i64, i64 }"));
        assert!(ir.contains("40 bytes"));
    }

    #[test]
    fn test_value_type_def_tagged() {
        let cg = codegen_tagged();
        let mut ir = String::new();
        cg.emit_value_type_def(&mut ir).unwrap();
        assert!(ir.contains("%Value = type i64"));
        assert!(ir.contains("8 bytes"));
    }

    #[test]
    fn test_stack_gep_default() {
        let mut cg = codegen_default();
        let tmp = cg.emit_stack_gep("sp", -1).unwrap();
        assert!(cg.output.contains("getelementptr %Value"));
        assert!(cg.output.contains(&format!("%{}", tmp)));
    }

    #[test]
    fn test_stack_gep_tagged() {
        let mut cg = codegen_tagged();
        let tmp = cg.emit_stack_gep("sp", -1).unwrap();
        assert!(cg.output.contains("getelementptr i64"));
        assert!(!cg.output.contains("%Value"));
        assert!(cg.output.contains(&format!("%{}", tmp)));
    }

    #[test]
    fn test_load_int_payload_default() {
        let mut cg = codegen_default();
        let val = cg.emit_load_int_payload("ptr_a").unwrap();
        // Should GEP to slot1 then load
        assert!(cg.output.contains("getelementptr i64, ptr %ptr_a, i64 1"));
        assert!(cg.output.contains("load i64, ptr %"));
        assert!(!val.is_empty());
    }

    #[test]
    fn test_load_int_payload_tagged() {
        let mut cg = codegen_tagged();
        let val = cg.emit_load_int_payload("ptr_a").unwrap();
        // Should load then ashr
        assert!(cg.output.contains("load i64, ptr %ptr_a"));
        assert!(cg.output.contains("ashr i64"));
        assert!(!val.is_empty());
    }

    #[test]
    fn test_store_int_default() {
        let mut cg = codegen_default();
        cg.emit_store_int("ptr_a", "val").unwrap();
        // Should store discriminant 0, then GEP to slot1, then store value
        assert!(cg.output.contains("store i64 0, ptr %ptr_a"));
        assert!(cg.output.contains("getelementptr i64, ptr %ptr_a, i64 1"));
        assert!(cg.output.contains("store i64 %val"));
    }

    #[test]
    fn test_store_int_tagged() {
        let mut cg = codegen_tagged();
        cg.emit_store_int("ptr_a", "val").unwrap();
        // Should shl, or, then store
        assert!(cg.output.contains("shl i64 %val, 1"));
        assert!(cg.output.contains("or i64"));
        assert!(cg.output.contains("store i64"));
        // Should NOT write a discriminant
        assert!(!cg.output.contains("store i64 0, ptr"));
    }

    #[test]
    fn test_store_bool_default() {
        let mut cg = codegen_default();
        cg.emit_store_bool("ptr_a", "bval").unwrap();
        // Should store discriminant 2
        assert!(cg.output.contains("store i64 2, ptr %ptr_a"));
        assert!(cg.output.contains("getelementptr i64, ptr %ptr_a, i64 1"));
    }

    #[test]
    fn test_store_bool_tagged() {
        let mut cg = codegen_tagged();
        cg.emit_store_bool("ptr_a", "bval").unwrap();
        // Should shl by 1 (false=0, true=2)
        assert!(cg.output.contains("shl i64 %bval, 1"));
        // Should NOT write discriminant 2
        assert!(!cg.output.contains("store i64 2"));
    }

    #[test]
    fn test_value_size_bytes() {
        assert_eq!(codegen_default().value_size_bytes(), 40);
        assert_eq!(codegen_tagged().value_size_bytes(), 8);
    }

    #[test]
    fn test_load_two_int_operands_default() {
        let mut cg = codegen_default();
        let (ptr_a, val_a, val_b) = cg.emit_load_two_int_operands("sp").unwrap();
        // Should GEP to -1 and -2 with %Value stride, then load slot1 from each
        assert!(cg.output.contains("getelementptr %Value"));
        assert!(cg.output.contains("getelementptr i64"));
        assert!(!ptr_a.is_empty());
        assert!(!val_a.is_empty());
        assert!(!val_b.is_empty());
    }

    #[test]
    fn test_load_two_int_operands_tagged() {
        let mut cg = codegen_tagged();
        let (_ptr_a, val_a, val_b) = cg.emit_load_two_int_operands("sp").unwrap();
        // Should GEP with i64 stride, load, then ashr to untag
        assert!(cg.output.contains("getelementptr i64"));
        assert!(cg.output.contains("ashr i64"));
        assert!(!cg.output.contains("%Value"));
        assert!(!val_a.is_empty());
        assert!(!val_b.is_empty());
    }

    #[test]
    fn test_load_top_int_default() {
        let mut cg = codegen_default();
        let (top_ptr, val) = cg.emit_load_top_int("sp").unwrap();
        assert!(cg.output.contains("getelementptr %Value"));
        assert!(!top_ptr.is_empty());
        assert!(!val.is_empty());
    }

    #[test]
    fn test_store_int_result_in_place_default() {
        let mut cg = codegen_default();
        cg.emit_store_int_result_in_place("ptr_a", "result")
            .unwrap();
        // Should GEP to slot1 and store there (not write discriminant)
        assert!(cg.output.contains("getelementptr i64, ptr %ptr_a, i64 1"));
        assert!(cg.output.contains("store i64 %result"));
    }

    #[test]
    fn test_store_int_result_in_place_tagged() {
        let mut cg = codegen_tagged();
        cg.emit_store_int_result_in_place("ptr_a", "result")
            .unwrap();
        // Should tag (shl + or) then store
        assert!(cg.output.contains("shl i64 %result, 1"));
        assert!(cg.output.contains("or i64"));
    }

    #[test]
    fn test_load_float_operands_tagged_errors() {
        let mut cg = codegen_tagged();
        let result = cg.emit_load_two_float_operands("sp");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not yet implemented")
        );
    }

    #[test]
    fn test_store_float_bits_tagged_errors() {
        let mut cg = codegen_tagged();
        let result = cg.emit_store_float_bits("ptr", "bits");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not yet implemented")
        );
    }

    #[test]
    fn test_load_value_default() {
        let mut cg = codegen_default();
        let val = cg.emit_load_value("ptr_a").unwrap();
        assert!(cg.output.contains("load %Value, ptr %ptr_a"));
        assert!(!val.is_empty());
    }

    #[test]
    fn test_load_value_tagged() {
        let mut cg = codegen_tagged();
        let val = cg.emit_load_value("ptr_a").unwrap();
        assert!(cg.output.contains("load i64, ptr %ptr_a"));
        assert!(!cg.output.contains("%Value"));
        assert!(!val.is_empty());
    }

    #[test]
    fn test_store_value_default() {
        let mut cg = codegen_default();
        cg.emit_store_value("ptr_a", "val").unwrap();
        assert!(cg.output.contains("store %Value %val, ptr %ptr_a"));
    }

    #[test]
    fn test_store_value_tagged() {
        let mut cg = codegen_tagged();
        cg.emit_store_value("ptr_a", "val").unwrap();
        assert!(cg.output.contains("store i64 %val, ptr %ptr_a"));
        assert!(!cg.output.contains("%Value"));
    }

    #[test]
    fn test_dynamic_stack_gep_default() {
        let mut cg = codegen_default();
        let tmp = cg.emit_dynamic_stack_gep("sp", "offset").unwrap();
        assert!(
            cg.output
                .contains("getelementptr %Value, ptr %sp, i64 %offset")
        );
        assert!(!tmp.is_empty());
    }

    #[test]
    fn test_dynamic_stack_gep_tagged() {
        let mut cg = codegen_tagged();
        let tmp = cg.emit_dynamic_stack_gep("sp", "offset").unwrap();
        assert!(
            cg.output
                .contains("getelementptr i64, ptr %sp, i64 %offset")
        );
        assert!(!cg.output.contains("%Value"));
        assert!(!tmp.is_empty());
    }
}
