//! Tagged Stack Implementation
//!
//! A contiguous array of 32-byte values for high-performance stack operations.
//! The 32-byte size matches the LLVM `%Value = type { i64, i64, i64, i64 }` layout,
//! enabling interoperability between inline IR and FFI operations.
//!
//! ## Stack Value Layout (32 bytes)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │  slot0 (8 bytes)  │  slot1  │  slot2  │  slot3  │
//! ├───────────────────┼─────────┼─────────┼─────────┤
//! │ Tag + inline data │  data   │  data   │  data   │
//! └─────────────────────────────────────────────────────────────────┘
//!
//! For integers (most common):
//! - slot0 = (value << 1) | 1  (tagged integer, low bit = 1)
//! - slot1, slot2, slot3 = unused (can be garbage)
//!
//! For other types (strings, floats, etc.):
//! - slot0 = type tag (low bit = 0 indicates non-integer)
//! - slot1-3 = type-specific data or pointer
//! ```
//!
//! ## Stack Layout
//!
//! ```text
//! Stack: contiguous array of 32-byte StackValue slots
//! ┌──────────┬──────────┬──────────┬──────────┬─────────┐
//! │   v0     │   v1     │   v2     │   v3     │  ...    │
//! │ (32 B)   │ (32 B)   │ (32 B)   │ (32 B)   │         │
//! └──────────┴──────────┴──────────┴──────────┴─────────┘
//!                                              ↑ SP
//!
//! - Grows upward
//! - SP points to next free slot
//! - Push: store at SP, increment SP
//! - Pop: decrement SP, load from SP
//! ```
//!
//! ## Performance Considerations
//!
//! The 32-byte size enables:
//! - Direct compatibility with existing FFI functions
//! - No conversion overhead when calling runtime functions
//! - Cache-line friendly (2 values per 64-byte cache line)
//!
//! For integer-heavy code, inline IR can use just slot0 and ignore the rest.

use std::alloc::{Layout, alloc, dealloc, realloc};
use std::ptr;

/// A 32-byte stack value, layout-compatible with LLVM's %Value type.
///
/// This matches `%Value = type { i64, i64, i64, i64 }` in the generated IR.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StackValue {
    /// First slot: for integers, contains (value << 1) | 1
    /// For other types, contains type tag (low bit = 0)
    pub slot0: u64,
    /// Second slot: type-specific data
    pub slot1: u64,
    /// Third slot: type-specific data
    pub slot2: u64,
    /// Fourth slot: type-specific data
    pub slot3: u64,
}

impl std::fmt::Debug for StackValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if is_tagged_int(self.slot0) {
            write!(f, "Int({})", untag_int(self.slot0))
        } else {
            write!(
                f,
                "StackValue {{ slot0: 0x{:x}, slot1: 0x{:x}, slot2: 0x{:x}, slot3: 0x{:x} }}",
                self.slot0, self.slot1, self.slot2, self.slot3
            )
        }
    }
}

/// Size of StackValue in bytes (should be 32)
pub const STACK_VALUE_SIZE: usize = std::mem::size_of::<StackValue>();

// Compile-time assertion that StackValue is 32 bytes
const _: () = assert!(STACK_VALUE_SIZE == 32, "StackValue must be 32 bytes");

/// Legacy type alias for backward compatibility
pub type TaggedValue = u64;

/// Tag bit mask
pub const TAG_MASK: u64 = 1;

/// Integer tag (low bit set)
pub const TAG_INT: u64 = 1;

/// Heap pointer tag (low bit clear)
pub const TAG_HEAP: u64 = 0;

/// Check if a tagged value is an integer
#[inline(always)]
pub const fn is_tagged_int(val: TaggedValue) -> bool {
    (val & TAG_MASK) == TAG_INT
}

/// Check if a tagged value is a heap pointer
#[inline(always)]
pub const fn is_tagged_heap(val: TaggedValue) -> bool {
    (val & TAG_MASK) == TAG_HEAP
}

