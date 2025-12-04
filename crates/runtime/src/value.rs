use crate::seqstring::SeqString;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// MapKey: Hashable subset of Value for use as map keys
///
/// Only types that can be meaningfully hashed are allowed as map keys:
/// Int, String, Bool. Float is excluded due to NaN equality issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MapKey {
    Int(i64),
    String(SeqString),
    Bool(bool),
}

impl Hash for MapKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Discriminant for type safety
        std::mem::discriminant(self).hash(state);
        match self {
            MapKey::Int(n) => n.hash(state),
            MapKey::String(s) => s.as_str().hash(state),
            MapKey::Bool(b) => b.hash(state),
        }
    }
}

impl MapKey {
    /// Try to convert a Value to a MapKey
    /// Returns None for non-hashable types (Float, Variant, Quotation, Closure, Map)
    pub fn from_value(value: &Value) -> Option<MapKey> {
        match value {
            Value::Int(n) => Some(MapKey::Int(*n)),
            Value::String(s) => Some(MapKey::String(s.clone())),
            Value::Bool(b) => Some(MapKey::Bool(*b)),
            _ => None,
        }
    }

    /// Convert MapKey back to Value
    pub fn to_value(&self) -> Value {
        match self {
            MapKey::Int(n) => Value::Int(*n),
            MapKey::String(s) => Value::String(s.clone()),
            MapKey::Bool(b) => Value::Bool(*b),
        }
    }
}

/// Value: What the language talks about
///
/// This is pure data with no pointers to other values.
/// Values can be pushed on the stack, stored in variants, etc.
/// The key insight: Value is independent of Stack structure.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value
    Int(i64),

    /// Floating-point value (IEEE 754 double precision)
    Float(f64),

    /// Boolean value
    Bool(bool),

    /// String (arena or globally allocated via SeqString)
    String(SeqString),

    /// Variant (sum type with tagged fields)
    Variant(Box<VariantData>),

    /// Map (key-value dictionary with O(1) lookup)
    /// Keys must be hashable types (Int, String, Bool)
    Map(Box<HashMap<MapKey, Value>>),

    /// Quotation (stateless function pointer stored as usize for Send safety)
    /// No captured environment - backward compatible
    Quotation(usize),

    /// Closure (quotation with captured environment)
    /// Contains function pointer and Arc-shared array of captured values.
    /// Arc enables TCO: no cleanup needed after tail call, ref-count handles it.
    Closure {
        /// Function pointer (transmuted to function taking Stack + environment)
        fn_ptr: usize,
        /// Captured values from creation site (Arc for TCO support)
        /// Ordered top-down: env[0] is top of stack at creation
        env: Arc<[Value]>,
    },
}

// Safety: Value can be sent between strands (green threads)
// - Int, Float, Bool, String are all Send
// - Variant contains only Send types (recursively)
// - Quotation stores function pointer as usize (Send-safe)
// - Closure: fn_ptr is usize (Send), env is Arc<[Value]> (Send+Sync because Value is Send)
// Arc is used instead of Box to enable TCO: the ref-count handles cleanup automatically
// This is required for channel communication between strands
unsafe impl Send for Value {}

/// VariantData: Composite values (sum types)
///
/// Fields are stored in a heap-allocated array, NOT linked via next pointers.
/// This is the key difference from cem2, which used StackCell.next for field linking.
#[derive(Debug, Clone, PartialEq)]
pub struct VariantData {
    /// Tag identifies which variant constructor was used
    pub tag: u32,

    /// Fields stored as an owned array of values
    /// This is independent of any stack structure
    pub fields: Box<[Value]>,
}

impl VariantData {
    /// Create a new variant with the given tag and fields
    pub fn new(tag: u32, fields: Vec<Value>) -> Self {
        Self {
            tag,
            fields: fields.into_boxed_slice(),
        }
    }
}

// We'll implement proper cleanup in Drop later
// For now, Rust's ownership handles most of it
