# Closure Design for cem3

**Date:** 2025-11-16

**Goal:** Add proper closure support to enable HTTP servers and higher-order programming

---

## Motivation

**Current limitation:** Quotations are stateless (just function pointers)

**Need:** Closures that capture stack values for:
- HTTP request handlers that capture config/state
- Higher-order combinators (map, filter, fold)
- Event callbacks that remember context

**Example use case:**
```cem
: make-handler ( Config -- [Request -- Response] )
  [
    # This quotation needs to capture Config from outer scope
    swap parse-request
    swap handle-with-config
    build-response
  ]
;

# Usage
my-config make-handler
8080 swap start-http-server
```

---

## Design Principles

1. **Automatic capture** - `[ ... ]` automatically captures needed values
2. **Type-safe** - Type system tracks captured values
3. **Arena-friendly** - Use existing memory infrastructure
4. **Send-safe** - Closures can cross strand boundaries (channels)
5. **Backward compatible** - Existing quotations still work

---

## Part 1: Semantics

### What Gets Captured?

In a concatenative language, closures capture **stack values** needed by the quotation body.

**Example 1: Simple capture**
```cem
: make-adder ( Int -- [Int -- Int] )
  [ add ]  # Captures the Int from outer stack
;

5 make-adder    # Stack: [closure that adds 5]
10 swap call    # Pops [closure], pops 10, adds 5, pushes 15
# Result: 15
```

**Analysis:**
- Quotation body `[ add ]` references `add` which expects 2 Ints
- Only 1 Int provided when calling the closure (the `10`)
- Therefore, 1 Int must be captured from creation site (the `5`)

**Example 2: Multiple captures**
```cem
: make-range-checker ( Int Int -- [Int -- Bool] )
  # Captures min and max
  [ dup rot > swap rot > and ]
;

10 20 make-range-checker  # Captures min=10, max=20
15 swap call              # Checks if 15 is in range [10, 20]
# Result: true
```

**Example 3: No capture needed**
```cem
: make-doubler ( -- [Int -- Int] )
  [ 2 multiply ]  # No capture - 2 is a literal
;

make-doubler   # Stack: [quotation that doubles]
5 swap call    # Result: 10
```

### Capture Analysis Algorithm

**For each quotation `[ body ]`:**

1. **Compute stack effect** of quotation body
   - Inputs needed: `N` values
   - Outputs produced: `M` values

2. **Determine quotation's intended type**
   - From context or annotation: `[A B -- C D]`
   - Declared inputs: `K` values (A, B = 2 values)
   - Declared outputs: `L` values (C, D = 2 values)

3. **Compute captures**
   - If `N > K`, then `N - K` values must be captured
   - Those values come from the **creation site's stack**

4. **Example:**
   ```cem
   5 [ add ]  # Body needs 2 Ints, quotation provides 1 → capture 1 Int
   ```
   - Body (`add`) needs: 2 Ints
   - Quotation type: `[Int -- Int]` (provides 1 Int)
   - Captures: 2 - 1 = 1 Int (the `5`)

---

## Part 2: Type System Changes

### Current Quotation Type

```rust
Type::Quotation(Box<Effect>)

// Example: [Int -- String]
Type::Quotation(Box::new(Effect {
    inputs: StackType::Cons { rest: RowVar("a"), top: Int },
    outputs: StackType::Cons { rest: RowVar("a"), top: String },
}))
```

### New Closure Type

**Option 1: Separate Closure Type**
```rust
pub enum Type {
    Int,
    Bool,
    String,
    Var(String),
    Quotation(Box<Effect>),     // Stateless quotations
    Closure {
        effect: Box<Effect>,     // Stack effect when called
        captures: Vec<Type>,     // Types of captured values
    }
}
```

**Option 2: Augment Quotation Type**
```rust
pub enum Type {
    Int,
    Bool,
    String,
    Var(String),
    Quotation {
        effect: Box<Effect>,
        captures: Option<Vec<Type>>,  // None = stateless, Some = closure
    }
}
```

**Recommendation:** **Option 1** - clearer separation

**Example:**
```cem
: make-adder ( Int -- [Int -- Int] )
  [ add ]
;
```

Type analysis:
- Input: `Int` on stack
- Quotation body `[ add ]` needs 2 Ints, provides 1 → captures 1 Int
- Return type: `Closure { effect: [Int -- Int], captures: [Int] }`

---

## Part 3: Value Representation

### Current Value Enum

```rust
pub enum Value {
    Int(i64),
    Bool(bool),
    String(CemString),
    Variant(Box<VariantData>),
    Quotation(usize),  // Just function pointer
}
```

### New Closure Variant

