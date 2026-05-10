//! Shared declarative macros for building builtin signatures.
//!
//! `builtin!` is the primary entry point, but the family-specific macros
//! (`builtins_int_int_to_int!`, …) reduce boilerplate for common signature
//! shapes. All are re-exported with `pub(super)` so category sub-modules
//! can import them via `use super::macros::*;`.

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
    (Socket) => {
        Type::Socket
    };
    (Variant) => {
        Type::Variant
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
    // (a T -- a U V W)
    ($sigs:ident, $name:expr, (a $i1:tt -- a $o1:tt $o2:tt $o3:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1), stack!(a $o1 $o2 $o3)));
    };
    // (a T -- a U V W X)
    ($sigs:ident, $name:expr, (a $i1:tt -- a $o1:tt $o2:tt $o3:tt $o4:tt)) => {
        $sigs.insert($name.to_string(), Effect::new(stack!(a $i1), stack!(a $o1 $o2 $o3 $o4)));
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

pub(super) use builtin;
pub(super) use builtins_bool_bool_to_bool;
pub(super) use builtins_float_float_to_bool;
pub(super) use builtins_float_float_to_float;
pub(super) use builtins_int_int_to_bool;
pub(super) use builtins_int_int_to_int;
pub(super) use builtins_int_to_int;
pub(super) use builtins_string_to_string;
pub(super) use stack;
pub(super) use ty;
