//! Tagged Stack Implementation
//!
//! This module implements the stack using a contiguous array of 40-byte StackValue entries.
//! Each StackValue has the layout: { slot0: discriminant, slot1-4: payload }
//!
//! The Stack type is a pointer to the "current position" (where next push goes).
//! - Push: store at *sp, return sp + 1
//! - Pop: return sp - 1, read from *(sp - 1)

use crate::tagged_stack::StackValue;
use crate::value::Value;
use std::sync::Arc;

/// Stack: A pointer to the current position in a contiguous array of StackValue.
///
/// Points to where the next value would be pushed.
/// sp - 1 points to the top value, sp - 2 to second, etc.
pub type Stack = *mut StackValue;

/// Discriminant values matching codegen
pub const DISC_INT: u64 = 0;
pub const DISC_FLOAT: u64 = 1;
pub const DISC_BOOL: u64 = 2;
pub const DISC_STRING: u64 = 3;
pub const DISC_VARIANT: u64 = 4;
pub const DISC_MAP: u64 = 5;
pub const DISC_QUOTATION: u64 = 6;
pub const DISC_CLOSURE: u64 = 7;
pub const DISC_CHANNEL: u64 = 8;
pub const DISC_WEAVECTX: u64 = 9;
pub const DISC_SYMBOL: u64 = 10;

/// Convert a Value to a StackValue for pushing onto the tagged stack
#[inline]
pub fn value_to_stack_value(value: Value) -> StackValue {
    match value {
        Value::Int(i) => StackValue {
            slot0: DISC_INT,
            slot1: i as u64,
            slot2: 0,
            slot3: 0,
            slot4: 0,
        },
        Value::Float(f) => StackValue {
            slot0: DISC_FLOAT,
            slot1: f.to_bits(),
            slot2: 0,
            slot3: 0,
            slot4: 0,
        },
        Value::Bool(b) => StackValue {
            slot0: DISC_BOOL,
            slot1: if b { 1 } else { 0 },
            slot2: 0,
            slot3: 0,
            slot4: 0,
        },
        Value::String(s) => {
            // SeqString has: ptr, len, capacity, global
            // Store these in slots 1-4
            let (ptr, len, capacity, global) = s.into_raw_parts();
            StackValue {
                slot0: DISC_STRING,
                slot1: ptr as u64,
                slot2: len as u64,
                slot3: capacity as u64,
                slot4: if global { 1 } else { 0 },
            }
        }
        Value::Symbol(s) => {
            // Symbol uses the same SeqString representation as String
            let (ptr, len, capacity, global) = s.into_raw_parts();
            StackValue {
                slot0: DISC_SYMBOL,
                slot1: ptr as u64,
                slot2: len as u64,
                slot3: capacity as u64,
                slot4: if global { 1 } else { 0 },
            }
        }
        Value::Variant(v) => {
            let ptr = Arc::into_raw(v) as u64;
            StackValue {
                slot0: DISC_VARIANT,
                slot1: ptr,
                slot2: 0,
                slot3: 0,
                slot4: 0,
            }
        }
        Value::Map(m) => {
            let ptr = Box::into_raw(m) as u64;
            StackValue {
                slot0: DISC_MAP,
                slot1: ptr,
                slot2: 0,
                slot3: 0,
                slot4: 0,
            }
        }
        Value::Quotation { wrapper, impl_ } => StackValue {
            slot0: DISC_QUOTATION,
            slot1: wrapper as u64,
            slot2: impl_ as u64,
            slot3: 0,
            slot4: 0,
        },
        Value::Closure { fn_ptr, env } => {
            // Arc<[Value]> is a fat pointer - use Box to store it
            let env_box = Box::new(env);
            let env_ptr = Box::into_raw(env_box) as u64;
            StackValue {
                slot0: DISC_CLOSURE,
                slot1: fn_ptr as u64,
                slot2: env_ptr,
                slot3: 0,
                slot4: 0,
            }
        }
        Value::Channel(ch) => {
            // Store Arc<ChannelData> as raw pointer
            let ptr = Arc::into_raw(ch) as u64;
            StackValue {
                slot0: DISC_CHANNEL,
                slot1: ptr,
                slot2: 0,
                slot3: 0,
                slot4: 0,
            }
        }
        Value::WeaveCtx {
            yield_chan,
            resume_chan,
        } => {
            // Store both Arc<WeaveChannelData> as raw pointers
            let yield_ptr = Arc::into_raw(yield_chan) as u64;
            let resume_ptr = Arc::into_raw(resume_chan) as u64;
            StackValue {
                slot0: DISC_WEAVECTX,
                slot1: yield_ptr,
                slot2: resume_ptr,
                slot3: 0,
                slot4: 0,
            }
        }
    }
}

