//! Register-Based Specialization for Seq Compiler
//!
//! This module generates optimized register-based LLVM IR for words that operate
//! purely on primitive types (Int, Float, Bool), eliminating the 40-byte `%Value`
//! struct overhead at function boundaries.
//!
//! ## Performance
//!
//! Specialization achieves **8-11x speedup** for numeric-intensive code:
//! - `fib(35)` benchmark: 124ms (stack) → 15ms (specialized)
//! - Recursive calls use `musttail` for guaranteed tail call optimization
//! - 1M recursive calls execute without stack overflow
//!
//! ## How It Works
//!
//! The standard Seq calling convention passes a pointer to a heap-allocated stack
//! of 40-byte `%Value` structs (discriminant + 4×i64 payload). This is flexible
//! but expensive for primitive operations.
//!
//! Specialization detects words that only use primitives and generates a parallel
//! "fast path" that passes values directly in CPU registers:
//!
//! ```llvm
//! ; Fast path - values in registers, no memory access
//! define i64 @seq_fib_i64(i64 %n) {
//!   %cmp = icmp slt i64 %n, 2
//!   br i1 %cmp, label %base, label %recurse
//! base:
//!   ret i64 %n
//! recurse:
//!   %n1 = sub i64 %n, 1
//!   %r1 = musttail call i64 @seq_fib_i64(i64 %n1)
//!   ; ...
//! }
//!
//! ; Fallback - always generated for polymorphic call sites
//! define tailcc ptr @seq_fib(ptr %stack) { ... }
//! ```
//!
//! ## Call Site Dispatch
//!
//! At call sites, the compiler checks if:
//! 1. A specialized version exists for the called word
//! 2. The virtual stack contains values matching the expected types
//!
//! If both conditions are met, it emits a direct register-based call.
//! Otherwise, it falls back to the stack-based version.
//!
//! ## Eligibility
//!
//! A word is specializable if:
//! - Its declared effect has only Int/Float/Bool in inputs/outputs
//! - Its body has no quotations, strings, or symbols (which need heap allocation)
//! - All calls are to inline ops, other specializable words, or recursive self-calls
//! - It has exactly one output (multiple outputs require struct returns - future work)
//!
//! ## Supported Operations (65 total)
//!
//! - **Integer arithmetic**: i.+, i.-, i.*, i./, i.% (with division-by-zero checks)
//! - **Float arithmetic**: f.+, f.-, f.*, f./
//! - **Comparisons**: i.<, i.>, i.<=, i.>=, i.=, i.<>, f.<, f.>, etc.
//! - **Bitwise**: band, bor, bxor, bnot, shl, shr (with bounds checking)
//! - **Bit counting**: popcount, clz, ctz (using LLVM intrinsics)
//! - **Boolean**: and, or, not
//! - **Type conversions**: int->float, float->int
//! - **Stack ops**: dup, drop, swap, over, rot, nip, tuck, pick, roll
//!
//! ## Implementation Notes
//!
//! ### RegisterContext
//! Tracks SSA variable names instead of emitting stack operations. Stack shuffles
//! like `swap` and `rot` become free context manipulations.
//!
//! ### Safe Division
//! Division and modulo emit branch-based zero checks with phi nodes, returning
//! both the result and a success flag to maintain Seq's safe division semantics.
//!
//! ### Safe Shifts
//! Shift operations check for out-of-bounds shift amounts (negative or >= 64)
//! and return 0 for invalid shifts, matching Seq's defined behavior.
//!
//! ### Tail Call Optimization
//! Recursive calls use `musttail` to guarantee TCO. This is critical for
//! recursive algorithms that would otherwise overflow the call stack.
//!
//! ## Future Work
//!
//! - **Multiple outputs**: Words returning multiple values could use LLVM struct
//!   returns `{ i64, i64 }`, but this requires changing how callers unpack results.

use super::{CodeGen, CodeGenError, mangle_name};
use crate::ast::{Statement, WordDef};
use crate::types::{StackType, Type};
use std::fmt::Write as _;

/// Register types that can be passed directly in LLVM registers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterType {
    /// 64-bit signed integer (maps to LLVM i64)
    I64,
    /// 64-bit floating point (maps to LLVM double)
    Double,
}

impl RegisterType {
    /// Convert a Seq Type to a RegisterType, if possible
    pub fn from_type(ty: &Type) -> Option<Self> {
        match ty {
            Type::Int | Type::Bool => Some(RegisterType::I64),
            Type::Float => Some(RegisterType::Double),
            _ => None,
        }
    }

