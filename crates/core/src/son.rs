//! SON (Seq Object Notation) Serialization
//!
//! Serializes Seq Values to SON format - a prefix/postfix notation compatible
//! with Seq syntax. SON values can be evaluated in Seq to recreate the original data.
//!
//! # Format Examples
//!
//! - Int: `42`
//! - Float: `3.14`
//! - Bool: `true` / `false`
//! - String: `"hello"` (with proper escaping)
//! - Symbol: `:my-symbol`
//! - List: `list-of 1 lv 2 lv 3 lv`
//! - Map: `map-of "key" "value" kv`
//! - Variant: `:Tag field1 field2 wrap-2`

use crate::seqstring::SeqString;
use crate::stack::{Stack, pop, push};
use crate::value::{MapKey, Value, VariantData};
use std::collections::HashMap;

/// Configuration for SON output formatting
#[derive(Clone)]
pub struct SonConfig {
    /// Use pretty printing with indentation
    pub pretty: bool,
    /// Number of spaces per indentation level
    pub indent: usize,
}

impl Default for SonConfig {
    fn default() -> Self {
        Self {
            pretty: false,
            indent: 2,
        }
    }
}

impl SonConfig {
    /// Create a compact (single-line) config
    pub fn compact() -> Self {
        Self::default()
    }

    /// Create a pretty-printed config
    pub fn pretty() -> Self {
        Self {
            pretty: true,
            indent: 2,
        }
    }
}

/// Format a Value to SON string
pub fn value_to_son(value: &Value, config: &SonConfig) -> String {
    let mut buf = String::new();
    format_value(value, config, 0, &mut buf);
    buf
}

/// Internal formatting function with indentation tracking
fn format_value(value: &Value, config: &SonConfig, depth: usize, buf: &mut String) {
    match value {
        Value::Int(n) => {
            buf.push_str(&n.to_string());
        }
        Value::Float(f) => {
            let s = f.to_string();
            buf.push_str(&s);
            // Ensure floats always have decimal point for disambiguation
            if !s.contains('.') && f.is_finite() {
                buf.push_str(".0");
            }
        }
        Value::Bool(b) => {
            buf.push_str(if *b { "true" } else { "false" });
        }
        Value::String(s) => {
            format_string(s.as_str(), buf);
        }
        Value::Symbol(s) => {
            buf.push(':');
            buf.push_str(s.as_str());
        }
        Value::Variant(v) => {
            format_variant(v, config, depth, buf);
        }
        Value::Map(m) => {
            format_map(m, config, depth, buf);
        }
        Value::Quotation { .. } => {
            buf.push_str("<quotation>");
        }
        Value::Closure { .. } => {
            buf.push_str("<closure>");
        }
        Value::Channel(_) => {
            buf.push_str("<channel>");
        }
        Value::WeaveCtx { .. } => {
            buf.push_str("<weave-ctx>");
        }
    }
}