/// Convert a StackValue back to a Value
///
/// # Safety
/// The StackValue must contain valid data for its discriminant
#[inline]
pub unsafe fn stack_value_to_value(sv: StackValue) -> Value {
    unsafe {
        match sv.slot0 {
            DISC_INT => Value::Int(sv.slot1 as i64),
            DISC_FLOAT => Value::Float(f64::from_bits(sv.slot1)),
            DISC_BOOL => Value::Bool(sv.slot1 != 0),
            DISC_STRING => {
                use crate::seqstring::SeqString;
                let ptr = sv.slot1 as *const u8;
                let len = sv.slot2 as usize;
                let capacity = sv.slot3 as usize;
                let global = sv.slot4 != 0;
                Value::String(SeqString::from_raw_parts(ptr, len, capacity, global))
            }
            DISC_VARIANT => {
                use crate::value::VariantData;
                let arc = Arc::from_raw(sv.slot1 as *const VariantData);
                Value::Variant(arc)
            }
            DISC_MAP => {
                use crate::value::MapKey;
                use std::collections::HashMap;
                let boxed = Box::from_raw(sv.slot1 as *mut HashMap<MapKey, Value>);
                Value::Map(boxed)
            }
            DISC_QUOTATION => Value::Quotation {
                wrapper: sv.slot1 as usize,
                impl_: sv.slot2 as usize,
            },
            DISC_CLOSURE => {
                // Unbox the Arc<[Value]> that we boxed in value_to_stack_value
                let env_box = Box::from_raw(sv.slot2 as *mut Arc<[Value]>);
                Value::Closure {
                    fn_ptr: sv.slot1 as usize,
                    env: *env_box,
                }
            }
            DISC_CHANNEL => {
                use crate::value::ChannelData;
                let arc = Arc::from_raw(sv.slot1 as *const ChannelData);
                Value::Channel(arc)
            }
            DISC_WEAVECTX => {
                use crate::value::WeaveChannelData;
                let yield_chan = Arc::from_raw(sv.slot1 as *const WeaveChannelData);
                let resume_chan = Arc::from_raw(sv.slot2 as *const WeaveChannelData);
                Value::WeaveCtx {
                    yield_chan,
                    resume_chan,
                }
            }
            DISC_SYMBOL => {
                use crate::seqstring::SeqString;
                let ptr = sv.slot1 as *const u8;
                let len = sv.slot2 as usize;
                let capacity = sv.slot3 as usize;
                let global = sv.slot4 != 0;
                Value::Symbol(SeqString::from_raw_parts(ptr, len, capacity, global))
            }
            _ => panic!("Invalid discriminant: {}", sv.slot0),
        }
    }
}

/// Clone a StackValue from LLVM IR - reads from src pointer, writes to dst pointer.
/// This is the FFI-callable version for inline codegen that avoids ABI issues
/// with passing large structs by value.
///
/// # Safety
/// Both src and dst pointers must be valid and properly aligned StackValue pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_clone_value(src: *const StackValue, dst: *mut StackValue) {
    unsafe {
        let sv = &*src;
        let cloned = clone_stack_value(sv);
        *dst = cloned;
    }
}

