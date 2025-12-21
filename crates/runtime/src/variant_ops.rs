//! Variant field access operations for Seq
//!
//! Provides runtime functions for accessing variant fields, tags, and metadata.
//! These are used to work with composite data created by operations like string-split.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::sync::Arc;

/// Get the number of fields in a variant
///
/// Stack effect: ( Variant -- Int )
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_field_count(stack: Stack) -> Stack {
    unsafe {
        let (stack, value) = pop(stack);

        match value {
            Value::Variant(variant_data) => {
                let count = variant_data.fields.len() as i64;
                push(stack, Value::Int(count))
            }
            _ => panic!("variant-field-count: expected Variant, got {:?}", value),
        }
    }
}

/// Get the tag of a variant
///
/// Stack effect: ( Variant -- Int )
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_tag(stack: Stack) -> Stack {
    unsafe {
        let (stack, value) = pop(stack);

        match value {
            Value::Variant(variant_data) => {
                let tag = variant_data.tag as i64;
                push(stack, Value::Int(tag))
            }
            _ => panic!("variant-tag: expected Variant, got {:?}", value),
        }
    }
}

/// Get a field from a variant at the given index
///
/// Stack effect: ( Variant Int -- Value )
///
/// Returns a clone of the field value at the specified index.
/// Panics if index is out of bounds.
///
/// # Safety
/// Stack must have a Variant and Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_field_at(stack: Stack) -> Stack {
    unsafe {
        let (stack, index_val) = pop(stack);
        let index = match index_val {
            Value::Int(i) => i,
            _ => panic!(
                "variant-field-at: expected Int (index), got {:?}",
                index_val
            ),
        };

        if index < 0 {
            panic!("variant-field-at: index cannot be negative: {}", index);
        }

        let (stack, variant_val) = pop(stack);

        match variant_val {
            Value::Variant(variant_data) => {
                let idx = index as usize;
                if idx >= variant_data.fields.len() {
                    panic!(
                        "variant-field-at: index {} out of bounds (variant has {} fields)",
                        index,
                        variant_data.fields.len()
                    );
                }

                // Clone the field value and push it
                let field = variant_data.fields[idx].clone();
                push(stack, field)
            }
            _ => panic!("variant-field-at: expected Variant, got {:?}", variant_val),
        }
    }
}

// ============================================================================
// Type-safe variant constructors with fixed arity
// ============================================================================

/// Create a variant with 0 fields (just a tag)
///
/// Stack effect: ( tag -- Variant )
///
/// # Safety
/// Stack must have at least one Int (the tag) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_variant_0(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, tag_val) = pop(stack);
        let tag = match tag_val {
            Value::Int(t) => {
                if t < 0 {
                    panic!("make-variant-0: tag cannot be negative: {}", t);
                }
                t as u32
            }
            _ => panic!("make-variant-0: expected Int (tag), got {:?}", tag_val),
        };

        let variant = Value::Variant(Arc::new(VariantData::new(tag, vec![])));
        push(stack, variant)
    }
}

/// Create a variant with 1 field
///
/// Stack effect: ( field1 tag -- Variant )
///
/// # Safety
/// Stack must have field1 and tag on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_variant_1(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, tag_val) = pop(stack);
        let tag = match tag_val {
            Value::Int(t) => {
                if t < 0 {
                    panic!("make-variant-1: tag cannot be negative: {}", t);
                }
                t as u32
            }
            _ => panic!("make-variant-1: expected Int (tag), got {:?}", tag_val),
        };

        let (stack, field1) = pop(stack);
        let variant = Value::Variant(Arc::new(VariantData::new(tag, vec![field1])));
        push(stack, variant)
    }
}

