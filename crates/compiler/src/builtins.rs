//! Built-in word signatures for Seq
//!
//! Defines the stack effects for all runtime built-in operations.
//!
//! Uses declarative macros to minimize boilerplate. The `builtin!` macro
//! supports a Forth-like notation: `(a Type1 Type2 -- a Type3)` where:
//! - `a` is the row variable (representing "rest of stack")
//! - Concrete types: `Int`, `String`, `Float`
//! - Type variables: single uppercase letters like `T`, `U`, `V`

use crate::types::{Effect, SideEffect, StackType, Type};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Convert a type token to a Type expression
macro_rules! ty {
    (Int) => {
        Type::Int
    };
    (Bool) => {
        Type::Bool
    };
    (String) => {
        Type::String
    };
    (Float) => {
        Type::Float
    };
    (Symbol) => {
        Type::Symbol
    };
    (Channel) => {
        Type::Channel
    };
    // Single uppercase letter = type variable
    (T) => {
        Type::Var("T".to_string())
    };
    (U) => {
        Type::Var("U".to_string())
    };
    (V) => {
        Type::Var("V".to_string())
    };
    (W) => {
        Type::Var("W".to_string())
    };
    (K) => {
        Type::Var("K".to_string())
    };
    (M) => {
        Type::Var("M".to_string())
    };
    (Q) => {
        Type::Var("Q".to_string())
    };
    // Multi-char type variables (T1, T2, etc.)
    (T1) => {
        Type::Var("T1".to_string())
    };
    (T2) => {
        Type::Var("T2".to_string())
    };
    (T3) => {
        Type::Var("T3".to_string())
    };
    (T4) => {
        Type::Var("T4".to_string())
    };
    (V2) => {
        Type::Var("V2".to_string())
    };
    (M2) => {
        Type::Var("M2".to_string())
    };
    (Acc) => {
        Type::Var("Acc".to_string())
    };
}

/// Build a stack type from row variable 'a' plus pushed types
macro_rules! stack {
    // Just the row variable
    (a) => {
        StackType::RowVar("a".to_string())
    };
    // Row variable with one type pushed
    (a $t1:tt) => {
        StackType::RowVar("a".to_string()).push(ty!($t1))
    };
    // Row variable with two types pushed
    (a $t1:tt $t2:tt) => {
        StackType::RowVar("a".to_string())
            .push(ty!($t1))
            .push(ty!($t2))
    };
    // Row variable with three types pushed
    (a $t1:tt $t2:tt $t3:tt) => {
        StackType::RowVar("a".to_string())
            .push(ty!($t1))
            .push(ty!($t2))
            .push(ty!($t3))
    };
    // Row variable with four types pushed
    (a $t1:tt $t2:tt $t3:tt $t4:tt) => {
        StackType::RowVar("a".to_string())
            .push(ty!($t1))
            .push(ty!($t2))
            .push(ty!($t3))
            .push(ty!($t4))
    };
    // Row variable with five types pushed
    (a $t1:tt $t2:tt $t3:tt $t4:tt $t5:tt) => {
        StackType::RowVar("a".to_string())
            .push(ty!($t1))
            .push(ty!($t2))
            .push(ty!($t3))
            .push(ty!($t4))
            .push(ty!($t5))
    };
    // Row variable 'b' (used in some signatures)
    (b) => {
        StackType::RowVar("b".to_string())
    };
    (b $t1:tt) => {
        StackType::RowVar("b".to_string()).push(ty!($t1))
    };
    (b $t1:tt $t2:tt) => {
        StackType::RowVar("b".to_string())
            .push(ty!($t1))
            .push(ty!($t2))
    };
}

/// Define a builtin signature with Forth-like stack effect notation
///
/// Usage: `builtin!(sigs, "name", (a Type1 Type2 -- a Type3));`
macro_rules! builtin {
    // (a -- a)
    ($sigs:ident, $name:expr, (a -- a)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a), stack!(a)));
    };
    // (a -- a T)
    ($sigs:ident, $name:expr, (a -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a), stack!(a $o1)));
    };
    // (a -- a T U)
    ($sigs:ident, $name:expr, (a -- a $o1:tt $o2:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a), stack!(a $o1 $o2)));
    };
    // (a T -- a)
    ($sigs:ident, $name:expr, (a $i1:tt -- a)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1), stack!(a)));
    };
    // (a T -- a U)
    ($sigs:ident, $name:expr, (a $i1:tt -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1), stack!(a $o1)));
    };
    // (a T -- a U V)
    ($sigs:ident, $name:expr, (a $i1:tt -- a $o1:tt $o2:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1), stack!(a $o1 $o2)));
    };
    // (a T U -- a)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt -- a)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2), stack!(a)));
    };
    // (a T U -- a V)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2), stack!(a $o1)));
    };
    // (a T U -- a V W)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt -- a $o1:tt $o2:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2), stack!(a $o1 $o2)));
    };
    // (a T U -- a V W X)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt -- a $o1:tt $o2:tt $o3:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2), stack!(a $o1 $o2 $o3)));
    };
    // (a T U -- a V W X Y)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt -- a $o1:tt $o2:tt $o3:tt $o4:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2), stack!(a $o1 $o2 $o3 $o4)));
    };
    // (a T U V -- a)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt -- a)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3), stack!(a)));
    };
    // (a T U V -- a W)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3), stack!(a $o1)));
    };
    // (a T U V -- a W X)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt -- a $o1:tt $o2:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3), stack!(a $o1 $o2)));
    };
    // (a T U V -- a W X Y)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt -- a $o1:tt $o2:tt $o3:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3), stack!(a $o1 $o2 $o3)));
    };
    // (a T U V W -- a X)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt $i4:tt -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3 $i4), stack!(a $o1)));
    };
    // (a T U V W X -- a Y)
    ($sigs:ident, $name:expr, (a $i1:tt $i2:tt $i3:tt $i4:tt $i5:tt -- a $o1:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1 $i2 $i3 $i4 $i5), stack!(a $o1)));
    };
}