/// Clone a StackValue, handling reference counting for heap types.
///
/// # Cloning Strategy by Type
/// - **Int, Float, Bool, Quotation**: Bitwise copy (no heap allocation)
/// - **String**: Deep copy to a new global (refcounted) string. This is necessary
///   because the source may be an arena-allocated string that would become invalid
///   when the arena resets. Global strings are heap-allocated with Arc refcounting.
/// - **Variant**: Arc refcount increment (O(1), shares underlying data)
/// - **Map**: Deep clone of the HashMap and all contained values
/// - **Closure**: Deep clone of the Arc<[Value]> environment
/// - **Channel**: Arc refcount increment (O(1), shares underlying sender/receiver)
///
/// # Safety
/// The StackValue must contain valid data for its discriminant.
#[inline]
pub unsafe fn clone_stack_value(sv: &StackValue) -> StackValue {
    unsafe {
        match sv.slot0 {
            DISC_INT | DISC_FLOAT | DISC_BOOL | DISC_QUOTATION => *sv,
            DISC_STRING => {
                // Deep copy: arena strings may become invalid, so always create a global string
                let ptr = sv.slot1 as *const u8;
                let len = sv.slot2 as usize;
                debug_assert!(!ptr.is_null(), "String pointer is null");
                // Read the string content without taking ownership
                let slice = std::slice::from_raw_parts(ptr, len);
                // Validate UTF-8 in debug builds, skip in release for performance
                #[cfg(debug_assertions)]
                let s = std::str::from_utf8(slice).expect("Invalid UTF-8 in string clone");
                #[cfg(not(debug_assertions))]
                let s = std::str::from_utf8_unchecked(slice);
                // Clone to a new global string
                let cloned = crate::seqstring::global_string(s.to_string());
                let (new_ptr, new_len, new_cap, new_global) = cloned.into_raw_parts();
                StackValue {
                    slot0: DISC_STRING,
                    slot1: new_ptr as u64,
                    slot2: new_len as u64,
                    slot3: new_cap as u64,
                    slot4: if new_global { 1 } else { 0 },
                }
            }
            DISC_VARIANT => {
                use crate::value::VariantData;
                let ptr = sv.slot1 as *const VariantData;
                debug_assert!(!ptr.is_null(), "Variant pointer is null");
                debug_assert!(
                    (ptr as usize).is_multiple_of(std::mem::align_of::<VariantData>()),
                    "Variant pointer is misaligned"
                );
                let arc = Arc::from_raw(ptr);
                let cloned = Arc::clone(&arc);
                std::mem::forget(arc);
                StackValue {
                    slot0: DISC_VARIANT,
                    slot1: Arc::into_raw(cloned) as u64,
                    slot2: 0,
                    slot3: 0,
                    slot4: 0,
                }
            }
            DISC_MAP => {
                // Deep clone the map
                use crate::value::MapKey;
                use std::collections::HashMap;
                let ptr = sv.slot1 as *mut HashMap<MapKey, Value>;
                debug_assert!(!ptr.is_null(), "Map pointer is null");
                debug_assert!(
                    (ptr as usize).is_multiple_of(std::mem::align_of::<HashMap<MapKey, Value>>()),
                    "Map pointer is misaligned"
                );
                let boxed = Box::from_raw(ptr);
                let cloned = boxed.clone();
                std::mem::forget(boxed);
                StackValue {
                    slot0: DISC_MAP,
                    slot1: Box::into_raw(cloned) as u64,
                    slot2: 0,
                    slot3: 0,
                    slot4: 0,
                }
            }
            DISC_CLOSURE => {
                // The env is stored as Box<Arc<[Value]>>
                let env_box_ptr = sv.slot2 as *mut Arc<[Value]>;
                debug_assert!(!env_box_ptr.is_null(), "Closure env pointer is null");
                debug_assert!(
                    (env_box_ptr as usize).is_multiple_of(std::mem::align_of::<Arc<[Value]>>()),
                    "Closure env pointer is misaligned"
                );
                let env_arc = &*env_box_ptr;
                let cloned_env = Arc::clone(env_arc);
                // Box the cloned Arc
                let new_env_box = Box::new(cloned_env);
                StackValue {
                    slot0: DISC_CLOSURE,
                    slot1: sv.slot1,
                    slot2: Box::into_raw(new_env_box) as u64,
                    slot3: 0,
                    slot4: 0,
                }
            }
            DISC_CHANNEL => {
                // Arc refcount increment - O(1) clone
                use crate::value::ChannelData;
                let ptr = sv.slot1 as *const ChannelData;
                debug_assert!(!ptr.is_null(), "Channel pointer is null");
                let arc = Arc::from_raw(ptr);
                let cloned = Arc::clone(&arc);
                std::mem::forget(arc);
                StackValue {
                    slot0: DISC_CHANNEL,
                    slot1: Arc::into_raw(cloned) as u64,
                    slot2: 0,
                    slot3: 0,
                    slot4: 0,
                }
            }
            DISC_WEAVECTX => {
                // Arc refcount increment for both channels - O(1) clone
                use crate::value::WeaveChannelData;
                let yield_ptr = sv.slot1 as *const WeaveChannelData;
                let resume_ptr = sv.slot2 as *const WeaveChannelData;
                debug_assert!(!yield_ptr.is_null(), "WeaveCtx yield pointer is null");
                debug_assert!(!resume_ptr.is_null(), "WeaveCtx resume pointer is null");
                let yield_arc = Arc::from_raw(yield_ptr);
                let resume_arc = Arc::from_raw(resume_ptr);
                let yield_cloned = Arc::clone(&yield_arc);
                let resume_cloned = Arc::clone(&resume_arc);
                std::mem::forget(yield_arc);
                std::mem::forget(resume_arc);
                StackValue {
                    slot0: DISC_WEAVECTX,
                    slot1: Arc::into_raw(yield_cloned) as u64,
                    slot2: Arc::into_raw(resume_cloned) as u64,
                    slot3: 0,
                    slot4: 0,
                }
            }
            DISC_SYMBOL => {
                let capacity = sv.slot3 as usize;
                let is_global = sv.slot4 != 0;

                // Fast path (Issue #166): interned symbols can share the static pointer
                // Interned symbols have capacity=0 and global=true
                if capacity == 0 && is_global {
                    let ptr = sv.slot1 as *const u8;
                    let len = sv.slot2 as usize;

                    // Safety: Interned symbols are guaranteed to point to valid static data.
                    // The compiler generates these in get_symbol_global(), which always
                    // creates valid string globals. A null pointer here indicates compiler bug.
                    debug_assert!(
                        !ptr.is_null(),
                        "Interned symbol has null pointer in clone fast path"
                    );

                    // Create a new SeqString that shares the static data.
                    // This properly maintains ownership semantics even though
                    // Drop is a no-op for capacity=0 symbols.
                    let seq_str = crate::seqstring::SeqString::from_raw_parts(ptr, len, 0, true);
                    let (new_ptr, new_len, new_cap, new_global) = seq_str.into_raw_parts();
                    StackValue {
                        slot0: DISC_SYMBOL,
                        slot1: new_ptr as u64,
                        slot2: new_len as u64,
                        slot3: new_cap as u64,
                        slot4: if new_global { 1 } else { 0 },
                    }
                } else {
                    // Deep copy: arena symbols may become invalid
                    let ptr = sv.slot1 as *const u8;
                    let len = sv.slot2 as usize;
                    debug_assert!(!ptr.is_null(), "Symbol pointer is null");
                    let slice = std::slice::from_raw_parts(ptr, len);
                    #[cfg(debug_assertions)]
                    let s = std::str::from_utf8(slice).expect("Invalid UTF-8 in symbol clone");
                    #[cfg(not(debug_assertions))]
                    let s = std::str::from_utf8_unchecked(slice);
                    let cloned = crate::seqstring::global_string(s.to_string());
                    let (new_ptr, new_len, new_cap, new_global) = cloned.into_raw_parts();
                    StackValue {
                        slot0: DISC_SYMBOL,
                        slot1: new_ptr as u64,
                        slot2: new_len as u64,
                        slot3: new_cap as u64,
                        slot4: if new_global { 1 } else { 0 },
                    }
                }
            }
            _ => panic!("Invalid discriminant for clone: {}", sv.slot0),
        }
    }
}

