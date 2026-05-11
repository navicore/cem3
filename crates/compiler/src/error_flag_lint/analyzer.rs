//! The `ErrorFlagAnalyzer` walks the AST, drives the abstract flag-stack
//! simulation, and emits lint diagnostics when tagged Bools are dropped
//! without being checked.

use std::path::{Path, PathBuf};

use crate::ast::{Program, Span, Statement, WordDef};
use crate::lint::{LintDiagnostic, Severity};

use super::state::{ErrorFlag, FlagStack, StackVal, fallible_op_info, is_checking_consumer};

pub struct ErrorFlagAnalyzer {
    file: PathBuf,
    diagnostics: Vec<LintDiagnostic>,
}

impl ErrorFlagAnalyzer {
    pub fn new(file: &Path) -> Self {
        ErrorFlagAnalyzer {
            file: file.to_path_buf(),
            diagnostics: Vec::new(),
        }
    }

    pub fn analyze_program(&mut self, program: &Program) -> Vec<LintDiagnostic> {
        let mut all_diagnostics = Vec::new();
        for word in &program.words {
            // Skip words with seq:allow(unchecked-error-flag)
            if word
                .allowed_lints
                .iter()
                .any(|l| l == "unchecked-error-flag")
            {
                continue;
            }
            let diags = self.analyze_word(word);
            all_diagnostics.extend(diags);
        }
        all_diagnostics
    }

    pub(super) fn analyze_word(&mut self, word: &WordDef) -> Vec<LintDiagnostic> {
        self.diagnostics.clear();
        let mut state = FlagStack::new();
        self.analyze_statements(&word.body, &mut state, word);
        // Flags remaining on stack at word end = returned to caller (escape)
        std::mem::take(&mut self.diagnostics)
    }

    fn analyze_statements(
        &mut self,
        statements: &[Statement],
        state: &mut FlagStack,
        word: &WordDef,
    ) {
        for stmt in statements {
            self.analyze_statement(stmt, state, word);
        }
    }

    fn analyze_statement(&mut self, stmt: &Statement, state: &mut FlagStack, word: &WordDef) {
        match stmt {
            Statement::IntLiteral(_)
            | Statement::FloatLiteral(_)
            | Statement::BoolLiteral(_)
            | Statement::StringLiteral(_)
            | Statement::Symbol(_) => {
                state.push_other();
            }

            Statement::Quotation { .. } => {
                state.push_other();
            }

            Statement::WordCall { name, span } => {
                self.analyze_word_call(name, span.as_ref(), state, word);
            }

            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => {
                // `if` consumes the Bool on top — this IS a check
                state.pop();

                let mut then_state = state.clone();
                let mut else_state = state.clone();
                self.analyze_statements(then_branch, &mut then_state, word);
                if let Some(else_stmts) = else_branch {
                    self.analyze_statements(else_stmts, &mut else_state, word);
                }
                *state = then_state.join(&else_state);
            }

            Statement::Match { arms, span: _ } => {
                state.pop(); // match value consumed
                let mut arm_states: Vec<FlagStack> = Vec::new();
                for arm in arms {
                    let mut arm_state = state.clone();
                    // Match arm bindings push values onto stack
                    match &arm.pattern {
                        crate::ast::Pattern::Variant(_) => {
                            // Variant without named bindings — field count unknown
                            // statically. Same limitation as resource_lint.
                        }
                        crate::ast::Pattern::VariantWithBindings { bindings, .. } => {
                            for _binding in bindings {
                                arm_state.push_other();
                            }
                        }
                    }
                    self.analyze_statements(&arm.body, &mut arm_state, word);
                    arm_states.push(arm_state);
                }
                if let Some(joined) = arm_states.into_iter().reduce(|acc, s| acc.join(&s)) {
                    *state = joined;
                }
            }
        }
    }

