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
//!
//! # Byte-cleanliness boundary
//!
//! `TypedValue::String` and `TypedMapKey::String` hold owned `String` —
//! UTF-8 by definition. Conversion from a runtime `Value::String`
//! (which is byte-clean and may carry arbitrary bytes) goes through
//! `as_str_or_empty()`: invalid UTF-8 collapses to the empty string.
//! That is the deliberate, narrow contract of this module — it serves
//! the *text-shaped* payloads of actor persistence, IPC, and
//! Arrow/Parquet pipelines, not arbitrary binary blobs.
//!
//! Programs that need to persist binary `String` payloads should
//! base64- or hex-encode them at the Seq layer before handing them
//! to `serialize`, or use a binary-aware transport (file slurp/spit,
//! HTTP body, channel send) which retains bytes verbatim.

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
    /// Bincode encoding error
    BincodeEncodeError(bincode::error::EncodeError),
    /// Bincode decoding error
    BincodeDecodeError(bincode::error::DecodeError),
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
            SerializeError::BincodeEncodeError(e) => write!(f, "Bincode encode error: {}", e),
            SerializeError::BincodeDecodeError(e) => write!(f, "Bincode decode error: {}", e),
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
            SerializeError::BincodeEncodeError(e) => Some(e),
            SerializeError::BincodeDecodeError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<bincode::error::EncodeError> for SerializeError {
    fn from(e: bincode::error::EncodeError) -> Self {
        SerializeError::BincodeEncodeError(e)
    }
}

impl From<bincode::error::DecodeError> for SerializeError {
    fn from(e: bincode::error::DecodeError) -> Self {
        SerializeError::BincodeDecodeError(e)
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
            RuntimeMapKey::String(s) => TypedMapKey::String(s.as_str_or_empty().to_string()),
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
    /// Symbol (interned identifier)
    Symbol(String),
    /// Map with typed keys and values
    Map(BTreeMap<TypedMapKey, TypedValue>),
    /// Variant with tag (symbol name) and fields
    Variant {
        tag: String,
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
            Value::String(s) => Ok(TypedValue::String(s.as_str_or_empty().to_string())),
            Value::Symbol(s) => Ok(TypedValue::Symbol(s.as_str_or_empty().to_string())),
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
                    tag: data.tag.as_str_or_empty().to_string(),
                    fields: typed_fields,
                })
            }
            Value::Quotation { .. } => Err(SerializeError::QuotationNotSerializable),
            Value::Closure { .. } => Err(SerializeError::ClosureNotSerializable),
            Value::Channel(_) => Err(SerializeError::ChannelNotSerializable),
            Value::WeaveCtx { .. } => Err(SerializeError::ChannelNotSerializable), // Weaves contain channels
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
            TypedValue::Symbol(s) => Value::Symbol(global_string(s.clone())),
            TypedValue::Map(map) => {
                let mut runtime_map = HashMap::new();
                for (k, v) in map.iter() {
                    runtime_map.insert(k.to_runtime(), v.to_value());
                }
                Value::Map(Box::new(runtime_map))
            }
            TypedValue::Variant { tag, fields } => {
                let runtime_fields: Vec<Value> = fields.iter().map(|f| f.to_value()).collect();
                Value::Variant(Arc::new(VariantData::new(
                    global_string(tag.clone()),
                    runtime_fields,
                )))
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
            TypedValue::Symbol(v) => Ok(TypedMapKey::String(v.clone())),
        }
    }

    /// Serialize to binary format (bincode)
    pub fn to_bytes(&self) -> Result<Vec<u8>, SerializeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(SerializeError::from)
    }

    /// Deserialize from binary format (bincode)
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SerializeError> {
        let (value, _read) = bincode::serde::decode_from_slice(bytes, bincode::config::standard())
            .map_err(SerializeError::from)?;
        Ok(value)
    }

    /// Convert to human-readable debug string
    pub fn to_debug_string(&self) -> String {
        match self {
            TypedValue::Int(v) => format!("{}", v),
            TypedValue::Float(v) => format!("{}", v),
            TypedValue::Bool(v) => format!("{}", v),
            TypedValue::String(v) => format!("{:?}", v),
            TypedValue::Symbol(v) => format!(":{}", v),
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
mod tests;