/// Create a tagged integer from an i64
///
/// Note: Only 63 bits of the integer are preserved.
/// Range: -4,611,686,018,427,387,904 to 4,611,686,018,427,387,903
#[inline(always)]
pub const fn tag_int(val: i64) -> TaggedValue {
    // Arithmetic left shift preserves sign, then set tag bit
    ((val << 1) as u64) | TAG_INT
}

/// Extract an i64 from a tagged integer
///
/// Caller must ensure `is_tagged_int(val)` is true.
#[inline(always)]
pub const fn untag_int(val: TaggedValue) -> i64 {
    // Arithmetic right shift to restore sign
    (val as i64) >> 1
}

/// Create a tagged heap pointer from a raw pointer
///
/// The pointer must be 8-byte aligned (low 3 bits clear).
#[inline(always)]
pub fn tag_heap_ptr(ptr: *mut HeapObject) -> TaggedValue {
    debug_assert!(
        (ptr as usize) & 0x7 == 0,
        "HeapObject pointer must be 8-byte aligned"
    );
    ptr as TaggedValue
}

/// Extract a heap pointer from a tagged value
///
/// Caller must ensure `is_tagged_heap(val)` is true.
#[inline(always)]
pub fn untag_heap_ptr(val: TaggedValue) -> *mut HeapObject {
    val as *mut HeapObject
}

/// Heap object type tags
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapTag {
    Float = 1,
    Bool = 2,
    String = 3,
    Variant = 4,
    Map = 5,
    Quotation = 6,
    Closure = 7,
}

/// Heap object header (8 bytes)
///
/// All heap-allocated values share this header for uniform access.
#[repr(C)]
pub struct HeapObject {
    /// Type tag identifying the payload type
    pub tag: u8,
    /// Flags (e.g., is_static for non-refcounted values)
    pub flags: u8,
    /// Reserved for future use
    pub reserved: u16,
    /// Reference count (atomic for thread safety)
    pub refcount: u32,
    // Payload follows (variable size based on tag)
}

/// Flags for HeapObject
pub mod heap_flags {
    /// Object is statically allocated, don't decrement refcount
    pub const IS_STATIC: u8 = 0x01;
}

impl HeapObject {
    /// Check if this object is statically allocated
    #[inline(always)]
    pub fn is_static(&self) -> bool {
        self.flags & heap_flags::IS_STATIC != 0
    }
}

/// Float heap object
#[repr(C)]
pub struct FloatObject {
    pub header: HeapObject,
    pub value: f64,
}

/// Bool heap object
#[repr(C)]
pub struct BoolObject {
    pub header: HeapObject,
    pub value: bool,
}

/// Quotation heap object (stateless function)
#[repr(C)]
pub struct QuotationObject {
    pub header: HeapObject,
    /// C-convention wrapper function pointer
    pub wrapper: usize,
    /// tailcc implementation function pointer
    pub impl_ptr: usize,
}

/// Default stack capacity (number of stack values)
pub const DEFAULT_STACK_CAPACITY: usize = 4096;

/// Stack state for the tagged value stack
///
/// This struct is passed to/from runtime functions and represents
/// the complete state of a strand's value stack.
///
/// Uses 32-byte StackValue slots for FFI compatibility.
#[repr(C)]
pub struct TaggedStack {
    /// Pointer to the base of the stack array (array of StackValue)
    pub base: *mut StackValue,
    /// Current stack pointer (index into array, points to next free slot)
    pub sp: usize,
    /// Total capacity of the stack (number of slots)
    pub capacity: usize,
}

impl TaggedStack {
    /// Create a new tagged stack with the given capacity
    pub fn new(capacity: usize) -> Self {
        let layout = Layout::array::<StackValue>(capacity).expect("stack layout overflow");
        let base = unsafe { alloc(layout) as *mut StackValue };
        if base.is_null() {
            panic!("Failed to allocate tagged stack");
        }

        TaggedStack {
            base,
            sp: 0,
            capacity,
        }
    }

