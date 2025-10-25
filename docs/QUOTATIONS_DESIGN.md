# Quotations Design for cem3

## Goal

Add **simple quotations** (code blocks without closures) to enable idiomatic concatenative combinators for:
- Looping with `while` combinator
- Iteration with `times` combinator
- Deferred execution with `call`
- Building blocks for HTTP server

## What We're Implementing

**Simple quotations** - blocks of code as first-class values:
```cem
[ dup 0 > ]           # Quotation on stack
[ 1 subtract ]        # Another quotation
call                  # Execute top quotation
```

**NOT implementing (deferred to Phase 10b+):**
- Closures (capturing variables)
- Currying / partial application
- Local variables
- Nested scopes

## Syntax

### Quotation Literals

```cem
[ <statements> ]
```

**Examples:**
```cem
# Simple quotation
[ "hello" write_line ]

# Quotation with stack operations
[ dup 1 add ]

# Multi-statement quotation
[
  dup write_line
  1 subtract
]
```

### Combinators

**call** - Execute a quotation:
```cem
[ "hello" write_line ] call
# Prints: hello
```

**times** - Repeat quotation n times:
```cem
5 [ "hello" write_line ] times
# Prints: hello (5 times)
```

**while** - Loop while predicate is true:
```cem
10 [ dup 0 > ] [ 1 subtract ] while drop
# Counts down from 10 to 0
```

## Semantics

### Quotation as Value

A quotation is a **function pointer** pushed onto the stack:

```cem
[ 1 add ]    # Push function pointer to stack
             # Stack: ( quot )
call         # Pop and execute
             # Equivalent to: 1 add
```

### Stack Effects

Quotations have stack effect types:

```cem
[ 1 add ]              # Type: [ Int -- Int ]
[ dup 0 > ]            # Type: [ Int -- Int Bool ]
[ write_line ]         # Type: [ String -- ]
```

**Type checking:**
- Quotation type = effect of its body
- `call` applies quotation's effect to current stack
- `times` requires `[ -- ]` (no net stack effect)
- `while` requires predicate `[ ..a -- ..a Bool ]` and body `[ ..a -- ..a ]`

### No Closures

Quotations are **pure code blocks** - they don't capture variables:

```cem
# This is OK - no captures
5 [ 1 add ] call      # Adds 1 to 5 -> 6

# This would require closures (NOT supported yet)
: make-adder ( n -- quot )
  [ + ] curry ;       # ERROR: curry not implemented, needs closures
```

## Implementation Strategy

### Phase 1: AST and Parsing (Session 1)

**1. Add Quotation to AST:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    // ... existing variants
    Quotation(Vec<Statement>),
}
```

**2. Parse `[ ... ]` syntax:**
```rust
fn parse_statement(&mut self) -> Result<Statement, String> {
    if token == "[" {
        return self.parse_quotation();
    }
    // ... rest
}

fn parse_quotation(&mut self) -> Result<Statement, String> {
    let mut body = Vec::new();
    while !self.check("]") {
        body.push(self.parse_statement()?);
    }
    self.consume("]");
    Ok(Statement::Quotation(body))
}
```

**3. Tests:**
```rust
#[test]
fn test_parse_simple_quotation() {
    let source = ": test [ 1 add ] call ;";
    // Assert Quotation(vec![IntLiteral(1), WordCall("add")])
}
```

### Phase 2: Type Checking (Session 1)

**1. Add to Value:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    // ... existing variants
    Quotation(usize),  // Function ID for now (not fn pointer yet)
}
```

**2. Type check quotations:**
```rust
Statement::Quotation(body) => {
    // Infer quotation's stack effect
    let quot_effect = self.infer_statements(body)?;

    // Quotations are values - they get pushed onto stack
    // Type: Quotation(effect)
    Ok(current_stack.push(Type::Quotation(Box::new(quot_effect))))
}
```

