//! Type Helper Functions
//!
//! This module handles type lookups, variant info, and exhaustiveness checking
//! for match statements.

use super::{CodeGen, CodeGenError};
use crate::ast::{MatchArm, Pattern, UnionDef};
use crate::types::Type;

impl CodeGen {
    /// Get the next quotation type (consumes it in DFS traversal order)
    /// Get the inferred type for a quotation by its ID
    pub(super) fn get_quotation_type(&self, id: usize) -> Result<&Type, CodeGenError> {
        self.type_map.get(&id).ok_or_else(|| {
            CodeGenError::Logic(format!(
                "CodeGen: no type information for quotation ID {}. This is a compiler bug.",
                id
            ))
        })
    }

    /// Check if top of stack at current statement is trivially copyable (Int, Float, Bool)
    /// These types can be duplicated with a simple memcpy instead of calling clone_value
    pub(super) fn is_trivially_copyable_at_current_stmt(&self) -> bool {
        // Only look up type info for top-level word body statements (depth 1)
        // Depth is incremented at entry to codegen_statements, so:
        // - First call (word body): runs at depth 1 (allow lookups)
        // - Nested calls (loop bodies, branches): run at depth > 1 (disable lookups)
        // This prevents index collisions between outer and inner statement indices
        if self.codegen_depth > 1 {
            return false;
        }
        if let Some(word_name) = &self.current_word_name {
            let key = (word_name.clone(), self.current_stmt_index);
            if let Some(ty) = self.statement_types.get(&key) {
                return matches!(ty, Type::Int | Type::Float | Type::Bool);
            }
        }
        false
    }

    /// Find variant info by name across all unions
    ///
    /// Returns (tag_index, field_count) for the variant
    /// Returns (tag_index, field_count, field_names)
    pub(super) fn find_variant_info(
        &self,
        variant_name: &str,
    ) -> Result<(usize, usize, Vec<String>), CodeGenError> {
        for union_def in &self.unions {
            for (tag_idx, variant) in union_def.variants.iter().enumerate() {
                if variant.name == variant_name {
                    let field_names: Vec<String> =
                        variant.fields.iter().map(|f| f.name.clone()).collect();
                    return Ok((tag_idx, variant.fields.len(), field_names));
                }
            }
        }
        Err(CodeGenError::Logic(format!(
            "Unknown variant '{}' in match pattern. No union defines this variant.",
            variant_name
        )))
    }

    /// Find the union that contains a given variant
    ///
    /// Returns the UnionDef reference if found
    pub(super) fn find_union_for_variant(&self, variant_name: &str) -> Option<&UnionDef> {
        for union_def in &self.unions {
            for variant in &union_def.variants {
                if variant.name == variant_name {
                    return Some(union_def);
                }
            }
        }
        None
    }

    /// Check if a match expression is exhaustive for its union type
    ///
    /// Returns Ok(()) if exhaustive, Err with missing variants if not
    pub(super) fn check_match_exhaustiveness(
        &self,
        arms: &[MatchArm],
    ) -> Result<(), (String, Vec<String>)> {
        if arms.is_empty() {
            return Ok(()); // Empty match is degenerate, skip check
        }

        // Get the first variant name to find the union
        let first_variant = match &arms[0].pattern {
            Pattern::Variant(name) => name.as_str(),
            Pattern::VariantWithBindings { name, .. } => name.as_str(),
        };

        // Find the union this variant belongs to
        let union_def = match self.find_union_for_variant(first_variant) {
            Some(u) => u,
            None => return Ok(()), // Unknown variant, let find_variant_info handle error
        };

        // Collect all variant names in the match arms
        let matched_variants: std::collections::HashSet<&str> = arms
            .iter()
            .map(|arm| match &arm.pattern {
                Pattern::Variant(name) => name.as_str(),
                Pattern::VariantWithBindings { name, .. } => name.as_str(),
            })
            .collect();

        // Check if all union variants are covered
        let missing: Vec<String> = union_def
            .variants
            .iter()
            .filter(|v| !matched_variants.contains(v.name.as_str()))
            .map(|v| v.name.clone())
            .collect();

        if missing.is_empty() {
            Ok(())
        } else {
            Err((union_def.name.clone(), missing))
        }
    }
}
