//! Built-in word signatures for Seq
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

    // I/O operations with row polymorphism
    sigs.insert(
        "write_line".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String), // ( ..a String -- ..a )
            StackType::RowVar("a".to_string()),
        ),
    );

    sigs.insert(
        "read_line".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()), // ( ..a -- ..a String )
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    sigs.insert(
        "int->string".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int), // ( ..a Int -- ..a String )
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // Arithmetic operations ( ..a Int Int -- ..a Int )
    for op in &["add", "subtract", "multiply", "divide"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::RowVar("a".to_string())
                    .push(Type::Int)
                    .push(Type::Int),
                StackType::RowVar("a".to_string()).push(Type::Int),
            ),
        );
    }

    // Comparison operations ( ..a Int Int -- ..a Int )
    // Note: Comparisons return Int (0 or 1), not Bool, for Forth compatibility
    for op in &["=", "<", ">", "<=", ">=", "<>"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::RowVar("a".to_string())
                    .push(Type::Int)
                    .push(Type::Int),
                StackType::RowVar("a".to_string()).push(Type::Int),
            ),
        );
    }

    // Boolean operations ( ..a Int Int -- ..a Int )
    // Forth-style: 0 is false, non-zero is true
    // and: ( a b -- result ) returns 1 if both non-zero, 0 otherwise
    sigs.insert(
        "and".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // or: ( a b -- result ) returns 1 if either non-zero, 0 otherwise
    sigs.insert(
        "or".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // not: ( a -- result ) returns 1 if zero, 0 if non-zero
    sigs.insert(
        "not".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

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

    // pick: ( ..a T Int -- ..a T T )
    // Copies value at depth n to top of stack
    // pick(0) = dup, pick(1) = over, pick(2) = third value, etc.
    //
    // TODO: This type signature is a known limitation of the current type system.
    // The signature claims pick copies the top value T, but pick actually copies
    // the value at depth n, which could be any type within the row variable ..a.
    // For example:
    //   - pick(0) on ( Int String -- ) copies the String (type T)
    //   - pick(1) on ( Int String -- ) copies the Int (type U, not T)
    //
    // A proper signature would require dependent types or indexed row variables:
    //   pick: ∀a, n. ( ..a[T_0, ..., T_n, ...] Int(n) -- ..a[T_0, ..., T_n, ...] T_n )
    //
    // The runtime validates stack depth at runtime (see pick_op in runtime/src/stack.rs),
    // but the type system cannot statically verify the copied value's type matches
    // how it's used. This is acceptable for now as pick is primarily used for
    // building known-safe utilities (third, fourth, 3dup, etc.).
    sigs.insert(
        "pick".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Int),
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Var("T".to_string())),
        ),
    );

    // Concurrency operations with row polymorphism
    // make-channel: ( ..a -- ..a Int )
    // Returns channel ID as Int
    sigs.insert(
        "make-channel".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
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

    // close-channel: ( ..a Int -- ..a )
    sigs.insert(
        "close-channel".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()),
        ),
    );

    // yield: ( ..a -- ..a )
    sigs.insert(
        "yield".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string()),
        ),
    );

    // Quotation operations
    // call: ( ..a Callable -- ..b )
    // Note: Accepts both Quotation and Closure types
    // The actual effect depends on the quotation/closure's type,
    // but we use ..a and ..b to indicate this is polymorphic
    // A more precise type would require dependent types
    //
    // Using type variable Q to represent "something callable"
    // This allows both Quotation and Closure to unify with Q
    sigs.insert(
        "call".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("Q".to_string())),
            StackType::RowVar("b".to_string()),
        ),
    );

    // times: ( ..a Quotation Int -- ..a )
    // Executes quotation n times. Quotation must have effect ( ..a -- ..a )
    sigs.insert(
        "times".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()),
                ))))
                .push(Type::Int),
            StackType::RowVar("a".to_string()),
        ),
    );

    // while: ( ..a Quotation Quotation -- ..a )
    // First quotation is condition ( ..a -- ..a Int ), second is body ( ..a -- ..a )
    sigs.insert(
        "while".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()).push(Type::Int),
                ))))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()),
                )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // until: ( ..a Quotation Quotation -- ..a )
    // First quotation is body ( ..a -- ..a ), second is condition ( ..a -- ..a Int )
    // Executes body, then checks condition; repeats until condition is true
    sigs.insert(
        "until".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()),
                ))))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()).push(Type::Int),
                )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // forever: ( ..a Quotation -- ..a )
    // Executes quotation infinitely. Quotation must have effect ( ..a -- ..a )
    // Note: This never returns in practice, but type-wise it has same stack effect
    sigs.insert(
        "forever".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Quotation(Box::new(Effect::new(
                StackType::RowVar("a".to_string()),
                StackType::RowVar("a".to_string()),
            )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // spawn: ( ..a Quotation -- ..a Int )
    // Spawns a quotation as a new strand, returns strand ID
    // The quotation should have effect ( -- ) (empty stack in, empty stack out)
    sigs.insert(
        "spawn".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Quotation(Box::new(Effect::new(
                StackType::Empty,
                StackType::Empty,
            )))),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // cond: ( ..a -- ..b )
    // Multi-way conditional combinator
    // Actual stack effect: ( value [pred1] [body1] [pred2] [body2] ... [predN] [bodyN] count -- result )
    // Each predicate quotation has effect: ( T -- T Int )
    // Each body quotation has effect: ( T -- U )
    // Note: Variable-arity makes precise typing difficult; using row polymorphism as approximation
    sigs.insert(
        "cond".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("b".to_string()),
        ),
    );

    // TCP operations with row polymorphism
    // tcp-listen: ( ..a Int -- ..a Int )
    // Takes port number, returns listener ID
    sigs.insert(
        "tcp-listen".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // tcp-accept: ( ..a Int -- ..a Int )
    // Takes listener ID, returns client socket ID
    sigs.insert(
        "tcp-accept".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // tcp-read: ( ..a Int -- ..a String )
    // Takes socket ID, returns data read as String
    sigs.insert(
        "tcp-read".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // tcp-write: ( ..a String Int -- ..a )
    // Takes data String and socket ID, writes to socket
    sigs.insert(
        "tcp-write".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
            StackType::RowVar("a".to_string()),
        ),
    );

    // tcp-close: ( ..a Int -- ..a )
    // Takes socket ID, closes the socket
    sigs.insert(
        "tcp-close".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()),
        ),
    );

    // String operations
    // string-concat: ( ..a String String -- ..a String )
    // Concatenate two strings
    sigs.insert(
        "string-concat".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string-length: ( ..a String -- ..a Int )
    // Get string length
    sigs.insert(
        "string-length".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-split: ( ..a String String -- ..a Variant )
    // Split string by delimiter, returns a Variant containing the parts
    sigs.insert(
        "string-split".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // string-contains: ( ..a String String -- ..a Int )
    // Check if string contains substring (returns 0 or 1)
    sigs.insert(
        "string-contains".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-starts-with: ( ..a String String -- ..a Int )
    // Check if string starts with prefix (returns 0 or 1)
    sigs.insert(
        "string-starts-with".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-empty: ( ..a String -- ..a Int )
    // Check if string is empty (returns 0 or 1)
    sigs.insert(
        "string-empty".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-trim: ( ..a String -- ..a String )
    // Trim whitespace from both ends
    sigs.insert(
        "string-trim".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string-to-upper: ( ..a String -- ..a String )
    // Convert to uppercase
    sigs.insert(
        "string-to-upper".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string-to-lower: ( ..a String -- ..a String )
    // Convert to lowercase
    sigs.insert(
        "string-to-lower".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // Variant operations
    // variant-field-count: ( ..a Variant -- ..a Int )
    // Get number of fields in a variant
    sigs.insert(
        "variant-field-count".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // variant-tag: ( ..a Variant -- ..a Int )
    // Get tag of a variant
    sigs.insert(
        "variant-tag".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // variant-field-at: ( ..a Variant Int -- ..a Value )
    // Get field at index from variant
    sigs.insert(
        "variant-field-at".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
        ),
    );

    sigs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_signature_write_line() {
        let sig = builtin_signature("write_line").unwrap();
        // ( ..a String -- ..a )
        let (rest, top) = sig.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
        assert_eq!(sig.outputs, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_builtin_signature_add() {
        let sig = builtin_signature("add").unwrap();
        // ( ..a Int Int -- ..a Int )
        let (rest, top) = sig.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::Int);
        let (rest2, top2) = rest.pop().unwrap();
        assert_eq!(top2, Type::Int);
        assert_eq!(rest2, StackType::RowVar("a".to_string()));

        let (rest3, top3) = sig.outputs.clone().pop().unwrap();
        assert_eq!(top3, Type::Int);
        assert_eq!(rest3, StackType::RowVar("a".to_string()));
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
