//! Compiler Bridge for TUI
//!
//! Provides IR extraction by wrapping seq-compiler functionality.
//! Converts compiler types to TUI-friendly representations.

use crate::ir::stack_art::{Stack, StackEffect, StackValue};
use seqc::{CodeGen, CompilerConfig, Effect, Parser, StackType, Type, TypeChecker};

/// Result of analyzing Seq source code
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Stack effects for all word definitions (for future IR display)
    #[allow(dead_code)]
    pub word_effects: Vec<WordEffect>,
    /// Any errors encountered during analysis
    pub errors: Vec<String>,
    /// LLVM IR if compilation succeeded
    pub llvm_ir: Option<String>,
}

/// A word and its stack effect (for future IR display)
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct WordEffect {
    /// Name of the word
    pub name: String,
    /// Stack effect signature
    pub effect: StackEffect,
}

/// Convert a compiler StackType to our Stack representation
fn stack_type_to_stack(st: &StackType) -> Stack {
    let mut values = Vec::new();
    collect_stack_values(st, &mut values);
    Stack::new(values)
}

/// Recursively collect values from a StackType
fn collect_stack_values(st: &StackType, values: &mut Vec<StackValue>) {
    match st {
        StackType::Empty => {}
        StackType::Cons { rest, top } => {
            // Collect rest first (bottom of stack)
            collect_stack_values(rest, values);
            // Then add top
            values.push(type_to_stack_value(top));
        }
        StackType::RowVar(name) => {
            // Strip the freshening suffix for display (e.g., "a$5" -> "a")
            let clean_name = name.split('$').next().unwrap_or(name);
            values.push(StackValue::rest(clean_name.to_string()));
        }
    }
}

/// Convert a compiler Type to a StackValue
fn type_to_stack_value(ty: &Type) -> StackValue {
    match ty {
        Type::Int => StackValue::ty("Int"),
        Type::Float => StackValue::ty("Float"),
        Type::Bool => StackValue::ty("Bool"),
        Type::String => StackValue::ty("String"),
        Type::Var(name) => {
            // Strip the freshening suffix
            let clean_name = name.split('$').next().unwrap_or(name);
            StackValue::var(clean_name.to_string())
        }
        Type::Quotation(effect) => {
            // Format quotation type as its effect
            StackValue::ty(format_effect(effect))
        }
        Type::Closure {
            effect,
            captures: _,
        } => StackValue::ty(format!("Closure{}", format_effect(effect))),
        Type::Union(name) => StackValue::ty(name.clone()),
    }
}

/// Format an Effect as a string for display
fn format_effect(effect: &Effect) -> String {
    let inputs = format_stack_type(&effect.inputs);
    let outputs = format_stack_type(&effect.outputs);
    format!("[ {} -- {} ]", inputs, outputs)
}

/// Format a StackType as a space-separated string
fn format_stack_type(st: &StackType) -> String {
    let mut parts = Vec::new();
    collect_type_strings(st, &mut parts);
    parts.join(" ")
}

/// Collect type strings from a StackType
fn collect_type_strings(st: &StackType, parts: &mut Vec<String>) {
    match st {
        StackType::Empty => {}
        StackType::Cons { rest, top } => {
            collect_type_strings(rest, parts);
            parts.push(format_type(top));
        }
        StackType::RowVar(name) => {
            let clean_name = name.split('$').next().unwrap_or(name);
            parts.push(format!("..{}", clean_name));
        }
    }
}

/// Format a Type for display
fn format_type(ty: &Type) -> String {
    match ty {
        Type::Int => "Int".to_string(),
        Type::Float => "Float".to_string(),
        Type::Bool => "Bool".to_string(),
        Type::String => "String".to_string(),
        Type::Var(name) => {
            let clean_name = name.split('$').next().unwrap_or(name);
            clean_name.to_string()
        }
        Type::Quotation(effect) => format_effect(effect),
        Type::Closure { effect, .. } => format!("Closure{}", format_effect(effect)),
        Type::Union(name) => name.clone(),
    }
}

