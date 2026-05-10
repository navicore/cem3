use super::*;
use crate::ast::{Pattern, Statement};
use crate::types::{StackType, Type};

#[test]
fn test_parse_hello_world() {
    let source = r#"
: main ( -- )
  "Hello, World!" write_line ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].name, "main");
    assert_eq!(program.words[0].body.len(), 2);

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => assert_eq!(s, b"Hello, World!"),
        _ => panic!("Expected StringLiteral"),
    }

    match &program.words[0].body[1] {
        Statement::WordCall { name, .. } => assert_eq!(name, "write_line"),
        _ => panic!("Expected WordCall"),
    }
}

#[test]
fn test_parse_with_numbers() {
    let source = ": add-example ( -- ) 2 3 add ;";

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words[0].body.len(), 3);
    assert_eq!(program.words[0].body[0], Statement::IntLiteral(2));
    assert_eq!(program.words[0].body[1], Statement::IntLiteral(3));
    assert!(matches!(
        &program.words[0].body[2],
        Statement::WordCall { name, .. } if name == "add"
    ));
}

#[test]
fn test_parse_hex_literals() {
    let source = ": test ( -- ) 0xFF 0x10 0X1A ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words[0].body[0], Statement::IntLiteral(255));
    assert_eq!(program.words[0].body[1], Statement::IntLiteral(16));
    assert_eq!(program.words[0].body[2], Statement::IntLiteral(26));
}

#[test]
fn test_parse_binary_literals() {
    let source = ": test ( -- ) 0b1010 0B1111 0b0 ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words[0].body[0], Statement::IntLiteral(10));
    assert_eq!(program.words[0].body[1], Statement::IntLiteral(15));
    assert_eq!(program.words[0].body[2], Statement::IntLiteral(0));
}

#[test]
fn test_parse_invalid_hex_literal() {
    let source = ": test ( -- ) 0xGG ;";
    let mut parser = Parser::new(source);
    let err = parser.parse().unwrap_err();
    assert!(err.contains("Invalid hex literal"));
}

#[test]
fn test_parse_invalid_binary_literal() {
    let source = ": test ( -- ) 0b123 ;";
    let mut parser = Parser::new(source);
    let err = parser.parse().unwrap_err();
    assert!(err.contains("Invalid binary literal"));
}

#[test]
fn test_parse_escaped_quotes() {
    let source = r#": main ( -- ) "Say \"hello\" there" write_line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 2);

    match &program.words[0].body[0] {
        // Escape sequences should be processed: \" becomes actual quote
        Statement::StringLiteral(s) => assert_eq!(s, b"Say \"hello\" there"),
        _ => panic!("Expected StringLiteral with escaped quotes"),
    }
}

/// Regression test for issue #117: escaped quote at end of string
/// Previously failed with "String ends with incomplete escape sequence"
#[test]
fn test_escaped_quote_at_end_of_string() {
    let source = r#": main ( -- ) "hello\"" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => assert_eq!(s, b"hello\""),
        _ => panic!("Expected StringLiteral ending with escaped quote"),
    }
}

/// Test escaped quote at start of string (boundary case)
#[test]
fn test_escaped_quote_at_start_of_string() {
    let source = r#": main ( -- ) "\"hello" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => assert_eq!(s, b"\"hello"),
        _ => panic!("Expected StringLiteral starting with escaped quote"),
    }
}

#[test]
fn test_escape_sequences() {
    let source = r#": main ( -- ) "Line 1\nLine 2\tTabbed" write_line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => assert_eq!(s, b"Line 1\nLine 2\tTabbed"),
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_unknown_escape_sequence() {
    let source = r#": main ( -- ) "Bad \q sequence" write_line ;"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown escape sequence"));
}

#[test]
fn test_hex_escape_sequence() {
    // \x1b is ESC (27), \x41 is 'A' (65)
    let source = r#": main ( -- ) "\x1b[2K\x41" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => {
            assert_eq!(s.len(), 5); // ESC [ 2 K A
            assert_eq!(s[0], 0x1b); // ESC
            assert_eq!(s[4], 0x41); // 'A'
        }
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_hex_escape_null_byte() {
    let source = r#": main ( -- ) "before\x00after" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => {
            assert_eq!(s.len(), 12); // "before" + NUL + "after"
            assert_eq!(s[6], 0x00);
        }
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_hex_escape_uppercase() {
    // Both uppercase and lowercase hex digits should work.
    // \xNN is byte-clean: it produces the literal byte 0xNN, not a
    // Unicode codepoint (which is what the pre-byte-clean tokenizer did
    // for high-byte escapes — see test_hex_escape_high_bytes).
    let source = r#": main ( -- ) "\x41\x42\x4F" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => {
            assert_eq!(s, b"ABO"); // 0x41='A', 0x42='B', 0x4F='O'
        }
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_hex_escape_high_bytes() {
    // Byte-clean: \xFF is the single byte 0xFF — *not* the UTF-8
    // encoding of U+00FF (which would be two bytes, 0xC3 0xBF).
    // This guarantee is what makes binary protocol literals (OSC pad
    // NULs, magic numbers, IEEE-754 patterns) expressible in Seq.
    let source = r#": main ( -- ) "\xFF" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => {
            assert_eq!(s.as_slice(), &[0xFFu8]);
        }
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_hex_escape_full_byte_range() {
    // Sanity: every \xNN byte round-trips literally, including the
    // high-byte range that would have been UTF-8-expanded under the
    // pre-byte-clean tokenizer.
    let source = r#": main ( -- ) "\x00\x7F\x80\xC3\xDC\xFF" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::StringLiteral(s) => {
            assert_eq!(s.as_slice(), &[0x00, 0x7F, 0x80, 0xC3, 0xDC, 0xFF]);
        }
        _ => panic!("Expected StringLiteral"),
    }
}

