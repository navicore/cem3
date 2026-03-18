//! Tagged Stack Implementation
//!
//! Stack operations using 8-byte tagged values (tagged pointers).
//!
//! Encoding:
//! - Odd (bit 0 = 1): Int — 63-bit signed integer, value = tagged >> 1
//! - 0x0: Bool false
//! - 0x2: Bool true
//! - Even > 2: Heap pointer to Box<Value>
//!
//! The Stack type is a pointer to the "current position" (where next push goes).
//! - Push: store at *sp, return sp + 1
//! - Pop: return sp - 1, read from *(sp - 1)

use crate::tagged_stack::{StackValue, TAG_FALSE, TAG_TRUE, is_tagged_int, tag_int, untag_int};
use crate::value::Value;

/// Stack: A pointer to the current position in a contiguous array of u64.
pub type Stack = *mut StackValue;

/// Returns the size of a StackValue in bytes
#[inline]
pub fn stack_value_size() -> usize {
    std::mem::size_of::<StackValue>()
}

/// Discriminant constants — retained for API compatibility with codegen and
/// runtime code that switches on type. In tagged-ptr mode, these values are
/// NOT stored in the StackValue itself (the tag is in the pointer bits).
/// They are used only when the runtime unpacks a Value (via pop()) and needs
/// to identify its type. Phase 2 codegen will use bit-level tag checks instead
/// of loading these discriminants from memory.
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

/// Convert a Value to a tagged StackValue
#[inline]
pub fn value_to_stack_value(value: Value) -> StackValue {
    match value {
        Value::Int(i) => tag_int(i),
        Value::Bool(false) => TAG_FALSE,
        Value::Bool(true) => TAG_TRUE,
        other => {
            // Heap-allocate all other types as Box<Value>
            Box::into_raw(Box::new(other)) as u64
        }
    }
}

/// Convert a tagged StackValue back to a Value (takes ownership)
///
/// # Safety
/// The StackValue must contain valid data — either a tagged int, bool,
/// or a valid heap pointer from Box::into_raw.
#[inline]
pub unsafe fn stack_value_to_value(sv: StackValue) -> Value {
    if is_tagged_int(sv) {
        Value::Int(untag_int(sv))
    } else if sv == TAG_FALSE {
        Value::Bool(false)
    } else if sv == TAG_TRUE {
        Value::Bool(true)
    } else {
        // Heap pointer — take ownership of the Box<Value>
        unsafe { *Box::from_raw(sv as *mut Value) }
    }
}

/// Clone a StackValue from LLVM IR.
///
/// # Safety
/// src and dst must be valid pointers to StackValue slots.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_clone_value(src: *const StackValue, dst: *mut StackValue) {
    unsafe {
        let sv = *src;
        let cloned = clone_stack_value(sv);
        *dst = cloned;
    }
}

/// Clone a tagged StackValue, handling heap types.
///
/// - Int, Bool: bitwise copy (no allocation)
/// - Heap types: clone the Value and re-box
///
/// # Safety
/// The StackValue must contain valid tagged data.
#[inline]
pub unsafe fn clone_stack_value(sv: StackValue) -> StackValue {
    if is_tagged_int(sv) || sv == TAG_FALSE || sv == TAG_TRUE {
        // Int or Bool — just copy
        sv
    } else {
        // Heap pointer — clone the Value and create a new Box
        unsafe {
            let val_ref = &*(sv as *const Value);
            let cloned = val_ref.clone();
            Box::into_raw(Box::new(cloned)) as u64
        }
    }
}

/// Drop a tagged StackValue, freeing heap types.
///
/// # Safety
/// The StackValue must be valid and not previously dropped.
#[inline]
pub unsafe fn drop_stack_value(sv: StackValue) {
    if is_tagged_int(sv) || sv == TAG_FALSE || sv == TAG_TRUE {
        // Int or Bool — nothing to do
        return;
    }
    // Heap pointer — reconstruct Box and drop it
    unsafe {
        let _ = Box::from_raw(sv as *mut Value);
    }
}

// ============================================================================
// Core Stack Operations
// ============================================================================

/// Push a value onto the stack.
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

/// Push a StackValue directly onto the stack.
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

/// Pop a value from the stack.
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn pop(stack: Stack) -> (Stack, Value) {
    unsafe {
        let new_sp = stack.sub(1);
        let sv = *new_sp;
        (new_sp, stack_value_to_value(sv))
    }
}

