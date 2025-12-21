//! Serialization of Seq Values
//!
//! This module provides a serializable representation of Seq runtime values.
//! It enables Value persistence and exchange with external systems.
//!
//! # Use Cases
//!
//! - **Actor persistence**: Event sourcing and state snapshots
//! - **Data pipelines**: Arrow/Parquet integration
//! - **IPC**: Message passing between processes
//! - **Storage**: Database and file persistence
//!
//! # Why TypedValue?
//!
//! The runtime `Value` type contains arena-allocated strings (`SeqString`)
//! which aren't directly serializable. `TypedValue` uses owned `String`s
//! and can be serialized with serde/bincode.
//!
//! # Why BTreeMap instead of HashMap?
//!
//! `TypedValue::Map` uses `BTreeMap` (not `HashMap`) for deterministic serialization.
//! This ensures that the same logical map always serializes to identical bytes,
//! which is important for:
//! - Content-addressable storage (hashing serialized data)
//! - Reproducible snapshots for testing and debugging
//! - Consistent behavior across runs
//!
//! The O(n log n) insertion overhead is acceptable since serialization is
//! typically infrequent (snapshots, persistence) rather than on the hot path.
//!
//! # Performance
//!
//! Uses bincode for fast, compact binary serialization.
//! For debugging, use `TypedValue::to_debug_string()`.

use crate::seqstring::global_string;
use crate::value::{MapKey as RuntimeMapKey, Value, VariantData};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Error during serialization/deserialization
#[derive(Debug)]
pub enum SerializeError {
    /// Cannot serialize quotations (code)
    QuotationNotSerializable,
    /// Cannot serialize closures
    ClosureNotSerializable,
    /// Cannot serialize channels (runtime state)
    ChannelNotSerializable,
    /// Bincode encoding/decoding error (preserves original error for debugging)
    BincodeError(Box<bincode::Error>),
    /// Invalid data structure
    InvalidData(String),
    /// Non-finite float (NaN or Infinity)
    NonFiniteFloat(f64),
}

impl std::fmt::Display for SerializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializeError::QuotationNotSerializable => {
                write!(f, "Quotations cannot be serialized - code is not data")
            }
            SerializeError::ClosureNotSerializable => {
                write!(f, "Closures cannot be serialized - code is not data")
            }
            SerializeError::ChannelNotSerializable => {
                write!(f, "Channels cannot be serialized - runtime state")
            }
            SerializeError::BincodeError(e) => write!(f, "Bincode error: {}", e),
            SerializeError::InvalidData(msg) => write!(f, "Invalid data: {}", msg),
            SerializeError::NonFiniteFloat(v) => {
                write!(f, "Cannot serialize non-finite float: {}", v)
            }
        }
    }
}

impl std::error::Error for SerializeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerializeError::BincodeError(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

impl From<bincode::Error> for SerializeError {
    fn from(e: bincode::Error) -> Self {
        SerializeError::BincodeError(Box::new(e))
    }
}

/// Serializable map key types
///
/// Subset of TypedValue that can be used as map keys.
/// Mirrors runtime `MapKey` but with owned strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TypedMapKey {
    Int(i64),
    Bool(bool),
    String(String),
}

impl TypedMapKey {
    /// Convert to a TypedValue
    pub fn to_typed_value(&self) -> TypedValue {
        match self {
            TypedMapKey::Int(v) => TypedValue::Int(*v),
            TypedMapKey::Bool(v) => TypedValue::Bool(*v),
            TypedMapKey::String(v) => TypedValue::String(v.clone()),
        }
    }

    /// Convert from runtime MapKey
    pub fn from_runtime(key: &RuntimeMapKey) -> Self {
        match key {
            RuntimeMapKey::Int(v) => TypedMapKey::Int(*v),
            RuntimeMapKey::Bool(v) => TypedMapKey::Bool(*v),
            RuntimeMapKey::String(s) => TypedMapKey::String(s.as_str().to_string()),
        }
    }

