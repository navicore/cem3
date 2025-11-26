use crate::seqstring::SeqString;

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

    /// Quotation (stateless function pointer stored as usize for Send safety)
    /// No captured environment - backward compatible
    Quotation(usize),

    /// Closure (quotation with captured environment)
    /// Contains function pointer and boxed array of captured values
    Closure {
        /// Function pointer (transmuted to function taking Stack + environment)
        fn_ptr: usize,
        /// Captured values from creation site
        /// Ordered top-down: env[0] is top of stack at creation
        env: Box<[Value]>,
    },
}

// Safety: Value can be sent between strands (green threads)
// - Int, Float, Bool, String are all Send
// - Variant contains only Send types (recursively)
// - Quotation stores function pointer as usize (Send-safe)
// - Closure: fn_ptr is usize (Send), env is Box<[Value]> (Send because Value is Send)
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
