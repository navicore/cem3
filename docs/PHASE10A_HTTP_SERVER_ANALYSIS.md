# Phase 10a: Minimal HTTP Server Features

## Goal

Add **just enough** language features to build a proof-of-concept HTTP server that:
- Accepts connections (one at a time, blocking is OK)
- Reads HTTP request headers
- Routes based on path
- Sends simple responses

## Pseudo-Code HTTP Server (What We Want to Write)

```cem
# Simple HTTP echo server

: read-headers ( -- headers )
  # Read lines until empty line
  # BLOCKED: Need loops!
  # while
  #   read-line dup empty? not
  # do
  #   # Add to headers list
  # end
  # drop  # Drop the empty line
;

: parse-request-line ( String -- method path )
  # "GET /api/users HTTP/1.1"
  # BLOCKED: Need string split!
  " " split     # ( String -- [parts] )
  0 get         # method
  swap 1 get    # path
;

: route ( path -- handler )
  # BLOCKED: Pattern matching would be nice, but can work around with ifs
  dup "/api/users" = if
    drop handle-users
  else dup "/api/posts" = if
    drop handle-posts
  else
    drop handle-404
  then then
;

: handle-request ( -- )
  # Read request line
  read-line                    # ( -- "GET /api/users HTTP/1.1" )
  parse-request-line           # ( -- method path )

  # Read headers (ignore for now)
  read-headers drop

  # Route to handler
  route                        # ( method -- handler )

  # Execute handler
  call                         # Handler writes response
;

: main ( -- )
  # Listen loop
  # BLOCKED: Need loops!
  # while true do
  #   handle-request
  # end

  # For now: handle one request
  handle-request
;
```

## Blocking Features Analysis

### 1. ✅ Can Work Around: Pattern Matching

**Current workaround:** Nested `if/else/then`
```cem
dup "/api/users" = if
  drop handle-users
else dup "/api/posts" = if
  drop handle-posts
else
  drop handle-404
then then
```

**Why defer:**
- Pattern matching is complex (syntax design, exhaustiveness checking, codegen)
- Nested ifs are ugly but functional
- Can add pattern matching in Phase 10b after validating design

**Decision:** ✅ **DEFER to Phase 10b**

---

### 2. ❌ BLOCKING: Loops

**Need:** `while/do/end` for:
- Reading headers until empty line
- Accept loop (handle requests continuously)

