//! Type unification for Seq
//!
//! Implements Hindley-Milner style unification with support for:
//! - Type variables (T, U, V)
//! - Row variables (..a, ..rest)
//! - Concrete types (Int, Bool, String)

use crate::types::{Effect, StackType, Type};
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
        // Concrete types contain no type variables
        Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Symbol
        | Type::Channel
        | Type::Socket
        | Type::Union(_)
        | Type::Variant => false,
        Type::Quotation(effect) => {
            // Check if var occurs in quotation's input or output stack types
            occurs_in_stack(var, &effect.inputs) || occurs_in_stack(var, &effect.outputs)
        }
        Type::Closure { effect, captures } => {
            // Check if var occurs in closure's effect or any captured types
            occurs_in_stack(var, &effect.inputs)
                || occurs_in_stack(var, &effect.outputs)
                || captures.iter().any(|t| occurs_in_type(var, t))
        }
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

/// Unify two stack effects: unify inputs, then outputs under the resulting
/// substitution, and compose. Shared by the Quotation/Closure arms of
/// `unify_types` (captures are an implementation detail, ignored here).
fn unify_effects(e1: &Effect, e2: &Effect) -> Result<Subst, String> {
    let s_in = unify_stacks(&e1.inputs, &e2.inputs)?;
    let out1 = s_in.apply_stack(&e1.outputs);
    let out2 = s_in.apply_stack(&e2.outputs);
    let s_out = unify_stacks(&out1, &out2)?;
    Ok(s_in.compose(&s_out))
}

/// Unify two types, returning a substitution or an error
pub fn unify_types(t1: &Type, t2: &Type) -> Result<Subst, String> {
    match (t1, t2) {
        // Same concrete types unify
        (Type::Int, Type::Int)
        | (Type::Float, Type::Float)
        | (Type::Bool, Type::Bool)
        | (Type::String, Type::String)
        | (Type::Symbol, Type::Symbol)
        | (Type::Channel, Type::Channel)
        | (Type::Socket, Type::Socket) => Ok(Subst::empty()),

        // Union types unify if they have the same name
        (Type::Union(name1), Type::Union(name2)) => {
            if name1 == name2 {
                Ok(Subst::empty())
            } else {
                Err(format!(
                    "Type mismatch: cannot unify Union({}) with Union({})",
                    name1, name2
                ))
            }
        }

        // Variant matches itself
        (Type::Variant, Type::Variant) => Ok(Subst::empty()),

        // Union <: Variant relaxation — a named union value is a variant.
        // This lets `variant.*` builtins (typed against `Variant`) accept
        // user values typed as `Union(name)` without losing union safety
        // elsewhere: the rule applies only when one side is the bare
        // `Variant` placeholder. Mirrors the Closure <: Quotation rule
        // below; the symmetric form is a minor unsoundness in the reverse
        // direction (a `Variant` flowing back into a `Union(name)` slot)
        // that we accept for now.
        //
        // TODO: tighten to a directional rule once the typechecker tracks
        // which side of a unification is "expected" vs "actual". Today a
        // `Variant` (e.g. the result of `variant.append`) silently
        // satisfies a `Union(name)` constraint without checking the tag —
        // intended pragmatic loophole, not a permanent stance.
        (Type::Union(_), Type::Variant) | (Type::Variant, Type::Union(_)) => Ok(Subst::empty()),

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

        // Quotation types unify if their effects unify
        (Type::Quotation(effect1), Type::Quotation(effect2)) => unify_effects(effect1, effect2),

        // Closure types unify if their effects unify. Captures are an
        // implementation detail determined by the type checker, not part of
        // the user-visible type.
        (
            Type::Closure {
                effect: effect1, ..
            },
            Type::Closure {
                effect: effect2, ..
            },
        ) => unify_effects(effect1, effect2),

        // Closure <: Quotation (subtyping): a Closure can be used where a
        // Quotation is expected; the runtime dispatches appropriately.
        (Type::Quotation(quot_effect), Type::Closure { effect, .. })
        | (Type::Closure { effect, .. }, Type::Quotation(quot_effect)) => {
            unify_effects(quot_effect, effect)
        }

        // Different concrete types don't unify
        _ => Err(format!("Type mismatch: cannot unify {} with {}", t1, t2)),
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
                    "Occurs check failed: cannot unify {} with {} (would create infinite stack type)",
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
            "Stack shape mismatch: cannot unify {} with {}",
            s1, s2
        )),
    }
}

#[cfg(test)]
mod tests;
