//! Statement Code Generation
//!
//! This module handles generating LLVM IR for individual statements,
//! word calls, quotation pushes, and the main function.

use super::specialization::RegisterType;
use super::{BUILTIN_SYMBOLS, CodeGen, CodeGenError, TailPosition, VirtualValue, mangle_name};
use crate::ast::Statement;
use crate::types::Type;
use std::fmt::Write as _;

impl CodeGen {
    /// Generate code for a word call
    ///
    /// Handles builtin functions, external builtins, and user-defined words.
    /// Emits tail calls when appropriate.
    pub(super) fn codegen_word_call(
        &mut self,
        stack_var: &str,
        name: &str,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Inline operations for common stack/arithmetic ops
        if let Some(result) = self.try_codegen_inline_op(stack_var, name)? {
            return Ok(result);
        }

        // Check if this would be a tail call position for a user-defined word.
        // If so, skip specialized dispatch - we need the tail call path to emit
        // musttail + ret for proper TCO. Specialized dispatch returns a value
        // without emitting a terminator, which would leave the basic block
        // without a terminator if codegen_branch expects one (Issue #338).
        let is_seq_word = !BUILTIN_SYMBOLS.contains_key(name)
            && !self.external_builtins.contains_key(name)
            && !self.ffi_bindings.is_ffi_function(name);
        let would_tail_call = position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
            && is_seq_word;

        // Try dispatch to specialized version if virtual stack has matching types,
        // but only if we're NOT in tail position (tail calls need musttail + ret)
        if !would_tail_call && let Some(result) = self.try_specialized_dispatch(stack_var, name)? {
            return Ok(result);
        }

        // Spill virtual registers before function call (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        let result_var = self.fresh_temp();

        // Phase 2 TCO: Special handling for `call` in tail position
        // Not available in main/quotation (C convention can't musttail to tailcc)
        if name == "call"
            && position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
        {
            return self.codegen_tail_call_quotation(stack_var, &result_var);
        }

        // Map source-level word names to runtime function names
        let (function_name, is_seq_word) = if let Some(&symbol) = BUILTIN_SYMBOLS.get(name) {
            (symbol.to_string(), false)
        } else if let Some(symbol) = self.external_builtins.get(name) {
            (symbol.clone(), false)
        } else if self.ffi_bindings.is_ffi_function(name) {
            // FFI wrapper function
            (format!("seq_ffi_{}", mangle_name(name)), false)
        } else {
            (format!("seq_{}", mangle_name(name)), true)
        };

        // Emit tail call for user-defined words in tail position
        // Not available in main/quotation (C convention can't musttail to tailcc)
        let can_tail_call = position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
            && is_seq_word;
        if can_tail_call {
            // Yield check before tail call to prevent starvation in tight loops
            writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
            writeln!(
                &mut self.output,
                "  %{} = musttail call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
            writeln!(&mut self.output, "  ret ptr %{}", result_var)?;
        } else if is_seq_word {
            // Non-tail call to user-defined word: must use tailcc calling convention
            writeln!(
                &mut self.output,
                "  %{} = call tailcc ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
        } else {
            // Call to builtin (C calling convention)
            writeln!(
                &mut self.output,
                "  %{} = call ptr @{}(ptr %{})",
                result_var, function_name, stack_var
            )?;
        }
        Ok(result_var)
    }

    /// Try to dispatch to a specialized version of a word
    ///
    /// If the called word has a specialized version and the virtual stack
    /// has values matching the specialized signature, we emit a direct call
    /// to the specialized function instead of the stack-based version.
    ///
    /// Returns Some(result) if dispatch succeeded, None if fallback needed.
    fn try_specialized_dispatch(
        &mut self,
        stack_var: &str,
        name: &str,
    ) -> Result<Option<String>, CodeGenError> {
        // Check if this word has a specialized version
        let sig = match self.specialized_words.get(name) {
            Some(sig) => sig.clone(),
            None => return Ok(None),
        };

        // Check if we have enough values on the virtual stack
        let input_count = sig.inputs.len();
        if self.virtual_stack.len() < input_count {
            return Ok(None);
        }

        // Verify all inputs match expected types (check from bottom to top of what we'll pop)
        // sig.inputs is bottom-to-top, but virtual_stack.last() is the top
        // So sig.inputs[input_count-1] should match virtual_stack.last(), etc.
        for (i, expected_ty) in sig.inputs.iter().enumerate() {
            // Index into virtual stack: last element minus offset
            let stack_idx = self.virtual_stack.len() - input_count + i;
            let matches = match expected_ty {
                RegisterType::I64 => {
                    matches!(
                        self.virtual_stack.get(stack_idx),
                        Some(VirtualValue::Int { .. }) | Some(VirtualValue::Bool { .. })
                    )
                }
                RegisterType::Double => {
                    matches!(
                        self.virtual_stack.get(stack_idx),
                        Some(VirtualValue::Float { .. })
                    )
                }
            };
            if !matches {
                return Ok(None);
            }
        }

        // Pop arguments from virtual stack (top first, so reverse order)
        let mut args = Vec::with_capacity(input_count);
        for _ in 0..input_count {
            let arg = self.virtual_stack.pop().unwrap();
            let arg_var = match arg {
                VirtualValue::Int { ssa_var, .. } => ssa_var,
                VirtualValue::Float { ssa_var } => ssa_var,
                VirtualValue::Bool { ssa_var } => ssa_var,
            };
            args.push(arg_var);
        }
        args.reverse(); // Now in bottom-to-top order (matches sig.inputs)

        // Generate specialized function name
        let spec_name = format!("seq_{}{}", mangle_name(name), sig.suffix());

        // Build argument list string
        let arg_strs: Vec<String> = sig
            .inputs
            .iter()
            .zip(args.iter())
            .map(|(ty, var)| format!("{} %{}", ty.llvm_type(), var))
            .collect();

        // Emit the specialized call
        let result_var = self.fresh_temp();
        let return_type = sig.llvm_return_type();

        writeln!(
            &mut self.output,
            "  %{} = call {} @{}({})",
            result_var,
            return_type,
            spec_name,
            arg_strs.join(", ")
        )?;

        // Push results back to virtual stack
        let mut final_stack_var = stack_var.to_string();

        if sig.outputs.len() == 1 {
            // Single output - push directly
            let output_ty = sig.outputs[0];
            let result = match output_ty {
                RegisterType::I64 => VirtualValue::Int {
                    ssa_var: result_var.clone(),
                    value: 0, // Unknown runtime value
                },
                RegisterType::Double => VirtualValue::Float {
                    ssa_var: result_var.clone(),
                },
            };
            final_stack_var = self.push_virtual(result, &final_stack_var)?;
        } else {
            // Multi-output - extract values from struct and push each
            for (i, output_ty) in sig.outputs.iter().enumerate() {
                let extracted = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = extractvalue {} %{}, {}",
                    extracted, return_type, result_var, i
                )?;

                let result = match output_ty {
                    RegisterType::I64 => VirtualValue::Int {
                        ssa_var: extracted,
                        value: 0, // Unknown runtime value
                    },
                    RegisterType::Double => VirtualValue::Float { ssa_var: extracted },
                };
                final_stack_var = self.push_virtual(result, &final_stack_var)?;
            }
        }

        Ok(Some(final_stack_var))
    }

    /// Generate code for pushing a quotation or closure onto the stack
    pub(super) fn codegen_quotation_push(
        &mut self,
        stack_var: &str,
        id: usize,
        body: &[Statement],
    ) -> Result<String, CodeGenError> {
        // Spill virtual registers before quotation operations (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        let quot_type = self.get_quotation_type(id)?.clone();
        let fns = self.codegen_quotation(body, &quot_type)?;

        match quot_type {
            Type::Quotation(_) => {
                // Get both wrapper and impl function pointers as i64
                let wrapper_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    wrapper_ptr_var, fns.wrapper
                )?;

                let impl_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    impl_ptr_var, fns.impl_
                )?;

                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_quotation(ptr %{}, i64 %{}, i64 %{})",
                    result_var, stack_var, wrapper_ptr_var, impl_ptr_var
                )?;
                Ok(result_var)
            }
            Type::Closure { captures, .. } => {
                // For closures, just use the single function pointer (no TCO yet)
                let fn_ptr_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = ptrtoint ptr @{} to i64",
                    fn_ptr_var, fns.wrapper
                )?;

