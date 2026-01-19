//! Control Flow Code Generation
//!
//! This module handles if/else statements and match expressions,
//! including phi node merging and tail call handling.

use super::{BranchResult, CodeGen, CodeGenError, TailPosition};
use crate::ast::{MatchArm, Pattern, Statement};
use std::fmt::Write as _;

impl CodeGen {
    /// Generate code for an if statement with optional else branch
    ///
    /// Handles phi node merging for branches with different control flow.
    pub(super) fn codegen_if_statement(
        &mut self,
        stack_var: &str,
        then_branch: &[Statement],
        else_branch: Option<&Vec<Statement>>,
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Spill virtual registers before control flow (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Peek and pop condition: read bool value from top of stack and decrement SP
        let top_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            top_ptr, stack_var
        )?;
        let slot1_ptr = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr i64, ptr %{}, i64 1",
            slot1_ptr, top_ptr
        )?;
        let cond_val = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = load i64, ptr %{}",
            cond_val, slot1_ptr
        )?;
        let popped_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = getelementptr %Value, ptr %{}, i64 -1",
            popped_stack, stack_var
        )?;

        // Compare with 0 (0 = false, non-zero = true)
        let cmp_temp = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = icmp ne i64 %{}, 0",
            cmp_temp, cond_val
        )?;

        // Generate unique block labels
        let then_block = self.fresh_block("if_then");
        let else_block = self.fresh_block("if_else");
        let merge_block = self.fresh_block("if_merge");

        writeln!(
            &mut self.output,
            "  br i1 %{}, label %{}, label %{}",
            cmp_temp, then_block, else_block
        )?;

        // Then branch
        writeln!(&mut self.output, "{}:", then_block)?;
        let then_result = self.codegen_branch(
            then_branch,
            &popped_stack,
            position,
            &merge_block,
            "if_then",
        )?;

        // Else branch
        writeln!(&mut self.output, "{}:", else_block)?;
        let else_result = if let Some(eb) = else_branch {
            self.codegen_branch(eb, &popped_stack, position, &merge_block, "if_else")?
        } else {
            // No else clause - emit landing block with unchanged stack
            let else_pred = self.fresh_block("if_else_end");
            writeln!(&mut self.output, "  br label %{}", else_pred)?;
            writeln!(&mut self.output, "{}:", else_pred)?;
            writeln!(&mut self.output, "  br label %{}", merge_block)?;
            BranchResult {
                stack_var: popped_stack.clone(),
                emitted_tail_call: false,
                predecessor: else_pred,
            }
        };

        // If both branches emitted tail calls, no merge needed
        if then_result.emitted_tail_call && else_result.emitted_tail_call {
            return Ok(then_result.stack_var);
        }

        // Merge block with phi node
        writeln!(&mut self.output, "{}:", merge_block)?;
        let result_var = self.fresh_temp();

        if then_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, else_result.stack_var, else_result.predecessor
            )?;
        } else if else_result.emitted_tail_call {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ]",
                result_var, then_result.stack_var, then_result.predecessor
            )?;
        } else {
            writeln!(
                &mut self.output,
                "  %{} = phi ptr [ %{}, %{} ], [ %{}, %{} ]",
                result_var,
                then_result.stack_var,
                then_result.predecessor,
                else_result.stack_var,
                else_result.predecessor
            )?;
        }

        Ok(result_var)
    }

    /// Generate code for a match expression (pattern matching on union types)
    ///
    /// Match expressions use symbol-based tags (for SON support):
    /// 1. Get the variant's tag as a Symbol
    /// 2. Compare with each arm's variant name using string comparison
    /// 3. Jump to the matching arm using cascading if-else
    /// 4. In each arm, unpack the variant's fields onto the stack
    /// 5. Execute the arm's body
    /// 6. Merge control flow at the end
    pub(super) fn codegen_match_statement(
        &mut self,
        stack_var: &str,
        arms: &[MatchArm],
        position: TailPosition,
    ) -> Result<String, CodeGenError> {
        // Spill virtual registers before control flow (Issue #189)
        let stack_var = self.spill_virtual_stack(stack_var)?;
        let stack_var = stack_var.as_str();

        // Step 0: Check exhaustiveness
        if let Err((union_name, missing)) = self.check_match_exhaustiveness(arms) {
            return Err(CodeGenError::Logic(format!(
                "Non-exhaustive match on union '{}'. Missing variants: {}",
                union_name,
                missing.join(", ")
            )));
        }

        // Step 1: Duplicate the variant so we can get the tag without consuming it
        let dup_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_dup(ptr %{})",
            dup_stack, stack_var
        )?;

        // Step 2: Call variant-tag on the duplicate to get the tag as Symbol
        let tagged_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_variant_tag(ptr %{})",
            tagged_stack, dup_stack
        )?;

        // Now tagged_stack has the symbol tag on top, original variant below

        // Step 3: Prepare for cascading if-else pattern matching
        let default_block = self.fresh_block("match_unreachable");
        let merge_block = self.fresh_block("match_merge");

        // Collect arm info: (variant_name, block_name, field_count, field_names)
        let mut arm_info: Vec<(String, String, usize, Vec<String>)> = Vec::new();
        for (i, arm) in arms.iter().enumerate() {
            let block = self.fresh_block(&format!("match_arm_{}", i));
            let variant_name = match &arm.pattern {
                Pattern::Variant(name) => name.clone(),
                Pattern::VariantWithBindings { name, .. } => name.clone(),
            };
            let (_tag, field_count, field_names) = self.find_variant_info(&variant_name)?;
            arm_info.push((variant_name, block, field_count, field_names));
        }

        // Step 4-5: Generate cascading if-else dispatch (Issue #215: extracted helper)
        self.codegen_match_dispatch(&tagged_stack, &arm_info, &default_block)?;

        // Step 6: Generate each match arm (Issue #215: extracted helper)
        let arm_results =
            self.codegen_match_arms(stack_var, arms, &arm_info, position, &merge_block)?;

        // Step 7: Generate merge block with phi node (Issue #215: extracted helper)
        self.codegen_match_merge(&arm_results, &merge_block)
    }

    /// Extract fields from a variant using named bindings (Issue #213: extracted to reduce nesting).
    ///
    /// Uses variant_field_at to extract only the bound fields in binding order.
    /// For bindings [a, b, c], produces stack: ( a b c )
    pub(super) fn codegen_extract_variant_bindings(
        &mut self,
        stack_var: &str,
        bindings: &[String],
        field_names: &[String],
    ) -> Result<String, CodeGenError> {
        if bindings.is_empty() {
            // No bindings: just drop the variant
            let drop_stack = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_drop_op(ptr %{})",
                drop_stack, stack_var
            )?;
            return Ok(drop_stack);
        }

        let mut current_stack = stack_var.to_string();
        let last_idx = bindings.len() - 1;

        for (bind_idx, binding) in bindings.iter().enumerate() {
            let field_idx = field_names
                .iter()
                .position(|f| f == binding)
                .expect("binding validation should have caught unknown field");

            current_stack = if bind_idx != last_idx {
                self.codegen_extract_field_middle(&current_stack, field_idx)?
            } else {
                self.codegen_extract_field_last(&current_stack, field_idx)?
            };
        }

        Ok(current_stack)
    }

    /// Extract a field (not last) and keep variant on top: dup, push idx, field_at, swap
    pub(super) fn codegen_extract_field_middle(
        &mut self,
        stack_var: &str,
        field_idx: usize,
    ) -> Result<String, CodeGenError> {
        let dup_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_dup(ptr %{})",
            dup_stack, stack_var
        )?;

        let idx_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
            idx_stack, dup_stack, field_idx
        )?;

        let field_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_variant_field_at(ptr %{})",
            field_stack, idx_stack
        )?;

        let swap_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_swap(ptr %{})",
            swap_stack, field_stack
        )?;

        Ok(swap_stack)
    }

    /// Extract the last field (consumes variant): push idx, field_at
    pub(super) fn codegen_extract_field_last(
        &mut self,
        stack_var: &str,
        field_idx: usize,
    ) -> Result<String, CodeGenError> {
        let idx_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 {})",
            idx_stack, stack_var, field_idx
        )?;

        let field_stack = self.fresh_temp();
        writeln!(
            &mut self.output,
            "  %{} = call ptr @patch_seq_variant_field_at(ptr %{})",
            field_stack, idx_stack
        )?;

        Ok(field_stack)
    }

    /// Generate cascading if-else dispatch for match arms (Issue #215: extracted helper).
    ///
    /// Compares the tag symbol against each variant name, branching to the matching arm.
    pub(super) fn codegen_match_dispatch(
        &mut self,
        tagged_stack: &str,
        arm_info: &[(String, String, usize, Vec<String>)],
        default_block: &str,
    ) -> Result<(), CodeGenError> {
        let mut current_tag_stack = tagged_stack.to_string();

        for (i, (variant_name, arm_block, _, _)) in arm_info.iter().enumerate() {
            let is_last = i == arm_info.len() - 1;
            let next_check = if is_last {
                default_block.to_string()
            } else {
                self.fresh_block(&format!("match_check_{}", i + 1))
            };

            // For all but last arm: dup the tag, compare, branch
            // For last arm: just compare (tag will be consumed)
            let compare_stack = if !is_last {
                let dup = self.fresh_temp();
                writeln!(
                    &mut self.output,
                    "  %{} = call ptr @patch_seq_dup(ptr %{})",
                    dup, current_tag_stack
                )?;
                dup
            } else {
                current_tag_stack.clone()
            };

            // Compare symbol with C string
            let str_const = self.get_string_global(variant_name)?;
            let cmp_stack = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_symbol_eq_cstr(ptr %{}, ptr {})",
                cmp_stack, compare_stack, str_const
            )?;

            // Peek the bool result
            let cmp_val = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call i1 @patch_seq_peek_bool_value(ptr %{})",
                cmp_val, cmp_stack
            )?;

            // Pop the bool
            let popped = self.fresh_temp();
            writeln!(
                &mut self.output,
                "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
                popped, cmp_stack
            )?;

            // Branch: if true goto arm, else continue checking
            writeln!(
                &mut self.output,
                "  br i1 %{}, label %{}, label %{}",
                cmp_val, arm_block, next_check
            )?;

            // Start next check block (unless this was the last arm)
            if !is_last {
                writeln!(&mut self.output, "{}:", next_check)?;
                current_tag_stack = popped;
            }
        }

        // Generate unreachable default block (should never reach for exhaustive match)
        writeln!(&mut self.output, "{}:", default_block)?;
        writeln!(&mut self.output, "  unreachable")?;

        Ok(())
    }

    /// Generate code for each match arm body (Issue #215: extracted helper).
    pub(super) fn codegen_match_arms(
        &mut self,
        stack_var: &str,
        arms: &[MatchArm],
        arm_info: &[(String, String, usize, Vec<String>)],
        position: TailPosition,
        merge_block: &str,
    ) -> Result<Vec<BranchResult>, CodeGenError> {
        let mut arm_results: Vec<BranchResult> = Vec::new();
        for (i, (arm, (_variant_name, block, field_count, field_names))) in
            arms.iter().zip(arm_info.iter()).enumerate()
        {
            writeln!(&mut self.output, "{}:", block)?;

            // Extract fields based on pattern type
            let unpacked_stack = match &arm.pattern {
                Pattern::Variant(_) => {
                    // Stack-based: unpack all fields in declaration order
                    let result = self.fresh_temp();
                    writeln!(
                        &mut self.output,
                        "  %{} = call ptr @patch_seq_unpack_variant(ptr %{}, i64 {})",
                        result, stack_var, field_count
                    )?;
                    result
                }
                Pattern::VariantWithBindings { bindings, .. } => {
                    // Issue #213: Extracted to helper to reduce nesting
                    self.codegen_extract_variant_bindings(stack_var, bindings, field_names)?
                }
            };

            // Generate the arm body
            let result = self.codegen_branch(
                &arm.body,
                &unpacked_stack,
                position,
                merge_block,
                &format!("match_arm_{}", i),
            )?;
            arm_results.push(result);
        }
        Ok(arm_results)
    }

    /// Generate merge block with phi node for match (Issue #215: extracted helper).
    pub(super) fn codegen_match_merge(
        &mut self,
        arm_results: &[BranchResult],
        merge_block: &str,
    ) -> Result<String, CodeGenError> {
        // Check if all arms emitted tail calls
        let all_tail_calls = arm_results.iter().all(|r| r.emitted_tail_call);
        if all_tail_calls {
            // All branches tail-called, no merge needed
            return Ok(arm_results[0].stack_var.clone());
        }

        writeln!(&mut self.output, "{}:", merge_block)?;
        let result_var = self.fresh_temp();

        // Build phi node from non-tail-call branches
        let phi_entries: Vec<_> = arm_results
            .iter()
            .filter(|r| !r.emitted_tail_call)
            .map(|r| format!("[ %{}, %{} ]", r.stack_var, r.predecessor))
            .collect();

        if phi_entries.is_empty() {
            // Shouldn't happen if not all_tail_calls
            return Err(CodeGenError::Logic(
                "Match codegen: unexpected empty phi".to_string(),
            ));
        }

        writeln!(
            &mut self.output,
            "  %{} = phi ptr {}",
            result_var,
            phi_entries.join(", ")
        )?;

        Ok(result_var)
    }
}
