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

/// Unify two types, returning a substitution or an error
pub fn unify_types(t1: &Type, t2: &Type) -> Result<Subst, String> {
    match (t1, t2) {
        // Same concrete types unify
        (Type::Int, Type::Int) | (Type::Bool, Type::Bool) | (Type::String, Type::String) => {
            Ok(Subst::empty())
        }

        // Type variable unifies with anything
        (Type::Var(name), ty) | (ty, Type::Var(name)) => {
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

        // Row variable unifies with any stack
        (StackType::RowVar(name), stack) | (stack, StackType::RowVar(name)) => {
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
}
