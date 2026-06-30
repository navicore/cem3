//! Loop Lowering
//!
//! Detects self-tail-recursive words matching the Phase 1 pattern and lowers
//! them to native LLVM loops instead of `musttail` calls. See
//! `docs/design/LOOP_LOWERING.md`.
//!
//! # Phase 1 Pattern
//!
//! A word qualifies when:
//! 1. It is directly self-recursive (calls itself) — *not* mutual recursion.
//! 2. Its body's last top-level statement is an `if/else/then` with both branches.
//! 3. Exactly one branch ends in a self-tail-call; the other (base) branch never
//!    calls self anywhere.
//! 4. The recursing branch's final statement is a plain `WordCall` to self.
//!
//! Statements before the trailing `if` are the loop "prelude" (condition setup);
//! they execute inside the loop each iteration.
//!
//! # Generated Structure
//!
//! ```text
//! entry:
//!   br label %loop
//! loop:
//!   %sp   = phi ptr [%entry_stack, %entry], [%cont_sp, %cont], [%cont_sp, %yield]
//!   %iter = phi i64 [0, %entry], [%iter_next, %cont], [%iter_next, %yield]
//!   ; prelude (condition setup) operating on %sp
//!   ; pop condition, compare, branch to base / continue
//! base:
//!   ; base-case body
//!   ret ptr %result
//! continue:
//!   ; recursion body minus the trailing self-call
//!   %cont_sp   = <spilled stack>            ; named via identity bitcast
//!   %iter_next = add i64 %iter, 1
//!   ; yield check every `loop_yield_cadence` iterations (AND mask, power of 2)
//!   br i1 %need_yield, label %yield, label %loop
//! yield:
//!   call void @patch_seq_maybe_yield()
//!   br label %loop
//! ```
//!
//! Phase 1 keeps values in memory (the virtual register stack is spilled at the
//! loop header). The phi is only over the stack pointer. Keeping virtual
//! registers live across the back-edge via phi nodes is a follow-up.

use super::{CodeGen, CodeGenError};
use crate::ast::{Statement, WordDef};
use crate::call_graph::CallGraph;
use std::fmt::Write as _;

/// A detected Phase 1 loop pattern, borrowing slices of the word body.
pub(super) struct LoopPattern<'a> {
    /// Statements before the trailing `if` (condition setup). Runs each iter.
    prelude: &'a [Statement],
    /// The base-case branch (does not call self). Becomes the loop exit.
    base_branch: &'a [Statement],
    /// The recursing branch. Its prefix (all but the trailing self-call)
    /// becomes the loop body; the trailing self-call becomes the back-edge.
    rec_branch: &'a [Statement],
    /// Whether the `if`'s `then` branch is the base case. Determines which way
    /// the condition branches: cond-true selects `then`, cond-false `else`.
    base_is_then: bool,
}

/// Detect the Phase 1 loop pattern on a word.
///
/// Returns `None` if the word is not self-recursive or doesn't match the shape.
pub(super) fn detect_loop_pattern<'a>(
    word: &'a WordDef,
    call_graph: &CallGraph,
) -> Option<LoopPattern<'a>> {
    if word.name == "main" {
        return None;
    }
    if !call_graph.is_self_recursive(&word.name) {
        return None;
    }

    let last = word.body.last()?;
    let (then_branch, else_branch) = match last {
        Statement::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => (then_branch.as_slice(), else_branch.as_slice()),
        _ => return None,
    };

    let then_recs = ends_in_self_call(then_branch, &word.name);
    let else_recs = ends_in_self_call(else_branch, &word.name);

    // Exactly one branch ends in a self-call.
    let (rec_branch, base_branch, base_is_then) = match (then_recs, else_recs) {
        (true, false) => (then_branch, else_branch, false), // then recurses, else is base
        (false, true) => (else_branch, then_branch, true),  // else recurses, then is base
        _ => return None,
    };

    // Base branch must not call self anywhere (it must terminate the loop).
    if calls_self(base_branch, &word.name) {
        return None;
    }

    let prelude = &word.body[..word.body.len() - 1];

    Some(LoopPattern {
        prelude,
        base_branch,
        rec_branch,
        base_is_then,
    })
}

/// True if the branch's last statement is a direct self-tail-call.
fn ends_in_self_call(branch: &[Statement], self_name: &str) -> bool {
    matches!(
        branch.last(),
        Some(Statement::WordCall { name, .. }) if name == self_name
    )
}

/// True if the branch calls `self_name` anywhere (descends into nested
/// if/match/quotation).
fn calls_self(branch: &[Statement], self_name: &str) -> bool {
    branch.iter().any(|s| stmt_calls_self(s, self_name))
}

fn stmt_calls_self(stmt: &Statement, self_name: &str) -> bool {
    match stmt {
        Statement::WordCall { name, .. } => name == self_name,
        Statement::If {
            then_branch,
            else_branch,
            ..
        } => {
            calls_self(then_branch, self_name)
                || else_branch
                    .as_ref()
                    .is_some_and(|eb| calls_self(eb, self_name))
        }
        Statement::Quotation { body, .. } => calls_self(body, self_name),
        Statement::Match { arms, .. } => arms.iter().any(|arm| calls_self(&arm.body, self_name)),
        _ => false,
    }
}

