//! NaN-Boxing Implementation
//!
//! Encodes Seq values into 8 bytes using IEEE 754 NaN-boxing.
//! This reduces Value size from 40 bytes to 8 bytes, improving cache
//! utilization and reducing memory bandwidth.
//!
//! ## Encoding Scheme
//!
//! IEEE 754 doubles use specific bit patterns for NaN. We encode non-float
//! values in the "quiet NaN" space:
//!
//! ```text
//! Float (normal):    [any valid IEEE 754 double that isn't in our quiet NaN range]
//! Boxed values:      0x7FF8_TTTT_PPPP_PPPP
//!                         ^^^^-- 4-bit type tag (0-15)
//!                              ^^^^^^^^^^^-- 48-bit payload
//! ```
//!
//! ## Type Tags
//!
//! - 0x0: Int (48-bit signed integer, range ~±140 trillion)
//! - 0x1: Bool (0 or 1 in low bit)
//! - 0x2: String (48-bit pointer to SeqString)
//! - 0x3: Symbol (48-bit pointer to SeqString)
//! - 0x4: Variant (48-bit pointer to Arc<VariantData>)
//! - 0x5: Map (48-bit pointer to Box<HashMap>)
//! - 0x6: Quotation (48-bit pointer to QuotationData)
//! - 0x7: Closure (48-bit pointer to ClosureData)
//! - 0x8: Channel (48-bit pointer to Arc<ChannelData>)
//! - 0x9: WeaveCtx (48-bit pointer to WeaveCtxData)
//!
//! ## Float Handling
//!
//! Real float values are stored directly as IEEE 754 doubles. To distinguish
//! them from boxed values, we canonicalize NaN results to a specific pattern
//! that doesn't collide with our tagged encoding.

use std::collections::HashMap;
use std::sync::Arc;

use crate::seqstring::SeqString;
use crate::value::{ChannelData, MapKey, Value, VariantData, WeaveChannelData};

// =============================================================================
// Constants
// =============================================================================

/// We use negative quiet NaNs (sign bit + exponent all 1s + quiet bit) for boxing.
/// Any value >= this threshold is a boxed value, not a float.
/// This leaves all positive floats (including +inf, +NaN) untouched.
const NANBOX_THRESHOLD: u64 = 0xFFFC_0000_0000_0000;

/// Base value for boxed types (0xFFFC in high 16 bits)
const NANBOX_BASE: u64 = 0xFFFC_0000_0000_0000;

/// Mask for the 4-bit type tag (stored in bits 47:44)
const TAG_MASK: u64 = 0x0000_F000_0000_0000;

/// Shift amount for the type tag (44 bits)
const TAG_SHIFT: u32 = 44;

/// Mask for the 44-bit payload (stored in bits 43:0)
const PAYLOAD_MASK: u64 = 0x0000_0FFF_FFFF_FFFF;

/// Canonical NaN value (used when float operations produce NaN)
/// This is a positive quiet NaN that doesn't collide with our boxed encoding
pub const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

/// Maximum 44-bit signed integer: 2^43 - 1 = 8,796,093,022,207 (~8.8 trillion)
pub const MAX_NANBOX_INT: i64 = (1i64 << 43) - 1;

/// Minimum 44-bit signed integer: -2^43 = -8,796,093,022,208
pub const MIN_NANBOX_INT: i64 = -(1i64 << 43);

// =============================================================================
// Type Tags
// =============================================================================

/// Type tags for NaN-boxed values
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NanBoxTag {
    Int = 0,
    Bool = 1,
    String = 2,
    Symbol = 3,
    Variant = 4,
    Map = 5,
    Quotation = 6,
    Closure = 7,
    Channel = 8,
    WeaveCtx = 9,
}

// =============================================================================
// Heap-Allocated Data Types
// =============================================================================

/// Quotation data stored on the heap for NaN-boxing
///
/// In the 40-byte Value, Quotation stored two pointers inline.
/// With NaN-boxing, we need to heap-allocate this struct.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct QuotationData {
    /// C-convention wrapper function pointer (for runtime calls)
    pub wrapper: usize,
    /// tailcc implementation function pointer (for musttail from compiled code)
    pub impl_: usize,
}

/// Closure data stored on the heap for NaN-boxing
///
/// In the 40-byte Value, Closure stored fn_ptr and env pointer inline.
/// With NaN-boxing, we heap-allocate this struct.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct ClosureData {
    /// Function pointer
    pub fn_ptr: usize,
    /// Captured environment (Arc for shared ownership)
    pub env: Arc<[Value]>,
}

/// Weave context data stored on the heap for NaN-boxing
///
/// In the 40-byte Value, WeaveCtx stored two Arc pointers inline.
/// With NaN-boxing, we heap-allocate this struct.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct WeaveCtxData {
    /// Channel for yielding values from weave to consumer
    pub yield_chan: Arc<WeaveChannelData>,
    /// Channel for resuming with values from consumer to weave
    pub resume_chan: Arc<WeaveChannelData>,
}

// =============================================================================
// NanBoxedValue
// =============================================================================

/// An 8-byte NaN-boxed value
///
/// This is the core type for the NaN-boxing optimization. All Seq values
/// are encoded into exactly 8 bytes.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct NanBoxedValue(u64);

impl NanBoxedValue {
    // =========================================================================
    // Type Checking
    // =========================================================================

