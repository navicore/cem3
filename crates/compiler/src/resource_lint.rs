//! Resource Leak Detection (Phase 2a)
//!
//! Data flow analysis to detect resource leaks within single word definitions.
//! Tracks resources (weave handles, channels) through stack operations and
//! control flow to ensure proper cleanup.
//!
//! # Architecture
//!
//! 1. **Resource Tagging**: Values from resource-creating words are tagged
//!    with their creation location.
//!
//! 2. **Stack Simulation**: Abstract interpretation tracks tagged values
//!    through stack operations (dup, swap, drop, etc.).
//!
//! 3. **Control Flow**: If/else and match branches must handle resources
//!    consistently - either all consume or all preserve.
//!
//! 4. **Escape Analysis**: Resources returned from a word are the caller's
//!    responsibility - no warning emitted.
//!
//! # Known Limitations
//!
//! - **`strand.resume` completion not tracked**: When `strand.resume` returns
//!   false, the weave completed and handle is consumed. We can't determine this
//!   statically, so we assume the handle remains active. Use pattern-based lint
//!   rules to catch unchecked resume results.
//!
//! - **Unknown word effects**: User-defined words and FFI calls have unknown
//!   stack effects. We conservatively leave the stack unchanged, which may
//!   cause false negatives if those words consume or create resources.
//!
//! - **No cross-word analysis**: Resources returned from a word are the
//!   caller's responsibility. Phase 2b could track these across call sites.

use crate::ast::{MatchArm, Span, Statement, WordDef};
use crate::lint::{LintDiagnostic, Severity};
use std::collections::HashMap;
use std::path::Path;

/// Identifies a resource type for tracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    /// Weave handle from `strand.weave`
    WeaveHandle,
    /// Channel from `chan.make`
    Channel,
}

impl ResourceKind {
    fn name(&self) -> &'static str {
        match self {
            ResourceKind::WeaveHandle => "WeaveHandle",
            ResourceKind::Channel => "Channel",
        }
    }

    fn cleanup_suggestion(&self) -> &'static str {
        match self {
            ResourceKind::WeaveHandle => "use `strand.weave-cancel` or resume to completion",
            ResourceKind::Channel => "use `chan.close` when done",
        }
    }
}

/// A tracked resource with its origin
#[derive(Debug, Clone)]
pub struct TrackedResource {
    /// What kind of resource this is
    pub kind: ResourceKind,
    /// Unique ID for this resource instance
    pub id: usize,
    /// Line where the resource was created (0-indexed)
    pub created_line: usize,
    /// Column where the resource was created (0-indexed)
    pub created_column: usize,
    /// The word that created this resource
    pub created_by: String,
}

/// A value on the abstract stack - either a resource or unknown
#[derive(Debug, Clone)]
pub enum StackValue {
    /// A tracked resource
    Resource(TrackedResource),
    /// An unknown value (literal, result of non-resource operation)
    Unknown,
}

/// State of the abstract stack during analysis
#[derive(Debug, Clone)]
pub struct StackState {
    /// The stack contents (top is last element)
    stack: Vec<StackValue>,
    /// Resources that have been properly consumed
    consumed: Vec<TrackedResource>,
    /// Next resource ID to assign
    next_id: usize,
}

impl Default for StackState {
    fn default() -> Self {
        Self::new()
    }
}

impl StackState {
    pub fn new() -> Self {
        StackState {
            stack: Vec::new(),
            consumed: Vec::new(),
            next_id: 0,
        }
    }

    /// Push an unknown value onto the stack
    pub fn push_unknown(&mut self) {
        self.stack.push(StackValue::Unknown);
    }

    /// Push a new tracked resource onto the stack
    pub fn push_resource(&mut self, kind: ResourceKind, line: usize, column: usize, word: &str) {
        let resource = TrackedResource {
            kind,
            id: self.next_id,
            created_line: line,
            created_column: column,
            created_by: word.to_string(),
        };
        self.next_id += 1;
        self.stack.push(StackValue::Resource(resource));
    }

