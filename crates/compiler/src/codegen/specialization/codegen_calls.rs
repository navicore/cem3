//! Recursive-self and cross-word call lowering for specialized codegen.
//! Both emit an LLVM `call`, pop arguments from the register context, and
//! push the result(s). The recursive path additionally uses `musttail` to
//! guarantee TCO for tail positions.

use super::codegen_word::SpecializedEmitter;
use super::context::RegisterContext;
use super::types::SpecSignature;
use crate::codegen::CodeGenError;
use crate::codegen::mangle_name;
use std::fmt::Write as _;

impl SpecializedEmitter<'_> {
    /// Emit a recursive call to the specialized version of the current word.
    ///
    /// Uses `musttail` when `is_tail` is true to guarantee tail call optimization.
    /// This is critical for recursive algorithms like `fib` or `count-down` that
    /// would otherwise overflow the call stack.
    pub(super) fn emit_recursive_call(
        &mut self,
        ctx: &mut RegisterContext,
        is_tail: bool,
    ) -> Result<(), CodeGenError> {
        let sig = self.sig().clone();
        let spec_name = format!("seq_{}{}", mangle_name(self.word_name()), sig.suffix());

        if ctx.values.len() < sig.inputs.len() {
            return Err(CodeGenError::Logic(format!(
                "Not enough values in context for recursive call to {}: need {}, have {}",
                self.word_name(),
                sig.inputs.len(),
                ctx.values.len()
            )));
        }

        self.emit_specialized_call(ctx, &spec_name, &sig, is_tail)
    }

    /// Emit a call to another specialized word.
    pub(super) fn emit_word_dispatch(
        &mut self,
        ctx: &mut RegisterContext,
        name: &str,
    ) -> Result<(), CodeGenError> {
        let sig = self
            .specialized_words
            .get(name)
            .ok_or_else(|| CodeGenError::Logic(format!("Unknown specialized word: {}", name)))?
            .clone();

        let spec_name = format!("seq_{}{}", mangle_name(name), sig.suffix());

        self.emit_specialized_call(ctx, &spec_name, &sig, false)
    }

    /// Emit an LLVM call to specialized function `spec_name` with signature
    /// `sig`: pop its inputs from the register context, emit the call, then
    /// either `musttail`+`ret` (tail position) or push the result(s) back.
    fn emit_specialized_call(
        &mut self,
        ctx: &mut RegisterContext,
        spec_name: &str,
        sig: &SpecSignature,
        is_tail: bool,
    ) -> Result<(), CodeGenError> {
        let mut args = Vec::new();
        for _ in 0..sig.inputs.len() {
            args.push(ctx.pop().unwrap());
        }
        args.reverse();

        let arg_strs: Vec<String> = args
            .iter()
            .map(|(var, ty)| format!("{} %{}", ty.llvm_type(), var))
            .collect();

        let return_type = sig.llvm_return_type();
        let result = self.fresh_temp();

        if is_tail {
            // Tail call - use musttail for guaranteed TCO
            writeln!(
                &mut self.output,
                "  %{} = musttail call {} @{}({})",
                result,
                return_type,
                spec_name,
                arg_strs.join(", ")
            )?;
            writeln!(&mut self.output, "  ret {} %{}", return_type, result)?;
        } else {
            writeln!(
                &mut self.output,
                "  %{} = call {} @{}({})",
                result,
                return_type,
                spec_name,
                arg_strs.join(", ")
            )?;

            if sig.outputs.len() == 1 {
                ctx.push(result, sig.outputs[0]);
            } else {
                for (i, out_ty) in sig.outputs.iter().enumerate() {
                    let extracted = self.fresh_temp();
                    writeln!(
                        &mut self.output,
                        "  %{} = extractvalue {} %{}, {}",
                        extracted, return_type, result, i
                    )?;
                    ctx.push(extracted, *out_ty);
                }
            }
        }

        Ok(())
    }
}
