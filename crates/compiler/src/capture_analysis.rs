//! Capture Analysis for Closures
//!
//! This module handles the analysis of closure captures - determining which values
//! from the creation site need to be captured in a closure's environment.
//!
//! The key insight is that closures bridge two stack effects:
//! - **Body effect**: what the quotation body actually needs to execute
//! - **Call effect**: what the call site will provide when the closure is invoked
//!
//! The difference between these determines what must be captured at creation time.
//!
//! ## Example
//!
//! ```text
//! : add-to ( Int -- [Int -- Int] )
//!   [ add ] ;
//! ```
//!
//! Here:
//! - Body needs: `(Int Int -- Int)` (add requires two integers)
//! - Call provides: `(Int -- Int)` (caller provides one integer)
//! - Captures: `[Int]` (one integer captured from creation site)

use crate::types::{Effect, StackType, Type};

/// Calculate capture types for a closure
///
/// Given:
/// - `body_effect`: what the quotation body needs (e.g., `Int Int -- Int`)
/// - `call_effect`: what the call site will provide (e.g., `Int -- Int`)
///
/// Returns:
/// - `captures`: types to capture from creation stack (e.g., `[Int]`)
///
/// # Capture Ordering
///
/// Captures are returned bottom-to-top (deepest value first).
/// This matches how `push_closure` pops from the stack:
///
/// ```text
/// Stack at creation: ( ...rest bottom top )
/// push_closure pops top-down: pop top, pop bottom
/// Stores as: env[0]=top, env[1]=bottom (reversed)
/// Closure function pushes: push env[0], push env[1]
/// Result: bottom is deeper, top is shallower (correct order)
/// ```
///
/// # Errors
///
/// Returns an error if the call site provides more values than the body needs.
pub fn calculate_captures(body_effect: &Effect, call_effect: &Effect) -> Result<Vec<Type>, String> {
    // Extract concrete types from stack types (bottom to top)
    let body_inputs = extract_concrete_types(&body_effect.inputs);
    let call_inputs = extract_concrete_types(&call_effect.inputs);

    // Validate: call site shouldn't provide MORE than body needs
    if call_inputs.len() > body_inputs.len() {
        return Err(format!(
            "Closure signature error: call site provides {} values but body only needs {}",
            call_inputs.len(),
            body_inputs.len()
        ));
    }

    // Calculate how many to capture (from bottom of stack)
    let capture_count = body_inputs.len() - call_inputs.len();

    // Captures are the first N types (bottom of stack)
    // Example: body needs [Int, String] (bottom to top), call provides [String]
    // Captures: [Int] (the bottom type)
    Ok(body_inputs[0..capture_count].to_vec())
}

/// Extract concrete types from a stack type (bottom to top order)
///
/// This function traverses a `StackType` and returns a vector of concrete types
/// in bottom-to-top order (deepest stack element first).
///
/// # Example
///
/// ```text
/// Input: Cons { rest: Cons { rest: Empty, top: Int }, top: String }
/// Output: [Int, String]  (bottom to top)
/// ```
///
/// # Row Variables
///
/// Row variables (like `..a`) are skipped - this function only extracts
/// concrete types. This is appropriate for capture analysis where we need
/// to know the actual types being captured.
///
/// # Performance
///
/// Uses recursion to build the vector in the correct order without needing
/// to clone the entire stack structure or reverse the result.
pub fn extract_concrete_types(stack: &StackType) -> Vec<Type> {
    // Use recursion to build the vector in bottom-to-top order
    fn collect(stack: &StackType, result: &mut Vec<Type>) {
        match stack {
            StackType::Cons { rest, top } => {
                // First recurse to collect types below, then add this type
                collect(rest, result);
                result.push(top.clone());
            }
            StackType::Empty | StackType::RowVar(_) => {
                // Base case: nothing more to collect
            }
        }
    }

    let mut types = Vec::new();
    collect(stack, &mut types);
    types
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Effect, StackType, Type};

    fn make_stack(types: &[Type]) -> StackType {
        let mut stack = StackType::Empty;
        for t in types {
            stack = StackType::Cons {
                rest: Box::new(stack),
                top: t.clone(),
            };
        }
        stack
    }

    fn make_effect(inputs: &[Type], outputs: &[Type]) -> Effect {
        Effect {
            inputs: make_stack(inputs),
            outputs: make_stack(outputs),
        }
    }

    #[test]
    fn test_extract_empty_stack() {
        let types = extract_concrete_types(&StackType::Empty);
        assert!(types.is_empty());
    }

    #[test]
    fn test_extract_single_type() {
        let stack = make_stack(&[Type::Int]);
        let types = extract_concrete_types(&stack);
        assert_eq!(types, vec![Type::Int]);
    }

    #[test]
    fn test_extract_multiple_types() {
        let stack = make_stack(&[Type::Int, Type::String, Type::Bool]);
        let types = extract_concrete_types(&stack);
        assert_eq!(types, vec![Type::Int, Type::String, Type::Bool]);
    }

    #[test]
    fn test_calculate_no_captures() {
        // Body needs (Int -- Int), call provides (Int -- Int)
        let body = make_effect(&[Type::Int], &[Type::Int]);
        let call = make_effect(&[Type::Int], &[Type::Int]);

        let captures = calculate_captures(&body, &call).unwrap();
        assert!(captures.is_empty());
    }

    #[test]
    fn test_calculate_one_capture() {
        // Body needs (Int Int -- Int), call provides (Int -- Int)
        // Should capture one Int
        let body = make_effect(&[Type::Int, Type::Int], &[Type::Int]);
        let call = make_effect(&[Type::Int], &[Type::Int]);

        let captures = calculate_captures(&body, &call).unwrap();
        assert_eq!(captures, vec![Type::Int]);
    }

    #[test]
    fn test_calculate_multiple_captures() {
        // Body needs (Int String Bool -- Bool), call provides (Bool -- Bool)
        // Should capture [Int, String] (bottom to top)
        let body = make_effect(&[Type::Int, Type::String, Type::Bool], &[Type::Bool]);
        let call = make_effect(&[Type::Bool], &[Type::Bool]);

        let captures = calculate_captures(&body, &call).unwrap();
        assert_eq!(captures, vec![Type::Int, Type::String]);
    }

    #[test]
    fn test_calculate_all_captured() {
        // Body needs (Int String -- Int), call provides ( -- Int)
        // Should capture [Int, String]
        let body = make_effect(&[Type::Int, Type::String], &[Type::Int]);
        let call = make_effect(&[], &[Type::Int]);

        let captures = calculate_captures(&body, &call).unwrap();
        assert_eq!(captures, vec![Type::Int, Type::String]);
    }

    #[test]
    fn test_calculate_error_too_many_call_inputs() {
        // Body needs (Int -- Int), call provides (Int Int -- Int)
        // Error: call provides more than body needs
        let body = make_effect(&[Type::Int], &[Type::Int]);
        let call = make_effect(&[Type::Int, Type::Int], &[Type::Int]);

        let result = calculate_captures(&body, &call);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .contains("provides 2 values but body only needs 1")
        );
    }
}
