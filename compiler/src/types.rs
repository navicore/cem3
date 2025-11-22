//! Type system for Seq
//!
//! Based on cem2's row polymorphism design with improvements.
//! Supports stack effect declarations like: ( ..a Int -- ..a Bool )

/// Base types in the language
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// Integer type
    Int,
    /// Boolean type
    Bool,
    /// String type
    String,
    /// Quotation type (stateless code block with stack effect)
    /// Example: [ Int -- Int ] is a quotation that takes Int and produces Int
    /// No captured values - backward compatible with existing quotations
    Quotation(Box<Effect>),
    /// Closure type (quotation with captured environment)
    /// Example: Closure { effect: [Int -- Int], captures: [Int] }
    /// A closure that captures one Int and takes another Int to produce Int
    Closure {
        /// Stack effect when the closure is called
        effect: Box<Effect>,
        /// Types of values captured from the creation site
        /// Ordered top-down: captures[0] is top of stack at creation
        captures: Vec<Type>,
    },
    /// Type variable (for polymorphism)
    /// Example: T in ( ..a T -- ..a T T )
    Var(String),
}

/// Stack types with row polymorphism
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StackType {
    /// Empty stack
    Empty,

    /// Stack with a value on top of rest
    /// Example: Int on top of ..a
    Cons {
        /// The rest of the stack (may be Empty, another Cons, or RowVar)
        rest: Box<StackType>,
        /// The type on top of the stack
        top: Type,
    },

    /// Row variable representing "rest of stack"
    /// Example: ..a in ( ..a Int -- ..a Bool )
    RowVar(String),
}

/// Stack effect: transformation from input stack to output stack
/// Example: ( ..a Int -- ..a Bool ) means:
///   - Consumes an Int from stack with ..a underneath
///   - Produces a Bool on stack with ..a underneath
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Effect {
    /// Input stack type (before word executes)
    pub inputs: StackType,
    /// Output stack type (after word executes)
    pub outputs: StackType,
}

impl StackType {
    /// Create an empty stack type
    pub fn empty() -> Self {
        StackType::Empty
    }

    /// Create a stack type with a single value
    pub fn singleton(ty: Type) -> Self {
        StackType::Cons {
            rest: Box::new(StackType::Empty),
            top: ty,
        }
    }

    /// Push a type onto a stack type
    pub fn push(self, ty: Type) -> Self {
        StackType::Cons {
            rest: Box::new(self),
            top: ty,
        }
    }

    /// Create a stack type from a vector of types (bottom to top)
    pub fn from_vec(types: Vec<Type>) -> Self {
        types
            .into_iter()
            .fold(StackType::Empty, |stack, ty| stack.push(ty))
    }

    /// Pop a type from a stack type, returning (rest, top) if successful
    pub fn pop(self) -> Option<(StackType, Type)> {
        match self {
            StackType::Cons { rest, top } => Some((*rest, top)),
            _ => None,
        }
    }
}

impl Effect {
    /// Create a new stack effect
    pub fn new(inputs: StackType, outputs: StackType) -> Self {
        Effect { inputs, outputs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_stack() {
        let stack = StackType::empty();
        assert_eq!(stack, StackType::Empty);
    }

    #[test]
    fn test_singleton_stack() {
        let stack = StackType::singleton(Type::Int);
        assert_eq!(
            stack,
            StackType::Cons {
                rest: Box::new(StackType::Empty),
                top: Type::Int
            }
        );
    }

    #[test]
    fn test_push_pop() {
        let stack = StackType::empty().push(Type::Int).push(Type::Bool);

        let (rest, top) = stack.pop().unwrap();
        assert_eq!(top, Type::Bool);

        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Int);
        assert_eq!(rest2, StackType::Empty);
    }

    #[test]
    fn test_from_vec() {
        let stack = StackType::from_vec(vec![Type::Int, Type::Bool, Type::String]);

        // Stack should be: String on top of Bool on top of Int on top of Empty
        let (rest, top) = stack.pop().unwrap();
        assert_eq!(top, Type::String);

        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Bool);

        let (rest3, top3) = rest2.pop().unwrap();
        assert_eq!(top3, Type::Int);
        assert_eq!(rest3, StackType::Empty);
    }

    #[test]
    fn test_row_variable() {
        let stack = StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Int,
        };

        // This represents: Int on top of ..a
        let (rest, top) = stack.pop().unwrap();
        assert_eq!(top, Type::Int);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_effect() {
        // Effect: ( Int -- Bool )
        let effect = Effect::new(
            StackType::singleton(Type::Int),
            StackType::singleton(Type::Bool),
        );

        assert_eq!(effect.inputs, StackType::singleton(Type::Int));
        assert_eq!(effect.outputs, StackType::singleton(Type::Bool));
    }

    #[test]
    fn test_polymorphic_effect() {
        // Effect: ( ..a Int -- ..a Bool )
        let inputs = StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Int,
        };

        let outputs = StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Bool,
        };

        let effect = Effect::new(inputs, outputs);

        // Verify structure
        assert!(matches!(effect.inputs, StackType::Cons { .. }));
        assert!(matches!(effect.outputs, StackType::Cons { .. }));
    }
}
