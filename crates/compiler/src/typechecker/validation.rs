//! Type-name parsing plus validation of effects, stacks, and union field types.

use crate::ast::Program;
use crate::types::{Effect, StackType, Type};

use super::TypeChecker;

impl TypeChecker {
    pub(super) fn parse_type_name(&self, name: &str) -> Type {
        match name {
            "Int" => Type::Int,
            "Float" => Type::Float,
            "Bool" => Type::Bool,
            "String" => Type::String,
            "Channel" => Type::Channel,
            "Socket" => Type::Socket,
            // Any other name is assumed to be a union type reference
            other => Type::Union(other.to_string()),
        }
    }

    /// Check whether `name` is valid as a *union field* type name.
    ///
    /// Built-in field types (Int, Float, Bool, String, Channel, Socket) plus
    /// registered union names. `Symbol` and `Variant` are intentionally not
    /// declarable as union field types here (kept in sync with the 6-type set
    /// in `parse_type_name`); they remain valid as already-formed `Type`s in
    /// `validate_type`.
    pub(super) fn is_valid_type_name(&self, name: &str) -> bool {
        matches!(
            name,
            "Int" | "Float" | "Bool" | "String" | "Channel" | "Socket"
        ) || self.unions.contains_key(name)
    }

    /// Validate that all field types in union definitions reference known types
    ///
    /// Note: Field count validation happens earlier in generate_constructors()
    pub(super) fn validate_union_field_types(&self, program: &Program) -> Result<(), String> {
        for union_def in &program.unions {
            for variant in &union_def.variants {
                for field in &variant.fields {
                    if !self.is_valid_type_name(&field.type_name) {
                        return Err(format!(
                            "Unknown type '{}' in field '{}' of variant '{}' in union '{}'. \
                             Valid types are: Int, Float, Bool, String, Channel, or a defined union name.",
                            field.type_name, field.name, variant.name, union_def.name
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// Validate that all types in a stack effect are known types
    ///
    /// RFC #345: This catches cases where an uppercase identifier was parsed as a type
    /// variable but should have been a union type (e.g., from an include that wasn't
    /// available when parsing). Type variables should be single uppercase letters (T, U, V).
    pub(super) fn validate_effect_types(
        &self,
        effect: &Effect,
        word_name: &str,
    ) -> Result<(), String> {
        self.validate_stack_types(&effect.inputs, word_name)?;
        self.validate_stack_types(&effect.outputs, word_name)?;
        Ok(())
    }

    /// Validate types in a stack type
    pub(super) fn validate_stack_types(
        &self,
        stack: &StackType,
        word_name: &str,
    ) -> Result<(), String> {
        match stack {
            StackType::Empty | StackType::RowVar(_) => Ok(()),
            StackType::Cons { rest, top } => {
                self.validate_type(top, word_name)?;
                self.validate_stack_types(rest, word_name)
            }
        }
    }

    /// Validate a single type
    ///
    /// Single-character uppercase and any lowercase identifiers are legitimate
    /// type variables. Multi-character uppercase identifiers that survived
    /// `fixup_union_types()` were neither concrete types nor registered unions
    /// — almost certainly typos. Reject them with a "did you mean" hint.
    pub(super) fn validate_type(&self, ty: &Type, word_name: &str) -> Result<(), String> {
        match ty {
            Type::Var(name) => {
                let mut chars = name.chars();
                let first = chars.next();
                let is_multi_upper =
                    first.is_some_and(|c| c.is_uppercase()) && name.chars().count() > 1;
                if is_multi_upper {
                    return Err(format!(
                        "In word '{}': unknown type '{}' in stack effect.{} \
                         Multi-character type names must be a concrete type or \
                         a registered union; use a single uppercase letter \
                         (T, U, V, …) for a polymorphic type variable.",
                        word_name,
                        name,
                        self.suggest_type_hint(name),
                    ));
                }
                Ok(())
            }
            Type::Quotation(effect) => self.validate_effect_types(effect, word_name),
            Type::Closure { effect, captures } => {
                self.validate_effect_types(effect, word_name)?;
                for cap in captures {
                    self.validate_type(cap, word_name)?;
                }
                Ok(())
            }
            // Concrete types are always valid
            Type::Int
            | Type::Float
            | Type::Bool
            | Type::String
            | Type::Symbol
            | Type::Channel
            | Type::Socket
            | Type::Variant => Ok(()),
            // Union types are valid if they're registered
            Type::Union(name) => {
                if !self.unions.contains_key(name) {
                    return Err(format!(
                        "In word '{}': Unknown union type '{}' in stack effect.\n\
                         Make sure this union is defined before the word that uses it.",
                        word_name, name
                    ));
                }
                Ok(())
            }
        }
    }

    /// Build " Did you mean 'X'?" suffix for an unknown uppercase type name,
    /// or empty string if no close match. Edit-distance ≤ 2 against the known
    /// concrete types and registered unions.
    fn suggest_type_hint(&self, unknown: &str) -> String {
        const CONCRETE: &[&str] = &[
            "Int", "Float", "Bool", "String", "Symbol", "Channel", "Socket", "Variant",
        ];
        let mut best: Option<(String, usize)> = None;
        for cand in CONCRETE
            .iter()
            .map(|s| s.to_string())
            .chain(self.unions.keys().cloned())
        {
            let d = crate::parser::type_parse::edit_distance(unknown, &cand);
            if d <= 2 && best.as_ref().is_none_or(|(_, bd)| d < *bd) {
                best = Some((cand, d));
            }
        }
        match best {
            Some((cand, _)) => format!(" Did you mean '{}'?", cand),
            None => String::new(),
        }
    }
}