#[test]
fn test_hex_escape_incomplete() {
    // \x with only one hex digit
    let source = r#": main ( -- ) "\x1" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Incomplete hex escape"));
}

#[test]
fn test_hex_escape_invalid_digits() {
    // \xGG is not valid hex
    let source = r#": main ( -- ) "\xGG" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid hex escape"));
}

#[test]
fn test_hex_escape_at_end_of_string() {
    // \x at end of string with no digits
    let source = r#": main ( -- ) "test\x" io.write-line ;"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Incomplete hex escape"));
}

#[test]
fn test_unclosed_string_literal() {
    let source = r#": main ( -- ) "unclosed string ;"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(err_msg.contains("Unclosed string literal"));
    // Should include position information (line 1, column 15 for the opening quote)
    assert!(
        err_msg.contains("line 1"),
        "Expected line number in error: {}",
        err_msg
    );
    assert!(
        err_msg.contains("column 15"),
        "Expected column number in error: {}",
        err_msg
    );
}

#[test]
fn test_multiple_word_definitions() {
    let source = r#"
: double ( Int -- Int )
  2 multiply ;

: quadruple ( Int -- Int )
  double double ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 2);
    assert_eq!(program.words[0].name, "double");
    assert_eq!(program.words[1].name, "quadruple");

    // Verify stack effects were parsed
    assert!(program.words[0].effect.is_some());
    assert!(program.words[1].effect.is_some());
}

#[test]
fn test_user_word_calling_user_word() {
    let source = r#"
: helper ( -- )
  "helper called" write_line ;

: main ( -- )
  helper ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 2);

    // Check main calls helper
    match &program.words[1].body[0] {
        Statement::WordCall { name, .. } => assert_eq!(name, "helper"),
        _ => panic!("Expected WordCall to helper"),
    }
}

#[test]
fn test_parse_simple_stack_effect() {
    // Test: ( Int -- Bool )
    // With implicit row polymorphism, this becomes: ( ..rest Int -- ..rest Bool )
    let source = ": test ( Int -- Bool ) 1 ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    let word = &program.words[0];
    assert!(word.effect.is_some());

    let effect = word.effect.as_ref().unwrap();

    // Input: Int on RowVar("rest") (implicit row polymorphism)
    assert_eq!(
        effect.inputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("rest".to_string())),
            top: Type::Int
        }
    );

    // Output: Bool on RowVar("rest") (implicit row polymorphism)
    assert_eq!(
        effect.outputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("rest".to_string())),
            top: Type::Bool
        }
    );
}

#[test]
fn test_parse_row_polymorphic_stack_effect() {
    // Test: ( ..a Int -- ..a Bool )
    let source = ": test ( ..a Int -- ..a Bool ) 1 ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    let word = &program.words[0];
    assert!(word.effect.is_some());

    let effect = word.effect.as_ref().unwrap();

    // Input: Int on RowVar("a")
    assert_eq!(
        effect.inputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Int
        }
    );

    // Output: Bool on RowVar("a")
    assert_eq!(
        effect.outputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Bool
        }
    );
}

#[test]
fn test_parse_invalid_row_var_starts_with_digit() {
    // Test: Row variable cannot start with digit
    let source = ": test ( ..123 Int -- ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("lowercase letter"),
        "Expected error about lowercase letter, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_invalid_row_var_starts_with_uppercase() {
    // Test: Row variable cannot start with uppercase (that's a type variable)
    let source = ": test ( ..Int Int -- ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("lowercase letter") || err_msg.contains("type name"),
        "Expected error about lowercase letter or type name, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_invalid_row_var_with_special_chars() {
    // Test: Row variable cannot contain special characters
    let source = ": test ( ..a-b Int -- ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("letters, numbers, and underscores") || err_msg.contains("Unknown type"),
        "Expected error about valid characters, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_valid_row_var_with_underscore() {
    // Test: Row variable CAN contain underscore
    let source = ": test ( ..my_row Int -- ..my_row Bool ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_ok(), "Should accept row variable with underscore");
}