/// Drop a StackValue, decrementing refcounts for heap types
///
/// # Safety
/// The StackValue must contain valid data for its discriminant.
#[inline]
pub unsafe fn drop_stack_value(sv: StackValue) {
    unsafe {
        match sv.slot0 {
            DISC_INT | DISC_FLOAT | DISC_BOOL | DISC_QUOTATION => {
                // No heap allocation, nothing to drop
            }
            DISC_STRING => {
                // Reconstruct SeqString and let it drop
                use crate::seqstring::SeqString;
                let ptr = sv.slot1 as *const u8;
                let len = sv.slot2 as usize;
                let capacity = sv.slot3 as usize;
                let global = sv.slot4 != 0;
                let _ = SeqString::from_raw_parts(ptr, len, capacity, global);
                // SeqString::drop will free global strings, ignore arena strings
            }
            DISC_VARIANT => {
                use crate::value::VariantData;
                let _ = Arc::from_raw(sv.slot1 as *const VariantData);
            }
            DISC_MAP => {
                use crate::value::MapKey;
                use std::collections::HashMap;
                let _ = Box::from_raw(sv.slot1 as *mut HashMap<MapKey, Value>);
            }
            DISC_CLOSURE => {
                // Unbox and drop the Arc<[Value]>
                let _ = Box::from_raw(sv.slot2 as *mut Arc<[Value]>);
            }
            DISC_CHANNEL => {
                use crate::value::ChannelData;
                let _ = Arc::from_raw(sv.slot1 as *const ChannelData);
            }
            DISC_WEAVECTX => {
                use crate::value::WeaveChannelData;
                let _ = Arc::from_raw(sv.slot1 as *const WeaveChannelData);
                let _ = Arc::from_raw(sv.slot2 as *const WeaveChannelData);
            }
            DISC_SYMBOL => {
                // Reconstruct SeqString and let it drop (same as String)
                use crate::seqstring::SeqString;
                let ptr = sv.slot1 as *const u8;
                let len = sv.slot2 as usize;
                let capacity = sv.slot3 as usize;
                let global = sv.slot4 != 0;
                let _ = SeqString::from_raw_parts(ptr, len, capacity, global);
            }
            _ => panic!("Invalid discriminant for drop: {}", sv.slot0),
        }
    }
}

