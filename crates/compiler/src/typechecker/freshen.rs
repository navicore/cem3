//! Fresh type-variable generation and alpha-renaming of effects/types.
use std::collections::HashMap;

use crate::types::{Effect, SideEffect, StackType, Type};

use super::TypeChecker;

impl TypeChecker {
    pub(super) fn fresh_var(&self, prefix: &str) -> String {
        let n = self.fresh_counter.get();
        self.fresh_counter.set(n + 1);
        format!("{}${}", prefix, n)
    }

    /// Freshen all type and row variables in an effect
    pub(super) fn freshen_effect(&self, effect: &Effect) -> Effect {
        let mut type_map = HashMap::new();
        let mut row_map = HashMap::new();

        let fresh_inputs = self.freshen_stack(&effect.inputs, &mut type_map, &mut row_map);
        let fresh_outputs = self.freshen_stack(&effect.outputs, &mut type_map, &mut row_map);

        // Freshen the side effects too
        let fresh_effects = effect
            .effects
            .iter()
            .map(|e| self.freshen_side_effect(e, &mut type_map, &mut row_map))
            .collect();

        Effect::with_effects(fresh_inputs, fresh_outputs, fresh_effects)
    }

    pub(super) fn freshen_side_effect(
        &self,
        effect: &SideEffect,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> SideEffect {
        match effect {
            SideEffect::Yield(ty) => {
                SideEffect::Yield(Box::new(self.freshen_type(ty, type_map, row_map)))
            }
        }
    }

    pub(super) fn freshen_stack(
        &self,
        stack: &StackType,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> StackType {
        match stack {
            StackType::Empty => StackType::Empty,
            StackType::RowVar(name) => {
                let fresh_name = row_map
                    .entry(name.clone())
                    .or_insert_with(|| self.fresh_var(name));
                StackType::RowVar(fresh_name.clone())
            }
            StackType::Cons { rest, top } => {
                let fresh_rest = self.freshen_stack(rest, type_map, row_map);
                let fresh_top = self.freshen_type(top, type_map, row_map);
                StackType::Cons {
                    rest: Box::new(fresh_rest),
                    top: fresh_top,
                }
            }
        }
    }

    pub(super) fn freshen_type(
        &self,
        ty: &Type,
        type_map: &mut HashMap<String, String>,
        row_map: &mut HashMap<String, String>,
    ) -> Type {
        match ty {
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Symbol
            | Type::Channel
            | Type::Socket
            | Type::Variant => ty.clone(),
            Type::Var(name) => {
                let fresh_name = type_map
                    .entry(name.clone())
                    .or_insert_with(|| self.fresh_var(name));
                Type::Var(fresh_name.clone())
            }
            Type::Quotation(effect) => {
                let fresh_inputs = self.freshen_stack(&effect.inputs, type_map, row_map);
                let fresh_outputs = self.freshen_stack(&effect.outputs, type_map, row_map);
                Type::Quotation(Box::new(Effect::new(fresh_inputs, fresh_outputs)))
            }
            Type::Closure { effect, captures } => {
                let fresh_inputs = self.freshen_stack(&effect.inputs, type_map, row_map);
                let fresh_outputs = self.freshen_stack(&effect.outputs, type_map, row_map);
                let fresh_captures = captures
                    .iter()
                    .map(|t| self.freshen_type(t, type_map, row_map))
                    .collect();
                Type::Closure {
                    effect: Box::new(Effect::new(fresh_inputs, fresh_outputs)),
                    captures: fresh_captures,
                }
            }
            // Union types are concrete named types - no freshening needed
            Type::Union(name) => Type::Union(name.clone()),
        }
    }
}