    /// Convert to runtime MapKey (requires global string allocation)
    pub fn to_runtime(&self) -> RuntimeMapKey {
        match self {
            TypedMapKey::Int(v) => RuntimeMapKey::Int(*v),
            TypedMapKey::Bool(v) => RuntimeMapKey::Bool(*v),
            TypedMapKey::String(s) => RuntimeMapKey::String(global_string(s.clone())),
        }
    }
}

/// Serializable representation of Seq Values
///
/// This type mirrors `Value` but uses owned data suitable for serialization.
/// Quotations and closures cannot be serialized (they contain code, not data).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TypedValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    /// Map with typed keys and values
    Map(BTreeMap<TypedMapKey, TypedValue>),
    /// Variant with tag and fields
    Variant {
        tag: u32,
        fields: Vec<TypedValue>,
    },
}

impl TypedValue {
    /// Convert from runtime Value
    ///
    /// Returns error if Value contains:
    /// - Code (Quotation/Closure) - not serializable
    /// - Non-finite floats (NaN/Infinity) - could cause logic issues
    pub fn from_value(value: &Value) -> Result<Self, SerializeError> {
        match value {
            Value::Int(v) => Ok(TypedValue::Int(*v)),
            Value::Float(v) => {
                if !v.is_finite() {
                    return Err(SerializeError::NonFiniteFloat(*v));
                }
                Ok(TypedValue::Float(*v))
            }
            Value::Bool(v) => Ok(TypedValue::Bool(*v)),
            Value::String(s) => Ok(TypedValue::String(s.as_str().to_string())),
            Value::Map(map) => {
                let mut typed_map = BTreeMap::new();
                for (k, v) in map.iter() {
                    let typed_key = TypedMapKey::from_runtime(k);
                    let typed_value = TypedValue::from_value(v)?;
                    typed_map.insert(typed_key, typed_value);
                }
                Ok(TypedValue::Map(typed_map))
            }
            Value::Variant(data) => {
                let mut typed_fields = Vec::with_capacity(data.fields.len());
                for field in data.fields.iter() {
                    typed_fields.push(TypedValue::from_value(field)?);
                }
                Ok(TypedValue::Variant {
                    tag: data.tag,
                    fields: typed_fields,
                })
            }
            Value::Quotation { .. } => Err(SerializeError::QuotationNotSerializable),
            Value::Closure { .. } => Err(SerializeError::ClosureNotSerializable),
            Value::Channel(_) => Err(SerializeError::ChannelNotSerializable),
        }
    }

    /// Convert to runtime Value
    ///
    /// Note: Strings are allocated as global strings (not arena)
    /// to ensure they outlive any strand context.
    pub fn to_value(&self) -> Value {
        match self {
            TypedValue::Int(v) => Value::Int(*v),
            TypedValue::Float(v) => Value::Float(*v),
            TypedValue::Bool(v) => Value::Bool(*v),
            TypedValue::String(s) => Value::String(global_string(s.clone())),
            TypedValue::Map(map) => {
                let mut runtime_map = HashMap::new();
                for (k, v) in map.iter() {
                    runtime_map.insert(k.to_runtime(), v.to_value());
                }
                Value::Map(Box::new(runtime_map))
            }
            TypedValue::Variant { tag, fields } => {
                let runtime_fields: Vec<Value> = fields.iter().map(|f| f.to_value()).collect();
                Value::Variant(Arc::new(VariantData::new(*tag, runtime_fields)))
            }
        }
    }

