# Float Support Plan

## Goal
Add native floating-point number support to Seq, enabling JSON number parsing and general numeric computation.

## Design Decisions

### 1. Representation
Use IEEE 754 double-precision (f64), matching JSON's number type and Rust's `f64`.

### 2. Naming Convention
Use `f.` prefix for float operations to distinguish from integer operations:
- `f.add`, `f.subtract`, `f.multiply`, `f.divide`
- `f.=`, `f.<`, `f.>`, `f.<=`, `f.>=`, `f.<>`

This is explicit and avoids ambiguity in a stack-based language.

### 3. Stack Operations
Existing polymorphic stack operations (`dup`, `drop`, `swap`, `over`, `rot`, etc.) work unchanged - they operate on `Value`, not specific types.

### 4. Literal Syntax
```seq
3.14          # Simple decimal
-0.5          # Negative
.5            # Leading zero optional
1e10          # Scientific notation
1.5e-3        # Scientific with decimal
```

## Implementation Checklist

### Phase 1: Runtime Foundation

#### 1.1 Value Enum (`runtime/src/value.rs`)
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),        // NEW
    String(SeqString),
    Variant(Box<VariantData>),
    Quotation(usize),
    Closure(Box<ClosureData>),
}
```

#### 1.2 Float Arithmetic (`runtime/src/float_ops.rs`) - NEW FILE
```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_add(stack: Stack) -> Stack {
    let (stack, b) = pop(stack);
    let (stack, a) = pop(stack);
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => push(stack, Value::Float(x + y)),
        _ => panic!("f.add: expected two Floats"),
    }
}

// Similar for: f_subtract, f_multiply, f_divide
```

#### 1.3 Float Comparisons (`runtime/src/float_ops.rs`)
```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_f_eq(stack: Stack) -> Stack {
    let (stack, b) = pop(stack);
    let (stack, a) = pop(stack);
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => {
            let result = if x == y { 1 } else { 0 };
            push(stack, Value::Int(result))  // Returns Int for boolean
        }
        _ => panic!("f.=: expected two Floats"),
    }
}

// Similar for: f_lt, f_gt, f_lte, f_gte, f_neq
```

#### 1.4 Conversions (`runtime/src/float_ops.rs`)
```rust
// int->float: ( Int -- Float )
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_int_to_float(stack: Stack) -> Stack { ... }

// float->int: ( Float -- Int ) - truncates toward zero
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_float_to_int(stack: Stack) -> Stack { ... }

// float->string: ( Float -- String )
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_float_to_string(stack: Stack) -> Stack { ... }
```

#### 1.5 Push Float (`runtime/src/stack.rs`)
```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_push_float(stack: Stack, value: f64) -> Stack {
    push(stack, Value::Float(value))
}
```

### Phase 2: Compiler Support

#### 2.1 AST (`compiler/src/ast.rs`)
```rust
pub enum Statement {
    IntLiteral(i64),
    FloatLiteral(f64),  // NEW
    BoolLiteral(bool),
    StringLiteral(String),
    WordCall(String),
    If { ... },
    Quotation { ... },
}
```

Add to builtins list:
```rust
"f.add", "f.subtract", "f.multiply", "f.divide",
"f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>",
"int->float", "float->int", "float->string",
```

#### 2.2 Parser (`compiler/src/parser.rs`)
Add float literal parsing in `parse_word_or_literal`:
```rust
// Try parsing as float (must check before int due to overlap)
if let Some(f) = try_parse_float(token) {
    return Ok(Statement::FloatLiteral(f));
}
```

Float regex pattern: `^-?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$`

#### 2.3 Type System (`compiler/src/types.rs`)
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,  // NEW
    String,
    Bool,
    Var(String),
    Quotation(Box<Effect>),
    Closure { ... },
}
```

#### 2.4 Builtins (`compiler/src/builtins.rs`)
```rust
// Float arithmetic: ( ..a Float Float -- ..a Float )
for op in &["f.add", "f.subtract", "f.multiply", "f.divide"] {
    sigs.insert(
        op.to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Float)
                .push(Type::Float),
            StackType::RowVar("a".to_string()).push(Type::Float),
        ),
    );
}

// Float comparisons: ( ..a Float Float -- ..a Int )
for op in &["f.=", "f.<", "f.>", "f.<=", "f.>=", "f.<>"] {
    sigs.insert(
        op.to_string(),
        Effect::new(
            StackType::RowVar("a".to_string())
                .push(Type::Float)
                .push(Type::Float),
            StackType::RowVar("a".to_string()).push(Type::Int),
        ),
    );
}

// int->float: ( ..a Int -- ..a Float )
sigs.insert("int->float".to_string(), Effect::new(
    StackType::RowVar("a".to_string()).push(Type::Int),
    StackType::RowVar("a".to_string()).push(Type::Float),
));

// float->int: ( ..a Float -- ..a Int )
sigs.insert("float->int".to_string(), Effect::new(
    StackType::RowVar("a".to_string()).push(Type::Float),
    StackType::RowVar("a".to_string()).push(Type::Int),
));

// float->string: ( ..a Float -- ..a String )
sigs.insert("float->string".to_string(), Effect::new(
    StackType::RowVar("a".to_string()).push(Type::Float),
    StackType::RowVar("a".to_string()).push(Type::String),
));
```