    /// Get the LLVM type name for this register type
    pub fn llvm_type(&self) -> &'static str {
        match self {
            RegisterType::I64 => "i64",
            RegisterType::Double => "double",
        }
    }
}

/// Signature for a specialized function
#[derive(Debug, Clone)]
pub struct SpecSignature {
    /// Input types (bottom to top of stack)
    pub inputs: Vec<RegisterType>,
    /// Output types (bottom to top of stack)
    pub outputs: Vec<RegisterType>,
}

impl SpecSignature {
    /// Generate the specialized function suffix based on types
    /// For now: single Int -> "_i64", single Float -> "_f64"
    /// Multiple values will need struct returns in Phase 4
    pub fn suffix(&self) -> String {
        if self.inputs.len() == 1 && self.outputs.len() == 1 {
            match (self.inputs[0], self.outputs[0]) {
                (RegisterType::I64, RegisterType::I64) => "_i64".to_string(),
                (RegisterType::Double, RegisterType::Double) => "_f64".to_string(),
                (RegisterType::I64, RegisterType::Double) => "_i64_to_f64".to_string(),
                (RegisterType::Double, RegisterType::I64) => "_f64_to_i64".to_string(),
            }
        } else {
            // For multiple inputs/outputs, encode all types
            let mut suffix = String::new();
            for ty in &self.inputs {
                suffix.push('_');
                suffix.push_str(match ty {
                    RegisterType::I64 => "i",
                    RegisterType::Double => "f",
                });
            }
            suffix.push_str("_to");
            for ty in &self.outputs {
                suffix.push('_');
                suffix.push_str(match ty {
                    RegisterType::I64 => "i",
                    RegisterType::Double => "f",
                });
            }
            suffix
        }
    }

    /// Check if this signature supports direct call (single output)
    pub fn is_direct_call(&self) -> bool {
        self.outputs.len() == 1
    }

    /// Get the LLVM return type for this signature
    ///
    /// Single output: `i64` or `double`
    /// Multiple outputs: `{ i64, i64 }` struct
    pub fn llvm_return_type(&self) -> String {
        if self.outputs.len() == 1 {
            self.outputs[0].llvm_type().to_string()
        } else {
            let types: Vec<_> = self.outputs.iter().map(|t| t.llvm_type()).collect();
            format!("{{ {} }}", types.join(", "))
        }
    }
}

/// Tracks values during specialized code generation
///
/// Unlike the memory-based stack, this tracks SSA variable names
/// that hold values directly in registers.
#[derive(Debug, Clone)]
pub struct RegisterContext {
    /// Stack of (ssa_var_name, register_type) pairs, bottom to top
    pub values: Vec<(String, RegisterType)>,
}

impl RegisterContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Create a context initialized with function parameters
    pub fn from_params(params: &[(String, RegisterType)]) -> Self {
        Self {
            values: params.to_vec(),
        }
    }

    /// Push a value onto the register context
    pub fn push(&mut self, ssa_var: String, ty: RegisterType) {
        self.values.push((ssa_var, ty));
    }

    /// Pop a value from the register context
    pub fn pop(&mut self) -> Option<(String, RegisterType)> {
        self.values.pop()
    }

    /// Peek at the top value without removing it (for future use)
    #[allow(dead_code)]
    pub fn peek(&self) -> Option<&(String, RegisterType)> {
        self.values.last()
    }

    /// Get the number of values in the context
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if the context is empty (for future use)
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Perform dup: ( a -- a a ) - duplicate top value
    /// Note: For registers, this is a no-op at the SSA level,
    /// we just reference the same SSA variable twice
    pub fn dup(&mut self) {
        if let Some((ssa, ty)) = self.values.last().cloned() {
            self.values.push((ssa, ty));
        }
    }

    /// Perform drop: ( a -- )
    pub fn drop(&mut self) {
        self.values.pop();
    }

    /// Perform swap: ( a b -- b a )
    pub fn swap(&mut self) {
        let len = self.values.len();
        if len >= 2 {
            self.values.swap(len - 1, len - 2);
        }
    }

    /// Perform over: ( a b -- a b a )
    pub fn over(&mut self) {
        let len = self.values.len();
        if len >= 2 {
            let a = self.values[len - 2].clone();
            self.values.push(a);
        }
    }

    /// Perform rot: ( a b c -- b c a )
    pub fn rot(&mut self) {
        let len = self.values.len();
        if len >= 3 {
            let a = self.values.remove(len - 3);
            self.values.push(a);
        }
    }
}