/// Define multiple builtins with the same signature
/// Note: Can't use a generic macro due to tt repetition issues, so we use specific helpers
macro_rules! builtins_int_int_to_int {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Int Int -- a Int));
        )+
    };
}

macro_rules! builtins_int_int_to_bool {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Int Int -- a Bool));
        )+
    };
}

macro_rules! builtins_bool_bool_to_bool {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Bool Bool -- a Bool));
        )+
    };
}

macro_rules! builtins_int_to_int {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Int -- a Int));
        )+
    };
}

macro_rules! builtins_string_to_string {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a String -- a String));
        )+
    };
}

macro_rules! builtins_float_float_to_float {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Float Float -- a Float));
        )+
    };
}

macro_rules! builtins_float_float_to_bool {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Float Float -- a Bool));
        )+
    };
}

/// Get the stack effect signature for a built-in word
pub fn builtin_signature(name: &str) -> Option<Effect> {
    let signatures = builtin_signatures();
    signatures.get(name).cloned()
}

/// Get all built-in word signatures
pub fn builtin_signatures() -> HashMap<String, Effect> {
    let mut sigs = HashMap::new();

    // =========================================================================
    // I/O Operations
    // =========================================================================

    builtin!(sigs, "io.write", (a String -- a)); // Write without newline
    builtin!(sigs, "io.write-line", (a String -- a));
    builtin!(sigs, "io.read-line", (a -- a String Bool)); // Returns line + success flag
    builtin!(sigs, "io.read-line+", (a -- a String Int)); // Returns line + status (legacy)
    builtin!(sigs, "io.read-n", (a Int -- a String Int)); // Read N bytes, returns bytes + status

    // =========================================================================
    // Command-line Arguments
    // =========================================================================

    builtin!(sigs, "args.count", (a -- a Int));
    builtin!(sigs, "args.at", (a Int -- a String));

    // =========================================================================
    // File Operations
    // =========================================================================

    builtin!(sigs, "file.slurp", (a String -- a String Bool)); // returns (content success) - errors are values
    builtin!(sigs, "file.exists?", (a String -- a Bool));

    // file.for-each-line+: Complex quotation type - defined manually
    sigs.insert(
        "file.for-each-line+".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()).push(Type::String),
                    StackType::RowVar("a".to_string()),
                )))),
            StackType::RowVar("a".to_string())
                .push(Type::String)
                .push(Type::Bool),
        ),
    );

    // =========================================================================
    // Type Conversions
    // =========================================================================

    builtin!(sigs, "int->string", (a Int -- a String));
    builtin!(sigs, "int->float", (a Int -- a Float));
    builtin!(sigs, "float->int", (a Float -- a Int));
    builtin!(sigs, "float->string", (a Float -- a String));
    builtin!(sigs, "string->int", (a String -- a Int Bool)); // value + success flag
    builtin!(sigs, "string->float", (a String -- a Float Bool)); // value + success flag
    builtin!(sigs, "char->string", (a Int -- a String));
    builtin!(sigs, "symbol->string", (a Symbol -- a String));
    builtin!(sigs, "string->symbol", (a String -- a Symbol));

    // =========================================================================
    // Integer Arithmetic ( a Int Int -- a Int )
    // =========================================================================

    builtins_int_int_to_int!(
        sigs,
        "i.add",
        "i.subtract",
        "i.multiply",
        "i.divide",
        "i.modulo"
    );
    builtins_int_int_to_int!(sigs, "i.+", "i.-", "i.*", "i./", "i.%");

    // =========================================================================
    // Integer Comparison ( a Int Int -- a Bool )
    // =========================================================================

    builtins_int_int_to_bool!(sigs, "i.=", "i.<", "i.>", "i.<=", "i.>=", "i.<>");
    builtins_int_int_to_bool!(sigs, "i.eq", "i.lt", "i.gt", "i.lte", "i.gte", "i.neq");

    // =========================================================================
    // Boolean Operations ( a Bool Bool -- a Bool )
    // =========================================================================

    builtins_bool_bool_to_bool!(sigs, "and", "or");
    builtin!(sigs, "not", (a Bool -- a Bool));

    // =========================================================================
    // Bitwise Operations
    // =========================================================================

    builtins_int_int_to_int!(sigs, "band", "bor", "bxor", "shl", "shr");
    builtins_int_to_int!(sigs, "bnot", "popcount", "clz", "ctz");
    builtin!(sigs, "int-bits", (a -- a Int));

    // =========================================================================
    // Stack Operations (Polymorphic)
    // =========================================================================

    builtin!(sigs, "dup", (a T -- a T T));
    builtin!(sigs, "drop", (a T -- a));
    builtin!(sigs, "swap", (a T U -- a U T));
    builtin!(sigs, "over", (a T U -- a T U T));
    builtin!(sigs, "rot", (a T U V -- a U V T));
    builtin!(sigs, "nip", (a T U -- a U));
    builtin!(sigs, "tuck", (a T U -- a U T U));
    builtin!(sigs, "2dup", (a T U -- a T U T U));
    builtin!(sigs, "3drop", (a T U V -- a));

    // pick and roll: Type approximations (see detailed comments below)
    // pick: ( ..a T Int -- ..a T T ) - copies value at depth n to top
    builtin!(sigs, "pick", (a T Int -- a T T));
    // roll: ( ..a T Int -- ..a T ) - rotates n+1 items, bringing depth n to top
    builtin!(sigs, "roll", (a T Int -- a T));

    // =========================================================================
    // Channel Operations (CSP-style concurrency)
    // Errors are values, not crashes - all ops return success flags
    // =========================================================================

    builtin!(sigs, "chan.make", (a -- a Channel));
    builtin!(sigs, "chan.send", (a T Channel -- a Bool)); // returns success flag
    builtin!(sigs, "chan.receive", (a Channel -- a T Bool)); // returns value and success flag
    builtin!(sigs, "chan.close", (a Channel -- a));
    builtin!(sigs, "chan.yield", (a - -a));

    // =========================================================================
    // Quotation/Control Flow Operations
    // =========================================================================

    // call: Polymorphic - accepts Quotation or Closure
    // Uses type variable Q to represent "something callable"
    sigs.insert(
        "call".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("Q".to_string())),
            StackType::RowVar("b".to_string()),
        ),
    );

    // cond: Multi-way conditional (variable arity)
    sigs.insert(
        "cond".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()),
            StackType::RowVar("b".to_string()),
        ),
    );

    // times: ( a Quotation Int -- a ) where Quotation has effect ( a -- a )
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

    // while: ( a CondQuot BodyQuot -- a )
    // CondQuot: ( a -- a Bool ), BodyQuot: ( a -- a )
    sigs.insert(
        "while".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()).push(Type::Bool),
                ))))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("a".to_string()),
                    StackType::RowVar("a".to_string()),
                )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // until: ( a BodyQuot CondQuot -- a )
    // BodyQuot: ( a -- a ), CondQuot: ( a -- a Bool )
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
                    StackType::RowVar("a".to_string()).push(Type::Bool),
                )))),
            StackType::RowVar("a".to_string()),
        ),
    );

    // strand.spawn: ( a Quotation -- a Int ) - spawn a concurrent strand
    // The quotation can have any stack effect - it runs independently
    sigs.insert(
        "strand.spawn".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Quotation(Box::new(Effect::new(
                StackType::RowVar("spawn_in".to_string()),
                StackType::RowVar("spawn_out".to_string()),
            )))),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );

    // strand.weave: ( a Quotation -- a handle ) - create a woven strand (generator)
    // The quotation receives (WeaveCtx, first_resume_value) and must thread WeaveCtx through.
    // Returns a handle (WeaveCtx) for use with strand.resume.
    sigs.insert(
        "strand.weave".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Quotation(Box::new(Effect::new(
                StackType::RowVar("weave_in".to_string()),
                StackType::RowVar("weave_out".to_string()),
            )))),
            StackType::RowVar("a".to_string()).push(Type::Var("handle".to_string())),
        ),
    );

    // strand.resume: ( a handle b -- a handle b Bool ) - resume weave with value
    // Takes handle and value to send, returns (handle, yielded_value, has_more)
    sigs.insert(
        "strand.resume".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("handle".to_string()))
                .push(Type::Var("b".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("handle".to_string()))
                .push(Type::Var("b".to_string()))
                .push(Type::Bool),
        ),
    );

    // yield: ( a ctx b -- a ctx b | Yield b ) - yield value and receive resume value
    // The WeaveCtx must be passed explicitly and threaded through.
    // The Yield effect indicates this word produces yield semantics.
    sigs.insert(
        "yield".to_string(),
        Effect::with_effects(
            StackType::RowVar("a".to_string())
                .push(Type::Var("ctx".to_string()))
                .push(Type::Var("b".to_string())),
            StackType::RowVar("a".to_string())
                .push(Type::Var("ctx".to_string()))
                .push(Type::Var("b".to_string())),
            vec![SideEffect::Yield(Box::new(Type::Var("b".to_string())))],
        ),
    );

    // strand.weave-cancel: ( a handle -- a ) - cancel a weave and release its resources
    // Use this to clean up a weave that won't be resumed to completion.
    // This prevents resource leaks from abandoned weaves.
    sigs.insert(
        "strand.weave-cancel".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()).push(Type::Var("handle".to_string())),
            StackType::RowVar("a".to_string()),
        ),
    );

    // =========================================================================
    // TCP Operations
    // =========================================================================

    builtin!(sigs, "tcp.listen", (a Int -- a Int));
    builtin!(sigs, "tcp.accept", (a Int -- a Int));
    builtin!(sigs, "tcp.read", (a Int -- a String));
    builtin!(sigs, "tcp.write", (a String Int -- a));
    builtin!(sigs, "tcp.close", (a Int -- a));

    // =========================================================================
    // OS Operations
    // =========================================================================

    builtin!(sigs, "os.getenv", (a String -- a String Bool));
    builtin!(sigs, "os.home-dir", (a -- a String Bool));
    builtin!(sigs, "os.current-dir", (a -- a String Bool));
    builtin!(sigs, "os.path-exists", (a String -- a Bool));
    builtin!(sigs, "os.path-is-file", (a String -- a Bool));
    builtin!(sigs, "os.path-is-dir", (a String -- a Bool));
    builtin!(sigs, "os.path-join", (a String String -- a String));
    builtin!(sigs, "os.path-parent", (a String -- a String Bool));
    builtin!(sigs, "os.path-filename", (a String -- a String Bool));
    builtin!(sigs, "os.exit", (a Int -- a)); // Never returns, but typed as identity
    builtin!(sigs, "os.name", (a -- a String));
    builtin!(sigs, "os.arch", (a -- a String));

    // =========================================================================
    // String Operations
    // =========================================================================

    builtin!(sigs, "string.concat", (a String String -- a String));
    builtin!(sigs, "string.length", (a String -- a Int));
    builtin!(sigs, "string.byte-length", (a String -- a Int));
    builtin!(sigs, "string.char-at", (a String Int -- a Int));
    builtin!(sigs, "string.substring", (a String Int Int -- a String));
    builtin!(sigs, "string.find", (a String String -- a Int));
    builtin!(sigs, "string.split", (a String String -- a V)); // Returns Variant (list)
    builtin!(sigs, "string.contains", (a String String -- a Bool));
    builtin!(sigs, "string.starts-with", (a String String -- a Bool));
    builtin!(sigs, "string.empty?", (a String -- a Bool));
    builtin!(sigs, "string.equal?", (a String String -- a Bool));

    // Symbol operations
    builtin!(sigs, "symbol.=", (a Symbol Symbol -- a Bool));

    // String transformations
    builtins_string_to_string!(
        sigs,
        "string.trim",
        "string.chomp",
        "string.to-upper",
        "string.to-lower",
        "string.json-escape"
    );

    // =========================================================================
    // Encoding Operations
    // =========================================================================

    builtin!(sigs, "encoding.base64-encode", (a String -- a String));
    builtin!(sigs, "encoding.base64-decode", (a String -- a String Bool));
    builtin!(sigs, "encoding.base64url-encode", (a String -- a String));
    builtin!(sigs, "encoding.base64url-decode", (a String -- a String Bool));
    builtin!(sigs, "encoding.hex-encode", (a String -- a String));
    builtin!(sigs, "encoding.hex-decode", (a String -- a String Bool));

    // =========================================================================
    // Crypto Operations
    // =========================================================================

    builtin!(sigs, "crypto.sha256", (a String -- a String));
    builtin!(sigs, "crypto.hmac-sha256", (a String String -- a String));
    builtin!(sigs, "crypto.constant-time-eq", (a String String -- a Bool));
    builtin!(sigs, "crypto.random-bytes", (a Int -- a String));
    builtin!(sigs, "crypto.uuid4", (a -- a String));

    // =========================================================================
    // Variant Operations
    // =========================================================================

    builtin!(sigs, "variant.field-count", (a V -- a Int));
    builtin!(sigs, "variant.tag", (a V -- a Symbol));
    builtin!(sigs, "variant.field-at", (a V Int -- a T));
    builtin!(sigs, "variant.append", (a V T -- a V2));
    builtin!(sigs, "variant.last", (a V -- a T));
    builtin!(sigs, "variant.init", (a V -- a V2));

    // Type-safe variant constructors with fixed arity (symbol tags for SON support)
    builtin!(sigs, "variant.make-0", (a Symbol -- a V));
    builtin!(sigs, "variant.make-1", (a T1 Symbol -- a V));
    builtin!(sigs, "variant.make-2", (a T1 T2 Symbol -- a V));
    builtin!(sigs, "variant.make-3", (a T1 T2 T3 Symbol -- a V));
    builtin!(sigs, "variant.make-4", (a T1 T2 T3 T4 Symbol -- a V));

    // Aliases for dynamic variant construction (SON-friendly names)
    builtin!(sigs, "wrap-0", (a Symbol -- a V));
    builtin!(sigs, "wrap-1", (a T1 Symbol -- a V));
    builtin!(sigs, "wrap-2", (a T1 T2 Symbol -- a V));
    builtin!(sigs, "wrap-3", (a T1 T2 T3 Symbol -- a V));
    builtin!(sigs, "wrap-4", (a T1 T2 T3 T4 Symbol -- a V));

    // =========================================================================
    // List Operations (Higher-order combinators for Variants)
    // =========================================================================

    // List construction and access
    builtin!(sigs, "list.make", (a -- a V));
    builtin!(sigs, "list.push", (a V T -- a V));
    builtin!(sigs, "list.get", (a V Int -- a T Bool));
    builtin!(sigs, "list.set", (a V Int T -- a V Bool));

    builtin!(sigs, "list.length", (a V -- a Int));
    builtin!(sigs, "list.empty?", (a V -- a Bool));

    // list.map: ( a Variant Quotation -- a Variant )
    // Quotation: ( b T -- b U )
    sigs.insert(
        "list.map".to_string(),
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

    // list.filter: ( a Variant Quotation -- a Variant )
    // Quotation: ( b T -- b Bool )
    sigs.insert(
        "list.filter".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Var("V".to_string()))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::RowVar("b".to_string()).push(Type::Var("T".to_string())),
                    StackType::RowVar("b".to_string()).push(Type::Bool),
                )))),
            StackType::RowVar("a".to_string()).push(Type::Var("V2".to_string())),
        ),
    );

    // list.fold: ( a Variant init Quotation -- a result )
    // Quotation: ( b Acc T -- b Acc )
    sigs.insert(
        "list.fold".to_string(),
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

    // list.each: ( a Variant Quotation -- a )
    // Quotation: ( b T -- b )
    sigs.insert(
        "list.each".to_string(),
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

    // =========================================================================
    // Map Operations (Dictionary with O(1) lookup)
    // =========================================================================

    builtin!(sigs, "map.make", (a -- a M));
    builtin!(sigs, "map.get", (a M K -- a V Bool)); // returns (value success) - errors are values, not crashes
    builtin!(sigs, "map.set", (a M K V -- a M2));
    builtin!(sigs, "map.has?", (a M K -- a Bool));
    builtin!(sigs, "map.remove", (a M K -- a M2));
    builtin!(sigs, "map.keys", (a M -- a V));
    builtin!(sigs, "map.values", (a M -- a V));
    builtin!(sigs, "map.size", (a M -- a Int));
    builtin!(sigs, "map.empty?", (a M -- a Bool));

    // =========================================================================
    // Float Arithmetic ( a Float Float -- a Float )
    // =========================================================================

    builtins_float_float_to_float!(sigs, "f.add", "f.subtract", "f.multiply", "f.divide");
    builtins_float_float_to_float!(sigs, "f.+", "f.-", "f.*", "f./");

    // =========================================================================
    // Float Comparison ( a Float Float -- a Bool )
    // =========================================================================

    builtins_float_float_to_bool!(sigs, "f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>");
    builtins_float_float_to_bool!(sigs, "f.eq", "f.lt", "f.gt", "f.lte", "f.gte", "f.neq");

    // =========================================================================
    // Test Framework
    // =========================================================================

    builtin!(sigs, "test.init", (a String -- a));
    builtin!(sigs, "test.finish", (a - -a));
    builtin!(sigs, "test.has-failures", (a -- a Bool));
    builtin!(sigs, "test.assert", (a Bool -- a));
    builtin!(sigs, "test.assert-not", (a Bool -- a));
    builtin!(sigs, "test.assert-eq", (a Int Int -- a));
    builtin!(sigs, "test.assert-eq-str", (a String String -- a));
    builtin!(sigs, "test.fail", (a String -- a));
    builtin!(sigs, "test.pass-count", (a -- a Int));
    builtin!(sigs, "test.fail-count", (a -- a Int));

    // Time operations
    builtin!(sigs, "time.now", (a -- a Int));
    builtin!(sigs, "time.nanos", (a -- a Int));
    builtin!(sigs, "time.sleep-ms", (a Int -- a));

    // SON serialization
    builtin!(sigs, "son.dump", (a T -- a String));
    builtin!(sigs, "son.dump-pretty", (a T -- a String));

    // Stack introspection (for REPL)
    // stack.dump prints all values and clears the stack
    sigs.insert(
        "stack.dump".to_string(),
        Effect::new(
            StackType::RowVar("a".to_string()), // Consumes any stack
            StackType::RowVar("b".to_string()), // Returns empty stack (different row var)
        ),
    );

    sigs
}

/// Get documentation for a built-in word
pub fn builtin_doc(name: &str) -> Option<&'static str> {
    BUILTIN_DOCS.get(name).copied()
}

