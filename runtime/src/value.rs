use crate::cemstring::CemString;

/// Value: What the language talks about
///
/// This is pure data with no pointers to other values.
/// Values can be pushed on the stack, stored in variants, etc.
/// The key insight: Value is independent of Stack structure.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Integer value
    Int(i64),

    /// Boolean value
    Bool(bool),

    /// String (arena or globally allocated via CemString)
    String(CemString),

    /// Variant (sum type with tagged fields)
    Variant(Box<VariantData>),

    /// Quotation (function pointer stored as usize for Send safety)
    /// Will be properly implemented later with quotation support
    Quotation(usize),
}

// Safety: Value can be sent between strands (green threads)
// - Int, Bool, String are all Send
// - Variant contains only Send types (recursively)
// - Quotation stores function pointer as usize (Send-safe)
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