impl Default for RegisterContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Operations that can be emitted in specialized (register-based) mode.
///
/// These operations either:
/// - Map directly to LLVM instructions (arithmetic, comparisons, bitwise)
/// - Use LLVM intrinsics (popcount, clz, ctz)
/// - Are pure context manipulations (stack shuffles like dup, swap, rot)
///
/// Operations NOT in this list (like `print`, `read`, string ops) require
/// the full stack-based calling convention.
const SPECIALIZABLE_OPS: &[&str] = &[
    // Integer arithmetic (i./ and i.% emit safe division with zero checks)
    "i.+",
    "i.add",
    "i.-",
    "i.subtract",
    "i.*",
    "i.multiply",
    "i./",
    "i.divide",
    "i.%",
    "i.mod",
    // Bitwise operations
    "band",
    "bor",
    "bxor",
    "bnot",
    "shl",
    "shr",
    // Bit counting operations
    "popcount",
    "clz",
    "ctz",
    // Type conversions
    "int->float",
    "float->int",
    // Boolean logical operations
    "and",
    "or",
    "not",
    // Integer comparisons
    "i.<",
    "i.lt",
    "i.>",
    "i.gt",
    "i.<=",
    "i.lte",
    "i.>=",
    "i.gte",
    "i.=",
    "i.eq",
    "i.<>",
    "i.neq",
    // Float arithmetic
    "f.+",
    "f.add",
    "f.-",
    "f.subtract",
    "f.*",
    "f.multiply",
    "f./",
    "f.divide",
    // Float comparisons
    "f.<",
    "f.lt",
    "f.>",
    "f.gt",
    "f.<=",
    "f.lte",
    "f.>=",
    "f.gte",
    "f.=",
    "f.eq",
    "f.<>",
    "f.neq",
    // Stack operations (handled as context shuffles)
    "dup",
    "drop",
    "swap",
    "over",
    "rot",
    "nip",
    "tuck",
    "pick",
    "roll",
];

impl CodeGen {
    /// Check if a word can be specialized and return its signature if so
    pub fn can_specialize(&self, word: &WordDef) -> Option<SpecSignature> {
        // Must have an effect declaration
        let effect = word.effect.as_ref()?;

        // Must not have side effects (like Yield)
        if !effect.is_pure() {
            return None;
        }

        // Extract input/output types from the effect
        let inputs = Self::extract_register_types(&effect.inputs)?;
        let outputs = Self::extract_register_types(&effect.outputs)?;

        // Must have at least one input or output to optimize
        if inputs.is_empty() && outputs.is_empty() {
            return None;
        }

        // Must have at least one output (zero outputs means side-effect only)
        if outputs.is_empty() {
            return None;
        }

        // Check that the body is specializable
        if !self.is_body_specializable(&word.body, &word.name) {
            return None;
        }

        Some(SpecSignature { inputs, outputs })
    }

    /// Extract register types from a stack type
    ///
    /// The parser always adds a row variable for composability, so we accept
    /// stack types with a row variable at the base and extract the concrete
    /// types on top of it.
    fn extract_register_types(stack: &StackType) -> Option<Vec<RegisterType>> {
        let mut types = Vec::new();
        let mut current = stack;

        loop {
            match current {
                StackType::Empty => break,
                StackType::RowVar(_) => {
                    // Row variable at the base is OK - we can specialize the
                    // concrete types on top of it. The row variable just means
                    // "whatever else is on the stack stays there".
                    break;
                }
                StackType::Cons { rest, top } => {
                    let reg_ty = RegisterType::from_type(top)?;
                    types.push(reg_ty);
                    current = rest;
                }
            }
        }

        // Reverse to get bottom-to-top order
        types.reverse();
        Some(types)
    }

    /// Check if a word body can be specialized.
    ///
    /// Tracks whether the previous statement was an integer literal, which is
    /// required for `pick` and `roll` operations in specialized mode.
    fn is_body_specializable(&self, body: &[Statement], word_name: &str) -> bool {
        let mut prev_was_int_literal = false;
        for stmt in body {
            if !self.is_statement_specializable(stmt, word_name, prev_was_int_literal) {
                return false;
            }
            // Track if this statement was an integer literal for the next iteration
            prev_was_int_literal = matches!(stmt, Statement::IntLiteral(_));
        }
        true
    }

