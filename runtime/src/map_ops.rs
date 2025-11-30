//! Map operations for Seq
//!
//! Dictionary/hash map operations with O(1) lookup.
//! Maps use hashable keys (Int, String, Bool) and can store any Value.
//!
//! # Examples
//!
//! ```seq
//! # Create empty map and add entries
//! make-map "name" "Alice" map-set "age" 30 map-set
//!
//! # Get value by key
//! my-map "name" map-get  # -> "Alice"
//!
//! # Check if key exists
//! my-map "email" map-has?  # -> 0 (false)
//!
//! # Get keys/values as lists
//! my-map map-keys    # -> ["name", "age"]
//! my-map map-values  # -> ["Alice", 30]
//! ```

use crate::stack::{Stack, pop, push};
use crate::value::{MapKey, Value, VariantData};

/// Create an empty map
///
/// Stack effect: ( -- Map )
///
/// # Safety
/// Stack can be any valid stack pointer (including null for empty stack)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_make_map(stack: Stack) -> Stack {
    unsafe { push(stack, Value::Map(Box::default())) }
}

/// Get a value from the map by key
///
/// Stack effect: ( Map key -- value )
///
/// Panics if the key is not found or if the key type is not hashable.
///
/// # Safety
/// Stack must have a hashable key on top and a Map below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_get(stack: Stack) -> Stack {
    unsafe {
        // Pop key
        let (stack, key_val) = pop(stack);
        let key = MapKey::from_value(&key_val).unwrap_or_else(|| {
            panic!(
                "map-get: key must be Int, String, or Bool, got {:?}",
                key_val
            )
        });

        // Pop map
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-get: expected Map, got {:?}", map_val),
        };

        // Look up value
        let value = map
            .get(&key)
            .unwrap_or_else(|| panic!("map-get: key {:?} not found", key))
            .clone();

        push(stack, value)
    }
}

/// Get a value from the map by key, with error handling
///
/// Stack effect: ( Map key -- value Int )
///
/// Returns (value 1) if found, or (0 0) if not found.
/// Panics if the key type is not hashable.
///
/// # Safety
/// Stack must have a hashable key on top and a Map below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_get_safe(stack: Stack) -> Stack {
    unsafe {
        // Pop key
        let (stack, key_val) = pop(stack);
        let key = MapKey::from_value(&key_val).unwrap_or_else(|| {
            panic!(
                "map-get-safe: key must be Int, String, or Bool, got {:?}",
                key_val
            )
        });

        // Pop map
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-get-safe: expected Map, got {:?}", map_val),
        };

        // Look up value
        match map.get(&key) {
            Some(value) => {
                let stack = push(stack, value.clone());
                push(stack, Value::Int(1))
            }
            None => {
                let stack = push(stack, Value::Int(0)); // placeholder value
                push(stack, Value::Int(0)) // not found
            }
        }
    }
}

/// Set a key-value pair in the map (functional style)
///
/// Stack effect: ( Map key value -- Map )
///
/// Returns a new map with the key-value pair added/updated.
/// Panics if the key type is not hashable.
///
/// # Safety
/// Stack must have value on top, key below, and Map at third position
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_set(stack: Stack) -> Stack {
    unsafe {
        // Pop value
        let (stack, value) = pop(stack);

        // Pop key
        let (stack, key_val) = pop(stack);
        let key = MapKey::from_value(&key_val).unwrap_or_else(|| {
            panic!(
                "map-set: key must be Int, String, or Bool, got {:?}",
                key_val
            )
        });

        // Pop map
        let (stack, map_val) = pop(stack);
        let mut map = match map_val {
            Value::Map(m) => *m,
            _ => panic!("map-set: expected Map, got {:?}", map_val),
        };

        // Insert key-value pair
        map.insert(key, value);

        push(stack, Value::Map(Box::new(map)))
    }
}

/// Check if a key exists in the map
///
/// Stack effect: ( Map key -- Int )
///
/// Returns 1 if the key exists, 0 otherwise.
/// Panics if the key type is not hashable.
///
/// # Safety
/// Stack must have a hashable key on top and a Map below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_has(stack: Stack) -> Stack {
    unsafe {
        // Pop key
        let (stack, key_val) = pop(stack);
        let key = MapKey::from_value(&key_val).unwrap_or_else(|| {
            panic!(
                "map-has?: key must be Int, String, or Bool, got {:?}",
                key_val
            )
        });

        // Pop map
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-has?: expected Map, got {:?}", map_val),
        };

        let has_key = if map.contains_key(&key) { 1i64 } else { 0i64 };
        push(stack, Value::Int(has_key))
    }
}

