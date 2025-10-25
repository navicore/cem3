//! Type unification for cem3
//!
//! Implements Hindley-Milner style unification with support for:
//! - Type variables (T, U, V)
//! - Row variables (..a, ..rest)
//! - Concrete types (Int, Bool, String)

use crate::types::{StackType, Type};
use std::collections::HashMap;

/// Substitutions for type variables
pub type TypeSubst = HashMap<String, Type>;

/// Substitutions for row variables (stack type variables)
pub type RowSubst = HashMap<String, StackType>;

/// Combined substitution environment
#[derive(Debug, Clone, PartialEq)]
pub struct Subst {
    pub types: TypeSubst,
    pub rows: RowSubst,
}

impl Subst {
    /// Create an empty substitution
    pub fn empty() -> Self {
        Subst {
            types: HashMap::new(),
            rows: HashMap::new(),
        }
    }

    /// Apply substitutions to a Type
    pub fn apply_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Var(name) => self.types.get(name).cloned().unwrap_or(ty.clone()),
            _ => ty.clone(),
        }
    }

    /// Apply substitutions to a StackType
    pub fn apply_stack(&self, stack: &StackType) -> StackType {
        match stack {
            StackType::Empty => StackType::Empty,
            StackType::Cons { rest, top } => {
                let new_rest = self.apply_stack(rest);
                let new_top = self.apply_type(top);
                StackType::Cons {
                    rest: Box::new(new_rest),
                    top: new_top,
                }
            }
            StackType::RowVar(name) => self.rows.get(name).cloned().unwrap_or(stack.clone()),
        }
    }

    /// Compose two substitutions (apply other after self)
    /// Result: (other ∘ self) where self is applied first, then other
    pub fn compose(&self, other: &Subst) -> Subst {
        let mut types = HashMap::new();
        let mut rows = HashMap::new();

        // Apply other to all of self's type substitutions
        for (k, v) in &self.types {
            types.insert(k.clone(), other.apply_type(v));
        }

        // Add other's type substitutions (applying self to other's values)
        for (k, v) in &other.types {
            let v_subst = self.apply_type(v);
            types.insert(k.clone(), v_subst);
        }

        // Apply other to all of self's row substitutions
        for (k, v) in &self.rows {
            rows.insert(k.clone(), other.apply_stack(v));
        }

        // Add other's row substitutions (applying self to other's values)
        for (k, v) in &other.rows {
            let v_subst = self.apply_stack(v);
            rows.insert(k.clone(), v_subst);
        }

        Subst { types, rows }
    }
}

/// Check if a type variable occurs in a type (for occurs check)
///
/// Prevents infinite types like: T = List<T>
///
/// NOTE: Currently we only have simple types (Int, String, Bool).
/// When parametric types are added (e.g., List<T>, Option<T>), this function
/// must be extended to recursively check type arguments:
///
/// ```ignore
/// Type::Named { name: _, args } => {
///     args.iter().any(|arg| occurs_in_type(var, arg))
/// }
/// ```
fn occurs_in_type(var: &str, ty: &Type) -> bool {
    match ty {
        Type::Var(name) => name == var,
        Type::Int | Type::Bool | Type::String => false,
    }
}

/// Check if a row variable occurs in a stack type (for occurs check)
fn occurs_in_stack(var: &str, stack: &StackType) -> bool {
    match stack {
        StackType::Empty => false,
        StackType::RowVar(name) => name == var,
        StackType::Cons { rest, top: _ } => {
            // Row variables only occur in stack positions, not in type positions
            // So we only need to check the rest of the stack
            occurs_in_stack(var, rest)
        }
    }
}

/// Unify two types, returning a substitution or an error
pub fn unify_types(t1: &Type, t2: &Type) -> Result<Subst, String> {
    match (t1, t2) {
        // Same concrete types unify
        (Type::Int, Type::Int) | (Type::Bool, Type::Bool) | (Type::String, Type::String) => {
            Ok(Subst::empty())
        }

        // Type variable unifies with anything (with occurs check)
        (Type::Var(name), ty) | (ty, Type::Var(name)) => {
            // If unifying a variable with itself, no substitution needed
            if matches!(ty, Type::Var(ty_name) if ty_name == name) {
                return Ok(Subst::empty());
            }

            // Occurs check: prevent infinite types
            if occurs_in_type(name, ty) {
                return Err(format!(
                    "Occurs check failed: cannot unify {:?} with {:?} (would create infinite type)",
                    Type::Var(name.clone()),
                    ty
                ));
            }

            let mut subst = Subst::empty();
            subst.types.insert(name.clone(), ty.clone());
            Ok(subst)
        }

        // Different concrete types don't unify
        _ => Err(format!(
            "Type mismatch: cannot unify {:?} with {:?}",
            t1, t2
        )),
    }
}

