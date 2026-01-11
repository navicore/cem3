//! Word and Quotation Code Generation
//!
//! This module handles generating LLVM IR for word definitions,
//! quotations (closures), and tail call optimization.

use super::{
    BUILTIN_SYMBOLS, BranchResult, CodeGen, CodeGenError, QuotationFunctions, TailPosition,
    UNREACHABLE_PREDECESSOR, VirtualValue, mangle_name,
};
use crate::ast::{Statement, WordDef};
use crate::types::Type;
use std::fmt::Write as _;

impl CodeGen {
    /// Generate code for a word definition
    pub(super) fn codegen_word(&mut self, word: &WordDef) -> Result<(), CodeGenError> {
        // Prefix word names with "seq_" to avoid conflicts with C symbols
        // Also mangle special characters that aren't valid in LLVM IR identifiers
        let function_name = format!("seq_{}", mangle_name(&word.name));

        // main uses C calling convention since it's called from the runtime via function pointer.
        // All other words use tailcc for guaranteed tail call optimization.
        // This is fine because recursive main would be terrible design anyway.
        let is_main = word.name == "main";
        self.inside_main = is_main;

        // Issue #187: Mark small functions for inlining
        let inline_attr = if Self::is_inlineable_word(word) {
            " alwaysinline"
        } else {
            ""
        };

        if is_main {
            writeln!(
                &mut self.output,
                "define ptr @{}(ptr %stack) {{",
                function_name
            )?;
        } else {
            writeln!(
                &mut self.output,
                "define tailcc ptr @{}(ptr %stack){} {{",
                function_name, inline_attr
            )?;
        }
        writeln!(&mut self.output, "entry:")?;

        // For main (non-pure-inline): allocate the tagged stack and get base pointer
        // In pure_inline_test mode, main() allocates the stack, so seq_main just uses %stack
        let mut stack_var = if is_main && !self.pure_inline_test {
            // Allocate tagged stack
            writeln!(
                &mut self.output,
                "  %tagged_stack = call ptr @seq_stack_new_default()"
            )?;
            // Get base pointer - this is our initial "stack" (SP points to first free slot)
            writeln!(
                &mut self.output,
                "  %stack_base = call ptr @seq_stack_base(ptr %tagged_stack)"
            )?;
            // Set thread-local stack base for clone_stack (needed by spawn)
            writeln!(
                &mut self.output,
                "  call void @patch_seq_set_stack_base(ptr %stack_base)"
            )?;
            "stack_base".to_string()
        } else {
            "stack".to_string()
        };

        // Clear virtual stack at word boundary (Issue #189)
        self.virtual_stack.clear();

        // Set current word for type-specialized optimizations (Issue #186)
        self.current_word_name = Some(word.name.clone());
        self.current_stmt_index = 0;

        // Generate code for all statements with pattern detection for inline loops
        stack_var = self.codegen_statements(&word.body, &stack_var, true)?;

        // Clear current word tracking
        self.current_word_name = None;

        // Only emit ret if the last statement wasn't a tail call
        // (tail calls emit their own ret)
        if word.body.is_empty()
            || !self.will_emit_tail_call(word.body.last().unwrap(), TailPosition::Tail)
        {
            // Spill any remaining virtual registers before return (Issue #189)
            let stack_var = self.spill_virtual_stack(&stack_var)?;

            if is_main && !self.pure_inline_test {
                // Normal mode: free the stack before returning
                writeln!(
                    &mut self.output,
                    "  call void @seq_stack_free(ptr %tagged_stack)"
                )?;
                // Return null since we've freed the stack
                writeln!(&mut self.output, "  ret ptr null")?;
            } else {
                // Return the final stack pointer (used by main to read result)
                writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
            }
        }
        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        self.inside_main = false;
        Ok(())
    }