/// Remove a key from the map (functional style)
///
/// Stack effect: ( Map key -- Map )
///
/// Returns a new map without the specified key.
/// If the key doesn't exist, returns the map unchanged.
/// Panics if the key type is not hashable.
///
/// # Safety
/// Stack must have a hashable key on top and a Map below
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_remove(stack: Stack) -> Stack {
    unsafe {
        // Pop key
        let (stack, key_val) = pop(stack);
        let key = MapKey::from_value(&key_val).unwrap_or_else(|| {
            panic!(
                "map-remove: key must be Int, String, or Bool, got {:?}",
                key_val
            )
        });

        // Pop map
        let (stack, map_val) = pop(stack);
        let mut map = match map_val {
            Value::Map(m) => *m,
            _ => panic!("map-remove: expected Map, got {:?}", map_val),
        };

        // Remove key (if present)
        map.remove(&key);

        push(stack, Value::Map(Box::new(map)))
    }
}

/// Get all keys from the map as a list
///
/// Stack effect: ( Map -- Variant )
///
/// Returns a Variant containing all keys in the map.
/// Note: Order is not guaranteed (HashMap iteration order).
///
/// # Safety
/// Stack must have a Map on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_keys(stack: Stack) -> Stack {
    unsafe {
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-keys: expected Map, got {:?}", map_val),
        };

        let keys: Vec<Value> = map.keys().map(|k| k.to_value()).collect();
        let variant = Value::Variant(Box::new(VariantData::new(0, keys)));
        push(stack, variant)
    }
}

/// Get all values from the map as a list
///
/// Stack effect: ( Map -- Variant )
///
/// Returns a Variant containing all values in the map.
/// Note: Order is not guaranteed (HashMap iteration order).
///
/// # Safety
/// Stack must have a Map on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_values(stack: Stack) -> Stack {
    unsafe {
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-values: expected Map, got {:?}", map_val),
        };

        let values: Vec<Value> = map.values().cloned().collect();
        let variant = Value::Variant(Box::new(VariantData::new(0, values)));
        push(stack, variant)
    }
}

/// Get the number of entries in the map
///
/// Stack effect: ( Map -- Int )
///
/// # Safety
/// Stack must have a Map on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_size(stack: Stack) -> Stack {
    unsafe {
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-size: expected Map, got {:?}", map_val),
        };

        push(stack, Value::Int(map.len() as i64))
    }
}

/// Check if the map is empty
///
/// Stack effect: ( Map -- Int )
///
/// Returns 1 if the map has no entries, 0 otherwise.
///
/// # Safety
/// Stack must have a Map on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_map_empty(stack: Stack) -> Stack {
    unsafe {
        let (stack, map_val) = pop(stack);
        let map = match map_val {
            Value::Map(m) => m,
            _ => panic!("map-empty?: expected Map, got {:?}", map_val),
        };

        let is_empty = if map.is_empty() { 1i64 } else { 0i64 };
        push(stack, Value::Int(is_empty))
    }
}