/// Pop a StackValue directly from the stack.
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn pop_sv(stack: Stack) -> (Stack, StackValue) {
    unsafe {
        let new_sp = stack.sub(1);
        let sv = *new_sp;
        (new_sp, sv)
    }
}

/// Pop two values from the stack.
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

/// Pop three values from the stack.
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

/// Peek at the top value without removing it.
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn peek(stack: Stack) -> Value {
    unsafe {
        let sv = *stack.sub(1);
        let cloned = clone_stack_value(sv);
        stack_value_to_value(cloned)
    }
}

/// Peek at the raw StackValue without removing it.
///
/// # Safety
/// Stack must have at least one value.
#[inline]
pub unsafe fn peek_sv(stack: Stack) -> StackValue {
    unsafe { *stack.sub(1) }
}

// ============================================================================
// FFI Stack Operations
// ============================================================================

/// Duplicate the top value: ( a -- a a )
///
/// # Safety
/// Stack must have at least one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dup(stack: Stack) -> Stack {
    unsafe {
        let sv = peek_sv(stack);
        let cloned = clone_stack_value(sv);
        push_sv(stack, cloned)
    }
}

/// Drop the top value: ( a -- )
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

/// # Safety
/// Stack must have at least one value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_drop_op(stack: Stack) -> Stack {
    unsafe { drop_top(stack) }
}

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
        let cloned = clone_stack_value(sv_a);
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

/// Copy top value below second: ( a b -- b a b )
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
        let b_clone = clone_stack_value(b);
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
        let a_clone = clone_stack_value(sv_a);
        let b_clone = clone_stack_value(sv_b);
        let sp = push_sv(stack, a_clone);
        push_sv(sp, b_clone)
    }
}

/// Pick: Copy the nth value to the top.
///
/// # Safety
/// Stack must have at least n+2 values (n+1 data values plus the index).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_pick_op(stack: Stack) -> Stack {
    unsafe {
        let (sp, n_val) = pop(stack);
        let n_raw = match n_val {
            Value::Int(i) => i,
            _ => {
                crate::error::set_runtime_error("pick: expected Int index on top of stack");
                return sp;
            }
        };

        if n_raw < 0 {
            crate::error::set_runtime_error(format!(
                "pick: index cannot be negative (got {})",
                n_raw
            ));
            return sp;
        }
        let n = n_raw as usize;

        let base = get_stack_base();
        let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();
        if n >= depth {
            crate::error::set_runtime_error(format!(
                "pick: index {} exceeds stack depth {} (need at least {} values)",
                n,
                depth,
                n + 1
            ));
            return sp;
        }

        let sv = *sp.sub(n + 1);
        let cloned = clone_stack_value(sv);
        push_sv(sp, cloned)
    }
}

/// Roll: Rotate n+1 items, bringing the item at depth n to the top.
///
/// # Safety
/// Stack must have at least n+2 values (n+1 data values plus the index).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_roll(stack: Stack) -> Stack {
    unsafe {
        let (sp, n_val) = pop(stack);
        let n_raw = match n_val {
            Value::Int(i) => i,
            _ => {
                crate::error::set_runtime_error("roll: expected Int index on top of stack");
                return sp;
            }
        };

        if n_raw < 0 {
            crate::error::set_runtime_error(format!(
                "roll: index cannot be negative (got {})",
                n_raw
            ));
            return sp;
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

        let base = get_stack_base();
        let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();
        if n >= depth {
            crate::error::set_runtime_error(format!(
                "roll: index {} exceeds stack depth {} (need at least {} values)",
                n,
                depth,
                n + 1
            ));
            return sp;
        }

        let src_ptr = sp.sub(n + 1);
        let saved = *src_ptr;
        std::ptr::copy(src_ptr.add(1), src_ptr, n);
        *sp.sub(1) = saved;

        sp
    }
}

/// Clone a stack segment.
///
/// # Safety
/// Both src and dst must be valid stack pointers with sufficient space for count values.
pub unsafe fn clone_stack_segment(src: Stack, dst: Stack, count: usize) {
    unsafe {
        for i in 0..count {
            let sv = *src.sub(count - i);
            let cloned = clone_stack_value(sv);
            *dst.add(i) = cloned;
        }
    }
}

// ============================================================================
// Coroutine-Local Stack Base Tracking
// ============================================================================

use std::cell::Cell;

may::coroutine_local!(static STACK_BASE: Cell<usize> = Cell::new(0));

