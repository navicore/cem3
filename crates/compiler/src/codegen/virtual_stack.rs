//! Virtual Stack Management
//!
//! This module handles the virtual register stack for optimizing stack operations.
//! Values are kept in SSA variables instead of memory when possible.

use super::{CodeGen, CodeGenError, MAX_VIRTUAL_STACK, VirtualValue};
use std::fmt::Write as _;

impl CodeGen {
    /// Generate a fresh temporary variable name
    pub(super) fn fresh_temp(&mut self) -> String {
        let name = format!("{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    /// Generate a fresh block label
    pub(super) fn fresh_block(&mut self, prefix: &str) -> String {
        let name = format!("{}{}", prefix, self.block_counter);
        self.block_counter += 1;
        name
    }

    /// Spill all virtual register values to memory (Issue #189).
    ///
    /// This must be called before:
    /// - Function/word calls (callee expects values in memory)
    /// - Control flow points (branches need consistent memory state)
    /// - Operations that access values deeper than virtual stack
    ///
    /// Returns the new stack pointer after spilling all values.
    pub(super) fn spill_virtual_stack(&mut self, stack_var: &str) -> Result<String, CodeGenError> {
        if self.virtual_stack.is_empty() {
            return Ok(stack_var.to_string());
        }

        let mut current_sp = stack_var.to_string();

        // Spill each value to memory (oldest first, so they're in correct order)
        for value in std::mem::take(&mut self.virtual_stack) {
            match &value {
                VirtualValue::Int { ssa_var, .. } => {
                    self.emit_store_int(&current_sp, ssa_var)?;
                }
                VirtualValue::Bool { ssa_var } => {
                    self.emit_store_bool(&current_sp, ssa_var)?;
                }
                VirtualValue::Float { ssa_var } => {
                    // Convert double to i64 bits for storage
                    let bits = self.fresh_temp();
                    writeln!(
                        &mut self.output,
                        "  %{} = bitcast double %{} to i64",
                        bits, ssa_var
                    )?;
                    self.emit_store_float_bits(&current_sp, &bits)?;
                }
            }

            // Advance stack pointer to next Value slot
            let next_sp = self.emit_stack_gep(&current_sp, 1)?;
            current_sp = next_sp;
        }

        Ok(current_sp)
    }

    /// Push a value to the virtual stack, spilling if at capacity.
    ///
    /// Returns the new memory stack pointer (unchanged if value stays virtual,
    /// advanced if we had to spill).
    pub(super) fn push_virtual(
        &mut self,
        value: VirtualValue,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // If at capacity, spill all to memory first
        if self.virtual_stack.len() >= MAX_VIRTUAL_STACK {
            let new_sp = self.spill_virtual_stack(stack_var)?;
            self.virtual_stack.push(value);
            Ok(new_sp)
        } else {
            self.virtual_stack.push(value);
            Ok(stack_var.to_string())
        }
    }
}
