//! List operations for Seq
//!
//! Higher-order combinators for working with lists (Variants).
//! These provide idiomatic concatenative-style list processing.
//!
//! # Examples
//!
//! ```seq
//! # Map: double each element
//! my-list [ 2 * ] list-map
//!
//! # Filter: keep positive numbers
//! my-list [ 0 > ] list-filter
//!
//! # Fold: sum all elements
//! my-list 0 [ + ] list-fold
//!
//! # Each: print each element
//! my-list [ write_line ] list-each
//! ```

use crate::stack::{Stack, is_empty, pop, push};
use crate::value::{Value, VariantData};

/// Helper to drain and free any remaining stack nodes
///
/// This ensures no memory is leaked if a quotation misbehaves
/// by leaving extra values on the stack.
unsafe fn drain_stack(mut stack: Stack) {
    unsafe {
        while !is_empty(stack) {
            let (rest, _) = pop(stack);
            stack = rest;
        }
    }
}

/// Helper to call a quotation or closure with a value on the stack
///
/// Pushes `value` onto the stack, then calls the callable.
/// Returns the resulting stack.
unsafe fn call_with_value(stack: Stack, value: Value, callable: &Value) -> Stack {
    unsafe {
        let stack = push(stack, value);

        match callable {
            Value::Quotation { wrapper, .. } => {
                let fn_ref: unsafe extern "C" fn(Stack) -> Stack = std::mem::transmute(*wrapper);
                fn_ref(stack)
            }
            Value::Closure { fn_ptr, env } => {
                let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                    std::mem::transmute(*fn_ptr);
                fn_ref(stack, env.as_ptr(), env.len())
            }
            _ => panic!("list operation: expected Quotation or Closure"),
        }
    }
}

/// Map a quotation over a list, returning a new list
///
/// Stack effect: ( Variant Quotation -- Variant )
///
/// The quotation should have effect ( elem -- elem' )
/// Each element is transformed by the quotation.
///
/// # Safety
/// Stack must have a Quotation/Closure on top and a Variant below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_map(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation
        let (stack, callable) = pop(stack);

        // Validate callable
        match &callable {
            Value::Quotation { .. } | Value::Closure { .. } => {}
            _ => panic!(
                "list-map: expected Quotation or Closure, got {:?}",
                callable
            ),
        }

        // Pop variant (list)
        let (stack, list_val) = pop(stack);

        let variant_data = match list_val {
            Value::Variant(v) => v,
            _ => panic!("list-map: expected Variant (list), got {:?}", list_val),
        };

        // Map over each element
        let mut results = Vec::with_capacity(variant_data.fields.len());

        for field in variant_data.fields.iter() {
            // Call quotation with element on stack
            let temp_stack = call_with_value(std::ptr::null_mut(), field.clone(), &callable);

            // Pop result - quotation should have effect ( elem -- elem' )
            if is_empty(temp_stack) {
                panic!("list-map: quotation consumed element without producing result");
            }
            let (remaining, result) = pop(temp_stack);
            results.push(result);

            // Stack hygiene: drain any extra values left by misbehaving quotation
            if !is_empty(remaining) {
                drain_stack(remaining);
            }
        }

        // Create new variant with same tag
        let new_variant = Value::Variant(Box::new(VariantData::new(variant_data.tag, results)));

        push(stack, new_variant)
    }
}

/// Filter a list, keeping elements where quotation returns non-zero
///
/// Stack effect: ( Variant Quotation -- Variant )
///
/// The quotation should have effect ( elem -- Int )
/// Elements are kept if the quotation returns non-zero.
///
/// # Safety
/// Stack must have a Quotation/Closure on top and a Variant below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_filter(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation
        let (stack, callable) = pop(stack);

        // Validate callable
        match &callable {
            Value::Quotation { .. } | Value::Closure { .. } => {}
            _ => panic!(
                "list-filter: expected Quotation or Closure, got {:?}",
                callable
            ),
        }

        // Pop variant (list)
        let (stack, list_val) = pop(stack);

        let variant_data = match list_val {
            Value::Variant(v) => v,
            _ => panic!("list-filter: expected Variant (list), got {:?}", list_val),
        };

        // Filter elements
        let mut results = Vec::new();

        for field in variant_data.fields.iter() {
            // Call quotation with element on stack
            let temp_stack = call_with_value(std::ptr::null_mut(), field.clone(), &callable);

            // Pop result - quotation should have effect ( elem -- Int )
            if is_empty(temp_stack) {
                panic!("list-filter: quotation consumed element without producing result");
            }
            let (remaining, result) = pop(temp_stack);

            let keep = match result {
                Value::Int(n) => n != 0,
                _ => panic!("list-filter: quotation must return Int, got {:?}", result),
            };

            if keep {
                results.push(field.clone());
            }

            // Stack hygiene: drain any extra values left by misbehaving quotation
            if !is_empty(remaining) {
                drain_stack(remaining);
            }
        }

        // Create new variant with same tag
        let new_variant = Value::Variant(Box::new(VariantData::new(variant_data.tag, results)));

        push(stack, new_variant)
    }
}

