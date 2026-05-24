//! Program-wide resource analyzer — does a two-pass walk that first collects
//! each word's resource-return shape, then simulates each body with that
//! information to catch cross-word leaks.

use std::collections::HashMap;
use std::path::Path;

use crate::ast::{Program, Span, Statement, WordDef};
use crate::lint::{LintDiagnostic, Severity};

use super::state::{
    InconsistentResource, ResourceKind, StackState, StackValue, TrackedResource, WordResourceInfo,
};

/// Program-wide resource analyzer for cross-word analysis
///
/// This analyzer performs two passes:
/// 1. Collect resource information about each word (what resources it returns)
/// 2. Analyze each word with knowledge of callee behavior
pub struct ProgramResourceAnalyzer {
    /// Per-word resource information (populated in first pass)
    word_info: HashMap<String, WordResourceInfo>,
    /// File being analyzed
    file: std::path::PathBuf,
    /// Diagnostics collected during analysis
    diagnostics: Vec<LintDiagnostic>,
}

impl ProgramResourceAnalyzer {
    pub fn new(file: &Path) -> Self {
        ProgramResourceAnalyzer {
            word_info: HashMap::new(),
            file: file.to_path_buf(),
            diagnostics: Vec::new(),
        }
    }

    /// Analyze an entire program for resource leaks with cross-word tracking
    pub fn analyze_program(&mut self, program: &Program) -> Vec<LintDiagnostic> {
        self.diagnostics.clear();
        self.word_info.clear();

        // Pass 1: Collect resource information about each word
        for word in &program.words {
            let info = self.collect_word_info(word);
            self.word_info.insert(word.name.clone(), info);
        }

        // Pass 2: Analyze each word with cross-word context
        for word in &program.words {
            self.analyze_word_with_context(word);
        }

        std::mem::take(&mut self.diagnostics)
    }

    /// First pass: Determine what resources a word returns
    fn collect_word_info(&self, word: &WordDef) -> WordResourceInfo {
        let mut state = StackState::new();

        // Simple analysis without emitting diagnostics
        self.simulate_statements(&word.body, &mut state);

        // Collect resource kinds remaining on stack (these are "returned")
        let returns: Vec<ResourceKind> = state
            .remaining_resources()
            .into_iter()
            .map(|r| r.kind)
            .collect();

        WordResourceInfo { returns }
    }

    /// Simulate statements to track resources (no diagnostics)
    fn simulate_statements(&self, statements: &[Statement], state: &mut StackState) {
        for stmt in statements {
            self.simulate_statement(stmt, state);
        }
    }

    /// Simulate a single statement (simplified, no diagnostics)
    fn simulate_statement(&self, stmt: &Statement, state: &mut StackState) {
        match stmt {
            Statement::IntLiteral(_)
            | Statement::FloatLiteral(_)
            | Statement::BoolLiteral(_)
            | Statement::StringLiteral(_)
            | Statement::Symbol(_) => {
                state.push_unknown();
            }

            Statement::WordCall { name, span } => {
                self.simulate_word_call(name, span.as_ref(), state);
            }

            Statement::Quotation { .. } => {
                state.push_unknown();
            }

            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => {
                state.pop(); // condition
                let mut then_state = state.clone();
                let mut else_state = state.clone();
                self.simulate_statements(then_branch, &mut then_state);
                if let Some(else_stmts) = else_branch {
                    self.simulate_statements(else_stmts, &mut else_state);
                }
                *state = then_state.join(&else_state);
            }

            Statement::Match { arms, span: _ } => {
                state.pop();
                let mut arm_states: Vec<StackState> = Vec::new();
                for arm in arms {
                    let mut arm_state = state.clone();
                    self.simulate_statements(&arm.body, &mut arm_state);
                    arm_states.push(arm_state);
                }
                if let Some(joined) = arm_states.into_iter().reduce(|acc, s| acc.join(&s)) {
                    *state = joined;
                }
            }
        }
    }