    /// Try to convert to a map key (fails for Float, Map, Variant)
    pub fn to_map_key(&self) -> Result<TypedMapKey, SerializeError> {
        match self {
            TypedValue::Int(v) => Ok(TypedMapKey::Int(*v)),
            TypedValue::Bool(v) => Ok(TypedMapKey::Bool(*v)),
            TypedValue::String(v) => Ok(TypedMapKey::String(v.clone())),
            TypedValue::Float(_) => Err(SerializeError::InvalidData(
                "Float cannot be a map key".to_string(),
            )),
            TypedValue::Map(_) => Err(SerializeError::InvalidData(
                "Map cannot be a map key".to_string(),
            )),
            TypedValue::Variant { .. } => Err(SerializeError::InvalidData(
                "Variant cannot be a map key".to_string(),
            )),
        }
    }

    /// Serialize to binary format (bincode)
    pub fn to_bytes(&self) -> Result<Vec<u8>, SerializeError> {
        bincode::serialize(self).map_err(SerializeError::from)
    }

    /// Deserialize from binary format (bincode)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SerializeError> {
        bincode::deserialize(bytes).map_err(SerializeError::from)
    }

    /// Convert to human-readable debug string
    pub fn to_debug_string(&self) -> String {
        match self {
            TypedValue::Int(v) => format!("{}", v),
            TypedValue::Float(v) => format!("{}", v),
            TypedValue::Bool(v) => format!("{}", v),
            TypedValue::String(v) => format!("{:?}", v),
            TypedValue::Map(m) => {
                let entries: Vec<String> = m
                    .iter()
                    .map(|(k, v)| format!("{}: {}", key_to_debug_string(k), v.to_debug_string()))
                    .collect();
                format!("{{ {} }}", entries.join(", "))
            }
            TypedValue::Variant { tag, fields } => {
                if fields.is_empty() {
                    format!("(Variant#{})", tag)
                } else {
                    let field_strs: Vec<String> =
                        fields.iter().map(|f| f.to_debug_string()).collect();
                    format!("(Variant#{} {})", tag, field_strs.join(" "))
                }
            }
        }
    }
}

fn key_to_debug_string(key: &TypedMapKey) -> String {
    match key {
        TypedMapKey::Int(v) => format!("{}", v),
        TypedMapKey::Bool(v) => format!("{}", v),
        TypedMapKey::String(v) => format!("{:?}", v),
    }
}

/// Extension trait for Value to add serialization methods
pub trait ValueSerialize {
    /// Convert to serializable TypedValue
    fn to_typed(&self) -> Result<TypedValue, SerializeError>;

    /// Serialize directly to bytes
    fn to_bytes(&self) -> Result<Vec<u8>, SerializeError>;
}

impl ValueSerialize for Value {
    fn to_typed(&self) -> Result<TypedValue, SerializeError> {
        TypedValue::from_value(self)
    }

