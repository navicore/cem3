# FFI Callbacks Design

## Overview

FFI callbacks enable C libraries to call back into Seq code. This is required for:
- Custom comparators (qsort)
- Event handlers (GUI, signals)
- Completion functions (readline/libedit)
- Query result handlers (SQLite)

## The Challenge

Seq functions have a uniform signature:
```c
Stack seq_word(Stack stack);           // Quotation
Stack seq_closure(Stack s, Value* env, size_t len);  // Closure
```

C callbacks have arbitrary signatures:
```c
int compare(const void* a, const void* b);           // qsort
char** complete(const char* text, int start, int end); // readline
int callback(void* data, int argc, char** argv, char** cols); // sqlite3_exec
```

We need trampolines that bridge these worlds.

## Design: Static Trampolines

### Approach

1. **Manifest declares callback shapes** - each callback type has a known C signature
2. **Compiler generates trampolines** - type-safe C-to-Seq bridges
3. **Runtime binds quotation to slot** - associate Seq function with trampoline

### Manifest Format

```toml
[[library]]
name = "example"
link = "example"

# Declare callback type
[[library.callback]]
name = "comparator"
# C signature: int (*)(const void*, const void*)
args = [
  { type = "ptr", name = "a" },
  { type = "ptr", name = "b" }
]
return = { type = "int" }
# How it maps to Seq: ( Int Int -- Int )
seq_effect = "( Int Int -- Int )"

# Function that uses the callback
[[library.function]]
c_name = "qsort"
seq_name = "c-qsort"
stack_effect = "( Ptr Int Int [Int Int -- Int] -- )"
args = [
  { type = "ptr", pass = "ptr" },          # base
  { type = "int", pass = "int" },           # nmemb
  { type = "int", pass = "int" },           # size
  { type = "callback", callback = "comparator" }  # compar
]
return = { type = "void" }
```

### Generated Trampoline

For the `comparator` callback:

```llvm
; Global slot to hold the Seq quotation
@callback_comparator_fn = global i64 0
@callback_comparator_impl = global i64 0

; C-callable trampoline
define i32 @seq_callback_comparator(ptr %a, ptr %b) {
entry:
    ; Create fresh stack
    %stack0 = call ptr @patch_seq_stack_new()

    ; Push arguments (C -> Seq direction)
    %a_int = ptrtoint ptr %a to i64
    %stack1 = call ptr @patch_seq_push_int(ptr %stack0, i64 %a_int)
    %b_int = ptrtoint ptr %b to i64
    %stack2 = call ptr @patch_seq_push_int(ptr %stack1, i64 %b_int)

    ; Load and call the bound Seq function
    %fn_ptr = load i64, ptr @callback_comparator_fn
    %fn = inttoptr i64 %fn_ptr to ptr
    %stack3 = call ptr %fn(ptr %stack2)

    ; Pop return value (Seq -> C direction)
    %result_i64 = call i64 @patch_seq_pop_int(ptr %stack3)
    %result = trunc i64 %result_i64 to i32

    ; Clean up stack
    call void @patch_seq_stack_free(ptr %stack3)

    ret i32 %result
}

; Runtime function to bind a quotation to this callback
define void @seq_bind_callback_comparator(i64 %wrapper, i64 %impl) {
    store i64 %wrapper, ptr @callback_comparator_fn
    store i64 %impl, ptr @callback_comparator_impl
    ret void
}
```

### Usage in Seq

```seq
include ffi:example

: my-compare ( Int Int -- Int )
  # a b on stack as raw pointers
  # ... comparison logic ...
  -   # return negative, zero, or positive
;

: main ( -- Int )
  my-array array-ptr
  100                    # count
  8                      # element size
  [ my-compare ]         # callback quotation
  c-qsort
  0
;
```

### Codegen for Callback Arguments

When the compiler sees `{ type = "callback", callback = "comparator" }`:

1. Pop the quotation from the Seq stack
2. Extract wrapper/impl pointers
3. Call `seq_bind_callback_comparator(wrapper, impl)`
4. Pass `@seq_callback_comparator` as the C function pointer

