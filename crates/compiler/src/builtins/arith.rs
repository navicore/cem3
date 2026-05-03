//! Integer arithmetic, comparison, boolean, and bitwise operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
    // =========================================================================
    // Integer Arithmetic ( a Int Int -- a Int )
    // =========================================================================

    builtins_int_int_to_int!(sigs, "i.add", "i.subtract", "i.multiply");
    builtins_int_int_to_int!(sigs, "i.+", "i.-", "i.*");

    // Division operations return ( a Int Int -- a Int Bool ) for error handling
    builtin!(sigs, "i.divide", (a Int Int -- a Int Bool));
    builtin!(sigs, "i.modulo", (a Int Int -- a Int Bool));
    builtin!(sigs, "i./", (a Int Int -- a Int Bool));
    builtin!(sigs, "i.%", (a Int Int -- a Int Bool));

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
    builtins_int_to_int!(sigs, "i.neg", "negate"); // Integer negation (inline)
    builtin!(sigs, "int-bits", (a -- a Int));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // Integer Arithmetic
    docs.insert("i.add", "Add two integers.");
    docs.insert("i.subtract", "Subtract second integer from first.");
    docs.insert("i.multiply", "Multiply two integers.");
    docs.insert(
        "i.divide",
        "Integer division. Returns (result Bool) -- Bool is false on division by zero.",
    );
    docs.insert(
        "i.modulo",
        "Integer modulo. Returns (result Bool) -- Bool is false on division by zero.",
    );
    docs.insert("i.+", "Add two integers.");
    docs.insert("i.-", "Subtract second integer from first.");
    docs.insert("i.*", "Multiply two integers.");
    docs.insert(
        "i./",
        "Integer division. Returns (result Bool) -- Bool is false on division by zero.",
    );
    docs.insert(
        "i.%",
        "Integer modulo. Returns (result Bool) -- Bool is false on division by zero.",
    );

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
    docs.insert(
        "i.neg",
        "Negate an integer (0 - n). Canonical name; `negate` is an alias.",
    );
    docs.insert(
        "negate",
        "Negate an integer (0 - n). Ergonomic alias for `i.neg`.",
    );
    docs.insert("popcount", "Count the number of set bits.");
    docs.insert("clz", "Count leading zeros.");
    docs.insert("ctz", "Count trailing zeros.");
    docs.insert("int-bits", "Push the bit width of integers (63).");
}
