//! Word and Quotation Code Generation
//!
//! This module handles generating LLVM IR for word definitions,
//! quotations (closures), and tail call optimization.

use super::{
    BUILTIN_SYMBOLS, BranchResult, CodeGen, CodeGenError, QuotationFunctions, QuotationScope,
    TailPosition, UNREACHABLE_PREDECESSOR, VirtualValue, mangle_name,
};
use crate::ast::{Statement, WordDef};
use crate::types::Type;
use std::fmt::Write as _;

impl CodeGen {
    /// Generate code for a word definition
    pub(super) fn codegen_word(&mut self, word: &WordDef) -> Result<(), CodeGenError> {
        // Detect the loop-lowering pattern up front. A loop-lowered word keeps
        // its stack-based `seq_<name>` function but the body becomes a native
        // loop instead of a musttail recursion. Specialization is independent:
        // if the word is also specializable, BOTH paths are emitted and the
        // call-site dispatcher picks the specialized (register) variant when
        // types match. Loop-lowering only affects the stack-based fallback,
        // so it never competes with specialization (see
        // docs/design/LOOP_LOWERING.md).
        let loop_pattern = if self.loop_opt_enabled {
            self.call_graph
                .as_ref()
                .and_then(|cg| super::loop_lowering::detect_loop_pattern(word, cg))
        } else {
            None
        };

        // Try to generate a specialized register-based version first.
        if let Some(sig) = self.can_specialize(word) {
            self.codegen_specialized_word(word, &sig)?;
        }

        // Always generate the stack-based version for compatibility
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

        // Open a DISubprogram so backtraces resolve to .seq:line. Anchor
        // at the word's source line if known, else line 0.
        let dbg_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);
        let dbg_attr = self.dbg_open_subprogram(&word.name, dbg_line);

        if is_main {
            writeln!(
                &mut self.output,
                "define ptr @{}(ptr %stack){} {{",
                function_name, dbg_attr
            )?;
        } else {
            writeln!(
                &mut self.output,
                "define tailcc ptr @{}(ptr %stack){}{} {{",
                function_name, inline_attr, dbg_attr
            )?;
        }
        writeln!(&mut self.output, "entry:")?;