/// # Safety
/// Base pointer must be a valid stack pointer for the current strand.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_set_stack_base(base: Stack) {
    STACK_BASE.with(|cell| {
        cell.set(base as usize);
    });
}

#[inline]
pub fn get_stack_base() -> Stack {
    STACK_BASE.with(|cell| cell.get() as *mut StackValue)
}

/// # Safety
/// Current stack must have a valid base set via `patch_seq_set_stack_base`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clone_stack(sp: Stack) -> Stack {
    unsafe {
        let (new_sp, _base) = clone_stack_with_base(sp);
        new_sp
    }
}

/// # Safety
/// Current stack must have a valid base set and sp must point within the stack.
pub unsafe fn clone_stack_with_base(sp: Stack) -> (Stack, Stack) {
    let base = get_stack_base();
    if base.is_null() {
        panic!("clone_stack: stack base not set");
    }

    let depth = unsafe { sp.offset_from(base) as usize };

    if depth == 0 {
        use crate::tagged_stack::{DEFAULT_STACK_CAPACITY, TaggedStack};
        let new_stack = TaggedStack::new(DEFAULT_STACK_CAPACITY);
        let new_base = new_stack.base;
        std::mem::forget(new_stack);
        return (new_base, new_base);
    }

    use crate::tagged_stack::{DEFAULT_STACK_CAPACITY, TaggedStack};
    let capacity = depth.max(DEFAULT_STACK_CAPACITY);
    let new_stack = TaggedStack::new(capacity);
    let new_base = new_stack.base;
    std::mem::forget(new_stack);

    unsafe {
        for i in 0..depth {
            let sv = *base.add(i);
            let cloned = clone_stack_value(sv);
            *new_base.add(i) = cloned;
        }
    }

    unsafe { (new_base.add(depth), new_base) }
}

// ============================================================================
// Stack Allocation Helpers
// ============================================================================

pub fn alloc_stack() -> Stack {
    use crate::tagged_stack::TaggedStack;
    let stack = TaggedStack::with_default_capacity();
    let base = stack.base;
    std::mem::forget(stack);
    base
}

pub fn alloc_test_stack() -> Stack {
    let stack = alloc_stack();
    unsafe { patch_seq_set_stack_base(stack) };
    stack
}

/// Dump all values on the stack (for REPL debugging).
///
/// # Safety
/// Stack base must have been set and sp must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_stack_dump(sp: Stack) -> Stack {
    let base = get_stack_base();
    if base.is_null() {
        eprintln!("[stack.dump: base not set]");
        return sp;
    }

    let depth = (sp as usize - base as usize) / std::mem::size_of::<StackValue>();

    if depth == 0 {
        println!("»");
    } else {
        use std::io::Write;
        print!("» ");
        for i in 0..depth {
            if i > 0 {
                print!(" ");
            }
            unsafe {
                let sv = *base.add(i);
                print_stack_value(sv);
            }
        }
        println!();
        let _ = std::io::stdout().flush();

        // Drop all heap-allocated values
        for i in 0..depth {
            unsafe {
                let sv = *base.add(i);
                drop_stack_value(sv);
            }
        }
    }

    base
}

fn print_stack_value(sv: StackValue) {
    use crate::son::{SonConfig, value_to_son};

    let cloned = unsafe { clone_stack_value(sv) };
    let value = unsafe { stack_value_to_value(cloned) };
    let son = value_to_son(&value, &SonConfig::compact());
    print!("{}", son);
}