// Public re-exports
pub use patch_seq_make_map as make_map;
pub use patch_seq_map_empty as map_empty;
pub use patch_seq_map_get as map_get;
pub use patch_seq_map_get_safe as map_get_safe;
pub use patch_seq_map_has as map_has;
pub use patch_seq_map_keys as map_keys;
pub use patch_seq_map_remove as map_remove;
pub use patch_seq_map_set as map_set;
pub use patch_seq_map_size as map_size;
pub use patch_seq_map_values as map_values;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_make_map() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::Map(m) => assert!(m.is_empty()),
                _ => panic!("Expected Map"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_set_and_get() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::String("name".into()));
            let stack = push(stack, Value::String("Alice".into()));
            let stack = map_set(stack);

            // Get the value back
            let stack = push(stack, Value::String("name".into()));
            let stack = map_get(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "Alice"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_set_with_int_key() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::Int(42));
            let stack = push(stack, Value::String("answer".into()));
            let stack = map_set(stack);

            let stack = push(stack, Value::Int(42));
            let stack = map_get(stack);

            let (stack, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "answer"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_has() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::String("key".into()));
            let stack = push(stack, Value::Int(100));
            let stack = map_set(stack);

            // Check existing key (dup map first since map_has consumes it)
            let stack = crate::stack::dup(stack);
            let stack = push(stack, Value::String("key".into()));
            let stack = map_has(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));

            // Check non-existing key (map is still on stack)
            let stack = push(stack, Value::String("missing".into()));
            let stack = map_has(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_remove() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::String("a".into()));
            let stack = push(stack, Value::Int(1));
            let stack = map_set(stack);
            let stack = push(stack, Value::String("b".into()));
            let stack = push(stack, Value::Int(2));
            let stack = map_set(stack);

            // Remove "a"
            let stack = push(stack, Value::String("a".into()));
            let stack = map_remove(stack);

            // Check "a" is gone (dup map first since map_has consumes it)
            let stack = crate::stack::dup(stack);
            let stack = push(stack, Value::String("a".into()));
            let stack = map_has(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));

            // Check "b" is still there (map is still on stack)
            let stack = push(stack, Value::String("b".into()));
            let stack = map_has(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_size() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);

            // Empty map
            let stack = map_size(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));

            // Add entries
            let stack = make_map(stack);
            let stack = push(stack, Value::String("a".into()));
            let stack = push(stack, Value::Int(1));
            let stack = map_set(stack);
            let stack = push(stack, Value::String("b".into()));
            let stack = push(stack, Value::Int(2));
            let stack = map_set(stack);

            let stack = map_size(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(2));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_empty() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);

            let stack = map_empty(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(1));

            // Non-empty
            let stack = make_map(stack);
            let stack = push(stack, Value::String("key".into()));
            let stack = push(stack, Value::Int(1));
            let stack = map_set(stack);

            let stack = map_empty(stack);
            let (stack, result) = pop(stack);
            assert_eq!(result, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_keys_and_values() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::String("x".into()));
            let stack = push(stack, Value::Int(10));
            let stack = map_set(stack);
            let stack = push(stack, Value::String("y".into()));
            let stack = push(stack, Value::Int(20));
            let stack = map_set(stack);

            // Get keys
            let stack = crate::stack::dup(stack); // Keep map for values test
            let stack = map_keys(stack);
            let (stack, keys_result) = pop(stack);
            match keys_result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 2);
                    // Keys are "x" and "y" but order is not guaranteed
                }
                _ => panic!("Expected Variant"),
            }

            // Get values
            let stack = map_values(stack);
            let (stack, values_result) = pop(stack);
            match values_result {
                Value::Variant(v) => {
                    assert_eq!(v.fields.len(), 2);
                    // Values are 10 and 20 but order is not guaranteed
                }
                _ => panic!("Expected Variant"),
            }
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_get_safe_found() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::String("key".into()));
            let stack = push(stack, Value::Int(42));
            let stack = map_set(stack);

            let stack = push(stack, Value::String("key".into()));
            let stack = map_get_safe(stack);

            let (stack, flag) = pop(stack);
            let (stack, value) = pop(stack);
            assert_eq!(flag, Value::Int(1));
            assert_eq!(value, Value::Int(42));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_get_safe_not_found() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);

            let stack = push(stack, Value::String("missing".into()));
            let stack = map_get_safe(stack);

            let (stack, flag) = pop(stack);
            let (stack, _value) = pop(stack); // placeholder
            assert_eq!(flag, Value::Int(0));
            assert!(stack.is_null());
        }
    }

    #[test]
    fn test_map_with_bool_key() {
        unsafe {
            let stack = std::ptr::null_mut();
            let stack = make_map(stack);
            let stack = push(stack, Value::Bool(true));
            let stack = push(stack, Value::String("yes".into()));
            let stack = map_set(stack);
            let stack = push(stack, Value::Bool(false));
            let stack = push(stack, Value::String("no".into()));
            let stack = map_set(stack);

            let stack = push(stack, Value::Bool(true));
            let stack = map_get(stack);
            let (stack, result) = pop(stack);
            match result {
                Value::String(s) => assert_eq!(s.as_str(), "yes"),
                _ => panic!("Expected String"),
            }
            assert!(stack.is_null());
        }
    }
}
