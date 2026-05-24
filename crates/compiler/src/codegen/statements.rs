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
        span: Option<&crate::ast::Span>,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Resolve arithmetic sugar (e.g., `+` → `i.+`) using typechecker's resolution
        let resolved;
        let name = if let Some(s) = span
            && let Some(concrete) = self.resolve_sugar_at(s.line, s.column)
        {
            resolved = concrete.to_string();
            &resolved
        } else {
            name
        };

        // Inline operations for common stack/arithmetic ops
        if let Some(result) = self.try_codegen_inline_op(stack_var, name)? {
            return Ok(result);
        }

        let (function_name, is_seq_word) = self.resolve_call_target(name);

        // Tail-call eligibility: only user-defined Seq words, in tail
        // position, outside the C-convention contexts (main, quotations,
        // closures). Computed once and reused for both the
        // skip-specialized-dispatch decision and the final call emission
        // (Issue #338: specialized dispatch doesn't emit a terminator, which
        // breaks codegen_branch when it expects a tail-call ret).
        let can_tail_call = position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
            && is_seq_word;

        // Try dispatch to specialized version if virtual stack has matching
        // types, but only if we're NOT in tail position (tail calls need
        // musttail + ret).
        if !can_tail_call && let Some(result) = self.try_specialized_dispatch(stack_var, name)? {
            return Ok(result);
        }

        // Spill virtual registers before function call (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        let result_var = self.fresh_temp();

        // Phase 2 TCO: Special handling for `call` in tail position.
        // Not available in main/quotation (C convention can't musttail to tailcc).
        if name == "call"
            && position == TailPosition::Tail
            && !self.inside_closure
            && !self.inside_main
            && !self.inside_quotation
        {
            return self.codegen_tail_call_quotation(stack_var, &result_var);
        }

        // Allocate a DILocation for this call site so a runtime panic
        // here resolves back to .seq:line:col in the backtrace. No-op when
        // debug info is disabled or the statement has no span.
        let dbg = self.dbg_call_suffix(span);

        if can_tail_call {
            // Yield check before tail call to prevent starvation in tight loops
            writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
            writeln!(
                &mut self.output,
                "  %{} = musttail call tailcc ptr @{}(ptr %{}){}",
                result_var, function_name, stack_var, dbg
            )?;
            // Leave the `ret` bare. `musttail` requires the ret to mirror the
            // call exactly; the !dbg on the call is enough for backtrace
            // resolution (the call-site address is what gets symbolised).
            writeln!(&mut self.output, "  ret ptr %{}", result_var)?;
        } else if is_seq_word {
            // Non-tail call to user-defined word: must use tailcc calling convention
            writeln!(
                &mut self.output,
                "  %{} = call tailcc ptr @{}(ptr %{}){}",
                result_var, function_name, stack_var, dbg
            )?;
        } else {
            // Call to builtin (C calling convention).
            //
            // For `test.assert*` builtins, first tell the runtime which
            // source line this assertion came from so a failure can be
            // attributed. The hook is a no-op at runtime outside tests
            // (it just writes to the global test context).
            if name.starts_with("test.assert")
                && let Some(s) = span
            {
                let line = (s.line + 1) as i64; // 1-indexed for humans
                writeln!(
                    &mut self.output,
                    "  call void @patch_seq_test_set_line(i64 {})",
                    line
                )?;
            }
            writeln!(
                &mut self.output,
                "  %{} = call ptr @{}(ptr %{}){}",
                result_var, function_name, stack_var, dbg
            )?;
        }
        Ok(result_var)
    }

    /// Map a Seq word name to its LLVM symbol and whether it's a user-defined
    /// Seq word (vs. builtin, external builtin, or FFI wrapper).
    fn resolve_call_target(&self, name: &str) -> (String, bool) {
        if let Some(&symbol) = BUILTIN_SYMBOLS.get(name) {
            (symbol.to_string(), false)
        } else if let Some(symbol) = self.external_builtins.get(name) {
            (symbol.clone(), false)
        } else if self.ffi_bindings.is_ffi_function(name) {
            // FFI wrapper function
            (format!("seq_ffi_{}", mangle_name(name)), false)
        } else {
            (format!("seq_{}", mangle_name(name)), true)
        }
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
        let Some(sig) = self.specialized_words.get(name).cloned() else {
            return Ok(None);
        };

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
            let arg = self
                .virtual_stack
                .pop()
                .expect("checked virtual_stack.len() >= input_count above");
            let arg_var = match arg {
                VirtualValue::Int { ssa_var, .. } => ssa_var,
                VirtualValue::Float { ssa_var } => ssa_var,
                VirtualValue::Bool { ssa_var } => ssa_var,
            };
            args.push(arg_var);
        }
        args.reverse(); // Now in bottom-to-top order (matches sig.inputs)

        let spec_name = format!("seq_{}{}", mangle_name(name), sig.suffix());

        let arg_strs: Vec<String> = sig
            .inputs
            .iter()
            .zip(args.iter())
            .map(|(ty, var)| format!("{} %{}", ty.llvm_type(), var))
            .collect();

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
        let fns = self.codegen_quotation(id, body, &quot_type)?;

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
            // Float is heap-boxed (needs clone), so only Int/Bool are trivially copyable.
            self.prev_stmt_is_trivial_literal = i > 0
                && matches!(
                    &statements[i - 1],
                    Statement::IntLiteral(_) | Statement::BoolLiteral(_)
                );

            // Track the actual int value if previous was IntLiteral (Issue #192)
            // This enables optimized roll/pick with constant N (e.g., `2 roll` -> rot)
            self.prev_stmt_int_value = if i > 0
                && let Statement::IntLiteral(n) = &statements[i - 1]
            {
                Some(*n)
            } else {
                None
            };

            let is_last = i == len - 1;
            let position = if is_last && last_is_tail {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };

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
            Statement::WordCall { name, span } => {
                self.codegen_word_call(stack_var, name, span.as_ref(), position)
            }
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

    /// Generate the C `main` that the linker invokes. Depending on
    /// `pure_inline_test`, either runs the Seq code directly or spawns it
    /// as the first strand on the scheduler.
    pub(super) fn codegen_main(&mut self) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.output,
            "define i32 @main(i32 %argc, ptr %argv) {{"
        )?;
        writeln!(&mut self.output, "entry:")?;

        if self.pure_inline_test {
            self.codegen_main_pure_inline()?;
        } else {
            self.codegen_main_scheduled()?;
        }

        writeln!(&mut self.output, "}}")?;
        Ok(())
    }

    /// Pure-inline-test main: no scheduler, no args, no report hook. Runs
    /// `seq_main` on a fresh tagged stack and returns the top-of-stack int
    /// as the process exit code. Only valid for programs that use inlined
    /// operations exclusively (integer arithmetic, stack ops) — there is
    /// no runtime coroutine context for non-blocking I/O.
    fn codegen_main_pure_inline(&mut self) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.output,
            "  %tagged_stack = call ptr @seq_stack_new_default()"
        )?;
        writeln!(
            &mut self.output,
            "  %stack_base = call ptr @seq_stack_base(ptr %tagged_stack)"
        )?;

        writeln!(
            &mut self.output,
            "  %final_sp = call ptr @seq_main(ptr %stack_base)"
        )?;

        let top_ptr = self.emit_stack_gep("final_sp", -1)?;
        let result = self.emit_load_int_payload(&top_ptr)?;

        writeln!(
            &mut self.output,
            "  call void @seq_stack_free(ptr %tagged_stack)"
        )?;

        writeln!(
            &mut self.output,
            "  %exit_code = trunc i64 %{} to i32",
            result
        )?;
        writeln!(&mut self.output, "  ret i32 %exit_code")?;
        Ok(())
    }

    /// Normal scheduled main: initialises argv, starts the scheduler, spawns
    /// `seq_main` as the first strand, waits for completion, runs the at-
    /// exit report hook, and returns whatever Seq wrote to the exit-code
    /// global (Issue #355 — defaults to 0 for void main).
    fn codegen_main_scheduled(&mut self) -> Result<(), CodeGenError> {
        // Initialize command-line arguments before scheduler so args are
        // available to any strand that spawns early.
        writeln!(
            &mut self.output,
            "  call void @patch_seq_args_init(i32 %argc, ptr %argv)"
        )?;

        writeln!(&mut self.output, "  call void @patch_seq_scheduler_init()")?;

        // --instrument: register per-word counters and name pointers with the report system.
        if self.instrument {
            let n = self.word_instrument_ids.len();
            writeln!(
                &mut self.output,
                "  call void @patch_seq_report_init(ptr @seq_word_counters, ptr @seq_word_names, i64 {})",
                n
            )?;
        }

        // Spawn user's main as the first strand so everything runs in
        // coroutine context (required for non-blocking I/O).
        writeln!(
            &mut self.output,
            "  %0 = call i64 @patch_seq_strand_spawn(ptr @seq_main, ptr null)"
        )?;
        writeln!(
            &mut self.output,
            "  %1 = call ptr @patch_seq_scheduler_run()"
        )?;

        // At-exit report hook (no-op unless SEQ_REPORT is set at runtime).
        writeln!(&mut self.output, "  call void @patch_seq_report()")?;

        // Truncate to i32 — Unix exit codes are limited to the low 8 bits
        // on Linux; other platforms vary. We pass through whatever the
        // user returned and let the OS apply its convention.
        let exit_code_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call i64 @patch_seq_get_exit_code()",
            exit_code_var
        )?;
        let exit_code_i32 = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = trunc i64 %{} to i32",
            exit_code_i32, exit_code_var
        )?;
        writeln!(&mut self.output, "  ret i32 %{}", exit_code_i32)?;
        Ok(())
    }
}