    pub(super) fn analyze_word_call(
        &mut self,
        name: &str,
        span: Option<&Span>,
        state: &mut FlagStack,
        word: &WordDef,
    ) {
        let line = span.map(|s| s.line).unwrap_or(0);

        // Check if this is a fallible operation
        if let Some(info) = fallible_op_info(name) {
            // Pop inputs consumed by the operation
            for _ in 0..info.inputs {
                state.pop();
            }
            // Push output values, then the error flag Bool
            for _ in 0..info.values_before_bool {
                state.push_other();
            }
            state.push_flag(line, name, info.description);
            return;
        }

        // Check if this is a checking consumer
        if is_checking_consumer(name) {
            // `cond` is a multi-way conditional that consumes quotation pairs
            // + a count from the stack. Its variable arity means we can't
            // precisely model what it consumes. Conservative: assume it
            // checks any flags it touches (no warning), but don't clear
            // the entire stack — flags below the cond args may still need checking.
            state.pop(); // at minimum, the count argument
            return;
        }

        // Stack operations — simulate movement
        match name {
            "drop" => {
                if let Some(StackVal::Flag(flag)) = state.pop() {
                    self.emit_warning(&flag, line, word);
                }
            }
            "nip" => {
                // ( a b -- b ) — drops a (second from top)
                let top = state.pop();
                if let Some(StackVal::Flag(flag)) = state.pop() {
                    self.emit_warning(&flag, line, word);
                }
                if let Some(v) = top {
                    state.stack.push(v);
                }
            }
            "3drop" => {
                for _ in 0..3 {
                    if let Some(StackVal::Flag(flag)) = state.pop() {
                        self.emit_warning(&flag, line, word);
                    }
                }
            }
            "2drop" => {
                for _ in 0..2 {
                    if let Some(StackVal::Flag(flag)) = state.pop() {
                        self.emit_warning(&flag, line, word);
                    }
                }
            }
            "dup" => {
                if let Some(top) = state.stack.last().cloned() {
                    state.stack.push(top);
                }
            }
            "swap" => {
                let a = state.pop();
                let b = state.pop();
                if let Some(v) = a {
                    state.stack.push(v);
                }
                if let Some(v) = b {
                    state.stack.push(v);
                }
            }
            // Guard arms ("over"/"2dup" with depth check) intentionally
            // fall through to the catch-all `_ => { /* no-op */ }` when
            // the guard fails. Adding a new earlier arm whose pattern
            // could match these names (e.g. another `_ if ... =>`) would
            // silently change behavior — keep guard-bearing arms close
            // to the catch-all or convert back to inner-`if` form.
            "over" if state.depth() >= 2 => {
                let second = state.stack[state.depth() - 2].clone();
                state.stack.push(second);
            }
            "rot" => {
                let c = state.pop();
                let b = state.pop();
                let a = state.pop();
                if let Some(v) = b {
                    state.stack.push(v);
                }
                if let Some(v) = c {
                    state.stack.push(v);
                }
                if let Some(v) = a {
                    state.stack.push(v);
                }
            }
            "tuck" => {
                let b = state.pop();
                let a = state.pop();
                if let Some(v) = b.clone() {
                    state.stack.push(v);
                }
                if let Some(v) = a {
                    state.stack.push(v);
                }
                if let Some(v) = b {
                    state.stack.push(v);
                }
            }
            "2dup" if state.depth() >= 2 => {
                let a = state.stack[state.depth() - 2].clone();
                let b = state.stack[state.depth() - 1].clone();
                state.stack.push(a);
                state.stack.push(b);
            }
            ">aux" => {
                if let Some(v) = state.pop() {
                    state.aux.push(v);
                }
            }
            "aux>" => {
                if let Some(v) = state.aux.pop() {
                    state.stack.push(v);
                }
            }
            "pick" | "roll" => {
                // Conservative: push unknown (can't statically know depth)
                state.push_other();
            }

            // Combinators — dip hides top, runs quotation, restores
            "dip" => {
                // ( x quot -- ? x ) — pop quot, pop x, run quot (unknown effect), push x
                state.pop(); // quotation
                let preserved = state.pop();
                // Quotation effect unknown — conservatively clear flags from stack
                // (quotation might check them, might not)
                state.stack.retain(|v| !matches!(v, StackVal::Flag(_)));
                if let Some(v) = preserved {
                    state.stack.push(v);
                }
            }
            "keep" => {
                // ( x quot -- ? x ) — similar to dip but quotation gets x
                state.pop(); // quotation
                let preserved = state.pop();
                state.stack.retain(|v| !matches!(v, StackVal::Flag(_)));
                if let Some(v) = preserved {
                    state.stack.push(v);
                }
            }
            "bi" => {
                // ( x q1 q2 -- ? ) — two quotations consume x
                state.pop(); // q2
                state.pop(); // q1
                state.pop(); // x
                // Both quotations have unknown effects
                state.stack.retain(|v| !matches!(v, StackVal::Flag(_)));
            }

            // call — quotation effect unknown, conservatively assume it checks
            "call" => {
                state.pop(); // quotation
                // Conservative: clear tracked flags (quotation might do anything)
                state.stack.retain(|v| !matches!(v, StackVal::Flag(_)));
            }

            // Known type-conversion words that consume one value and push one
            "int->string" | "int->float" | "float->int" | "float->string" | "char->string"
            | "symbol->string" | "string->symbol" => {
                // These consume the top value. If it's a flag, that's suspicious
                // but not necessarily wrong (e.g., converting a Bool to string for display).
                // Conservative: don't warn, just remove tracking.
                state.pop();
                state.push_other();
            }

            // Boolean operations that legitimately consume Bools
            "and" | "or" | "not" => {
                // These consume Bool(s) and produce Bool — not a check per se,
                // but the user is clearly working with the Bool value.
                // Conservative: mark as consumed (no warning).
                state.pop();
                if name != "not" {
                    state.pop();
                }
                state.push_other();
            }

            // Test assertions that check Bools
            "test.assert" | "test.assert-not" => {
                state.pop(); // Bool consumed by assertion = checked
            }

            // All other words: conservative — assume they consume/produce
            // unknown values. Pop any flags without warning (might be checked
            // inside the word).
            _ => {
                // For unknown words, we don't know the stack effect.
                // Conservative: leave the stack as-is (don't warn, don't clear).
                // This avoids false positives from user-defined words that
                // properly handle the Bool internally.
            }
        }
    }

    fn emit_warning(&mut self, flag: &ErrorFlag, drop_line: usize, word: &WordDef) {
        // Don't warn if the drop is adjacent to the operation (within 2 lines).
        // Adjacent drops like `net.tcp.write drop` are covered by the pattern-based
        // linter with better precision (exact column info, replacement suggestions).
        // We only add value for non-adjacent drops (e.g., swap nip, aux round-trips).
        // Note: if spans are missing, both lines default to 0 and this suppresses
        // the warning — acceptable since span-less nodes are rare (synthetic AST only).
        if drop_line <= flag.created_line + 2 {
            return;
        }

        self.diagnostics.push(LintDiagnostic {
            id: "unchecked-error-flag".to_string(),
            message: format!(
                "`{}` returns a Bool error flag (indicates {}) — dropped without checking",
                flag.operation, flag.description,
            ),
            severity: Severity::Warning,
            replacement: String::new(),
            file: self.file.clone(),
            line: flag.created_line,
            end_line: Some(drop_line),
            start_column: None,
            end_column: None,
            word_name: word.name.clone(),
            start_index: 0,
            end_index: 0,
        });
    }
}