// ============================================================================
// Core Stack Operations
// ============================================================================

/// Push a value onto the stack
///
/// Stores the value at the current stack pointer and returns sp + 1.
///
/// # Safety
/// Stack pointer must be valid and have room for the value.
#[inline]
pub unsafe fn push(stack: Stack, value: Value) -> Stack {
    unsafe {
        let sv = value_to_stack_value(value);
        *stack = sv;
        stack.add(1)
    }
}

/// Push a StackValue directly onto the stack
///
/// # Safety
/// Stack pointer must be valid and have room for the value.
#[inline]
pub unsafe fn push_sv(stack: Stack, sv: StackValue) -> Stack {
    unsafe {
        *stack = sv;
        stack.add(1)
    }
}

/// Pop a value from the stack
///
/// Returns (new_sp, value) where new_sp = sp - 1.
///
/// # Safety
/// Stack must not be at base (must have at least one value).
#[inline]
pub unsafe fn pop(stack: Stack) -> (Stack, Value) {
    unsafe {
        let new_sp = stack.sub(1);
        let sv = *new_sp;
        (new_sp, stack_value_to_value(sv))
    }
}

/// Pop a StackValue directly from the stack
///
/// # Safety
/// Stack must not be at base and must have at least one value.
#[inline]
pub unsafe fn pop_sv(stack: Stack) -> (Stack, StackValue) {
    unsafe {
        let new_sp = stack.sub(1);
        let sv = *new_sp;
        (new_sp, sv)
    }
}

/// Pop two values from the stack (for binary operations)
///
/// Returns (new_sp, a, b) where a was below b on stack.
/// Stack effect: ( a b -- ) returns (a, b)
///
/// # Safety
/// Stack must have at least two values.
#[inline]
pub unsafe fn pop_two(stack: Stack, _op_name: &str) -> (Stack, Value, Value) {
    unsafe {
        let (sp, b) = pop(stack);
        let (sp, a) = pop(sp);
        (sp, a, b)
    }
}

/// Pop three values from the stack (for ternary operations)
///
/// # Safety
/// Stack must have at least three values.
#[inline]
pub unsafe fn pop_three(stack: Stack, _op_name: &str) -> (Stack, Value, Value, Value) {
    unsafe {
        let (sp, c) = pop(stack);
        let (sp, b) = pop(sp);
        let (sp, a) = pop(sp);
        (sp, a, b, c)
    }
}

/// Peek at the top value without removing it
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn peek(stack: Stack) -> Value {
    unsafe {
        let sv = *stack.sub(1);
        // Don't consume - need to clone for heap types
        stack_value_to_value(clone_stack_value(&sv))
    }
}

/// Peek at the raw StackValue without removing it
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn peek_sv(stack: Stack) -> StackValue {
    unsafe { *stack.sub(1) }
}

/// Check if stack is empty (at base pointer)
/// Note: With tagged stack, we need to compare against base, not null
#[inline]
pub fn is_empty(_stack: Stack) -> bool {
    // For now, assume stack is never truly empty in valid programs
    // The caller should track base pointer if needed
    false
}

// ============================================================================
// FFI Stack Operations
// ============================================================================

/// Duplicate the top value on the stack: ( a -- a a )
///
/// # Safety
/// Stack must have at least one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dup(stack: Stack) -> Stack {
    unsafe {
        let sv = peek_sv(stack);
        let cloned = clone_stack_value(&sv);
        push_sv(stack, cloned)
    }
}

/// Drop the top value from the stack: ( a -- )
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn drop_top(stack: Stack) -> Stack {
    unsafe {
        let (new_sp, sv) = pop_sv(stack);
        drop_stack_value(sv);
        new_sp
    }
}

/// Alias for drop to avoid LLVM keyword conflicts
///
/// # Safety
/// Stack must have at least one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_drop_op(stack: Stack) -> Stack {
    unsafe { drop_top(stack) }
}