**3. Add Quotation type:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Bool,
    String,
    Quotation(Box<Effect>),  // [ StackIn -- StackOut ]
}
```

**4. Tests:**
```rust
#[test]
fn test_quotation_type() {
    // : test ( -- Quot ) [ 1 add ] ;
    // Quotation type should be [ Int -- Int ]
}
```

### Phase 3: Code Generation (Session 1-2)

**Approach: Generate Named Functions**

Each quotation becomes a generated function:

```cem
[ dup 1 add ]
```

Becomes:

```llvm
define ptr @quot_1(ptr %stack) {
entry:
  %0 = call ptr @dup(ptr %stack)
  %1 = call ptr @push_int(ptr %0, i64 1)
  %2 = call ptr @add(ptr %1)
  ret ptr %2
}
```

Then push function pointer:

```llvm
%quot_ptr = ptrtoint ptr @quot_1 to i64
%stack1 = call ptr @push_int(ptr %stack, i64 %quot_ptr)
```

**Codegen steps:**

1. **Generate quotation functions:**
```rust
fn codegen_quotation(&mut self, body: &[Statement]) -> Result<String, String> {
    let func_name = self.fresh_quot_func();  // quot_1, quot_2, etc.

    // Generate function definition
    writeln!(&mut self.quotations_output, "define ptr @{}(ptr %stack) {{", func_name)?;
    writeln!(&mut self.quotations_output, "entry:")?;

    let mut stack_var = "stack".to_string();
    for stmt in body {
        stack_var = self.codegen_statement(&stack_var, stmt)?;
    }

    writeln!(&mut self.quotations_output, "  ret ptr %{}", stack_var)?;
    writeln!(&mut self.quotations_output, "}}")?;

    Ok(func_name)
}
```

2. **Push function pointer:**
```rust
Statement::Quotation(body) => {
    let func_name = self.codegen_quotation(body)?;

    // Convert function pointer to integer
    let ptr_temp = self.fresh_temp();
    writeln!(&mut self.output, "  %{} = ptrtoint ptr @{} to i64", ptr_temp, func_name)?;

    // Push onto stack as Int (function pointer)
    let result_var = self.fresh_temp();
    writeln!(&mut self.output, "  %{} = call ptr @push_int(ptr %{}, i64 %{})",
        result_var, stack_var, ptr_temp)?;

    Ok(result_var)
}
```

3. **Tests:**
```rust
#[test]
fn test_codegen_quotation() {
    let program = r#"
        : test ( -- Quot )
          [ 1 add ] ;
    "#;
    // Verify generates quot_1 function
    // Verify pushes function pointer
}
```

### Phase 4: Combinators (Session 2)

**1. Implement `call` in runtime:**

```rust
// runtime/src/combinators.rs

/// Execute a quotation
///
/// Stack effect: ( quot -- ... )
/// The quotation's effect determines final stack
#[unsafe(no_mangle)]
pub unsafe extern "C" fn call(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "call: stack is empty");

    let (stack, value) = unsafe { pop(stack) };

    match value {
        Value::Int(func_ptr) => {
            // Cast function pointer and call it
            type QuotFunc = unsafe extern "C" fn(Stack) -> Stack;
            let func: QuotFunc = unsafe { std::mem::transmute(func_ptr as usize) };
            unsafe { func(stack) }
        }
        _ => panic!("call: expected Quotation (function pointer) on stack"),
    }
}
```

**2. Implement `times`:**

```rust
/// Repeat quotation n times
///
/// Stack effect: ( quot n -- )
#[unsafe(no_mangle)]
pub unsafe extern "C" fn times(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "times: stack needs count");

    let (stack, n_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "times: stack needs quotation");

    let (mut stack, quot_val) = unsafe { pop(stack) };

    match (quot_val, n_val) {
        (Value::Int(func_ptr), Value::Int(n)) => {
            type QuotFunc = unsafe extern "C" fn(Stack) -> Stack;
            let func: QuotFunc = unsafe { std::mem::transmute(func_ptr as usize) };

            for _ in 0..n {
                stack = unsafe { func(stack) };
            }

            stack
        }
        _ => panic!("times: expected quotation and count"),
    }
}
```

**3. Implement `while`:**

```rust
/// Loop while predicate is true
///
/// Stack effect: ( pred-quot body-quot -- )
#[unsafe(no_mangle)]
pub unsafe extern "C" fn while_combinator(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "while: stack needs body");

    let (stack, body_val) = unsafe { pop(stack) };
    assert!(!stack.is_null(), "while: stack needs predicate");

    let (mut stack, pred_val) = unsafe { pop(stack) };

    match (pred_val, body_val) {
        (Value::Int(pred_ptr), Value::Int(body_ptr)) => {
            type QuotFunc = unsafe extern "C" fn(Stack) -> Stack;
            let pred_func: QuotFunc = unsafe { std::mem::transmute(pred_ptr as usize) };
            let body_func: QuotFunc = unsafe { std::mem::transmute(body_ptr as usize) };

            loop {
                // Execute predicate
                let stack_with_bool = unsafe { pred_func(stack) };

                // Pop boolean result
                let (new_stack, bool_val) = unsafe { pop(stack_with_bool) };

                match bool_val {
                    Value::Int(0) => {
                        // Condition false - exit loop
                        stack = new_stack;
                        break;
                    }
                    Value::Int(_) => {
                        // Condition true - execute body
                        stack = unsafe { body_func(new_stack) };
                    }
                    _ => panic!("while: predicate must return Int (boolean)"),
                }
            }

            stack
        }
        _ => panic!("while: expected two quotations"),
    }
}
```

**4. Export combinators:**

```rust
// runtime/src/lib.rs
pub mod combinators;
pub use combinators::{call, times, while_combinator};
```

**5. Add to builtins:**

```rust
// compiler/src/builtins.rs
pub fn builtin_signature(name: &str) -> Option<Effect> {
    match name {
        // ... existing builtins

        "call" => Some(Effect::new(
            // ( Quot -- ... ) - We can't fully type this without dependent types
            // For now: polymorphic
            StackType::row_var("a").push(Type::Quotation(Box::new(Effect::new(
                StackType::row_var("b"),
                StackType::row_var("c"),
            )))),
            StackType::row_var("a").extend(&StackType::row_var("c")),
        )),

        "times" => Some(Effect::new(
            // ( ..a Quot Int -- ..a )
            StackType::row_var("a")
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::row_var("a"),
                    StackType::row_var("a"),
                ))))
                .push(Type::Int),
            StackType::row_var("a"),
        )),

        "while" => Some(Effect::new(
            // ( ..a PredQuot BodyQuot -- ..a )
            StackType::row_var("a")
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::row_var("a"),
                    StackType::row_var("a").push(Type::Int),
                ))))
                .push(Type::Quotation(Box::new(Effect::new(
                    StackType::row_var("a"),
                    StackType::row_var("a"),
                )))),
            StackType::row_var("a"),
        )),

        _ => None,
    }
}
```

**6. Tests:**

```rust
#[test]
fn test_call_combinator() {
    unsafe {
        // Push quotation (function pointer) and call it
        let stack = std::ptr::null_mut();
        let stack = push(stack, Value::Int(5));
        let stack = push(stack, Value::Int(quot_add_one as usize as i64));
        let stack = call(stack);

        let (stack, result) = pop(stack);
        assert_eq!(result, Value::Int(6));
    }
}

