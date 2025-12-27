//! ASCII Art Stack Effect Diagrams
//!
//! Renders stack states and transitions using box-drawing characters.
//! These are pure functions that take stack data and return multi-line strings.
//!
//! # Example Output
//!
//! ```text
//!  swap ( ..a x y -- ..a y x )
//!  ┌───────┐      ┌───────┐
//!  │ Float │      │  Int  │
//!  ├───────┤  →   ├───────┤
//!  │  Int  │      │ Float │
//!  ├───────┤      ├───────┤
//!  │  ..a  │      │  ..a  │
//!  └───────┘      └───────┘
//! ```

use std::fmt;

/// A value on the stack - either a concrete type/value or a rest variable
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackValue {
    /// A concrete type (e.g., "Int", "Float", "Bool")
    Type(String),
    /// A concrete value (e.g., "5", "3.14", "true")
    Value(String),
    /// A type variable (e.g., "x", "y", "a")
    TypeVar(String),
    /// A rest/row variable representing remaining stack (e.g., "..a", "..s")
    Rest(String),
}

impl StackValue {
    /// Create a type value
    pub fn ty(s: impl Into<String>) -> Self {
        Self::Type(s.into())
    }

    /// Create a concrete value
    pub fn val(s: impl Into<String>) -> Self {
        Self::Value(s.into())
    }

    /// Create a type variable
    pub fn var(s: impl Into<String>) -> Self {
        Self::TypeVar(s.into())
    }

    /// Create a rest variable
    pub fn rest(s: impl Into<String>) -> Self {
        Self::Rest(s.into())
    }

    /// Get the display string for this value
    fn display(&self) -> String {
        match self {
            Self::Type(s) | Self::Value(s) | Self::TypeVar(s) => s.clone(),
            Self::Rest(s) => format!("..{}", s),
        }
    }

    /// Check if this is a rest variable
    #[allow(dead_code)]
    fn is_rest(&self) -> bool {
        matches!(self, Self::Rest(_))
    }
}

impl fmt::Display for StackValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display())
    }
}

/// A stack state - a sequence of values with bottom (rest) first
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stack {
    /// Values from bottom to top (rest variable, if any, is first)
    values: Vec<StackValue>,
}

impl Stack {
    /// Create a new stack from values (bottom to top)
    pub fn new(values: Vec<StackValue>) -> Self {
        Self { values }
    }

    /// Create an empty stack
    #[allow(dead_code)]
    pub fn empty() -> Self {
        Self { values: vec![] }
    }

    /// Create a stack with just a rest variable
    pub fn with_rest(name: impl Into<String>) -> Self {
        Self {
            values: vec![StackValue::rest(name)],
        }
    }

    /// Push a value onto the stack (top)
    pub fn push(mut self, value: StackValue) -> Self {
        self.values.push(value);
        self
    }

    /// Get all values (bottom to top)
    #[allow(dead_code)]
    pub fn values(&self) -> &[StackValue] {
        &self.values
    }

    /// Check if the stack has values (excluding rest)
    #[allow(dead_code)]
    pub fn has_concrete_values(&self) -> bool {
        self.values.iter().any(|v| !v.is_rest())
    }

    /// Get the width needed to display this stack
    fn display_width(&self) -> usize {
        self.values
            .iter()
            .map(|v| v.display().len())
            .max()
            .unwrap_or(0)
            .max(3) // Minimum width of 3
    }
}

/// A stack effect signature (inputs → outputs)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackEffect {
    /// Name of the word/operation
    pub name: String,
    /// Input stack (what the operation consumes)
    pub inputs: Stack,
    /// Output stack (what the operation produces)
    pub outputs: Stack,
}

impl StackEffect {
    /// Create a new stack effect
    pub fn new(name: impl Into<String>, inputs: Stack, outputs: Stack) -> Self {
        Self {
            name: name.into(),
            inputs,
            outputs,
        }
    }

    /// Render the effect signature line: `swap ( ..a x y -- ..a y x )`
    pub fn render_signature(&self) -> String {
        let inputs: Vec<_> = self.inputs.values.iter().map(|v| v.display()).collect();
        let outputs: Vec<_> = self.outputs.values.iter().map(|v| v.display()).collect();
        format!(
            "{} ( {} -- {} )",
            self.name,
            inputs.join(" "),
            outputs.join(" ")
        )
    }
}

/// Render a single stack as ASCII art box
///
/// Returns a vector of lines (top to bottom in visual display)
pub fn render_stack(stack: &Stack) -> Vec<String> {
    if stack.values.is_empty() {
        return vec!["(empty)".to_string()];
    }

    let width = stack.display_width();
    let inner_width = width + 2; // Padding on each side

    let top_border = format!("┌{}┐", "─".repeat(inner_width));
    let mid_border = format!("├{}┤", "─".repeat(inner_width));
    let bot_border = format!("└{}┘", "─".repeat(inner_width));

    let mut lines = Vec::new();

    // Render from top to bottom (reverse of storage order)
    let values: Vec<_> = stack.values.iter().rev().collect();

    for (i, value) in values.iter().enumerate() {
        if i == 0 {
            lines.push(top_border.clone());
        } else {
            lines.push(mid_border.clone());
        }

        let display = value.display();
        let padding = inner_width - display.len();
        let left_pad = padding / 2;
        let right_pad = padding - left_pad;
        lines.push(format!(
            "│{}{}{}│",
            " ".repeat(left_pad),
            display,
            " ".repeat(right_pad)
        ));
    }

    lines.push(bot_border);
    lines
}