/// Fold a list with an accumulator and quotation
///
/// Stack effect: ( Variant init Quotation -- result )
///
/// The quotation should have effect ( acc elem -- acc' )
/// Starts with init as accumulator, folds left through the list.
///
/// # Safety
/// Stack must have Quotation on top, init below, and Variant below that
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_fold(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation
        let (stack, callable) = pop(stack);

        // Validate callable
        match &callable {
            Value::Quotation { .. } | Value::Closure { .. } => {}
            _ => panic!(
                "list-fold: expected Quotation or Closure, got {:?}",
                callable
            ),
        }

        // Pop initial accumulator
        let (stack, init) = pop(stack);

        // Pop variant (list)
        let (stack, list_val) = pop(stack);

        let variant_data = match list_val {
            Value::Variant(v) => v,
            _ => panic!("list-fold: expected Variant (list), got {:?}", list_val),
        };

        // Fold over elements
        let mut acc = init;

        for field in variant_data.fields.iter() {
            // Push acc, then element, then call quotation
            let temp_stack = push(std::ptr::null_mut(), acc);
            let temp_stack = push(temp_stack, field.clone());

            let temp_stack = match &callable {
                Value::Quotation { wrapper, .. } => {
                    let fn_ref: unsafe extern "C" fn(Stack) -> Stack =
                        std::mem::transmute(*wrapper);
                    fn_ref(temp_stack)
                }
                Value::Closure { fn_ptr, env } => {
                    let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                        std::mem::transmute(*fn_ptr);
                    fn_ref(temp_stack, env.as_ptr(), env.len())
                }
                _ => unreachable!(),
            };

            // Pop new accumulator - quotation should have effect ( acc elem -- acc' )
            if is_empty(temp_stack) {
                panic!("list-fold: quotation consumed inputs without producing result");
            }
            let (remaining, new_acc) = pop(temp_stack);
            acc = new_acc;

            // Stack hygiene: drain any extra values left by misbehaving quotation
            if !is_empty(remaining) {
                drain_stack(remaining);
            }
        }

        push(stack, acc)
    }
}

/// Apply a quotation to each element of a list (for side effects)
///
/// Stack effect: ( Variant Quotation -- )
///
/// The quotation should have effect ( elem -- )
/// Each element is passed to the quotation; results are discarded.
///
/// # Safety
/// Stack must have a Quotation/Closure on top and a Variant below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_each(stack: Stack) -> Stack {
    unsafe {
        // Pop quotation
        let (stack, callable) = pop(stack);

        // Validate callable
        match &callable {
            Value::Quotation { .. } | Value::Closure { .. } => {}
            _ => panic!(
                "list-each: expected Quotation or Closure, got {:?}",
                callable
            ),
        }

        // Pop variant (list)
        let (stack, list_val) = pop(stack);

        let variant_data = match list_val {
            Value::Variant(v) => v,
            _ => panic!("list-each: expected Variant (list), got {:?}", list_val),
        };

        // Call quotation for each element (for side effects)
        for field in variant_data.fields.iter() {
            let temp_stack = call_with_value(std::ptr::null_mut(), field.clone(), &callable);
            // Stack hygiene: drain any values left by quotation (effect should be ( elem -- ))
            if !is_empty(temp_stack) {
                drain_stack(temp_stack);
            }
        }

        stack
    }
}

/// Get the length of a list
///
/// Stack effect: ( Variant -- Int )
///
/// Returns the number of elements in the list.
/// This is an alias for variant-field-count, provided for semantic clarity.
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_length(stack: Stack) -> Stack {
    unsafe { crate::variant_ops::patch_seq_variant_field_count(stack) }
}

/// Check if a list is empty
///
/// Stack effect: ( Variant -- Int )
///
/// Returns 1 if the list has no elements, 0 otherwise.
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_list_empty(stack: Stack) -> Stack {
    unsafe {
        let (stack, list_val) = pop(stack);

        let is_empty = match list_val {
            Value::Variant(v) => {
                if v.fields.is_empty() {
                    1i64
                } else {
                    0i64
                }
            }
            _ => panic!("list-empty?: expected Variant (list), got {:?}", list_val),
        };

        push(stack, Value::Int(is_empty))
    }
}

// Public re-exports
pub use patch_seq_list_each as list_each;
pub use patch_seq_list_empty as list_empty;
pub use patch_seq_list_filter as list_filter;
pub use patch_seq_list_fold as list_fold;
pub use patch_seq_list_length as list_length;
pub use patch_seq_list_map as list_map;

#[cfg(test)]
mod tests {
    use super::*;

    // Helper quotation: double an integer
    unsafe extern "C" fn double_quot(stack: Stack) -> Stack {
        unsafe {
            let (stack, val) = pop(stack);
            match val {
                Value::Int(n) => push(stack, Value::Int(n * 2)),
                _ => panic!("Expected Int"),
            }
        }
    }

