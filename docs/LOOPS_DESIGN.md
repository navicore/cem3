# Loop Design for cem3

## Goal

Add `while/do/end` loops to enable:
- Reading arbitrary-length header lists in HTTP servers
- Accept loops for handling multiple requests
- Any iteration over unknown-length data

## Syntax

Forth-style loop syntax:

```cem
while <condition> do <body> end
```

**Example 1: Countdown**
```cem
: countdown ( n -- )
  while dup 0 > do
    dup write_line
    1 subtract
  end
  drop
;

10 countdown
# Prints: 10, 9, 8, ..., 1
```

**Example 2: Read lines until empty**
```cem
: read-until-empty ( -- lines... count )
  0  # Initialize counter
  while
    read-line
    dup empty? not  # Condition: line is not empty
  do
    # Stack: ( counter line )
    swap 1 add swap  # Increment counter
  end
  drop  # Drop the empty line
;
```

**Example 3: HTTP header reading**
```cem
: read-headers ( -- header1 header2 ... count )
  0  # Header count
  while
    read-line
    dup empty? not
  do
    # Process header
    swap 1 add swap
  end
  drop  # Drop empty line
;
```

## Semantics

### Execution Model

1. **Entry**: Jump to condition block
2. **Condition block**:
   - Execute condition words
   - Evaluate result as boolean (0 = false, non-zero = true for Forth-style)
   - If true: jump to body block
   - If false: jump to exit block
3. **Body block**:
   - Execute body words
   - Jump back to condition block
4. **Exit block**: Continue with rest of program

### Stack Effects

**Critical invariant**: Loop body must preserve stack shape.

```cem
# Valid: Body preserves depth
while dup 0 > do
  1 subtract  # ( n -- n-1 ) - same depth
end

# Invalid: Body changes depth
while dup 0 > do
  1 subtract
  2 add
  3 add  # Stack grows - INVALID!
end
```

**Condition stack effect**: Condition must leave a boolean (or int) on stack.

```cem
# Valid: Condition produces boolean
while dup 0 > do  # ( n -- n bool )
  ...
end

# Invalid: Condition doesn't produce boolean
while dup do  # ( n -- n n ) - no boolean!
  ...
end
```

**Overall loop effect**:

The loop's stack effect is determined by:
1. Stack state before condition
2. Stack state after condition (with boolean consumed)
3. Loop body preserves this state

```cem
# Before loop: ( n )
while dup 0 > do  # Condition: ( n -- n bool )
  1 subtract      # Body: ( n -- n-1 )
end
# After loop: ( n ) where n <= 0
```

## Alternative Syntax Considered

### Option 1: while/do/end (CHOSEN)
```cem
while <condition> do <body> end
```

**Pros:**
- Explicit condition evaluation
- Familiar to Forth programmers
- Clear separation of condition and body

**Cons:**
- Three keywords (while, do, end)

### Option 2: begin/while/repeat
```cem
begin <body> <condition> while repeat
```

**Pros:**
- Body comes before condition (more Forth-like)
- Enables do-while style loops

**Cons:**
- Confusing for those not familiar with Forth
- Condition at end makes stack effect reasoning harder

### Option 3: loop/until
```cem
loop <body> <condition> until
```

**Pros:**
- Only two keywords
- Clear termination condition

**Cons:**
- Inverted logic (loop until condition is true, not while)

**Decision: Use Option 1 (while/do/end)** for clarity and familiarity.

## Type Checking Requirements

### 1. Condition Type

Condition must produce a boolean:

```rust
// In type checker
fn check_loop(&mut self, condition: &[Word], body: &[Word]) -> Result<StackEffect> {
    // Check condition
    let cond_effect = self.check_words(condition)?;

    // Ensure condition produces a boolean
    if !cond_effect.outputs.ends_with_bool() {
        return Err(TypeError::ConditionNotBoolean);
    }

    // ... rest of checking
}
```

### 2. Body Preserves Stack Shape

Loop body must have same inputs and outputs:

```rust
// Check body preserves stack shape
let body_effect = self.check_words(body)?;

if body_effect.inputs != body_effect.outputs {
    return Err(TypeError::LoopBodyChangesStack {
        expected: body_effect.inputs,
        actual: body_effect.outputs,
    });
}
```

### 3. Overall Loop Effect

```rust
// Loop effect: condition inputs consumed, outputs preserved
let loop_effect = StackEffect {
    inputs: cond_effect.inputs.clone(),
    outputs: body_effect.outputs.clone(),
};
```

## Code Generation (LLVM IR)

### Example: `while dup 0 > do 1 subtract end`

```llvm
define ptr @loop_example(ptr %stack.entry) {
entry:
  br label %loop.cond

loop.cond:
  ; Phi for stack pointer
  %stack.cond = phi ptr [ %stack.entry, %entry ],
                        [ %stack.body, %loop.body ]

  ; Evaluate condition: dup 0 >
  %stack1 = call ptr @dup(ptr %stack.cond)
  %stack2 = call ptr @push_int(ptr %stack1, i64 0)
  %stack3 = call ptr @gt(ptr %stack2)

  ; Extract boolean result
  %cond = call i1 @pop_bool(ptr %stack3)

  ; Branch based on condition
  br i1 %cond, label %loop.body, label %loop.exit

loop.body:
  ; Execute body: 1 subtract
  %stack4 = call ptr @push_int(ptr %stack.cond, i64 1)
  %stack.body = call ptr @subtract(ptr %stack4)

  ; Loop back to condition
  br label %loop.cond

loop.exit:
  ; Phi for final stack state
  %stack.final = phi ptr [ %stack.cond, %loop.cond ]
  ret ptr %stack.final
}
```