    /// Pop a value from the stack
    pub fn pop(&mut self) -> Option<StackValue> {
        self.stack.pop()
    }

    /// Peek at the top value without removing it
    pub fn peek(&self) -> Option<&StackValue> {
        self.stack.last()
    }

    /// Get stack depth
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Mark a resource as consumed (properly cleaned up)
    pub fn consume_resource(&mut self, resource: TrackedResource) {
        self.consumed.push(resource);
    }

    /// Get all resources still on the stack (potential leaks)
    pub fn remaining_resources(&self) -> Vec<&TrackedResource> {
        self.stack
            .iter()
            .filter_map(|v| match v {
                StackValue::Resource(r) => Some(r),
                StackValue::Unknown => None,
            })
            .collect()
    }

    /// Merge two stack states (for branch unification)
    /// Returns resources that are leaked in one branch but not the other
    pub fn merge(&self, other: &StackState) -> BranchMergeResult {
        let self_resources: HashMap<usize, &TrackedResource> = self
            .stack
            .iter()
            .filter_map(|v| match v {
                StackValue::Resource(r) => Some((r.id, r)),
                StackValue::Unknown => None,
            })
            .collect();

        let other_resources: HashMap<usize, &TrackedResource> = other
            .stack
            .iter()
            .filter_map(|v| match v {
                StackValue::Resource(r) => Some((r.id, r)),
                StackValue::Unknown => None,
            })
            .collect();

        let self_consumed: std::collections::HashSet<usize> =
            self.consumed.iter().map(|r| r.id).collect();
        let other_consumed: std::collections::HashSet<usize> =
            other.consumed.iter().map(|r| r.id).collect();

        let mut inconsistent = Vec::new();

        // Find resources consumed in one branch but not the other
        for (id, resource) in &self_resources {
            if other_consumed.contains(id) && !self_consumed.contains(id) {
                // Consumed in 'other' branch, still on stack in 'self'
                inconsistent.push(InconsistentResource {
                    resource: (*resource).clone(),
                    consumed_in_else: true,
                });
            }
        }

        for (id, resource) in &other_resources {
            if self_consumed.contains(id) && !other_consumed.contains(id) {
                // Consumed in 'self' branch, still on stack in 'other'
                inconsistent.push(InconsistentResource {
                    resource: (*resource).clone(),
                    consumed_in_else: false,
                });
            }
        }

        BranchMergeResult { inconsistent }
    }

    /// Compute a lattice join of two stack states for continuation after branches.
    ///
    /// The join is conservative:
    /// - Resources present in EITHER branch are tracked (we don't know which path was taken)
    /// - Resources are only marked consumed if consumed in BOTH branches
    /// - The next_id is taken from the max of both states
    ///
    /// This ensures we don't miss potential leaks from either branch.
    pub fn join(&self, other: &StackState) -> StackState {
        // Collect resource IDs consumed in each branch
        let other_consumed: std::collections::HashSet<usize> =
            other.consumed.iter().map(|r| r.id).collect();

        // Resources consumed in BOTH branches are definitely consumed
        let definitely_consumed: Vec<TrackedResource> = self
            .consumed
            .iter()
            .filter(|r| other_consumed.contains(&r.id))
            .cloned()
            .collect();

        // For the stack, we need to be careful. After if/else, stacks should
        // have the same depth (Seq requires balanced stack effects in branches).
        // We take the union of resources - if a resource appears in either
        // branch's stack, it should be tracked.
        //
        // Since we can't know which branch was taken, we use the then-branch
        // stack structure but ensure any resource from either branch is present.
        let mut joined_stack = self.stack.clone();

        // Collect resources from other branch that might not be in self
        let other_resources: HashMap<usize, TrackedResource> = other
            .stack
            .iter()
            .filter_map(|v| match v {
                StackValue::Resource(r) => Some((r.id, r.clone())),
                StackValue::Unknown => None,
            })
            .collect();

        // For each position, if other has a resource that self doesn't, use other's
        for (i, val) in joined_stack.iter_mut().enumerate() {
            if matches!(val, StackValue::Unknown)
                && i < other.stack.len()
                && let StackValue::Resource(r) = &other.stack[i]
            {
                *val = StackValue::Resource(r.clone());
            }
        }

        // Also check if other branch has resources we should track
        // (in case stacks have different structures due to analysis imprecision)
        let self_resource_ids: std::collections::HashSet<usize> = joined_stack
            .iter()
            .filter_map(|v| match v {
                StackValue::Resource(r) => Some(r.id),
                StackValue::Unknown => None,
            })
            .collect();

        for (id, resource) in other_resources {
            if !self_resource_ids.contains(&id) && !definitely_consumed.iter().any(|r| r.id == id) {
                // Resource from other branch not in our stack - add it
                // This handles cases where branches have different stack shapes
                joined_stack.push(StackValue::Resource(resource));
            }
        }

        StackState {
            stack: joined_stack,
            consumed: definitely_consumed,
            next_id: self.next_id.max(other.next_id),
        }
    }
}