#[test]
fn test_times_combinator() {
    // 5 [ "hello" write_line ] times
}

#[test]
fn test_while_combinator() {
    // 10 [ dup 0 > ] [ 1 subtract ] while
}
```

### Phase 5: HTTP Server Example (Session 3)

**Goal:** Validate quotations work for real HTTP server code

```cem
: read-headers ( -- header1 header2 ... count )
  0  # Header count
  [ read-line dup empty? not ]  # Predicate: line not empty
  [ swap 1 add swap ]           # Body: increment count
  while
  drop  # Drop empty line
;

: parse-request-line ( String -- method path )
  " " string-split   # Split on space
  # Stack: parts... count
  3 =  # Should have 3 parts
  if
    # Stack: method path version
    drop swap  # Drop version, reorder to method path
  else
    drop drop "Invalid request line" write_line
    "" ""
  then
;

: main ( -- )
  read-line                      # Read request line
  parse-request-line             # Parse it
  # Stack: method path

  read-headers                   # Read all headers
  # Stack: method path headers... count

  # Simple response
  "HTTP/1.1 200 OK" write-line
  "Content-Length: 13" write-line
  "" write-line
  "Hello, World!" write-line
;
```

## Type System Integration

### Quotation Types

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Bool,
    String,
    Quotation(Box<Effect>),  // NEW
}
```

### Type Checking Quotations

```rust
Statement::Quotation(body) => {
    // Infer effect of quotation body
    let quot_effect = self.infer_statements(body)?;

    // Push quotation type onto stack
    Ok(current_stack.push(Type::Quotation(Box::new(quot_effect))))
}
```

### Polymorphic Combinators

`call`, `times`, and `while` need **row polymorphism** to work generically:

```cem
# call works with any quotation
5 [ 1 add ] call           # ( Int Quot[Int--Int] -- Int )
"hi" [ write_line ] call   # ( String Quot[String--] -- )

# times requires [ ..a -- ..a ] (no net effect)
5 [ "hello" write_line ] times   # OK: [ String -- ] repeated 5 times (with "hello" on stack each time)
```

This is already supported by our existing row polymorphism infrastructure!

## Edge Cases

### Empty Quotation

```cem
[ ]  # Valid - no-op quotation
call # Does nothing
```

### Nested Quotations

```cem
[ [ 1 add ] call ]  # Quotation containing quotation
call                # Executes inner quotation
```

### Quotation with Conditionals

```cem
[ dup 0 > if "positive" else "non-positive" then write_line ]
```

All of these should work naturally with our existing AST structure.

## Success Criteria

✅ Can parse `[ ... ]` quotations
✅ Can type check quotations with correct effects
✅ Can generate LLVM functions for quotations
✅ `call` combinator works
✅ `times` combinator works
✅ `while` combinator works
✅ HTTP header reading example compiles and runs
✅ All existing tests still pass

## Out of Scope (Future Phases)

**Closures (Phase 10b+):**
- Variable capture
- Currying (`, curry`)
- Partial application
- Local bindings

**Advanced Combinators:**
- `map`, `filter`, `fold` (require sequences/lists)
- `each`, `keep`, `reduce` (Factor-style)
- Recursion combinators (`linrec`, `binrec`)

**Quotation Composition:**
- `compose` - combine two quotations
- `dip` - execute quotation below top of stack
- `keep` - preserve value while executing quotation

These will come after we have collections/sequences.

## References

- **Factor quotations:** https://docs.factorcode.org/content/article-quotations.html
- **Cat quotations:** http://cat-language.com/tutorial.html
- **Joy combinators:** http://www.kevinalbrecht.com/code/joy-mirror/joy.html

## Reminder: Closures Deferred

**IMPORTANT:** This design document explicitly defers closures to Phase 10b+.

We are implementing **simple quotations** (code-as-data) without variable capture. This is sufficient for:
- HTTP server with `while` loops
- Iteration with `times`
- Basic control flow

**When we need closures:**
- Currying / partial application
- Creating functions that "remember" values
- Functional programming patterns (map/filter with closures)

But for now, we keep it simple and idiomatic!