    fn to_bytes(&self) -> Result<Vec<u8>, SerializeError> {
        TypedValue::from_value(self)?.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seqstring::global_string;

    #[test]
    fn test_int_roundtrip() {
        let value = Value::Int(42);
        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();
        assert_eq!(value, back);
    }

    #[test]
    fn test_float_roundtrip() {
        let value = Value::Float(1.23456);
        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();
        assert_eq!(value, back);
    }

    #[test]
    fn test_bool_roundtrip() {
        let value = Value::Bool(true);
        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();
        assert_eq!(value, back);
    }

    #[test]
    fn test_string_roundtrip() {
        let value = Value::String(global_string("hello".to_string()));
        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();
        // Compare string contents (not pointer equality)
        match (&value, &back) {
            (Value::String(a), Value::String(b)) => assert_eq!(a.as_str(), b.as_str()),
            _ => panic!("Expected strings"),
        }
    }

    #[test]
    fn test_map_roundtrip() {
        let mut map = HashMap::new();
        map.insert(
            RuntimeMapKey::String(global_string("key".to_string())),
            Value::Int(42),
        );
        map.insert(RuntimeMapKey::Int(1), Value::Bool(true));

        let value = Value::Map(Box::new(map));
        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();

        // Verify map contents
        if let Value::Map(m) = back {
            assert_eq!(m.len(), 2);
        } else {
            panic!("Expected map");
        }
    }

    #[test]
    fn test_variant_roundtrip() {
        let data = VariantData::new(1, vec![Value::Int(100), Value::Bool(false)]);
        let value = Value::Variant(Arc::new(data));

        let typed = TypedValue::from_value(&value).unwrap();
        let back = typed.to_value();

        if let Value::Variant(v) = back {
            assert_eq!(v.tag, 1);
            assert_eq!(v.fields.len(), 2);
        } else {
            panic!("Expected variant");
        }
    }

    #[test]
    fn test_quotation_not_serializable() {
        let value = Value::Quotation {
            wrapper: 12345,
            impl_: 12345,
        };
        let result = TypedValue::from_value(&value);
        assert!(matches!(
            result,
            Err(SerializeError::QuotationNotSerializable)
        ));
    }

    #[test]
    fn test_closure_not_serializable() {
        use std::sync::Arc;
        let value = Value::Closure {
            fn_ptr: 12345,
            env: Arc::from(vec![Value::Int(1)].into_boxed_slice()),
        };
        let result = TypedValue::from_value(&value);
        assert!(matches!(
            result,
            Err(SerializeError::ClosureNotSerializable)
        ));
    }

    #[test]
    fn test_bytes_roundtrip() {
        let typed = TypedValue::Map(BTreeMap::from([
            (TypedMapKey::String("x".to_string()), TypedValue::Int(10)),
            (TypedMapKey::Int(42), TypedValue::Bool(true)),
        ]));

        let bytes = typed.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(typed, parsed);
    }

    #[test]
    fn test_bincode_is_compact() {
        let typed = TypedValue::Int(42);
        let bytes = typed.to_bytes().unwrap();
        assert!(
            bytes.len() < 20,
            "Expected compact encoding, got {} bytes",
            bytes.len()
        );
    }

    #[test]
    fn test_debug_string() {
        let typed = TypedValue::String("hello".to_string());
        assert_eq!(typed.to_debug_string(), "\"hello\"");

        let typed = TypedValue::Int(42);
        assert_eq!(typed.to_debug_string(), "42");
    }

    #[test]
    fn test_nested_structure() {
        // Create nested map with variant
        let inner_variant = TypedValue::Variant {
            tag: 2,
            fields: vec![TypedValue::String("inner".to_string())],
        };

        let mut inner_map = BTreeMap::new();
        inner_map.insert(TypedMapKey::String("nested".to_string()), inner_variant);

        let outer = TypedValue::Map(inner_map);

        let bytes = outer.to_bytes().unwrap();
        let parsed = TypedValue::from_bytes(&bytes).unwrap();
        assert_eq!(outer, parsed);
    }

    #[test]
    fn test_nan_not_serializable() {
        let value = Value::Float(f64::NAN);
        let result = TypedValue::from_value(&value);
        assert!(matches!(result, Err(SerializeError::NonFiniteFloat(_))));
    }

    #[test]
    fn test_infinity_not_serializable() {
        let value = Value::Float(f64::INFINITY);
        let result = TypedValue::from_value(&value);
        assert!(matches!(result, Err(SerializeError::NonFiniteFloat(_))));

        let value = Value::Float(f64::NEG_INFINITY);
        let result = TypedValue::from_value(&value);
        assert!(matches!(result, Err(SerializeError::NonFiniteFloat(_))));
    }

    #[test]
    fn test_corrupted_data_returns_error() {
        // Random bytes that aren't valid bincode
        let corrupted = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        let result = TypedValue::from_bytes(&corrupted);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_data_returns_error() {
        let result = TypedValue::from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_data_returns_error() {
        // Serialize valid data, then truncate
        let typed = TypedValue::String("hello world".to_string());
        let bytes = typed.to_bytes().unwrap();
        let truncated = &bytes[..bytes.len() / 2];
        let result = TypedValue::from_bytes(truncated);
        assert!(result.is_err());
    }
}
