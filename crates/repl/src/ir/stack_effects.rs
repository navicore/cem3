//! Stack Effect Definitions
//!
//! Declarative definitions of stack effects for builtin words.
//! Used by the REPL to display stack transition visualizations.

use super::stack_art::{Stack, StackEffect, StackValue};
use std::collections::HashMap;
use std::sync::LazyLock;

/// Static table of stack effects for all known builtins
static EFFECTS: LazyLock<HashMap<&'static str, StackEffect>> = LazyLock::new(build_effects);

/// Look up a stack effect by word name
pub fn get_effect(word: &str) -> Option<&'static StackEffect> {
    EFFECTS.get(word)
}

/// Build the complete effects table
fn build_effects() -> HashMap<&'static str, StackEffect> {
    let mut m = HashMap::new();

    // Stack manipulation
    // dup ( ..a x -- ..a x x )
    effect(&mut m, "dup", &["x"], &["x", "x"]);
    // drop ( ..a x -- ..a )
    effect(&mut m, "drop", &["x"], &[]);
    // swap ( ..a x y -- ..a y x )
    effect(&mut m, "swap", &["x", "y"], &["y", "x"]);
    // over ( ..a x y -- ..a x y x )
    effect(&mut m, "over", &["x", "y"], &["x", "y", "x"]);
    // rot ( ..a x y z -- ..a y z x )
    effect(&mut m, "rot", &["x", "y", "z"], &["y", "z", "x"]);
    // nip ( ..a x y -- ..a y )
    effect(&mut m, "nip", &["x", "y"], &["y"]);
    // tuck ( ..a x y -- ..a y x y )
    effect(&mut m, "tuck", &["x", "y"], &["y", "x", "y"]);

    // Integer arithmetic ( ..a Int Int -- ..a Int )
    for name in [
        "i.add",
        "i.+",
        "i.subtract",
        "i.-",
        "i.multiply",
        "i.*",
        "i.divide",
        "i./",
        "modulo",
        "i.%",
    ] {
        typed_effect(&mut m, name, &["Int", "Int"], &["Int"]);
    }
    // negate ( ..a Int -- ..a Int )
    typed_effect(&mut m, "negate", &["Int"], &["Int"]);

    // Generic comparisons ( ..a x x -- ..a Bool )
    for name in [
        "equals",
        "not-equals",
        "less-than",
        "greater-than",
        "less-or-equal",
        "greater-or-equal",
    ] {
        effect_with_output_types(&mut m, name, &["x", "x"], &[("Bool", true)]);
    }

    // Integer comparisons ( ..a Int Int -- ..a Bool )
    for name in ["i.=", "i.<", "i.>", "i.<=", "i.>=", "i.<>"] {
        typed_effect(&mut m, name, &["Int", "Int"], &["Bool"]);
    }

    // Float arithmetic ( ..a Float Float -- ..a Float )
    for name in [
        "f.add",
        "f.+",
        "f.subtract",
        "f.-",
        "f.multiply",
        "f.*",
        "f.divide",
        "f./",
    ] {
        typed_effect(&mut m, name, &["Float", "Float"], &["Float"]);
    }

    // Float comparisons ( ..a Float Float -- ..a Bool )
    for name in ["f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>"] {
        typed_effect(&mut m, name, &["Float", "Float"], &["Bool"]);
    }

    // Logic operations ( ..a Int Int -- ..a Int )
    // Note: and/or/not use Int in the original, not Bool
    for name in ["and", "or"] {
        typed_effect(&mut m, name, &["Int", "Int"], &["Int"]);
    }
    typed_effect(&mut m, "not", &["Int"], &["Int"]);

    // Quotation combinators
    // apply ( ..a Quot -- ..b )
    m.insert(
        "apply",
        StackEffect::new(
            "apply",
            Stack::with_rest("a").push(StackValue::ty("Quot")),
            Stack::with_rest("b"),
        ),
    );
    // dip ( ..a x Quot -- ..b x )
    m.insert(
        "dip",
        StackEffect::new(
            "dip",
            Stack::with_rest("a")
                .push(StackValue::var("x"))
                .push(StackValue::ty("Quot")),
            Stack::with_rest("b").push(StackValue::var("x")),
        ),
    );

    m
}

/// Register a stack effect with type variables (x, y, z)
fn effect(
    m: &mut HashMap<&'static str, StackEffect>,
    name: &'static str,
    inputs: &[&str],
    outputs: &[&str],
) {
    let mut input_stack = Stack::with_rest("a");
    for &v in inputs {
        input_stack = input_stack.push(StackValue::var(v));
    }

    let mut output_stack = Stack::with_rest("a");
    for &v in outputs {
        output_stack = output_stack.push(StackValue::var(v));
    }

    m.insert(name, StackEffect::new(name, input_stack, output_stack));
}

/// Register a stack effect with concrete types (Int, Float, Bool, etc.)
fn typed_effect(
    m: &mut HashMap<&'static str, StackEffect>,
    name: &'static str,
    inputs: &[&str],
    outputs: &[&str],
) {
    let mut input_stack = Stack::with_rest("a");
    for &t in inputs {
        input_stack = input_stack.push(StackValue::ty(t));
    }

    let mut output_stack = Stack::with_rest("a");
    for &t in outputs {
        output_stack = output_stack.push(StackValue::ty(t));
    }

    m.insert(name, StackEffect::new(name, input_stack, output_stack));
}

/// Register a stack effect with variable inputs and typed outputs
fn effect_with_output_types(
    m: &mut HashMap<&'static str, StackEffect>,
    name: &'static str,
    inputs: &[&str],
    outputs: &[(&str, bool)], // (name, is_type)
) {
    let mut input_stack = Stack::with_rest("a");
    for &v in inputs {
        input_stack = input_stack.push(StackValue::var(v));
    }

    let mut output_stack = Stack::with_rest("a");
    for &(v, is_type) in outputs {
        if is_type {
            output_stack = output_stack.push(StackValue::ty(v));
        } else {
            output_stack = output_stack.push(StackValue::var(v));
        }
    }

    m.insert(name, StackEffect::new(name, input_stack, output_stack));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_effects_lookup() {
        assert!(get_effect("dup").is_some());
        assert!(get_effect("drop").is_some());
        assert!(get_effect("swap").is_some());
        assert!(get_effect("i.+").is_some());
        assert!(get_effect("f.add").is_some());
        assert!(get_effect("nonexistent").is_none());
    }

    #[test]
    fn test_dup_effect() {
        let dup = get_effect("dup").unwrap();
        assert_eq!(dup.name, "dup");
        assert_eq!(dup.render_signature(), "dup ( ..a x -- ..a x x )");
    }

    #[test]
    fn test_swap_effect() {
        let swap = get_effect("swap").unwrap();
        assert_eq!(swap.render_signature(), "swap ( ..a x y -- ..a y x )");
    }

    #[test]
    fn test_int_add_effect() {
        let add = get_effect("i.+").unwrap();
        assert_eq!(add.render_signature(), "i.+ ( ..a Int Int -- ..a Int )");
    }
}
