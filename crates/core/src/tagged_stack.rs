//! Tagged Stack Implementation
//!
//! A contiguous array of 40-byte values for high-performance stack operations.
//! The 40-byte size matches Rust's `Value` enum with `#[repr(C)]` and the LLVM
//! `%Value = type { i64, i64, i64, i64, i64 }` layout, enabling interoperability
//! between inline IR and FFI operations.
//!
//! ## Stack Value Layout (40 bytes)
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  slot0 (8 bytes)  │  slot1  │  slot2  │  slot3  │  slot4  │
//! ├───────────────────┼─────────┼─────────┼─────────┼─────────┤
//! │   Discriminant    │ Payload │  data   │  data   │  data   │
//! └─────────────────────────────────────────────────────────────────────────────┘
//!
//! Value discriminants:
//! - 0 = Int:   slot1 contains i64 value
//! - 1 = Float: slot1 contains f64 bits
//! - 2 = Bool:  slot1 contains 0 or 1
//! - 3 = String, 4 = Variant, 5 = Map, 6 = Quotation, 7 = Closure
//! ```
//!
//! ## Stack Layout
//!
//! ```text
//! Stack: contiguous array of 40-byte StackValue slots
//! ┌──────────┬──────────┬──────────┬──────────┬─────────┐
//! │   v0     │   v1     │   v2     │   v3     │  ...    │
//! │ (40 B)   │ (40 B)   │ (40 B)   │ (40 B)   │         │
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
//! The 40-byte size enables:
//! - Direct compatibility with Rust's Value enum (#[repr(C)])
//! - No conversion overhead when calling runtime functions
//! - Simple inline integer/boolean operations in compiled code

use std::alloc::{Layout, alloc, dealloc, realloc};

// =============================================================================
// StackValue
// =============================================================================

/// A 40-byte stack value, layout-compatible with LLVM's %Value type.
///
/// This matches `%Value = type { i64, i64, i64, i64, i64 }` in the generated IR.
/// The size matches Rust's Value enum with #[repr(C)].
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StackValue {
    /// First slot: discriminant (0=Int, 1=Float, 2=Bool, 3=String, etc.)
    pub slot0: u64,
    /// Second slot: primary payload (i64 value for Int, bool for Bool, etc.)
    pub slot1: u64,
    /// Third slot: type-specific data
    pub slot2: u64,
    /// Fourth slot: type-specific data
    pub slot3: u64,
    /// Fifth slot: type-specific data (for largest variant)
    pub slot4: u64,
}

impl std::fmt::Debug for StackValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Discriminant 0 = Int
        if self.slot0 == 0 {
            write!(f, "Int({})", self.slot1 as i64)
        } else if self.slot0 == 2 {
            // Discriminant 2 = Bool
            write!(f, "Bool({})", self.slot1 != 0)
        } else {
            write!(
                f,
                "StackValue {{ slot0: 0x{:x}, slot1: 0x{:x}, slot2: 0x{:x}, slot3: 0x{:x}, slot4: 0x{:x} }}",
                self.slot0, self.slot1, self.slot2, self.slot3, self.slot4
            )
        }
    }
}

/// Size of StackValue in bytes (40 bytes = 5 x u64)
pub const STACK_VALUE_SIZE: usize = std::mem::size_of::<StackValue>();

// Compile-time assertion for StackValue size
const _: () = assert!(STACK_VALUE_SIZE == 40, "StackValue must be 40 bytes");

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

    /// Push an integer value using Value::Int layout
    /// slot0 = 0 (Int discriminant), slot1 = value
    #[inline]
    pub fn push_int(&mut self, val: i64) {
        self.push(StackValue {
            slot0: 0, // Int discriminant
            slot1: val as u64,
            slot2: 0,
            slot3: 0,
            slot4: 0,
        });
    }

    /// Pop and return an integer value
    ///
    /// Panics if the top value is not an integer.
    #[inline]
    pub fn pop_int(&mut self) -> i64 {
        let val = self.pop();
        assert!(
            val.slot0 == 0,
            "pop_int: expected Int (discriminant 0), got discriminant {}",
            val.slot0
        );
        val.slot1 as i64
    }

    /// Clone this stack (for spawn)
    ///
    /// Creates a deep copy. For heap objects, properly clones them using
    /// the clone_stack_value function which handles each type correctly.
    pub fn clone_stack(&self) -> Self {
        use crate::stack::clone_stack_value;

        // Allocate new stack array
        let layout = Layout::array::<StackValue>(self.capacity).expect("layout overflow");
        let new_base = unsafe { alloc(layout) as *mut StackValue };
        if new_base.is_null() {
            panic!("Failed to allocate cloned stack");
        }

        // Clone each value properly (handles heap types correctly)
        for i in 0..self.sp {
            unsafe {
                let sv = &*self.base.add(i);
                let cloned = clone_stack_value(sv);
                *new_base.add(i) = cloned;
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
        use crate::stack::drop_stack_value;

        // Drop all values properly (handles heap types correctly)
        for i in 0..self.sp {
            unsafe {
                let sv = *self.base.add(i);
                drop_stack_value(sv);
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
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

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

        // With Value layout: slot0 = discriminant (0 for Int), slot1 = value
        let peeked = stack.peek();
        assert_eq!(peeked.slot0, 0); // Int discriminant
        assert_eq!(peeked.slot1 as i64, 42); // Value in slot1
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
    fn test_stack_value_size() {
        // Verify StackValue is 40 bytes, matching LLVM's %Value type
        // (5 x i64 = 5 x 8 = 40 bytes, compatible with Rust's Value with #[repr(C)])
        assert_eq!(std::mem::size_of::<StackValue>(), 40);
        assert_eq!(STACK_VALUE_SIZE, 40);
    }
}