#[test]
fn test_parse_multiple_types_stack_effect() {
    // Test: ( Int String -- Bool )
    // With implicit row polymorphism: ( ..rest Int String -- ..rest Bool )
    let source = ": test ( Int String -- Bool ) 1 ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input: String on Int on RowVar("rest")
    let (rest, top) = effect.inputs.clone().pop().unwrap();
    assert_eq!(top, Type::String);
    let (rest2, top2) = rest.pop().unwrap();
    assert_eq!(top2, Type::Int);
    assert_eq!(rest2, StackType::RowVar("rest".to_string()));

    // Output: Bool on RowVar("rest") (implicit row polymorphism)
    assert_eq!(
        effect.outputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("rest".to_string())),
            top: Type::Bool
        }
    );
}

#[test]
fn test_parse_type_variable() {
    // Test: ( ..a T -- ..a T T ) for dup
    let source = ": dup ( ..a T -- ..a T T ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input: T on RowVar("a")
    assert_eq!(
        effect.inputs,
        StackType::Cons {
            rest: Box::new(StackType::RowVar("a".to_string())),
            top: Type::Var("T".to_string())
        }
    );

    // Output: T on T on RowVar("a")
    let (rest, top) = effect.outputs.clone().pop().unwrap();
    assert_eq!(top, Type::Var("T".to_string()));
    let (rest2, top2) = rest.pop().unwrap();
    assert_eq!(top2, Type::Var("T".to_string()));
    assert_eq!(rest2, StackType::RowVar("a".to_string()));
}

#[test]
fn test_parse_empty_stack_effect() {
    // Test: ( -- )
    // In concatenative languages, even empty effects are row-polymorphic
    // ( -- ) means ( ..rest -- ..rest ) - preserves stack
    let source = ": test ( -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Both inputs and outputs should use the same implicit row variable
    assert_eq!(effect.inputs, StackType::RowVar("rest".to_string()));
    assert_eq!(effect.outputs, StackType::RowVar("rest".to_string()));
}

#[test]
fn test_parse_invalid_type() {
    // Test invalid type (lowercase, not a row var)
    let source = ": test ( invalid -- Bool ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Unknown type"));
}

#[test]
fn test_parse_unclosed_stack_effect() {
    // Test unclosed stack effect - parser tries to parse all tokens until ')' or EOF
    // In this case, it encounters "body" which is an invalid type
    let source = ": test ( Int -- Bool body ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    // Parser will try to parse "body" as a type and fail
    assert!(err_msg.contains("Unknown type"));
}

#[test]
fn test_parse_simple_quotation_type() {
    // Test: ( [Int -- Int] -- )
    let source = ": apply ( [Int -- Int] -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input should be: Quotation(Int -- Int) on RowVar("rest")
    let (rest, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Quotation(quot_effect) => {
            // Check quotation's input: Int on RowVar("rest")
            assert_eq!(
                quot_effect.inputs,
                StackType::Cons {
                    rest: Box::new(StackType::RowVar("rest".to_string())),
                    top: Type::Int
                }
            );
            // Check quotation's output: Int on RowVar("rest")
            assert_eq!(
                quot_effect.outputs,
                StackType::Cons {
                    rest: Box::new(StackType::RowVar("rest".to_string())),
                    top: Type::Int
                }
            );
        }
        _ => panic!("Expected Quotation type, got {:?}", top),
    }
    assert_eq!(rest, StackType::RowVar("rest".to_string()));
}

#[test]
fn test_parse_quotation_type_with_row_vars() {
    // Test: ( ..a [..a T -- ..a Bool] -- ..a )
    let source = ": test ( ..a [..a T -- ..a Bool] -- ..a ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input: Quotation on RowVar("a")
    let (rest, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Quotation(quot_effect) => {
            // Check quotation's input: T on RowVar("a")
            let (q_in_rest, q_in_top) = quot_effect.inputs.clone().pop().unwrap();
            assert_eq!(q_in_top, Type::Var("T".to_string()));
            assert_eq!(q_in_rest, StackType::RowVar("a".to_string()));

            // Check quotation's output: Bool on RowVar("a")
            let (q_out_rest, q_out_top) = quot_effect.outputs.clone().pop().unwrap();
            assert_eq!(q_out_top, Type::Bool);
            assert_eq!(q_out_rest, StackType::RowVar("a".to_string()));
        }
        _ => panic!("Expected Quotation type, got {:?}", top),
    }
    assert_eq!(rest, StackType::RowVar("a".to_string()));
}

#[test]
fn test_parse_nested_quotation_type() {
    // Test: ( [[Int -- Int] -- Bool] -- )
    let source = ": nested ( [[Int -- Int] -- Bool] -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input: Quotation([Int -- Int] -- Bool) on RowVar("rest")
    let (_, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Quotation(outer_effect) => {
            // Outer quotation input: [Int -- Int] on RowVar("rest")
            let (_, outer_in_top) = outer_effect.inputs.clone().pop().unwrap();
            match outer_in_top {
                Type::Quotation(inner_effect) => {
                    // Inner quotation: Int -- Int
                    assert!(matches!(
                        inner_effect.inputs.clone().pop().unwrap().1,
                        Type::Int
                    ));
                    assert!(matches!(
                        inner_effect.outputs.clone().pop().unwrap().1,
                        Type::Int
                    ));
                }
                _ => panic!("Expected nested Quotation type"),
            }

            // Outer quotation output: Bool
            let (_, outer_out_top) = outer_effect.outputs.clone().pop().unwrap();
            assert_eq!(outer_out_top, Type::Bool);
        }
        _ => panic!("Expected Quotation type"),
    }
}