/// Result of merging two branch states
#[derive(Debug)]
pub struct BranchMergeResult {
    /// Resources handled inconsistently between branches
    pub inconsistent: Vec<InconsistentResource>,
}

/// A resource handled differently in different branches
#[derive(Debug)]
pub struct InconsistentResource {
    pub resource: TrackedResource,
    /// True if consumed in else branch but not then branch
    pub consumed_in_else: bool,
}

/// The resource leak analyzer
pub struct ResourceAnalyzer {
    /// Diagnostics collected during analysis
    diagnostics: Vec<LintDiagnostic>,
    /// File being analyzed
    file: std::path::PathBuf,
}

impl ResourceAnalyzer {
    pub fn new(file: &Path) -> Self {
        ResourceAnalyzer {
            diagnostics: Vec::new(),
            file: file.to_path_buf(),
        }
    }

    /// Analyze a word definition for resource leaks
    pub fn analyze_word(&mut self, word: &WordDef) -> Vec<LintDiagnostic> {
        self.diagnostics.clear();

        let mut state = StackState::new();

        // Analyze the word body
        self.analyze_statements(&word.body, &mut state, word);

        // Check for leaked resources at end of word
        // Note: Resources still on stack at word end could be:
        // 1. Intentionally returned (escape) - caller's responsibility
        // 2. Leaked - forgot to clean up
        //
        // For Phase 2a, we apply escape analysis: if a resource is still
        // on the stack at word end, it's being returned to the caller.
        // This is valid - the caller becomes responsible for cleanup.
        // We only warn about resources that are explicitly dropped without
        // cleanup, or handled inconsistently across branches.
        //
        // Phase 2b could add cross-word analysis to track if callers
        // properly handle returned resources.
        let _ = state.remaining_resources(); // Intentional: escape = no warning

        std::mem::take(&mut self.diagnostics)
    }

    /// Analyze a sequence of statements
    fn analyze_statements(
        &mut self,
        statements: &[Statement],
        state: &mut StackState,
        word: &WordDef,
    ) {
        for stmt in statements {
            self.analyze_statement(stmt, state, word);
        }
    }

    /// Analyze a single statement
    fn analyze_statement(&mut self, stmt: &Statement, state: &mut StackState, word: &WordDef) {
        match stmt {
            Statement::IntLiteral(_)
            | Statement::FloatLiteral(_)
            | Statement::BoolLiteral(_)
            | Statement::StringLiteral(_) => {
                state.push_unknown();
            }

            Statement::WordCall { name, span } => {
                self.analyze_word_call(name, span.as_ref(), state, word);
            }

            Statement::Quotation { body, .. } => {
                // Quotations capture the current stack conceptually but don't
                // execute immediately. For now, just push an unknown value
                // (the quotation itself). We could analyze the body when
                // we see `call`, but that's Phase 2b.
                let _ = body; // Acknowledge we're not analyzing the body yet
                state.push_unknown();
            }

            Statement::If {
                then_branch,
                else_branch,
            } => {
                self.analyze_if(then_branch, else_branch.as_ref(), state, word);
            }

            Statement::Match { arms } => {
                self.analyze_match(arms, state, word);
            }
        }
    }

