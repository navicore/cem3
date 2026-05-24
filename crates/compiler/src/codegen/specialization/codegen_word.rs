//! The top-level specialized codegen: walking a word definition, dispatching
//! on statement kind, generating the `define … { … }` prologue, lowering
//! `if / else`, and emitting returns. The per-operation and per-call
//! emitters live in sibling files (`codegen_ops`, `codegen_safe_math`,
//! `codegen_calls`).
//!
//! All of the per-function work happens inside a [`SpecializedEmitter`],
//! a short-lived wrapper that owns the function-level constants
//! (`word_name`, `sig`) so they don't have to be threaded through every
//! helper signature. It derefs to the underlying [`CodeGen`], so
//! existing `self.output` / `self.fresh_temp()` access keeps working.

use super::CodeGen;
use super::context::RegisterContext;
use super::types::{RegisterType, SpecSignature};
use crate::ast::{Statement, WordDef};
use crate::codegen::CodeGenError;
use crate::codegen::mangle_name;
use std::fmt::Write as _;
use std::ops::{Deref, DerefMut};

/// Wraps a [`CodeGen`] borrow with the constants that stay fixed for
/// one specialized-function compilation. Methods on this type drop
/// `word_name`/`sig`/`is_last` from the parameter parade — they're
/// fields, not arguments.
///
/// `Deref<Target = CodeGen>` lets the borrowed codegen state (output
/// buffer, fresh-name counters, side tables) be reached transparently
/// from the emitter's methods, so the migration from `impl CodeGen` to
/// `impl SpecializedEmitter` is a parameter cleanup, not a state rewire.
pub(super) struct SpecializedEmitter<'a> {
    codegen: &'a mut CodeGen,
    word_name: &'a str,
    sig: &'a SpecSignature,
}

impl<'a> SpecializedEmitter<'a> {
    pub(super) fn new(
        codegen: &'a mut CodeGen,
        word_name: &'a str,
        sig: &'a SpecSignature,
    ) -> Self {
        Self {
            codegen,
            word_name,
            sig,
        }
    }

    /// The name of the word being compiled. Set once per emitter.
    pub(super) fn word_name(&self) -> &str {
        self.word_name
    }

    /// The specialization signature of the word being compiled. Set
    /// once per emitter.
    pub(super) fn sig(&self) -> &SpecSignature {
        self.sig
    }
}

impl Deref for SpecializedEmitter<'_> {
    type Target = CodeGen;
    fn deref(&self) -> &CodeGen {
        self.codegen
    }
}

impl DerefMut for SpecializedEmitter<'_> {
    fn deref_mut(&mut self) -> &mut CodeGen {
        self.codegen
    }
}

impl CodeGen {
    /// Generate a specialized version of a word.
    ///
    /// This creates a register-based function that passes values directly in
    /// CPU registers instead of through the tagged pointer stack.
    ///
    /// The generated function:
    /// - Takes primitive arguments directly (i64 for Int/Bool, double for Float)
    /// - Returns the result in a register (not via stack pointer)
    /// - Uses `musttail` for recursive calls to guarantee TCO
    /// - Handles control flow with phi nodes for value merging
    pub fn codegen_specialized_word(
        &mut self,
        word: &WordDef,
        sig: &SpecSignature,
    ) -> Result<(), CodeGenError> {
        SpecializedEmitter::new(self, &word.name, sig).emit_word(word)
    }
}

