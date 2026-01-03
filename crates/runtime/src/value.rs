use crate::seqstring::SeqString;
use may::sync::mpmc;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Channel data: holds sender and receiver for direct handle passing
///
/// Both sender and receiver are Clone (MPMC), so duplicating a Channel value
/// just clones the Arc. Send/receive operations use the handles directly
/// with zero mutex overhead.
#[derive(Debug)]
pub struct ChannelData {
    pub sender: mpmc::Sender<Value>,
    pub receiver: mpmc::Receiver<Value>,
}

impl Clone for ChannelData {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

// PartialEq by identity (Arc pointer comparison)
impl PartialEq for ChannelData {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Message type for weave channels.
///
/// Using an enum instead of sentinel values ensures no collision with user data.
/// Any `Value` can be safely yielded/resumed, including `i64::MIN`.
#[derive(Debug, Clone, PartialEq)]
pub enum WeaveMessage {
    /// Normal value being yielded or resumed
    Value(Value),
    /// Weave completed naturally (sent on yield_chan)
    Done,
    /// Cancellation requested (sent on resume_chan)
    Cancel,
}

/// Channel data specifically for weave communication.
///
/// Uses `WeaveMessage` instead of raw `Value` to support typed control flow.
#[derive(Debug)]
pub struct WeaveChannelData {
    pub sender: mpmc::Sender<WeaveMessage>,
    pub receiver: mpmc::Receiver<WeaveMessage>,
}

impl Clone for WeaveChannelData {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
        }
    }
}

// PartialEq by identity (Arc pointer comparison)
impl PartialEq for WeaveChannelData {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

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
///
/// # Memory Layout
///
/// Using `#[repr(C)]` ensures a predictable C-compatible layout:
/// - Discriminant (tag) at offset 0
/// - Payload data follows at a fixed offset
///
/// This allows compiled code to write Values directly without FFI calls,
/// enabling inline integer/boolean operations for better performance.
#[repr(C)]
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

    /// Symbol (identifier for dynamic variant construction)
    /// Like Ruby/Clojure symbols - lightweight identifiers used for tags.
    /// Note: Currently NOT interned (each symbol allocates). Interning may be
    /// added in the future for O(1) equality comparison.
    Symbol(SeqString),

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

    /// Channel (MPMC sender/receiver pair for CSP-style concurrency)
    /// Uses Arc for O(1) cloning - duplicating a channel shares the underlying handles.
    /// Send/receive operations use the handles directly with zero mutex overhead.
    Channel(Arc<ChannelData>),

    /// Weave context (generator/coroutine communication channels)
    /// Contains both yield and resume channels for bidirectional communication.
    /// Travels on the stack - no global registry needed.
    /// Uses WeaveChannelData with WeaveMessage for type-safe control flow.
    WeaveCtx {
        yield_chan: Arc<WeaveChannelData>,
        resume_chan: Arc<WeaveChannelData>,
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
// - Channel contains Arc<ChannelData> which is Send (May's Sender/Receiver are Send)
//
// Sync (safe to share references between threads):
// - Value has no interior mutability (no Cell, RefCell, Mutex, etc.)
// - All operations on Value are read-only or create new values (functional semantics)
// - Arc requires T: Send + Sync for full thread-safety
//
// This is required for:
// - Channel communication between strands
// - Arc-based sharing of Variants, Closure environments, and Channels
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn test_value_layout() {
        // Print sizes for debugging
        println!("size_of::<Value>() = {}", size_of::<Value>());
        println!("align_of::<Value>() = {}", align_of::<Value>());

        // Verify Value is exactly 40 bytes to match StackValue layout
        // This is critical for FFI correctness between LLVM IR and Rust
        use crate::tagged_stack::StackValue;
        assert_eq!(
            size_of::<Value>(),
            size_of::<StackValue>(),
            "Value ({} bytes) must match StackValue ({} bytes) for FFI compatibility",
            size_of::<Value>(),
            size_of::<StackValue>()
        );
        assert_eq!(
            size_of::<Value>(),
            40,
            "Value must be exactly 40 bytes, got {}",
            size_of::<Value>()
        );

        // Verify alignment is 8 (for 64-bit pointers)
        assert_eq!(align_of::<Value>(), 8);
    }

    #[test]
    fn test_value_int_layout() {
        let val = Value::Int(42);
        let ptr = &val as *const Value as *const u8;

        unsafe {
            // With #[repr(C)], the discriminant is at offset 0
            // For 9 variants, discriminant fits in 1 byte but is padded
            let discriminant_byte = *ptr;
            assert_eq!(
                discriminant_byte, 0,
                "Int discriminant should be 0, got {}",
                discriminant_byte
            );

            // The i64 value should be at a fixed offset after the discriminant
            // With C repr, it's typically at offset 8 (discriminant + padding)
            let value_ptr = ptr.add(8) as *const i64;
            let stored_value = *value_ptr;
            assert_eq!(
                stored_value, 42,
                "Int value should be 42 at offset 8, got {}",
                stored_value
            );
        }
    }

    #[test]
    fn test_value_bool_layout() {
        let val_true = Value::Bool(true);
        let val_false = Value::Bool(false);
        let ptr_true = &val_true as *const Value as *const u8;
        let ptr_false = &val_false as *const Value as *const u8;

        unsafe {
            // Bool is variant index 2 (after Int=0, Float=1)
            let discriminant = *ptr_true;
            assert_eq!(
                discriminant, 2,
                "Bool discriminant should be 2, got {}",
                discriminant
            );

            // The bool value should be at offset 8
            let value_ptr_true = ptr_true.add(8);
            let value_ptr_false = ptr_false.add(8);
            assert_eq!(*value_ptr_true, 1, "true should be 1");
            assert_eq!(*value_ptr_false, 0, "false should be 0");
        }
    }
}
