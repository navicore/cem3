//! List operations.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
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
    builtin!(sigs, "list.reverse", (a V -- a V));

    // Convenience accessors. Match `list.get`'s (value Bool) shape so
    // empty lists fall through the same error-flag lint as out-of-bounds.
    builtin!(sigs, "list.first", (a V -- a T Bool));
    builtin!(sigs, "list.last",  (a V -- a T Bool));

    // list.map: ( a V [b T -- b U] -- a V2 )
    sigs.insert(
        "list.map".to_string(),
        Effect::new(
            stack!(a V).push(quot(stack!(b T), stack!(b U))),
            stack!(a V2),
        ),
    );

    // list.filter: ( a V [b T -- b Bool] -- a V2 )
    sigs.insert(
        "list.filter".to_string(),
        Effect::new(
            stack!(a V).push(quot(stack!(b T), stack!(b Bool))),
            stack!(a V2),
        ),
    );

    // list.fold: ( a V Acc [b Acc T -- b Acc] -- a Acc )
    sigs.insert(
        "list.fold".to_string(),
        Effect::new(
            stack!(a V Acc).push(quot(stack!(b Acc T), stack!(b Acc))),
            stack!(a Acc),
        ),
    );

    // list.each: ( a V [b T -- b] -- a )
    sigs.insert(
        "list.each".to_string(),
        Effect::new(stack!(a V).push(quot(stack!(b T), stack!(b))), stack!(a)),
    );
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
    // List Operations
    docs.insert("list.make", "Create an empty list.");
    docs.insert("list.push", "Push a value onto a list. Returns new list.");
    docs.insert(
        "list.get",
        "Get value at index. Returns (value Bool) -- Bool is false if index out of bounds.",
    );
    docs.insert(
        "list.set",
        "Set value at index. Returns (List Bool) -- Bool is false if index out of bounds.",
    );
    docs.insert("list.length", "Get the number of elements in a list.");
    docs.insert("list.empty?", "Check if a list is empty.");
    docs.insert("list.reverse", "Reverse the elements of a list.");
    docs.insert(
        "list.first",
        "Get the first element. Returns (value Bool) -- Bool is false on an empty list.",
    );
    docs.insert(
        "list.last",
        "Get the last element. Returns (value Bool) -- Bool is false on an empty list.",
    );
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
}
