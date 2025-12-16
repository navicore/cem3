use crate::seqstring::SeqString;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// Note: Arc is used for both Closure.env and Variant to enable O(1) cloning.
// This is essential for functional programming with recursive data structures.

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
    /// Uses Arc for O(1) cloning - essential for recursive data structures
    Variant(Arc<VariantData>),

    /// Map (key-value dictionary with O(1) lookup)
    /// Keys must be hashable types (Int, String, Bool)
    Map(Box<HashMap<MapKey, Value>>),

    /// Quotation (stateless function with two entry points for calling convention compatibility)
    /// - wrapper: C-convention entry point for calls from the runtime
    /// - impl_: tailcc entry point for tail calls from compiled code (enables TCO)
    Quotation {
        /// C-convention wrapper function pointer (for runtime calls via patch_seq_call)
        wrapper: usize,
        /// tailcc implementation function pointer (for musttail from compiled code)
        impl_: usize,
    },

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

// Safety: Value can be sent and shared between strands (green threads)
//
// Send (safe to transfer ownership between threads):
// - Int, Float, Bool are Copy types (trivially Send)
// - String (SeqString) implements Send (clone to global on transfer)
// - Variant contains Arc<VariantData> which is Send when VariantData is Send+Sync
// - Quotation stores function pointer as usize (Send-safe, no owned data)
// - Closure: fn_ptr is usize (Send), env is Arc<[Value]> (Send when Value is Send+Sync)
// - Map contains Box<HashMap> which is Send because keys and values are Send
//
// Sync (safe to share references between threads):
// - Value has no interior mutability (no Cell, RefCell, Mutex, etc.)
// - All operations on Value are read-only or create new values (functional semantics)
// - Arc requires T: Send + Sync for full thread-safety
//
// This is required for:
// - Channel communication between strands
// - Arc-based sharing of Variants and Closure environments
unsafe impl Send for Value {}
unsafe impl Sync for Value {}

/// VariantData: Composite values (sum types)
///
/// Fields are stored in a heap-allocated array, NOT linked via next pointers.
/// This is the key difference from cem2, which used StackCell.next for field linking.
///
/// # Arc and Reference Cycles
///
/// Variants use `Arc<VariantData>` for O(1) cloning, which could theoretically
/// create reference cycles. However, cycles are prevented by design:
/// - VariantData.fields is immutable (no mutation after creation)
/// - All variant operations create new variants rather than modifying existing ones
/// - The Seq language has no mutation primitives for variant fields
///
/// This functional/immutable design ensures Arc reference counts always reach zero.
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
