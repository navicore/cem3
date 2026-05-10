//! Float arithmetic and comparison.

use std::collections::HashMap;

use crate::types::{Effect, StackType, Type};

use super::macros::*;

pub(super) fn add_signatures(sigs: &mut HashMap<String, Effect>) {
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
    // Float Math
    // =========================================================================

    // Roots / powers
    builtin!(sigs, "f.sqrt", (a Float -- a Float));
    builtin!(sigs, "f.cbrt", (a Float -- a Float));
    builtin!(sigs, "f.pow",  (a Float Float -- a Float));

    // Exponential / logarithmic
    builtin!(sigs, "f.exp",   (a Float -- a Float));
    builtin!(sigs, "f.ln",    (a Float -- a Float));
    builtin!(sigs, "f.log10", (a Float -- a Float));
    builtin!(sigs, "f.log2",  (a Float -- a Float));

    // Trigonometric
    builtin!(sigs, "f.sin",   (a Float -- a Float));
    builtin!(sigs, "f.cos",   (a Float -- a Float));
    builtin!(sigs, "f.tan",   (a Float -- a Float));
    builtin!(sigs, "f.asin",  (a Float -- a Float));
    builtin!(sigs, "f.acos",  (a Float -- a Float));
    builtin!(sigs, "f.atan",  (a Float -- a Float));
    builtin!(sigs, "f.atan2", (a Float Float -- a Float));

    // Rounding
    builtin!(sigs, "f.floor", (a Float -- a Float));
    builtin!(sigs, "f.ceil",  (a Float -- a Float));
    builtin!(sigs, "f.round", (a Float -- a Float));
    builtin!(sigs, "f.trunc", (a Float -- a Float));

    // Constants
    builtin!(sigs, "f.pi",  (a -- a Float));
    builtin!(sigs, "f.e",   (a -- a Float));
    builtin!(sigs, "f.tau", (a -- a Float));
}

pub(super) fn add_docs(docs: &mut HashMap<&'static str, &'static str>) {
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

    // Float Math — roots / powers
    docs.insert(
        "f.sqrt",
        "Square root. Negative inputs return NaN per IEEE 754.",
    );
    docs.insert(
        "f.cbrt",
        "Cube root. Defined for all reals (negative inputs return a negative root).",
    );
    docs.insert(
        "f.pow",
        "Power: ( base exp -- result ). NaN/Infinity propagate per IEEE 754.",
    );

    // Float Math — exponential / logarithmic
    docs.insert(
        "f.exp",
        "e^x. Overflows to +Infinity, 0.0 for large negative x.",
    );
    docs.insert(
        "f.ln",
        "Natural log. ln(0) = -Infinity, ln(<0) = NaN per IEEE 754.",
    );
    docs.insert("f.log10", "Base-10 log. Same NaN/Infinity rules as f.ln.");
    docs.insert("f.log2", "Base-2 log. Same NaN/Infinity rules as f.ln.");

    // Float Math — trigonometric (radians)
    docs.insert("f.sin", "Sine of an angle in radians.");
    docs.insert("f.cos", "Cosine of an angle in radians.");
    docs.insert("f.tan", "Tangent of an angle in radians.");
    docs.insert(
        "f.asin",
        "Arcsine in radians, range [-π/2, π/2]. NaN for inputs outside [-1, 1].",
    );
    docs.insert(
        "f.acos",
        "Arccosine in radians, range [0, π]. NaN for inputs outside [-1, 1].",
    );
    docs.insert("f.atan", "Arctangent in radians, range (-π/2, π/2).");
    docs.insert(
        "f.atan2",
        "Two-argument arctangent: ( y x -- result ). \
         Returns the angle of (x, y) from the positive x-axis. \
         Argument order matches C/Rust/JS.",
    );

    // Float Math — rounding
    docs.insert("f.floor", "Round toward -Infinity.");
    docs.insert("f.ceil", "Round toward +Infinity.");
    docs.insert(
        "f.round",
        "Round to nearest integer, ties to even (banker's rounding, IEEE 754 default). \
         e.g. 0.5 -> 0.0, 1.5 -> 2.0, 2.5 -> 2.0.",
    );
    docs.insert("f.trunc", "Round toward zero (drop the fractional part).");

    // Float Math — constants
    docs.insert("f.pi", "Push π (std::f64::consts::PI).");
    docs.insert("f.e", "Push e, Euler's number (std::f64::consts::E).");
    docs.insert("f.tau", "Push τ = 2π (std::f64::consts::TAU).");
}