    /// Create a new tagged stack with default capacity
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_STACK_CAPACITY)
    }

    /// Get the current stack depth
    #[inline(always)]
    pub fn depth(&self) -> usize {
        self.sp
    }

    /// Check if the stack is empty
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.sp == 0
    }

    /// Check if the stack has room for `n` more values
    #[inline(always)]
    pub fn has_capacity(&self, n: usize) -> bool {
        self.sp + n <= self.capacity
    }

    /// Grow the stack to accommodate more values
    ///
    /// Doubles capacity by default, or grows to `min_capacity` if larger.
    pub fn grow(&mut self, min_capacity: usize) {
        let new_capacity = (self.capacity * 2).max(min_capacity);
        let old_layout = Layout::array::<StackValue>(self.capacity).expect("old layout overflow");
        let new_layout = Layout::array::<StackValue>(new_capacity).expect("new layout overflow");

        let new_base = unsafe {
            realloc(self.base as *mut u8, old_layout, new_layout.size()) as *mut StackValue
        };

        if new_base.is_null() {
            panic!(
                "Failed to grow tagged stack from {} to {}",
                self.capacity, new_capacity
            );
        }

        self.base = new_base;
        self.capacity = new_capacity;
    }

    /// Push a StackValue onto the stack
    ///
    /// Grows the stack if necessary.
    #[inline]
    pub fn push(&mut self, val: StackValue) {
        if self.sp >= self.capacity {
            self.grow(self.capacity + 1);
        }
        unsafe {
            *self.base.add(self.sp) = val;
        }
        self.sp += 1;
    }

    /// Pop a StackValue from the stack
    ///
    /// Panics if the stack is empty.
    #[inline]
    pub fn pop(&mut self) -> StackValue {
        assert!(self.sp > 0, "pop: stack is empty");
        self.sp -= 1;
        unsafe { *self.base.add(self.sp) }
    }

    /// Peek at the top value without removing it
    ///
    /// Panics if the stack is empty.
    #[inline]
    pub fn peek(&self) -> StackValue {
        assert!(self.sp > 0, "peek: stack is empty");
        unsafe { *self.base.add(self.sp - 1) }
    }

    /// Get a pointer to the current stack pointer position
    ///
    /// This is used by generated code for inline stack operations.
    /// Returns pointer to next free StackValue slot.
    #[inline(always)]
    pub fn sp_ptr(&self) -> *mut StackValue {
        unsafe { self.base.add(self.sp) }
    }

    /// Push an integer value
    #[inline]
    pub fn push_int(&mut self, val: i64) {
        self.push(StackValue {
            slot0: tag_int(val),
            slot1: 0,
            slot2: 0,
            slot3: 0,
        });
    }

    /// Pop and return an integer value
    ///
    /// Panics if the top value is not an integer.
    #[inline]
    pub fn pop_int(&mut self) -> i64 {
        let val = self.pop();
        assert!(
            is_tagged_int(val.slot0),
            "pop_int: expected integer, got heap object"
        );
        untag_int(val.slot0)
    }

    /// Clone this stack (for spawn)
    ///
    /// Creates a deep copy. For heap objects, increments reference counts.
    pub fn clone_stack(&self) -> Self {
        // Allocate new stack array directly
        let layout = Layout::array::<StackValue>(self.capacity).expect("layout overflow");
        let new_base = unsafe { alloc(layout) as *mut StackValue };
        if new_base.is_null() {
            panic!("Failed to allocate cloned stack");
        }

        // Copy all values
        unsafe {
            ptr::copy_nonoverlapping(self.base, new_base, self.sp);
        }

        // Increment refcounts for heap objects
        for i in 0..self.sp {
            let val = unsafe { (*self.base.add(i)).slot0 };
            if is_tagged_heap(val) && val != 0 {
                let obj = untag_heap_ptr(val);
                unsafe {
                    if !(*obj).is_static() {
                        // Atomic increment
                        let rc = &(*obj).refcount as *const u32 as *mut u32;
                        std::sync::atomic::AtomicU32::from_ptr(rc)
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
        }

        TaggedStack {
            base: new_base,
            sp: self.sp,
            capacity: self.capacity,
        }
    }
}

impl Drop for TaggedStack {
    fn drop(&mut self) {
        // Decrement refcounts for all heap objects
        for i in 0..self.sp {
            let val = unsafe { (*self.base.add(i)).slot0 };
            if is_tagged_heap(val) && val != 0 {
                let obj = untag_heap_ptr(val);
                unsafe {
                    if !(*obj).is_static() {
                        // Atomic decrement
                        let rc = &(*obj).refcount as *const u32 as *mut u32;
                        let old = std::sync::atomic::AtomicU32::from_ptr(rc)
                            .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                        if old == 1 {
                            // Last reference, free the object
                            seq_free_heap_object(obj);
                        }
                    }
                }
            }
        }

        // Free the stack array
        if !self.base.is_null() {
            let layout = Layout::array::<StackValue>(self.capacity).expect("layout overflow");
            unsafe {
                dealloc(self.base as *mut u8, layout);
            }
        }
    }
}

// =============================================================================
// FFI Functions for LLVM Codegen
// =============================================================================

/// Allocate a new tagged stack
///
/// Returns a pointer to a heap-allocated TaggedStack.
/// The caller owns this memory and must call `seq_stack_free` when done.
#[unsafe(no_mangle)]
pub extern "C" fn seq_stack_new(capacity: usize) -> *mut TaggedStack {
    let stack = Box::new(TaggedStack::new(capacity));
    Box::into_raw(stack)
}

/// Allocate a new tagged stack with default capacity
#[unsafe(no_mangle)]
pub extern "C" fn seq_stack_new_default() -> *mut TaggedStack {
    let stack = Box::new(TaggedStack::with_default_capacity());
    Box::into_raw(stack)
}

/// Free a tagged stack
///
/// # Safety
/// The pointer must have been returned by `seq_stack_new` or `seq_stack_new_default`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_free(stack: *mut TaggedStack) {
    if !stack.is_null() {
        unsafe {
            drop(Box::from_raw(stack));
        }
    }
}

/// Grow a tagged stack to at least the given capacity
///
/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_grow(stack: *mut TaggedStack, min_capacity: usize) {
    assert!(!stack.is_null(), "seq_stack_grow: null stack");
    unsafe {
        (*stack).grow(min_capacity);
    }
}

/// Get the base pointer of a tagged stack
///
/// This is used by generated code to get the array base.
/// Returns a pointer to the first StackValue slot (32 bytes each).
///
/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_base(stack: *mut TaggedStack) -> *mut StackValue {
    assert!(!stack.is_null(), "seq_stack_base: null stack");
    unsafe { (*stack).base }
}

/// Get the current stack pointer (as index)
///
/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_sp(stack: *mut TaggedStack) -> usize {
    assert!(!stack.is_null(), "seq_stack_sp: null stack");
    unsafe { (*stack).sp }
}

/// Set the current stack pointer (as index)
///
/// # Safety
/// The new SP must be <= capacity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_set_sp(stack: *mut TaggedStack, new_sp: usize) {
    assert!(!stack.is_null(), "seq_stack_set_sp: null stack");
    unsafe {
        assert!(
            new_sp <= (*stack).capacity,
            "seq_stack_set_sp: sp exceeds capacity"
        );
        (*stack).sp = new_sp;
    }
}