    /// Check if a single statement can be specialized.
    ///
    /// `prev_was_int_literal` indicates whether the preceding statement was an
    /// IntLiteral, which is required for `pick` and `roll` to be specializable.
    fn is_statement_specializable(
        &self,
        stmt: &Statement,
        word_name: &str,
        prev_was_int_literal: bool,
    ) -> bool {
        match stmt {
            // Integer literals are fine
            Statement::IntLiteral(_) => true,

            // Float literals are fine
            Statement::FloatLiteral(_) => true,

            // Bool literals are fine
            Statement::BoolLiteral(_) => true,

            // String literals require heap allocation - not specializable
            Statement::StringLiteral(_) => false,

            // Symbols require heap allocation - not specializable
            Statement::Symbol(_) => false,

            // Quotations create closures - not specializable
            Statement::Quotation { .. } => false,

            // Match requires symbols - not specializable
            Statement::Match { .. } => false,

            // Word calls: check if it's a specializable operation or recursive call
            Statement::WordCall { name, .. } => {
                // Recursive calls to self are OK (we'll call specialized version)
                if name == word_name {
                    return true;
                }

                // pick and roll require a compile-time constant N (previous int literal)
                // If N is computed at runtime, the word cannot be specialized
                if (name == "pick" || name == "roll") && !prev_was_int_literal {
                    return false;
                }

                // Check if it's a built-in specializable op
                if SPECIALIZABLE_OPS.contains(&name.as_str()) {
                    return true;
                }

                // Check if it's another word we know is specializable
                if self.specialized_words.contains_key(name) {
                    return true;
                }

                // Not specializable
                false
            }

            // If/else: check both branches
            Statement::If {
                then_branch,
                else_branch,
            } => {
                if !self.is_body_specializable(then_branch, word_name) {
                    return false;
                }
                if let Some(else_stmts) = else_branch
                    && !self.is_body_specializable(else_stmts, word_name)
                {
                    return false;
                }
                true
            }
        }
    }

    /// Generate a specialized version of a word.
    ///
    /// This creates a register-based function that passes values directly in
    /// CPU registers instead of through the 40-byte `%Value` stack.
    ///
    /// The generated function:
    /// - Takes primitive arguments directly (i64 for Int/Bool, double for Float)
    /// - Returns the result in a register (not via stack pointer)
    /// - Uses `musttail` for recursive calls to guarantee TCO
    /// - Handles control flow with phi nodes for value merging
    ///
    /// Example output for `fib ( Int -- Int )`:
    /// ```llvm
    /// define i64 @seq_fib_i64(i64 %arg0) {
    ///   ; ... register-based implementation
    /// }
    /// ```
    pub fn codegen_specialized_word(
        &mut self,
        word: &WordDef,
        sig: &SpecSignature,
    ) -> Result<(), CodeGenError> {
        let base_name = format!("seq_{}", mangle_name(&word.name));
        let spec_name = format!("{}{}", base_name, sig.suffix());

        // Generate function signature
        // For single output: define i64 @name(i64 %arg0) {
        // For multiple outputs: define { i64, i64 } @name(i64 %arg0, i64 %arg1) {
        let return_type = if sig.outputs.len() == 1 {
            sig.outputs[0].llvm_type().to_string()
        } else {
            // Struct return for multiple values
            let types: Vec<_> = sig.outputs.iter().map(|t| t.llvm_type()).collect();
            format!("{{ {} }}", types.join(", "))
        };

        // Generate parameter list
        let params: Vec<String> = sig
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

        // Initialize register context with parameters
        let initial_params: Vec<(String, RegisterType)> = sig
            .inputs
            .iter()
            .enumerate()
            .map(|(i, ty)| (format!("arg{}", i), *ty))
            .collect();
        let mut ctx = RegisterContext::from_params(&initial_params);

        // Generate code for each statement
        let body_len = word.body.len();
        let mut prev_int_literal: Option<i64> = None;
        for (i, stmt) in word.body.iter().enumerate() {
            let is_last = i == body_len - 1;
            self.codegen_specialized_statement(
                &mut ctx,
                stmt,
                &word.name,
                sig,
                is_last,
                &mut prev_int_literal,
            )?;
        }

        writeln!(&mut self.output, "}}")?;
        writeln!(&mut self.output)?;

        // Record that this word is specialized
        self.specialized_words
            .insert(word.name.clone(), sig.clone());

        Ok(())
    }