/// Convert a compiler Effect to our StackEffect
pub fn effect_to_stack_effect(name: &str, effect: &Effect) -> StackEffect {
    StackEffect::new(
        name.to_string(),
        stack_type_to_stack(&effect.inputs),
        stack_type_to_stack(&effect.outputs),
    )
}

/// Analyze Seq source code and extract IR information
pub fn analyze(source: &str) -> AnalysisResult {
    analyze_with_config(source, &CompilerConfig::default())
}

/// Analyze a standalone expression for IR display
/// Wraps the expression in a minimal function to generate clean IR
pub fn analyze_expression(expr: &str) -> Option<Vec<String>> {
    // Skip empty or whitespace-only expressions
    let expr = expr.trim();
    if expr.is_empty() {
        return None;
    }

    // Wrap expression in a standalone function with permissive stack effect
    // ( ..a -- ..b ) allows any stack transformation
    // Need a main word for codegen to work
    let source = format!(
        ": __expr__ ( ..a -- ..b )\n  {}\n;\n: main ( -- ) ;\n",
        expr
    );

    let result = analyze(&source);

    // Extract just the __expr__ function's IR (compiler adds seq_ prefix)
    if let Some(ir) = result.llvm_ir {
        let lines = extract_function_ir(&ir, "@seq___expr__");
        if !lines.is_empty() {
            return Some(lines);
        }
    }

    None
}

/// Extract a specific function's IR from LLVM output
fn extract_function_ir(ir: &str, func_name: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_function = false;

    for line in ir.lines() {
        if line.contains("define") && line.contains(func_name) {
            in_function = true;
        }
        if in_function {
            lines.push(line.to_string());
            if line.trim() == "}" {
                break;
            }
        }
    }

    lines
}

/// Analyze Seq source code with custom configuration
pub fn analyze_with_config(source: &str, config: &CompilerConfig) -> AnalysisResult {
    let mut errors = Vec::new();
    let mut word_effects = Vec::new();
    let mut llvm_ir = None;

    // Parse
    let mut parser = Parser::new(source);
    let mut program = match parser.parse() {
        Ok(prog) => prog,
        Err(e) => {
            errors.push(format!("Parse error: {}", e));
            return AnalysisResult {
                word_effects,
                errors,
                llvm_ir,
            };
        }
    };

    // Generate constructors for unions
    if !program.unions.is_empty()
        && let Err(e) = program.generate_constructors()
    {
        errors.push(format!("Constructor generation error: {}", e));
    }

    // Type check
    let mut typechecker = TypeChecker::new();

    // Register external builtins if configured
    if !config.external_builtins.is_empty() {
        let external_effects: Vec<_> = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.as_str(), b.effect.as_ref()))
            .collect();
        typechecker.register_external_words(&external_effects);
    }

    if let Err(e) = typechecker.check_program(&program) {
        errors.push(format!("Type error: {}", e));
        // Still try to extract what we can from definitions
    }

    // Extract effects from word definitions
    for word in &program.words {
        if let Some(effect) = &word.effect {
            word_effects.push(WordEffect {
                name: word.name.clone(),
                effect: effect_to_stack_effect(&word.name, effect),
            });
        }
    }

    // Try to generate LLVM IR (only if no errors)
    if errors.is_empty() {
        let quotation_types = typechecker.take_quotation_types();
        let mut codegen = CodeGen::new();
        match codegen.codegen_program_with_config(&program, quotation_types, config) {
            Ok(ir) => llvm_ir = Some(ir),
            Err(e) => errors.push(format!("Codegen error: {}", e)),
        }
    }

    AnalysisResult {
        word_effects,
        errors,
        llvm_ir,
    }
}