#[test]
fn test_parse_deeply_nested_quotation_type_exceeds_limit() {
    // Test: Deeply nested quotation types should fail with max depth error
    // Build a quotation type nested 35 levels deep (exceeds MAX_QUOTATION_DEPTH = 32)
    let mut source = String::from(": deep ( ");

    // Build opening brackets: [[[[[[...
    for _ in 0..35 {
        source.push_str("[ -- ");
    }

    source.push_str("Int");

    // Build closing brackets: ...]]]]]]
    for _ in 0..35 {
        source.push_str(" ]");
    }

    source.push_str(" -- ) ;");

    let mut parser = Parser::new(&source);
    let result = parser.parse();

    // Should fail with depth limit error
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("depth") || err_msg.contains("32"),
        "Expected depth limit error, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_empty_quotation_type() {
    // Test: ( [ -- ] -- )
    // An empty quotation type is also row-polymorphic: [ ..rest -- ..rest ]
    let source = ": empty-quot ( [ -- ] -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    let (_, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Quotation(quot_effect) => {
            // Empty quotation preserves the stack (row-polymorphic)
            assert_eq!(quot_effect.inputs, StackType::RowVar("rest".to_string()));
            assert_eq!(quot_effect.outputs, StackType::RowVar("rest".to_string()));
        }
        _ => panic!("Expected Quotation type"),
    }
}

#[test]
fn test_parse_quotation_type_in_output() {
    // Test: ( -- [Int -- Int] )
    let source = ": maker ( -- [Int -- Int] ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Output should be: Quotation(Int -- Int) on RowVar("rest")
    let (_, top) = effect.outputs.clone().pop().unwrap();
    match top {
        Type::Quotation(quot_effect) => {
            assert!(matches!(
                quot_effect.inputs.clone().pop().unwrap().1,
                Type::Int
            ));
            assert!(matches!(
                quot_effect.outputs.clone().pop().unwrap().1,
                Type::Int
            ));
        }
        _ => panic!("Expected Quotation type"),
    }
}

#[test]
fn test_parse_unclosed_quotation_type() {
    // Test: ( [Int -- Int -- )  (missing ])
    let source = ": broken ( [Int -- Int -- ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    // Parser might error with various messages depending on where it fails
    // It should at least indicate a parsing problem
    assert!(
        err_msg.contains("Unclosed")
            || err_msg.contains("Expected")
            || err_msg.contains("Unexpected"),
        "Got error: {}",
        err_msg
    );
}

#[test]
fn test_parse_multiple_quotation_types() {
    // Test: ( [Int -- Int] [String -- Bool] -- )
    let source = ": multi ( [Int -- Int] [String -- Bool] -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Pop second quotation (String -- Bool)
    let (rest, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Quotation(quot_effect) => {
            assert!(matches!(
                quot_effect.inputs.clone().pop().unwrap().1,
                Type::String
            ));
            assert!(matches!(
                quot_effect.outputs.clone().pop().unwrap().1,
                Type::Bool
            ));
        }
        _ => panic!("Expected Quotation type"),
    }

    // Pop first quotation (Int -- Int)
    let (_, top2) = rest.pop().unwrap();
    match top2 {
        Type::Quotation(quot_effect) => {
            assert!(matches!(
                quot_effect.inputs.clone().pop().unwrap().1,
                Type::Int
            ));
            assert!(matches!(
                quot_effect.outputs.clone().pop().unwrap().1,
                Type::Int
            ));
        }
        _ => panic!("Expected Quotation type"),
    }
}

#[test]
fn test_parse_quotation_type_without_separator() {
    // Test: ( [Int] -- ) should be REJECTED
    //
    // Design decision: The '--' separator is REQUIRED for clarity.
    // [Int] looks like a list type in most languages, not a consumer function.
    // This would confuse users.
    //
    // Require explicit syntax:
    // - `[Int -- ]` for quotation that consumes Int and produces nothing
    // - `[ -- Int]` for quotation that produces Int
    // - `[Int -- Int]` for transformation
    let source = ": consumer ( [Int] -- ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    // Should fail with helpful error message
    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("require") && err_msg.contains("--"),
        "Expected error about missing '--' separator, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_bare_quotation_type_rejected() {
    // Test: ( Int Quotation -- Int ) should be REJECTED
    //
    // 'Quotation' looks like a type name but would be silently treated as a
    // type variable without this check. Users must use explicit effect syntax.
    let source = ": apply-twice ( Int Quotation -- Int ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("Quotation") && err_msg.contains("not a valid type"),
        "Expected error about 'Quotation' not being valid, got: {}",
        err_msg
    );
    assert!(
        err_msg.contains("[Int -- Int]") || err_msg.contains("[ -- ]"),
        "Expected error to suggest explicit syntax, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_no_stack_effect() {
    // Test word without stack effect (should still work)
    let source = ": test 1 2 add ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert!(program.words[0].effect.is_none());
}

