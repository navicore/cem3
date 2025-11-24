//! Variant field access operations for Seq
//!
//! Provides runtime functions for accessing variant fields, tags, and metadata.
//! These are used to work with composite data created by operations like string-split.

use crate::stack::{Stack, pop, push};
use crate::value::Value;

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

// Public re-exports with short names for internal use
pub use patch_seq_variant_field_at as variant_field_at;
pub use patch_seq_variant_field_count as variant_field_count;
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
            let variant = Value::Variant(Box::new(VariantData::new(
                0,
                vec![Value::Int(10), Value::Int(20), Value::Int(30)],
            )));

            let stack = std::ptr::null_mut();
            let stack = push(stack, variant);
            let stack = variant_field_count(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(3));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_variant_tag() {
        unsafe {
            // Create a variant with tag 42
            let variant = Value::Variant(Box::new(VariantData::new(42, vec![Value::Int(10)])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, variant);
            let stack = variant_tag(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(42));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_variant_field_at() {
        unsafe {
            let str1 = global_string("hello".to_string());
            let str2 = global_string("world".to_string());

            // Create a variant with mixed fields
            let variant = Value::Variant(Box::new(VariantData::new(
                0,
                vec![
                    Value::String(str1.clone()),
                    Value::Int(42),
                    Value::String(str2.clone()),
                ],
            )));

            // Test accessing field 0
            let stack = std::ptr::null_mut();
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(0));
            let stack = variant_field_at(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::String(str1.clone()));
            assert!(stack.is_null());

            // Test accessing field 1
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(1));
            let stack = variant_field_at(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(42));
            assert!(stack.is_null());

            // Test accessing field 2
            let stack = push(stack, variant.clone());
            let stack = push(stack, Value::Int(2));
            let stack = variant_field_at(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::String(str2));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_variant_field_count_empty() {
        unsafe {
            // Create a variant with no fields
            let variant = Value::Variant(Box::new(VariantData::new(0, vec![])));

            let stack = std::ptr::null_mut();
            let stack = push(stack, variant);
            let stack = variant_field_count(stack);

            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }
}