    /// Analyze a word call
    fn analyze_word_call(
        &mut self,
        name: &str,
        span: Option<&Span>,
        state: &mut StackState,
        word: &WordDef,
    ) {
        let line = span.map(|s| s.line).unwrap_or(0);
        let column = span.map(|s| s.column).unwrap_or(0);

        match name {
            // Resource-creating words
            "strand.weave" => {
                // Pops quotation, pushes WeaveHandle
                state.pop(); // quotation
                state.push_resource(ResourceKind::WeaveHandle, line, column, name);
            }

            "chan.make" => {
                // Pushes a new channel
                state.push_resource(ResourceKind::Channel, line, column, name);
            }

            // Resource-consuming words
            "strand.weave-cancel" => {
                // Pops and consumes WeaveHandle
                if let Some(StackValue::Resource(r)) = state.pop()
                    && r.kind == ResourceKind::WeaveHandle
                {
                    state.consume_resource(r);
                }
            }

            "chan.close" => {
                // Pops and consumes Channel
                if let Some(StackValue::Resource(r)) = state.pop()
                    && r.kind == ResourceKind::Channel
                {
                    state.consume_resource(r);
                }
            }

            // strand.resume is special - it returns (handle value bool)
            // If bool is false, the weave completed and handle is consumed
            // We can't know statically, so we just track that the handle
            // is still in play (on the stack after resume)
            "strand.resume" => {
                // Pops (handle value), pushes (handle value bool)
                let value = state.pop(); // value to send
                let handle = state.pop(); // handle

                // Push them back plus the bool result
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
                state.push_unknown(); // bool result
            }

            // Stack operations
            "drop" => {
                let dropped = state.pop();
                // If we dropped a resource without consuming it properly, that's a leak
                // But check if it was already consumed (e.g., transferred via strand.spawn)
                if let Some(StackValue::Resource(r)) = dropped {
                    let already_consumed = state.consumed.iter().any(|c| c.id == r.id);
                    if !already_consumed {
                        self.emit_drop_warning(&r, span, word);
                    }
                }
            }

            "dup" => {
                if let Some(top) = state.peek().cloned() {
                    state.stack.push(top);
                } else {
                    state.push_unknown();
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
                // ( a b -- a b a ) - copy second element to top
                if state.depth() >= 2 {
                    let second = state.stack[state.depth() - 2].clone();
                    state.stack.push(second);
                } else {
                    state.push_unknown();
                }
            }

            "rot" => {
                // ( a b c -- b c a )
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
                // ( a b -- b ) - drop second
                let b = state.pop();
                let a = state.pop();
                if let Some(StackValue::Resource(r)) = a {
                    let already_consumed = state.consumed.iter().any(|c| c.id == r.id);
                    if !already_consumed {
                        self.emit_drop_warning(&r, span, word);
                    }
                }
                if let Some(bv) = b {
                    state.stack.push(bv);
                }
            }

            "tuck" => {
                // ( a b -- b a b )
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

            "2dup" => {
                // ( a b -- a b a b )
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

            "pick" => {
                // ( ... n -- ... value_at_n )
                // We can't know n statically, so just push unknown
                state.pop(); // pop n
                state.push_unknown();
            }

            "roll" => {
                // Similar to pick but also removes the item
                state.pop(); // pop n
                state.push_unknown();
            }

            // Channel operations that don't consume
            "chan.send" | "chan.receive" => {
                // These use the channel but don't consume it
                // chan.send: ( chan value -- bool )
                // chan.receive: ( chan -- value bool )
                state.pop();
                state.pop();
                state.push_unknown();
                state.push_unknown();
            }

            // strand.spawn clones the stack to the child strand
            // Resources on the stack are transferred to child's responsibility
            "strand.spawn" => {
                // Pops quotation, pushes strand-id
                // All resources currently on stack are now shared with child
                // Mark them as consumed since child takes responsibility
                state.pop(); // quotation
                let resources_on_stack: Vec<TrackedResource> = state
                    .stack
                    .iter()
                    .filter_map(|v| match v {
                        StackValue::Resource(r) => Some(r.clone()),
                        StackValue::Unknown => None,
                    })
                    .collect();
                for r in resources_on_stack {
                    state.consume_resource(r);
                }
                state.push_unknown(); // strand-id
            }

            // For any other word, we don't know its stack effect
            // Conservatively, we could assume it consumes/produces unknown values
            // For now, we just leave the stack unchanged (may cause false positives)
            _ => {
                // Unknown word - could be user-defined
                // We'd need type info to know its stack effect
                // For Phase 2a, we'll be conservative and do nothing
            }
        }
    }

    /// Analyze an if/else statement
    fn analyze_if(
        &mut self,
        then_branch: &[Statement],
        else_branch: Option<&Vec<Statement>>,
        state: &mut StackState,
        word: &WordDef,
    ) {
        // Pop the condition
        state.pop();

        // Clone state for each branch
        let mut then_state = state.clone();
        let mut else_state = state.clone();

        // Analyze then branch
        self.analyze_statements(then_branch, &mut then_state, word);

        // Analyze else branch if present
        if let Some(else_stmts) = else_branch {
            self.analyze_statements(else_stmts, &mut else_state, word);
        }

        // Check for inconsistent resource handling between branches
        let merge_result = then_state.merge(&else_state);
        for inconsistent in merge_result.inconsistent {
            self.emit_branch_inconsistency_warning(&inconsistent, word);
        }

        // Compute proper lattice join of both branch states
        // This ensures we track resources from either branch and only
        // consider resources consumed if consumed in BOTH branches
        *state = then_state.join(&else_state);
    }

    /// Analyze a match statement
    fn analyze_match(&mut self, arms: &[MatchArm], state: &mut StackState, word: &WordDef) {
        // Pop the matched value
        state.pop();

        if arms.is_empty() {
            return;
        }

        // Analyze each arm
        let mut arm_states: Vec<StackState> = Vec::new();

        for arm in arms {
            let mut arm_state = state.clone();

            // Match arms may push extracted fields - for now we push unknowns
            // based on the pattern (simplified)
            match &arm.pattern {
                crate::ast::Pattern::Variant(_) => {
                    // Variant match pushes all fields - we don't know how many
                    // so we just continue with current state
                }
                crate::ast::Pattern::VariantWithBindings { bindings, .. } => {
                    // Push unknowns for each binding
                    for _ in bindings {
                        arm_state.push_unknown();
                    }
                }
            }

            self.analyze_statements(&arm.body, &mut arm_state, word);
            arm_states.push(arm_state);
        }

        // Check consistency between all arms
        if arm_states.len() >= 2 {
            let first = &arm_states[0];
            for other in &arm_states[1..] {
                let merge_result = first.merge(other);
                for inconsistent in merge_result.inconsistent {
                    self.emit_branch_inconsistency_warning(&inconsistent, word);
                }
            }
        }

        // Compute proper lattice join of all arm states
        // Resources are only consumed if consumed in ALL arms
        if let Some(first) = arm_states.into_iter().reduce(|acc, s| acc.join(&s)) {
            *state = first;
        }
    }

    /// Emit a warning for a resource dropped without cleanup
    fn emit_drop_warning(
        &mut self,
        resource: &TrackedResource,
        span: Option<&Span>,
        word: &WordDef,
    ) {
        let line = span
            .map(|s| s.line)
            .unwrap_or_else(|| word.source.as_ref().map(|s| s.start_line).unwrap_or(0));
        let column = span.map(|s| s.column);

        self.diagnostics.push(LintDiagnostic {
            id: format!("resource-leak-{}", resource.kind.name().to_lowercase()),
            message: format!(
                "{} created at line {} dropped without cleanup - {}",
                resource.kind.name(),
                resource.created_line + 1,
                resource.kind.cleanup_suggestion()
            ),
            severity: Severity::Warning,
            replacement: String::new(),
            file: self.file.clone(),
            line,
            end_line: None,
            start_column: column,
            end_column: column.map(|c| c + 4), // approximate
            word_name: word.name.clone(),
            start_index: 0,
            end_index: 0,
        });
    }

    /// Emit a warning for inconsistent resource handling between branches
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
                "{} created at line {} is consumed in {} branch but not the other - all branches must handle resources consistently",
                inconsistent.resource.kind.name(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Statement, WordDef};

    fn make_word_call(name: &str) -> Statement {
        Statement::WordCall {
            name: name.to_string(),
            span: Some(Span::new(0, 0, name.len())),
        }
    }

    #[test]
    fn test_immediate_weave_drop() {
        // : bad ( -- ) [ gen ] strand.weave drop ;
        let word = WordDef {
            name: "bad".to_string(),
            effect: None,
            body: vec![
                Statement::Quotation {
                    id: 0,
                    body: vec![make_word_call("gen")],
                },
                make_word_call("strand.weave"),
                make_word_call("drop"),
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].id.contains("weavehandle"));
        assert!(diagnostics[0].message.contains("dropped without cleanup"));
    }

    #[test]
    fn test_weave_properly_cancelled() {
        // : good ( -- ) [ gen ] strand.weave strand.weave-cancel ;
        let word = WordDef {
            name: "good".to_string(),
            effect: None,
            body: vec![
                Statement::Quotation {
                    id: 0,
                    body: vec![make_word_call("gen")],
                },
                make_word_call("strand.weave"),
                make_word_call("strand.weave-cancel"),
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert!(
            diagnostics.is_empty(),
            "Expected no warnings for properly cancelled weave"
        );
    }

    #[test]
    fn test_branch_inconsistent_handling() {
        // : bad ( -- )
        //   [ gen ] strand.weave
        //   true if strand.weave-cancel else drop then ;
        let word = WordDef {
            name: "bad".to_string(),
            effect: None,
            body: vec![
                Statement::Quotation {
                    id: 0,
                    body: vec![make_word_call("gen")],
                },
                make_word_call("strand.weave"),
                Statement::BoolLiteral(true),
                Statement::If {
                    then_branch: vec![make_word_call("strand.weave-cancel")],
                    else_branch: Some(vec![make_word_call("drop")]),
                },
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        // Should warn about drop in else branch
        assert!(!diagnostics.is_empty());
    }

    #[test]
    fn test_both_branches_cancel() {
        // : good ( -- )
        //   [ gen ] strand.weave
        //   true if strand.weave-cancel else strand.weave-cancel then ;
        let word = WordDef {
            name: "good".to_string(),
            effect: None,
            body: vec![
                Statement::Quotation {
                    id: 0,
                    body: vec![make_word_call("gen")],
                },
                make_word_call("strand.weave"),
                Statement::BoolLiteral(true),
                Statement::If {
                    then_branch: vec![make_word_call("strand.weave-cancel")],
                    else_branch: Some(vec![make_word_call("strand.weave-cancel")]),
                },
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert!(
            diagnostics.is_empty(),
            "Expected no warnings when both branches cancel"
        );
    }

    #[test]
    fn test_channel_leak() {
        // : bad ( -- ) chan.make drop ;
        let word = WordDef {
            name: "bad".to_string(),
            effect: None,
            body: vec![make_word_call("chan.make"), make_word_call("drop")],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].id.contains("channel"));
    }

    #[test]
    fn test_channel_properly_closed() {
        // : good ( -- ) chan.make chan.close ;
        let word = WordDef {
            name: "good".to_string(),
            effect: None,
            body: vec![make_word_call("chan.make"), make_word_call("chan.close")],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert!(
            diagnostics.is_empty(),
            "Expected no warnings for properly closed channel"
        );
    }

    #[test]
    fn test_swap_resource_tracking() {
        // : test ( -- ) chan.make 1 swap drop drop ;
        // After swap: chan is on top, 1 is second
        // First drop removes chan (should warn), second drop removes 1
        let word = WordDef {
            name: "test".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                Statement::IntLiteral(1),
                make_word_call("swap"),
                make_word_call("drop"), // drops chan - should warn
                make_word_call("drop"), // drops 1
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert_eq!(
            diagnostics.len(),
            1,
            "Expected warning for dropped channel: {:?}",
            diagnostics
        );
        assert!(diagnostics[0].id.contains("channel"));
    }

    #[test]
    fn test_over_resource_tracking() {
        // : test ( -- ) chan.make 1 over drop drop drop ;
        // Stack after chan.make: (chan)
        // Stack after 1: (chan 1)
        // Stack after over: (chan 1 chan) - chan copied to top
        // Both chan references are dropped without cleanup - both warn
        let word = WordDef {
            name: "test".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                Statement::IntLiteral(1),
                make_word_call("over"),
                make_word_call("drop"), // drops copied chan - warns
                make_word_call("drop"), // drops 1
                make_word_call("drop"), // drops original chan - also warns
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        // Both channel drops warn (they share ID but neither was properly consumed)
        assert_eq!(
            diagnostics.len(),
            2,
            "Expected 2 warnings for dropped channels: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_channel_transferred_via_spawn() {
        // Pattern from shopping-cart: channel transferred to spawned worker
        // : accept-loop ( -- )
        //   chan.make                  # create channel
        //   dup [ worker ] strand.spawn  # transfer to worker
        //   drop drop                  # drop strand-id and dup'd chan
        //   chan.send                  # use remaining chan
        // ;
        let word = WordDef {
            name: "accept-loop".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                make_word_call("dup"),
                Statement::Quotation {
                    id: 0,
                    body: vec![make_word_call("worker")],
                },
                make_word_call("strand.spawn"),
                make_word_call("drop"),
                make_word_call("drop"),
                make_word_call("chan.send"),
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert!(
            diagnostics.is_empty(),
            "Expected no warnings when channel is transferred via strand.spawn: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_else_branch_only_leak() {
        // : test ( -- )
        //   chan.make
        //   true if chan.close else drop then ;
        // The else branch drops without cleanup - should warn about inconsistency
        // AND the join should track that the resource might not be consumed
        let word = WordDef {
            name: "test".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                Statement::BoolLiteral(true),
                Statement::If {
                    then_branch: vec![make_word_call("chan.close")],
                    else_branch: Some(vec![make_word_call("drop")]),
                },
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        // Should have warnings: branch inconsistency + drop without cleanup
        assert!(
            !diagnostics.is_empty(),
            "Expected warnings for else-branch leak: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_branch_join_both_consume() {
        // : test ( -- )
        //   chan.make
        //   true if chan.close else chan.close then ;
        // Both branches properly consume - no warnings
        let word = WordDef {
            name: "test".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                Statement::BoolLiteral(true),
                Statement::If {
                    then_branch: vec![make_word_call("chan.close")],
                    else_branch: Some(vec![make_word_call("chan.close")]),
                },
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert!(
            diagnostics.is_empty(),
            "Expected no warnings when both branches consume: {:?}",
            diagnostics
        );
    }

    #[test]
    fn test_branch_join_neither_consume() {
        // : test ( -- )
        //   chan.make
        //   true if else then drop ;
        // Neither branch consumes, then drop after - should warn
        let word = WordDef {
            name: "test".to_string(),
            effect: None,
            body: vec![
                make_word_call("chan.make"),
                Statement::BoolLiteral(true),
                Statement::If {
                    then_branch: vec![],
                    else_branch: Some(vec![]),
                },
                make_word_call("drop"), // drops the channel
            ],
            source: None,
        };

        let mut analyzer = ResourceAnalyzer::new(Path::new("test.seq"));
        let diagnostics = analyzer.analyze_word(&word);

        assert_eq!(
            diagnostics.len(),
            1,
            "Expected warning for dropped channel: {:?}",
            diagnostics
        );
        assert!(diagnostics[0].id.contains("channel"));
    }
}