/// Push an arbitrary Value onto the stack (for LLVM codegen)
///
/// # Safety
/// Stack pointer must be valid and have room for the value.
#[allow(improper_ctypes_definitions)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_value(stack: Stack, value: Value) -> Stack {
    unsafe { push(stack, value) }
}

/// Swap the top two values: ( a b -- b a )
///
/// # Safety
/// Stack must have at least two values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_swap(stack: Stack) -> Stack {
    unsafe {
        let ptr_b = stack.sub(1);
        let ptr_a = stack.sub(2);
        let a = *ptr_a;
        let b = *ptr_b;
        *ptr_a = b;
        *ptr_b = a;
        stack
    }
}

/// Copy the second value to the top: ( a b -- a b a )
///
/// # Safety
/// Stack must have at least two values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_over(stack: Stack) -> Stack {
    unsafe {
        let sv_a = *stack.sub(2);
        let cloned = clone_stack_value(&sv_a);
        push_sv(stack, cloned)
    }
}

/// Rotate the top three values: ( a b c -- b c a )
///
/// # Safety
/// Stack must have at least three values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_rot(stack: Stack) -> Stack {
    unsafe {
        let ptr_c = stack.sub(1);
        let ptr_b = stack.sub(2);
        let ptr_a = stack.sub(3);
        let a = *ptr_a;
        let b = *ptr_b;
        let c = *ptr_c;
        *ptr_a = b;
        *ptr_b = c;
        *ptr_c = a;
        stack
    }
}

/// Remove the second value: ( a b -- b )
///
/// # Safety
/// Stack must have at least two values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_nip(stack: Stack) -> Stack {
    unsafe {
        let ptr_b = stack.sub(1);
        let ptr_a = stack.sub(2);
        let a = *ptr_a;
        let b = *ptr_b;
        drop_stack_value(a);
        *ptr_a = b;
        stack.sub(1)
    }
}

/// Copy top value below second value: ( a b -- b a b )
///
/// # Safety
/// Stack must have at least two values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_tuck(stack: Stack) -> Stack {
    unsafe {
        let ptr_b = stack.sub(1);
        let ptr_a = stack.sub(2);
        let a = *ptr_a;
        let b = *ptr_b;
        let b_clone = clone_stack_value(&b);
        *ptr_a = b;
        *ptr_b = a;
        push_sv(stack, b_clone)
    }
}

/// Duplicate top two values: ( a b -- a b a b )
///
/// # Safety
/// Stack must have at least two values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_2dup(stack: Stack) -> Stack {
    unsafe {
        let sv_a = *stack.sub(2);
        let sv_b = *stack.sub(1);
        let a_clone = clone_stack_value(&sv_a);
        let b_clone = clone_stack_value(&sv_b);
        let sp = push_sv(stack, a_clone);
        push_sv(sp, b_clone)
    }
}

/// Drop top three values: ( a b c -- )
///
/// # Safety
/// Stack must have at least three values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_3drop(stack: Stack) -> Stack {
    unsafe {
        let (sp, sv_c) = pop_sv(stack);
        let (sp, sv_b) = pop_sv(sp);
        let (sp, sv_a) = pop_sv(sp);
        drop_stack_value(sv_c);
        drop_stack_value(sv_b);
        drop_stack_value(sv_a);
        sp
    }
}

/// Pick: Copy the nth value to the top
/// ( ... xn ... x1 x0 n -- ... xn ... x1 x0 xn )
///
/// # Safety
/// Stack must have at least n+1 values (plus the index value).
///
/// # Panics
/// - If the top value is not an Int
/// - If n is negative
/// - If n exceeds the current stack depth
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_pick_op(stack: Stack) -> Stack {
    unsafe {
        // Get n from top of stack
        let (sp, n_val) = pop(stack);
        let n_raw = match n_val {
            Value::Int(i) => i,
            _ => panic!("pick: expected Int"),
        };

        // Bounds check: n must be non-negative
        if n_raw < 0 {
            panic!("pick: index cannot be negative (got {})", n_raw);
        }
        let n = n_raw as usize;

        // Check stack depth to prevent out-of-bounds access
        let base = get_stack_base();
        let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();
        if n >= depth {
            panic!(
                "pick: index {} exceeds stack depth {} (need at least {} values)",
                n,
                depth,
                n + 1
            );
        }

        // Get the value at depth n (0 = top after popping n)
        let sv = *sp.sub(n + 1);
        let cloned = clone_stack_value(&sv);
        push_sv(sp, cloned)
    }
}