#[test]
fn test_parse_simple_quotation() {
    // T is a single-letter type variable — multi-character names like `Quot`
    // are no longer accepted as ad-hoc type vars (they're typo-prone and
    // collide with concrete types). Use a real Quotation effect or `T` here.
    let source = r#"
: test ( -- T )
  [ 1 add ] ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].name, "test");
    assert_eq!(program.words[0].body.len(), 1);

    match &program.words[0].body[0] {
        Statement::Quotation { body, .. } => {
            assert_eq!(body.len(), 2);
            assert_eq!(body[0], Statement::IntLiteral(1));
            assert!(matches!(&body[1], Statement::WordCall { name, .. } if name == "add"));
        }
        _ => panic!("Expected Quotation statement"),
    }
}

#[test]
fn test_parse_empty_quotation() {
    let source = ": test [ ] ;";

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);

    match &program.words[0].body[0] {
        Statement::Quotation { body, .. } => {
            assert_eq!(body.len(), 0);
        }
        _ => panic!("Expected Quotation statement"),
    }
}

#[test]
fn test_parse_quotation_with_call() {
    let source = r#"
: test ( -- )
  5 [ 1 add ] call ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 3);

    assert_eq!(program.words[0].body[0], Statement::IntLiteral(5));

    match &program.words[0].body[1] {
        Statement::Quotation { body, .. } => {
            assert_eq!(body.len(), 2);
        }
        _ => panic!("Expected Quotation"),
    }

    assert!(matches!(
        &program.words[0].body[2],
        Statement::WordCall { name, .. } if name == "call"
    ));
}

#[test]
fn test_parse_nested_quotation() {
    let source = ": test [ [ 1 add ] call ] ;";

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);

    match &program.words[0].body[0] {
        Statement::Quotation {
            body: outer_body, ..
        } => {
            assert_eq!(outer_body.len(), 2);

            match &outer_body[0] {
                Statement::Quotation {
                    body: inner_body, ..
                } => {
                    assert_eq!(inner_body.len(), 2);
                    assert_eq!(inner_body[0], Statement::IntLiteral(1));
                    assert!(
                        matches!(&inner_body[1], Statement::WordCall { name, .. } if name == "add")
                    );
                }
                _ => panic!("Expected nested Quotation"),
            }

            assert!(matches!(&outer_body[1], Statement::WordCall { name, .. } if name == "call"));
        }
        _ => panic!("Expected Quotation"),
    }
}

#[test]
fn test_parse_while_with_quotations() {
    let source = r#"
: countdown ( Int -- )
  [ dup 0 > ] [ 1 subtract ] while drop ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 4);

    // First quotation: [ dup 0 > ]
    match &program.words[0].body[0] {
        Statement::Quotation { body: pred, .. } => {
            assert_eq!(pred.len(), 3);
            assert!(matches!(&pred[0], Statement::WordCall { name, .. } if name == "dup"));
            assert_eq!(pred[1], Statement::IntLiteral(0));
            assert!(matches!(&pred[2], Statement::WordCall { name, .. } if name == ">"));
        }
        _ => panic!("Expected predicate quotation"),
    }

    // Second quotation: [ 1 subtract ]
    match &program.words[0].body[1] {
        Statement::Quotation { body, .. } => {
            assert_eq!(body.len(), 2);
            assert_eq!(body[0], Statement::IntLiteral(1));
            assert!(matches!(&body[1], Statement::WordCall { name, .. } if name == "subtract"));
        }
        _ => panic!("Expected body quotation"),
    }

    // while call
    assert!(matches!(
        &program.words[0].body[2],
        Statement::WordCall { name, .. } if name == "while"
    ));

    // drop
    assert!(matches!(
        &program.words[0].body[3],
        Statement::WordCall { name, .. } if name == "drop"
    ));
}

#[test]
fn test_parse_simple_closure_type() {
    // Test: ( Int -- Closure[Int -- Int] )
    let source = ": make-adder ( Int -- Closure[Int -- Int] ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    let word = &program.words[0];
    assert!(word.effect.is_some());

    let effect = word.effect.as_ref().unwrap();

    // Input: Int on RowVar("rest")
    let (input_rest, input_top) = effect.inputs.clone().pop().unwrap();
    assert_eq!(input_top, Type::Int);
    assert_eq!(input_rest, StackType::RowVar("rest".to_string()));

    // Output: Closure[Int -- Int] on RowVar("rest")
    let (output_rest, output_top) = effect.outputs.clone().pop().unwrap();
    match output_top {
        Type::Closure { effect, captures } => {
            // Closure effect: Int -> Int
            assert_eq!(
                effect.inputs,
                StackType::Cons {
                    rest: Box::new(StackType::RowVar("rest".to_string())),
                    top: Type::Int
                }
            );
            assert_eq!(
                effect.outputs,
                StackType::Cons {
                    rest: Box::new(StackType::RowVar("rest".to_string())),
                    top: Type::Int
                }
            );
            // Captures should be empty (filled in by type checker)
            assert_eq!(captures.len(), 0);
        }
        _ => panic!("Expected Closure type, got {:?}", output_top),
    }
    assert_eq!(output_rest, StackType::RowVar("rest".to_string()));
}