#[macro_export]
macro_rules! test_stack {
    () => {{ $crate::stack::alloc_test_stack() }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pick_negative_index_sets_error() {
        unsafe {
            crate::error::clear_runtime_error();
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Int(100));
            let stack = push(stack, Value::Int(-1));

            let _stack = patch_seq_pick_op(stack);

            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("negative"));
        }
    }

    #[test]
    fn test_pick_out_of_bounds_sets_error() {
        unsafe {
            crate::error::clear_runtime_error();
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Int(100));
            let stack = push(stack, Value::Int(10));

            let _stack = patch_seq_pick_op(stack);

            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("exceeds stack depth"));
        }
    }

    #[test]
    fn test_roll_negative_index_sets_error() {
        unsafe {
            crate::error::clear_runtime_error();
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Int(100));
            let stack = push(stack, Value::Int(-1));

            let _stack = patch_seq_roll(stack);

            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("negative"));
        }
    }

    #[test]
    fn test_roll_out_of_bounds_sets_error() {
        unsafe {
            crate::error::clear_runtime_error();
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Int(100));
            let stack = push(stack, Value::Int(10));

            let _stack = patch_seq_roll(stack);

            assert!(crate::error::has_runtime_error());
            let error = crate::error::take_runtime_error().unwrap();
            assert!(error.contains("exceeds stack depth"));
        }
    }

    #[test]
    fn test_int_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Int(42));
            let (_, val) = pop(stack);
            assert_eq!(val, Value::Int(42));
        }
    }

    #[test]
    fn test_bool_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Bool(true));
            let stack = push(stack, Value::Bool(false));
            let (stack, val_f) = pop(stack);
            let (_, val_t) = pop(stack);
            assert_eq!(val_f, Value::Bool(false));
            assert_eq!(val_t, Value::Bool(true));
        }
    }

    #[test]
    fn test_float_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, Value::Float(std::f64::consts::PI));
            let (_, val) = pop(stack);
            assert_eq!(val, Value::Float(std::f64::consts::PI));
        }
    }

    #[test]
    fn test_string_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let s = crate::seqstring::SeqString::from("hello");
            let stack = push(stack, Value::String(s));
            let (_, val) = pop(stack);
            match val {
                Value::String(s) => assert_eq!(s.as_str(), "hello"),
                other => panic!("Expected String, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_symbol_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let s = crate::seqstring::SeqString::from("my-sym");
            let stack = push(stack, Value::Symbol(s));
            let (_, val) = pop(stack);
            match val {
                Value::Symbol(s) => assert_eq!(s.as_str(), "my-sym"),
                other => panic!("Expected Symbol, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_variant_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let tag = crate::seqstring::SeqString::from("Foo");
            let data = crate::value::VariantData::new(tag, vec![Value::Int(1), Value::Int(2)]);
            let stack = push(stack, Value::Variant(std::sync::Arc::new(data)));
            let (_, val) = pop(stack);
            match val {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "Foo");
                    assert_eq!(v.fields.len(), 2);
                }
                other => panic!("Expected Variant, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_map_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let mut map = std::collections::HashMap::new();
            map.insert(crate::value::MapKey::Int(1), Value::Int(100));
            let stack = push(stack, Value::Map(Box::new(map)));
            let (_, val) = pop(stack);
            match val {
                Value::Map(m) => {
                    assert_eq!(m.len(), 1);
                    assert_eq!(m.get(&crate::value::MapKey::Int(1)), Some(&Value::Int(100)));
                }
                other => panic!("Expected Map, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_quotation_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: 0x1000,
                    impl_: 0x2000,
                },
            );
            let (_, val) = pop(stack);
            match val {
                Value::Quotation { wrapper, impl_ } => {
                    assert_eq!(wrapper, 0x1000);
                    assert_eq!(impl_, 0x2000);
                }
                other => panic!("Expected Quotation, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_closure_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let env: std::sync::Arc<[Value]> = std::sync::Arc::from(vec![Value::Int(42)]);
            let stack = push(
                stack,
                Value::Closure {
                    fn_ptr: 0x3000,
                    env,
                },
            );
            let (_, val) = pop(stack);
            match val {
                Value::Closure { fn_ptr, env } => {
                    assert_eq!(fn_ptr, 0x3000);
                    assert_eq!(env.len(), 1);
                }
                other => panic!("Expected Closure, got {:?}", other),
            }
        }
    }

    #[test]
    fn test_channel_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let (sender, receiver) = may::sync::mpmc::channel();
            let ch = std::sync::Arc::new(crate::value::ChannelData { sender, receiver });
            let stack = push(stack, Value::Channel(ch));
            let (_, val) = pop(stack);
            assert!(matches!(val, Value::Channel(_)));
        }
    }

    #[test]
    fn test_weavectx_roundtrip() {
        unsafe {
            let stack = alloc_test_stack();
            let (ys, yr) = may::sync::mpmc::channel();
            let (rs, rr) = may::sync::mpmc::channel();
            let yield_chan = std::sync::Arc::new(crate::value::WeaveChannelData {
                sender: ys,
                receiver: yr,
            });
            let resume_chan = std::sync::Arc::new(crate::value::WeaveChannelData {
                sender: rs,
                receiver: rr,
            });
            let stack = push(
                stack,
                Value::WeaveCtx {
                    yield_chan,
                    resume_chan,
                },
            );
            let (_, val) = pop(stack);
            assert!(matches!(val, Value::WeaveCtx { .. }));
        }
    }
}