/// Render a stack transition (before → after) for a single operation
pub fn render_transition(effect: &StackEffect, before: &Stack, after: &Stack) -> Vec<String> {
    let sig = effect.render_signature();
    let before_lines = render_stack(before);
    let after_lines = render_stack(after);

    // Calculate widths
    let before_width = before_lines.iter().map(|l| l.len()).max().unwrap_or(0);
    let arrow_col = "  →   ";

    // Pad to same height
    let max_height = before_lines.len().max(after_lines.len());

    let mut result = vec![format!(" {}", sig)];

    for i in 0..max_height {
        let before_line = before_lines
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let after_line = after_lines
            .get(i)
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();

        // Pad before_line to consistent width
        let padded_before = format!("{:width$}", before_line, width = before_width);

        // Arrow only on the middle-ish row
        let arrow = if i == max_height / 2 {
            arrow_col
        } else {
            "      "
        };

        result.push(format!(" {}{}{}", padded_before, arrow, after_line));
    }

    result
}

/// Render a sequence of operations showing the stack evolving
///
/// Each step shows: word name, input stack → output stack
#[allow(dead_code)]
pub fn render_sequence(steps: &[(StackEffect, Stack, Stack)]) -> Vec<String> {
    if steps.is_empty() {
        return vec![];
    }

    let mut result = Vec::new();

    for (i, (effect, before, after)) in steps.iter().enumerate() {
        if i > 0 {
            result.push(String::new()); // Blank line between steps
        }
        result.extend(render_transition(effect, before, after));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_value_display() {
        assert_eq!(StackValue::ty("Int").display(), "Int");
        assert_eq!(StackValue::val("42").display(), "42");
        assert_eq!(StackValue::var("x").display(), "x");
        assert_eq!(StackValue::rest("a").display(), "..a");
    }

    #[test]
    fn test_empty_stack_render() {
        let stack = Stack::empty();
        let lines = render_stack(&stack);
        assert_eq!(lines, vec!["(empty)"]);
    }

    #[test]
    fn test_single_value_stack() {
        let stack = Stack::new(vec![StackValue::val("42")]);
        let lines = render_stack(&stack);
        assert_eq!(lines.len(), 3); // top, content, bottom
        assert!(lines[0].contains("┌"));
        assert!(lines[1].contains("42"));
        assert!(lines[2].contains("└"));
    }

    #[test]
    fn test_multi_value_stack() {
        let stack = Stack::new(vec![
            StackValue::rest("a"),
            StackValue::ty("Int"),
            StackValue::ty("Float"),
        ]);
        let lines = render_stack(&stack);

        // Should have: top, Float, mid, Int, mid, ..a, bottom = 7 lines
        assert_eq!(lines.len(), 7);
        // Float is on top visually (rendered first)
        assert!(lines[1].contains("Float"));
        // Int in middle
        assert!(lines[3].contains("Int"));
        // Rest at bottom
        assert!(lines[5].contains("..a"));
    }

    #[test]
    fn test_stack_effect_signature() {
        let effect = StackEffect::new(
            "swap",
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::var("x"),
                StackValue::var("y"),
            ]),
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::var("y"),
                StackValue::var("x"),
            ]),
        );

        assert_eq!(effect.render_signature(), "swap ( ..a x y -- ..a y x )");
    }

    #[test]
    fn test_swap_transition() {
        let effect = StackEffect::new(
            "swap",
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::var("x"),
                StackValue::var("y"),
            ]),
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::var("y"),
                StackValue::var("x"),
            ]),
        );

        let before = Stack::new(vec![
            StackValue::rest("a"),
            StackValue::ty("Int"),
            StackValue::ty("Float"),
        ]);

        let after = Stack::new(vec![
            StackValue::rest("a"),
            StackValue::ty("Float"),
            StackValue::ty("Int"),
        ]);

        let lines = render_transition(&effect, &before, &after);

        // First line should be the signature
        assert!(lines[0].contains("swap ( ..a x y -- ..a y x )"));

        // Should contain the arrow
        let arrow_line = lines.iter().find(|l| l.contains("→"));
        assert!(arrow_line.is_some());
    }

    #[test]
    fn test_dup_multiply_example() {
        // dup ( ..a x -- ..a x x )
        let dup_effect = StackEffect::new(
            "dup",
            Stack::new(vec![StackValue::rest("a"), StackValue::var("x")]),
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::var("x"),
                StackValue::var("x"),
            ]),
        );

        let before_dup = Stack::new(vec![StackValue::val("5")]);
        let after_dup = Stack::new(vec![StackValue::val("5"), StackValue::val("5")]);

        // multiply ( ..a Int Int -- ..a Int )
        let mult_effect = StackEffect::new(
            "multiply",
            Stack::new(vec![
                StackValue::rest("a"),
                StackValue::ty("Int"),
                StackValue::ty("Int"),
            ]),
            Stack::new(vec![StackValue::rest("a"), StackValue::ty("Int")]),
        );

        let after_mult = Stack::new(vec![StackValue::val("25")]);

        let sequence = vec![
            (dup_effect, before_dup, after_dup.clone()),
            (mult_effect, after_dup, after_mult),
        ];

        let lines = render_sequence(&sequence);

        // Should contain both word signatures
        let text = lines.join("\n");
        assert!(text.contains("dup"));
        assert!(text.contains("multiply"));
        assert!(text.contains("5"));
        assert!(text.contains("25"));
    }

    #[test]
    fn test_width_calculation() {
        let stack = Stack::new(vec![
            StackValue::ty("VeryLongTypeName"),
            StackValue::ty("Int"),
        ]);

        let lines = render_stack(&stack);
        // All lines should have the same visual width (character count, not bytes)
        let widths: Vec<_> = lines.iter().map(|l| l.chars().count()).collect();
        assert!(widths.windows(2).all(|w| w[0] == w[1]));
    }
}