                let capture_count = i32::try_from(captures.len()).map_err(|_| {
                    format!(
                        "Closure has too many captures ({}) - maximum is {}",
                        captures.len(),
                        i32::MAX
                    )
                })?;
                let result_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_push_closure(ptr %{}, i64 %{}, i32 {})",
                    result_var, stack_var, fn_ptr_var, capture_count
                )?;
                Ok(result_var)
            }
            _ => Err(CodeGenError::Logic(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            ))),
        }
    }

    // =========================================================================
    // Main Statement Dispatcher
    // =========================================================================

    /// Generate code for a sequence of statements with pattern detection.
    ///
    /// Detects patterns like `[cond] [body] while` and emits inline loops
    /// instead of quotation push + FFI call.
    ///
    /// Returns the final stack variable name.
    pub(super) fn codegen_statements(
        &mut self,
        statements: &[Statement],
        initial_stack_var: &str,
        last_is_tail: bool,
    ) -> Result<String, CodeGenError> {
        // Track nesting depth for type-specialized optimizations:
        // - codegen_depth starts at 0, we increment to 1 for the first (top-level) call
        // - Top-level word body runs at depth 1 (type lookups allowed)
        // - Nested calls (loop bodies, branches) run at depth > 1 (type lookups disabled)
        // The check in is_trivially_copyable_at_current_stmt uses `depth > 1` accordingly
        let entering_depth = self.codegen_depth;
        self.codegen_depth += 1;

        let result = self.codegen_statements_inner(statements, initial_stack_var, last_is_tail);

        self.codegen_depth = entering_depth;

        result
    }

    /// Internal implementation of codegen_statements
    pub(super) fn codegen_statements_inner(
        &mut self,
        statements: &[Statement],
        initial_stack_var: &str,
        last_is_tail: bool,
    ) -> Result<String, CodeGenError> {
        let mut stack_var = initial_stack_var.to_string();
        let len = statements.len();
        let mut i = 0;

        while i < len {
            // Update statement index for type-specialized optimizations (Issue #186)
            // This tracks our position in the word body for looking up type info
            self.current_stmt_index = i;

            // Check if previous statement was a trivially-copyable literal (Issue #195)
            // This enables optimized dup after patterns like `42 dup`
            self.prev_stmt_is_trivial_literal = i > 0
                && matches!(
                    &statements[i - 1],
                    Statement::IntLiteral(_)
                        | Statement::FloatLiteral(_)
                        | Statement::BoolLiteral(_)
                );

            // Track the actual int value if previous was IntLiteral (Issue #192)
            // This enables optimized roll/pick with constant N (e.g., `2 roll` -> rot)
            self.prev_stmt_int_value = if i > 0 {
                if let Statement::IntLiteral(n) = &statements[i - 1] {
                    Some(*n)
                } else {
                    None
                }
            } else {
                None
            };

            let is_last = i == len - 1;
            let position = if is_last && last_is_tail {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };

            // Regular statement processing
            stack_var = self.codegen_statement(&stack_var, &statements[i], position)?;
            i += 1;
        }

        Ok(stack_var)
    }

    /// Generate code for a single statement
    ///
    /// The `position` parameter indicates whether this statement is in tail position.
    /// For tail calls, we emit `musttail call` followed by `ret` to guarantee TCO.
    pub(super) fn codegen_statement(
        &mut self,
        stack_var: &str,
        statement: &Statement,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        match statement {
            Statement::IntLiteral(n) => self.codegen_int_literal(stack_var, *n),
            Statement::FloatLiteral(f) => self.codegen_float_literal(stack_var, *f),
            Statement::BoolLiteral(b) => self.codegen_bool_literal(stack_var, *b),
            Statement::StringLiteral(s) => self.codegen_string_literal(stack_var, s),
            Statement::Symbol(s) => self.codegen_symbol_literal(stack_var, s),
            Statement::WordCall { name, .. } => self.codegen_word_call(stack_var, name, position),
            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => self.codegen_if_statement(stack_var, then_branch, else_branch.as_ref(), position),
            Statement::Quotation { id, body, .. } => {
                self.codegen_quotation_push(stack_var, *id, body)
            }
            Statement::Match { arms, span: _ } => {
                self.codegen_match_statement(stack_var, arms, position)
            }
        }
    }

    /// Generate main function that calls user's main word
    pub(super) fn codegen_main(&mut self) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.output,
            "define i32 @main(i32 %argc, ptr %argv) {{"
        )?;
        writeln!(&mut self.output, "entry:")?;

        if self.pure_inline_test {
            // Pure inline test mode: no scheduler, just run the code directly
            // and return the top of stack as exit code.
            //
            // This mode is for testing pure integer programs that use only
            // inlined operations (push_int, arithmetic, stack ops).

            // Allocate tagged stack
            writeln!(
                &mut self.output,
                "  %tagged_stack = call ptr @seq_stack_new_default()"
            )?;
            writeln!(
                &mut self.output,
                "  %stack_base = call ptr @seq_stack_base(ptr %tagged_stack)"
            )?;

            // Call seq_main which returns the final stack pointer
            writeln!(
                &mut self.output,
                "  %final_sp = call ptr @seq_main(ptr %stack_base)"
            )?;

            // Read top of stack value (at sp - 1, slot1 contains the int value)
            writeln!(
                &mut self.output,
                "  %top_ptr = getelementptr %Value, ptr %final_sp, i64 -1"
            )?;
            writeln!(
                &mut self.output,
                "  %val_ptr = getelementptr i64, ptr %top_ptr, i64 1"
            )?;
            writeln!(&mut self.output, "  %result = load i64, ptr %val_ptr")?;

            // Free the stack
            writeln!(
                &mut self.output,
                "  call void @seq_stack_free(ptr %tagged_stack)"
            )?;

            // Return result as exit code (truncate to i32)
            writeln!(&mut self.output, "  %exit_code = trunc i64 %result to i32")?;
            writeln!(&mut self.output, "  ret i32 %exit_code")?;
        } else {
            // Normal mode: use scheduler for concurrency support

            // Initialize command-line arguments (before scheduler so args are available)
            writeln!(
                &mut self.output,
                "  call void @patch_seq_args_init(i32 %argc, ptr %argv)"
            )?;

            // Initialize scheduler
            writeln!(&mut self.output, "  call void @patch_seq_scheduler_init()")?;

            // Register instrumentation data with report system (--instrument)
            if self.instrument {
                let n = self.word_instrument_ids.len();
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_report_init(ptr @seq_word_counters, ptr @seq_word_names, i64 {})",
                    n
                )?;
            }

            // Spawn user's main function as the first strand
            // This ensures all code runs in coroutine context for non-blocking I/O
            writeln!(
                &mut self.output,
                "  %0 = call i64 @patch_seq_strand_spawn(ptr @seq_main, ptr null)"
            )?;

            // Wait for all spawned strands to complete (including main)
            writeln!(
                &mut self.output,
                "  %1 = call ptr @patch_seq_scheduler_run()"
            )?;

            // Emit at-exit report (no-op unless SEQ_REPORT is set at runtime)
            writeln!(&mut self.output, "  call void @patch_seq_report()")?;

            writeln!(&mut self.output, "  ret i32 0")?;
        }
        writeln!(&mut self.output, "}}")?;

        Ok(())
    }
}