    /// Check if this value is a float (not in the NaN-box range)
    #[inline(always)]
    pub fn is_float(self) -> bool {
        // A value is a float if it's below our boxed threshold
        // All positive floats (including +inf, +NaN) are below 0xFFFC...
        self.0 < NANBOX_THRESHOLD
    }

    /// Check if this value is a boxed (non-float) value
    #[inline(always)]
    pub fn is_boxed(self) -> bool {
        self.0 >= NANBOX_THRESHOLD
    }

    /// Get the type tag (only valid if is_boxed() is true)
    #[inline(always)]
    pub fn tag(self) -> u8 {
        debug_assert!(self.is_boxed(), "tag() called on float value");
        // Tag is in bits 47:44
        ((self.0 & TAG_MASK) >> TAG_SHIFT) as u8
    }

    /// Get the 44-bit payload (only valid if is_boxed() is true)
    #[inline(always)]
    pub fn payload(self) -> u64 {
        debug_assert!(self.is_boxed(), "payload() called on float value");
        self.0 & PAYLOAD_MASK
    }

    /// Check if this is an Int
    #[inline(always)]
    pub fn is_int(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Int as u8
    }

    /// Check if this is a Bool
    #[inline(always)]
    pub fn is_bool(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Bool as u8
    }

    /// Check if this is a String
    #[inline(always)]
    pub fn is_string(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::String as u8
    }

    /// Check if this is a Symbol
    #[inline(always)]
    pub fn is_symbol(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Symbol as u8
    }

    /// Check if this is a Variant
    #[inline(always)]
    pub fn is_variant(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Variant as u8
    }

    /// Check if this is a Map
    #[inline(always)]
    pub fn is_map(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Map as u8
    }

    /// Check if this is a Quotation
    #[inline(always)]
    pub fn is_quotation(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Quotation as u8
    }

    /// Check if this is a Closure
    #[inline(always)]
    pub fn is_closure(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Closure as u8
    }

    /// Check if this is a Channel
    #[inline(always)]
    pub fn is_channel(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::Channel as u8
    }

    /// Check if this is a WeaveCtx
    #[inline(always)]
    pub fn is_weave_ctx(self) -> bool {
        self.is_boxed() && self.tag() == NanBoxTag::WeaveCtx as u8
    }

    // =========================================================================
    // Encoding (Creating NanBoxedValue)
    // =========================================================================

    /// Helper to create a boxed value from tag and payload
    #[inline(always)]
    fn make_boxed(tag: NanBoxTag, payload: u64) -> Self {
        // Encoding: 0xFFFC in bits 63:48, tag in bits 47:44, payload in bits 43:0
        debug_assert!(
            payload <= PAYLOAD_MASK,
            "Payload 0x{:x} exceeds 44-bit limit",
            payload
        );
        NanBoxedValue(NANBOX_BASE | ((tag as u64) << TAG_SHIFT) | payload)
    }

    /// Create a NaN-boxed float
    ///
    /// If the float is NaN, it's canonicalized to avoid collision with boxed values.
    #[inline(always)]
    pub fn from_float(f: f64) -> Self {
        let bits = f.to_bits();
        // Check if this is a NaN that could collide with our boxed range
        if bits >= NANBOX_THRESHOLD {
            // Canonicalize to a safe NaN
            NanBoxedValue(CANONICAL_NAN)
        } else {
            NanBoxedValue(bits)
        }
    }

    /// Create a NaN-boxed integer
    ///
    /// # Panics
    /// Panics if the integer is outside the 48-bit signed range.
    #[inline(always)]
    pub fn from_int(n: i64) -> Self {
        debug_assert!(
            (MIN_NANBOX_INT..=MAX_NANBOX_INT).contains(&n),
            "Integer {} outside NaN-boxing range [{}, {}]",
            n,
            MIN_NANBOX_INT,
            MAX_NANBOX_INT
        );
        // Sign-extend handling: mask to 48 bits
        let payload = (n as u64) & PAYLOAD_MASK;
        Self::make_boxed(NanBoxTag::Int, payload)
    }

    /// Create a NaN-boxed integer, returning None if out of range
    #[inline(always)]
    pub fn try_from_int(n: i64) -> Option<Self> {
        if (MIN_NANBOX_INT..=MAX_NANBOX_INT).contains(&n) {
            Some(Self::from_int(n))
        } else {
            None
        }
    }

    /// Create a NaN-boxed boolean
    #[inline(always)]
    pub fn from_bool(b: bool) -> Self {
        let payload = if b { 1 } else { 0 };
        Self::make_boxed(NanBoxTag::Bool, payload)
    }