```rust
pub enum Value {
    Int(i64),
    Bool(bool),
    String(CemString),
    Variant(Box<VariantData>),
    Quotation(usize),          // Stateless quotation (backward compatible)
    Closure {
        fn_ptr: usize,         // Function pointer
        env: Box<[Value]>,     // Captured values (boxed slice)
    }
}
```

**Why `Box<[Value]>` instead of `Vec<Value>`?**
- Slightly more efficient (one less usize for length)
- Captures are immutable once created
- Still owned, still Send

**Memory layout:**
```
Stack: [Value::Closure { fn_ptr: 0x1234, env: Box -> Heap }]
                                                    |
Heap:  [Value::Int(5), Value::String("hello")]  <--+
```

**Send safety:** ✅
- `Box<[Value]>` is Send (owned)
- All `Value` types are Send (Int, Bool, String, etc.)
- Can pass closures through channels

---

## Part 4: AST Changes

### Current Quotation Statement

```rust
pub enum Statement {
    // ...
    Quotation(Vec<Statement>),  // Just the body
}
```

### No Change Needed!

**Why:** At parse time, we don't know what gets captured yet. That's determined during type checking / codegen.

The AST stays simple: `Statement::Quotation(Vec<Statement>)`

**Capture analysis happens in:** Compiler passes (type checker or codegen)

---

## Part 5: Codegen Strategy

### Current Quotation Codegen

**Input:** `[ 2 multiply ]`

**Output:**
```llvm
define ptr @cem_quot_0(ptr %stack) {
entry:
  %0 = call ptr @push_int(ptr %stack, i64 2)
  %1 = call ptr @multiply(ptr %0)
  ret ptr %1
}
```

Then push function pointer onto stack.

### Closure Codegen

**Input:** `5 [ add ]` where quotation captures the 5

**Steps:**

1. **Analyze captures**
   - Body needs 2 Ints
   - Quotation type is `[Int -- Int]` (provides 1 Int)
   - Captures: 1 Int

2. **Generate closure function** with environment parameter:
   ```llvm
   define ptr @cem_closure_0(ptr %stack, ptr %env) {
   entry:
     ; Load captured value from environment
     %captured_0 = call { ptr, %Value } @env_get(ptr %env, i32 0)
     %env_val = extractvalue { ptr, %Value } %captured_0, 1

     ; Push captured value onto stack
     %0 = call ptr @push_value(ptr %stack, %Value %env_val)

     ; Now execute body (add)
     %1 = call ptr @add(ptr %0)
     ret ptr %1
   }
   ```

3. **At quotation creation site**, generate code to:
   ```llvm
   ; Pop value to capture
   %pop_result = call { ptr, %Value } @pop(ptr %stack)
   %new_stack = extractvalue { ptr, %Value } %pop_result, 0
   %captured = extractvalue { ptr, %Value } %pop_result, 1

   ; Create environment (array of captured values)
   %env = call ptr @create_env(i32 1)  ; 1 captured value
   call void @env_set(ptr %env, i32 0, %Value %captured)

   ; Create closure value
   %fn_ptr = ptrtoint ptr @cem_closure_0 to i64
   %closure = call %Value @make_closure(i64 %fn_ptr, ptr %env)

   ; Push closure onto stack
   %final_stack = call ptr @push_value(ptr %new_stack, %Value %closure)
   ```

### Determining What to Capture

**During codegen:**

1. **Type check quotation** to get its stack effect
2. **Compare** with declared type (or infer minimal type)
3. **Compute captures:**
   ```rust
   let body_inputs = typecheck_quotation_body(body)?;
   let quotation_inputs = quotation_type.inputs.count();
   let capture_count = body_inputs - quotation_inputs;
   ```

4. **Generate code** to pop `capture_count` values before creating closure

---

## Part 6: Runtime Support

### New Runtime Functions

**In `runtime/src/closures.rs` (new file):**

```rust
use crate::value::Value;

/// Create closure environment (array of captured values)
#[no_mangle]
pub extern "C" fn create_env(size: i32) -> *mut [Value] {
    let vec: Vec<Value> = Vec::with_capacity(size as usize);
    Box::into_raw(vec.into_boxed_slice())
}

/// Set value in environment
#[no_mangle]
pub unsafe extern "C" fn env_set(env: *mut [Value], index: i32, value: Value) {
    let env_slice = &mut *env;
    env_slice[index as usize] = value;
}

/// Get value from environment
#[no_mangle]
pub unsafe extern "C" fn env_get(env: *const [Value], index: i32) -> Value {
    let env_slice = &*env;
    env_slice[index as usize].clone()
}

/// Create closure value
#[no_mangle]
pub extern "C" fn make_closure(fn_ptr: u64, env: *mut [Value]) -> Value {
    Value::Closure {
        fn_ptr: fn_ptr as usize,
        env: unsafe { Box::from_raw(env) },
    }
}
```