/// Get the builtin word effects for display (for future IR display)
#[allow(dead_code)]
pub fn builtin_effects() -> Vec<WordEffect> {
    // Common stack manipulation words
    vec![
        WordEffect {
            name: "dup".to_string(),
            effect: StackEffect::new(
                "dup",
                Stack::with_rest("a").push(StackValue::var("x")),
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("x")),
            ),
        },
        WordEffect {
            name: "drop".to_string(),
            effect: StackEffect::new(
                "drop",
                Stack::with_rest("a").push(StackValue::var("x")),
                Stack::with_rest("a"),
            ),
        },
        WordEffect {
            name: "swap".to_string(),
            effect: StackEffect::new(
                "swap",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a")
                    .push(StackValue::var("y"))
                    .push(StackValue::var("x")),
            ),
        },
        WordEffect {
            name: "over".to_string(),
            effect: StackEffect::new(
                "over",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y")),
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y"))
                    .push(StackValue::var("x")),
            ),
        },
        WordEffect {
            name: "rot".to_string(),
            effect: StackEffect::new(
                "rot",
                Stack::with_rest("a")
                    .push(StackValue::var("x"))
                    .push(StackValue::var("y"))
                    .push(StackValue::var("z")),
                Stack::with_rest("a")
                    .push(StackValue::var("y"))
                    .push(StackValue::var("z"))
                    .push(StackValue::var("x")),
            ),
        },
        WordEffect {
            name: "i.add".to_string(),
            effect: StackEffect::new(
                "i.add",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            ),
        },
        WordEffect {
            name: "i.multiply".to_string(),
            effect: StackEffect::new(
                "i.multiply",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            ),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_simple_program() {
        let source = r#"
: main ( -- )
    42 drop
;
"#;
        let result = analyze(source);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);
        assert!(result.llvm_ir.is_some());
    }

    #[test]
    fn test_analyze_word_with_effect() -> Result<(), String> {
        let source = r#"
: double ( Int -- Int )
    dup i.add
;

: main ( -- )
    5 double drop
;
"#;
        let result = analyze(source);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        // Find the double word
        let double = result
            .word_effects
            .iter()
            .find(|w| w.name == "double")
            .ok_or("double word not found")?;

        assert_eq!(double.effect.name, "double");
        Ok(())
    }

    #[test]
    fn test_analyze_expression_simple() {
        // Test that analyze_expression works for simple literals
        let result = analyze_expression("5");
        assert!(result.is_some(), "Should produce IR for '5'");
        let ir = result.unwrap();
        assert!(ir.iter().any(|l| l.contains("seq___expr__")));

        // Test arithmetic expression
        let result = analyze_expression("5 10 i.add");
        assert!(result.is_some(), "Should produce IR for '5 10 i.add'");
    }

    #[test]
    fn test_analyze_type_error() {
        let source = r#"
: main ( -- )
    "hello" 42 i.add
;
"#;
        let result = analyze(source);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("error") || result.errors[0].contains("mismatch"));
    }

    #[test]
    fn test_builtin_effects() -> Result<(), String> {
        let effects = builtin_effects();
        assert!(!effects.is_empty());

        // Check that dup has correct signature
        let dup = effects
            .iter()
            .find(|w| w.name == "dup")
            .ok_or("dup not found in builtins")?;
        let sig = dup.effect.render_signature();
        assert!(sig.contains("dup"));
        assert!(sig.contains("..a"));
        Ok(())
    }

    #[test]
    fn test_effect_conversion() {
        let compiler_effect = Effect {
            inputs: StackType::RowVar("a".to_string()).push(Type::Int),
            outputs: StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
            effects: Vec::new(),
        };

        let effect = effect_to_stack_effect("dup-int", &compiler_effect);
        assert_eq!(effect.name, "dup-int");

        let sig = effect.render_signature();
        assert!(sig.contains("Int"));
        assert!(sig.contains("..a"));
    }
}
