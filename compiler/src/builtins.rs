//! Built-in word signatures for cem3
//!
//! Defines the stack effects for all runtime built-in operations.

use crate::types::{Effect, StackType, Type};
use std::collections::HashMap;

/// Get the stack effect signature for a built-in word
pub fn builtin_signature(name: &str) -> Option<Effect> {
    // Build the map lazily
    let signatures = builtin_signatures();
    signatures.get(name).cloned()
}

/// Get all built-in word signatures
pub fn builtin_signatures() -> HashMap<String, Effect> {
    let mut sigs = HashMap::new();

    // I/O operations
    sigs.insert(
        "write_line".to_string(),
        Effect::new(
            StackType::singleton(Type::String), // ( String -- )
            StackType::Empty,
        ),
    );

    sigs.insert(
        "read_line".to_string(),
        Effect::new(
            StackType::Empty, // ( -- String )
            StackType::singleton(Type::String),
        ),
    );

    sigs.insert(
        "int->string".to_string(),
        Effect::new(
            StackType::singleton(Type::Int), // ( Int -- String )
            StackType::singleton(Type::String),
        ),
    );

    // Arithmetic operations ( Int Int -- Int )
    for op in &["add", "subtract", "multiply", "divide"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::Empty.push(Type::Int).push(Type::Int), // ( Int Int -- Int )
                StackType::singleton(Type::Int),
            ),
        );
    }

    // Comparison operations ( Int Int -- Int )
    // Note: Comparisons return Int (0 or 1), not Bool, for Forth compatibility
    for op in &["=", "<", ">", "<=", ">=", "<>"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::Empty.push(Type::Int).push(Type::Int), // ( Int Int -- Int )
                StackType::singleton(Type::Int),
            ),
        );
    }

    // Stack operations with row polymorphism
    // dup: ( ..a T -- ..a T T )
    sigs.insert(
        "dup".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("T".to_string())),
        ),
    );

    // drop: ( ..a T -- ..a )
    sigs.insert(
        "drop".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
            StackType::RowVar("a".to_string()),
        ),
    );

    // swap: ( ..a T U -- ..a U T )
    sigs.insert(
        "swap".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("U".to_string()))
                .push(Type::Var("T".to_string())),
        ),
    );

    // over: ( ..a T U -- ..a T U T )
    sigs.insert(
        "over".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string()))
                .push(Type::Var("T".to_string())),
        ),
    );

    // rot: ( ..a T U V -- ..a U V T )
    sigs.insert(
        "rot".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string()))
                .push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("U".to_string()))
                .push(Type::Var("V".to_string()))
                .push(Type::Var("T".to_string())),
        ),
    );

    // nip: ( ..a T U -- ..a U )
    sigs.insert(
        "nip".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("U".to_string())),
        ),
    );

    // tuck: ( ..a T U -- ..a U T U )
    sigs.insert(
        "tuck".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("U".to_string()))
                .push(Type::Var("T".to_string()))
                .push(Type::Var("U".to_string())),
        ),
    );

    // Concurrency operations
    // make-channel: ( -- Int )
    // Returns channel ID as Int
    sigs.insert(
        "make-channel".to_string(),
        Effect::new(StackType::Empty, StackType::singleton(Type::Int)),
    );

    // send: ( ..a T Int -- ..a )
    // Takes value T and channel Int
    sigs.insert(
        "send".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Int),
            StackType::RowVar("a".to_string()),
        ),
    );

    // receive: ( ..a Int -- ..a T )
    // Takes channel Int, returns value T
    sigs.insert(
        "receive".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
        ),
    );

    // close-channel: ( Int -- )
    sigs.insert(
        "close-channel".to_string(),
        Effect::new(StackType::singleton(Type::Int), StackType::Empty),
    );

    // yield: ( -- )
    sigs.insert(
        "yield".to_string(),
        Effect::new(StackType::Empty, StackType::Empty),
    );

    sigs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_signature_write_line() {
        let sig = builtin_signature("write_line").unwrap();
        assert_eq!(sig.inputs, StackType::singleton(Type::String));
        assert_eq!(sig.outputs, StackType::Empty);
    }

    #[test]
    fn test_builtin_signature_add() {
        let sig = builtin_signature("add").unwrap();
        let (rest, top) = sig.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::Int);
        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Int);
        assert_eq!(rest2, StackType::Empty);
        assert_eq!(sig.outputs, StackType::singleton(Type::Int));
    }

    #[test]
    fn test_builtin_signature_dup() {
        let sig = builtin_signature("dup").unwrap();
        // Input: ( ..a T )
        assert_eq!(
            sig.inputs,
            StackType::Cons {
                rest: Box::new(StackType::RowVar("a".to_string())),
                top: Type::Var("T".to_string())
            }
        );
        // Output: ( ..a T T )
        let (rest, top) = sig.outputs.clone().pop().unwrap();
        assert_eq!(top, Type::Var("T".to_string()));
        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Var("T".to_string()));
        assert_eq!(rest2, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_all_builtins_have_signatures() {
        let sigs = builtin_signatures();

        // Verify all expected builtins have signatures
        assert!(sigs.contains_key("write_line"));
        assert!(sigs.contains_key("read_line"));
        assert!(sigs.contains_key("int->string"));
        assert!(sigs.contains_key("add"));
        assert!(sigs.contains_key("dup"));
        assert!(sigs.contains_key("swap"));
        assert!(sigs.contains_key("make-channel"));
        assert!(sigs.contains_key("send"));
        assert!(sigs.contains_key("receive"));
    }
}