/// Get the capacity of a tagged stack
///
/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_capacity(stack: *mut TaggedStack) -> usize {
    assert!(!stack.is_null(), "seq_stack_capacity: null stack");
    unsafe { (*stack).capacity }
}

/// Clone a tagged stack (for spawn)
///
/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_clone(stack: *mut TaggedStack) -> *mut TaggedStack {
    assert!(!stack.is_null(), "seq_stack_clone: null stack");
    let cloned = unsafe { (*stack).clone_stack() };
    Box::into_raw(Box::new(cloned))
}

// =============================================================================
// Heap Object Allocation Functions
// =============================================================================

/// Allocate a float heap object
#[unsafe(no_mangle)]
pub extern "C" fn seq_alloc_float(value: f64) -> *mut HeapObject {
    let layout = Layout::new::<FloatObject>();
    let ptr = unsafe { alloc(layout) as *mut FloatObject };
    if ptr.is_null() {
        panic!("Failed to allocate FloatObject");
    }

    unsafe {
        (*ptr).header = HeapObject {
            tag: HeapTag::Float as u8,
            flags: 0,
            reserved: 0,
            refcount: 1,
        };
        (*ptr).value = value;
    }

    ptr as *mut HeapObject
}

/// Allocate a bool heap object
#[unsafe(no_mangle)]
pub extern "C" fn seq_alloc_bool(value: bool) -> *mut HeapObject {
    let layout = Layout::new::<BoolObject>();
    let ptr = unsafe { alloc(layout) as *mut BoolObject };
    if ptr.is_null() {
        panic!("Failed to allocate BoolObject");
    }

    unsafe {
        (*ptr).header = HeapObject {
            tag: HeapTag::Bool as u8,
            flags: 0,
            reserved: 0,
            refcount: 1,
        };
        (*ptr).value = value;
    }

    ptr as *mut HeapObject
}

