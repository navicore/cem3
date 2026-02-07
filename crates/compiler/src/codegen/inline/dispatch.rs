//! Inline Operation Dispatch
//!
//! This module contains the main `try_codegen_inline_op` function that dispatches
//! to appropriate inline implementations for stack, arithmetic, and other operations.

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
                // Spill virtual registers before runtime call (Issue #189)
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
            // For trivially-copyable types (Int, Float, Bool): direct load/store
            // For heap types (String, etc.): call clone_value runtime
            "dup" => {
                // Spill virtual registers (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let top_ptr = self.fresh_temp();

                // Get pointer to top value (sp - 1)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;

                // Optimization: use fast path if we know top is trivially copyable
                // Either from type map (Issue #186) or previous literal (Issue #195)
                let use_fast_path = self.prev_stmt_is_trivial_literal
                    || self.is_trivially_copyable_at_current_stmt();

                if use_fast_path {
                    // Optimized path: load/store 40-byte Value struct directly
                    // No runtime call needed for Int, Float, Bool (no heap references)
                    let val = self.fresh_temp();
                    writeln!(
                        &mut self.output,
                        "  %{} = load %Value, ptr %{}",
                        val, top_ptr
                    )?;
                    writeln!(
                        &mut self.output,
                        "  store %Value %{}, ptr %{}",
                        val, stack_var
                    )?;
                } else {
                    // General path: call clone_value for heap types (String, etc.)
                    writeln!(
                        &mut self.output,
                        "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                        top_ptr, stack_var
                    )?;
                }

                // Increment SP (allocate result_var after the branch to maintain SSA order)
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // swap: ( a b -- b a )
            "swap" => {
                // Spill virtual registers (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();

                // Get pointers to a and b
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Load full Values (40 bytes each)
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                // Store swapped
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_b
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // over: ( a b -- a b a )
            // Uses patch_seq_clone_value to properly clone heap values
            "over" => {
                // Spill virtual registers before runtime call (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_a = self.fresh_temp();
                let result_var = self.fresh_temp();

                // Get pointer to a (sp - 2)
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Clone the value from ptr_a to stack_var (current SP)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Increment SP
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // i.add / i.+: ( a b -- a+b )
            "i.add" | "i.+" => self.codegen_inline_binary_op(stack_var, "add", "sub"),

            // i.subtract / i.-: ( a b -- a-b )
            "i.subtract" | "i.-" => self.codegen_inline_binary_op(stack_var, "sub", "add"),

            // i.multiply / i.*: ( a b -- a*b )
            // Issue #189: Uses virtual registers via codegen_inline_binary_op
            "i.multiply" | "i.*" => self.codegen_inline_binary_op(stack_var, "mul", "div"),

            // i.divide / i./ and i.modulo / i.%: ( a b -- result success )
            // These now return a Bool success flag, so we use the runtime functions
            // which properly handle the two-value return.

            // Integer comparison operations - result is tagged bool (0=false, 1=true)
            "i.=" | "i.eq" => self.codegen_inline_comparison(stack_var, "eq"),
            "i.<>" | "i.neq" => self.codegen_inline_comparison(stack_var, "ne"),
            "i.<" | "i.lt" => self.codegen_inline_comparison(stack_var, "slt"),
            "i.>" | "i.gt" => self.codegen_inline_comparison(stack_var, "sgt"),
            "i.<=" | "i.lte" => self.codegen_inline_comparison(stack_var, "sle"),
            "i.>=" | "i.gte" => self.codegen_inline_comparison(stack_var, "sge"),

            // Float arithmetic operations
            // Values are stored as f64 bits in slot1, discriminant 1 (Float)
            "f.add" | "f.+" => self.codegen_inline_float_binary_op(stack_var, "fadd"),
            "f.subtract" | "f.-" => self.codegen_inline_float_binary_op(stack_var, "fsub"),
            "f.multiply" | "f.*" => self.codegen_inline_float_binary_op(stack_var, "fmul"),
            "f.divide" | "f./" => self.codegen_inline_float_binary_op(stack_var, "fdiv"),

            // Float comparison operations - result is tagged bool
            "f.=" | "f.eq" => self.codegen_inline_float_comparison(stack_var, "oeq"),
            "f.<>" | "f.neq" => self.codegen_inline_float_comparison(stack_var, "one"),
            "f.<" | "f.lt" => self.codegen_inline_float_comparison(stack_var, "olt"),
            "f.>" | "f.gt" => self.codegen_inline_float_comparison(stack_var, "ogt"),
            "f.<=" | "f.lte" => self.codegen_inline_float_comparison(stack_var, "ole"),
            "f.>=" | "f.gte" => self.codegen_inline_float_comparison(stack_var, "oge"),

            // Boolean operations - values are in slot1, discriminant 2 (Bool)
            // and: ( a b -- a&&b )
            "and" => {
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

                // AND the values and convert to 0 or 1
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

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // or: ( a b -- a||b )
            "or" => {
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

                // OR the values and convert to 0 or 1
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

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", ptr_a)?;
                writeln!(&mut self.output, "  store i64 %{}, ptr %{}", zext, slot1_a)?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // not: ( a -- !a )
            "not" => {
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

                // not: if val == 0, result is 1; else result is 0
                let is_zero = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = icmp eq i64 %{}, 0", is_zero, val)?;
                let zext = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = zext i1 %{} to i64",
                    zext, is_zero
                )?;

                // Store result as Value::Bool (discriminant 2 at slot0, value at slot1)
                writeln!(&mut self.output, "  store i64 2, ptr %{}", top_ptr)?;
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    zext, slot1_ptr
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // Bitwise operations - operate on Int values (discriminant 0)
            // band: ( a b -- a&b ) bitwise AND
            "band" => self.codegen_inline_int_bitwise_binary(stack_var, "and"),

            // bor: ( a b -- a|b ) bitwise OR
            "bor" => self.codegen_inline_int_bitwise_binary(stack_var, "or"),

            // bxor: ( a b -- a^b ) bitwise XOR
            "bxor" => self.codegen_inline_int_bitwise_binary(stack_var, "xor"),

            // shl: ( a b -- a<<b ) shift left, returns 0 for shift >= 64 or negative
            "shl" => self.codegen_inline_shift(stack_var, true),

            // shr: ( a b -- a>>b ) logical shift right, returns 0 for shift >= 64 or negative
            "shr" => self.codegen_inline_shift(stack_var, false),

            // bnot: ( a -- ~a ) bitwise NOT
            "bnot" => {
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

                // Bitwise NOT: XOR with -1 (all 1s)
                let not_result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, -1", not_result, val)?;

                // Store result (discriminant stays 0 for Int)
                writeln!(
                    &mut self.output,
                    "  store i64 %{}, ptr %{}",
                    not_result, slot1_ptr
                )?;
                // SP unchanged
                Ok(Some(stack_var.to_string()))
            }

            // popcount: ( a -- count ) count number of 1 bits
            "popcount" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctpop.i64"),

            // clz: ( a -- count ) count leading zeros
            "clz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.ctlz.i64"),

            // ctz: ( a -- count ) count trailing zeros
            "ctz" => self.codegen_inline_int_unary_intrinsic(stack_var, "llvm.cttz.i64"),

            // More stack operations
            // rot: ( a b c -- b c a )
            "rot" => {
                // Spill virtual registers before memory access (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_c = self.fresh_temp();
                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_c, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -3",
                    ptr_a, stack_var
                )?;

                // Load full Values (40 bytes each)
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_c, ptr_c
                )?;

                // Rotate: a goes to top, b goes to a's position, c goes to b's position
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_c, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_c
                )?;

                Ok(Some(stack_var.to_string()))
            }

            // nip: ( a b -- b )
            // Must call runtime to properly drop the removed value
            "nip" => {
                // Spill virtual registers before runtime call (Issue #189)
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
            // Uses patch_seq_clone_value to properly clone heap values
            "tuck" => {
                // Spill virtual registers (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let result_var = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Load full Values
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                // Clone b to the new top position
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, stack_var
                )?;

                // Result: b a b (a's slot gets b, b's slot gets a, new slot gets b_clone)
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_b
                )?;

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // 2dup: ( a b -- a b a b )
            // Uses patch_seq_clone_value to properly clone heap values
            "2dup" => {
                // Spill virtual registers (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let ptr_b = self.fresh_temp();
                let ptr_a = self.fresh_temp();
                let new_ptr = self.fresh_temp();
                let result_var = self.fresh_temp();

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, stack_var
                )?;
                // Clone a to stack_var
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_a, stack_var
                )?;
                // Clone b to stack_var + 1
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    new_ptr, stack_var
                )?;
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    ptr_b, new_ptr
                )?;

                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 2",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // 3drop: ( a b c -- )
            // Just call drop three times - simple and correct
            "3drop" => {
                // Spill virtual registers before runtime calls (Issue #189)
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
            // Copy the nth item (0-indexed from below n) to top
            // Uses patch_seq_clone_value to properly clone heap values
            "pick" => {
                // Spill virtual registers before memory access (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Issue #192: Optimize for constant N from previous IntLiteral
                if let Some(n) = self.prev_stmt_int_value
                    && n >= 0
                {
                    return self.codegen_pick_constant(stack_var, n as usize);
                }

                // Dynamic N case: read from stack
                // Get pointer to n (top of stack)
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load n from slot1
                let n_slot1 = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    n_slot1, n_ptr
                )?;
                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    n_val, n_slot1
                )?;

                // Calculate offset: -(n + 2) from stack_var
                // After popping n, x0 is at -1, x1 at -2, xn at -(n+1)
                // But we're indexing from stack_var, so xn is at -(n+2)
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 2", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to the item to copy
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                    src_ptr, stack_var, neg_offset
                )?;

                // Clone the value from src_ptr to n_ptr (replacing n with the picked value)
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
                    src_ptr, n_ptr
                )?;

                // SP unchanged (we replaced n with the picked value)
                Ok(Some(stack_var.to_string()))
            }

            // roll: ( ... xn xn-1 ... x1 x0 n -- ... xn-1 ... x1 x0 xn )
            // Move the nth item to top, shifting others down
            "roll" => {
                // Spill virtual registers before memory access (Issue #189)
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Issue #192: Optimize for constant N from previous IntLiteral
                if let Some(n) = self.prev_stmt_int_value
                    && n >= 0
                {
                    return self.codegen_roll_constant(stack_var, n as usize);
                }

                // Dynamic N case: read from stack
                // Get pointer to n (top of stack)
                let n_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    n_ptr, stack_var
                )?;

                // Load n from slot1
                let n_slot1 = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr i64, ptr %{}, i64 1",
                    n_slot1, n_ptr
                )?;
                let n_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load i64, ptr %{}",
                    n_val, n_slot1
                )?;

                // Pop n first - new SP is stack_var - 1
                let popped_sp = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    popped_sp, stack_var
                )?;

                // Calculate offset to the item to roll: -(n + 1) from popped_sp
                let offset = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, 1", offset, n_val)?;
                let neg_offset = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sub i64 0, %{}",
                    neg_offset, offset
                )?;

                // Get pointer to the item to roll (xn)
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 %{}",
                    src_ptr, popped_sp, neg_offset
                )?;

                // Load the value to roll
                let rolled_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    rolled_val, src_ptr
                )?;

                // Use memmove to shift items down (from src+1 to src, n items)
                // memmove(dest, src, size) - dest is src_ptr, src is src_ptr+1
                let src_plus_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    src_plus_one, src_ptr
                )?;

                // Size in bytes = n * 40 (sizeof %Value)
                let size_bytes = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = mul i64 %{}, 40",
                    size_bytes, n_val
                )?;

                // Call memmove
                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 %{}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top (popped_sp - 1, which is where x0 was)
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    rolled_val, top_ptr
                )?;

                // SP = popped_sp (we removed n, rolled doesn't change count)
                Ok(Some(popped_sp))
            }

            // >aux: ( T -- ) move top of main stack to aux slot (Issue #350)
            ">aux" => {
                // Spill virtual registers before aux access
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                let slot_idx = self.current_aux_sp;
                let slot_name = &self.current_aux_slots[slot_idx].clone();

                // Load top value from main stack (SP - 1)
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, stack_var
                )?;
                let val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val, top_ptr
                )?;

                // Store to aux slot
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val, slot_name
                )?;

                self.current_aux_sp += 1;

                // Decrement main stack SP (return pointer to top, which is now SP - 1)
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // aux>: ( -- T ) move top of aux stack to main stack (Issue #350)
            "aux>" => {
                // Spill virtual registers before aux access
                let stack_var = self.spill_virtual_stack(stack_var)?;
                let stack_var = stack_var.as_str();

                // Typechecker guarantees aux_sp > 0 here (paired >aux precedes)
                debug_assert!(
                    self.current_aux_sp > 0,
                    "aux>: aux stack underflow (typechecker should have caught this)"
                );
                self.current_aux_sp -= 1;
                let slot_idx = self.current_aux_sp;
                let slot_name = &self.current_aux_slots[slot_idx].clone();

                // Load from aux slot
                let val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val, slot_name
                )?;

                // Store to main stack at current SP
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val, stack_var
                )?;

                // Increment main stack SP
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    result_var, stack_var
                )?;
                Ok(Some(result_var))
            }

            // Not an inline-able operation
            _ => Ok(None),
        }
    }

    /// Generate optimized roll code when N is known at compile time (Issue #192)
    ///
    /// Stack effect: ( ... xn xn-1 ... x1 x0 n -- ... xn-1 ... x1 x0 xn )
    /// With constant N, we can:
    /// - n=0: no-op (just pop the 0)
    /// - n=1: swap (after popping the 1)
    /// - n=2: rot (after popping the 2)
    /// - n>=3: inline with constant offsets (no dynamic calculations)
    pub(super) fn codegen_roll_constant(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        // First, pop the N value from stack
        let popped_sp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_sp, stack_var
        )?;

        match n {
            0 => {
                // 0 roll is a no-op - just return after popping the 0
                Ok(Some(popped_sp))
            }
            1 => {
                // 1 roll = swap: ( a b -- b a )
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_b, popped_sp
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_a, popped_sp
                )?;
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_b
                )?;
                Ok(Some(popped_sp))
            }
            2 => {
                // 2 roll = rot: ( a b c -- b c a )
                let ptr_c = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    ptr_c, popped_sp
                )?;
                let ptr_b = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -2",
                    ptr_b, popped_sp
                )?;
                let ptr_a = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -3",
                    ptr_a, popped_sp
                )?;
                let val_a = self.fresh_temp();
                let val_b = self.fresh_temp();
                let val_c = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_a, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_b, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    val_c, ptr_c
                )?;
                // ( a b c -- b c a )
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_b, ptr_a
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_c, ptr_b
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    val_a, ptr_c
                )?;
                Ok(Some(popped_sp))
            }
            _ => {
                // n >= 3: Use memmove with constant offsets
                // Offset to xn: -(n+1) from popped_sp
                let neg_offset = -((n + 1) as i64);

                // Get pointer to the item to roll (xn)
                let src_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 {}",
                    src_ptr, popped_sp, neg_offset
                )?;

                // Load the value to roll
                let rolled_val = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = load %Value, ptr %{}",
                    rolled_val, src_ptr
                )?;

                // memmove: shift items down (from src+1 to src, n items)
                let src_plus_one = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 1",
                    src_plus_one, src_ptr
                )?;

                // Size in bytes = n * 40 (constant)
                let size_bytes = n * 40;
                writeln!(
                    &mut self.output,
                    "  call void @llvm.memmove.p0.p0.i64(ptr %{}, ptr %{}, i64 {}, i1 false)",
                    src_ptr, src_plus_one, size_bytes
                )?;

                // Store rolled value at top (popped_sp - 1)
                let top_ptr = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = getelementptr %Value, ptr %{}, i64 -1",
                    top_ptr, popped_sp
                )?;
                writeln!(
                    &mut self.output,
                    "  store %Value %{}, ptr %{}",
                    rolled_val, top_ptr
                )?;

                Ok(Some(popped_sp))
            }
        }
    }

    /// Generate optimized pick code when N is known at compile time (Issue #192)
    ///
    /// Stack effect: ( ... xn ... x1 x0 n -- ... xn ... x1 x0 xn )
    /// With constant N, we can:
    /// - n=0: dup (copy x0)
    /// - n=1: over (copy x1)
    /// - n>=2: inline with constant offset
    pub(super) fn codegen_pick_constant(
        &mut self,
        stack_var: &str,
        n: usize,
    ) -> Result<Option<String>, CodeGenError> {
        // Destination: replace n at top of stack (sp - 1)
        let n_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            n_ptr, stack_var
        )?;

        // Source offset: -(n + 2) from stack_var
        // n=0: x0 is at -2 (right below the n we're replacing)
        // n=1: x1 is at -3
        // etc.
        let neg_offset = -((n + 2) as i64);

        let src_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 {}",
            src_ptr, stack_var, neg_offset
        )?;

        // Clone the value from src to dest
        // We still need clone_value because the source could be a heap type
        writeln!(
            &mut self.output,
            "  call void @patch_seq_clone_value(ptr %{}, ptr %{})",
            src_ptr, n_ptr
        )?;

        // SP unchanged (we replaced n with the picked value)
        Ok(Some(stack_var.to_string()))
    }
}
