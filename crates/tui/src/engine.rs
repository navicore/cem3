//! Compiler Bridge for TUI
//!
//! Provides IR extraction by wrapping seq-compiler functionality.
//! Converts compiler types to TUI-friendly representations.

use crate::ir::stack_art::{Stack, StackEffect, StackValue};
use seqc::{CodeGen, CompilerConfig, Effect, Parser, StackType, Type, TypeChecker};

/// Result of analyzing Seq source code
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Stack effects for all word definitions
    pub word_effects: Vec<WordEffect>,
    /// Any errors encountered during analysis
    pub errors: Vec<String>,
    /// LLVM IR if compilation succeeded
    pub llvm_ir: Option<String>,
}

/// A word and its stack effect
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

/// Get the builtin word effects for display
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
            name: "add".to_string(),
            effect: StackEffect::new(
                "add",
                Stack::with_rest("a")
                    .push(StackValue::ty("Int"))
                    .push(StackValue::ty("Int")),
                Stack::with_rest("a").push(StackValue::ty("Int")),
            ),
        },
        WordEffect {
            name: "multiply".to_string(),
            effect: StackEffect::new(
                "multiply",
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
    fn test_analyze_word_with_effect() {
        let source = r#"
: double ( Int -- Int )
    dup add
;

: main ( -- )
    5 double drop
;
"#;
        let result = analyze(source);
        assert!(result.errors.is_empty(), "Errors: {:?}", result.errors);

        // Find the double word
        let double = result.word_effects.iter().find(|w| w.name == "double");
        assert!(double.is_some());

        let effect = &double.unwrap().effect;
        assert_eq!(effect.name, "double");
    }

    #[test]
    fn test_analyze_type_error() {
        let source = r#"
: main ( -- )
    "hello" 42 add
;
"#;
        let result = analyze(source);
        assert!(!result.errors.is_empty());
        assert!(result.errors[0].contains("error") || result.errors[0].contains("mismatch"));
    }

    #[test]
    fn test_builtin_effects() {
        let effects = builtin_effects();
        assert!(!effects.is_empty());

        // Check that dup has correct signature
        let dup = effects.iter().find(|w| w.name == "dup").unwrap();
        let sig = dup.effect.render_signature();
        assert!(sig.contains("dup"));
        assert!(sig.contains("..a"));
    }

    #[test]
    fn test_effect_conversion() {
        let compiler_effect = Effect {
            inputs: StackType::RowVar("a".to_string()).push(Type::Int),
            outputs: StackType::RowVar("a".to_string())
                .push(Type::Int)
                .push(Type::Int),
        };

        let effect = effect_to_stack_effect("dup-int", &compiler_effect);
        assert_eq!(effect.name, "dup-int");

        let sig = effect.render_signature();
        assert!(sig.contains("Int"));
        assert!(sig.contains("..a"));
    }
}