impl CodeGen {
    /// Emit a word body as a native LLVM loop (Phase 1).
    ///
    /// See the module docs for the generated structure. `entry_stack` is the
    /// stack pointer SSA name on function entry (`%stack`).
    pub(super) fn codegen_loop_body(
        &mut self,
        entry_stack: &str,
        pattern: &LoopPattern,
    ) -> Result<(), CodeGenError> {
        let loop_lbl = self.fresh_block("loop");
        let base_lbl = self.fresh_block("loop_base");
        let cont_lbl = self.fresh_block("loop_cont");
        let yield_lbl = self.fresh_block("loop_yield");

        // Reserved SSA names, referenced by the loop-header phis before their
        // definitions are emitted (legal forward reference in LLVM IR). These
        // use *named* values (not numbered `%N`) because LLVM requires
        // numbered temporaries to be defined in ascending order within a
        // function, but these are defined in the back-edge block — far below
        // the values emitted by the loop body.
        let loop_sp = self.fresh_named("loop_sp");
        let cont_sp = self.fresh_named("loop_cont_sp");
        let iter_phi = self.fresh_named("loop_iter");
        let iter_next = self.fresh_named("loop_iter_next");

        // Power-of-two cadence → AND mask (cadence - 1).
        let mask = (self.loop_yield_cadence as u64).saturating_sub(1);

        // The compile-time aux stack pointer each iteration starts from. The
        // loop body must balance >aux/aux> so the back-edge restores it; we
        // model that by resetting to this value at each iteration's start.
        let aux_sp_entry = self.current_aux_sp;

        // entry -> loop
        writeln!(&mut self.output, "  br label %{}", loop_lbl)?;

        // loop header: phis over the stack pointer and the iteration counter.
        // Both back-edge predecessors (`cont`, `yield`) carry the same values.
        writeln!(&mut self.output, "{}:", loop_lbl)?;
        writeln!(
            &mut self.output,
            "  %{} = phi ptr [ %{}, %entry ], [ %{}, %{} ], [ %{}, %{} ]",
            loop_sp, entry_stack, cont_sp, cont_lbl, cont_sp, yield_lbl
        )?;
        writeln!(
            &mut self.output,
            "  %{} = phi i64 [ 0, %entry ], [ %{}, %{} ], [ %{}, %{} ]",
            iter_phi, iter_next, cont_lbl, iter_next, yield_lbl
        )?;

        // Prelude (condition setup) operates on the phi'd stack pointer.
        self.virtual_stack.clear();
        self.current_aux_sp = aux_sp_entry;
        let cond_stack = self.codegen_statements(pattern.prelude, &loop_sp, false)?;

        // Pop the condition (mirrors codegen_if_statement), compare, branch.
        // cond-true selects the `then` branch; cond-false the `else` branch.
        let cond_stack = self.spill_virtual_stack(&cond_stack)?;
        let top_ptr = self.emit_stack_gep(&cond_stack, -1)?;
        let cond_val = self.emit_load_int_payload(&top_ptr)?;
        let popped = top_ptr; // new SP after consuming the condition
        let cmp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp, cond_val
        )?;
        let (true_lbl, false_lbl) = if pattern.base_is_then {
            (base_lbl.as_str(), cont_lbl.as_str())
        } else {
            (cont_lbl.as_str(), base_lbl.as_str())
        };
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp, true_lbl, false_lbl
        )?;

        // base: base-case body, then return (loop exit).
        writeln!(&mut self.output, "{}:", base_lbl)?;
        self.virtual_stack.clear();
        self.current_aux_sp = aux_sp_entry;
        let base_stack = self.codegen_statements(pattern.base_branch, &popped, false)?;
        let base_stack = self.spill_virtual_stack(&base_stack)?;
        writeln!(&mut self.output, "  ret ptr %{}", base_stack)?;

        // continue: recursion body minus the trailing self-call, then the
        // yield check and back-edge.
        writeln!(&mut self.output, "{}:", cont_lbl)?;
        self.virtual_stack.clear();
        self.current_aux_sp = aux_sp_entry;
        // All statements except the final self-call form the next-iteration
        // stack transformation.
        let rec_prefix = &pattern.rec_branch[..pattern.rec_branch.len() - 1];
        let cont_stack = self.codegen_statements(rec_prefix, &popped, false)?;
        let cont_stack = self.spill_virtual_stack(&cont_stack)?;
        // Bind the spilled stack to the reserved phi name via an identity
        // bitcast (a no-op that LLVM removes).
        writeln!(
            &mut self.output,
            "  %{} = bitcast ptr %{} to ptr",
            cont_sp, cont_stack
        )?;
        // Advance the iteration counter and check the yield cadence.
        writeln!(
            &mut self.output,
            "  %{} = add i64 %{}, 1",
            iter_next, iter_phi
        )?;
        let masked = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = and i64 %{}, {}",
            masked, iter_next, mask
        )?;
        let need_yield = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp eq i64 %{}, 0",
            need_yield, masked
        )?;
        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            need_yield, yield_lbl, loop_lbl
        )?;

        // yield: cooperative yield, then back to the loop header.
        writeln!(&mut self.output, "{}:", yield_lbl)?;
        writeln!(&mut self.output, "  call void @patch_seq_maybe_yield()")?;
        writeln!(&mut self.output, "  br label %{}", loop_lbl)?;

        Ok(())
    }
}