**Why essential:**
- Can't parse arbitrary-length header lists without loops
- Can't run a server without an accept loop
- No reasonable workaround (can't unroll infinite loops)

**Implementation complexity:**
- **Parser:** Recognize `while`, `do`, `end` keywords
- **Codegen:** Emit LLVM loop with:
  - Condition block (evaluate condition)
  - Body block (execute if true)
  - Exit block (jump to when false)
  - Phi nodes for stack threading
- **Type checker:** Validate stack effect consistency (loop body must preserve stack shape)

**Example LLVM IR needed:**
```llvm
# while dup 0 > do 1 subtract end
entry:
  br label %loop_cond

loop_cond:
  %stack.cond = phi ptr [ %stack.entry, %entry ], [ %stack.body, %loop_body ]
  %stack1 = call ptr @dup(%stack.cond)
  %stack2 = call ptr @push_int(%stack1, 0)
  %stack3 = call ptr @gt(%stack2)
  %cond = call i1 @pop_bool(%stack3)
  br i1 %cond, label %loop_body, label %loop_exit

loop_body:
  %stack4 = call ptr @push_int(%stack.cond, 1)
  %stack.body = call ptr @subtract(%stack4)
  br label %loop_cond

loop_exit:
  %stack.final = phi ptr [ %stack.cond, %loop_cond ]
  ret %stack.final
```

**Decision:** ❌ **MUST IMPLEMENT (Phase 10a)**

---

### 3. ❌ BLOCKING: String Operations

**Need:**
- `split: ( String delimiter -- [parts] )` - Parse request line, headers
- `empty?: ( String -- Bool )` - Detect end of headers
- `contains?: ( String substring -- Bool )` - Header matching
- `starts-with?: ( String prefix -- Bool )` - Path routing

**Why essential:**
- Can't parse "GET /path HTTP/1.1" without `split`
- Can't detect empty line without `empty?`
- Can't do header-based routing without `contains?`

**Implementation complexity:**
- **Low!** These are just runtime functions (no compiler changes)
- Similar to existing `write_line`, `read_line`

**Example:**
```rust
// runtime/src/string_ops.rs

pub unsafe extern "C" fn string_split(stack: Stack) -> Stack {
    let (stack, delim) = pop(stack);
    let (stack, s) = pop(stack);

    match (s, delim) {
        (Value::String(str), Value::String(d)) => {
            let parts: Vec<Value> = str.as_str()
                .split(d.as_str())
                .map(|part| Value::String(part.into()))
                .collect();

            // Push each part onto stack
            // (or create a Variant list - design decision)
            ...
        }
        _ => panic!("string_split: expected two strings"),
    }
}
```

**Decision:** ❌ **MUST IMPLEMENT (Phase 10a)** - Start here (easiest)

---

## Implementation Plan (Phase 10a)

### Step 1: String Operations (1 session)
**Goal:** Enable basic string manipulation

**Tasks:**
1. Create `runtime/src/string_ops.rs`
2. Implement runtime functions:
   - `string_split: ( String delim -- parts... count )`
   - `string_empty: ( String -- Bool )`
   - `string_contains: ( String substring -- Bool )`
   - `string_starts_with: ( String prefix -- Bool )`
3. Export via C ABI for compiler use
4. Test with unit tests

**Design decision needed:** How to return multiple strings from `split`?
- **Option A:** Push each part + count: `"a b c" " " split → "a" "b" "c" 3`
- **Option B:** Return Variant list: `"a b c" " " split → List("a", "b", "c")`
- **Option C:** Return array as new Value type

**Recommendation:** **Option A** (push parts + count) - simplest, no new types needed

---

### Step 2: Loops (2-3 sessions)
**Goal:** Enable `while/do/end` loops

**Tasks:**

**2.1: Parser Changes**
1. Add keywords: `while`, `do`, `end`
2. Parse loop structure: `while <condition> do <body> end`
3. Create AST node: `Loop { condition: Vec<Word>, body: Vec<Word> }`

**2.2: Type Checker Changes**
1. Validate condition produces Bool (or Int for Forth-style)
2. Validate loop body preserves stack shape:
   - Stack before loop = Stack after loop
   - Condition must consume and produce predictable shape
3. Detect infinite loops (optional, could defer)

**2.3: Codegen Changes**
1. Emit three blocks: `loop_cond`, `loop_body`, `loop_exit`
2. Thread stack pointer with phi nodes
3. Jump to `loop_body` if condition true, `loop_exit` if false
4. Test with simple loops first, then complex

**2.4: Testing**
1. Unit tests for parser
2. Integration tests for codegen
3. Test stack threading correctness
4. Test nested loops (if time permits)

---

### Step 3: Validate with HTTP Server Example (1 session)
**Goal:** Write actual HTTP server code

**Tasks:**
1. Write `examples/http_hello.cem`:
   - Read request line
   - Parse with `split`
   - Read headers with `while` loop
   - Send simple response
2. Compile and run
3. Test with `curl` or `netcat`
4. Identify any missing features

**Success criteria:**
- Server accepts one request
- Parses method and path
- Sends valid HTTP response
- Demonstrates loops and string ops working together

---

## Out of Scope (Defer to Phase 10b+)

1. **Pattern matching** - Use nested ifs for now
2. **Recursion** - Not needed for HTTP server
3. **Lists/Collections** - Use stack manipulation for now
4. **Error handling** - Panic on errors for now
5. **Non-blocking I/O** - Blocking is fine for proof-of-concept
6. **TCP sockets** - Use stdin/stdout with netcat wrapper
7. **String formatting** - Manual concatenation for now

---

## Estimated Effort

- **String operations:** 1 session (easy)
- **Loops:** 2-3 sessions (moderate complexity)
- **HTTP server example:** 1 session (validation)

**Total:** 4-5 sessions for Phase 10a

---

## Success Criteria

✅ Can write loop that reads lines until empty
✅ Can split string on delimiter
✅ Can check if string is empty/contains substring
✅ Can compile and run HTTP server example
✅ Server handles one request successfully
✅ Code is readable (validates design)

---

## Next Steps

1. **Decide:** Option A vs B for `split` return value
2. **Implement:** String operations first (quick win)
3. **Then:** Loops (more complex)
4. **Finally:** Write HTTP server example to validate