/// Roll: Rotate n+1 items, bringing the item at depth n to the top
/// ( x_n x_(n-1) ... x_1 x_0 n -- x_(n-1) ... x_1 x_0 x_n )
///
/// # Safety
/// Stack must have at least n+1 values (plus the index value).
///
/// # Panics
/// - If the top value is not an Int
/// - If n is negative
/// - If n exceeds the current stack depth
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_roll(stack: Stack) -> Stack {
    unsafe {
        // Get n from top of stack
        let (sp, n_val) = pop(stack);
        let n_raw = match n_val {
            Value::Int(i) => i,
            _ => panic!("roll: expected Int"),
        };

        // Bounds check: n must be non-negative
        if n_raw < 0 {
            panic!("roll: index cannot be negative (got {})", n_raw);
        }
        let n = n_raw as usize;

        if n == 0 {
            return sp;
        }
        if n == 1 {
            return patch_seq_swap(sp);
        }
        if n == 2 {
            return patch_seq_rot(sp);
        }

        // Check stack depth to prevent out-of-bounds access
        let base = get_stack_base();
        let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();
        if n >= depth {
            panic!(
                "roll: index {} exceeds stack depth {} (need at least {} values)",
                n,
                depth,
                n + 1
            );
        }

        // General case: save item at depth n, shift others, put saved at top
        let src_ptr = sp.sub(n + 1);
        let saved = *src_ptr;

        // Shift items down: memmove from src+1 to src, n items
        std::ptr::copy(src_ptr.add(1), src_ptr, n);

        // Put saved item at top (sp - 1)
        *sp.sub(1) = saved;

        sp
    }
}

/// Clone a stack segment
///
/// Clones `count` StackValues from src to dst, handling refcounts.
///
/// # Safety
/// Both src and dst must be valid stack pointers with sufficient space for count values.
pub unsafe fn clone_stack_segment(src: Stack, dst: Stack, count: usize) {
    unsafe {
        for i in 0..count {
            let sv = *src.sub(count - i);
            let cloned = clone_stack_value(&sv);
            *dst.add(i) = cloned;
        }
    }
}

// ============================================================================
// Coroutine-Local Stack Base Tracking (for spawn)
// ============================================================================
//
// IMPORTANT: We use May's coroutine_local! instead of thread_local! because
// May coroutines can migrate between OS threads. Using thread_local would cause
// STACK_BASE to be lost when a coroutine is moved to a different thread.

use std::cell::Cell;

// Use coroutine-local storage that moves with the coroutine
may::coroutine_local!(static STACK_BASE: Cell<usize> = Cell::new(0));

/// Set the current strand's stack base (called at strand entry)
///
/// # Safety
/// Base pointer must be a valid stack pointer for the current strand.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_set_stack_base(base: Stack) {
    STACK_BASE.with(|cell| {
        cell.set(base as usize);
    });
}

/// Get the current strand's stack base
#[inline]
pub fn get_stack_base() -> Stack {
    STACK_BASE.with(|cell| cell.get() as *mut StackValue)
}

/// Clone the current stack for spawning a child strand
///
/// Allocates a new stack buffer and copies all values from the current stack.
/// Returns a pointer to the first value in the new stack (like Stack convention).
///
/// # Safety
/// - Current stack must have a valid base set via set_stack_base
/// - sp must point to a valid position within the current stack
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clone_stack(sp: Stack) -> Stack {
    unsafe {
        let (new_sp, _base) = clone_stack_with_base(sp);
        new_sp
    }
}

/// Clone the current stack for spawning, returning both base and sp.
///
/// This is used by spawn to create a copy of the parent's stack for the child strand.
/// Returns (new_sp, new_base) so the spawn mechanism can set STACK_BASE for the child.
///
/// # Safety
/// Current stack must have a valid base set via set_stack_base and sp must point to a valid position.
pub unsafe fn clone_stack_with_base(sp: Stack) -> (Stack, Stack) {
    let base = get_stack_base();
    if base.is_null() {
        panic!("clone_stack: stack base not set");
    }

    // Calculate depth (number of values on stack)
    let depth = unsafe { sp.offset_from(base) as usize };

    if depth == 0 {
        // Empty stack - still need to allocate a buffer
        use crate::tagged_stack::{DEFAULT_STACK_CAPACITY, TaggedStack};
        let new_stack = TaggedStack::new(DEFAULT_STACK_CAPACITY);
        let new_base = new_stack.base;
        std::mem::forget(new_stack); // Don't drop - caller owns memory
        return (new_base, new_base);
    }

    // Allocate new stack with capacity for at least the current depth
    use crate::tagged_stack::{DEFAULT_STACK_CAPACITY, TaggedStack};
    let capacity = depth.max(DEFAULT_STACK_CAPACITY);
    let new_stack = TaggedStack::new(capacity);
    let new_base = new_stack.base;
    std::mem::forget(new_stack); // Don't drop - caller owns memory

    // Clone all values from base to sp
    unsafe {
        for i in 0..depth {
            let sv = &*base.add(i);
            let cloned = clone_stack_value(sv);
            *new_base.add(i) = cloned;
        }
    }

    // Return both sp and base
    unsafe { (new_base.add(depth), new_base) }
}

