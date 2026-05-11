//! Type system for Seq
//!
//! Based on cem2's row polymorphism design with improvements.
//! Supports stack effect declarations like: ( ..a Int -- ..a Bool )
//!
//! ## Computational Effects
//!
//! Beyond stack effects, Seq tracks computational side effects using the `|` syntax:
//! - `( a -- b | Yield T )` - may yield values of type T (generators)
//! - Effects propagate through function calls
//! - `strand.weave` handles the Yield effect, `strand.spawn` requires pure quotations

/// Computational side effects (beyond stack transformation)
///
/// These track effects that go beyond the stack transformation:
/// - Yield: generator/coroutine that yields values
/// - Future: IO, Throw, Async, etc.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SideEffect {
    /// Yields values of type T (generator effect)
    /// Used by strand.weave quotations
    Yield(Box<Type>),
}

impl std::fmt::Display for SideEffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SideEffect::Yield(ty) => write!(f, "Yield {}", ty),
        }
    }
}

/// Base types in the language
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// Integer type
    Int,
    /// Floating-point type (IEEE 754 double precision)
    Float,
    /// Boolean type
    Bool,
    /// String type
    String,
    /// Symbol type (interned identifier for dynamic variant construction)
    /// Syntax: :foo, :some-name
    Symbol,
    /// Channel type (for CSP-style concurrency)
    /// Channels are reference-counted handles - dup increments refcount
    Channel,
    /// Socket type (TCP/UDP file descriptor — phantom over Int).
    /// Distinct from Int at the type level so `tcp.write` can't accept an
    /// arbitrary integer; runtime representation stays Value::Int(fd).
    /// Cross over with `fd->socket` / `socket->fd` when really needed (FFI).
    Socket,
    /// Quotation type (stateless code block with stack effect)
    /// Example: [ Int -- Int ] is a quotation that takes Int and produces Int
    /// No captured values - backward compatible with existing quotations
    Quotation(Box<Effect>),
    /// Closure type (quotation with captured environment)
    /// Example: `Closure { effect: [Int -- Int], captures: [Int] }`
    /// A closure that captures one Int and takes another Int to produce Int
    Closure {
        /// Stack effect when the closure is called
        effect: Box<Effect>,
        /// Types of values captured from the creation site
        /// Ordered top-down: `captures[0]` is top of stack at creation
        captures: Vec<Type>,
    },
    /// Union type - references a union definition by name
    /// Example: Message in `union Message { Get { ... } Increment { ... } }`
    /// The full definition is looked up in the type environment
    Union(String),
    /// Anonymous variant value — a tagged variant of unspecified union shape.
    /// This is the compile-time mate of the runtime `Value::Variant` for cases
    /// where we know the value is a variant but not which union it belongs to.
    /// Used by the low-level `variant.*` builtins (variant.field-at, variant.tag,
    /// variant.make-N, etc.). A `Type::Union(name)` is accepted where `Variant`
    /// is expected (one-way relaxation, see unification).
    Variant,
    /// Type variable (for polymorphism)
    /// Example: T in ( ..a T -- ..a T T )
    Var(String),
}

/// Information about a variant field
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantFieldInfo {
    pub name: String,
    pub field_type: Type,
}

/// Information about a union variant (used by type checker)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantInfo {
    pub name: String,
    pub fields: Vec<VariantFieldInfo>,
}

/// Type information for a union definition (used by type checker)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnionTypeInfo {
    pub name: String,
    pub variants: Vec<VariantInfo>,
}

