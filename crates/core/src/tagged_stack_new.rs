//! Tagged Stack Implementation
//!
//! A contiguous array of 8-byte tagged values for high-performance stack operations.
//!
//! ## Tagged Value Encoding (8 bytes)
//!
//! ```text
//! - Odd (bit 0 = 1): Int — 63-bit signed integer
//!     tagged = (value << 1) | 1
//!     value  = tagged >> 1  (arithmetic shift)
//!
//! - 0x0: Bool false
//! - 0x2: Bool true
//!
//! - Even, > 2: Heap pointer to Box<Value>
//!     All heap pointers are 8-byte aligned (low 3 bits = 0)
//!     and always > 2, so no ambiguity with false/true.
//! ```
//!
//! ## Stack Layout
//!
//! ```text
//! Stack: contiguous array of 8-byte u64 slots
//! ┌──────┬──────┬──────┬──────┬─────┐
//! │  v0  │  v1  │  v2  │  v3  │ ... │
//! │(8 B) │(8 B) │(8 B) │(8 B) │     │
//! └──────┴──────┴──────┴──────┴─────┘
//!                                ↑ SP
//! ```

use std::alloc::{Layout, alloc, dealloc, realloc};

// =============================================================================
// StackValue
// =============================================================================

/// An 8-byte tagged stack value.
///
/// This is a u64 that encodes values using tagged pointers:
/// - Odd values are 63-bit signed integers
/// - 0 is false, 2 is true
/// - Other even values are pointers to heap-allocated Box<Value>
pub type StackValue = u64;

/// Size of StackValue in bytes (8 bytes = 1 x u64)
pub const STACK_VALUE_SIZE: usize = std::mem::size_of::<StackValue>();

// Compile-time assertion for StackValue size
const _: () = assert!(STACK_VALUE_SIZE == 8, "StackValue must be 8 bytes");

/// Tagged value for Bool false
pub const TAG_FALSE: u64 = 0;

/// Tagged value for Bool true
pub const TAG_TRUE: u64 = 2;

/// Check if a tagged value is an inline integer
#[inline(always)]
pub fn is_tagged_int(tagged: u64) -> bool {
    tagged & 1 != 0
}

/// Check if a tagged value is a heap pointer (not Int, not Bool)
#[inline(always)]
pub fn is_tagged_heap(tagged: u64) -> bool {
    tagged & 1 == 0 && tagged > TAG_TRUE
}

/// Encode an i64 as a tagged integer.
///
/// Valid range: -(2^62) to (2^62 - 1). Values outside this range will
/// silently overflow due to the left shift. This is acceptable because
/// Seq's integer range is documented as 63-bit signed.
#[inline(always)]
pub fn tag_int(value: i64) -> u64 {
    ((value as u64) << 1) | 1
}

/// Decode a tagged integer back to i64 (arithmetic shift)
#[inline(always)]
pub fn untag_int(tagged: u64) -> i64 {
    (tagged as i64) >> 1
}

/// Default stack capacity (number of stack values)
pub const DEFAULT_STACK_CAPACITY: usize = 4096;

/// Stack state for the tagged value stack
#[repr(C)]
pub struct TaggedStack {
    /// Pointer to the base of the stack array
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
    #[inline]
    pub fn pop(&mut self) -> StackValue {
        assert!(self.sp > 0, "pop: stack is empty");
        self.sp -= 1;
        unsafe { *self.base.add(self.sp) }
    }

    /// Peek at the top value without removing it
    #[inline]
    pub fn peek(&self) -> StackValue {
        assert!(self.sp > 0, "peek: stack is empty");
        unsafe { *self.base.add(self.sp - 1) }
    }

    /// Get a pointer to the current stack pointer position
    #[inline(always)]
    pub fn sp_ptr(&self) -> *mut StackValue {
        unsafe { self.base.add(self.sp) }
    }

    /// Push an integer value using tagged encoding
    #[inline]
    pub fn push_int(&mut self, val: i64) {
        self.push(tag_int(val));
    }

    /// Pop and return an integer value
    #[inline]
    pub fn pop_int(&mut self) -> i64 {
        let val = self.pop();
        assert!(
            is_tagged_int(val),
            "pop_int: expected tagged Int, got 0x{:x}",
            val
        );
        untag_int(val)
    }