/// Allocate a quotation heap object
#[unsafe(no_mangle)]
pub extern "C" fn seq_alloc_quotation(wrapper: usize, impl_ptr: usize) -> *mut HeapObject {
    let layout = Layout::new::<QuotationObject>();
    let ptr = unsafe { alloc(layout) as *mut QuotationObject };
    if ptr.is_null() {
        panic!("Failed to allocate QuotationObject");
    }

    unsafe {
        (*ptr).header = HeapObject {
            tag: HeapTag::Quotation as u8,
            flags: 0,
            reserved: 0,
            refcount: 1,
        };
        (*ptr).wrapper = wrapper;
        (*ptr).impl_ptr = impl_ptr;
    }

    ptr as *mut HeapObject
}

/// Free a heap object based on its type tag
///
/// # Safety
/// `obj` must be a valid pointer to a HeapObject that was allocated by seq_alloc_*.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_free_heap_object(obj: *mut HeapObject) {
    if obj.is_null() {
        return;
    }

    unsafe {
        let tag = (*obj).tag;
        match tag {
            t if t == HeapTag::Float as u8 => {
                let layout = Layout::new::<FloatObject>();
                dealloc(obj as *mut u8, layout);
            }
            t if t == HeapTag::Bool as u8 => {
                let layout = Layout::new::<BoolObject>();
                dealloc(obj as *mut u8, layout);
            }
            t if t == HeapTag::Quotation as u8 => {
                let layout = Layout::new::<QuotationObject>();
                dealloc(obj as *mut u8, layout);
            }
            // TODO: Add other types as needed
            _ => {
                // Unknown type, use minimum HeapObject size
                let layout = Layout::new::<HeapObject>();
                dealloc(obj as *mut u8, layout);
            }
        }
    }
}

/// Increment the reference count of a heap object
///
/// # Safety
/// `obj` must be a valid pointer to a HeapObject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_heap_incref(obj: *mut HeapObject) {
    if obj.is_null() {
        return;
    }
    unsafe {
        if !(*obj).is_static() {
            let rc = &(*obj).refcount as *const u32 as *mut u32;
            std::sync::atomic::AtomicU32::from_ptr(rc)
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
}

/// Decrement the reference count of a heap object, freeing if zero
///
/// # Safety
/// `obj` must be a valid pointer to a HeapObject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_heap_decref(obj: *mut HeapObject) {
    if obj.is_null() {
        return;
    }
    unsafe {
        if !(*obj).is_static() {
            let rc = &(*obj).refcount as *const u32 as *mut u32;
            let old = std::sync::atomic::AtomicU32::from_ptr(rc)
                .fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            if old == 1 {
                seq_free_heap_object(obj);
            }
        }
    }
}