#### 2.5 Codegen (`compiler/src/codegen.rs`)

Declarations:
```rust
writeln!(&mut ir, "declare ptr @patch_seq_push_float(ptr, double)").unwrap();
writeln!(&mut ir, "; Float operations").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_add(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_subtract(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_multiply(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_divide(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_eq(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_lt(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_gt(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_lte(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_gte(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_f_neq(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_int_to_float(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_float_to_int(ptr)").unwrap();
writeln!(&mut ir, "declare ptr @patch_seq_float_to_string(ptr)").unwrap();
```

Statement codegen:
```rust
Statement::FloatLiteral(f) => {
    let result_var = self.fresh_temp();
    writeln!(
        &mut self.output,
        "  %{} = call ptr @patch_seq_push_float(ptr %{}, double {:e})",
        result_var, stack_var, f
    ).unwrap();
    Ok(result_var)
}
```

Name mappings:
```rust
"f.add" => "patch_seq_f_add".to_string(),
"f.subtract" => "patch_seq_f_subtract".to_string(),
"f.multiply" => "patch_seq_f_multiply".to_string(),
"f.divide" => "patch_seq_f_divide".to_string(),
"f.=" => "patch_seq_f_eq".to_string(),
"f.<" => "patch_seq_f_lt".to_string(),
"f.>" => "patch_seq_f_gt".to_string(),
"f.<=" => "patch_seq_f_lte".to_string(),
"f.>=" => "patch_seq_f_gte".to_string(),
"f.<>" => "patch_seq_f_neq".to_string(),
"int->float" => "patch_seq_int_to_float".to_string(),
"float->int" => "patch_seq_float_to_int".to_string(),
"float->string" => "patch_seq_float_to_string".to_string(),
```

### Phase 3: Testing

#### 3.1 Runtime Tests (`runtime/src/float_ops.rs`)
```rust
#[test]
fn test_f_add() {
    unsafe {
        let stack = push(null_mut(), Value::Float(1.5));
        let stack = push(stack, Value::Float(2.5));
        let stack = f_add(stack);
        let (_, result) = pop(stack);
        assert_eq!(result, Value::Float(4.0));
    }
}

#[test]
fn test_float_comparison() {
    unsafe {
        let stack = push(null_mut(), Value::Float(1.5));
        let stack = push(stack, Value::Float(2.5));
        let stack = f_lt(stack);
        let (_, result) = pop(stack);
        assert_eq!(result, Value::Int(1));  // 1.5 < 2.5
    }
}
```

#### 3.2 Integration Test
```seq
: main ( -- Int )
  # Basic arithmetic
  1.5 2.5 f.add 4.0 f.= if
    "PASS: f.add" write_line
  else
    "FAIL: f.add" write_line
  then

  # Conversion
  42 int->float float->string "42" string-equal if
    "PASS: int->float->string" write_line
  else
    "FAIL: conversions" write_line
  then

  # Scientific notation
  1e3 1000.0 f.= if
    "PASS: scientific notation" write_line
  else
    "FAIL: scientific notation" write_line
  then

  0
;
```

## Edge Cases to Handle

1. **Division by zero**: Return infinity (`f64::INFINITY` or `f64::NEG_INFINITY`)
2. **NaN**: Allow NaN to propagate (standard IEEE 754 behavior)
3. **Overflow**: f64 handles this naturally (becomes infinity)
4. **Parser edge cases**: `.5`, `5.`, `1e10`, `1E-5`, `-0.0`

## Future Considerations

1. **Math stdlib**: `f.sin`, `f.cos`, `f.sqrt`, `f.pow`, `f.abs`, etc.
2. **Rounding**: `f.floor`, `f.ceil`, `f.round`
3. **Special values**: `f.nan?`, `f.infinite?`, `f.nan`, `f.infinity`

## Implementation Order

1. Runtime: Value enum + push_float
2. Runtime: float_ops.rs with arithmetic
3. Runtime: float comparisons
4. Runtime: conversions
5. Compiler: AST FloatLiteral
6. Compiler: Parser float parsing
7. Compiler: Type::Float
8. Compiler: builtins signatures
9. Compiler: codegen declarations + statement handling
10. Compiler: AST builtins list
11. Tests: runtime unit tests
12. Tests: integration test

## Estimated Effort

- Runtime changes: 1 session
- Compiler changes: 1 session
- Testing and debugging: 0.5 session

Total: ~2-3 sessions