    /// Clone this stack (for spawn)
    pub fn clone_stack(&self) -> Self {
        use crate::stack::clone_stack_value;

        let layout = Layout::array::<StackValue>(self.capacity).expect("layout overflow");
        let new_base = unsafe { alloc(layout) as *mut StackValue };
        if new_base.is_null() {
            panic!("Failed to allocate cloned stack");
        }

        for i in 0..self.sp {
            unsafe {
                let sv = *self.base.add(i);
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

        for i in 0..self.sp {
            unsafe {
                let sv = *self.base.add(i);
                drop_stack_value(sv);
            }
        }

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

#[unsafe(no_mangle)]
pub extern "C" fn seq_stack_new(capacity: usize) -> *mut TaggedStack {
    let stack = Box::new(TaggedStack::new(capacity));
    Box::into_raw(stack)
}

#[unsafe(no_mangle)]
pub extern "C" fn seq_stack_new_default() -> *mut TaggedStack {
    let stack = Box::new(TaggedStack::with_default_capacity());
    Box::into_raw(stack)
}

/// # Safety
/// Pointer must have been returned by `seq_stack_new`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_free(stack: *mut TaggedStack) {
    if !stack.is_null() {
        unsafe {
            drop(Box::from_raw(stack));
        }
    }
}

/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_grow(stack: *mut TaggedStack, min_capacity: usize) {
    assert!(!stack.is_null(), "seq_stack_grow: null stack");
    unsafe {
        (*stack).grow(min_capacity);
    }
}

/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_base(stack: *mut TaggedStack) -> *mut StackValue {
    assert!(!stack.is_null(), "seq_stack_base: null stack");
    unsafe { (*stack).base }
}

/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_sp(stack: *mut TaggedStack) -> usize {
    assert!(!stack.is_null(), "seq_stack_sp: null stack");
    unsafe { (*stack).sp }
}

/// # Safety
/// `stack` must be valid and new_sp must be <= capacity.
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

/// # Safety
/// `stack` must be a valid pointer to a TaggedStack.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_stack_capacity(stack: *mut TaggedStack) -> usize {
    assert!(!stack.is_null(), "seq_stack_capacity: null stack");
    unsafe { (*stack).capacity }
}

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

        stack.push_int(10);
        stack.push_int(20);
        stack.push_int(30);

        assert!(!stack.is_empty());
        assert_eq!(stack.depth(), 3);

        assert_eq!(stack.pop_int(), 30);
        assert_eq!(stack.pop_int(), 20);
        assert_eq!(stack.pop_int(), 10);

        assert!(stack.is_empty());
    }

    #[test]
    fn test_stack_peek() {
        let mut stack = TaggedStack::new(16);
        stack.push_int(42);

        let peeked = stack.peek();
        assert!(is_tagged_int(peeked));
        assert_eq!(untag_int(peeked), 42);
        assert_eq!(stack.depth(), 1);

        assert_eq!(stack.pop_int(), 42);
        assert!(stack.is_empty());
    }

    #[test]
    fn test_stack_grow() {
        let mut stack = TaggedStack::new(4);

        for i in 0..100 {
            stack.push_int(i);
        }

        assert_eq!(stack.depth(), 100);
        assert!(stack.capacity >= 100);

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

        assert_eq!(cloned.pop_int(), 3);
        assert_eq!(cloned.pop_int(), 2);
        assert_eq!(cloned.pop_int(), 1);

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
        assert_eq!(std::mem::size_of::<StackValue>(), 8);
        assert_eq!(STACK_VALUE_SIZE, 8);
    }

    #[test]
    fn test_tagged_int_encoding() {
        // Basic values
        assert_eq!(untag_int(tag_int(0)), 0);
        assert_eq!(untag_int(tag_int(1)), 1);
        assert_eq!(untag_int(tag_int(-1)), -1);
        assert_eq!(untag_int(tag_int(42)), 42);
        assert_eq!(untag_int(tag_int(-42)), -42);

        // 63-bit range limits
        let max_63 = (1i64 << 62) - 1; // 4611686018427387903
        let min_63 = -(1i64 << 62); // -4611686018427387904
        assert_eq!(untag_int(tag_int(max_63)), max_63);
        assert_eq!(untag_int(tag_int(min_63)), min_63);

        // All tagged ints are odd
        assert!(is_tagged_int(tag_int(0)));
        assert!(is_tagged_int(tag_int(42)));
        assert!(is_tagged_int(tag_int(-1)));
    }

    #[test]
    fn test_tagged_bool_encoding() {
        assert!(!is_tagged_int(TAG_FALSE));
        assert!(!is_tagged_int(TAG_TRUE));
        assert!(!is_tagged_heap(TAG_FALSE));
        assert!(!is_tagged_heap(TAG_TRUE));
    }
}