/// Format a string with proper escaping
fn format_string(s: &str, buf: &mut String) {
    buf.push('"');
    for c in s.chars() {
        match c {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            '\x08' => buf.push_str("\\b"),
            '\x0C' => buf.push_str("\\f"),
            c if c.is_control() => {
                buf.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => buf.push(c),
        }
    }
    buf.push('"');
}

/// Format a variant (includes List as special case)
fn format_variant(v: &VariantData, config: &SonConfig, depth: usize, buf: &mut String) {
    let tag = v.tag.as_str();

    // Special case: List variant uses list-of/lv syntax
    if tag == "List" {
        format_list(&v.fields, config, depth, buf);
    } else {
        // General variant: :Tag field1 field2 wrap-N
        buf.push(':');
        buf.push_str(tag);

        let field_count = v.fields.len();

        if config.pretty && !v.fields.is_empty() {
            for field in v.fields.iter() {
                buf.push('\n');
                push_indent(buf, depth + 1, config.indent);
                format_value(field, config, depth + 1, buf);
            }
            buf.push('\n');
            push_indent(buf, depth, config.indent);
        } else {
            for field in v.fields.iter() {
                buf.push(' ');
                format_value(field, config, depth, buf);
            }
        }

        buf.push_str(&format!(" wrap-{}", field_count));
    }
}

/// Format a list using list-of/lv syntax
fn format_list(fields: &[Value], config: &SonConfig, depth: usize, buf: &mut String) {
    buf.push_str("list-of");

    if fields.is_empty() {
        return;
    }

    if config.pretty {
        for field in fields.iter() {
            buf.push('\n');
            push_indent(buf, depth + 1, config.indent);
            format_value(field, config, depth + 1, buf);
            buf.push_str(" lv");
        }
    } else {
        for field in fields.iter() {
            buf.push(' ');
            format_value(field, config, depth, buf);
            buf.push_str(" lv");
        }
    }
}

/// Format a map using map-of/kv syntax
fn format_map(map: &HashMap<MapKey, Value>, config: &SonConfig, depth: usize, buf: &mut String) {
    buf.push_str("map-of");

    if map.is_empty() {
        return;
    }

    // Sort keys for deterministic output (important for testing/debugging)
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by(|(k1, _), (k2, _)| {
        let s1 = map_key_sort_string(k1);
        let s2 = map_key_sort_string(k2);
        s1.cmp(&s2)
    });

    if config.pretty {
        for (key, value) in entries {
            buf.push('\n');
            push_indent(buf, depth + 1, config.indent);
            format_map_key(key, buf);
            buf.push(' ');
            format_value(value, config, depth + 1, buf);
            buf.push_str(" kv");
        }
    } else {
        for (key, value) in entries {
            buf.push(' ');
            format_map_key(key, buf);
            buf.push(' ');
            format_value(value, config, depth, buf);
            buf.push_str(" kv");
        }
    }
}

/// Get a sort key string for a MapKey
fn map_key_sort_string(key: &MapKey) -> String {
    match key {
        MapKey::Int(n) => format!("0_{:020}", n), // Prefix with 0 for ints
        MapKey::Bool(b) => format!("1_{}", b),    // Prefix with 1 for bools
        MapKey::String(s) => format!("2_{}", s.as_str()), // Prefix with 2 for strings
    }
}

/// Format a map key
fn format_map_key(key: &MapKey, buf: &mut String) {
    match key {
        MapKey::Int(n) => buf.push_str(&n.to_string()),
        MapKey::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
        MapKey::String(s) => format_string(s.as_str(), buf),
    }
}

/// Push indentation spaces
fn push_indent(buf: &mut String, depth: usize, indent_size: usize) {
    for _ in 0..(depth * indent_size) {
        buf.push(' ');
    }
}

// ============================================================================
// Runtime Builtins
// ============================================================================

/// son.dump: Serialize top of stack to SON string (compact)
/// Stack effect: ( Value -- String )
///
/// # Safety
/// - The stack must be a valid stack pointer
/// - The stack must contain at least one value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_son_dump(stack: Stack) -> Stack {
    unsafe { son_dump_impl(stack, false) }
}

/// son.dump-pretty: Serialize top of stack to SON string (pretty-printed)
/// Stack effect: ( Value -- String )
///
/// # Safety
/// - The stack must be a valid stack pointer
/// - The stack must contain at least one value
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_son_dump_pretty(stack: Stack) -> Stack {
    unsafe { son_dump_impl(stack, true) }
}

