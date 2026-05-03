//! The per-operation dispatch for specialized codegen: `dup`/`swap`/…,
//! integer and float arithmetic, bitwise ops, LLVM-intrinsic bit counting,
//! boolean logic, and comparisons. Safe-division/shift and recursive / cross-
//! word calls live in `codegen_safe_math` and `codegen_calls`.

use super::CodeGen;
use super::context::RegisterContext;
use super::types::{RegisterType, SpecSignature};
use crate::codegen::CodeGenError;
use std::fmt::Write as _;

impl CodeGen {
    /// Dispatch a single word call in specialized mode.
    pub(super) fn codegen_specialized_word_call(
        &mut self,
        ctx: &mut RegisterContext,
        name: &str,
        word_name: &str,
        sig: &SpecSignature,
        is_last: bool,
        prev_int: Option<i64>,
    ) -> Result<(), CodeGenError> {
        match name {
            // Stack operations - just manipulate the context
            "dup" => ctx.dup(),
            "drop" => ctx.drop(),
            "swap" => ctx.swap(),
            "over" => ctx.over(),
            "rot" => ctx.rot(),
            "nip" => {
                // ( a b -- b )
                ctx.swap();
                ctx.drop();
            }
            "tuck" => {
                // ( a b -- b a b )
                ctx.dup();
                let b = ctx.pop().unwrap();
                let b2 = ctx.pop().unwrap();
                let a = ctx.pop().unwrap();
                ctx.push(b.0, b.1);
                ctx.push(a.0, a.1);
                ctx.push(b2.0, b2.1);
            }
            "pick" => {
                // pick requires constant N from previous IntLiteral
                // ( ... xn ... x0 n -- ... xn ... x0 xn )
                let n = prev_int.ok_or_else(|| {
                    CodeGenError::Logic("pick requires constant N in specialized mode".to_string())
                })?;
                if n < 0 {
                    return Err(CodeGenError::Logic(format!(
                        "pick requires non-negative N, got {}",
                        n
                    )));
                }
                let n = n as usize;
                // Pop the N value (it was pushed by the IntLiteral)
                ctx.pop();
                // Now copy the value at depth n
                let len = ctx.values.len();
                if n >= len {
                    return Err(CodeGenError::Logic(format!(
                        "pick {} but only {} values in context",
                        n, len
                    )));
                }
                let (var, ty) = ctx.values[len - 1 - n].clone();
                ctx.push(var, ty);
            }
            "roll" => {
                // roll requires constant N from previous IntLiteral
                // ( ... xn xn-1 ... x0 n -- ... xn-1 ... x0 xn )
                let n = prev_int.ok_or_else(|| {
                    CodeGenError::Logic("roll requires constant N in specialized mode".to_string())
                })?;
                if n < 0 {
                    return Err(CodeGenError::Logic(format!(
                        "roll requires non-negative N, got {}",
                        n
                    )));
                }
                let n = n as usize;
                // Pop the N value (it was pushed by the IntLiteral)
                ctx.pop();
                // Now rotate: move value at depth n to top
                let len = ctx.values.len();
                if n >= len {
                    return Err(CodeGenError::Logic(format!(
                        "roll {} but only {} values in context",
                        n, len
                    )));
                }
                if n > 0 {
                    let val = ctx.values.remove(len - 1 - n);
                    ctx.values.push(val);
                }
                // n=0 is a no-op (value already at top)
            }

            // Integer arithmetic - uses LLVM's default wrapping behavior (no nsw/nuw flags).
            // This matches the runtime's wrapping_add/sub/mul semantics for defined overflow.
            "i.+" | "i.add" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = add i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "i.-" | "i.subtract" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = sub i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "i.*" | "i.multiply" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = mul i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "i./" | "i.divide" => {
                self.emit_specialized_safe_div(ctx, "sdiv")?;
            }
            "i.%" | "i.mod" => {
                self.emit_specialized_safe_div(ctx, "srem")?;
            }

            // Bitwise operations
            "band" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = and i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "bor" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = or i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "bxor" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "bnot" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, -1", result, a)?;
                ctx.push(result, RegisterType::I64);
            }
            "shl" => {
                self.emit_specialized_safe_shift(ctx, true)?;
            }
            "shr" => {
                self.emit_specialized_safe_shift(ctx, false)?;
            }

            // Bit counting operations — 63-bit-aware via shared helpers.
            // See codegen/bitwise_i63.rs and the Bitwise Operations section
            // of docs/language-guide.md for the 63-bit Int contract.
            "popcount" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.emit_popcount_i63(&a)?;
                ctx.push(result, RegisterType::I64);
            }
            "clz" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.emit_clz_i63(&a)?;
                ctx.push(result, RegisterType::I64);
            }
            "ctz" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.emit_ctz_i63(&a)?;
                ctx.push(result, RegisterType::I64);
            }

            // Type conversions
            "int->float" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = sitofp i64 %{} to double",
                    result, a
                )?;
                ctx.push(result, RegisterType::Double);
            }
            "float->int" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = fptosi double %{} to i64",
                    result, a
                )?;
                ctx.push(result, RegisterType::I64);
            }

            // Boolean logical operations (Bool is i64 0 or 1)
            "and" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = and i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "or" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = or i64 %{}, %{}", result, a, b)?;
                ctx.push(result, RegisterType::I64);
            }
            "not" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(&mut self.output, "  %{} = xor i64 %{}, 1", result, a)?;
                ctx.push(result, RegisterType::I64);
            }

            // Integer comparisons - return i64 0 or 1 (like Bool)
            "i.<" | "i.lt" => self.emit_specialized_icmp(ctx, "slt")?,
            "i.>" | "i.gt" => self.emit_specialized_icmp(ctx, "sgt")?,
            "i.<=" | "i.lte" => self.emit_specialized_icmp(ctx, "sle")?,
            "i.>=" | "i.gte" => self.emit_specialized_icmp(ctx, "sge")?,
            "i.=" | "i.eq" => self.emit_specialized_icmp(ctx, "eq")?,
            "i.<>" | "i.neq" => self.emit_specialized_icmp(ctx, "ne")?,

            // Float arithmetic
            "f.+" | "f.add" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = fadd double %{}, %{}",
                    result, a, b
                )?;
                ctx.push(result, RegisterType::Double);
            }
            "f.-" | "f.subtract" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = fsub double %{}, %{}",
                    result, a, b
                )?;
                ctx.push(result, RegisterType::Double);
            }
            "f.*" | "f.multiply" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = fmul double %{}, %{}",
                    result, a, b
                )?;
                ctx.push(result, RegisterType::Double);
            }
            "f./" | "f.divide" => {
                let (b, _) = ctx.pop().unwrap();
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = fdiv double %{}, %{}",
                    result, a, b
                )?;
                ctx.push(result, RegisterType::Double);
            }

            // Float comparisons - return i64 0 or 1 (like Bool)
            "f.<" | "f.lt" => self.emit_specialized_fcmp(ctx, "olt")?,
            "f.>" | "f.gt" => self.emit_specialized_fcmp(ctx, "ogt")?,
            "f.<=" | "f.lte" => self.emit_specialized_fcmp(ctx, "ole")?,
            "f.>=" | "f.gte" => self.emit_specialized_fcmp(ctx, "oge")?,
            "f.=" | "f.eq" => self.emit_specialized_fcmp(ctx, "oeq")?,
            "f.<>" | "f.neq" => self.emit_specialized_fcmp(ctx, "one")?,

            // Recursive call to self
            _ if name == word_name => {
                self.emit_specialized_recursive_call(ctx, word_name, sig, is_last)?;
            }

            // Call to another specialized word
            _ if self.specialized_words.contains_key(name) => {
                self.emit_specialized_word_dispatch(ctx, name)?;
            }

            _ => {
                return Err(CodeGenError::Logic(format!(
                    "Unhandled operation in specialized codegen: {}",
                    name
                )));
            }
        }
        Ok(())
    }

    /// Emit a specialized integer comparison.
    fn emit_specialized_icmp(
        &mut self,
        ctx: &mut RegisterContext,
        cmp_op: &str,
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap();
        let (a, _) = ctx.pop().unwrap();
        let cmp_result = self.fresh_temp();
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp {} i64 %{}, %{}",
            cmp_result, cmp_op, a, b
        )?;
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            result, cmp_result
        )?;
        ctx.push(result, RegisterType::I64);
        Ok(())
    }

    /// Emit a specialized float comparison.
    fn emit_specialized_fcmp(
        &mut self,
        ctx: &mut RegisterContext,
        cmp_op: &str,
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap();
        let (a, _) = ctx.pop().unwrap();
        let cmp_result = self.fresh_temp();
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = fcmp {} double %{}, %{}",
            cmp_result, cmp_op, a, b
        )?;
        writeln!(
            &mut self.output,
            "  %{} = zext i1 %{} to i64",
            result, cmp_result
        )?;
        ctx.push(result, RegisterType::I64);
        Ok(())
    }
}