    /// Simulate common word operations shared between first and second pass.
    ///
    /// Returns `true` if the word was handled, `false` if the caller should
    /// handle it (for pass-specific operations).
    ///
    /// The `on_resource_dropped` callback is invoked when a resource is dropped
    /// without being consumed. The second pass uses this to emit warnings.
    fn simulate_word_common<F>(
        name: &str,
        span: Option<&Span>,
        state: &mut StackState,
        word_info: &HashMap<String, WordResourceInfo>,
        mut on_resource_dropped: F,
    ) -> bool
    where
        F: FnMut(&TrackedResource),
    {
        let line = span.map(|s| s.line).unwrap_or(0);

        match name {
            // Resource-creating builtins
            "strand.weave" => {
                state.pop();
                state.push_resource(ResourceKind::WeaveHandle, line, name);
            }
            "chan.make" => {
                state.push_resource(ResourceKind::Channel, line, name);
            }

            // Resource-consuming builtins
            "strand.weave-cancel" => {
                if let Some(StackValue::Resource(r)) = state.pop()
                    && r.kind == ResourceKind::WeaveHandle
                {
                    state.consume_resource(r);
                }
            }
            "chan.close" => {
                if let Some(StackValue::Resource(r)) = state.pop()
                    && r.kind == ResourceKind::Channel
                {
                    state.consume_resource(r);
                }
            }

            // Stack operations
            "drop" => {
                let dropped = state.pop();
                if let Some(StackValue::Resource(r)) = dropped {
                    // Check if already consumed (e.g., via strand.spawn)
                    let already_consumed = state.consumed.iter().any(|c| c.id == r.id);
                    if !already_consumed {
                        on_resource_dropped(&r);
                    }
                }
            }
            "dup" => {
                // Only duplicate if there's something on the stack
                // Don't push unknown on empty - maintains original first-pass behavior
                if let Some(top) = state.peek().cloned() {
                    state.stack.push(top);
                }
            }
            "swap" => {
                let a = state.pop();
                let b = state.pop();
                if let Some(av) = a {
                    state.stack.push(av);
                }
                if let Some(bv) = b {
                    state.stack.push(bv);
                }
            }
            "over" => {
                // ( ..a x y -- ..a x y x )
                if state.depth() >= 2 {
                    let second = state.stack[state.depth() - 2].clone();
                    state.stack.push(second);
                }
            }
            "rot" => {
                // ( ..a x y z -- ..a y z x )
                let c = state.pop();
                let b = state.pop();
                let a = state.pop();
                if let Some(bv) = b {
                    state.stack.push(bv);
                }
                if let Some(cv) = c {
                    state.stack.push(cv);
                }
                if let Some(av) = a {
                    state.stack.push(av);
                }
            }
            "nip" => {
                // ( ..a x y -- ..a y ) - drops x, which may be a resource
                let b = state.pop();
                let a = state.pop();
                if let Some(StackValue::Resource(r)) = a {
                    let already_consumed = state.consumed.iter().any(|c| c.id == r.id);
                    if !already_consumed {
                        on_resource_dropped(&r);
                    }
                }
                if let Some(bv) = b {
                    state.stack.push(bv);
                }
            }
            ">aux" => {
                // Move top of main stack to aux stack (Issue #350)
                if let Some(val) = state.pop() {
                    state.aux_stack.push(val);
                }
            }
            "aux>" => {
                // Move top of aux stack back to main stack (Issue #350)
                if let Some(val) = state.aux_stack.pop() {
                    state.stack.push(val);
                }
            }
            "tuck" => {
                // ( ..a x y -- ..a y x y )
                let b = state.pop();
                let a = state.pop();
                if let Some(bv) = b.clone() {
                    state.stack.push(bv);
                }
                if let Some(av) = a {
                    state.stack.push(av);
                }
                if let Some(bv) = b {
                    state.stack.push(bv);
                }
            }

            // strand.spawn transfers resources
            "strand.spawn" => {
                state.pop();
                let resources: Vec<TrackedResource> = state
                    .stack
                    .iter()
                    .filter_map(|v| match v {
                        StackValue::Resource(r) => Some(r.clone()),
                        StackValue::Unknown => None,
                    })
                    .collect();
                for r in resources {
                    state.consume_resource(r);
                }
                state.push_unknown();
            }

            // Map operations that store values safely
            "map.set" => {
                // ( map key value -- map' ) - value is stored in map
                let value = state.pop();
                state.pop(); // key
                state.pop(); // map
                // Value is now safely stored in the map - consume if resource
                if let Some(StackValue::Resource(r)) = value {
                    state.consume_resource(r);
                }
                state.push_unknown(); // map'
            }

            // List operations that store values safely
            "list.push" | "list.prepend" => {
                // ( list value -- list' ) - value is stored in list
                let value = state.pop();
                state.pop(); // list
                if let Some(StackValue::Resource(r)) = value {
                    state.consume_resource(r);
                }
                state.push_unknown(); // list'
            }

            // User-defined words - check if we have info about them
            _ => {
                if let Some(info) = word_info.get(name) {
                    // Push resources that this word returns
                    for kind in &info.returns {
                        state.push_resource(*kind, line, name);
                    }
                    return true;
                }
                // Not handled - caller should handle pass-specific operations
                return false;
            }
        }
        true
    }