/// Implementation for both dump variants
unsafe fn son_dump_impl(stack: Stack, pretty: bool) -> Stack {
    let (rest, value) = unsafe { pop(stack) };

    let config = if pretty {
        SonConfig::pretty()
    } else {
        SonConfig::compact()
    };

    let result = value_to_son(&value, &config);
    let result_str = SeqString::from(result);

    unsafe { push(rest, Value::String(result_str)) }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seqstring::global_string;
    use std::sync::Arc;

    #[test]
    fn test_int() {
        let v = Value::Int(42);
        assert_eq!(value_to_son(&v, &SonConfig::default()), "42");
    }

    #[test]
    fn test_negative_int() {
        let v = Value::Int(-123);
        assert_eq!(value_to_son(&v, &SonConfig::default()), "-123");
    }

    #[test]
    fn test_float() {
        let v = Value::Float(2.5);
        assert_eq!(value_to_son(&v, &SonConfig::default()), "2.5");
    }

    #[test]
    fn test_float_whole_number() {
        let v = Value::Float(42.0);
        let s = value_to_son(&v, &SonConfig::default());
        assert!(s.contains('.'), "Float should contain decimal point: {}", s);
    }

    #[test]
    fn test_bool_true() {
        let v = Value::Bool(true);
        assert_eq!(value_to_son(&v, &SonConfig::default()), "true");
    }

    #[test]
    fn test_bool_false() {
        let v = Value::Bool(false);
        assert_eq!(value_to_son(&v, &SonConfig::default()), "false");
    }

    #[test]
    fn test_string_simple() {
        let v = Value::String(global_string("hello".to_string()));
        assert_eq!(value_to_son(&v, &SonConfig::default()), r#""hello""#);
    }

    #[test]
    fn test_string_escaping() {
        let v = Value::String(global_string("hello\nworld".to_string()));
        assert_eq!(value_to_son(&v, &SonConfig::default()), r#""hello\nworld""#);
    }

    #[test]
    fn test_string_quotes() {
        let v = Value::String(global_string(r#"say "hi""#.to_string()));
        assert_eq!(value_to_son(&v, &SonConfig::default()), r#""say \"hi\"""#);
    }

    #[test]
    fn test_symbol() {
        let v = Value::Symbol(global_string("my-symbol".to_string()));
        assert_eq!(value_to_son(&v, &SonConfig::default()), ":my-symbol");
    }

    #[test]
    fn test_empty_list() {
        let list = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            vec![],
        )));
        assert_eq!(value_to_son(&list, &SonConfig::default()), "list-of");
    }

    #[test]
    fn test_list() {
        let list = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        )));
        assert_eq!(
            value_to_son(&list, &SonConfig::default()),
            "list-of 1 lv 2 lv 3 lv"
        );
    }

    #[test]
    fn test_list_pretty() {
        let list = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            vec![Value::Int(1), Value::Int(2)],
        )));
        let expected = "list-of\n  1 lv\n  2 lv";
        assert_eq!(value_to_son(&list, &SonConfig::pretty()), expected);
    }

    #[test]
    fn test_empty_map() {
        let m: HashMap<MapKey, Value> = HashMap::new();
        let v = Value::Map(Box::new(m));
        assert_eq!(value_to_son(&v, &SonConfig::default()), "map-of");
    }

    #[test]
    fn test_map() {
        let mut m = HashMap::new();
        m.insert(
            MapKey::String(global_string("key".to_string())),
            Value::Int(42),
        );
        let v = Value::Map(Box::new(m));
        assert_eq!(
            value_to_son(&v, &SonConfig::default()),
            r#"map-of "key" 42 kv"#
        );
    }

    #[test]
    fn test_variant_no_fields() {
        let v = Value::Variant(Arc::new(VariantData::new(
            global_string("None".to_string()),
            vec![],
        )));
        assert_eq!(value_to_son(&v, &SonConfig::default()), ":None wrap-0");
    }

    #[test]
    fn test_variant_with_fields() {
        let v = Value::Variant(Arc::new(VariantData::new(
            global_string("Point".to_string()),
            vec![Value::Int(10), Value::Int(20)],
        )));
        assert_eq!(
            value_to_son(&v, &SonConfig::default()),
            ":Point 10 20 wrap-2"
        );
    }

    #[test]
    fn test_variant_pretty() {
        let v = Value::Variant(Arc::new(VariantData::new(
            global_string("Point".to_string()),
            vec![Value::Int(10), Value::Int(20)],
        )));
        let expected = ":Point\n  10\n  20\n wrap-2";
        assert_eq!(value_to_son(&v, &SonConfig::pretty()), expected);
    }

    #[test]
    fn test_nested_list_in_map() {
        let list = Value::Variant(Arc::new(VariantData::new(
            global_string("List".to_string()),
            vec![Value::Int(1), Value::Int(2)],
        )));
        let mut m = HashMap::new();
        m.insert(MapKey::String(global_string("items".to_string())), list);
        let v = Value::Map(Box::new(m));
        assert_eq!(
            value_to_son(&v, &SonConfig::default()),
            r#"map-of "items" list-of 1 lv 2 lv kv"#
        );
    }

    #[test]
    fn test_quotation() {
        let v = Value::Quotation {
            wrapper: 0,
            impl_: 0,
        };
        assert_eq!(value_to_son(&v, &SonConfig::default()), "<quotation>");
    }

    #[test]
    fn test_closure() {
        let v = Value::Closure {
            fn_ptr: 0,
            env: Arc::new([]),
        };
        assert_eq!(value_to_son(&v, &SonConfig::default()), "<closure>");
    }
}