// ============================================================================
// Short Aliases for Internal/Test Use
// ============================================================================

pub use patch_seq_2dup as two_dup;
pub use patch_seq_3drop as three_drop;
pub use patch_seq_dup as dup;
pub use patch_seq_nip as nip;
pub use patch_seq_over as over;
pub use patch_seq_pick_op as pick;
pub use patch_seq_roll as roll;
pub use patch_seq_rot as rot;
pub use patch_seq_swap as swap;
pub use patch_seq_tuck as tuck;

// ============================================================================
// Stack Allocation Helpers
// ============================================================================

/// Allocate a new stack with default capacity.
/// Returns a pointer to the base of the stack (where first push goes).
///
/// # Note
/// The returned stack is allocated but not tracked.
/// The memory will be leaked when the caller is done with it.
/// This is used for temporary stacks in quotation calls and tests.
pub fn alloc_stack() -> Stack {
    use crate::tagged_stack::TaggedStack;
    let stack = TaggedStack::with_default_capacity();
    let base = stack.base;
    std::mem::forget(stack); // Don't drop - caller owns memory
    base
}

/// Allocate a new test stack and set it as the stack base
/// This is used in tests that need clone_stack to work
#[cfg(test)]
pub fn alloc_test_stack() -> Stack {
    let stack = alloc_stack();
    unsafe { patch_seq_set_stack_base(stack) };
    stack
}

/// Dump all values on the stack (for REPL debugging)
///
/// Prints all stack values in a readable format, then clears the stack.
/// Returns the stack base (empty stack).
///
/// # Safety
/// - Stack base must have been set via set_stack_base
/// - sp must be a valid stack pointer
/// - All stack values between base and sp must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_stack_dump(sp: Stack) -> Stack {
    let base = get_stack_base();
    if base.is_null() {
        eprintln!("[stack.dump: base not set]");
        return sp;
    }

    let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();

    if depth == 0 {
        println!("stack:");
    } else {
        use std::io::Write;
        print!("stack: ");
        for i in 0..depth {
            if i > 0 {
                print!(" ");
            }
            unsafe {
                let sv = *base.add(i);
                print_stack_value(&sv);
            }
        }
        println!();
        // Flush stdout to ensure output is visible immediately
        // This prevents partial output if the program terminates unexpectedly
        let _ = std::io::stdout().flush();

        // Drop all heap-allocated values to prevent memory leaks
        for i in 0..depth {
            unsafe {
                let sv = *base.add(i);
                drop_stack_value(sv);
            }
        }
    }

    // Return base (empty stack)
    base
}

/// Print a stack value in SON (Seq Object Notation) format
///
/// # Safety Requirements
/// The StackValue must be valid and not previously dropped. For strings,
/// the pointer (slot1) must point to valid, readable memory of length slot2.
/// This is guaranteed when called from stack.dump on freshly computed values.
fn print_stack_value(sv: &StackValue) {
    use crate::son::{SonConfig, value_to_son};

    // Safety: We must clone the StackValue before converting to Value because
    // stack_value_to_value takes ownership of heap-allocated data (via Box::from_raw
    // for maps, Arc::from_raw for variants, etc.). Without cloning, the Value would
    // be dropped after printing, freeing the memory, and then the later drop_stack_value
    // loop would double-free. clone_stack_value properly handles refcounting.
    let cloned = unsafe { clone_stack_value(sv) };
    let value = unsafe { stack_value_to_value(cloned) };
    let son = value_to_son(&value, &SonConfig::compact());
    print!("{}", son);
}

/// Macro to create a test stack
#[macro_export]
macro_rules! test_stack {
    () => {{
        use $crate::tagged_stack::StackValue;
        static mut BUFFER: [StackValue; 256] = unsafe { std::mem::zeroed() };
        unsafe { BUFFER.as_mut_ptr() }
    }};
}