/// Stack types with row polymorphism
///
/// # Understanding Stack Type Representation
///
/// Seq uses **row polymorphism** to type stack operations. The stack is represented
/// as a linked list structure using `Cons` cells (from Lisp terminology).
///
/// ## Components
///
/// - **`Cons { rest, top }`**: A "cons cell" pairing a value type with the rest of the stack
///   - `top`: The type of the value at this position
///   - `rest`: What's underneath (another `Cons`, `Empty`, or `RowVar`)
///
/// - **`RowVar("name")`**: A row variable representing "the rest of the stack we don't care about"
///   - Enables polymorphic functions like `dup` that work regardless of stack depth
///   - Written as `..name` in stack effect signatures
///
/// - **`Empty`**: An empty stack (no values)
///
/// ## Debug vs Display Format
///
/// The `Debug` format shows the internal structure (useful for compiler developers):
/// ```text
/// Cons { rest: Cons { rest: RowVar("a$5"), top: Int }, top: Int }
/// ```
///
/// The `Display` format shows user-friendly notation (matches stack effect syntax):
/// ```text
/// (..a$5 Int Int)
/// ```
///
/// ## Reading the Debug Format
///
/// To read `Cons { rest: Cons { rest: RowVar("a"), top: Int }, top: Float }`:
///
/// 1. Start from the outermost `Cons` - its `top` is the stack top: `Float`
/// 2. Follow `rest` to the next `Cons` - its `top` is next: `Int`
/// 3. Follow `rest` to `RowVar("a")` - this is the polymorphic "rest of stack"
///
/// ```text
/// Cons { rest: Cons { rest: RowVar("a"), top: Int }, top: Float }
/// │                                           │           │
/// │                                           │           └── top of stack: Float
/// │                                           └── second from top: Int
/// └── rest of stack: ..a (whatever else is there)
///
/// Equivalent to: (..a Int Float)  or in signature: ( ..a Int Float -- ... )
/// ```
///
/// ## Fresh Variables (e.g., "a$5")
///
/// During type checking, variables are "freshened" to avoid name collisions:
/// - `a` becomes `a$0`, `a$1`, etc.
/// - The number is just a unique counter, not semantically meaningful
/// - `a$5` means "the 6th fresh variable generated with prefix 'a'"
///
/// ## Example Error Message
///
/// ```text
/// divide: stack type mismatch. Expected (..a$0 Int Int), got (..rest Float Float)
/// ```
///
/// Meaning:
/// - `divide` expects two `Int` values on top of any stack (`..a$0`)
/// - You provided two `Float` values on top of the stack (`..rest`)
/// - The types don't match: `Int` vs `Float`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum StackType {
    /// Empty stack - no values
    Empty,

    /// Stack with a value on top of rest (a "cons cell")
    ///
    /// Named after Lisp's cons (construct) operation that builds pairs.
    /// Think of it as: `top` is the head, `rest` is the tail.
    Cons {
        /// The rest of the stack (may be Empty, another Cons, or RowVar)
        rest: Box<StackType>,
        /// The type on top of the stack at this position
        top: Type,
    },

    /// Row variable representing "rest of stack" for polymorphism
    ///
    /// Allows functions to be polymorphic over stack depth.
    /// Example: `dup` has effect `( ..a T -- ..a T T )` where `..a` means
    /// "whatever is already on the stack stays there".
    RowVar(String),
}

/// Stack effect: transformation from input stack to output stack
/// Example: ( ..a Int -- ..a Bool ) means:
///   - Consumes an Int from stack with ..a underneath
///   - Produces a Bool on stack with ..a underneath
///
/// With computational effects: ( ..a Int -- ..a Bool | Yield Int )
///   - Same stack transformation
///   - May also yield Int values (generator effect)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Effect {
    /// Input stack type (before word executes)
    pub inputs: StackType,
    /// Output stack type (after word executes)
    pub outputs: StackType,
    /// Computational side effects (Yield, etc.)
    pub effects: Vec<SideEffect>,
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
    /// Create a new stack effect (pure, no side effects)
    pub fn new(inputs: StackType, outputs: StackType) -> Self {
        Effect {
            inputs,
            outputs,
            effects: Vec::new(),
        }
    }

    /// Create a new stack effect with computational effects
    pub fn with_effects(inputs: StackType, outputs: StackType, effects: Vec<SideEffect>) -> Self {
        Effect {
            inputs,
            outputs,
            effects,
        }
    }

    /// Check if this effect is pure (no side effects)
    pub fn is_pure(&self) -> bool {
        self.effects.is_empty()
    }

    /// Check if this effect has a Yield effect
    pub fn has_yield(&self) -> bool {
        self.effects
            .iter()
            .any(|e| matches!(e, SideEffect::Yield(_)))
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Float => write!(f, "Float"),
            Type::Bool => write!(f, "Bool"),
            Type::String => write!(f, "String"),
            Type::Symbol => write!(f, "Symbol"),
            Type::Channel => write!(f, "Channel"),
            Type::Socket => write!(f, "Socket"),
            Type::Quotation(effect) => write!(f, "[{}]", effect),
            Type::Closure { effect, captures } => {
                let cap_str: Vec<_> = captures.iter().map(|t| format!("{}", t)).collect();
                write!(f, "Closure[{}, captures=({})]", effect, cap_str.join(", "))
            }
            Type::Union(name) => write!(f, "{}", name),
            Type::Variant => write!(f, "Variant"),
            Type::Var(name) => write!(f, "{}", name),
        }
    }
}

impl std::fmt::Display for StackType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StackType::Empty => write!(f, "()"),
            StackType::RowVar(name) => write!(f, "..{}", name),
            StackType::Cons { rest, top } => {
                // Collect all types from top to bottom
                let mut types = vec![format!("{}", top)];
                let mut current = rest.as_ref();
                loop {
                    match current {
                        StackType::Empty => break,
                        StackType::RowVar(name) => {
                            types.push(format!("..{}", name));
                            break;
                        }
                        StackType::Cons { rest, top } => {
                            types.push(format!("{}", top));
                            current = rest;
                        }
                    }
                }
                types.reverse();
                write!(f, "({})", types.join(" "))
            }
        }
    }
}

impl std::fmt::Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.effects.is_empty() {
            write!(f, "{} -- {}", self.inputs, self.outputs)
        } else {
            let effects_str: Vec<_> = self.effects.iter().map(|e| format!("{}", e)).collect();
            write!(
                f,
                "{} -- {} | {}",
                self.inputs,
                self.outputs,
                effects_str.join(" ")
            )
        }
    }
}

#[cfg(test)]
mod tests;