```llvm
; In the FFI wrapper for c-qsort
define ptr @seq_ffi_c_qsort(ptr %stack) {
    ; Pop callback quotation first (it's on top)
    %quot_wrapper = call i64 @patch_seq_peek_quotation_wrapper(ptr %stack)
    %quot_impl = call i64 @patch_seq_peek_quotation_impl(ptr %stack)
    %stack1 = call ptr @patch_seq_drop(ptr %stack)

    ; Bind it to the callback slot
    call void @seq_bind_callback_comparator(i64 %quot_wrapper, i64 %quot_impl)

    ; Pop other arguments
    %size = call i64 @patch_seq_pop_int(ptr %stack1)
    ; ... etc ...

    ; Call C function with trampoline as callback
    call void @qsort(ptr %base, i64 %nmemb, i64 %size, ptr @seq_callback_comparator)

    ret ptr %stackN
}
```

## Limitations

### Single Callback Per Type (Initial)

The global slot approach means only one quotation can be bound to each callback type at a time. This is fine for:
- qsort (callback only used during the call)
- readline completion (one completer active)

For concurrent/nested callbacks, we'd need:
- Thread-local slots
- Or closure-based trampolines (Phase 4+)

### No Closure Support (Initial)

Closures with captured environments need additional machinery:
- Pass environment pointer somehow (userdata?)
- Or use libffi for dynamic closure creation

Initial implementation: quotations only, no captures.

### Callback Lifetime

Callbacks are only valid during the C function call. The trampoline references global state that may be overwritten.

## Implementation Phases

### Phase 3a: Basic Callbacks
- Manifest parser for `[[library.callback]]`
- Trampoline codegen for simple signatures
- Callback binding in FFI wrappers
- Test with qsort-style comparators

### Phase 3b: Complex Return Types
- String returns (allocation strategy)
- Array returns (readline completion style)
- Struct returns

### Phase 3c: Userdata Pattern
- Pass opaque pointer through C API
- Retrieve in callback to access closure environment
- Enables stateful callbacks

## Example: readline Completion

```toml
[[library.callback]]
name = "completion_func"
args = [
  { type = "string", name = "text" },
  { type = "int", name = "start" },
  { type = "int", name = "end" }
]
return = { type = "string_array", ownership = "caller_frees" }
seq_effect = "( String Int Int -- StringArray )"

[[library.function]]
c_name = "rl_completion_matches"
seq_name = "completion-matches"
stack_effect = "( String [String Int Int -- StringArray] -- StringArray )"
args = [
  { type = "string", pass = "c_string" },
  { type = "callback", callback = "completion_func" }
]
return = { type = "string_array" }
```

## Example: SQLite Callback

```toml
[[library.callback]]
name = "exec_callback"
args = [
  { type = "ptr", name = "userdata" },
  { type = "int", name = "argc" },
  { type = "string_array", name = "argv" },
  { type = "string_array", name = "col_names" }
]
return = { type = "int" }
seq_effect = "( Int Int StringArray StringArray -- Int )"

[[library.function]]
c_name = "sqlite3_exec"
seq_name = "db-exec-callback"
stack_effect = "( Int String [Int Int StringArray StringArray -- Int] -- Int )"
args = [
  { type = "ptr", pass = "ptr" },
  { type = "string", pass = "c_string" },
  { type = "callback", callback = "exec_callback" },
  { type = "ptr", value = "null" },  # userdata (unused initially)
  { type = "ptr", value = "null" }   # errmsg
]
return = { type = "int" }
```

## Open Questions

1. **Thread safety**: Should callback slots be thread-local?
2. **Reentrant callbacks**: What if C calls the callback recursively?
3. **Error handling**: What if the Seq callback panics?
4. **Stack allocation**: Fresh stack per callback or reuse?

## References

- [libffi](https://sourceware.org/libffi/) - dynamic callback creation
- [Lua C API](https://www.lua.org/manual/5.4/manual.html#4) - similar callback challenges
- [Python ctypes callbacks](https://docs.python.org/3/library/ctypes.html#callback-functions)