### Key Codegen Challenges

1. **Phi Nodes**: Thread stack pointer through loop blocks
2. **Condition Evaluation**: Properly convert stack value to i1 for branch
3. **Stack State Management**: Ensure correct stack pointer at each block entry

## Implementation Plan

### Step 1: Parser (1 session)

**Add to lexer:**
- `while` keyword
- `do` keyword
- `end` keyword (already exists for if/then)

**Add to AST:**
```rust
pub enum Word {
    // ... existing variants
    Loop {
        condition: Vec<Word>,
        body: Vec<Word>,
    },
}
```

**Parser logic:**
```rust
fn parse_loop(&mut self) -> Result<Word> {
    self.expect_keyword("while")?;

    let mut condition = Vec::new();
    while !self.check_keyword("do") {
        condition.push(self.parse_word()?);
    }

    self.expect_keyword("do")?;

    let mut body = Vec::new();
    while !self.check_keyword("end") {
        body.push(self.parse_word()?);
    }

    self.expect_keyword("end")?;

    Ok(Word::Loop { condition, body })
}
```

### Step 2: Type Checker (1 session)

**Validation:**
1. Check condition produces boolean
2. Check body preserves stack shape
3. Compute overall loop effect

**Error messages:**
- "Loop condition must produce boolean, got {actual}"
- "Loop body must preserve stack shape: expected {inputs} -> {outputs}, got {actual}"

### Step 3: Code Generation (1-2 sessions)

**Three blocks needed:**
1. `loop.cond`: Evaluate condition, branch
2. `loop.body`: Execute body, jump back
3. `loop.exit`: Continue after loop

**Stack threading:**
- Use phi nodes to merge stack pointers
- Ensure correct stack state at each block

**Testing:**
- Simple countdown loop
- Loop with stack manipulation
- Nested loops (if time permits)

## Testing Strategy

### Unit Tests (Parser)
```rust
#[test]
fn test_parse_simple_loop() {
    let input = "while dup 0 > do 1 subtract end";
    let ast = parse(input).unwrap();
    // Assert structure
}
```

### Unit Tests (Type Checker)
```rust
#[test]
fn test_loop_condition_not_boolean() {
    let input = "while dup do 1 subtract end";
    let err = type_check(input).unwrap_err();
    // Assert error type
}

#[test]
fn test_loop_body_changes_stack() {
    let input = "while dup 0 > do 1 add 2 add end";
    let err = type_check(input).unwrap_err();
    // Assert error type
}
```

### Integration Tests (Codegen)
```rust
#[test]
fn test_simple_countdown() {
    let program = r#"
        : countdown ( n -- )
          while dup 0 > do
            dup write_line
            1 subtract
          end
          drop
        ;

        : main ( -- )
          5 countdown
        ;
    "#;

    let output = compile_and_run(program).unwrap();
    assert_eq!(output, "5\n4\n3\n2\n1\n");
}
```

### Real-World Test (HTTP Headers)
```rust
#[test]
fn test_read_headers_loop() {
    let program = r#"
        : read-headers ( -- count )
          0
          while
            read-line
            dup empty? not
          do
            swap 1 add swap
          end
          drop
        ;
    "#;

    // Feed stdin with headers + empty line
    // Assert count is correct
}
```

## Edge Cases

### Empty Condition
```cem
while do 1 add end  # Error: no condition
```

### Empty Body
```cem
while dup 0 > do end  # Valid but useless (infinite loop if condition starts true)
```

### Infinite Loops
```cem
while 1 do
  "hello" write_line
end
# Infinite loop - no compile-time detection (would need termination analysis)
```

**Decision**: Don't detect infinite loops at compile time. Let program hang (user's responsibility).

### Nested Loops
```cem
while dup 0 > do
  10 while dup 0 > do
    1 subtract
  end
  drop
  1 subtract
end
```

**Decision**: Support nested loops, but defer testing until basic loops work.

## Interaction with Existing Features

### With Conditionals
```cem
while dup 0 > do
  dup 5 = if
    "Found 5!" write_line
  else
    dup write_line
  then
  1 subtract
end
```

**Decision**: Should work seamlessly (nested control flow).

### With Strands
```cem
: worker ( -- )
  while 1 do
    receive
    # Process message
  end
;

spawn worker
```

**Decision**: Loops work in strands just like in main thread.

## Success Criteria

✅ Parser recognizes `while/do/end`
✅ Type checker validates condition produces boolean
✅ Type checker validates body preserves stack shape
✅ Codegen emits correct LLVM IR with phi nodes
✅ Simple countdown loop compiles and runs
✅ HTTP header reading example works
✅ Nested loops work (stretch goal)

## Open Questions

1. **Do we need `break` or `continue`?**
   - **Answer**: Not for Phase 10a. Can add in 10b if needed.

2. **Do we need `until` (inverted while)?**
   - **Answer**: Not for Phase 10a. `while <cond> not` is sufficient.

3. **Do we need do-while (condition at end)?**
   - **Answer**: Not for Phase 10a. Can add `begin/until` later if needed.

4. **Should we detect infinite loops?**
   - **Answer**: No. Termination analysis is complex, defer to future.

## References

- Forth loop constructs: https://www.forth.com/starting-forth/4-conditional-if-then-statements/
- Factor loops: https://docs.factorcode.org/content/article-sequences-combinators.html
- LLVM loop docs: https://llvm.org/docs/LangRef.html#loop-optimization-metadata