    // Helper quotation: check if positive
    unsafe extern "C" fn is_positive_quot(stack: Stack) -> Stack {
        unsafe {
            let (stack, val) = pop(stack);
            match val {
                Value::Int(n) => push(stack, Value::Int(if n > 0 { 1 } else { 0 })),
                _ => panic!("Expected Int"),
            }
        }
    }

    // Helper quotation: add two integers
    unsafe extern "C" fn add_quot(stack: Stack) -> Stack {
        unsafe {
            let (stack, b) = pop(stack);
            let (stack, a) = pop(stack);
            match (a, b) {
                (Value::Int(x), Value::Int(y)) => push(stack, Value::Int(x + y)),
                _ => panic!("Expected two Ints"),
            }
        }
    }

    #[test]
    fn test_list_map_double() {
        unsafe {
            // Create list [1, 2, 3]
            let list = Value::Variant(Box::new(VariantData::new(
                0,
                vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let fn_ptr = double_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_map(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::Int(2));
                    assert_eq!(v.fields[1], Value::Int(4));
                    assert_eq!(v.fields[2], Value::Int(6));
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_filter_positive() {
        unsafe {
            // Create list [-1, 2, -3, 4, 0]
            let list = Value::Variant(Box::new(VariantData::new(
                0,
                vec![
                    Value::Int(-1),
                    Value::Int(2),
                    Value::Int(-3),
                    Value::Int(4),
                    Value::Int(0),
                ],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let fn_ptr = is_positive_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_filter(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 2);
                    assert_eq!(v.fields[0], Value::Int(2));
                    assert_eq!(v.fields[1], Value::Int(4));
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_fold_sum() {
        unsafe {
            // Create list [1, 2, 3, 4, 5]
            let list = Value::Variant(Box::new(VariantData::new(
                0,
                vec![
                    Value::Int(1),
                    Value::Int(2),
                    Value::Int(3),
                    Value::Int(4),
                    Value::Int(5),
                ],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = push(stack, Value::Int(0)); // initial accumulator
            let fn_ptr = add_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_fold(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(15)); // 1+2+3+4+5 = 15
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_fold_empty() {
        unsafe {
            // Create empty list
            let list = Value::Variant(Box::new(VariantData::new(0, vec![])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = push(stack, Value::Int(42)); // initial accumulator
            let fn_ptr = add_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_fold(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(42)); // Should return initial value
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_length() {
        unsafe {
            let list = Value::Variant(Box::new(VariantData::new(
                0,
                vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = list_length(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_empty_true() {
        unsafe {
            let list = Value::Variant(Box::new(VariantData::new(0, vec![])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = list_empty(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_empty_false() {
        unsafe {
            let list = Value::Variant(Box::new(VariantData::new(0, vec![Value::Int(1)])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = list_empty(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_map_empty() {
        unsafe {
            let list = Value::Variant(Box::new(VariantData::new(0, vec![])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let fn_ptr = double_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_map(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 0);
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_list_map_preserves_tag() {
        unsafe {
            // Create list with custom tag (e.g., 42)
            let list = Value::Variant(Box::new(VariantData::new(
                42,
                vec![Value::Int(1), Value::Int(2)],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let fn_ptr = double_quot as usize;
            let stack = push(
                stack,
                Value::Quotation {
                    wrapper: fn_ptr,
                    impl_: fn_ptr,
                },
            );
            let stack = list_map(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 42); // Tag preserved
                    assert_eq!(v.fields[0], Value::Int(2));
                    assert_eq!(v.fields[1], Value::Int(4));
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }

    // Helper closure function: adds captured value to element
    // Closure receives: stack with element, env with [captured_value]
    unsafe extern "C" fn add_captured_closure(
        stack: Stack,
        env: *const Value,
        _env_len: usize,
    ) -> Stack {
        unsafe {
            let (stack, val) = pop(stack);
            let captured = &*env; // First (and only) captured value
            match (val, captured) {
                (Value::Int(n), Value::Int(c)) => push(stack, Value::Int(n + c)),
                _ => panic!("Expected Int"),
            }
        }
    }

    #[test]
    fn test_list_map_with_closure() {
        unsafe {
            // Create list [1, 2, 3]
            let list = Value::Variant(Box::new(VariantData::new(
                0,
                vec![Value::Int(1), Value::Int(2), Value::Int(3)],
            )));

            // Create closure that adds 10 to each element
            let env: std::sync::Arc<[Value]> =
                std::sync::Arc::from(vec![Value::Int(10)].into_boxed_slice());
            let closure = Value::Closure {
                fn_ptr: add_captured_closure as usize,
                env,
            };

            let stack = std::ptr::null_mut();
            let stack = push(stack, list);
            let stack = push(stack, closure);
            let stack = list_map(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::Int(11)); // 1 + 10
                    assert_eq!(v.fields[1], Value::Int(12)); // 2 + 10
                    assert_eq!(v.fields[2], Value::Int(13)); // 3 + 10
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }
}