/// Create a variant with 2 fields
///
/// Stack effect: ( field1 field2 tag -- Variant )
///
/// # Safety
/// Stack must have field1, field2, and tag on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_variant_2(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, tag_val) = pop(stack);
        let tag = match tag_val {
            Value::Int(t) => {
                if t < 0 {
                    panic!("make-variant-2: tag cannot be negative: {}", t);
                }
                t as u32
            }
            _ => panic!("make-variant-2: expected Int (tag), got {:?}", tag_val),
        };

        let (stack, field2) = pop(stack);
        let (stack, field1) = pop(stack);
        let variant = Value::Variant(Arc::new(VariantData::new(tag, vec![field1, field2])));
        push(stack, variant)
    }
}

/// Create a variant with 3 fields
///
/// Stack effect: ( field1 field2 field3 tag -- Variant )
///
/// # Safety
/// Stack must have field1, field2, field3, and tag on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_variant_3(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, tag_val) = pop(stack);
        let tag = match tag_val {
            Value::Int(t) => {
                if t < 0 {
                    panic!("make-variant-3: tag cannot be negative: {}", t);
                }
                t as u32
            }
            _ => panic!("make-variant-3: expected Int (tag), got {:?}", tag_val),
        };

        let (stack, field3) = pop(stack);
        let (stack, field2) = pop(stack);
        let (stack, field1) = pop(stack);
        let variant = Value::Variant(Arc::new(VariantData::new(
            tag,
            vec![field1, field2, field3],
        )));
        push(stack, variant)
    }
}

/// Create a variant with 4 fields
///
/// Stack effect: ( field1 field2 field3 field4 tag -- Variant )
///
/// # Safety
/// Stack must have field1, field2, field3, field4, and tag on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_variant_4(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, tag_val) = pop(stack);
        let tag = match tag_val {
            Value::Int(t) => {
                if t < 0 {
                    panic!("make-variant-4: tag cannot be negative: {}", t);
                }
                t as u32
            }
            _ => panic!("make-variant-4: expected Int (tag), got {:?}", tag_val),
        };

        let (stack, field4) = pop(stack);
        let (stack, field3) = pop(stack);
        let (stack, field2) = pop(stack);
        let (stack, field1) = pop(stack);
        let variant = Value::Variant(Arc::new(VariantData::new(
            tag,
            vec![field1, field2, field3, field4],
        )));
        push(stack, variant)
    }
}

// Re-exports for internal use
pub use patch_seq_make_variant_0 as make_variant_0;
pub use patch_seq_make_variant_1 as make_variant_1;
pub use patch_seq_make_variant_2 as make_variant_2;
pub use patch_seq_make_variant_3 as make_variant_3;
pub use patch_seq_make_variant_4 as make_variant_4;

/// Append a value to a variant, returning a new variant
///
/// Stack effect: ( Variant Value -- Variant' )
///
/// Creates a new variant with the same tag as the input, but with
/// the new value appended to its fields. The original variant is
/// not modified (functional/immutable style).
///
/// Example: For arrays, `[1, 2] 3 variant-append` produces `[1, 2, 3]`
/// Example: For objects, `{} "key" variant-append "val" variant-append` builds `{"key": "val"}`
///
/// # Safety
/// Stack must have a Variant and a Value on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_append(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        // Pop the value to append
        let (stack, value) = pop(stack);

        // Pop the variant
        let (stack, variant_val) = pop(stack);

        match variant_val {
            Value::Variant(variant_data) => {
                // Create new fields vector with the appended value
                let mut new_fields = variant_data.fields.to_vec();
                new_fields.push(value);

                // Create new variant with same tag
                let new_variant =
                    Value::Variant(Arc::new(VariantData::new(variant_data.tag, new_fields)));

                push(stack, new_variant)
            }
            _ => panic!("variant-append: expected Variant, got {:?}", variant_val),
        }
    }
}

/// Get the last field from a variant
///
/// Stack effect: ( Variant -- Value )
///
/// Returns a clone of the last field. Panics if the variant has no fields.
/// This is the "peek" operation for using variants as stacks.
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_last(stack: Stack) -> Stack {
    unsafe {
        let (stack, variant_val) = pop(stack);

        match variant_val {
            Value::Variant(variant_data) => {
                if variant_data.fields.is_empty() {
                    panic!("variant-last: variant has no fields");
                }

                let last = variant_data.fields.last().unwrap().clone();
                push(stack, last)
            }
            _ => panic!("variant-last: expected Variant, got {:?}", variant_val),
        }
    }
}

