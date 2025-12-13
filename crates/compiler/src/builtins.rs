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

    // read_line+: ( ..a -- ..a String Int )
    // Returns line and status (1=success, 0=EOF)
    // The + suffix indicates result pattern (value + status)
    sigs.insert(
        "read_line+".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // Command-line argument operations
    // arg-count: ( ..a -- ..a Int ) returns number of arguments including program name
    sigs.insert(
        "arg-count".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // arg: ( ..a Int -- ..a String ) returns argument at index (0 = program name)
    sigs.insert(
        "arg".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // File operations
    // file-slurp: ( ..a String -- ..a String ) reads entire file contents
    sigs.insert(
        "file-slurp".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // file-slurp-safe: ( ..a String -- ..a String Int )
    // Reads entire file, returns (contents 1) on success or ("" 0) on failure
    sigs.insert(
        "file-slurp-safe".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // file-exists?: ( ..a String -- ..a Int ) returns 1 if file exists, 0 otherwise
    sigs.insert(
        "file-exists?".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // file-for-each-line+: ( ..a String Quotation -- ..a String Int )
    // Opens file, calls quotation with each line, closes file.
    // Quotation has effect ( ..a String -- ..a ) - receives line, must consume it.
    // Returns ("" 1) on success, ("error message" 0) on failure.
    sigs.insert(
        "file-for-each-line+".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()).push(Type::String),
                    StackType::RowVar("a".to_string()),
                )))),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
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

    // Bitwise operations ( ..a Int Int -- ..a Int )
    // band: bitwise AND
    sigs.insert(
        "band".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // bor: bitwise OR
    sigs.insert(
        "bor".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // bxor: bitwise XOR
    sigs.insert(
        "bxor".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // bnot: bitwise NOT (one's complement)
    sigs.insert(
        "bnot".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // shl: shift left ( value count -- result )
    sigs.insert(
        "shl".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // shr: logical shift right ( value count -- result )
    sigs.insert(
        "shr".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // Bit counting operations
    // popcount: count number of 1 bits
    sigs.insert(
        "popcount".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // clz: count leading zeros
    sigs.insert(
        "clz".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // ctz: count trailing zeros
    sigs.insert(
        "ctz".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // int-bits: push the bit width of Int (64)
    sigs.insert(
        "int-bits".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
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

    // roll: ( ..a T_n ... T_1 T_0 Int(n) -- ..a T_(n-1) ... T_1 T_0 T_n )
    // Rotates n+1 items, bringing the item at depth n to the top.
    // roll(0) = no-op, roll(1) = swap, roll(2) = rot
    //
    // Like pick, the true type requires dependent types. The signature below
    // is a simplified approximation that validates the depth parameter is Int
    // and that the stack has at least one item to rotate.
    //
    // Runtime validates stack depth (see roll in runtime/src/stack.rs).
    sigs.insert(
        "roll".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
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

    // send-safe: ( ..a T Int -- ..a Int )
    // Takes value T and channel Int, returns 1 on success, 0 on failure
    sigs.insert(
        "send-safe".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
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

    // receive-safe: ( ..a Int -- ..a T Int )
    // Takes channel Int, returns (value, 1) on success or (0, 0) on failure
    sigs.insert(
        "receive-safe".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string())
                .push(Type::Var("T".to_string()))
                .push(Type::Int),
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

    // spawn: ( ..a Quotation[any] -- ..a Int )
    // Spawns a quotation as a new strand, returns strand ID
    // The quotation can have any effect because spawn copies the stack to the child
    // and the parent's stack is unaffected by what the child does.
    sigs.insert(
        "spawn".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Quotation(Box::new(Effect::new(
                StackType::RowVar("spawn_in".to_string()),
                StackType::RowVar("spawn_out".to_string()),
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

    // OS operations
    // getenv: ( ..a String -- ..a String Int )
    // Get environment variable, returns value and success flag (1=found, 0=not found)
    sigs.insert(
        "getenv".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // home-dir: ( ..a -- ..a String Int )
    // Get user's home directory, returns path and success flag
    sigs.insert(
        "home-dir".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // current-dir: ( ..a -- ..a String Int )
    // Get current working directory, returns path and success flag
    sigs.insert(
        "current-dir".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // path-exists: ( ..a String -- ..a Int )
    // Check if path exists, returns 1 if exists, 0 otherwise
    sigs.insert(
        "path-exists".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // path-is-file: ( ..a String -- ..a Int )
    // Check if path is a regular file, returns 1 if file, 0 otherwise
    sigs.insert(
        "path-is-file".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // path-is-dir: ( ..a String -- ..a Int )
    // Check if path is a directory, returns 1 if directory, 0 otherwise
    sigs.insert(
        "path-is-dir".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // path-join: ( ..a String String -- ..a String )
    // Join two path components
    sigs.insert(
        "path-join".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // path-parent: ( ..a String -- ..a String Int )
    // Get parent directory, returns path and success flag
    sigs.insert(
        "path-parent".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
        ),
    );

    // path-filename: ( ..a String -- ..a String Int )
    // Get filename component, returns filename and success flag
    sigs.insert(
        "path-filename".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
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

    // string-chomp: ( ..a String -- ..a String )
    // Remove trailing newline (\n or \r\n)
    sigs.insert(
        "string-chomp".to_string(),
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

    // string-equal: ( ..a String String -- ..a Int )
    // Check if two strings are equal (returns 0 or 1)
    sigs.insert(
        "string-equal".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // json-escape: ( ..a String -- ..a String )
    // Escape special characters for JSON output
    sigs.insert(
        "json-escape".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string->int: ( ..a String -- ..a Int Int )
    // Parse string to integer, returns value and success flag (1=ok, 0=fail)
    sigs.insert(
        "string->int".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
        ),
    );

    // string-byte-length: ( ..a String -- ..a Int )
    // Get byte length (for HTTP Content-Length, buffer allocation)
    sigs.insert(
        "string-byte-length".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-char-at: ( ..a String Int -- ..a Int )
    // Get Unicode code point at character index
    sigs.insert(
        "string-char-at".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // string-substring: ( ..a String Int Int -- ..a String )
    // Extract substring by character indices (string, start, length)
    sigs.insert(
        "string-substring".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Int)
                .push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // char->string: ( ..a Int -- ..a String )
    // Convert Unicode code point to single-character string
    sigs.insert(
        "char->string".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string-find: ( ..a String String -- ..a Int )
    // Find first occurrence of substring, returns character index or -1
    sigs.insert(
        "string-find".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::String),
            StackType::RowVar("a".to_string()).push(Type::Int),
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

    // Type-safe variant constructors with fixed arity
    // These have proper type signatures that account for all consumed values

    // make-variant-0: ( ..a tag -- ..a Variant )
    // Create a variant with 0 fields (just tag)
    sigs.insert(
        "make-variant-0".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int), // tag
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // make-variant-1: ( ..a field1 tag -- ..a Variant )
    // Create a variant with 1 field
    sigs.insert(
        "make-variant-1".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T1".to_string())) // field1
                .push(Type::Int), // tag
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // make-variant-2: ( ..a field1 field2 tag -- ..a Variant )
    // Create a variant with 2 fields
    sigs.insert(
        "make-variant-2".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T1".to_string())) // field1
                .push(Type::Var("T2".to_string())) // field2
                .push(Type::Int), // tag
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // make-variant-3: ( ..a field1 field2 field3 tag -- ..a Variant )
    // Create a variant with 3 fields
    sigs.insert(
        "make-variant-3".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T1".to_string())) // field1
                .push(Type::Var("T2".to_string())) // field2
                .push(Type::Var("T3".to_string())) // field3
                .push(Type::Int), // tag
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // make-variant-4: ( ..a field1 field2 field3 field4 tag -- ..a Variant )
    // Create a variant with 4 fields
    sigs.insert(
        "make-variant-4".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("T1".to_string())) // field1
                .push(Type::Var("T2".to_string())) // field2
                .push(Type::Var("T3".to_string())) // field3
                .push(Type::Var("T4".to_string())) // field4
                .push(Type::Int), // tag
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // variant-append: ( ..a Variant Value -- ..a Variant' )
    // Append a value to a variant, returning a new variant with the value added
    // Functional style - original variant is not modified
    sigs.insert(
        "variant-append".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Var("T".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("V2".to_string())),
        ),
    );

    // variant-last: ( ..a Variant -- ..a Value )
    // Get the last field from a variant (peek for stack-like usage)
    sigs.insert(
        "variant-last".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("T".to_string())),
        ),
    );

    // variant-init: ( ..a Variant -- ..a Variant' )
    // Get all but the last field from a variant (pop without returning value)
    sigs.insert(
        "variant-init".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("V2".to_string())),
        ),
    );

    // List operations (higher-order combinators for Variants used as lists)
    // list-map: ( ..a Variant Quotation -- ..a Variant )
    // Apply quotation to each element, return new variant with transformed elements
    sigs.insert(
        "list-map".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("b".to_string()).push(Type::Var("T".to_string())),
                    StackType::RowVar("b".to_string()).push(Type::Var("U".to_string())),
                )))),
            StackType::RowVar("a".to_string()).push(Type::Var("V2".to_string())),
        ),
    );

    // list-filter: ( ..a Variant Quotation -- ..a Variant )
    // Keep elements where quotation returns non-zero
    sigs.insert(
        "list-filter".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("b".to_string()).push(Type::Var("T".to_string())),
                    StackType::RowVar("b".to_string()).push(Type::Int),
                )))),
            StackType::RowVar("a".to_string()).push(Type::Var("V2".to_string())),
        ),
    );

    // list-fold: ( ..a Variant init Quotation -- ..a result )
    // Fold over list with accumulator; quotation has effect ( acc elem -- acc' )
    sigs.insert(
        "list-fold".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Var("Acc".to_string()))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("b".to_string())
                        .push(Type::Var("Acc".to_string()))
                        .push(Type::Var("T".to_string())),
                    StackType::RowVar("b".to_string()).push(Type::Var("Acc".to_string())),
                )))),
            StackType::RowVar("a".to_string()).push(Type::Var("Acc".to_string())),
        ),
    );

    // list-each: ( ..a Variant Quotation -- ..a )
    // Apply quotation to each element for side effects only
    sigs.insert(
        "list-each".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("b".to_string()).push(Type::Var("T".to_string())),
                    StackType::RowVar("b".to_string()),
                )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // list-length: ( ..a Variant -- ..a Int )
    // Get number of elements in list (alias for variant-field-count)
    sigs.insert(
        "list-length".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // list-empty?: ( ..a Variant -- ..a Int )
    // Check if list has no elements (returns 1 if empty, 0 otherwise)
    sigs.insert(
        "list-empty?".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // Map operations (dictionary/hash map with O(1) lookup)
    // make-map: ( ..a -- ..a Map )
    // Create an empty map
    sigs.insert(
        "make-map".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("a".to_string()).push(Type::Var("M".to_string())),
        ),
    );

    // map-get: ( ..a Map key -- ..a value )
    // Get value by key (panics if not found)
    sigs.insert(
        "map-get".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("M".to_string()))
                .push(Type::Var("K".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // map-get-safe: ( ..a Map key -- ..a value Int )
    // Get value with success flag (1=found, 0=not found)
    sigs.insert(
        "map-get-safe".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("M".to_string()))
                .push(Type::Var("K".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Int),
        ),
    );

    // map-set: ( ..a Map key value -- ..a Map )
    // Set key-value pair, returns new map
    sigs.insert(
        "map-set".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("M".to_string()))
                .push(Type::Var("K".to_string()))
                .push(Type::Var("V".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("M2".to_string())),
        ),
    );

    // map-has?: ( ..a Map key -- ..a Int )
    // Check if key exists (1=yes, 0=no)
    sigs.insert(
        "map-has?".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("M".to_string()))
                .push(Type::Var("K".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // map-remove: ( ..a Map key -- ..a Map )
    // Remove key, returns new map
    sigs.insert(
        "map-remove".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("M".to_string()))
                .push(Type::Var("K".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("M2".to_string())),
        ),
    );

    // map-keys: ( ..a Map -- ..a Variant )
    // Get all keys as a list
    sigs.insert(
        "map-keys".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("M".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // map-values: ( ..a Map -- ..a Variant )
    // Get all values as a list
    sigs.insert(
        "map-values".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("M".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Var("V".to_string())),
        ),
    );

    // map-size: ( ..a Map -- ..a Int )
    // Get number of entries
    sigs.insert(
        "map-size".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("M".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // map-empty?: ( ..a Map -- ..a Int )
    // Check if map is empty (1=yes, 0=no)
    sigs.insert(
        "map-empty?".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("M".to_string())),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // Float arithmetic operations ( ..a Float Float -- ..a Float )
    for op in &["f.add", "f.subtract", "f.multiply", "f.divide"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::RowVar("a".to_string())
                    .push(Type::Float)
                    .push(Type::Float),
                StackType::RowVar("a".to_string()).push(Type::Float),
            ),
        );
    }

    // Float comparison operations ( ..a Float Float -- ..a Int )
    // Comparisons return Int (0 or 1) like integer comparisons
    for op in &["f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>"] {
        sigs.insert(
            op.to_string(),
            Effect::new(
                StackType::RowVar("a".to_string())
                    .push(Type::Float)
                    .push(Type::Float),
                StackType::RowVar("a".to_string()).push(Type::Int),
            ),
        );
    }

    // int->float: ( ..a Int -- ..a Float )
    // Convert integer to floating-point
    sigs.insert(
        "int->float".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Int),
            StackType::RowVar("a".to_string()).push(Type::Float),
        ),
    );

    // float->int: ( ..a Float -- ..a Int )
    // Truncate floating-point to integer (toward zero)
    sigs.insert(
        "float->int".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Float),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // float->string: ( ..a Float -- ..a String )
    // Convert floating-point to string representation
    sigs.insert(
        "float->string".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Float),
            StackType::RowVar("a".to_string()).push(Type::String),
        ),
    );

    // string->float: ( ..a String -- ..a Float Int )
    // Parse string as float, returns value and success flag (1 on success, 0 on failure)
    sigs.insert(
        "string->float".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::String),
            StackType::RowVar("a".to_string())
                .push(Type::Float)
                .push(Type::Int),
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
        assert!(
            sigs.contains_key("string->float"),
            "string->float should be a builtin"
        );
    }
}