#[test]
fn test_parse_closure_type_with_row_vars() {
    // Test: ( ..a T -- ..a Closure[U -- V] )
    // Single-letter type variables — multi-character names like `Config`/
    // `Request`/`Response` are no longer accepted as ad-hoc type vars under
    // the strict parser rule.
    let source = ": make-handler ( ..a T -- ..a Closure[U -- V] ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Output: Closure on RowVar("a")
    let (rest, top) = effect.outputs.clone().pop().unwrap();
    match top {
        Type::Closure { effect, .. } => {
            // Closure effect: U -> V
            let (_, in_top) = effect.inputs.clone().pop().unwrap();
            assert_eq!(in_top, Type::Var("U".to_string()));
            let (_, out_top) = effect.outputs.clone().pop().unwrap();
            assert_eq!(out_top, Type::Var("V".to_string()));
        }
        _ => panic!("Expected Closure type"),
    }
    assert_eq!(rest, StackType::RowVar("a".to_string()));
}

#[test]
fn test_parse_closure_type_missing_bracket() {
    // Test: ( Int -- Closure ) should fail
    let source = ": broken ( Int -- Closure ) ;";
    let mut parser = Parser::new(source);
    let result = parser.parse();

    assert!(result.is_err());
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("[") && err_msg.contains("Closure"),
        "Expected error about missing '[' after Closure, got: {}",
        err_msg
    );
}

#[test]
fn test_parse_closure_type_in_input() {
    // Test: ( Closure[Int -- Int] -- )
    let source = ": apply-closure ( Closure[Int -- Int] -- ) ;";
    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    let effect = program.words[0].effect.as_ref().unwrap();

    // Input: Closure[Int -- Int] on RowVar("rest")
    let (_, top) = effect.inputs.clone().pop().unwrap();
    match top {
        Type::Closure { effect, .. } => {
            // Verify closure effect
            assert!(matches!(effect.inputs.clone().pop().unwrap().1, Type::Int));
            assert!(matches!(effect.outputs.clone().pop().unwrap().1, Type::Int));
        }
        _ => panic!("Expected Closure type in input"),
    }
}

// Tests for token position tracking

#[test]
fn test_token_position_single_line() {
    // Test token positions on a single line
    let source = ": main ( -- ) ;";
    let tokens = tokenize(source);

    // : is at line 0, column 0
    assert_eq!(tokens[0].text, ":");
    assert_eq!(tokens[0].line, 0);
    assert_eq!(tokens[0].column, 0);

    // main is at line 0, column 2
    assert_eq!(tokens[1].text, "main");
    assert_eq!(tokens[1].line, 0);
    assert_eq!(tokens[1].column, 2);

    // ( is at line 0, column 7
    assert_eq!(tokens[2].text, "(");
    assert_eq!(tokens[2].line, 0);
    assert_eq!(tokens[2].column, 7);
}

#[test]
fn test_token_position_multiline() {
    // Test token positions across multiple lines
    let source = ": main ( -- )\n  42\n;";
    let tokens = tokenize(source);

    // Find the 42 token (after the newline)
    let token_42 = tokens.iter().find(|t| t.text == "42").unwrap();
    assert_eq!(token_42.line, 1);
    assert_eq!(token_42.column, 2); // After 2 spaces of indentation

    // Find the ; token (on line 2)
    let token_semi = tokens.iter().find(|t| t.text == ";").unwrap();
    assert_eq!(token_semi.line, 2);
    assert_eq!(token_semi.column, 0);
}

#[test]
fn test_word_def_source_location_span() {
    // Test that word definitions capture correct start and end lines
    let source = r#": helper ( -- )
  "hello"
  write_line
;

: main ( -- )
  helper
;"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 2);

    // First word: helper spans lines 0-3
    let helper = &program.words[0];
    assert_eq!(helper.name, "helper");
    let helper_source = helper.source.as_ref().unwrap();
    assert_eq!(helper_source.start_line, 0);
    assert_eq!(helper_source.end_line, 3);

    // Second word: main spans lines 5-7
    let main_word = &program.words[1];
    assert_eq!(main_word.name, "main");
    let main_source = main_word.source.as_ref().unwrap();
    assert_eq!(main_source.start_line, 5);
    assert_eq!(main_source.end_line, 7);
}

#[test]
fn test_token_position_string_with_newline() {
    // Test that newlines inside strings are tracked correctly
    let source = "\"line1\\nline2\"";
    let tokens = tokenize(source);

    // The string token should start at line 0, column 0
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].line, 0);
    assert_eq!(tokens[0].column, 0);
}

// ============================================================================
//                         ADT PARSING TESTS
// ============================================================================