    /// Simulate a word call (for first pass)
    fn simulate_word_call(&self, name: &str, span: Option<&Span>, state: &mut StackState) {
        // First pass uses shared logic with no-op callback for dropped resources
        Self::simulate_word_common(name, span, state, &self.word_info, |_| {});
    }

    /// Second pass: Analyze a word with full cross-word context
    fn analyze_word_with_context(&mut self, word: &WordDef) {
        let mut state = StackState::new();

        self.analyze_statements_with_context(&word.body, &mut state, word);

        // Resources on stack at end are returned - no warning (escape analysis)
    }

    /// Analyze statements with diagnostics and cross-word tracking
    fn analyze_statements_with_context(
        &mut self,
        statements: &[Statement],
        state: &mut StackState,
        word: &WordDef,
    ) {
        for stmt in statements {
            self.analyze_statement_with_context(stmt, state, word);
        }
    }

    /// Analyze a single statement with cross-word context
    fn analyze_statement_with_context(
        &mut self,
        stmt: &Statement,
        state: &mut StackState,
        word: &WordDef,
    ) {
        match stmt {
            Statement::IntLiteral(_)
            | Statement::FloatLiteral(_)
            | Statement::BoolLiteral(_)
            | Statement::StringLiteral(_)
            | Statement::Symbol(_) => {
                state.push_unknown();
            }

            Statement::WordCall { name, span } => {
                self.analyze_word_call_with_context(name, span.as_ref(), state, word);
            }

            Statement::Quotation { .. } => {
                state.push_unknown();
            }

            Statement::If {
                then_branch,
                else_branch,
                span: _,
            } => {
                state.pop();
                let mut then_state = state.clone();
                let mut else_state = state.clone();

                self.analyze_statements_with_context(then_branch, &mut then_state, word);
                if let Some(else_stmts) = else_branch {
                    self.analyze_statements_with_context(else_stmts, &mut else_state, word);
                }

                let merge_result = then_state.merge(&else_state);
                for inconsistent in merge_result.inconsistent {
                    self.emit_branch_inconsistency_warning(&inconsistent, word);
                }

                *state = then_state.join(&else_state);
            }

            Statement::Match { arms, span: _ } => {
                state.pop();
                let mut arm_states: Vec<StackState> = Vec::new();

                for arm in arms {
                    let mut arm_state = state.clone();
                    self.analyze_statements_with_context(&arm.body, &mut arm_state, word);
                    arm_states.push(arm_state);
                }

                if arm_states.len() >= 2 {
                    let first = &arm_states[0];
                    for other in &arm_states[1..] {
                        let merge_result = first.merge(other);
                        for inconsistent in merge_result.inconsistent {
                            self.emit_branch_inconsistency_warning(&inconsistent, word);
                        }
                    }
                }

                if let Some(joined) = arm_states.into_iter().reduce(|acc, s| acc.join(&s)) {
                    *state = joined;
                }
            }
        }
    }