/// Get all built-in word documentation (cached with LazyLock for performance)
pub fn builtin_docs() -> &'static HashMap<&'static str, &'static str> {
    &BUILTIN_DOCS
}

/// Lazily initialized documentation for all built-in words
static BUILTIN_DOCS: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut docs = HashMap::new();

    // I/O Operations
    docs.insert(
        "io.write",
        "Write a string to stdout without a trailing newline.",
    );
    docs.insert(
        "io.write-line",
        "Write a string to stdout followed by a newline.",
    );
    docs.insert(
        "io.read-line",
        "Read a line from stdin. Returns (line, success).",
    );
    docs.insert(
        "io.read-line+",
        "Read a line from stdin. Returns (line, status_code).",
    );
    docs.insert(
        "io.read-n",
        "Read N bytes from stdin. Returns (bytes, status_code).",
    );

    // Command-line Arguments
    docs.insert("args.count", "Get the number of command-line arguments.");
    docs.insert("args.at", "Get the command-line argument at index N.");

    // File Operations
    docs.insert(
        "file.slurp",
        "Read entire file contents. Returns (content, success).",
    );
    docs.insert("file.exists?", "Check if a file exists at the given path.");
    docs.insert(
        "file.for-each-line+",
        "Execute a quotation for each line in a file.",
    );

    // Type Conversions
    docs.insert(
        "int->string",
        "Convert an integer to its string representation.",
    );
    docs.insert(
        "int->float",
        "Convert an integer to a floating-point number.",
    );
    docs.insert("float->int", "Truncate a float to an integer.");
    docs.insert(
        "float->string",
        "Convert a float to its string representation.",
    );
    docs.insert(
        "string->int",
        "Parse a string as an integer. Returns (value, success).",
    );
    docs.insert(
        "string->float",
        "Parse a string as a float. Returns (value, success).",
    );
    docs.insert(
        "char->string",
        "Convert a Unicode codepoint to a single-character string.",
    );
    docs.insert(
        "symbol->string",
        "Convert a symbol to its string representation.",
    );
    docs.insert("string->symbol", "Intern a string as a symbol.");

    // Integer Arithmetic
    docs.insert("i.add", "Add two integers.");
    docs.insert("i.subtract", "Subtract second integer from first.");
    docs.insert("i.multiply", "Multiply two integers.");
    docs.insert("i.divide", "Integer division (truncates toward zero).");
    docs.insert("i.modulo", "Integer modulo (remainder after division).");
    docs.insert("i.+", "Add two integers.");
    docs.insert("i.-", "Subtract second integer from first.");
    docs.insert("i.*", "Multiply two integers.");
    docs.insert("i./", "Integer division (truncates toward zero).");
    docs.insert("i.%", "Integer modulo (remainder after division).");

    // Integer Comparison
    docs.insert("i.=", "Test if two integers are equal.");
    docs.insert("i.<", "Test if first integer is less than second.");
    docs.insert("i.>", "Test if first integer is greater than second.");
    docs.insert(
        "i.<=",
        "Test if first integer is less than or equal to second.",
    );
    docs.insert(
        "i.>=",
        "Test if first integer is greater than or equal to second.",
    );
    docs.insert("i.<>", "Test if two integers are not equal.");
    docs.insert("i.eq", "Test if two integers are equal.");
    docs.insert("i.lt", "Test if first integer is less than second.");
    docs.insert("i.gt", "Test if first integer is greater than second.");
    docs.insert(
        "i.lte",
        "Test if first integer is less than or equal to second.",
    );
    docs.insert(
        "i.gte",
        "Test if first integer is greater than or equal to second.",
    );
    docs.insert("i.neq", "Test if two integers are not equal.");

    // Boolean Operations
    docs.insert("and", "Logical AND of two booleans.");
    docs.insert("or", "Logical OR of two booleans.");
    docs.insert("not", "Logical NOT of a boolean.");

    // Bitwise Operations
    docs.insert("band", "Bitwise AND of two integers.");
    docs.insert("bor", "Bitwise OR of two integers.");
    docs.insert("bxor", "Bitwise XOR of two integers.");
    docs.insert("bnot", "Bitwise NOT (complement) of an integer.");
    docs.insert("shl", "Shift left by N bits.");
    docs.insert("shr", "Shift right by N bits (arithmetic).");
    docs.insert("popcount", "Count the number of set bits.");
    docs.insert("clz", "Count leading zeros.");
    docs.insert("ctz", "Count trailing zeros.");
    docs.insert("int-bits", "Push the bit width of integers (64).");

    // Stack Operations
    docs.insert("dup", "Duplicate the top stack value.");
    docs.insert("drop", "Remove the top stack value.");
    docs.insert("swap", "Swap the top two stack values.");
    docs.insert("over", "Copy the second value to the top.");
    docs.insert("rot", "Rotate the top three values (third to top).");
    docs.insert("nip", "Remove the second value from the stack.");
    docs.insert("tuck", "Copy the top value below the second.");
    docs.insert("2dup", "Duplicate the top two values.");
    docs.insert("3drop", "Remove the top three values.");
    docs.insert("pick", "Copy the value at depth N to the top.");
    docs.insert("roll", "Rotate N+1 items, bringing depth N to top.");

    // Channel Operations
    docs.insert(
        "chan.make",
        "Create a new channel for inter-strand communication.",
    );
    docs.insert(
        "chan.send",
        "Send a value on a channel. Returns success flag.",
    );
    docs.insert(
        "chan.receive",
        "Receive a value from a channel. Returns (value, success).",
    );
    docs.insert("chan.close", "Close a channel.");
    docs.insert("chan.yield", "Yield control to the scheduler.");

    // Control Flow
    docs.insert("call", "Call a quotation or closure.");
    docs.insert(
        "cond",
        "Multi-way conditional: test clauses until one succeeds.",
    );
    docs.insert("times", "Execute a quotation N times.");
    docs.insert("while", "Loop while condition is true: [cond] [body] while");
    docs.insert("until", "Loop until condition is true: [body] [cond] until");

    // Concurrency
    docs.insert(
        "strand.spawn",
        "Spawn a concurrent strand. Returns strand ID.",
    );
    docs.insert(
        "strand.weave",
        "Create a generator/coroutine. Returns handle.",
    );
    docs.insert(
        "strand.resume",
        "Resume a weave with a value. Returns (handle, value, has_more).",
    );
    docs.insert(
        "yield",
        "Yield a value from a weave and receive resume value.",
    );
    docs.insert(
        "strand.weave-cancel",
        "Cancel a weave and release its resources.",
    );

    // TCP Operations
    docs.insert(
        "tcp.listen",
        "Start listening on a port. Returns socket ID.",
    );
    docs.insert(
        "tcp.accept",
        "Accept a connection. Returns client socket ID.",
    );
    docs.insert("tcp.read", "Read data from a socket. Returns string.");
    docs.insert("tcp.write", "Write data to a socket.");
    docs.insert("tcp.close", "Close a socket.");

    // OS Operations
    docs.insert(
        "os.getenv",
        "Get environment variable. Returns (value, exists).",
    );
    docs.insert(
        "os.home-dir",
        "Get user's home directory. Returns (path, success).",
    );
    docs.insert(
        "os.current-dir",
        "Get current working directory. Returns (path, success).",
    );
    docs.insert("os.path-exists", "Check if a path exists.");
    docs.insert("os.path-is-file", "Check if path is a regular file.");
    docs.insert("os.path-is-dir", "Check if path is a directory.");
    docs.insert("os.path-join", "Join two path components.");
    docs.insert(
        "os.path-parent",
        "Get parent directory. Returns (path, success).",
    );
    docs.insert(
        "os.path-filename",
        "Get filename component. Returns (name, success).",
    );
    docs.insert("os.exit", "Exit the program with a status code.");
    docs.insert(
        "os.name",
        "Get the operating system name (e.g., \"macos\", \"linux\").",
    );
    docs.insert(
        "os.arch",
        "Get the CPU architecture (e.g., \"aarch64\", \"x86_64\").",
    );

    // String Operations
    docs.insert("string.concat", "Concatenate two strings.");
    docs.insert("string.length", "Get the character length of a string.");
    docs.insert("string.byte-length", "Get the byte length of a string.");
    docs.insert(
        "string.char-at",
        "Get Unicode codepoint at character index.",
    );
    docs.insert(
        "string.substring",
        "Extract substring from start index with length.",
    );
    docs.insert(
        "string.find",
        "Find substring. Returns index or -1 if not found.",
    );
    docs.insert("string.split", "Split string by delimiter. Returns a list.");
    docs.insert("string.contains", "Check if string contains a substring.");
    docs.insert(
        "string.starts-with",
        "Check if string starts with a prefix.",
    );
    docs.insert("string.empty?", "Check if string is empty.");
    docs.insert("string.equal?", "Check if two strings are equal.");
    docs.insert("string.trim", "Remove leading and trailing whitespace.");
    docs.insert("string.chomp", "Remove trailing newline.");
    docs.insert("string.to-upper", "Convert to uppercase.");
    docs.insert("string.to-lower", "Convert to lowercase.");
    docs.insert("string.json-escape", "Escape special characters for JSON.");
    docs.insert("symbol.=", "Check if two symbols are equal.");

    // Encoding Operations
    docs.insert(
        "encoding.base64-encode",
        "Encode a string to Base64 (standard alphabet with padding).",
    );
    docs.insert(
        "encoding.base64-decode",
        "Decode a Base64 string. Returns (decoded, success).",
    );
    docs.insert(
        "encoding.base64url-encode",
        "Encode to URL-safe Base64 (no padding). Suitable for JWTs and URLs.",
    );
    docs.insert(
        "encoding.base64url-decode",
        "Decode URL-safe Base64. Returns (decoded, success).",
    );
    docs.insert(
        "encoding.hex-encode",
        "Encode a string to lowercase hexadecimal.",
    );
    docs.insert(
        "encoding.hex-decode",
        "Decode a hexadecimal string. Returns (decoded, success).",
    );

    // Crypto Operations
    docs.insert(
        "crypto.sha256",
        "Compute SHA-256 hash of a string. Returns 64-char hex digest.",
    );
    docs.insert(
        "crypto.hmac-sha256",
        "Compute HMAC-SHA256 signature. ( message key -- signature )",
    );
    docs.insert(
        "crypto.constant-time-eq",
        "Timing-safe string comparison. Use for comparing signatures/tokens.",
    );
    docs.insert(
        "crypto.random-bytes",
        "Generate N cryptographically secure random bytes as hex string.",
    );
    docs.insert("crypto.uuid4", "Generate a random UUID v4 string.");

    // Variant Operations
    docs.insert(
        "variant.field-count",
        "Get the number of fields in a variant.",
    );
    docs.insert(
        "variant.tag",
        "Get the tag (constructor name) of a variant.",
    );
    docs.insert("variant.field-at", "Get the field at index N.");
    docs.insert(
        "variant.append",
        "Append a value to a variant (creates new).",
    );
    docs.insert("variant.last", "Get the last field of a variant.");
    docs.insert("variant.init", "Get all fields except the last.");
    docs.insert("variant.make-0", "Create a variant with 0 fields.");
    docs.insert("variant.make-1", "Create a variant with 1 field.");
    docs.insert("variant.make-2", "Create a variant with 2 fields.");
    docs.insert("variant.make-3", "Create a variant with 3 fields.");
    docs.insert("variant.make-4", "Create a variant with 4 fields.");
    docs.insert("wrap-0", "Create a variant with 0 fields (alias).");
    docs.insert("wrap-1", "Create a variant with 1 field (alias).");
    docs.insert("wrap-2", "Create a variant with 2 fields (alias).");
    docs.insert("wrap-3", "Create a variant with 3 fields (alias).");
    docs.insert("wrap-4", "Create a variant with 4 fields (alias).");

    // List Operations
    docs.insert("list.make", "Create an empty list.");
    docs.insert("list.push", "Push a value onto a list. Returns new list.");
    docs.insert("list.get", "Get value at index. Returns (value, success).");
    docs.insert("list.set", "Set value at index. Returns (list, success).");
    docs.insert("list.length", "Get the number of elements in a list.");
    docs.insert("list.empty?", "Check if a list is empty.");
    docs.insert(
        "list.map",
        "Apply quotation to each element. Returns new list.",
    );
    docs.insert("list.filter", "Keep elements where quotation returns true.");
    docs.insert("list.fold", "Reduce list with accumulator and quotation.");
    docs.insert(
        "list.each",
        "Execute quotation for each element (side effects).",
    );

    // Map Operations
    docs.insert("map.make", "Create an empty map.");
    docs.insert("map.get", "Get value for key. Returns (value, success).");
    docs.insert("map.set", "Set key to value. Returns new map.");
    docs.insert("map.has?", "Check if map contains a key.");
    docs.insert("map.remove", "Remove a key. Returns new map.");
    docs.insert("map.keys", "Get all keys as a list.");
    docs.insert("map.values", "Get all values as a list.");
    docs.insert("map.size", "Get the number of key-value pairs.");
    docs.insert("map.empty?", "Check if map is empty.");

    // Float Arithmetic
    docs.insert("f.add", "Add two floats.");
    docs.insert("f.subtract", "Subtract second float from first.");
    docs.insert("f.multiply", "Multiply two floats.");
    docs.insert("f.divide", "Divide first float by second.");
    docs.insert("f.+", "Add two floats.");
    docs.insert("f.-", "Subtract second float from first.");
    docs.insert("f.*", "Multiply two floats.");
    docs.insert("f./", "Divide first float by second.");

    // Float Comparison
    docs.insert("f.=", "Test if two floats are equal.");
    docs.insert("f.<", "Test if first float is less than second.");
    docs.insert("f.>", "Test if first float is greater than second.");
    docs.insert("f.<=", "Test if first float is less than or equal.");
    docs.insert("f.>=", "Test if first float is greater than or equal.");
    docs.insert("f.<>", "Test if two floats are not equal.");
    docs.insert("f.eq", "Test if two floats are equal.");
    docs.insert("f.lt", "Test if first float is less than second.");
    docs.insert("f.gt", "Test if first float is greater than second.");
    docs.insert("f.lte", "Test if first float is less than or equal.");
    docs.insert("f.gte", "Test if first float is greater than or equal.");
    docs.insert("f.neq", "Test if two floats are not equal.");

    // Test Framework
    docs.insert(
        "test.init",
        "Initialize the test framework with a test name.",
    );
    docs.insert("test.finish", "Finish testing and print results.");
    docs.insert("test.has-failures", "Check if any tests have failed.");
    docs.insert("test.assert", "Assert that a boolean is true.");
    docs.insert("test.assert-not", "Assert that a boolean is false.");
    docs.insert("test.assert-eq", "Assert that two integers are equal.");
    docs.insert("test.assert-eq-str", "Assert that two strings are equal.");
    docs.insert("test.fail", "Mark a test as failed with a message.");
    docs.insert("test.pass-count", "Get the number of passed assertions.");
    docs.insert("test.fail-count", "Get the number of failed assertions.");

    // Time Operations
    docs.insert("time.now", "Get current Unix timestamp in seconds.");
    docs.insert(
        "time.nanos",
        "Get high-resolution monotonic time in nanoseconds.",
    );
    docs.insert("time.sleep-ms", "Sleep for N milliseconds.");

    // Serialization
    docs.insert("son.dump", "Serialize any value to SON format (compact).");
    docs.insert(
        "son.dump-pretty",
        "Serialize any value to SON format (pretty-printed).",
    );

    // Stack Introspection
    docs.insert(
        "stack.dump",
        "Print all stack values and clear the stack (REPL).",
    );

    docs
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_signature_write_line() {
        let sig = builtin_signature("io.write-line").unwrap();
        // ( ..a String -- ..a )
        let (rest, top) = sig.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
        assert_eq!(sig.outputs, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_builtin_signature_i_add() {
        let sig = builtin_signature("i.add").unwrap();
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
        assert!(sigs.contains_key("io.write-line"));
        assert!(sigs.contains_key("io.read-line"));
        assert!(sigs.contains_key("int->string"));
        assert!(sigs.contains_key("i.add"));
        assert!(sigs.contains_key("dup"));
        assert!(sigs.contains_key("swap"));
        assert!(sigs.contains_key("chan.make"));
        assert!(sigs.contains_key("chan.send"));
        assert!(sigs.contains_key("chan.receive"));
        assert!(
            sigs.contains_key("string->float"),
            "string->float should be a builtin"
        );
    }

    #[test]
    fn test_all_docs_have_signatures() {
        let sigs = builtin_signatures();
        let docs = builtin_docs();

        for name in docs.keys() {
            assert!(
                sigs.contains_key(*name),
                "Builtin '{}' has documentation but no signature",
                name
            );
        }
    }

    #[test]
    fn test_all_signatures_have_docs() {
        let sigs = builtin_signatures();
        let docs = builtin_docs();

        for name in sigs.keys() {
            assert!(
                docs.contains_key(name.as_str()),
                "Builtin '{}' has signature but no documentation",
                name
            );
        }
    }
}