        // For main (non-pure-inline): allocate the tagged stack and get base pointer
        // In pure_inline_test mode, main() allocates the stack, so seq_main just uses %stack
        let mut stack_var = if is_main && !self.pure_inline_test {
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

        // Allocate aux stack slots if this word uses >aux/aux> (Issue #350)
        self.current_aux_sp = 0;
        let aux_slot_count = self.aux_slot_counts.get(&word.name).copied().unwrap_or(0);
        self.emit_aux_slots(aux_slot_count)?;

        // Emit instrumentation counter increment (--instrument)
        if let Some(&word_id) = self.word_instrument_ids.get(&word.name) {
            let n = self.word_instrument_ids.len();
            writeln!(
                &mut self.output,
                "  %instr_ptr_{0} = getelementptr [{1} x i64], ptr @seq_word_counters, i64 0, i64 {0}",
                word_id, n
            )?;
            writeln!(
                &mut self.output,
                "  %instr_old_{0} = atomicrmw add ptr %instr_ptr_{0}, i64 1 monotonic",
                word_id
            )?;
        }

        // Set current word for type-specialized optimizations (Issue #186)
        self.current_word_name = Some(word.name.clone());
        self.current_stmt_index = 0;

        // Loop lowering (--loop-opt): emit a native LLVM loop when the word
        // matched the Phase 1 pattern (detected at the top of codegen_word).
        // `is_main` is already excluded by detect_loop_pattern.
        let lowered_as_loop = loop_pattern.is_some();
        if let Some(pattern) = loop_pattern {
            self.codegen_loop_body(&stack_var, &pattern)?;
        } else {
            // Generate code for all statements with pattern detection for inline loops
            stack_var = self.codegen_statements(&word.body, &stack_var, true)?;
        }

        self.current_word_name = None;

        // Only emit ret if the last statement wasn't a tail call
        // (tail calls emit their own ret). A loop-lowered word emits its own
        // ret from the base-case block, so skip this entirely.
        if !lowered_as_loop
            && !word
                .body
                .last()
                .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail))
        {
            // Spill any remaining virtual registers before return (Issue #189)
            let stack_var = self.spill_virtual_stack(&stack_var)?;

            if is_main && !self.pure_inline_test {
                // Normal mode: maybe write exit code, then free stack and return.
                //
                // For `main ( -- Int )`: peek the top Int from the stack and
                // write it to the global exit code via patch_seq_set_exit_code,
                // BEFORE freeing the stack. The C `main` reads it after the
                // scheduler joins all strands. (Issue #355)
                if self.main_returns_int {
                    let top_ptr = self.emit_stack_gep(&stack_var, -1)?;
                    let exit_val = self.emit_load_int_payload(&top_ptr)?;
                    writeln!(
                        &mut self.output,
                        "  call void @patch_seq_set_exit_code(i64 %{})",
                        exit_val
                    )?;
                }
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

        self.dbg_close_subprogram();
        self.inside_main = false;
        Ok(())
    }

    /// Generate a quotation function.
    /// Returns wrapper and impl function names for TCO support.
    pub(super) fn codegen_quotation(
        &mut self,
        quot_id: usize,
        body: &[Statement],
        quot_type: &Type,
    ) -> Result<QuotationFunctions, CodeGenError> {
        let base_name = format!("seq_quot_{}", self.quot_counter);
        self.quot_counter += 1;

        // Each quotation is a standalone LLVM function with its own output
        // buffer, virtual stack, and aux-slot table. Save the enclosing
        // scope so everything emitted between here and `exit_quotation_scope`
        // accumulates into `self.output` in isolation.
        let scope = self.enter_quotation_scope();

        // Zero for quotations that don't use >aux/aux> (Issue #393).
        let aux_slot_count = self
            .quotation_aux_slot_counts
            .get(&quot_id)
            .copied()
            .unwrap_or(0);

        let result = match quot_type {
            Type::Quotation(_) => {
                self.emit_stateless_quotation_fns(&base_name, body, aux_slot_count)
            }
            Type::Closure { captures, .. } => {
                self.emit_closure_fn(&base_name, body, captures, aux_slot_count)
            }
            _ => Err(CodeGenError::Logic(format!(
                "CodeGen: expected Quotation or Closure type, got {:?}",
                quot_type
            ))),
        };

        self.exit_quotation_scope(scope);
        result
    }

    /// Emit a stateless quotation as a wrapper + impl pair.
    ///
    /// - `impl_` is `tailcc` and holds the actual body; it can be a `musttail`
    ///   target from other user words.
    /// - `wrapper` is C-convention and calls `impl_`. It's what gets stored
    ///   in a `Quotation` value so the runtime can invoke it without knowing
    ///   about `tailcc`.
    fn emit_stateless_quotation_fns(
        &mut self,
        base_name: &str,
        body: &[Statement],
        aux_slot_count: usize,
    ) -> Result<QuotationFunctions, CodeGenError> {
        let wrapper_name = base_name.to_string();
        let impl_name = format!("{}_impl", base_name);

        // Impl (tailcc, can be a musttail target)
        writeln!(
            &mut self.output,
            "define tailcc ptr @{}(ptr %stack) {{",
            impl_name
        )?;
        writeln!(&mut self.output, "entry:")?;
        self.emit_aux_slots(aux_slot_count)?;
        self.emit_body_with_tail_ret(body, "stack")?;
        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        // Wrapper (C-convention, thin trampoline to impl)
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

        Ok(QuotationFunctions {
            wrapper: wrapper_name,
            impl_: impl_name,
        })
    }

    /// Emit a closure function (the quotation variant that captures values
    /// from its creation site). Closures take `(stack, env_data, env_len)`
    /// and cannot use `tailcc` / `musttail` yet, so `inside_closure` is set
    /// to disable tail-call optimisation for statements inside the body.
    fn emit_closure_fn(
        &mut self,
        base_name: &str,
        body: &[Statement],
        captures: &[Type],
        aux_slot_count: usize,
    ) -> Result<QuotationFunctions, CodeGenError> {
        self.inside_closure = true;

        writeln!(
            &mut self.output,
            "define ptr @{}(ptr %stack, ptr %env_data, i64 %env_len) {{",
            base_name
        )?;
        writeln!(&mut self.output, "entry:")?;
        self.emit_aux_slots(aux_slot_count)?;

        // Push captured values onto the stack before executing body.
        // Captures are stored bottom-to-top, so push them in index order.
        let mut stack_var = "stack".to_string();
        for (index, capture_type) in captures.iter().enumerate() {
            stack_var = self.emit_capture_push(capture_type, index, &stack_var)?;
        }

        self.emit_body_with_tail_ret(body, &stack_var)?;
        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        self.inside_closure = false;

        // Closures have no separate impl; wrapper == impl.
        Ok(QuotationFunctions {
            wrapper: base_name.to_string(),
            impl_: base_name.to_string(),
        })
    }

    /// Save the enclosing function's mutable codegen state (output buffer,
    /// virtual stack, current word name, aux-slot table, aux SP) and reset
    /// the aux SP to 0 so the nested function starts fresh. Call
    /// `exit_quotation_scope` with the returned guard to commit the nested
    /// function into `quotation_functions` and restore the enclosing state.
    fn enter_quotation_scope(&mut self) -> QuotationScope {
        let scope = QuotationScope {
            output: std::mem::take(&mut self.output),
            virtual_stack: std::mem::take(&mut self.virtual_stack),
            word_name: self.current_word_name.take(),
            aux_slots: std::mem::take(&mut self.current_aux_slots),
            aux_sp: self.current_aux_sp,
            dbg_subprogram_id: self.current_dbg_subprogram_id.take(),
        };
        self.current_aux_sp = 0;
        scope
    }

    /// Commit the nested function's emitted IR into `quotation_functions`
    /// and restore the enclosing codegen scope.
    fn exit_quotation_scope(&mut self, scope: QuotationScope) {
        self.quotation_functions.push_str(&self.output);
        self.output = scope.output;
        self.virtual_stack = scope.virtual_stack;
        self.current_word_name = scope.word_name;
        self.current_aux_slots = scope.aux_slots;
        self.current_aux_sp = scope.aux_sp;
        self.current_dbg_subprogram_id = scope.dbg_subprogram_id;
    }

    /// Walk a function body, emitting each statement with the last in tail
    /// position; then, if the final statement didn't already emit a
    /// terminator (`musttail … ret`), spill the virtual stack and emit a
    /// plain `ret ptr %<stack>`.
    fn emit_body_with_tail_ret(
        &mut self,
        body: &[Statement],
        initial_stack: &str,
    ) -> Result<(), CodeGenError> {
        let mut stack_var = initial_stack.to_string();
        let body_len = body.len();
        for (i, statement) in body.iter().enumerate() {
            let position = if i == body_len - 1 {
                TailPosition::Tail
            } else {
                TailPosition::NonTail
            };
            stack_var = self.codegen_statement(&stack_var, statement, position)?;
        }
        if !body
            .last()
            .is_some_and(|s| self.will_emit_tail_call(s, TailPosition::Tail))
        {
            let stack_var = self.spill_virtual_stack(&stack_var)?;
            writeln!(&mut self.output, "  ret ptr %{}", stack_var)?;
        }
        Ok(())
    }

    /// Check if a name refers to a runtime builtin (not a user-defined word).
    pub(super) fn is_runtime_builtin(&self, name: &str) -> bool {
        BUILTIN_SYMBOLS.contains_key(name)
            || self.external_builtins.contains_key(name)
            || self.ffi_bindings.is_ffi_function(name)
    }

    /// Emit `alloca %Value` slots for the aux stack and populate
    /// `current_aux_slots` with their LLVM names.
    ///
    /// Used by `codegen_word`, the quotation arm of `codegen_quotation`,
    /// and the closure arm of `codegen_quotation`. Each function (word,
    /// quotation, or closure) gets its own independent slot table — they
    /// are never shared across function boundaries.
    ///
    /// Caller is responsible for resetting `current_aux_sp` if needed.
    pub(super) fn emit_aux_slots(&mut self, count: usize) -> Result<(), CodeGenError> {
        self.current_aux_slots.clear();
        for i in 0..count {
            let slot_name = format!("aux_slot_{}", i);
            writeln!(&mut self.output, "  %{} = alloca %Value", slot_name)?;
            self.current_aux_slots.push(slot_name);
        }
        Ok(())
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
            // All other types (Variant, Map, Union, Symbol, Channel, type
            // variables that resolved to non-primitive types) use the generic
            // combined get+push that works for any Value. This avoids passing
            // Value by value through the FFI boundary.
            _ => {
                let new_stack_var = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_env_push_value(ptr %{}, ptr %env_data, i64 %env_len, i32 {})",
                    new_stack_var, stack_var, index
                )?;
                return Ok(new_stack_var);
            }
        };

        let value_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call {} @{}(ptr %env_data, i64 %env_len, i32 {})",
            value_var, getter_type, getter, index
        )?;

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

        // Save aux stack pointer for this branch (Issue #350)
        let saved_aux_sp = self.current_aux_sp;

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

        // Restore virtual stack, depth, and aux stack pointer (Issue #189, #350)
        self.virtual_stack = saved_virtual_stack;
        self.current_aux_sp = saved_aux_sp;
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
            Statement::WordCall { name, span } => {
                // Phase 2 TCO: `call` is now TCO-eligible (it emits its own ret)
                if name == "call" {
                    return true;
                }
                // Arithmetic sugar ops resolve to inline builtins — they don't emit tail calls
                if let Some(s) = span
                    && self.resolve_sugar_at(s.line, s.column).is_some()
                {
                    return false;
                }
                !self.is_runtime_builtin(name)
            }
            Statement::If {
                then_branch,
                else_branch,
                span: _,
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
        let ssa_var = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = add i64 0, {}", ssa_var, n)?;

        // Push to virtual stack (may spill if at capacity)
        let value = VirtualValue::Int { ssa_var, value: n };
        self.push_virtual(value, stack_var)
    }

    /// Generate code for a float literal: ( -- f )
    ///
    /// Uses LLVM's hexadecimal floating point format for exact representation.
    /// Handles special values (NaN, Infinity) explicitly.
    pub(super) fn codegen_float_literal(
        &mut self,
        stack_var: &str,
        f: f64,
    ) -> Result<String, CodeGenError> {
        let ssa_var = self.fresh_temp();
        let float_bits = f.to_bits();
        writeln!(
            &mut self.output,
            "  %{} = bitcast i64 {} to double",
            ssa_var, float_bits
        )?;

        // Push to virtual stack (may spill if at capacity)
        let value = VirtualValue::Float { ssa_var };
        self.push_virtual(value, stack_var)
    }

    /// Generate code for a boolean literal: ( -- b )
    ///
    /// Bools are stored as i64 values (0 for false, 1 for true) and pushed
    /// to the virtual stack for potential specialized dispatch.
    pub(super) fn codegen_bool_literal(
        &mut self,
        stack_var: &str,
        b: bool,
    ) -> Result<String, CodeGenError> {
        let ssa_var = self.fresh_temp();
        let val = if b { 1 } else { 0 };
        writeln!(&mut self.output, "  %{} = add i64 0, {}", ssa_var, val)?;

        // Push to virtual stack (may spill if at capacity)
        let value = VirtualValue::Bool { ssa_var };
        self.push_virtual(value, stack_var)
    }

    /// Generate code for a string literal: ( -- s )
    ///
    /// String literals are byte-clean — the payload is a `&[u8]`, which
    /// may contain embedded NULs or arbitrary high-bit bytes. We pass an
    /// explicit `(ptr, len)` to `patch_seq_push_string_bytes` so embedded
    /// NULs survive (the C-string-based `push_string` would truncate at
    /// the first NUL).
    pub(super) fn codegen_string_literal(
        &mut self,
        stack_var: &str,
        bytes: &[u8],
    ) -> Result<String, CodeGenError> {
        // Spill virtual values before calling runtime (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;

        let global = self.get_string_global(bytes)?;
        let ptr_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr inbounds [{} x i8], ptr {}, i32 0, i32 0",
            ptr_temp,
            bytes.len() + 1,
            global
        )?;
        let result_var = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_string_bytes(ptr %{}, ptr %{}, i64 {})",
            result_var,
            stack_var,
            ptr_temp,
            bytes.len()
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
                    span: _,
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
                Statement::Match { arms, span: _ } => {
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