    /// Generate a quotation function
    /// Returns wrapper and impl function names for TCO support
    pub(super) fn codegen_quotation(
        &mut self,
        body: &[Statement],
        quot_type: &Type,
    ) -> Result<QuotationFunctions, CodeGenError> {
        // Generate unique function names
        let base_name = format!("seq_quot_{}", self.quot_counter);
        self.quot_counter += 1;

        // Save current output and switch to quotation_functions
        let saved_output = std::mem::take(&mut self.output);

        // Save and clear virtual stack for quotation scope (Issue #189)
        let saved_virtual_stack = std::mem::take(&mut self.virtual_stack);

        // Clear word context during quotation codegen to prevent
        // incorrect type lookups (quotations don't have their own type info)
        let saved_word_name = self.current_word_name.take();

        // Generate function signature based on type
        match quot_type {
            Type::Quotation(_) => {
                // Stateless quotation: generate both wrapper (C) and impl (tailcc)
                let wrapper_name = base_name.clone();
                let impl_name = format!("{}_impl", base_name);

                // First, generate the impl function with tailcc convention
                // This is the actual function body that can be tail-called
                writeln!(
                    &mut self.output,
                    "define tailcc ptr @{}(ptr %stack) {{",
                    impl_name
                )?;
                writeln!(&mut self.output, "entry:")?;

                let mut stack_var = "stack".to_string();
                let body_len = body.len();

                // Generate code for each statement in the quotation body
                // Last statement is in tail position (can use musttail)
                for (i, statement) in body.iter().enumerate() {
                    let position = if i == body_len - 1 {
                        TailPosition::Tail
                    } else {
                        TailPosition::NonTail
                    };
                    stack_var = self.codegen_statement(&stack_var, statement, position)?;
                }

                // Only emit ret if the last statement wasn't a tail call
                if body.is_empty()
                    || !self.will_emit_tail_call(body.last().unwrap(), TailPosition::Tail)
                {
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
                }
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Now generate the wrapper function with C convention
                // This is a thin wrapper that just calls the impl
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack) {{",
                    wrapper_name
                )?;
                writeln!(&mut self.output, "entry:")?;
                writeln!(
                    &mut self.output,
                    "  %result = call tailcc ptr @{}(ptr %stack)",
                    impl_name
                )?;
                writeln!(&mut self.output, "  ret ptr %result")?;
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Move generated functions to quotation_functions
                self.quotation_functions.push_str(&self.output);

                // Restore original output, word context, and virtual stack (Issue #189)
                self.output = saved_output;
                self.current_word_name = saved_word_name;
                self.virtual_stack = saved_virtual_stack;

                Ok(QuotationFunctions {
                    wrapper: wrapper_name,
                    impl_: impl_name,
                })
            }
            Type::Closure { captures, .. } => {
                // Closure: fn(Stack, *const Value, usize) -> Stack
                // Note: Closures don't use tailcc yet (Phase 3 work)
                // Mark that we're inside a closure to disable tail calls
                self.inside_closure = true;
                writeln!(
                    &mut self.output,
                    "define ptr @{}(ptr %stack, ptr %env_data, i64 %env_len) {{",
                    base_name
                )?;
                writeln!(&mut self.output, "entry:")?;

                // Push captured values onto the stack before executing body
                // Captures are stored bottom-to-top, so push them in order
                let mut stack_var = "stack".to_string();
                for (index, capture_type) in captures.iter().enumerate() {
                    stack_var = self.emit_capture_push(capture_type, index, &stack_var)?;
                }

                // Generate code for each statement in the quotation body
                // Last statement is in tail position
                let body_len = body.len();
                for (i, statement) in body.iter().enumerate() {
                    let position = if i == body_len - 1 {
                        TailPosition::Tail
                    } else {
                        TailPosition::NonTail
                    };
                    stack_var = self.codegen_statement(&stack_var, statement, position)?;
                }

                // Only emit ret if the last statement wasn't a tail call
                if body.is_empty()
                    || !self.will_emit_tail_call(body.last().unwrap(), TailPosition::Tail)
                {
                    writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
                }
                writeln!(&mut self.output, "}}")?;
                writeln!(&mut self.output)?;

                // Move generated function to quotation_functions
                self.quotation_functions.push_str(&self.output);

                // Restore original output, word context, virtual stack, and reset closure flag (Issue #189)
                self.output = saved_output;
                self.current_word_name = saved_word_name;
                self.virtual_stack = saved_virtual_stack;
                self.inside_closure = false;

                // For closures, both wrapper and impl are the same (no TCO yet)
                Ok(QuotationFunctions {
                    wrapper: base_name.clone(),
                    impl_: base_name,
                })
            }
            _ => Err(CodeGenError::Logic(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            ))),
        }
    }

    /// Check if a name refers to a runtime builtin (not a user-defined word).
    pub(super) fn is_runtime_builtin(&self, name: &str) -> bool {
        BUILTIN_SYMBOLS.contains_key(name)
            || self.external_builtins.contains_key(name)
            || self.ffi_bindings.is_ffi_function(name)
    }

    /// Emit code to push a captured value onto the stack.
    /// Returns the new stack variable name, or an error for unsupported types.
    pub(super) fn emit_capture_push(
        &mut self,
        capture_type: &Type,
        index: usize,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // String captures use a combined get+push function to avoid returning
        // SeqString by value through FFI (causes crashes on Linux due to calling convention)
        if matches!(capture_type, Type::String) {
            let new_stack_var = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_env_push_string(ptr %{}, ptr %env_data, i64 %env_len, i32 {})",
                new_stack_var, stack_var, index
            )?;
            return Ok(new_stack_var);
        }

        // Each capture type needs: (getter_fn, getter_llvm_type, pusher_fn, pusher_llvm_type)
        let (getter, getter_type, pusher, pusher_type) = match capture_type {
            Type::Int => ("patch_seq_env_get_int", "i64", "patch_seq_push_int", "i64"),
            Type::Bool => ("patch_seq_env_get_bool", "i64", "patch_seq_push_int", "i64"),
            Type::Float => (
                "patch_seq_env_get_float",
                "double",
                "patch_seq_push_float",
                "double",
            ),
            Type::String => unreachable!("String handled above"),
            Type::Quotation(_) => (
                "patch_seq_env_get_quotation",
                "i64",
                "patch_seq_push_quotation",
                "i64",
            ),
            Type::Closure { .. } => {
                return Err(CodeGenError::Logic(
                    "Closure captures are not yet supported. \
                     Closures capturing other closures require additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                        .to_string(),
                ));
            }
            Type::Var(name) if name.starts_with("Variant") => {
                return Err(CodeGenError::Logic(
                    "Variant captures are not yet supported. \
                     Capturing Variants in closures requires additional implementation. \
                     Supported capture types: Int, Bool, Float, String, Quotation."
                        .to_string(),
                ));
            }
            _ => {
                return Err(CodeGenError::Logic(format!(
                    "Unsupported capture type: {:?}. \
                     Supported capture types: Int, Bool, Float, String, Quotation.",
                    capture_type
                )));
            }
        };

        // Get value from environment
        let value_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call {} @{}(ptr %env_data, i64 %env_len, i32 {})",
            value_var, getter_type, getter, index
        )?;

        // Push value onto stack
        let new_stack_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @{}(ptr %{}, {} %{})",
            new_stack_var, pusher, stack_var, pusher_type, value_var
        )?;

        Ok(new_stack_var)
    }

    /// Generate code for a single branch of an if statement.
    ///
    /// Returns the final stack variable, whether a tail call was emitted,
    /// and the predecessor block label for the phi node.
    pub(super) fn codegen_branch(
        &mut self,
        statements: &[Statement],
        initial_stack: &str,
        position: TailPosition,
        merge_block: &str,
        block_prefix: &str,
    ) -> Result<BranchResult, CodeGenError> {
        // Increment depth to disable type lookups in nested branches
        self.codegen_depth += 1;

        // Save and clear virtual stack for this branch (Issue #189)
        // Each branch starts fresh; values must be in memory for phi merge
        let saved_virtual_stack = std::mem::take(&mut self.virtual_stack);

        let mut stack_var = initial_stack.to_string();
        let len = statements.len();
        let mut emitted_tail_call = false;

        for (i, stmt) in statements.iter().enumerate() {
            let stmt_position = if i == len - 1 {
                position // Last statement inherits our tail position
            } else {
                TailPosition::NonTail
            };
            if i == len - 1 {
                emitted_tail_call = self.will_emit_tail_call(stmt, stmt_position);
            }
            stack_var = self.codegen_statement(&stack_var, stmt, stmt_position)?;
        }

        // Spill any remaining virtual values before branch merge (Issue #189)
        if !emitted_tail_call {
            stack_var = self.spill_virtual_stack(&stack_var)?;
        }

        // Only emit landing block if no tail call was emitted
        let predecessor = if emitted_tail_call {
            UNREACHABLE_PREDECESSOR.to_string()
        } else {
            let pred = self.fresh_block(&format!("{}_end", block_prefix));
            writeln!(&mut self.output, "  br label %{}", pred)?;
            writeln!(&mut self.output, "{}:", pred)?;
            writeln!(&mut self.output, "  br label %{}", merge_block)?;
            pred
        };

        // Restore virtual stack and depth (Issue #189)
        self.virtual_stack = saved_virtual_stack;
        self.codegen_depth -= 1;

        Ok(BranchResult {
            stack_var,
            emitted_tail_call,
            predecessor,
        })
    }

    /// Check if a statement in tail position would emit a terminator (ret).
    ///
    /// Returns true for:
    /// - User-defined word calls (emit `musttail` + `ret`)
    /// - `call` word (Phase 2 TCO for quotations)
    /// - If statements where BOTH branches emit terminators
    ///
    /// Returns false if inside a closure (closures can't use `musttail` due to
    /// signature mismatch - they have 3 params vs 1 for regular functions).
    /// Also returns false if inside main or quotation (they use C convention, can't musttail to tailcc).
    pub(super) fn will_emit_tail_call(
        &self,
        statement: &Statement,
        position: TailPosition,
    ) -> bool {
        if position != TailPosition::Tail
            || self.inside_closure
            || self.inside_main
            || self.inside_quotation
        {
            return false;
        }
        match statement {
            Statement::WordCall { name, .. } => {
                // Phase 2 TCO: `call` is now TCO-eligible (it emits its own ret)
                if name == "call" {
                    return true;
                }
                !self.is_runtime_builtin(name)
            }
            Statement::If {
                then_branch,
                else_branch,
            } => {
                // An if statement emits a terminator (no merge block) if BOTH branches
                // end with terminators. Empty branches don't terminate.
                let then_terminates = then_branch
                    .last()
                    .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail));
                let else_terminates = else_branch
                    .as_ref()
                    .and_then(|eb| eb.last())
                    .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail));
                then_terminates && else_terminates
            }
            _ => false,
        }
    }

    /// Generate code for a tail call to a quotation (Phase 2 TCO).
    ///
    /// This is called when `call` is in tail position. We generate inline dispatch:
    /// 1. Check if top of stack is a Quotation (not Closure)
    /// 2. If Quotation: pop, extract fn_ptr, musttail call it
    /// 3. If Closure: call regular patch_seq_call (no TCO for closures yet)
    ///
    /// The function always emits a `ret`, so no merge block is needed.
    pub(super) fn codegen_tail_call_quotation(
        &mut self,
        stack_var: &str,
        _result_var: &str,
    ) -> Result<String, CodeGenError> {
        // Check if top of stack is a quotation
        let is_quot_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_is_quotation(ptr %{})",
            is_quot_var, stack_var
        )?;

        let cmp_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 1",
            cmp_var, is_quot_var
        )?;

        // Create labels for branching
        let quot_block = self.fresh_block("call_quotation");
        let closure_block = self.fresh_block("call_closure");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_var, quot_block, closure_block
        )?;

        // Quotation path: extract fn_ptr and musttail call (Issue #215: extracted helper)
        writeln!(&mut self.output, "{}:", quot_block)?;
        self.codegen_tail_call_quotation_path(stack_var)?;

        // Closure path: fall back to regular patch_seq_call (Issue #215: extracted helper)
        writeln!(&mut self.output, "{}:", closure_block)?;
        let closure_result = self.codegen_tail_call_closure_path(stack_var)?;

        // Return a dummy value - both branches emit ret, so this won't be used
        Ok(closure_result)
    }

    /// Generate tail call path for quotation (Issue #215: extracted helper).
    pub(super) fn codegen_tail_call_quotation_path(
        &mut self,
        stack_var: &str,
    ) -> Result<(), CodeGenError> {
        let fn_ptr_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_peek_quotation_fn_ptr(ptr %{})",
            fn_ptr_var, stack_var
        )?;

        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            popped_stack, stack_var
        )?;

        let fn_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = inttoptr i64 %{} to ptr",
            fn_var, fn_ptr_var
        )?;

        // Yield check before tail call to prevent starvation in tight loops
        writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
        let quot_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = musttail call tailcc ptr %{}(ptr %{})",
            quot_result, fn_var, popped_stack
        )?;
        writeln!(&mut self.output, "  ret ptr %{}", quot_result)?;
        Ok(())
    }

    /// Generate tail call path for closure (Issue #215: extracted helper).
    pub(super) fn codegen_tail_call_closure_path(
        &mut self,
        stack_var: &str,
    ) -> Result<String, CodeGenError> {
        // Note: No yield check here because closures use regular calls (not musttail),
        // so recursive closures will eventually hit stack limits.
        let closure_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_call(ptr %{})",
            closure_result, stack_var
        )?;
        writeln!(&mut self.output, "  ret ptr %{}", closure_result)?;
        Ok(closure_result)
    }

    // =========================================================================
    // Statement Code Generation Helpers
    // =========================================================================

    /// Generate code for an integer literal: ( -- n )
    ///
    /// Issue #189: Keeps value in virtual register instead of writing to memory.
    /// The value will be spilled to memory at control flow points or function calls.
    pub(super) fn codegen_int_literal(
        &mut self,
        stack_var: &str,
        n: i64,
    ) -> Result<String, CodeGenError> {
        // Create an SSA variable for this integer value
        let ssa_var = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = add i64 0, {}", ssa_var, n)?;

        // Push to virtual stack (may spill if at capacity)
        let value = VirtualValue::Int { ssa_var, value: n };
        self.push_virtual(value, stack_var)
    }

    /// Generate code for a float literal: ( -- f ) - Non-nanbox version
    ///
    /// Uses LLVM's hexadecimal floating point format for exact representation.
    /// Handles special values (NaN, Infinity) explicitly.
    #[cfg(not(feature = "nanbox"))]
    pub(super) fn codegen_float_literal(
        &mut self,
        stack_var: &str,
        f: f64,
    ) -> Result<String, CodeGenError> {
        // Spill virtual values before writing to memory (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;

        // Format float bits as hex for LLVM
        let float_bits = f.to_bits();

        // Inline push: Write Value directly to stack
        // Value layout with #[repr(C)]: slot0=discriminant, slot1=value
        // Float discriminant = 1 (Int=0, Float=1, Bool=2)

        // Store discriminant 1 (Float) at slot0
        writeln!(&mut self.output, "  store i64 1, ptr %{}", stack_var)?;

        // Get pointer to slot1 (offset 8 bytes = 1 i64)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, stack_var
        )?;

        // Store float bits as i64 at slot1
        writeln!(
            &mut self.output,
            "  store i64 {}, ptr %{}",
            float_bits, slot1_ptr
        )?;

        // Return pointer to next Value slot
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a float literal: ( -- f ) - NaN-box version
    ///
    /// In NaN-box mode, floats are stored directly as their bit pattern.
    /// NaN values that could collide with boxed values are canonicalized.
    #[cfg(feature = "nanbox")]
    pub(super) fn codegen_float_literal(
        &mut self,
        stack_var: &str,
        f: f64,
    ) -> Result<String, CodeGenError> {
        // NaN-boxing threshold: values >= this are in our NaN-box range
        const NANBOX_THRESHOLD: u64 = 0xFFF8_0000_0000_0000;
        const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

        // Spill virtual values before writing to memory
        let stack_var = self.spill_virtual_stack(stack_var)?;

        // Get float bits - canonicalize if it could collide with nanbox range
        let float_bits = f.to_bits();
        let stored_bits = if float_bits >= NANBOX_THRESHOLD {
            CANONICAL_NAN
        } else {
            float_bits
        };

        // Store float bits directly as i64 (no encoding needed for normal floats)
        writeln!(
            &mut self.output,
            "  store i64 {}, ptr %{}",
            stored_bits, stack_var
        )?;

        // Return pointer to next Value slot (8 bytes in nanbox mode)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a boolean literal: ( -- b ) - Non-nanbox version
    #[cfg(not(feature = "nanbox"))]
    pub(super) fn codegen_bool_literal(
        &mut self,
        stack_var: &str,
        b: bool,
    ) -> Result<String, CodeGenError> {
        // Spill virtual values before writing to memory (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;

        let val = if b { 1 } else { 0 };

        // Inline push: Write Value directly to stack
        // Value layout with #[repr(C)]: slot0=discriminant, slot1=value
        // Bool discriminant = 2 (Int=0, Float=1, Bool=2)

        // Store discriminant 2 (Bool) at slot0
        writeln!(&mut self.output, "  store i64 2, ptr %{}", stack_var)?;

        // Get pointer to slot1 (offset 8 bytes = 1 i64)
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, stack_var
        )?;

        // Store value at slot1 (1 for true, 0 for false)
        writeln!(&mut self.output, "  store i64 {}, ptr %{}", val, slot1_ptr)?;

        // Return pointer to next Value slot
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a boolean literal: ( -- b ) - NaN-box version
    #[cfg(feature = "nanbox")]
    pub(super) fn codegen_bool_literal(
        &mut self,
        stack_var: &str,
        b: bool,
    ) -> Result<String, CodeGenError> {
        // NaN-boxing constants (must match runtime/nanbox.rs)
        const NANBOX_BASE: u64 = 0xFFF8_0000_0000_0000;
        const TAG_SHIFT: u32 = 47;
        const TAG_BOOL: u64 = 1;

        // Spill virtual values before writing to memory
        let stack_var = self.spill_virtual_stack(stack_var)?;

        // Encode as NaN-boxed Bool: NANBOX_BASE | (TAG_BOOL << TAG_SHIFT) | payload
        let payload = if b { 1 } else { 0 };
        let nanboxed = NANBOX_BASE | (TAG_BOOL << TAG_SHIFT) | payload;

        // Store single i64 NaN-boxed value
        writeln!(
            &mut self.output,
            "  store i64 {}, ptr %{}",
            nanboxed, stack_var
        )?;

        // Return pointer to next Value slot (8 bytes in nanbox mode)
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            result_var, stack_var
        )?;

        Ok(result_var)
    }

    /// Generate code for a string literal: ( -- s )
    pub(super) fn codegen_string_literal(
        &mut self,
        stack_var: &str,
        s: &str,
    ) -> Result<String, CodeGenError> {
        // Spill virtual values before calling runtime (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;

        let global = self.get_string_global(s)?;
        let ptr_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr inbounds [{} x i8], ptr {}, i32 0, i32 0",
            ptr_temp,
            s.len() + 1,
            global
        )?;
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_string(ptr %{}, ptr %{})",
            result_var, stack_var, ptr_temp
        )?;
        Ok(result_var)
    }

    /// Generate code for a symbol literal: ( -- sym )
    pub(super) fn codegen_symbol_literal(
        &mut self,
        stack_var: &str,
        s: &str,
    ) -> Result<String, CodeGenError> {
        // Spill virtual values before calling runtime (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;

        // Get interned symbol global (static SeqString structure)
        let sym_global = self.get_symbol_global(s)?;

        // Push the interned symbol - passes pointer to static SeqString structure
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_interned_symbol(ptr %{}, ptr {})",
            result_var, stack_var, sym_global
        )?;
        Ok(result_var)
    }

    // =========================================================================
    // Word Inlineability Checking
    // =========================================================================

    /// Determine if a word is inlineable.
    ///
    /// A word is considered inlineable if it:
    /// - Is not main (main is the entry point)
    /// - Not recursive (doesn't call itself, even in branches)
    /// - Few statements (<= 10)
    /// - No quotations (create closures, make function large)
    pub(super) fn is_inlineable_word(word: &WordDef) -> bool {
        const MAX_INLINE_STATEMENTS: usize = 10;

        // main is never inlined
        if word.name == "main" {
            return false;
        }

        // Too many statements
        if word.body.len() > MAX_INLINE_STATEMENTS {
            return false;
        }

        // Check for disqualifying patterns (recursively)
        Self::check_statements_inlineable(&word.body, &word.name)
    }

    /// Recursively check if statements allow inlining
    pub(super) fn check_statements_inlineable(statements: &[Statement], word_name: &str) -> bool {
        for stmt in statements {
            match stmt {
                // Recursive calls prevent inlining
                Statement::WordCall { name, .. } if name == word_name => {
                    return false;
                }
                // Quotations create closures - don't inline
                Statement::Quotation { .. } => {
                    return false;
                }
                // Check inside if branches for recursive calls
                Statement::If {
                    then_branch,
                    else_branch,
                } => {
                    if !Self::check_statements_inlineable(then_branch, word_name) {
                        return false;
                    }
                    if let Some(else_stmts) = else_branch
                        && !Self::check_statements_inlineable(else_stmts, word_name)
                    {
                        return false;
                    }
                }
                // Check inside match arms for recursive calls
                Statement::Match { arms } => {
                    for arm in arms {
                        if !Self::check_statements_inlineable(&arm.body, word_name) {
                            return false;
                        }
                    }
                }
                // Everything else is fine
                _ => {}
            }
        }
        true
    }
}