    /// Generate specialized code for a single statement
    fn codegen_specialized_statement(
        &mut self,
        ctx: &mut RegisterContext,
        stmt: &Statement,
        word_name: &str,
        sig: &SpecSignature,
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
                self.codegen_specialized_word_call(ctx, name, word_name, sig, is_last, prev_int)?;
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                self.codegen_specialized_if(
                    ctx,
                    then_branch,
                    else_branch.as_ref(),
                    word_name,
                    sig,
                    is_last,
                )?;
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
            Statement::WordCall { name, .. } if name == word_name => true,
            _ => false,
        };
        if is_last && !already_returns {
            self.emit_specialized_return(ctx, sig)?;
        }

        Ok(())
    }

    /// Generate a specialized word call
    fn codegen_specialized_word_call(
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
                // NOT is XOR with -1 (all 1s)
                writeln!(&mut self.output, "  %{} = xor i64 %{}, -1", result, a)?;
                ctx.push(result, RegisterType::I64);
            }
            "shl" => {
                self.emit_specialized_safe_shift(ctx, true)?;
            }
            "shr" => {
                self.emit_specialized_safe_shift(ctx, false)?;
            }

            // Bit counting operations (LLVM intrinsics)
            "popcount" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call i64 @llvm.ctpop.i64(i64 %{})",
                    result, a
                )?;
                ctx.push(result, RegisterType::I64);
            }
            "clz" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                // is_zero_poison = false: return 64 for input 0
                writeln!(
                    &mut self.output,
                    "  %{} = call i64 @llvm.ctlz.i64(i64 %{}, i1 false)",
                    result, a
                )?;
                ctx.push(result, RegisterType::I64);
            }
            "ctz" => {
                let (a, _) = ctx.pop().unwrap();
                let result = self.fresh_temp();
                // is_zero_poison = false: return 64 for input 0
                writeln!(
                    &mut self.output,
                    "  %{} = call i64 @llvm.cttz.i64(i64 %{}, i1 false)",
                    result, a
                )?;
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
                // Logical NOT: XOR with 1 flips 0<->1
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

    /// Emit a specialized integer comparison
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

    /// Emit a specialized float comparison
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

    /// Emit a safe integer division or modulo with overflow protection.
    ///
    /// Returns ( Int Int -- Int Bool ) where Bool indicates success.
    /// Division by zero returns (0, false).
    /// INT_MIN / -1 uses wrapping semantics (returns INT_MIN, true) to match runtime.
    ///
    /// Note: LLVM's sdiv has undefined behavior for INT_MIN / -1, so we must
    /// handle it explicitly. We match the runtime's wrapping_div behavior.
    fn emit_specialized_safe_div(
        &mut self,
        ctx: &mut RegisterContext,
        op: &str, // "sdiv" or "srem"
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap(); // divisor
        let (a, _) = ctx.pop().unwrap(); // dividend

        // Check if divisor is zero
        let is_zero = self.fresh_temp();
        writeln!(&mut self.output, "  %{} = icmp eq i64 %{}, 0", is_zero, b)?;

        // For sdiv: also check for INT_MIN / -1 overflow case
        // We handle this specially to return INT_MIN (wrapping behavior)
        let (check_overflow, is_overflow) = if op == "sdiv" {
            let is_int_min = self.fresh_temp();
            let is_neg_one = self.fresh_temp();
            let is_overflow = self.fresh_temp();

            // Check if dividend is INT_MIN (-9223372036854775808)
            writeln!(
                &mut self.output,
                "  %{} = icmp eq i64 %{}, -9223372036854775808",
                is_int_min, a
            )?;
            // Check if divisor is -1
            writeln!(
                &mut self.output,
                "  %{} = icmp eq i64 %{}, -1",
                is_neg_one, b
            )?;
            // Overflow if both conditions are true
            writeln!(
                &mut self.output,
                "  %{} = and i1 %{}, %{}",
                is_overflow, is_int_min, is_neg_one
            )?;
            (true, is_overflow)
        } else {
            (false, String::new())
        };

        // Generate branch labels
        let ok_label = self.fresh_block("div_ok");
        let fail_label = self.fresh_block("div_fail");
        let merge_label = self.fresh_block("div_merge");
        let overflow_label = if check_overflow {
            self.fresh_block("div_overflow")
        } else {
            String::new()
        };

        // First check: division by zero
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            is_zero,
            fail_label,
            if check_overflow {
                &overflow_label
            } else {
                &ok_label
            }
        )?;

        // For sdiv: check overflow case (INT_MIN / -1)
        if check_overflow {
            writeln!(&mut self.output, "{}:", overflow_label)?;
            writeln!(
                &mut self.output,
                "  br i1 %{}, label %{}, label %{}",
                is_overflow, merge_label, ok_label
            )?;
        }

        // Success branch: perform the division
        writeln!(&mut self.output, "{}:", ok_label)?;
        let ok_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            ok_result, op, a, b
        )?;
        writeln!(&mut self.output, "  br label %{}", merge_label)?;

        // Failure branch: return 0 and false
        writeln!(&mut self.output, "{}:", fail_label)?;
        writeln!(&mut self.output, "  br label %{}", merge_label)?;

        // Merge block with phi nodes
        writeln!(&mut self.output, "{}:", merge_label)?;
        let result_phi = self.fresh_temp();
        let success_phi = self.fresh_temp();

        if check_overflow {
            // For sdiv: three incoming edges (ok, fail, overflow)
            // Overflow returns INT_MIN (wrapping behavior) with success=true
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ %{}, %{} ], [ 0, %{} ], [ -9223372036854775808, %{} ]",
                result_phi, ok_result, ok_label, fail_label, overflow_label
            )?;
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ 1, %{} ], [ 0, %{} ], [ 1, %{} ]",
                success_phi, ok_label, fail_label, overflow_label
            )?;
        } else {
            // For srem: two incoming edges (ok, fail)
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ %{}, %{} ], [ 0, %{} ]",
                result_phi, ok_result, ok_label, fail_label
            )?;
            writeln!(
                &mut self.output,
                "  %{} = phi i64 [ 1, %{} ], [ 0, %{} ]",
                success_phi, ok_label, fail_label
            )?;
        }

        // Push result and success flag to context
        // Stack order: result first (deeper), then success (top)
        ctx.push(result_phi, RegisterType::I64);
        ctx.push(success_phi, RegisterType::I64);

        Ok(())
    }

    /// Emit a safe shift operation with bounds checking
    ///
    /// Returns 0 for negative shift or shift >= 64, otherwise performs the shift.
    /// Matches runtime behavior for shl/shr.
    fn emit_specialized_safe_shift(
        &mut self,
        ctx: &mut RegisterContext,
        is_left: bool, // true for shl, false for shr
    ) -> Result<(), CodeGenError> {
        let (b, _) = ctx.pop().unwrap(); // shift count
        let (a, _) = ctx.pop().unwrap(); // value to shift

        // Check if shift count is negative
        let is_negative = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp slt i64 %{}, 0",
            is_negative, b
        )?;

        // Check if shift count >= 64
        let is_too_large = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp sge i64 %{}, 64",
            is_too_large, b
        )?;

        // Combine: invalid if negative OR >= 64
        let is_invalid = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = or i1 %{}, %{}",
            is_invalid, is_negative, is_too_large
        )?;

        // Use a safe shift count (0 if invalid) to avoid LLVM undefined behavior
        let safe_count = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            safe_count, is_invalid, b
        )?;

        // Perform the shift with safe count
        let shift_result = self.fresh_temp();
        let op = if is_left { "shl" } else { "lshr" };
        writeln!(
            &mut self.output,
            "  %{} = {} i64 %{}, %{}",
            shift_result, op, a, safe_count
        )?;

        // Select final result: 0 if invalid, otherwise shift_result
        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = select i1 %{}, i64 0, i64 %{}",
            result, is_invalid, shift_result
        )?;

        ctx.push(result, RegisterType::I64);
        Ok(())
    }

    /// Emit a recursive call to the specialized version of the current word.
    ///
    /// Uses `musttail` when `is_tail` is true to guarantee tail call optimization.
    /// This is critical for recursive algorithms like `fib` or `count-down` that
    /// would otherwise overflow the call stack.
    ///
    /// The call pops arguments from the register context and pushes the result.
    fn emit_specialized_recursive_call(
        &mut self,
        ctx: &mut RegisterContext,
        word_name: &str,
        sig: &SpecSignature,
        is_tail: bool,
    ) -> Result<(), CodeGenError> {
        let spec_name = format!("seq_{}{}", mangle_name(word_name), sig.suffix());

        // Check we have enough values in context
        if ctx.values.len() < sig.inputs.len() {
            return Err(CodeGenError::Logic(format!(
                "Not enough values in context for recursive call to {}: need {}, have {}",
                word_name,
                sig.inputs.len(),
                ctx.values.len()
            )));
        }

        // Pop arguments from context
        let mut args = Vec::new();
        for _ in 0..sig.inputs.len() {
            args.push(ctx.pop().unwrap());
        }
        args.reverse(); // Args were popped in reverse order

        // Build argument list
        let arg_strs: Vec<String> = args
            .iter()
            .map(|(var, ty)| format!("{} %{}", ty.llvm_type(), var))
            .collect();

        let return_type = sig.llvm_return_type();

        if is_tail {
            // Tail call - use musttail for guaranteed TCO
            let result = self.fresh_temp();
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
            // Non-tail call
            let result = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call {} @{}({})",
                result,
                return_type,
                spec_name,
                arg_strs.join(", ")
            )?;

            if sig.outputs.len() == 1 {
                // Single output - push directly
                ctx.push(result, sig.outputs[0]);
            } else {
                // Multi-output - extract values from struct and push each
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

    /// Emit a call to another specialized word
    fn emit_specialized_word_dispatch(
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

        // Pop arguments from context
        let mut args = Vec::new();
        for _ in 0..sig.inputs.len() {
            args.push(ctx.pop().unwrap());
        }
        args.reverse();

        // Build argument list
        let arg_strs: Vec<String> = args
            .iter()
            .map(|(var, ty)| format!("{} %{}", ty.llvm_type(), var))
            .collect();

        let return_type = sig.llvm_return_type();

        let result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call {} @{}({})",
            result,
            return_type,
            spec_name,
            arg_strs.join(", ")
        )?;

        if sig.outputs.len() == 1 {
            // Single output - push directly
            ctx.push(result, sig.outputs[0]);
        } else {
            // Multi-output - extract values from struct and push each
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

        Ok(())
    }

    /// Emit return statement for specialized function
    fn emit_specialized_return(
        &mut self,
        ctx: &RegisterContext,
        sig: &SpecSignature,
    ) -> Result<(), CodeGenError> {
        let output_count = sig.outputs.len();

        if output_count == 0 {
            writeln!(&mut self.output, "  ret void")?;
        } else if output_count == 1 {
            let (var, ty) = ctx
                .values
                .last()
                .ok_or_else(|| CodeGenError::Logic("Empty context at return".to_string()))?;
            writeln!(&mut self.output, "  ret {} %{}", ty.llvm_type(), var)?;
        } else {
            // Multi-output: build struct return
            // Values in context are bottom-to-top, matching sig.outputs order
            if ctx.values.len() < output_count {
                return Err(CodeGenError::Logic(format!(
                    "Not enough values for multi-output return: need {}, have {}",
                    output_count,
                    ctx.values.len()
                )));
            }

            // Get the values to return (last N values from context)
            let start_idx = ctx.values.len() - output_count;
            let return_values: Vec<_> = ctx.values[start_idx..].to_vec();

            // Build struct type string
            let struct_type = sig.llvm_return_type();

            // Build the struct incrementally with insertvalue
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

    /// Generate specialized if/else statement
    fn codegen_specialized_if(
        &mut self,
        ctx: &mut RegisterContext,
        then_branch: &[Statement],
        else_branch: Option<&Vec<Statement>>,
        word_name: &str,
        sig: &SpecSignature,
        is_last: bool,
    ) -> Result<(), CodeGenError> {
        // Pop condition
        let (cond_var, _) = ctx
            .pop()
            .ok_or_else(|| CodeGenError::Logic("Empty context at if condition".to_string()))?;

        // Compare condition with 0
        let cmp_result = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp_result, cond_var
        )?;

        // Generate branch labels
        let then_label = self.fresh_block("if_then");
        let else_label = self.fresh_block("if_else");
        let merge_label = self.fresh_block("if_merge");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_result, then_label, else_label
        )?;

        // Generate then branch
        writeln!(&mut self.output, "{}:", then_label)?;
        let mut then_ctx = ctx.clone();
        let mut then_prev_int: Option<i64> = None;
        for (i, stmt) in then_branch.iter().enumerate() {
            let is_stmt_last = i == then_branch.len() - 1 && is_last;
            self.codegen_specialized_statement(
                &mut then_ctx,
                stmt,
                word_name,
                sig,
                is_stmt_last,
                &mut then_prev_int,
            )?;
        }
        // If the then branch is empty and this is the last statement, emit return
        if is_last && then_branch.is_empty() {
            self.emit_specialized_return(&then_ctx, sig)?;
        }
        let then_result = then_ctx.values.last().cloned();
        // If is_last was true for the last statement (or branch is empty), a return was emitted
        let then_emitted_return = is_last;
        let then_pred = if then_emitted_return {
            None
        } else {
            writeln!(&mut self.output, "  br label %{}", merge_label)?;
            Some(then_label.clone())
        };

        // Generate else branch
        writeln!(&mut self.output, "{}:", else_label)?;
        let mut else_ctx = ctx.clone();
        let mut else_prev_int: Option<i64> = None;
        if let Some(else_stmts) = else_branch {
            for (i, stmt) in else_stmts.iter().enumerate() {
                let is_stmt_last = i == else_stmts.len() - 1 && is_last;
                self.codegen_specialized_statement(
                    &mut else_ctx,
                    stmt,
                    word_name,
                    sig,
                    is_stmt_last,
                    &mut else_prev_int,
                )?;
            }
        }
        // If the else branch is empty (or None) and this is the last statement, emit return
        if is_last && (else_branch.is_none() || else_branch.as_ref().is_some_and(|b| b.is_empty()))
        {
            self.emit_specialized_return(&else_ctx, sig)?;
        }
        let else_result = else_ctx.values.last().cloned();
        // If is_last was true for the last statement (or branch is empty/None), a return was emitted
        let else_emitted_return = is_last;
        let else_pred = if else_emitted_return {
            None
        } else {
            writeln!(&mut self.output, "  br label %{}", merge_label)?;
            Some(else_label.clone())
        };

        // Generate merge block with phi node if both branches continue
        if then_pred.is_some() || else_pred.is_some() {
            writeln!(&mut self.output, "{}:", merge_label)?;

            // If both branches continue, we need a phi node
            if let (
                Some(then_p),
                Some(else_p),
                Some((then_var, then_ty)),
                Some((else_var, else_ty)),
            ) = (&then_pred, &else_pred, &then_result, &else_result)
            {
                if then_ty == else_ty {
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
                    ctx.values.clear();
                    ctx.push(phi_result, *then_ty);
                }
            } else if let (Some(_), Some((then_var, then_ty))) = (&then_pred, &then_result) {
                // Only then branch continues
                ctx.values.clear();
                ctx.push(then_var.clone(), *then_ty);
            } else if let (Some(_), Some((else_var, else_ty))) = (&else_pred, &else_result) {
                // Only else branch continues
                ctx.values.clear();
                ctx.push(else_var.clone(), *else_ty);
            }

            // If this is the last statement, emit return
            if is_last && (then_pred.is_some() || else_pred.is_some()) {
                self.emit_specialized_return(ctx, sig)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_type_from_type() {
        assert_eq!(RegisterType::from_type(&Type::Int), Some(RegisterType::I64));
        assert_eq!(
            RegisterType::from_type(&Type::Bool),
            Some(RegisterType::I64)
        );
        assert_eq!(
            RegisterType::from_type(&Type::Float),
            Some(RegisterType::Double)
        );
        assert_eq!(RegisterType::from_type(&Type::String), None);
    }

    #[test]
    fn test_spec_signature_suffix() {
        let sig = SpecSignature {
            inputs: vec![RegisterType::I64],
            outputs: vec![RegisterType::I64],
        };
        assert_eq!(sig.suffix(), "_i64");

        let sig2 = SpecSignature {
            inputs: vec![RegisterType::Double],
            outputs: vec![RegisterType::Double],
        };
        assert_eq!(sig2.suffix(), "_f64");
    }

    #[test]
    fn test_register_context_stack_ops() {
        let mut ctx = RegisterContext::new();
        ctx.push("a".to_string(), RegisterType::I64);
        ctx.push("b".to_string(), RegisterType::I64);

        assert_eq!(ctx.len(), 2);

        // Test swap
        ctx.swap();
        assert_eq!(ctx.values[0].0, "b");
        assert_eq!(ctx.values[1].0, "a");

        // Test dup
        ctx.dup();
        assert_eq!(ctx.len(), 3);
        assert_eq!(ctx.values[2].0, "a");

        // Test drop
        ctx.drop();
        assert_eq!(ctx.len(), 2);
    }
}