    /// Analyze a word call with cross-word tracking
    fn analyze_word_call_with_context(
        &mut self,
        name: &str,
        span: Option<&Span>,
        state: &mut StackState,
        word: &WordDef,
    ) {
        // Collect dropped resources to emit warnings after shared simulation
        let mut dropped_resources: Vec<TrackedResource> = Vec::new();

        // Try shared logic first
        let handled = Self::simulate_word_common(name, span, state, &self.word_info, |r| {
            dropped_resources.push(r.clone())
        });

        for r in dropped_resources {
            self.emit_drop_warning(&r, span, word);
        }

        if handled {
            return;
        }

        // Handle operations unique to the second pass
        match name {
            // strand.resume handling - can't be shared because it has complex stack behavior
            "strand.resume" => {
                let value = state.pop();
                let handle = state.pop();
                if let Some(h) = handle {
                    state.stack.push(h);
                } else {
                    state.push_unknown();
                }
                if let Some(v) = value {
                    state.stack.push(v);
                } else {
                    state.push_unknown();
                }
                state.push_unknown();
            }

            "2dup" => {
                if state.depth() >= 2 {
                    let b = state.stack[state.depth() - 1].clone();
                    let a = state.stack[state.depth() - 2].clone();
                    state.stack.push(a);
                    state.stack.push(b);
                } else {
                    state.push_unknown();
                    state.push_unknown();
                }
            }

            "3drop" => {
                for _ in 0..3 {
                    if let Some(StackValue::Resource(r)) = state.pop() {
                        let already_consumed = state.consumed.iter().any(|c| c.id == r.id);
                        if !already_consumed {
                            self.emit_drop_warning(&r, span, word);
                        }
                    }
                }
            }

            "pick" | "roll" => {
                state.pop();
                state.push_unknown();
            }

            "chan.send" | "chan.receive" => {
                state.pop();
                state.pop();
                state.push_unknown();
                state.push_unknown();
            }

            // Unknown words: leave stack unchanged (may cause false negatives)
            _ => {}
        }
    }

    fn emit_drop_warning(
        &mut self,
        resource: &TrackedResource,
        span: Option<&Span>,
        word: &WordDef,
    ) {
        let line = span
            .map(|s| s.line)
            .or_else(|| word.source.as_ref().map(|s| s.start_line))
            .unwrap_or(0);
        let column = span.map(|s| s.column);

        self.diagnostics.push(LintDiagnostic {
            id: format!("resource-leak-{}", resource.kind.name().to_lowercase()),
            message: format!(
                "{} from `{}` (line {}) dropped without cleanup - {}",
                resource.kind.name(),
                resource.created_by,
                resource.created_line + 1,
                resource.kind.cleanup_suggestion()
            ),
            severity: Severity::Warning,
            replacement: String::new(),
            file: self.file.clone(),
            line,
            end_line: None,
            start_column: column,
            end_column: column.map(|c| c + 4),
            word_name: word.name.clone(),
            start_index: 0,
            end_index: 0,
        });
    }

    fn emit_branch_inconsistency_warning(
        &mut self,
        inconsistent: &InconsistentResource,
        word: &WordDef,
    ) {
        let line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);
        let branch = if inconsistent.consumed_in_else {
            "else"
        } else {
            "then"
        };

        self.diagnostics.push(LintDiagnostic {
            id: "resource-branch-inconsistent".to_string(),
            message: format!(
                "{} from `{}` (line {}) is consumed in {} branch but not the other - all branches must handle resources consistently",
                inconsistent.resource.kind.name(),
                inconsistent.resource.created_by,
                inconsistent.resource.created_line + 1,
                branch
            ),
            severity: Severity::Warning,
            replacement: String::new(),
            file: self.file.clone(),
            line,
            end_line: None,
            start_column: None,
            end_column: None,
            word_name: word.name.clone(),
            start_index: 0,
            end_index: 0,
        });
    }
}