    /// Create a NaN-boxed string pointer
    ///
    /// # Safety
    /// The pointer must be valid and properly aligned.
    #[inline(always)]
    pub fn from_string_ptr(ptr: *const SeqString) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "String pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::String, payload)
    }

    /// Create a NaN-boxed symbol pointer
    ///
    /// # Safety
    /// The pointer must be valid and properly aligned.
    #[inline(always)]
    pub fn from_symbol_ptr(ptr: *const SeqString) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Symbol pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Symbol, payload)
    }

    /// Create a NaN-boxed variant pointer
    #[inline(always)]
    pub fn from_variant_ptr(ptr: *const Arc<VariantData>) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Variant pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Variant, payload)
    }

    /// Create a NaN-boxed map pointer
    #[inline(always)]
    pub fn from_map_ptr(ptr: *const Box<HashMap<MapKey, Value>>) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Map pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Map, payload)
    }

    /// Create a NaN-boxed quotation pointer
    #[inline(always)]
    pub fn from_quotation_ptr(ptr: *const QuotationData) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Quotation pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Quotation, payload)
    }

    /// Create a NaN-boxed closure pointer
    #[inline(always)]
    pub fn from_closure_ptr(ptr: *const ClosureData) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Closure pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Closure, payload)
    }

    /// Create a NaN-boxed channel pointer
    #[inline(always)]
    pub fn from_channel_ptr(ptr: *const Arc<ChannelData>) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "Channel pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::Channel, payload)
    }

    /// Create a NaN-boxed weave context pointer
    #[inline(always)]
    pub fn from_weave_ctx_ptr(ptr: *const WeaveCtxData) -> Self {
        let payload = (ptr as u64) & PAYLOAD_MASK;
        debug_assert_eq!(
            payload, ptr as u64,
            "WeaveCtx pointer exceeds 48-bit address space"
        );
        Self::make_boxed(NanBoxTag::WeaveCtx, payload)
    }

    // =========================================================================
    // Decoding (Extracting values)
    // =========================================================================

    /// Extract a float value
    ///
    /// # Panics
    /// Panics in debug mode if this is not a float.
    #[inline(always)]
    pub fn as_float(self) -> f64 {
        debug_assert!(self.is_float(), "as_float() called on non-float value");
        f64::from_bits(self.0)
    }

    /// Extract an integer value
    ///
    /// # Panics
    /// Panics in debug mode if this is not an integer.
    #[inline(always)]
    pub fn as_int(self) -> i64 {
        debug_assert!(self.is_int(), "as_int() called on non-int value");
        // Sign-extend from 44 bits to 64 bits
        let payload = self.payload();
        // Check if the sign bit (bit 43) is set
        if payload & (1 << 43) != 0 {
            // Negative: sign-extend by setting upper 20 bits
            (payload | 0xFFFF_F000_0000_0000) as i64
        } else {
            payload as i64
        }
    }

    /// Extract a boolean value
    ///
    /// # Panics
    /// Panics in debug mode if this is not a boolean.
    #[inline(always)]
    pub fn as_bool(self) -> bool {
        debug_assert!(self.is_bool(), "as_bool() called on non-bool value");
        self.payload() != 0
    }

    /// Extract a string pointer
    ///
    /// # Safety
    /// The caller must ensure this is a String value.
    #[inline(always)]
    pub unsafe fn as_string_ptr(self) -> *const SeqString {
        debug_assert!(
            self.is_string(),
            "as_string_ptr() called on non-string value"
        );
        self.payload() as *const SeqString
    }

    /// Extract a symbol pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Symbol value.
    #[inline(always)]
    pub unsafe fn as_symbol_ptr(self) -> *const SeqString {
        debug_assert!(
            self.is_symbol(),
            "as_symbol_ptr() called on non-symbol value"
        );
        self.payload() as *const SeqString
    }

    /// Extract a variant pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Variant value.
    #[inline(always)]
    pub unsafe fn as_variant_ptr(self) -> *const Arc<VariantData> {
        debug_assert!(
            self.is_variant(),
            "as_variant_ptr() called on non-variant value"
        );
        self.payload() as *const Arc<VariantData>
    }

    /// Extract a map pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Map value.
    #[inline(always)]
    pub unsafe fn as_map_ptr(self) -> *const Box<HashMap<MapKey, Value>> {
        debug_assert!(self.is_map(), "as_map_ptr() called on non-map value");
        self.payload() as *const Box<HashMap<MapKey, Value>>
    }

    /// Extract a quotation pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Quotation value.
    #[inline(always)]
    pub unsafe fn as_quotation_ptr(self) -> *const QuotationData {
        debug_assert!(
            self.is_quotation(),
            "as_quotation_ptr() called on non-quotation value"
        );
        self.payload() as *const QuotationData
    }

    /// Extract a closure pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Closure value.
    #[inline(always)]
    pub unsafe fn as_closure_ptr(self) -> *const ClosureData {
        debug_assert!(
            self.is_closure(),
            "as_closure_ptr() called on non-closure value"
        );
        self.payload() as *const ClosureData
    }

    /// Extract a channel pointer
    ///
    /// # Safety
    /// The caller must ensure this is a Channel value.
    #[inline(always)]
    pub unsafe fn as_channel_ptr(self) -> *const Arc<ChannelData> {
        debug_assert!(
            self.is_channel(),
            "as_channel_ptr() called on non-channel value"
        );
        self.payload() as *const Arc<ChannelData>
    }

    /// Extract a weave context pointer
    ///
    /// # Safety
    /// The caller must ensure this is a WeaveCtx value.
    #[inline(always)]
    pub unsafe fn as_weave_ctx_ptr(self) -> *const WeaveCtxData {
        debug_assert!(
            self.is_weave_ctx(),
            "as_weave_ctx_ptr() called on non-weave_ctx value"
        );
        self.payload() as *const WeaveCtxData
    }

    // =========================================================================
    // Raw Access
    // =========================================================================

    /// Get the raw 64-bit representation
    #[inline(always)]
    pub fn to_bits(self) -> u64 {
        self.0
    }

    /// Create from raw 64-bit representation
    ///
    /// # Safety
    /// The caller must ensure the bits represent a valid NanBoxedValue.
    #[inline(always)]
    pub unsafe fn from_bits(bits: u64) -> Self {
        NanBoxedValue(bits)
    }
}