/// Get the float value from a FloatObject
///
/// # Safety
/// `obj` must be a valid pointer to a FloatObject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_get_float(obj: *mut HeapObject) -> f64 {
    assert!(!obj.is_null(), "seq_get_float: null object");
    unsafe {
        assert!(
            (*obj).tag == HeapTag::Float as u8,
            "seq_get_float: not a float"
        );
        (*(obj as *mut FloatObject)).value
    }
}

/// Get the bool value from a BoolObject
///
/// # Safety
/// `obj` must be a valid pointer to a BoolObject.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_get_bool(obj: *mut HeapObject) -> bool {
    assert!(!obj.is_null(), "seq_get_bool: null object");
    unsafe {
        assert!(
            (*obj).tag == HeapTag::Bool as u8,
            "seq_get_bool: not a bool"
        );
        (*(obj as *mut BoolObject)).value
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_untag_int() {
        // Positive integers
        assert_eq!(untag_int(tag_int(0)), 0);
        assert_eq!(untag_int(tag_int(1)), 1);
        assert_eq!(untag_int(tag_int(42)), 42);
        assert_eq!(untag_int(tag_int(1000000)), 1000000);

        // Negative integers
        assert_eq!(untag_int(tag_int(-1)), -1);
        assert_eq!(untag_int(tag_int(-42)), -42);
        assert_eq!(untag_int(tag_int(-1000000)), -1000000);

        // Edge cases (63-bit range)
        let max_63bit = (1i64 << 62) - 1;
        let min_63bit = -(1i64 << 62);
        assert_eq!(untag_int(tag_int(max_63bit)), max_63bit);
        assert_eq!(untag_int(tag_int(min_63bit)), min_63bit);
    }

    #[test]
    fn test_is_tagged() {
        // Integers have low bit set
        assert!(is_tagged_int(tag_int(0)));
        assert!(is_tagged_int(tag_int(42)));
        assert!(is_tagged_int(tag_int(-1)));

        // Pointers have low bit clear (use a fake aligned address)
        let fake_ptr = 0x1000u64; // 8-byte aligned
        assert!(is_tagged_heap(fake_ptr));
        assert!(!is_tagged_int(fake_ptr));
    }

    #[test]
    fn test_tagged_int_examples() {
        // From design doc: Integer `42` → `0x55` (42 << 1 | 1 = 85)
        assert_eq!(tag_int(42), 85);
        assert_eq!(tag_int(42), 0x55);

        // Integer `-1` → `0xFFFFFFFFFFFFFFFF` (-1 << 1 | 1)
        assert_eq!(tag_int(-1), 0xFFFFFFFFFFFFFFFFu64);
    }

    #[test]
    fn test_stack_basic_operations() {
        let mut stack = TaggedStack::new(16);

        assert!(stack.is_empty());
        assert_eq!(stack.depth(), 0);

        // Push integers
        stack.push_int(10);
        stack.push_int(20);
        stack.push_int(30);

        assert!(!stack.is_empty());
        assert_eq!(stack.depth(), 3);

        // Pop and verify
        assert_eq!(stack.pop_int(), 30);
        assert_eq!(stack.pop_int(), 20);
        assert_eq!(stack.pop_int(), 10);

        assert!(stack.is_empty());
    }

    #[test]
    fn test_stack_peek() {
        let mut stack = TaggedStack::new(16);
        stack.push_int(42);

        assert_eq!(untag_int(stack.peek().slot0), 42);
        assert_eq!(stack.depth(), 1); // Still there

        assert_eq!(stack.pop_int(), 42);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_stack_grow() {
        let mut stack = TaggedStack::new(4);

        // Fill beyond initial capacity
        for i in 0..100 {
            stack.push_int(i);
        }

        assert_eq!(stack.depth(), 100);
        assert!(stack.capacity >= 100);

        // Verify all values
        for i in (0..100).rev() {
            assert_eq!(stack.pop_int(), i);
        }
    }

    #[test]
    fn test_stack_clone() {
        let mut stack = TaggedStack::new(16);
        stack.push_int(1);
        stack.push_int(2);
        stack.push_int(3);

        let mut cloned = stack.clone_stack();

        // Both should have same values
        assert_eq!(cloned.pop_int(), 3);
        assert_eq!(cloned.pop_int(), 2);
        assert_eq!(cloned.pop_int(), 1);

        // Original should be unchanged
        assert_eq!(stack.pop_int(), 3);
        assert_eq!(stack.pop_int(), 2);
        assert_eq!(stack.pop_int(), 1);
    }

    #[test]
    fn test_ffi_stack_new_free() {
        let stack = seq_stack_new(64);
        assert!(!stack.is_null());

        unsafe {
            assert_eq!(seq_stack_capacity(stack), 64);
            assert_eq!(seq_stack_sp(stack), 0);

            seq_stack_free(stack);
        }
    }

    #[test]
    fn test_float_object() {
        let obj = seq_alloc_float(2.5);
        assert!(!obj.is_null());

        unsafe {
            assert_eq!((*obj).tag, HeapTag::Float as u8);
            assert_eq!((*obj).refcount, 1);
            assert_eq!(seq_get_float(obj), 2.5);

            // Verify it's 8-byte aligned
            assert!((obj as usize) & 0x7 == 0);

            seq_free_heap_object(obj);
        }
    }

    #[test]
    fn test_bool_object() {
        let obj_true = seq_alloc_bool(true);
        let obj_false = seq_alloc_bool(false);

        unsafe {
            assert!(seq_get_bool(obj_true));
            assert!(!seq_get_bool(obj_false));

            seq_free_heap_object(obj_true);
            seq_free_heap_object(obj_false);
        }
    }

    #[test]
    fn test_refcount() {
        let obj = seq_alloc_float(1.0);

        unsafe {
            assert_eq!((*obj).refcount, 1);

            seq_heap_incref(obj);
            assert_eq!((*obj).refcount, 2);

            seq_heap_incref(obj);
            assert_eq!((*obj).refcount, 3);

            seq_heap_decref(obj);
            assert_eq!((*obj).refcount, 2);

            seq_heap_decref(obj);
            assert_eq!((*obj).refcount, 1);

            // Final decref should free
            seq_heap_decref(obj);
            // Can't check after free, but shouldn't crash
        }
    }

    #[test]
    fn test_stack_with_heap_objects() {
        let mut stack = TaggedStack::new(16);

        // Push an integer
        stack.push_int(42);

        // Push a float (heap object)
        let float_obj = seq_alloc_float(2.5);
        stack.push(StackValue {
            slot0: tag_heap_ptr(float_obj),
            slot1: 0,
            slot2: 0,
            slot3: 0,
        });

        // Push another integer
        stack.push_int(100);

        assert_eq!(stack.depth(), 3);

        // Pop and verify
        assert_eq!(stack.pop_int(), 100);

        let val = stack.pop();
        assert!(is_tagged_heap(val.slot0));
        unsafe {
            assert_eq!(seq_get_float(untag_heap_ptr(val.slot0)), 2.5);
        }

        assert_eq!(stack.pop_int(), 42);

        // Note: float_obj refcount was 1, we popped it without decref,
        // so it's still alive. In real code, drop would decref.
        unsafe {
            seq_free_heap_object(float_obj);
        }
    }

    #[test]
    fn test_stack_value_size() {
        // Verify StackValue is 32 bytes, matching LLVM's %Value type
        assert_eq!(std::mem::size_of::<StackValue>(), 32);
        assert_eq!(STACK_VALUE_SIZE, 32);
    }
}