/// Unify two stack types, returning a substitution or an error
pub fn unify_stacks(s1: &StackType, s2: &StackType) -> Result<Subst, String> {
    match (s1, s2) {
        // Empty stacks unify
        (StackType::Empty, StackType::Empty) => Ok(Subst::empty()),

        // Row variable unifies with any stack (with occurs check)
        (StackType::RowVar(name), stack) | (stack, StackType::RowVar(name)) => {
            // If unifying a row var with itself, no substitution needed
            if matches!(stack, StackType::RowVar(stack_name) if stack_name == name) {
                return Ok(Subst::empty());
            }

            // Occurs check: prevent infinite stack types
            if occurs_in_stack(name, stack) {
                return Err(format!(
                    "Occurs check failed: cannot unify {:?} with {:?} (would create infinite stack type)",
                    StackType::RowVar(name.clone()),
                    stack
                ));
            }

            let mut subst = Subst::empty();
            subst.rows.insert(name.clone(), stack.clone());
            Ok(subst)
        }

        // Cons cells unify if tops and rests unify
        (
            StackType::Cons {
                rest: rest1,
                top: top1,
            },
            StackType::Cons {
                rest: rest2,
                top: top2,
            },
        ) => {
            // Unify the tops
            let s_top = unify_types(top1, top2)?;

            // Apply substitution to rests and unify
            let rest1_subst = s_top.apply_stack(rest1);
            let rest2_subst = s_top.apply_stack(rest2);
            let s_rest = unify_stacks(&rest1_subst, &rest2_subst)?;

            // Compose substitutions
            Ok(s_top.compose(&s_rest))
        }

        // Empty doesn't unify with Cons
        _ => Err(format!(
            "Stack shape mismatch: cannot unify {:?} with {:?}",
            s1, s2
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unify_concrete_types() {
        assert!(unify_types(&Type::Int, &Type::Int).is_ok());
        assert!(unify_types(&Type::Bool, &Type::Bool).is_ok());
        assert!(unify_types(&Type::String, &Type::String).is_ok());

        assert!(unify_types(&Type::Int, &Type::Bool).is_err());
    }

    #[test]
    fn test_unify_type_variable() {
        let subst = unify_types(&Type::Var("T".to_string()), &Type::Int).unwrap();
        assert_eq!(subst.types.get("T"), Some(&Type::Int));

        let subst = unify_types(&Type::Bool, &Type::Var("U".to_string())).unwrap();
        assert_eq!(subst.types.get("U"), Some(&Type::Bool));
    }

    #[test]
    fn test_unify_empty_stacks() {
        assert!(unify_stacks(&StackType::Empty, &StackType::Empty).is_ok());
    }

    #[test]
    fn test_unify_row_variable() {
        let subst = unify_stacks(
            &StackType::RowVar("a".to_string()),
            &StackType::singleton(Type::Int),
        )
        .unwrap();

        assert_eq!(subst.rows.get("a"), Some(&StackType::singleton(Type::Int)));
    }

    #[test]
    fn test_unify_cons_stacks() {
        // ( Int ) unifies with ( Int )
        let s1 = StackType::singleton(Type::Int);
        let s2 = StackType::singleton(Type::Int);

        assert!(unify_stacks(&s1, &s2).is_ok());
    }

    #[test]
    fn test_unify_cons_with_type_var() {
        // ( T ) unifies with ( Int ), producing T := Int
        let s1 = StackType::singleton(Type::Var("T".to_string()));
        let s2 = StackType::singleton(Type::Int);

        let subst = unify_stacks(&s1, &s2).unwrap();
        assert_eq!(subst.types.get("T"), Some(&Type::Int));
    }

    #[test]
    fn test_unify_row_poly_stack() {
        // ( ..a Int ) unifies with ( Bool Int ), producing ..a := ( Bool )
        let s1 = StackType::RowVar("a".to_string()).push(Type::Int);
        let s2 = StackType::Empty.push(Type::Bool).push(Type::Int);

        let subst = unify_stacks(&s1, &s2).unwrap();

        assert_eq!(subst.rows.get("a"), Some(&StackType::singleton(Type::Bool)));
    }

    #[test]
    fn test_unify_polymorphic_dup() {
        // dup: ( ..a T -- ..a T T )
        // Applied to: ( Int ) should work with ..a := Empty, T := Int

        let input_actual = StackType::singleton(Type::Int);
        let input_declared = StackType::RowVar("a".to_string()).push(Type::Var("T".to_string()));

        let subst = unify_stacks(&input_declared, &input_actual).unwrap();

        assert_eq!(subst.rows.get("a"), Some(&StackType::Empty));
        assert_eq!(subst.types.get("T"), Some(&Type::Int));

        // Apply substitution to output: ( ..a T T )
        let output_declared = StackType::RowVar("a".to_string())
            .push(Type::Var("T".to_string()))
            .push(Type::Var("T".to_string()));

        let output_actual = subst.apply_stack(&output_declared);

        // Should be ( Int Int )
        assert_eq!(
            output_actual,
            StackType::Empty.push(Type::Int).push(Type::Int)
        );
    }

    #[test]
    fn test_subst_compose() {
        // s1: T := Int
        let mut s1 = Subst::empty();
        s1.types.insert("T".to_string(), Type::Int);

        // s2: U := T
        let mut s2 = Subst::empty();
        s2.types.insert("U".to_string(), Type::Var("T".to_string()));

        // Compose: should give U := Int, T := Int
        let composed = s1.compose(&s2);

        assert_eq!(composed.types.get("T"), Some(&Type::Int));
        assert_eq!(composed.types.get("U"), Some(&Type::Int));
    }

    #[test]
    fn test_occurs_check_type_var_with_itself() {
        // Unifying T with T should succeed (no substitution needed)
        let result = unify_types(&Type::Var("T".to_string()), &Type::Var("T".to_string()));
        assert!(result.is_ok());
        let subst = result.unwrap();
        // Should be empty - no substitution needed when unifying var with itself
        assert!(subst.types.is_empty());
    }

    #[test]
    fn test_occurs_check_row_var_with_itself() {
        // Unifying ..a with ..a should succeed (no substitution needed)
        let result = unify_stacks(
            &StackType::RowVar("a".to_string()),
            &StackType::RowVar("a".to_string()),
        );
        assert!(result.is_ok());
        let subst = result.unwrap();
        // Should be empty - no substitution needed when unifying var with itself
        assert!(subst.rows.is_empty());
    }

    #[test]
    fn test_occurs_check_prevents_infinite_stack() {
        // Attempting to unify ..a with (..a Int) should fail
        // This would create an infinite type: ..a = (..a Int) = ((..a Int) Int) = ...
        let row_var = StackType::RowVar("a".to_string());
        let infinite_stack = StackType::RowVar("a".to_string()).push(Type::Int);

        let result = unify_stacks(&row_var, &infinite_stack);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Occurs check failed"));
        assert!(err.contains("infinite"));
    }

    #[test]
    fn test_occurs_check_allows_different_row_vars() {
        // Unifying ..a with ..b should succeed (different variables)
        let result = unify_stacks(
            &StackType::RowVar("a".to_string()),
            &StackType::RowVar("b".to_string()),
        );
        assert!(result.is_ok());
        let subst = result.unwrap();
        assert_eq!(
            subst.rows.get("a"),
            Some(&StackType::RowVar("b".to_string()))
        );
    }

    #[test]
    fn test_occurs_check_allows_concrete_stack() {
        // Unifying ..a with (Int String) should succeed (no occurs)
        let row_var = StackType::RowVar("a".to_string());
        let concrete = StackType::Empty.push(Type::Int).push(Type::String);

        let result = unify_stacks(&row_var, &concrete);
        assert!(result.is_ok());
        let subst = result.unwrap();
        assert_eq!(subst.rows.get("a"), Some(&concrete));
    }

    #[test]
    fn test_occurs_in_type() {
        // T occurs in T
        assert!(occurs_in_type("T", &Type::Var("T".to_string())));

        // T does not occur in U
        assert!(!occurs_in_type("T", &Type::Var("U".to_string())));

        // T does not occur in Int
        assert!(!occurs_in_type("T", &Type::Int));
        assert!(!occurs_in_type("T", &Type::String));
        assert!(!occurs_in_type("T", &Type::Bool));
    }

    #[test]
    fn test_occurs_in_stack() {
        // ..a occurs in ..a
        assert!(occurs_in_stack("a", &StackType::RowVar("a".to_string())));

        // ..a does not occur in ..b
        assert!(!occurs_in_stack("a", &StackType::RowVar("b".to_string())));

        // ..a does not occur in Empty
        assert!(!occurs_in_stack("a", &StackType::Empty));

        // ..a occurs in (..a Int)
        let stack = StackType::RowVar("a".to_string()).push(Type::Int);
        assert!(occurs_in_stack("a", &stack));

        // ..a does not occur in (..b Int)
        let stack = StackType::RowVar("b".to_string()).push(Type::Int);
        assert!(!occurs_in_stack("a", &stack));

        // ..a does not occur in (Int String)
        let stack = StackType::Empty.push(Type::Int).push(Type::String);
        assert!(!occurs_in_stack("a", &stack));
    }
}
