//! Built-in word signatures for Seq
//!
//! Defines the stack effects for all runtime built-in operations.
//!
//! Uses declarative macros to minimize boilerplate. The `builtin!` macro
//! supports a Forth-like notation: `(a Type1 Type2 -- a Type3)` where:
//! - `a` is the row variable (representing "rest of stack")
//! - Concrete types: `Int`, `String`, `Float`
//! - Type variables: single uppercase letters like `T`, `U`, `V`

use crate::types::{Effect, StackType, Type};
use std::collections::HashMap;

/// Convert a type token to a Type expression
macro_rules! ty {
    (Int) => {
        Type::Int
    };
    (String) => {
        Type::String
    };
    (Float) => {
        Type::Float
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

macro_rules! builtins_float_float_to_int {
    ($sigs:ident, $($name:expr),+ $(,)?) => {
        $(
            builtin!($sigs, $name, (a Float Float -- a Int));
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

    builtin!(sigs, "io.write-line", (a String -- a));
    builtin!(sigs, "io.read-line", (a -- a String));
    builtin!(sigs, "io.read-line+", (a -- a String Int)); // Returns line + status
    builtin!(sigs, "io.read-n", (a Int -- a String Int)); // Read N bytes, returns bytes + status

    // =========================================================================
    // Command-line Arguments
    // =========================================================================

    builtin!(sigs, "args.count", (a -- a Int));
    builtin!(sigs, "args.at", (a Int -- a String));

    // =========================================================================
    // File Operations
    // =========================================================================

    builtin!(sigs, "file.slurp", (a String -- a String));
    builtin!(sigs, "file.slurp-safe", (a String -- a String Int));
    builtin!(sigs, "file.exists?", (a String -- a Int));

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
                .push(Type::Int),
        ),
    );

    // =========================================================================
    // Type Conversions
    // =========================================================================

    builtin!(sigs, "int->string", (a Int -- a String));
    builtin!(sigs, "int->float", (a Int -- a Float));
    builtin!(sigs, "float->int", (a Float -- a Int));
    builtin!(sigs, "float->string", (a Float -- a String));
    builtin!(sigs, "string->int", (a String -- a Int Int)); // value + success flag
    builtin!(sigs, "string->float", (a String -- a Float Int)); // value + success flag
    builtin!(sigs, "char->string", (a Int -- a String));

    // =========================================================================
    // Integer Arithmetic ( a Int Int -- a Int )
    // =========================================================================

    builtins_int_int_to_int!(sigs, "add", "subtract", "multiply", "divide");

    // =========================================================================
    // Integer Comparison ( a Int Int -- a Int )
    // =========================================================================

    builtins_int_int_to_int!(sigs, "=", "<", ">", "<=", ">=", "<>");

    // =========================================================================
    // Boolean Operations
    // =========================================================================

    builtins_int_int_to_int!(sigs, "and", "or");
    builtin!(sigs, "not", (a Int -- a Int));

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
    // =========================================================================

    builtin!(sigs, "chan.make", (a -- a Int));
    builtin!(sigs, "chan.send", (a T Int -- a));
    builtin!(sigs, "chan.send-safe", (a T Int -- a Int));
    builtin!(sigs, "chan.receive", (a Int -- a T));
    builtin!(sigs, "chan.receive-safe", (a Int -- a T Int));
    builtin!(sigs, "chan.close", (a Int -- a));
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
    // CondQuot: ( a -- a Int ), BodyQuot: ( a -- a )
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

    // until: ( a BodyQuot CondQuot -- a )
    // BodyQuot: ( a -- a ), CondQuot: ( a -- a Int )
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

    // spawn: ( a Quotation -- a Int ) - quotation can have any effect
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

    builtin!(sigs, "os.getenv", (a String -- a String Int));
    builtin!(sigs, "os.home-dir", (a -- a String Int));
    builtin!(sigs, "os.current-dir", (a -- a String Int));
    builtin!(sigs, "os.path-exists", (a String -- a Int));
    builtin!(sigs, "os.path-is-file", (a String -- a Int));
    builtin!(sigs, "os.path-is-dir", (a String -- a Int));
    builtin!(sigs, "os.path-join", (a String String -- a String));
    builtin!(sigs, "os.path-parent", (a String -- a String Int));
    builtin!(sigs, "os.path-filename", (a String -- a String Int));
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
    builtin!(sigs, "string.contains", (a String String -- a Int));
    builtin!(sigs, "string.starts-with", (a String String -- a Int));
    builtin!(sigs, "string.empty?", (a String -- a Int));
    builtin!(sigs, "string.equal?", (a String String -- a Int));

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
    // Variant Operations
    // =========================================================================

    builtin!(sigs, "variant.field-count", (a V -- a Int));
    builtin!(sigs, "variant.tag", (a V -- a Int));
    builtin!(sigs, "variant.field-at", (a V Int -- a T));
    builtin!(sigs, "variant.append", (a V T -- a V2));
    builtin!(sigs, "variant.last", (a V -- a T));
    builtin!(sigs, "variant.init", (a V -- a V2));

    // Type-safe variant constructors with fixed arity
    builtin!(sigs, "variant.make-0", (a Int -- a V));
    builtin!(sigs, "variant.make-1", (a T1 Int -- a V));
    builtin!(sigs, "variant.make-2", (a T1 T2 Int -- a V));
    builtin!(sigs, "variant.make-3", (a T1 T2 T3 Int -- a V));
    builtin!(sigs, "variant.make-4", (a T1 T2 T3 T4 Int -- a V));

    // =========================================================================
    // List Operations (Higher-order combinators for Variants)
    // =========================================================================

    builtin!(sigs, "list.length", (a V -- a Int));
    builtin!(sigs, "list.empty?", (a V -- a Int));

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
    // Quotation: ( b T -- b Int )
    sigs.insert(
        "list.filter".to_string(),
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
    builtin!(sigs, "map.get", (a M K -- a V));
    builtin!(sigs, "map.get-safe", (a M K -- a V Int));
    builtin!(sigs, "map.set", (a M K V -- a M2));
    builtin!(sigs, "map.has?", (a M K -- a Int));
    builtin!(sigs, "map.remove", (a M K -- a M2));
    builtin!(sigs, "map.keys", (a M -- a V));
    builtin!(sigs, "map.values", (a M -- a V));
    builtin!(sigs, "map.size", (a M -- a Int));
    builtin!(sigs, "map.empty?", (a M -- a Int));

    // =========================================================================
    // Float Arithmetic ( a Float Float -- a Float )
    // =========================================================================

    builtins_float_float_to_float!(sigs, "f.add", "f.subtract", "f.multiply", "f.divide");

    // =========================================================================
    // Float Comparison ( a Float Float -- a Int )
    // =========================================================================

    builtins_float_float_to_int!(sigs, "f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>");

    // =========================================================================
    // Test Framework
    // =========================================================================

    builtin!(sigs, "test.init", (a String -- a));
    builtin!(sigs, "test.finish", (a - -a));
    builtin!(sigs, "test.has-failures", (a -- a Int));
    builtin!(sigs, "test.assert", (a Int -- a));
    builtin!(sigs, "test.assert-not", (a Int -- a));
    builtin!(sigs, "test.assert-eq", (a Int Int -- a));
    builtin!(sigs, "test.assert-eq-str", (a String String -- a));
    builtin!(sigs, "test.fail", (a String -- a));
    builtin!(sigs, "test.pass-count", (a -- a Int));
    builtin!(sigs, "test.fail-count", (a -- a Int));

    // Time operations
    builtin!(sigs, "time.now", (a -- a Int));
    builtin!(sigs, "time.nanos", (a -- a Int));
    builtin!(sigs, "time.sleep-ms", (a Int -- a));

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
        assert!(sigs.contains_key("io.write-line"));
        assert!(sigs.contains_key("io.read-line"));
        assert!(sigs.contains_key("int->string"));
        assert!(sigs.contains_key("add"));
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
}