### Updated `call` Implementation

**In `runtime/src/quotations.rs`:**

```rust
#[no_mangle]
pub unsafe extern "C" fn call(stack: Stack) -> Stack {
    let (stack, value) = pop(stack);

    match value {
        // Existing: stateless quotations
        Value::Quotation(fn_ptr) => {
            if fn_ptr == 0 {
                panic!("call: quotation function pointer is null");
            }
            let fn_ref: unsafe extern "C" fn(Stack) -> Stack
                = std::mem::transmute(fn_ptr);
            fn_ref(stack)
        }

        // New: closures with environment
        Value::Closure { fn_ptr, env } => {
            if fn_ptr == 0 {
                panic!("call: closure function pointer is null");
            }

            // Convert environment to raw pointer
            let env_ptr = Box::into_raw(env);

            // Call closure function with environment
            let fn_ref: unsafe extern "C" fn(Stack, *const [Value]) -> Stack
                = std::mem::transmute(fn_ptr);
            let result_stack = fn_ref(stack, env_ptr);

            // Clean up environment (convert back to Box and drop)
            let _ = Box::from_raw(env_ptr);

            result_stack
        }

        _ => panic!("call: expected Quotation or Closure on stack, got {:?}", value),
    }
}
```

---

## Part 7: Type Checking Changes

### Quotation Type Inference

**Current:** Quotations have type `[inputs -- outputs]` from their body

**With closures:** Need to track captures

**Algorithm:**

```rust
fn infer_quotation_type(body: &[Statement], context: &TypeContext) -> Result<Type, String> {
    // 1. Infer body's stack effect
    let body_effect = infer_stack_effect(body, context)?;

    // 2. Check if there's a declared type annotation
    if let Some(declared_type) = context.expected_quotation_type {
        // 3. Compute captures
        let body_inputs = body_effect.inputs.count();
        let declared_inputs = declared_type.effect.inputs.count();

        if body_inputs > declared_inputs {
            let capture_count = body_inputs - declared_inputs;
            let capture_types = body_effect.inputs.take(capture_count);

            return Ok(Type::Closure {
                effect: declared_type.effect,
                captures: capture_types,
            });
        } else {
            // No captures needed - stateless quotation
            return Ok(Type::Quotation(declared_type.effect));
        }
    }

    // 4. No annotation - infer minimal quotation type (no captures)
    Ok(Type::Quotation(Box::new(body_effect)))
}
```

### Example Type Inference

**Example 1:**
```cem
: make-adder ( Int -- [Int -- Int] )
  [ add ]
;
```

1. Body `[ add ]` has effect `( Int Int -- Int )`
2. Declared quotation type: `[Int -- Int]`
3. Body needs 2 Ints, quotation provides 1
4. **Capture 1 Int** from outer stack
5. Result type: `Closure { effect: [Int -- Int], captures: [Int] }`

**Example 2:**
```cem
: make-doubler ( -- [Int -- Int] )
  [ 2 multiply ]
;
```

1. Body `[ 2 multiply ]` has effect `( Int -- Int )` (2 is a literal)
2. Declared quotation type: `[Int -- Int]`
3. Body needs 1 Int, quotation provides 1 Int
4. **No captures** needed
5. Result type: `Quotation([Int -- Int])` (stateless)

---

## Part 8: Examples

### Example 1: HTTP Request Handler

```cem
: Config ( port:Int root:String -- )

: make-404-handler ( Config -- [Request -- Response] )
  [
    # Closure captures Config
    drop  # Drop request
    "HTTP/1.1 404 Not Found\r\n\r\nNot Found"
  ]
;

: make-handler ( Config -- [Request -- Response] )
  [
    # Closure captures Config
    dup "GET /" string-starts-with if
      drop
      "HTTP/1.1 200 OK\r\n\r\nHello!"
    else
      drop
      "HTTP/1.1 404 Not Found\r\n\r\nNot Found"
    then
  ]
;

: main ( -- )
  8080 "/var/www" Config
  make-handler
  8080 swap start-server  # Pass closure as handler
;
```

### Example 2: Higher-Order Combinators

```cem
: map ( ..a [T -- U] List<T> -- ..a List<U> )
  # Closure captures transformation function
  swap [
    # Apply transformation to each element
    swap over call
  ] each
;

# Usage:
[ 2 multiply ] my-list map  # Doubles each element
```

### Example 3: Partial Application

```cem
: partial-add ( Int Int -- [-- Int] )
  [ + ]  # Captures both Ints
;

5 10 partial-add   # Creates [-- Int] closure that returns 15
call               # Result: 15
```