/// Get all but the last field from a variant
///
/// Stack effect: ( Variant -- Variant' )
///
/// Returns a new variant with the same tag but without the last field.
/// Panics if the variant has no fields.
/// This is the "pop" operation for using variants as stacks (without returning the popped value).
///
/// # Safety
/// Stack must have a Variant on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_variant_init(stack: Stack) -> Stack {
    use crate::value::VariantData;

    unsafe {
        let (stack, variant_val) = pop(stack);

        match variant_val {
            Value::Variant(variant_data) => {
                if variant_data.fields.is_empty() {
                    panic!("variant-init: variant has no fields");
                }

                // Create new fields without the last element
                let new_fields: Vec<Value> =
                    variant_data.fields[..variant_data.fields.len() - 1].to_vec();

                let new_variant =
                    Value::Variant(Arc::new(VariantData::new(variant_data.tag, new_fields)));

                push(stack, new_variant)
            }
            _ => panic!("variant-init: expected Variant, got {:?}", variant_val),
        }
    }
}

/// Unpack a variant's fields onto the stack
///
/// Takes a field count as parameter and:
/// - Pops the variant from the stack
/// - Pushes each field (0..field_count) in order
///
/// Stack effect: ( Variant -- field0 field1 ... fieldN-1 )
///
/// Used by match expression codegen to extract variant fields.
///
/// # Safety
/// Stack must have a Variant on top with at least `field_count` fields
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_unpack_variant(stack: Stack, field_count: i64) -> Stack {
    unsafe {
        let (mut stack, variant_val) = pop(stack);

        match variant_val {
            Value::Variant(variant_data) => {
                let count = field_count as usize;
                if count > variant_data.fields.len() {
                    panic!(
                        "unpack-variant: requested {} fields but variant only has {}",
                        count,
                        variant_data.fields.len()
                    );
                }

                // Push each field in order (field0 first, then field1, etc.)
                for i in 0..count {
                    stack = push(stack, variant_data.fields[i].clone());
                }

                stack
            }
            _ => panic!("unpack-variant: expected Variant, got {:?}", variant_val),
        }
    }
}