#[test]
fn test_parse_simple_union() {
    let source = r#"
union Message {
  Get { response-chan: Int }
  Set { value: Int }
}

: main ( -- ) ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.unions.len(), 1);
    let union_def = &program.unions[0];
    assert_eq!(union_def.name, "Message");
    assert_eq!(union_def.variants.len(), 2);

    // Check first variant
    assert_eq!(union_def.variants[0].name, "Get");
    assert_eq!(union_def.variants[0].fields.len(), 1);
    assert_eq!(union_def.variants[0].fields[0].name, "response-chan");
    assert_eq!(union_def.variants[0].fields[0].type_name, "Int");

    // Check second variant
    assert_eq!(union_def.variants[1].name, "Set");
    assert_eq!(union_def.variants[1].fields.len(), 1);
    assert_eq!(union_def.variants[1].fields[0].name, "value");
    assert_eq!(union_def.variants[1].fields[0].type_name, "Int");
}

#[test]
fn test_parse_union_with_multiple_fields() {
    let source = r#"
union Report {
  Data { op: Int, delta: Int, total: Int }
  Empty
}

: main ( -- ) ;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.unions.len(), 1);
    let union_def = &program.unions[0];
    assert_eq!(union_def.name, "Report");
    assert_eq!(union_def.variants.len(), 2);

    // Check Data variant with 3 fields
    let data_variant = &union_def.variants[0];
    assert_eq!(data_variant.name, "Data");
    assert_eq!(data_variant.fields.len(), 3);
    assert_eq!(data_variant.fields[0].name, "op");
    assert_eq!(data_variant.fields[1].name, "delta");
    assert_eq!(data_variant.fields[2].name, "total");

    // Check Empty variant with no fields
    let empty_variant = &union_def.variants[1];
    assert_eq!(empty_variant.name, "Empty");
    assert_eq!(empty_variant.fields.len(), 0);
}

#[test]
fn test_parse_union_lowercase_name_error() {
    let source = r#"
union message {
  Get { }
}
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("uppercase"));
}

#[test]
fn test_parse_union_empty_error() {
    let source = r#"
union Message {
}
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("at least one variant"));
}

#[test]
fn test_parse_union_duplicate_variant_error() {
    let source = r#"
union Message {
  Get { x: Int }
  Get { y: String }
}
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Duplicate variant name"));
    assert!(err.contains("Get"));
}

#[test]
fn test_parse_union_duplicate_field_error() {
    let source = r#"
union Data {
  Record { x: Int, x: String }
}
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains("Duplicate field name"));
    assert!(err.contains("x"));
}

#[test]
fn test_parse_simple_match() {
    let source = r#"
: handle ( -- )
  match
Get -> send-response
Set -> process-set
  end
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 1);

    match &program.words[0].body[0] {
        Statement::Match { arms, span: _ } => {
            assert_eq!(arms.len(), 2);

            // First arm: Get ->
            match &arms[0].pattern {
                Pattern::Variant(name) => assert_eq!(name, "Get"),
                _ => panic!("Expected Variant pattern"),
            }
            assert_eq!(arms[0].body.len(), 1);

            // Second arm: Set ->
            match &arms[1].pattern {
                Pattern::Variant(name) => assert_eq!(name, "Set"),
                _ => panic!("Expected Variant pattern"),
            }
            assert_eq!(arms[1].body.len(), 1);
        }
        _ => panic!("Expected Match statement"),
    }
}

#[test]
fn test_parse_match_with_bindings() {
    let source = r#"
: handle ( -- )
  match
Get { >chan } -> chan send-response
Report { >delta >total } -> delta total process
  end
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    assert_eq!(program.words.len(), 1);

    match &program.words[0].body[0] {
        Statement::Match { arms, span: _ } => {
            assert_eq!(arms.len(), 2);

            // First arm: Get { chan } ->
            match &arms[0].pattern {
                Pattern::VariantWithBindings { name, bindings } => {
                    assert_eq!(name, "Get");
                    assert_eq!(bindings.len(), 1);
                    assert_eq!(bindings[0], "chan");
                }
                _ => panic!("Expected VariantWithBindings pattern"),
            }

            // Second arm: Report { delta total } ->
            match &arms[1].pattern {
                Pattern::VariantWithBindings { name, bindings } => {
                    assert_eq!(name, "Report");
                    assert_eq!(bindings.len(), 2);
                    assert_eq!(bindings[0], "delta");
                    assert_eq!(bindings[1], "total");
                }
                _ => panic!("Expected VariantWithBindings pattern"),
            }
        }
        _ => panic!("Expected Match statement"),
    }
}

#[test]
fn test_parse_match_bindings_require_prefix() {
    // Old syntax without > prefix should error
    let source = r#"
: handle ( -- )
  match
Get { chan } -> chan send-response
  end
;
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.contains(">chan"));
    assert!(err.contains("stack extraction"));
}

