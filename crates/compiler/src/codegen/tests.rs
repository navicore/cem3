use super::*;
use crate::ast::{Program, Statement, WordDef};
use crate::config::CompilerConfig;
use std::collections::HashMap;

#[test]
fn test_codegen_hello_world() {
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::StringLiteral(b"Hello, World!".to_vec()),
                Statement::WordCall {
                    name: "io.write-line".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    assert!(ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
    // main uses C calling convention (no tailcc) since it's called from C runtime
    assert!(ir.contains("define ptr @seq_main(ptr %stack)"));
    assert!(ir.contains("call ptr @patch_seq_push_string"));
    assert!(ir.contains("call ptr @patch_seq_write_line"));
    assert!(ir.contains("\"Hello, World!\\00\""));
}

#[test]
fn test_codegen_io_write() {
    // Test io.write (write without newline)
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::StringLiteral(b"no newline".to_vec()),
                Statement::WordCall {
                    name: "io.write".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    assert!(ir.contains("call ptr @patch_seq_push_string"));
    assert!(ir.contains("call ptr @patch_seq_write"));
    assert!(ir.contains("\"no newline\\00\""));
}

#[test]
fn test_codegen_arithmetic() {
    // Test inline tagged stack arithmetic with virtual registers (Issue #189)
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::IntLiteral(2),
                Statement::IntLiteral(3),
                Statement::WordCall {
                    name: "i.add".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Issue #189: With virtual registers, integers are kept in SSA variables
    // Using identity add: %n = add i64 0, <value>
    assert!(ir.contains("add i64 0, 2"), "Should create SSA var for 2");
    assert!(ir.contains("add i64 0, 3"), "Should create SSA var for 3");
    // The add operation uses virtual registers directly
    assert!(ir.contains("add i64 %"), "Should add SSA variables");
}

#[test]
fn test_pure_inline_test_mode() {
    let mut codegen = CodeGen::new_pure_inline_test();

    // Simple program: 5 3 add (should return 8)
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::IntLiteral(5),
                Statement::IntLiteral(3),
                Statement::WordCall {
                    name: "i.add".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Pure inline test mode should:
    // 1. NOT CALL the scheduler (declarations are ok, calls are not)
    assert!(!ir.contains("call void @patch_seq_scheduler_init"));
    assert!(!ir.contains("call i64 @patch_seq_strand_spawn"));

    // 2. Have main allocate tagged stack and call seq_main directly
    assert!(ir.contains("call ptr @seq_stack_new_default()"));
    assert!(ir.contains("call ptr @seq_main(ptr %stack_base)"));

    // 3. Read result from stack and return as exit code
    // SSA name is a dynamic temp (not hardcoded %result), so check line-level
    assert!(
        ir.lines()
            .any(|l| l.contains("trunc i64 %") && l.contains("to i32")),
        "Expected a trunc i64 %N to i32 instruction"
    );
    assert!(ir.contains("ret i32 %exit_code"));

    // 4. Use inline push with virtual registers (Issue #189)
    assert!(!ir.contains("call ptr @patch_seq_push_int"));
    // Values are kept in SSA variables via identity add
    assert!(ir.contains("add i64 0, 5"), "Should create SSA var for 5");
    assert!(ir.contains("add i64 0, 3"), "Should create SSA var for 3");

    // 5. Use inline add with virtual registers (add i64 %, not call patch_seq_add)
    assert!(!ir.contains("call ptr @patch_seq_add"));
    assert!(ir.contains("add i64 %"), "Should add SSA variables");
}

#[test]
fn test_escape_llvm_string() {
    assert_eq!(CodeGen::escape_llvm_string(b"hello").unwrap(), "hello");
    assert_eq!(CodeGen::escape_llvm_string(b"a\nb").unwrap(), r"a\0Ab");
    assert_eq!(CodeGen::escape_llvm_string(b"a\tb").unwrap(), r"a\09b");
    assert_eq!(CodeGen::escape_llvm_string(b"a\"b").unwrap(), r"a\22b");
    // Backslash is emitted as the hex escape `\5C`, matching LLVM IR's
    // documented escape syntax (only `\NN` is spec'd inside string
    // constants).
    assert_eq!(CodeGen::escape_llvm_string(b"a\\b").unwrap(), r"a\5Cb");
    // Byte-clean: high-bit bytes round-trip as `\NN` hex escapes,
    // not as multi-byte UTF-8 sequences.
    assert_eq!(CodeGen::escape_llvm_string(&[0xDC]).unwrap(), r"\DC");
    assert_eq!(
        CodeGen::escape_llvm_string(&[0x00, 0xFF]).unwrap(),
        r"\00\FF"
    );
}

#[test]
#[allow(deprecated)] // Testing codegen in isolation, not full pipeline
fn test_external_builtins_declared() {
    use crate::config::{CompilerConfig, ExternalBuiltin};

    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None, // Codegen doesn't check effects
            body: vec![
                Statement::IntLiteral(42),
                Statement::WordCall {
                    name: "my-external-op".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let config = CompilerConfig::new()
        .with_builtin(ExternalBuiltin::new("my-external-op", "test_runtime_my_op"));

    let ir = codegen
        .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
        .unwrap();

    // Should declare the external builtin
    assert!(
        ir.contains("declare ptr @test_runtime_my_op(ptr)"),
        "IR should declare external builtin"
    );

    // Should call the external builtin
    assert!(
        ir.contains("call ptr @test_runtime_my_op"),
        "IR should call external builtin"
    );
}

#[test]
#[allow(deprecated)] // Testing codegen in isolation, not full pipeline
fn test_multiple_external_builtins() {
    use crate::config::{CompilerConfig, ExternalBuiltin};

    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None, // Codegen doesn't check effects
            body: vec![
                Statement::WordCall {
                    name: "actor-self".to_string(),
                    span: None,
                },
                Statement::WordCall {
                    name: "journal-append".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let config = CompilerConfig::new()
        .with_builtin(ExternalBuiltin::new("actor-self", "seq_actors_self"))
        .with_builtin(ExternalBuiltin::new(
            "journal-append",
            "seq_actors_journal_append",
        ));

    let ir = codegen
        .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
        .unwrap();

    // Should declare both external builtins
    assert!(ir.contains("declare ptr @seq_actors_self(ptr)"));
    assert!(ir.contains("declare ptr @seq_actors_journal_append(ptr)"));

    // Should call both
    assert!(ir.contains("call ptr @seq_actors_self"));
    assert!(ir.contains("call ptr @seq_actors_journal_append"));
}

#[test]
#[allow(deprecated)] // Testing config builder, not full pipeline
fn test_external_builtins_with_library_paths() {
    use crate::config::{CompilerConfig, ExternalBuiltin};

    let config = CompilerConfig::new()
        .with_builtin(ExternalBuiltin::new("my-op", "runtime_my_op"))
        .with_library_path("/custom/lib")
        .with_library("myruntime");

    assert_eq!(config.external_builtins.len(), 1);
    assert_eq!(config.library_paths, vec!["/custom/lib"]);
    assert_eq!(config.libraries, vec!["myruntime"]);
}

#[test]
fn test_external_builtin_full_pipeline() {
    // Test that external builtins work through the full compile pipeline
    // including parser, AST validation, type checker, and codegen
    use crate::compile_to_ir_with_config;
    use crate::config::{CompilerConfig, ExternalBuiltin};
    use crate::types::{Effect, StackType, Type};

    let source = r#"
        : main ( -- Int )
          42 my-transform
          0
        ;
    "#;

    // External builtins must have explicit effects (v2.0 requirement)
    let effect = Effect::new(StackType::singleton(Type::Int), StackType::Empty);
    let config = CompilerConfig::new().with_builtin(ExternalBuiltin::with_effect(
        "my-transform",
        "ext_runtime_transform",
        effect,
    ));

    // This should succeed - the external builtin is registered
    let result = compile_to_ir_with_config(source, &config);
    assert!(
        result.is_ok(),
        "Compilation should succeed: {:?}",
        result.err()
    );

    let ir = result.unwrap();
    assert!(ir.contains("declare ptr @ext_runtime_transform(ptr)"));
    assert!(ir.contains("call ptr @ext_runtime_transform"));
}

#[test]
fn test_external_builtin_without_config_fails() {
    // Test that using an external builtin without config fails validation
    use crate::compile_to_ir;

    let source = r#"
        : main ( -- Int )
          42 unknown-builtin
          0
        ;
    "#;

    // This should fail - unknown-builtin is not registered
    let result = compile_to_ir(source);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unknown-builtin"));
}

#[test]
fn test_match_exhaustiveness_error() {
    use crate::compile_to_ir;

    let source = r#"
        union Result { Ok { value: Int } Err { msg: String } }

        : handle ( Variant -- Int )
          match
            Ok -> drop 1
            # Missing Err arm!
          end
        ;

        : main ( -- ) 42 Make-Ok handle drop ;
    "#;

    let result = compile_to_ir(source);
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Non-exhaustive match"));
    assert!(err.contains("Result"));
    assert!(err.contains("Err"));
}

#[test]
fn test_match_exhaustive_compiles() {
    use crate::compile_to_ir;

    let source = r#"
        union Result { Ok { value: Int } Err { msg: String } }

        : handle ( Variant -- Int )
          match
            Ok -> drop 1
            Err -> drop 0
          end
        ;

        : main ( -- ) 42 Make-Ok handle drop ;
    "#;

    let result = compile_to_ir(source);
    assert!(
        result.is_ok(),
        "Exhaustive match should compile: {:?}",
        result
    );
}

#[test]
fn test_codegen_symbol() {
    // Test symbol literal codegen
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::Symbol("hello".to_string()),
                Statement::WordCall {
                    name: "symbol->string".to_string(),
                    span: None,
                },
                Statement::WordCall {
                    name: "io.write-line".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    assert!(ir.contains("call ptr @patch_seq_push_interned_symbol"));
    assert!(ir.contains("call ptr @patch_seq_symbol_to_string"));
    assert!(ir.contains("\"hello\\00\""));
}

#[test]
fn test_symbol_interning_dedup() {
    // Issue #166: Test that duplicate symbol literals share the same global
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                // Use :hello twice - should share the same .sym global
                Statement::Symbol("hello".to_string()),
                Statement::Symbol("hello".to_string()),
                Statement::Symbol("world".to_string()), // Different symbol
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Should have exactly one .sym global for "hello" and one for "world"
    // Count occurrences of symbol global definitions (lines starting with @.sym)
    let sym_defs: Vec<_> = ir
        .lines()
        .filter(|l| l.trim().starts_with("@.sym."))
        .collect();

    // There should be 2 definitions: .sym.0 for "hello" and .sym.1 for "world"
    assert_eq!(
        sym_defs.len(),
        2,
        "Expected 2 symbol globals, got: {:?}",
        sym_defs
    );

    // Verify deduplication: :hello appears twice but .sym.0 is reused
    let hello_uses: usize = ir.matches("@.sym.0").count();
    assert_eq!(
        hello_uses, 3,
        "Expected 3 occurrences of .sym.0 (1 def + 2 uses)"
    );

    // The IR should contain static symbol structure with capacity=0
    assert!(
        ir.contains("i64 0, i8 1"),
        "Symbol global should have capacity=0 and global=1"
    );
}

#[test]
fn test_dup_optimization_for_int() {
    // Test that dup on Int uses optimized load/store instead of clone_value
    // This verifies the Issue #186 optimization actually fires
    let mut codegen = CodeGen::new();

    use crate::types::Type;

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "test_dup".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(42), // stmt 0: push Int
                    Statement::WordCall {
                        // stmt 1: dup
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "test_dup".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    // Provide type info: before statement 1 (dup), top of stack is Int
    let mut statement_types = HashMap::new();
    statement_types.insert(("test_dup".to_string(), 1), Type::Int);

    let ir = codegen
        .codegen_program(&program, HashMap::new(), statement_types)
        .unwrap();

    // Extract just the test_dup function
    let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
    let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
    let test_dup_fn = &ir[func_start..func_end];

    // The optimized path should use load/store directly (no clone_value call)
    assert!(
        test_dup_fn.contains("load i64"),
        "Optimized dup should use 'load i64', got:\n{}",
        test_dup_fn
    );
    assert!(
        test_dup_fn.contains("store i64"),
        "Optimized dup should use 'store i64', got:\n{}",
        test_dup_fn
    );

    // The optimized path should NOT call clone_value
    assert!(
        !test_dup_fn.contains("@patch_seq_clone_value"),
        "Optimized dup should NOT call clone_value for Int, got:\n{}",
        test_dup_fn
    );
}

#[test]
fn test_dup_optimization_after_literal() {
    // Test Issue #195: dup after literal push uses optimized path
    // Pattern: `42 dup` should be optimized even without type map info
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "test_dup".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(42), // Previous statement is Int literal
                    Statement::WordCall {
                        // dup should be optimized
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "test_dup".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    // No type info provided - but literal heuristic should optimize
    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Extract just the test_dup function
    let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
    let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
    let test_dup_fn = &ir[func_start..func_end];

    // With literal heuristic, should use optimized path
    assert!(
        test_dup_fn.contains("load i64"),
        "Dup after int literal should use optimized load, got:\n{}",
        test_dup_fn
    );
    assert!(
        test_dup_fn.contains("store i64"),
        "Dup after int literal should use optimized store, got:\n{}",
        test_dup_fn
    );
    assert!(
        !test_dup_fn.contains("@patch_seq_clone_value"),
        "Dup after int literal should NOT call clone_value, got:\n{}",
        test_dup_fn
    );
}

#[test]
fn test_dup_no_optimization_after_word_call() {
    // Test that dup after word call (unknown type) uses safe clone_value path
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "get_value".to_string(),
                effect: None,
                body: vec![Statement::IntLiteral(42)],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "test_dup".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        // Previous statement is word call (unknown type)
                        name: "get_value".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        // dup should NOT be optimized
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "test_dup".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    // No type info provided and no literal before dup
    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Extract just the test_dup function
    let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
    let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
    let test_dup_fn = &ir[func_start..func_end];

    // Without literal or type info, should call clone_value (safe path)
    assert!(
        test_dup_fn.contains("@patch_seq_clone_value"),
        "Dup after word call should call clone_value, got:\n{}",
        test_dup_fn
    );
}

#[test]
fn test_roll_constant_optimization() {
    // Test Issue #192: roll with constant N uses optimized inline code
    // Pattern: `2 roll` should generate rot-like inline code
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "test_roll".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(1),
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::IntLiteral(2), // Constant N for roll
                    Statement::WordCall {
                        // 2 roll = rot
                        name: "roll".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "test_roll".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Extract just the test_roll function
    let func_start = ir.find("define tailcc ptr @seq_test_roll").unwrap();
    let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
    let test_roll_fn = &ir[func_start..func_end];

    // With constant N=2, should NOT do dynamic calculation
    // Should NOT have dynamic add/sub for offset calculation
    assert!(
        !test_roll_fn.contains("= add i64 %"),
        "Constant roll should use constant offset, not dynamic add, got:\n{}",
        test_roll_fn
    );

    // Should NOT call memmove for small N (n=2 uses direct loads/stores)
    assert!(
        !test_roll_fn.contains("@llvm.memmove"),
        "2 roll should not use memmove, got:\n{}",
        test_roll_fn
    );
}

#[test]
fn test_pick_constant_optimization() {
    // Test Issue #192: pick with constant N uses constant offset
    // Pattern: `1 pick` should generate code with constant -3 offset
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "test_pick".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(10),
                    Statement::IntLiteral(20),
                    Statement::IntLiteral(1), // Constant N for pick
                    Statement::WordCall {
                        // 1 pick = over
                        name: "pick".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "test_pick".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Extract just the test_pick function
    let func_start = ir.find("define tailcc ptr @seq_test_pick").unwrap();
    let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
    let test_pick_fn = &ir[func_start..func_end];

    // With constant N=1, should use constant offset -3
    // Should NOT have dynamic add/sub for offset calculation
    assert!(
        !test_pick_fn.contains("= add i64 %"),
        "Constant pick should use constant offset, not dynamic add, got:\n{}",
        test_pick_fn
    );

    // Should have the constant offset -3 in getelementptr
    assert!(
        test_pick_fn.contains("i64 -3"),
        "1 pick should use offset -3 (-(1+2)), got:\n{}",
        test_pick_fn
    );
}

#[test]
fn test_small_word_marked_alwaysinline() {
    // Test Issue #187: Small words get alwaysinline attribute
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "double".to_string(), // Small word: dup i.+
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "i.+".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(21),
                    Statement::WordCall {
                        name: "double".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Small word 'double' should have alwaysinline attribute
    assert!(
        ir.contains("define tailcc ptr @seq_double(ptr %stack) alwaysinline"),
        "Small word should have alwaysinline attribute, got:\n{}",
        ir.lines()
            .filter(|l| l.contains("define"))
            .collect::<Vec<_>>()
            .join("\n")
    );

    // main should NOT have alwaysinline (uses C calling convention)
    assert!(
        ir.contains("define ptr @seq_main(ptr %stack) {"),
        "main should not have alwaysinline, got:\n{}",
        ir.lines()
            .filter(|l| l.contains("define"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_recursive_word_not_inlined() {
    // Test Issue #187: Recursive words should NOT get alwaysinline
    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "countdown".to_string(), // Recursive
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.-".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "countdown".to_string(), // Recursive call
                                span: None,
                            },
                        ],
                        else_branch: Some(vec![]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(5),
                    Statement::WordCall {
                        name: "countdown".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Recursive word should NOT have alwaysinline
    assert!(
        ir.contains("define tailcc ptr @seq_countdown(ptr %stack) {"),
        "Recursive word should NOT have alwaysinline, got:\n{}",
        ir.lines()
            .filter(|l| l.contains("define"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_recursive_word_in_match_not_inlined() {
    // Test Issue #187: Recursive calls inside match arms should prevent inlining
    use crate::ast::{MatchArm, Pattern, UnionDef, UnionVariant};

    let mut codegen = CodeGen::new();

    let program = Program {
        includes: vec![],
        unions: vec![UnionDef {
            name: "Option".to_string(),
            variants: vec![
                UnionVariant {
                    name: "Some".to_string(),
                    fields: vec![],
                    source: None,
                },
                UnionVariant {
                    name: "None".to_string(),
                    fields: vec![],
                    source: None,
                },
            ],
            source: None,
        }],
        words: vec![
            WordDef {
                name: "process".to_string(), // Recursive in match arm
                effect: None,
                body: vec![Statement::Match {
                    arms: vec![
                        MatchArm {
                            pattern: Pattern::Variant("Some".to_string()),
                            body: vec![Statement::WordCall {
                                name: "process".to_string(), // Recursive call
                                span: None,
                            }],
                            span: None,
                        },
                        MatchArm {
                            pattern: Pattern::Variant("None".to_string()),
                            body: vec![],
                            span: None,
                        },
                    ],
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "process".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Recursive word (via match arm) should NOT have alwaysinline
    assert!(
        ir.contains("define tailcc ptr @seq_process(ptr %stack) {"),
        "Recursive word in match should NOT have alwaysinline, got:\n{}",
        ir.lines()
            .filter(|l| l.contains("define"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn test_issue_338_specialized_call_in_if_branch_has_terminator() {
    // Issue #338: When a specialized function is called in an if-then branch,
    // the generated IR was missing a terminator instruction because:
    // 1. will_emit_tail_call returned true (expecting musttail + ret)
    // 2. But try_specialized_dispatch took the specialized path instead
    // 3. The specialized path doesn't emit ret, leaving the basic block unterminated
    //
    // The fix skips specialized dispatch in tail position for user-defined words.
    use crate::types::{Effect, StackType, Type};

    let mut codegen = CodeGen::new();

    // Create a specializable word: get-value ( Int -- Int )
    // This will get a specialized version that returns i64 directly
    let get_value_effect = Effect {
        inputs: StackType::Cons {
            rest: Box::new(StackType::RowVar("S".to_string())),
            top: Type::Int,
        },
        outputs: StackType::Cons {
            rest: Box::new(StackType::RowVar("S".to_string())),
            top: Type::Int,
        },
        effects: vec![],
    };

    // Create a word that calls get-value in an if-then branch
    // This pattern triggered the bug in issue #338
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            // : get-value ( Int -- Int ) dup ;
            WordDef {
                name: "get-value".to_string(),
                effect: Some(get_value_effect),
                body: vec![Statement::WordCall {
                    name: "dup".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            // : test-caller ( Bool Int -- Int )
            //   if get-value else drop 0 then ;
            WordDef {
                name: "test-caller".to_string(),
                effect: None,
                body: vec![Statement::If {
                    then_branch: vec![Statement::WordCall {
                        name: "get-value".to_string(),
                        span: None,
                    }],
                    else_branch: Some(vec![
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::IntLiteral(0),
                    ]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            // : main ( -- ) true 42 test-caller drop ;
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::BoolLiteral(true),
                    Statement::IntLiteral(42),
                    Statement::WordCall {
                        name: "test-caller".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "drop".to_string(),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    // This should NOT panic with "basic block lacks terminator"
    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .expect("Issue #338: codegen should succeed for specialized call in if branch");

    // Verify the specialized version was generated
    assert!(
        ir.contains("@seq_get_value_i64"),
        "Should generate specialized version of get-value"
    );

    // Verify the test-caller function has proper structure
    // (both branches should have terminators leading to merge or return)
    assert!(
        ir.contains("define tailcc ptr @seq_test_caller"),
        "Should generate test-caller function"
    );

    // The then branch should use tail call (musttail + ret) for get-value
    // NOT the specialized dispatch (which would leave the block unterminated)
    assert!(
        ir.contains("musttail call tailcc ptr @seq_get_value"),
        "Then branch should use tail call to stack-based version, not specialized dispatch"
    );
}

#[test]
fn test_report_call_in_normal_mode() {
    let mut codegen = CodeGen::new();
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![
                Statement::IntLiteral(42),
                Statement::WordCall {
                    name: "io.write-line".to_string(),
                    span: None,
                },
            ],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Normal mode should call patch_seq_report after scheduler_run
    assert!(
        ir.contains("call void @patch_seq_report()"),
        "Normal mode should emit report call"
    );
}

#[test]
fn test_report_call_absent_in_pure_inline() {
    let mut codegen = CodeGen::new_pure_inline_test();
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![Statement::IntLiteral(42)],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    // Pure inline test mode should NOT call patch_seq_report
    assert!(
        !ir.contains("call void @patch_seq_report()"),
        "Pure inline mode should not emit report call"
    );
}

#[test]
fn test_instrument_emits_counters_and_atomicrmw() {
    let mut codegen = CodeGen::new();
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "helper".to_string(),
                effect: None,
                body: vec![Statement::IntLiteral(1)],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "helper".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let config = CompilerConfig {
        instrument: true,
        ..CompilerConfig::default()
    };

    let ir = codegen
        .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
        .unwrap();

    // Should emit counter array
    assert!(
        ir.contains("@seq_word_counters = global [2 x i64] zeroinitializer"),
        "Should emit counter array for 2 words"
    );

    // Should emit word name strings
    assert!(
        ir.contains("@seq_word_name_"),
        "Should emit word name constants"
    );

    // Should emit name pointer table
    assert!(
        ir.contains("@seq_word_names = private constant [2 x ptr]"),
        "Should emit name pointer table"
    );

    // Should emit atomicrmw in each word
    assert!(
        ir.contains("atomicrmw add ptr %instr_ptr_"),
        "Should emit atomicrmw add for word counters"
    );

    // Should emit report_init call
    assert!(
        ir.contains(
            "call void @patch_seq_report_init(ptr @seq_word_counters, ptr @seq_word_names, i64 2)"
        ),
        "Should emit report_init call with correct count"
    );
}

#[test]
fn test_no_instrument_no_counters() {
    let mut codegen = CodeGen::new();
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![WordDef {
            name: "main".to_string(),
            effect: None,
            body: vec![Statement::IntLiteral(42)],
            source: None,
            allowed_lints: vec![],
        }],
    };

    let config = CompilerConfig::default();
    assert!(!config.instrument);

    let ir = codegen
        .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
        .unwrap();

    // Should NOT emit counter array
    assert!(
        !ir.contains("@seq_word_counters"),
        "Should not emit counters when instrument=false"
    );

    // Should NOT emit atomicrmw
    assert!(
        !ir.contains("atomicrmw"),
        "Should not emit atomicrmw when instrument=false"
    );

    // Should NOT emit report_init call
    assert!(
        !ir.contains("call void @patch_seq_report_init"),
        "Should not emit report_init when instrument=false"
    );
}

// =========================================================================
// Loop lowering (--loop-opt) tests. See docs/design/LOOP_LOWERING.md.
// =========================================================================

/// Generate IR for `program` with loop-opt enabled (cadence 1024) and a call
/// graph wired in. Mirrors the CLI path: the flag flows through config into
/// `prepare_program_state`.
fn codegen_loop_opt_ir(program: &Program) -> String {
    let config = CompilerConfig::default().with_loop_opt();
    let mut codegen = CodeGen::new();
    codegen.set_call_graph(crate::call_graph::CallGraph::build(program));
    codegen
        .codegen_program_with_config(program, HashMap::new(), HashMap::new(), &config)
        .unwrap()
}

#[test]
fn test_loop_opt_is_off_by_default() {
    // Default config: loop_opt must be false, cadence 1024.
    let config = CompilerConfig::default();
    assert!(!config.loop_opt);
    assert_eq!(config.loop_yield_cadence, 1024);
}

#[test]
fn test_loop_opt_emits_native_loop_for_tail_recursive_word() {
    // Phase 1 pattern: single if with base (then) and self-tail-call (else).
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "countdown".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(0),
                    Statement::WordCall {
                        name: "i.<=".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        }],
                        else_branch: Some(vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.-".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "countdown".to_string(),
                                span: None,
                            },
                        ]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "countdown".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen_loop_opt_ir(&program);

    // The loop header and back-edge blocks must be present.
    assert!(
        ir.contains("loop_base"),
        "loop-opt should emit a base block; got:\n{}",
        ir
    );
    assert!(
        ir.contains("loop_cont"),
        "loop-opt should emit a continue block; got:\n{}",
        ir
    );
    assert!(
        ir.contains("loop_yield"),
        "loop-opt should emit a yield block; got:\n{}",
        ir
    );
    // The self-call must be gone — replaced by the back-edge.
    let countdown_fn = ir
        .find("define tailcc ptr @seq_countdown")
        .expect("countdown function should exist");
    let countdown_body = &ir[countdown_fn..];
    assert!(
        !countdown_body.contains("musttail"),
        "loop-lowered word must not musttail-call itself"
    );
    // Yield happens on a cadence, not every iteration: maybe_yield appears in
    // its own block, gated by the AND-mask compare.
    assert!(countdown_body.contains("and i64"));
}

#[test]
fn test_loop_opt_disabled_falls_back_to_musttail() {
    // Same program, but loop-opt OFF → musttail self-call, no loop blocks.
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "countdown".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(0),
                    Statement::WordCall {
                        name: "i.<=".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        }],
                        else_branch: Some(vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.-".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "countdown".to_string(),
                                span: None,
                            },
                        ]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "countdown".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let mut codegen = CodeGen::new(); // loop-opt off
    codegen.set_call_graph(crate::call_graph::CallGraph::build(&program));
    let ir = codegen
        .codegen_program(&program, HashMap::new(), HashMap::new())
        .unwrap();

    assert!(
        ir.contains("musttail call tailcc ptr @seq_countdown"),
        "without loop-opt, countdown must musttail-call itself"
    );
}

#[test]
fn test_loop_opt_skips_mutual_recursion() {
    // Mutual recursion (ping/pong) must NOT be lowered — it's not self-tail.
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "ping".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "pong".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "pong".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "ping".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "ping".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen_loop_opt_ir(&program);

    // Neither ping nor pong should be loop-lowered.
    assert!(
        !ir.contains("loop_base"),
        "mutual recursion must not be loop-lowered"
    );
    assert!(ir.contains("musttail"), "mutual recursion keeps musttail");
}

#[test]
fn test_loop_opt_skips_both_branches_recurse() {
    // If BOTH branches end in a self-call (Phase 3 fibonacci pattern), Phase 1
    // must NOT lower it.
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "fib".to_string(),
                effect: None,
                body: vec![Statement::If {
                    then_branch: vec![Statement::WordCall {
                        name: "fib".to_string(),
                        span: None,
                    }],
                    else_branch: Some(vec![Statement::WordCall {
                        name: "fib".to_string(),
                        span: None,
                    }]),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "fib".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let ir = codegen_loop_opt_ir(&program);

    assert!(
        !ir.contains("loop_base"),
        "both-branches-recurse is Phase 3, not Phase 1"
    );
}

#[test]
fn test_loop_opt_yield_cadence_mask() {
    // A custom cadence (256) produces an AND mask of 255.
    let program = Program {
        includes: vec![],
        unions: vec![],
        words: vec![
            WordDef {
                name: "countdown".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "dup".to_string(),
                        span: None,
                    },
                    Statement::IntLiteral(0),
                    Statement::WordCall {
                        name: "i.<=".to_string(),
                        span: None,
                    },
                    Statement::If {
                        then_branch: vec![Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        }],
                        else_branch: Some(vec![
                            Statement::IntLiteral(1),
                            Statement::WordCall {
                                name: "i.-".to_string(),
                                span: None,
                            },
                            Statement::WordCall {
                                name: "countdown".to_string(),
                                span: None,
                            },
                        ]),
                        span: None,
                    },
                ],
                source: None,
                allowed_lints: vec![],
            },
            WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![Statement::WordCall {
                    name: "countdown".to_string(),
                    span: None,
                }],
                source: None,
                allowed_lints: vec![],
            },
        ],
    };

    let config = CompilerConfig::default()
        .with_loop_opt()
        .with_loop_yield_cadence(256);
    let mut codegen = CodeGen::new();
    codegen.set_call_graph(crate::call_graph::CallGraph::build(&program));
    let ir = codegen
        .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
        .unwrap();

    assert!(
        ir.contains("and i64 %loop_iter_next_"),
        "cadence 256 should produce an AND mask on the iter counter; got:\n{}",
        ir
    );
    assert!(
        ir.contains(", 255"),
        "cadence 256 should produce AND mask 255; got:\n{}",
        ir
    );
}