impl std::fmt::Debug for NanBoxedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_float() {
            write!(f, "Float({})", self.as_float())
        } else {
            match self.tag() {
                t if t == NanBoxTag::Int as u8 => write!(f, "Int({})", self.as_int()),
                t if t == NanBoxTag::Bool as u8 => write!(f, "Bool({})", self.as_bool()),
                t if t == NanBoxTag::String as u8 => {
                    write!(f, "String(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::Symbol as u8 => {
                    write!(f, "Symbol(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::Variant as u8 => {
                    write!(f, "Variant(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::Map as u8 => write!(f, "Map(0x{:012x})", self.payload()),
                t if t == NanBoxTag::Quotation as u8 => {
                    write!(f, "Quotation(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::Closure as u8 => {
                    write!(f, "Closure(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::Channel as u8 => {
                    write!(f, "Channel(0x{:012x})", self.payload())
                }
                t if t == NanBoxTag::WeaveCtx as u8 => {
                    write!(f, "WeaveCtx(0x{:012x})", self.payload())
                }
                _ => write!(
                    f,
                    "Unknown(tag={}, payload=0x{:012x})",
                    self.tag(),
                    self.payload()
                ),
            }
        }
    }
}

impl Default for NanBoxedValue {
    fn default() -> Self {
        // Default to integer 0
        Self::from_int(0)
    }
}

// Safety: NanBoxedValue is just a u64, which is Send + Sync
unsafe impl Send for NanBoxedValue {}
unsafe impl Sync for NanBoxedValue {}

// =============================================================================
// Value <-> NanBoxedValue Conversions
// =============================================================================

impl NanBoxedValue {
    /// Convert a Value to a NanBoxedValue
    ///
    /// This clones heap-allocated data and stores pointers in the NaN-boxed format.
    /// The caller is responsible for ensuring the returned NanBoxedValue is properly
    /// dropped (via `drop_nanboxed`) to avoid memory leaks.
    ///
    /// # Panics
    /// Panics if an integer value is outside the 48-bit range.
    pub fn from_value(value: &Value) -> Self {
        match value {
            Value::Int(n) => {
                // Check range at runtime - in production, compiler should reject out-of-range literals
                if *n < MIN_NANBOX_INT || *n > MAX_NANBOX_INT {
                    panic!(
                        "Integer {} outside NaN-boxing range [{}, {}]",
                        n, MIN_NANBOX_INT, MAX_NANBOX_INT
                    );
                }
                Self::from_int(*n)
            }
            Value::Float(f) => Self::from_float(*f),
            Value::Bool(b) => Self::from_bool(*b),
            Value::String(s) => {
                // Clone the SeqString and leak it
                let boxed = Box::new(s.clone());
                let ptr = Box::into_raw(boxed);
                Self::from_string_ptr(ptr)
            }
            Value::Symbol(s) => {
                let boxed = Box::new(s.clone());
                let ptr = Box::into_raw(boxed);
                Self::from_symbol_ptr(ptr)
            }
            Value::Variant(arc) => {
                // Clone the Arc and leak it
                let arc_clone = arc.clone();
                let boxed = Box::new(arc_clone);
                let ptr = Box::into_raw(boxed);
                Self::from_variant_ptr(ptr)
            }
            Value::Map(map) => {
                // Clone the map and leak it
                let map_clone = map.clone();
                let boxed = Box::new(map_clone);
                let ptr = Box::into_raw(boxed);
                Self::from_map_ptr(ptr)
            }
            Value::Quotation { wrapper, impl_ } => {
                let data = Box::new(QuotationData {
                    wrapper: *wrapper,
                    impl_: *impl_,
                });
                let ptr = Box::into_raw(data);
                Self::from_quotation_ptr(ptr)
            }
            Value::Closure { fn_ptr, env } => {
                let data = Box::new(ClosureData {
                    fn_ptr: *fn_ptr,
                    env: env.clone(),
                });
                let ptr = Box::into_raw(data);
                Self::from_closure_ptr(ptr)
            }
            Value::Channel(arc) => {
                let arc_clone = arc.clone();
                let boxed = Box::new(arc_clone);
                let ptr = Box::into_raw(boxed);
                Self::from_channel_ptr(ptr)
            }
            Value::WeaveCtx {
                yield_chan,
                resume_chan,
            } => {
                let data = Box::new(WeaveCtxData {
                    yield_chan: yield_chan.clone(),
                    resume_chan: resume_chan.clone(),
                });
                let ptr = Box::into_raw(data);
                Self::from_weave_ctx_ptr(ptr)
            }
        }
    }

    /// Convert a NanBoxedValue back to a Value
    ///
    /// This reconstructs the Value from the NaN-boxed representation.
    /// For heap-allocated types, this takes ownership of the underlying memory.
    ///
    /// # Safety
    /// The NanBoxedValue must have been created by `from_value` and not yet
    /// converted back or dropped. Each NanBoxedValue should only be converted
    /// to Value once.
    pub unsafe fn to_value(self) -> Value {
        if self.is_float() {
            return Value::Float(self.as_float());
        }

        match self.tag() {
            t if t == NanBoxTag::Int as u8 => Value::Int(self.as_int()),
            t if t == NanBoxTag::Bool as u8 => Value::Bool(self.as_bool()),
            t if t == NanBoxTag::String as u8 => unsafe {
                let ptr = self.as_string_ptr() as *mut SeqString;
                let boxed = Box::from_raw(ptr);
                Value::String(*boxed)
            },
            t if t == NanBoxTag::Symbol as u8 => unsafe {
                let ptr = self.as_symbol_ptr() as *mut SeqString;
                let boxed = Box::from_raw(ptr);
                Value::Symbol(*boxed)
            },
            t if t == NanBoxTag::Variant as u8 => unsafe {
                let ptr = self.as_variant_ptr() as *mut Arc<VariantData>;
                let boxed = Box::from_raw(ptr);
                Value::Variant(*boxed)
            },
            t if t == NanBoxTag::Map as u8 => unsafe {
                let ptr = self.as_map_ptr() as *mut Box<HashMap<MapKey, Value>>;
                let boxed = Box::from_raw(ptr);
                Value::Map(*boxed)
            },
            t if t == NanBoxTag::Quotation as u8 => unsafe {
                let ptr = self.as_quotation_ptr() as *mut QuotationData;
                let data = Box::from_raw(ptr);
                Value::Quotation {
                    wrapper: data.wrapper,
                    impl_: data.impl_,
                }
            },
            t if t == NanBoxTag::Closure as u8 => unsafe {
                let ptr = self.as_closure_ptr() as *mut ClosureData;
                let data = Box::from_raw(ptr);
                Value::Closure {
                    fn_ptr: data.fn_ptr,
                    env: data.env,
                }
            },
            t if t == NanBoxTag::Channel as u8 => unsafe {
                let ptr = self.as_channel_ptr() as *mut Arc<ChannelData>;
                let boxed = Box::from_raw(ptr);
                Value::Channel(*boxed)
            },
            t if t == NanBoxTag::WeaveCtx as u8 => unsafe {
                let ptr = self.as_weave_ctx_ptr() as *mut WeaveCtxData;
                let data = Box::from_raw(ptr);
                Value::WeaveCtx {
                    yield_chan: data.yield_chan,
                    resume_chan: data.resume_chan,
                }
            },
            _ => panic!("Unknown NanBoxedValue tag: {}", self.tag()),
        }
    }

    /// Clone a NanBoxedValue, properly cloning any heap-allocated data
    ///
    /// For pointer types, this creates new heap allocations.
    pub fn clone_nanboxed(&self) -> Self {
        if self.is_float() {
            return *self;
        }

        match self.tag() {
            t if t == NanBoxTag::Int as u8 => *self,
            t if t == NanBoxTag::Bool as u8 => *self,
            t if t == NanBoxTag::String as u8 => {
                let ptr = unsafe { self.as_string_ptr() };
                let s = unsafe { &*ptr };
                let boxed = Box::new(s.clone());
                Self::from_string_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Symbol as u8 => {
                let ptr = unsafe { self.as_symbol_ptr() };
                let s = unsafe { &*ptr };
                let boxed = Box::new(s.clone());
                Self::from_symbol_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Variant as u8 => {
                let ptr = unsafe { self.as_variant_ptr() };
                let arc = unsafe { &*ptr };
                let boxed = Box::new(arc.clone());
                Self::from_variant_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Map as u8 => {
                let ptr = unsafe { self.as_map_ptr() };
                let map = unsafe { &*ptr };
                let boxed = Box::new(map.clone());
                Self::from_map_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Quotation as u8 => {
                let ptr = unsafe { self.as_quotation_ptr() };
                let data = unsafe { &*ptr };
                let boxed = Box::new(data.clone());
                Self::from_quotation_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Closure as u8 => {
                let ptr = unsafe { self.as_closure_ptr() };
                let data = unsafe { &*ptr };
                let boxed = Box::new(data.clone());
                Self::from_closure_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::Channel as u8 => {
                let ptr = unsafe { self.as_channel_ptr() };
                let arc = unsafe { &*ptr };
                let boxed = Box::new(arc.clone());
                Self::from_channel_ptr(Box::into_raw(boxed))
            }
            t if t == NanBoxTag::WeaveCtx as u8 => {
                let ptr = unsafe { self.as_weave_ctx_ptr() };
                let data = unsafe { &*ptr };
                let boxed = Box::new(data.clone());
                Self::from_weave_ctx_ptr(Box::into_raw(boxed))
            }
            _ => *self, // Unknown tag, just copy bits
        }
    }

    /// Drop a NanBoxedValue, freeing any heap-allocated data
    ///
    /// This must be called for NanBoxedValues that hold pointer types
    /// to avoid memory leaks.
    ///
    /// # Safety
    /// The NanBoxedValue must have been created by `from_value` or `clone_nanboxed`
    /// and not yet dropped or converted to Value.
    pub unsafe fn drop_nanboxed(self) {
        if self.is_float() {
            return;
        }

        match self.tag() {
            t if t == NanBoxTag::Int as u8 => {}
            t if t == NanBoxTag::Bool as u8 => {}
            t if t == NanBoxTag::String as u8 => unsafe {
                let ptr = self.as_string_ptr() as *mut SeqString;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Symbol as u8 => unsafe {
                let ptr = self.as_symbol_ptr() as *mut SeqString;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Variant as u8 => unsafe {
                let ptr = self.as_variant_ptr() as *mut Arc<VariantData>;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Map as u8 => unsafe {
                let ptr = self.as_map_ptr() as *mut Box<HashMap<MapKey, Value>>;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Quotation as u8 => unsafe {
                let ptr = self.as_quotation_ptr() as *mut QuotationData;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Closure as u8 => unsafe {
                let ptr = self.as_closure_ptr() as *mut ClosureData;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::Channel as u8 => unsafe {
                let ptr = self.as_channel_ptr() as *mut Arc<ChannelData>;
                drop(Box::from_raw(ptr));
            },
            t if t == NanBoxTag::WeaveCtx as u8 => unsafe {
                let ptr = self.as_weave_ctx_ptr() as *mut WeaveCtxData;
                drop(Box::from_raw(ptr));
            },
            _ => {} // Unknown tag, nothing to drop
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nanboxed_value_size() {
        assert_eq!(std::mem::size_of::<NanBoxedValue>(), 8);
        assert_eq!(std::mem::align_of::<NanBoxedValue>(), 8);
    }

    #[test]
    fn test_float_encoding() {
        // Normal floats
        let v = NanBoxedValue::from_float(2.5);
        assert!(v.is_float());
        assert!(!v.is_boxed());
        assert_eq!(v.as_float(), 2.5);

        // Zero
        let v = NanBoxedValue::from_float(0.0);
        assert!(v.is_float());
        assert_eq!(v.as_float(), 0.0);

        // Negative
        let v = NanBoxedValue::from_float(-123.456);
        assert!(v.is_float());
        assert_eq!(v.as_float(), -123.456);

        // Infinity
        let v = NanBoxedValue::from_float(f64::INFINITY);
        assert!(v.is_float());
        assert!(v.as_float().is_infinite());

        // Negative infinity
        let v = NanBoxedValue::from_float(f64::NEG_INFINITY);
        assert!(v.is_float());
        assert!(v.as_float().is_infinite());
    }

    #[test]
    fn test_nan_canonicalization() {
        // Standard NaN should be preserved (it's outside our boxed range)
        let v = NanBoxedValue::from_float(f64::NAN);
        assert!(v.is_float());
        assert!(v.as_float().is_nan());
    }

    #[test]
    fn test_int_encoding() {
        // Zero
        let v = NanBoxedValue::from_int(0);
        assert!(v.is_int());
        assert_eq!(v.as_int(), 0);

        // Positive
        let v = NanBoxedValue::from_int(42);
        assert!(v.is_int());
        assert_eq!(v.as_int(), 42);

        // Negative
        let v = NanBoxedValue::from_int(-42);
        assert!(v.is_int());
        assert_eq!(v.as_int(), -42);

        // Large positive (within 48-bit range)
        let v = NanBoxedValue::from_int(MAX_NANBOX_INT);
        assert!(v.is_int());
        assert_eq!(v.as_int(), MAX_NANBOX_INT);

        // Large negative (within 48-bit range)
        let v = NanBoxedValue::from_int(MIN_NANBOX_INT);
        assert!(v.is_int());
        assert_eq!(v.as_int(), MIN_NANBOX_INT);

        // Common values
        let v = NanBoxedValue::from_int(1000000);
        assert_eq!(v.as_int(), 1000000);

        let v = NanBoxedValue::from_int(-1);
        assert_eq!(v.as_int(), -1);
    }

    #[test]
    fn test_try_from_int() {
        // Within range
        assert!(NanBoxedValue::try_from_int(0).is_some());
        assert!(NanBoxedValue::try_from_int(MAX_NANBOX_INT).is_some());
        assert!(NanBoxedValue::try_from_int(MIN_NANBOX_INT).is_some());

        // Outside range
        assert!(NanBoxedValue::try_from_int(MAX_NANBOX_INT + 1).is_none());
        assert!(NanBoxedValue::try_from_int(MIN_NANBOX_INT - 1).is_none());
        assert!(NanBoxedValue::try_from_int(i64::MAX).is_none());
        assert!(NanBoxedValue::try_from_int(i64::MIN).is_none());
    }

    #[test]
    fn test_bool_encoding() {
        let v_true = NanBoxedValue::from_bool(true);
        assert!(v_true.is_bool());
        assert!(v_true.as_bool());

        let v_false = NanBoxedValue::from_bool(false);
        assert!(v_false.is_bool());
        assert!(!v_false.as_bool());
    }

    #[test]
    fn test_type_discrimination() {
        let float = NanBoxedValue::from_float(1.0);
        let int = NanBoxedValue::from_int(1);
        let bool_val = NanBoxedValue::from_bool(true);

        // Float checks
        assert!(float.is_float());
        assert!(!float.is_int());
        assert!(!float.is_bool());

        // Int checks
        assert!(!int.is_float());
        assert!(int.is_int());
        assert!(!int.is_bool());

        // Bool checks
        assert!(!bool_val.is_float());
        assert!(!bool_val.is_int());
        assert!(bool_val.is_bool());
    }

    #[test]
    fn test_int_range_boundaries() {
        // Test values near the boundaries
        let near_max = MAX_NANBOX_INT - 1;
        let near_min = MIN_NANBOX_INT + 1;

        let v = NanBoxedValue::from_int(near_max);
        assert_eq!(v.as_int(), near_max);

        let v = NanBoxedValue::from_int(near_min);
        assert_eq!(v.as_int(), near_min);
    }

    #[test]
    fn test_pointer_encoding() {
        // Create a test value on the heap to get a valid pointer
        let boxed: Box<u64> = Box::new(42);
        let ptr = Box::into_raw(boxed);

        // The pointer should fit in 48 bits on x86-64 and ARM64
        let ptr_val = ptr as u64;
        assert!(
            ptr_val <= PAYLOAD_MASK,
            "Pointer 0x{:x} exceeds 48-bit range",
            ptr_val
        );

        // Clean up
        unsafe {
            drop(Box::from_raw(ptr));
        }
    }

    #[test]
    fn test_quotation_data_size() {
        // QuotationData should be 16 bytes (2 x usize on 64-bit)
        assert_eq!(std::mem::size_of::<QuotationData>(), 16);
    }

    #[test]
    fn test_debug_format() {
        let float = NanBoxedValue::from_float(2.5);
        let int = NanBoxedValue::from_int(42);
        let bool_val = NanBoxedValue::from_bool(true);

        assert!(format!("{:?}", float).contains("Float"));
        assert!(format!("{:?}", int).contains("Int(42)"));
        assert!(format!("{:?}", bool_val).contains("Bool(true)"));
    }

    #[test]
    fn test_default() {
        let v = NanBoxedValue::default();
        assert!(v.is_int());
        assert_eq!(v.as_int(), 0);
    }

    #[test]
    fn test_raw_bits_roundtrip() {
        let original = NanBoxedValue::from_int(12345);
        let bits = original.to_bits();
        let restored = unsafe { NanBoxedValue::from_bits(bits) };
        assert_eq!(restored.as_int(), 12345);
    }

    // =========================================================================
    // Additional Edge Case Tests
    // =========================================================================

    #[test]
    fn test_negative_zero() {
        let v = NanBoxedValue::from_float(-0.0);
        assert!(v.is_float());
        // -0.0 and 0.0 compare equal in IEEE 754
        assert_eq!(v.as_float(), 0.0);
        // But the bits are different
        assert_eq!(v.to_bits(), (-0.0_f64).to_bits());
    }

    #[test]
    fn test_subnormal_floats() {
        // Smallest positive subnormal
        let smallest = f64::from_bits(1);
        let v = NanBoxedValue::from_float(smallest);
        assert!(v.is_float());
        assert_eq!(v.as_float().to_bits(), 1);

        // Largest subnormal (just below smallest normal)
        let largest_subnormal = f64::from_bits(0x000F_FFFF_FFFF_FFFF);
        let v = NanBoxedValue::from_float(largest_subnormal);
        assert!(v.is_float());
        assert_eq!(v.as_float().to_bits(), 0x000F_FFFF_FFFF_FFFF);
    }

    #[test]
    fn test_special_float_values() {
        // f64::MIN (largest negative)
        let v = NanBoxedValue::from_float(f64::MIN);
        assert!(v.is_float());
        assert_eq!(v.as_float(), f64::MIN);

        // f64::MAX (largest positive)
        let v = NanBoxedValue::from_float(f64::MAX);
        assert!(v.is_float());
        assert_eq!(v.as_float(), f64::MAX);

        // f64::MIN_POSITIVE (smallest positive normal)
        let v = NanBoxedValue::from_float(f64::MIN_POSITIVE);
        assert!(v.is_float());
        assert_eq!(v.as_float(), f64::MIN_POSITIVE);

        // f64::EPSILON
        let v = NanBoxedValue::from_float(f64::EPSILON);
        assert!(v.is_float());
        assert_eq!(v.as_float(), f64::EPSILON);
    }

    #[test]
    fn test_negative_infinity() {
        let v = NanBoxedValue::from_float(f64::NEG_INFINITY);
        assert!(v.is_float());
        assert!(!v.is_boxed());
        assert!(v.as_float().is_infinite());
        assert!(v.as_float().is_sign_negative());
    }

    #[test]
    fn test_large_negative_floats_not_boxed() {
        // Large negative floats should NOT be treated as boxed values
        // even though their bit patterns have the sign bit set
        let values = [-1.0e308, -1.0e100, -1.0e50, -1.0, -0.5, -f64::MIN_POSITIVE];

        for &f in &values {
            let v = NanBoxedValue::from_float(f);
            assert!(
                v.is_float(),
                "Float {} should be recognized as float, not boxed (bits: 0x{:016x})",
                f,
                f.to_bits()
            );
            assert_eq!(v.as_float(), f);
        }
    }

    #[test]
    fn test_no_float_boxed_collision() {
        // Verify that no valid float value >= NANBOX_THRESHOLD
        // The threshold is 0xFFFC_0000_0000_0000

        // Negative infinity: 0xFFF0_0000_0000_0000 - below threshold
        assert!(f64::NEG_INFINITY.to_bits() < NANBOX_THRESHOLD);

        // Largest negative normal: 0xFFEF_FFFF_FFFF_FFFF - below threshold
        assert!(f64::MIN.to_bits() < NANBOX_THRESHOLD);

        // -1.0: 0xBFF0_0000_0000_0000 - way below threshold
        assert!((-1.0_f64).to_bits() < NANBOX_THRESHOLD);

        // Negative quiet NaN starts at 0xFFF8... which is below our 0xFFFC threshold
        // so those are safe as floats
        let neg_qnan = f64::from_bits(0xFFF8_0000_0000_0000);
        assert!(neg_qnan.to_bits() < NANBOX_THRESHOLD);
    }

    #[test]
    fn test_all_tags_discriminate() {
        // Create one value of each boxed type and verify they're correctly discriminated
        let int = NanBoxedValue::from_int(0);
        let bool_v = NanBoxedValue::from_bool(false);

        // Verify each has correct tag
        assert_eq!(int.tag(), NanBoxTag::Int as u8);
        assert_eq!(bool_v.tag(), NanBoxTag::Bool as u8);

        // Verify they don't match other types
        assert!(int.is_int());
        assert!(!int.is_bool());
        assert!(!int.is_string());

        assert!(bool_v.is_bool());
        assert!(!bool_v.is_int());
        assert!(!bool_v.is_string());
    }

    #[test]
    fn test_encoding_bit_patterns() {
        // Verify specific bit patterns for debugging
        // New encoding: 0xFFFC in bits 63:48, tag in bits 47:44, payload in bits 43:0

        // Int 0: tag=0, payload=0
        let int_zero = NanBoxedValue::from_int(0);
        let bits = int_zero.to_bits();
        assert_eq!(
            bits >> 48,
            0xFFFC,
            "All boxed values have 0xFFFC in high 16 bits"
        );
        assert_eq!(int_zero.tag(), 0, "Int should have tag 0");
        assert_eq!(bits & PAYLOAD_MASK, 0, "Int(0) should have payload 0");

        // Bool true: tag=1, payload=1
        let bool_true = NanBoxedValue::from_bool(true);
        let bits = bool_true.to_bits();
        assert_eq!(
            bits >> 48,
            0xFFFC,
            "All boxed values have 0xFFFC in high 16 bits"
        );
        assert_eq!(bool_true.tag(), 1, "Bool should have tag 1");
        assert_eq!(bits & PAYLOAD_MASK, 1, "Bool(true) should have payload 1");

        // Int 42: tag=0, payload=42
        let int_42 = NanBoxedValue::from_int(42);
        let bits = int_42.to_bits();
        assert_eq!(bits >> 48, 0xFFFC);
        assert_eq!(int_42.tag(), 0);
        assert_eq!(bits & PAYLOAD_MASK, 42);

        // Verify tag is in correct position (bits 47:44)
        // Bool(true) should be: 0xFFFC_1000_0000_0001
        let expected_bool_bits = 0xFFFC_0000_0000_0000_u64 | (1_u64 << 44) | 1;
        assert_eq!(
            bool_true.to_bits(),
            expected_bool_bits,
            "Bool(true) bit pattern should be 0xFFFC_1000_0000_0001"
        );
    }

    #[test]
    fn test_negative_int_sign_extension() {
        // Verify negative integers are properly sign-extended when decoded

        let v = NanBoxedValue::from_int(-1);
        assert_eq!(v.as_int(), -1);

        let v = NanBoxedValue::from_int(-100);
        assert_eq!(v.as_int(), -100);

        let v = NanBoxedValue::from_int(-1000000);
        assert_eq!(v.as_int(), -1000000);

        // Test the boundary values
        let v = NanBoxedValue::from_int(MIN_NANBOX_INT);
        assert_eq!(v.as_int(), MIN_NANBOX_INT);
        assert!(v.as_int() < 0);
    }

    #[test]
    fn test_closure_and_weave_data_sizes() {
        // Verify our heap-allocated structs have expected sizes
        assert_eq!(std::mem::size_of::<QuotationData>(), 16); // 2 x usize
        // ClosureData has fn_ptr (usize) + Arc<[Value]> (2 x usize for fat pointer)
        assert!(std::mem::size_of::<ClosureData>() >= 24);
        // WeaveCtxData has 2 x Arc<WeaveChannelData>
        assert!(std::mem::size_of::<WeaveCtxData>() >= 16);
    }

    // =========================================================================
    // Value <-> NanBoxedValue Conversion Tests
    // =========================================================================

    #[test]
    fn test_value_int_roundtrip() {
        let original = Value::Int(42);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);

        // Negative
        let original = Value::Int(-12345);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_value_float_roundtrip() {
        let original = Value::Float(std::f64::consts::PI);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);

        // Negative
        let original = Value::Float(-std::f64::consts::E);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_value_bool_roundtrip() {
        let original = Value::Bool(true);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);

        let original = Value::Bool(false);
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_value_string_roundtrip() {
        let original = Value::String(SeqString::from("hello world"));
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_value_symbol_roundtrip() {
        let original = Value::Symbol(SeqString::from("my-symbol"));
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_value_quotation_roundtrip() {
        let original = Value::Quotation {
            wrapper: 0x1234,
            impl_: 0x5678,
        };
        let nb = NanBoxedValue::from_value(&original);
        let restored = unsafe { nb.to_value() };
        assert_eq!(restored, original);
    }

    #[test]
    fn test_clone_nanboxed_int() {
        let original = NanBoxedValue::from_int(42);
        let cloned = original.clone_nanboxed();
        assert_eq!(cloned.as_int(), 42);
        // Both should be valid (no double-free for primitives)
        assert_eq!(original.as_int(), 42);
    }

    #[test]
    fn test_clone_nanboxed_string() {
        let value = Value::String(SeqString::from("test"));
        let nb = NanBoxedValue::from_value(&value);
        let cloned = nb.clone_nanboxed();

        // Both should be valid and contain the same string
        let restored1 = unsafe { nb.to_value() };
        let restored2 = unsafe { cloned.to_value() };

        assert_eq!(restored1, value);
        assert_eq!(restored2, value);
    }

    #[test]
    fn test_drop_nanboxed_string() {
        let value = Value::String(SeqString::from("test"));
        let nb = NanBoxedValue::from_value(&value);
        // This should not leak memory
        unsafe { nb.drop_nanboxed() };
        // Test passes if no memory issues (detected by miri or valgrind)
    }

    #[test]
    fn test_drop_nanboxed_primitive() {
        let nb = NanBoxedValue::from_int(42);
        // Dropping a primitive should be a no-op
        unsafe { nb.drop_nanboxed() };
        // The value is still valid (Copy type)
        assert_eq!(nb.as_int(), 42);
    }
}