impl SpecializedEmitter<'_> {
    /// Emit the full specialized function: signature, entry block,
    /// statements, and register the word in `specialized_words`.
    fn emit_word(&mut self, word: &WordDef) -> Result<(), CodeGenError> {
        let base_name = format!("seq_{}", mangle_name(self.word_name));
        let spec_name = format!("{}{}", base_name, self.sig.suffix());

        // Generate function signature
        // For single output: define i64 @name(i64 %arg0) {
        // For multiple outputs: define { i64, i64 } @name(i64 %arg0, i64 %arg1) {
        let return_type = if self.sig.outputs.len() == 1 {
            self.sig.outputs[0].llvm_type().to_string()
        } else {
            let types: Vec<_> = self.sig.outputs.iter().map(|t| t.llvm_type()).collect();
            format!("{{ {} }}", types.join(", "))
        };

        let params: Vec<String> = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .map(|(i, ty)| format!("{} %arg{}", ty.llvm_type(), i))
            .collect();

        writeln!(
            &mut self.output,
            "define {} @{}({}) {{",
            return_type,
            spec_name,
            params.join(", ")
        )?;
        writeln!(&mut self.output, "entry:")?;

        let initial_params: Vec<(String, RegisterType)> = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .map(|(i, ty)| (format!("arg{}", i), *ty))
            .collect();
        let mut ctx = RegisterContext::from_params(&initial_params);

        let body_len = word.body.len();
        let mut prev_int_literal: Option<i64> = None;
        for (i, stmt) in word.body.iter().enumerate() {
            let is_last = i == body_len - 1;
            self.emit_statement(&mut ctx, stmt, is_last, &mut prev_int_literal)?;
        }

        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        // Record that this word is specialized. (Bind locals first: the
        // `insert` call needs `&mut self.specialized_words` via DerefMut,
        // which clashes with reading `self.word_name`/`self.sig` in the
        // same expression.)
        let key = self.word_name.to_string();
        let sig = self.sig.clone();
        self.specialized_words.insert(key, sig);

        Ok(())
    }

    /// Generate specialized code for a single statement.
    pub(super) fn emit_statement(
        &mut self,
        ctx: &mut RegisterContext,
        stmt: &Statement,
        is_last: bool,
        prev_int_literal: &mut Option<i64>,
    ) -> Result<(), CodeGenError> {
        // Track previous int literal for pick/roll optimization
        let prev_int = *prev_int_literal;
        *prev_int_literal = None; // Reset unless this is an IntLiteral

        match stmt {
            Statement::IntLiteral(n) => {
                let var = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 0, {}", var, n)?;
                ctx.push(var, RegisterType::I64);
                *prev_int_literal = Some(*n); // Track for next statement
            }

            Statement::FloatLiteral(f) => {
                let var = self.fresh_temp();
                // Use bitcast from integer bits for exact IEEE 754 representation.
                // This avoids precision loss from decimal string conversion (e.g., 0.1
                // cannot be exactly represented in binary floating point). By storing
                // the raw bit pattern and using bitcast, we preserve the exact value.
                let bits = f.to_bits();
                writeln!(
                    &mut self.output,
                    "  %{} = bitcast i64 {} to double",
                    var, bits
                )?;
                ctx.push(var, RegisterType::Double);
            }

            Statement::BoolLiteral(b) => {
                let var = self.fresh_temp();
                let val = if *b { 1 } else { 0 };
                writeln!(&mut self.output, "  %{} = add i64 0, {}", var, val)?;
                ctx.push(var, RegisterType::I64);
            }

            Statement::WordCall { name, .. } => {
                self.emit_word_call(ctx, name, is_last, prev_int)?;
            }

            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => {
                self.emit_if(ctx, then_branch, else_branch.as_deref(), is_last)?;
            }

            // These shouldn't appear in specializable words (checked in can_specialize)
            Statement::StringLiteral(_)
            | Statement::Symbol(_)
            | Statement::Quotation { .. }
            | Statement::Match { .. } => {
                return Err(CodeGenError::Logic(format!(
                    "Non-specializable statement in specialized word: {:?}",
                    stmt
                )));
            }
        }

        // Emit return if this is the last statement and it's not a control flow op
        // that already emits returns (like if, or recursive calls)
        let already_returns = match stmt {
            Statement::If { .. } => true,
            Statement::WordCall { name, .. } if name == self.word_name => true,
            _ => false,
        };
        if is_last && !already_returns {
            self.emit_return(ctx)?;
        }

        Ok(())
    }

    /// Emit return statement for specialized function.
    pub(super) fn emit_return(&mut self, ctx: &RegisterContext) -> Result<(), CodeGenError> {
        let output_count = self.sig.outputs.len();

        if output_count == 0 {
            writeln!(&mut self.output, "  ret void")?;
        } else if output_count == 1 {
            let (var, ty) = ctx
                .values
                .last()
                .ok_or_else(|| CodeGenError::Logic("Empty context at return".to_string()))?;
            writeln!(&mut self.output, "  ret {} %{}", ty.llvm_type(), var)?;
        } else {
            // Multi-output: build struct return.
            // Values in context are bottom-to-top, matching sig.outputs order.
            if ctx.values.len() < output_count {
                return Err(CodeGenError::Logic(format!(
                    "Not enough values for multi-output return: need {}, have {}",
                    output_count,
                    ctx.values.len()
                )));
            }

            let start_idx = ctx.values.len() - output_count;
            let return_values: Vec<_> = ctx.values[start_idx..].to_vec();

            let struct_type = self.sig.llvm_return_type();

            let mut current_struct = "undef".to_string();
            for (i, (var, ty)) in return_values.iter().enumerate() {
                let new_struct = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = insertvalue {} {}, {} %{}, {}",
                    new_struct,
                    struct_type,
                    current_struct,
                    ty.llvm_type(),
                    var,
                    i
                )?;
                current_struct = format!("%{}", new_struct);
            }

            writeln!(&mut self.output, "  ret {} {}", struct_type, current_struct)?;
        }
        Ok(())
    }

    /// Emit code for one branch of a specialized if-statement.
    ///
    /// Writes the branch's label, processes its statements on a cloned
    /// context, and either lets the branch's last statement emit the
    /// function's return (when `is_last`) or emits a `br` to
    /// `merge_label`.
    ///
    /// Returns `(branch_ctx, predecessor)` — `predecessor` is `Some` if
    /// the branch falls through to `merge_label` (and therefore feeds a
    /// phi node), `None` if it already returned.
    fn emit_branch(
        &mut self,
        parent_ctx: &RegisterContext,
        branch: &[Statement],
        branch_label: &str,
        merge_label: &str,
        is_last: bool,
    ) -> Result<(RegisterContext, Option<String>), CodeGenError> {
        writeln!(&mut self.output, "{}:", branch_label)?;
        let mut branch_ctx = parent_ctx.clone();
        let mut branch_prev_int: Option<i64> = None;
        for (i, stmt) in branch.iter().enumerate() {
            let is_stmt_last = i == branch.len() - 1 && is_last;
            self.emit_statement(&mut branch_ctx, stmt, is_stmt_last, &mut branch_prev_int)?;
        }
        // Empty branch (or no-else) needs its return emitted explicitly:
        // there was no last statement to do it via the in-statement path.
        if is_last && branch.is_empty() {
            self.emit_return(&branch_ctx)?;
        }
        let predecessor = if is_last {
            None
        } else {
            writeln!(&mut self.output, "  br label %{}", merge_label)?;
            Some(branch_label.to_string())
        };
        Ok((branch_ctx, predecessor))
    }

    /// Generate specialized if/else statement.
    pub(super) fn emit_if(
        &mut self,
        ctx: &mut RegisterContext,
        then_branch: &[Statement],
        else_branch: Option<&[Statement]>,
        is_last: bool,
    ) -> Result<(), CodeGenError> {
        let (cond_var, _) = ctx
            .pop()
            .ok_or_else(|| CodeGenError::Logic("Empty context at if condition".to_string()))?;

        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp_result, cond_var
        )?;

        let then_label = self.fresh_block("if_then");
        let else_label = self.fresh_block("if_else");
        let merge_label = self.fresh_block("if_merge");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_result, then_label, else_label
        )?;

        let (then_ctx, then_pred) =
            self.emit_branch(ctx, then_branch, &then_label, &merge_label, is_last)?;

        // None or empty else is the same shape from the helper's view.
        let else_slice: &[Statement] = else_branch.unwrap_or(&[]);
        let (else_ctx, else_pred) =
            self.emit_branch(ctx, else_slice, &else_label, &merge_label, is_last)?;

        // Merge block with phi nodes if either branch continues
        if then_pred.is_some() || else_pred.is_some() {
            writeln!(&mut self.output, "{}:", merge_label)?;

            if let (Some(then_p), Some(else_p)) = (&then_pred, &else_pred) {
                // Both branches continue - merge all values with phi nodes
                if then_ctx.values.len() != else_ctx.values.len() {
                    return Err(CodeGenError::Logic(format!(
                        "Stack depth mismatch in if branches: then has {}, else has {}",
                        then_ctx.values.len(),
                        else_ctx.values.len()
                    )));
                }

                ctx.values.clear();
                for i in 0..then_ctx.values.len() {
                    let (then_var, then_ty) = &then_ctx.values[i];
                    let (else_var, else_ty) = &else_ctx.values[i];

                    if then_ty != else_ty {
                        return Err(CodeGenError::Logic(format!(
                            "Type mismatch at position {} in if branches: {:?} vs {:?}",
                            i, then_ty, else_ty
                        )));
                    }

                    if then_var == else_var {
                        ctx.push(then_var.clone(), *then_ty);
                    } else {
                        let phi_result = self.fresh_temp();
                        writeln!(
                            &mut self.output,
                            "  %{} = phi {} [ %{}, %{} ], [ %{}, %{} ]",
                            phi_result,
                            then_ty.llvm_type(),
                            then_var,
                            then_p,
                            else_var,
                            else_p
                        )?;
                        ctx.push(phi_result, *then_ty);
                    }
                }
            } else if then_pred.is_some() {
                *ctx = then_ctx;
            } else {
                *ctx = else_ctx;
            }

            if is_last && (then_pred.is_some() || else_pred.is_some()) {
                self.emit_return(ctx)?;
            }
        }

        Ok(())
    }
}
