//! Map operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
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

    // map.each: ( a M [b K V -- b] -- a )
    sigs.insert(
        "map.each".to_string(),
        Effect::new(stack!(a M).push(quot(stack!(b K V), stack!(b))), stack!(a)),
    );

    // map.fold: ( a M Acc [b Acc K V -- b Acc] -- a Acc )
    sigs.insert(
        "map.fold".to_string(),
        Effect::new(
            stack!(a M Acc).push(quot(stack!(b Acc).push(ty!(K)).push(ty!(V)), stack!(b Acc))),
            stack!(a Acc),
        ),
    );
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // Map Operations
    docs.insert("map.make", "Create an empty map.");
    docs.insert(
        "map.get",
        "Get value for key. Returns (value Bool) -- Bool is false if key not found.",
    );
    docs.insert("map.set", "Set key to value. Returns new map.");
    docs.insert("map.has?", "Check if map contains a key.");
    docs.insert("map.remove", "Remove a key. Returns new map.");
    docs.insert("map.keys", "Get all keys as a list.");
    docs.insert("map.values", "Get all values as a list.");
    docs.insert("map.size", "Get the number of key-value pairs.");
    docs.insert("map.empty?", "Check if map is empty.");
    docs.insert(
        "map.each",
        "Iterate key-value pairs. Quotation: ( key value -- ).",
    );
    docs.insert(
        "map.fold",
        "Fold over key-value pairs with accumulator. Quotation: ( acc key value -- acc' ).",
    );
}