---

## Part 9: Implementation Plan

### Phase 1: Type System (Session 1)

1. Add `Type::Closure` variant to `types.rs`
2. Update type unification to handle closures
3. Add closure type inference
4. Write tests for closure type checking

**Deliverable:** Type system understands closures

### Phase 2: Value Representation (Session 1)

1. Add `Value::Closure` variant to `value.rs`
2. Update `Clone`, `Debug`, `PartialEq` for closures
3. Verify Send safety
4. Write tests for closure values

**Deliverable:** Runtime can represent closures

### Phase 3: Runtime Support (Session 2)

1. Create `runtime/src/closures.rs`
2. Implement `create_env`, `env_set`, `env_get`, `make_closure`
3. Update `call` to handle closures
4. Write tests for closure calling

**Deliverable:** Runtime can create and call closures

### Phase 4: Codegen (Session 2-3)

1. Implement capture analysis in codegen
2. Generate closure functions with environment parameter
3. Generate code to create closures at quotation sites
4. Generate code to load captures from environment
5. Write integration tests

**Deliverable:** Compiler generates working closures

### Phase 5: Testing (Session 3)

1. Unit tests for each component
2. Integration tests (make-adder, make-handler, etc.)
3. Test with channels (closures across strands)
4. Performance testing

**Deliverable:** Closures thoroughly tested

### Phase 6: HTTP Server Example (Session 4)

1. Build simple HTTP server using closures
2. Demonstrate stateful request handlers
3. Test with concurrent requests
4. Document patterns

**Deliverable:** Working HTTP server demo

---

## Part 10: Backward Compatibility

### Existing Quotations Still Work

**Before closures:**
```cem
: double ( -- [Int -- Int] )
  [ 2 multiply ]
;

double call  # Works
```

**After closures:**
```cem
: double ( -- [Int -- Int] )
  [ 2 multiply ]
;

double call  # Still works - creates stateless Quotation
```

**Analysis:** Body needs 1 Int, quotation provides 1 Int → no captures

### Migration Path

**Old code:** Uses `Value::Quotation(usize)`

**New code:** May use `Value::Closure { fn_ptr, env }`

**`call` handles both:**
- `Quotation(fn_ptr)` → call with just stack
- `Closure { fn_ptr, env }` → call with stack + environment

**Result:** Zero breaking changes for existing code

---

## Part 11: Open Questions

### 1. Capture Order

**Question:** When capturing multiple values, what order?

**Options:**
- **Top-down:** Top of stack captured first
- **Bottom-up:** Bottom of stack captured first

**Recommendation:** **Top-down** (matches parameter order)

```cem
5 10 [ + ]  # Captures 10 first, then 5
            # env[0] = 10, env[1] = 5
```

### 2. Explicit vs Implicit Capture

**Current design:** Implicit (automatic)

**Alternative:** Explicit capture syntax:
```cem
: make-adder ( Int -- [Int -- Int] )
  [ x | x add ]  # Explicitly name captured 'x'
;
```

**Decision:** Start with **implicit**, add explicit syntax later if needed

### 3. Mutable Captures?

**Question:** Should captured values be mutable?

**Options:**
- **Immutable:** Safer, simpler, functional style
- **Mutable:** More flexible, but complex

**Recommendation:** **Immutable** for now (captured values are copied)

Future: Could add mutable references via Variant or special capture mode

### 4. Recursion in Closures?

**Question:** Can closures call themselves?

**Example:**
```cem
: factorial ( Int -- [-- Int] )
  [ dup 0 = if drop 1 else dup 1 - factorial call * then ]
;
```

**Issue:** Closure needs to capture itself → infinite recursion in capture analysis

**Solution:** Handle recursion separately (named recursive functions, Y combinator)

**Decision:** Out of scope for initial implementation

---

## Part 12: Success Criteria

**Closures are done when:**

1. ✅ Type system tracks captures
2. ✅ Values can represent closures
3. ✅ Runtime can create and call closures
4. ✅ Compiler generates correct LLVM IR
5. ✅ `make-adder` example works
6. ✅ HTTP handler example works
7. ✅ Closures can cross strand boundaries (channels)
8. ✅ All tests passing
9. ✅ No performance regression for stateless quotations
10. ✅ Backward compatible

---

## Conclusion

This design provides:
- ✅ Automatic capture (user-friendly)
- ✅ Type-safe (compiler tracks captures)
- ✅ Arena-compatible (Box<[Value]> is minimal)
- ✅ Send-safe (works with channels)
- ✅ Backward compatible (stateless quotations unchanged)

**Estimated effort:** 3-4 sessions

**Next step:** Review design, then start implementation with Phase 1 (Type System)