#[test]
fn test_parse_match_with_body_statements() {
    let source = r#"
: handle ( -- )
  match
Get -> 1 2 add send-response
Set -> process-value store
  end
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::Match { arms, span: _ } => {
            // Get arm has 4 statements: 1, 2, add, send-response
            assert_eq!(arms[0].body.len(), 4);
            assert_eq!(arms[0].body[0], Statement::IntLiteral(1));
            assert_eq!(arms[0].body[1], Statement::IntLiteral(2));
            assert!(matches!(&arms[0].body[2], Statement::WordCall { name, .. } if name == "add"));

            // Set arm has 2 statements: process-value, store
            assert_eq!(arms[1].body.len(), 2);
        }
        _ => panic!("Expected Match statement"),
    }
}

#[test]
fn test_parse_match_empty_error() {
    let source = r#"
: handle ( -- )
  match
  end
;
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("at least one arm"));
}

#[test]
fn test_parse_symbol_literal() {
    let source = r#"
: main ( -- )
:hello drop
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert_eq!(program.words.len(), 1);

    let main = &program.words[0];
    assert_eq!(main.body.len(), 2);

    match &main.body[0] {
        Statement::Symbol(name) => assert_eq!(name, "hello"),
        _ => panic!("Expected Symbol statement, got {:?}", main.body[0]),
    }
}

#[test]
fn test_parse_symbol_with_hyphen() {
    let source = r#"
: main ( -- )
:hello-world drop
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::Symbol(name) => assert_eq!(name, "hello-world"),
        _ => panic!("Expected Symbol statement"),
    }
}

#[test]
fn test_parse_symbol_starting_with_digit_fails() {
    let source = r#"
: main ( -- )
:123abc drop
;
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("cannot start with a digit"));
}

#[test]
fn test_parse_symbol_with_invalid_char_fails() {
    let source = r#"
: main ( -- )
:hello@world drop
;
"#;

    let mut parser = Parser::new(source);
    let result = parser.parse();
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("invalid character"));
}

#[test]
fn test_parse_symbol_special_chars_allowed() {
    // Test that ? and ! are allowed in symbol names
    let source = r#"
: main ( -- )
:empty? drop
:save! drop
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();

    match &program.words[0].body[0] {
        Statement::Symbol(name) => assert_eq!(name, "empty?"),
        _ => panic!("Expected Symbol statement"),
    }
    match &program.words[0].body[2] {
        Statement::Symbol(name) => assert_eq!(name, "save!"),
        _ => panic!("Expected Symbol statement"),
    }
}

#[test]
fn test_comment_no_space_after_hash() {
    // `#xxx` (no space) should be a line comment, just like `# xxx`.
    // Without this, `#drop` would tokenize as one identifier and fail
    // word-call validation later.
    let source = r#"
: main ( -- Int )
    #drop drop 0
    42
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 1);
    assert_eq!(program.words[0].body[0], Statement::IntLiteral(42));
}

#[test]
fn test_comment_with_space_after_hash() {
    // The space variant must continue to work — both forms parse to
    // the same thing.
    let source = r#"
: main ( -- Int )
    # this is a comment with a space
    42
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].body.len(), 1);
    assert_eq!(program.words[0].body[0], Statement::IntLiteral(42));
}

#[test]
fn test_inline_comment_after_code() {
    // A `#` mid-line, after real code, ends the line as a comment.
    let source = r#"
: main ( -- Int )
    42 #commented-out drop drop drop
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert_eq!(program.words[0].body.len(), 1);
    assert_eq!(program.words[0].body[0], Statement::IntLiteral(42));
}

#[test]
fn test_shebang_still_recognized() {
    // `#!/usr/bin/env seqc` is just `#` followed by tokens until newline,
    // so the same comment-skipping path handles it. No regression on the
    // pre-existing shebang support.
    let source = "#!/usr/bin/env seqc\n: main ( -- Int ) 7 ;\n";

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert_eq!(program.words.len(), 1);
    assert_eq!(program.words[0].name, "main");
    assert_eq!(program.words[0].body[0], Statement::IntLiteral(7));
}

#[test]
fn test_seq_allow_annotation_with_space() {
    // The canonical `# seq:allow(...)` form (with a space) must keep
    // working. This guards the parser path explicitly — the existing
    // `error_flag_lint` tests build the AST directly and don't
    // exercise tokenization or `skip_comments`.
    let source = r#"
# seq:allow(unchecked-list-get)
: main ( -- Int )
    99
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert!(
        program.words[0]
            .allowed_lints
            .contains(&"unchecked-list-get".to_string()),
        "expected `unchecked-list-get` in allowed_lints (space variant), got {:?}",
        program.words[0].allowed_lints
    );
}

#[test]
fn test_seq_allow_annotation_no_space() {
    // `#seq:allow(...)` (no space) must still register the lint
    // suppression. The annotation has to immediately precede the word
    // def — that's how `pending_allowed_lints` flushes into the next
    // `parse_word_def`.
    let source = r#"
#seq:allow(unchecked-list-get)
: main ( -- Int )
    99
;
"#;

    let mut parser = Parser::new(source);
    let program = parser.parse().unwrap();
    assert!(
        program.words[0]
            .allowed_lints
            .contains(&"unchecked-list-get".to_string()),
        "expected `unchecked-list-get` in allowed_lints, got {:?}",
        program.words[0].allowed_lints
    );
}