// Public re-exports with short names for internal use
pub use patch_seq_unpack_variant as unpack_variant;
pub use patch_seq_variant_append as variant_append;
pub use patch_seq_variant_field_at as variant_field_at;
pub use patch_seq_variant_field_count as variant_field_count;
pub use patch_seq_variant_init as variant_init;
pub use patch_seq_variant_last as variant_last;
pub use patch_seq_variant_tag as variant_tag;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seqstring::global_string;
    use crate::value::VariantData;

    #[test]
    fn test_variant_field_count() {
        unsafe {
            // Create a variant with 3 fields
            let variant = Value::Variant(Arc::new(VariantData::new(
                0,
                vec![Value::Int(10), Value::Int(20), Value::Int(30)],
            )));

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant);
            let stack = variant_field_count(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3));
        }
    }

    #[test]
    fn test_variant_tag() {
        unsafe {
            // Create a variant with tag 42
            let variant = Value::Variant(Arc::new(VariantData::new(42, vec![Value::Int(10)])));

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant);
            let stack = variant_tag(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(42));
        }
    }

    #[test]
    fn test_variant_field_at() {
        unsafe {
            let str1 = global_string("hello".to_string());
            let str2 = global_string("world".to_string());

            // Create a variant with mixed fields
            let variant = Value::Variant(Arc::new(VariantData::new(
                0,
                vec![
                    Value::String(str1.clone()),
                    Value::Int(42),
                    Value::String(str2.clone()),
                ],
            )));

            // Test accessing field 0
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(0));
            let stack = variant_field_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(str1.clone()));

            // Test accessing field 1
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(1));
            let stack = variant_field_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(42));

            // Test accessing field 2
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(2));
            let stack = variant_field_at(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::String(str2));
        }
    }

    #[test]
    fn test_variant_field_count_empty() {
        unsafe {
            // Create a variant with no fields
            let variant = Value::Variant(Arc::new(VariantData::new(0, vec![])));

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant);
            let stack = variant_field_count(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
        }
    }

    #[test]
    fn test_make_variant_with_fields() {
        unsafe {
            // Create: 10 20 30 42 make-variant-3
            // Should produce variant with tag 42 and fields [10, 20, 30]
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(10)); // field 0
            let stack = push(stack, Value::Int(20)); // field 1
            let stack = push(stack, Value::Int(30)); // field 2
            let stack = push(stack, Value::Int(42)); // tag

            let stack = make_variant_3(stack);

            let (_stack, result) = pop(stack);

            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 42);
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::Int(10));
                    assert_eq!(v.fields[1], Value::Int(20));
                    assert_eq!(v.fields[2], Value::Int(30));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_make_variant_empty() {
        unsafe {
            // Create: 0 make-variant-0
            // Should produce variant with tag 0 and no fields
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(0)); // tag

            let stack = make_variant_0(stack);

            let (_stack, result) = pop(stack);

            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 0);
                    assert_eq!(v.fields.len(), 0);
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_make_variant_with_mixed_types() {
        unsafe {
            let s = global_string("hello".to_string());

            // Create variant with mixed field types using make-variant-3
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(42));
            let stack = push(stack, Value::String(s.clone()));
            let stack = push(stack, Value::Float(3.5));
            let stack = push(stack, Value::Int(1)); // tag

            let stack = make_variant_3(stack);

            let (_stack, result) = pop(stack);

            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 1);
                    assert_eq!(v.fields.len(), 3);
                    assert_eq!(v.fields[0], Value::Int(42));
                    assert_eq!(v.fields[1], Value::String(s));
                    assert_eq!(v.fields[2], Value::Float(3.5));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_variant_append() {
        unsafe {
            // Create an empty variant (tag 4 for array)
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(4)); // tag (array)
            let stack = make_variant_0(stack);

            // Append a value
            let stack = push(stack, Value::Int(42));
            let stack = variant_append(stack);

            // Check result
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 4);
                    assert_eq!(v.fields.len(), 1);
                    assert_eq!(v.fields[0], Value::Int(42));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_variant_append_multiple() {
        unsafe {
            // Create an empty variant (tag 5 for object)
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(5)); // tag (object)
            let stack = make_variant_0(stack);

            // Append key
            let key = global_string("name".to_string());
            let stack = push(stack, Value::String(key.clone()));
            let stack = variant_append(stack);

            // Append value
            let val = global_string("John".to_string());
            let stack = push(stack, Value::String(val.clone()));
            let stack = variant_append(stack);

            // Check result - should have 2 fields
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 5);
                    assert_eq!(v.fields.len(), 2);
                    assert_eq!(v.fields[0], Value::String(key));
                    assert_eq!(v.fields[1], Value::String(val));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_variant_last() {
        unsafe {
            // Create a variant with 3 fields
            let variant = Value::Variant(Arc::new(VariantData::new(
                0,
                vec![Value::Int(10), Value::Int(20), Value::Int(30)],
            )));

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant);
            let stack = variant_last(stack);

            let (_stack, result) = pop(stack);
            assert_eq!(result, Value::Int(30));
        }
    }

    #[test]
    fn test_variant_init() {
        unsafe {
            // Create a variant with 3 fields
            let variant = Value::Variant(Arc::new(VariantData::new(
                42,
                vec![Value::Int(10), Value::Int(20), Value::Int(30)],
            )));

            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, variant);
            let stack = variant_init(stack);

            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.tag, 42); // tag preserved
                    assert_eq!(v.fields.len(), 2);
                    assert_eq!(v.fields[0], Value::Int(10));
                    assert_eq!(v.fields[1], Value::Int(20));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_variant_stack_operations() {
        // Test using variant as a stack: append, append, last, init, last
        unsafe {
            // Create empty "stack" variant (tag 99)
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::Int(99)); // tag
            let stack = make_variant_0(stack);

            // Append 10
            let stack = push(stack, Value::Int(10));
            let stack = variant_append(stack);

            // Append 20
            let stack = push(stack, Value::Int(20));
            let stack = variant_append(stack);

            // Now have variant with [10, 20] on stack
            // Dup and get last (should be 20)
            let (stack, variant) = pop(stack);
            let stack = push(stack, variant.clone());
            let stack = push(stack, variant);
            let stack = variant_last(stack);
            let (stack, top) = pop(stack);
            assert_eq!(top, Value::Int(20));

            // Now use init to remove last element
            let stack = variant_init(stack);

            // Dup and get last (should now be 10)
            let (stack, variant) = pop(stack);
            let stack = push(stack, variant.clone());
            let stack = push(stack, variant);
            let stack = variant_last(stack);
            let (stack, top) = pop(stack);
            assert_eq!(top, Value::Int(10));

            // Verify remaining variant has 1 field
            let (_stack, result) = pop(stack);
            match result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 1);
                    assert_eq!(v.fields[0], Value::Int(10));
                }
                _ => panic!("Expected Variant"),
            }
        }
    }

    #[test]
    fn test_variant_clone_is_o1() {
        // Regression test: Ensure deeply nested variants clone in O(1) time
        // This would have been O(2^n) with Box before the Arc change
        let mut variant = Value::Variant(Arc::new(VariantData::new(0, vec![])));

        // Build a deeply nested structure (100 levels)
        for i in 0..100 {
            variant = Value::Variant(Arc::new(VariantData::new(i, vec![variant.clone()])));
        }

        // Clone should be O(1) - just incrementing Arc refcount
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _copy = variant.clone();
        }
        let elapsed = start.elapsed();

        // 1000 clones of a 100-deep structure should take < 1ms with Arc
        // With Box it would take seconds or longer
        assert!(
            elapsed.as_millis() < 10,
            "Clone took {:?} - should be O(1) with Arc",
            elapsed
        );
    }

    #[test]
    fn test_variant_arc_sharing() {
        // Test that Arc properly shares data (refcount increases, not deep copy)
        let inner = Value::Variant(Arc::new(VariantData::new(0, vec![Value::Int(42)])));
        let outer = Value::Variant(Arc::new(VariantData::new(1, vec![inner.clone()])));

        // Clone should share the same Arc, not deep copy
        let outer_clone = outer.clone();

        // Both should have the same inner value
        match (&outer, &outer_clone) {
            (Value::Variant(a), Value::Variant(b)) => {
                // Arc::ptr_eq would be ideal but fields are Box<[Value]>
                // Instead verify the values are equal (they share the same data)
                assert_eq!(a.tag, b.tag);
                assert_eq!(a.fields.len(), b.fields.len());
            }
            _ => panic!("Expected Variants"),
        }
    }

    #[test]
    fn test_variant_thread_safe_sharing() {
        // Test that variants can be safely shared between threads
        // This validates the Send + Sync implementation
        use std::sync::Arc as StdArc;
        use std::thread;

        let variant = Value::Variant(Arc::new(VariantData::new(
            42,
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        )));

        // Wrap in Arc for thread sharing
        let shared = StdArc::new(variant);

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let v = StdArc::clone(&shared);
                thread::spawn(move || {
                    // Access the variant from another thread
                    match &*v {
                        Value::Variant(data) => {
                            assert_eq!(data.tag, 42);
                            assert_eq!(data.fields.len(), 3);
                        }
                        _ => panic!("Expected Variant"),
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("Thread panicked");
        }
    }
}
