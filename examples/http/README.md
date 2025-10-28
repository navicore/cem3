# HTTP Examples for cem3

This directory contains a tutorial series demonstrating HTTP routing and server
capabilities in cem3.

## Prerequisites

Build the compiler: ```bash cargo build --release ```

## Examples (in order of complexity)

### 1. `01_simple_router.cem` - Basic HTTP Routing

Demonstrates:
- Using the `cond` combinator for multi-way branching
- HTTP request pattern matching with `string-starts-with`
- Composing words with proper type signatures
- Row polymorphism in action

**Concepts:**
- Type signature `( String -- String )` is implicitly row-polymorphic
- The `cond` combinator takes N pairs of condition/action quotations plus a
  count
- Each condition quotation should return an Int (0=false, non-zero=true)

**Run it:** ```bash ./target/release/cem3 --output tmp/01_simple_router
examples/http/01_simple_router.cem ./tmp/01_simple_router ```

**Expected output:** ``` HTTP/1.1 200 OK Content-Type: text/plain

Welcome to cem3! HTTP/1.1 200 OK Content-Type: application/json

{"status":"healthy"} HTTP/1.1 404 Not Found Content-Type: text/plain

404 Not Found ```

---

### 2. `02_router_with_helpers.cem` - Modular Routing

Demonstrates:
- Breaking routing logic into helper words
- Forward references (using words before they're defined in quotations)
- Building response strings with proper headers
- Type-safe composition of multiple words

**Concepts:**
- Helper words can be called from quotations before their definition
- Type checker validates the entire call chain
- Proper HTTP response format with headers

**Run it:** ```bash ./target/release/cem3 --output tmp/02_router_with_helpers
examples/http/02_router_with_helpers.cem ./tmp/02_router_with_helpers ```

---

### 3. `03_echo_server.cem` - TCP Echo Server (no spawn)

Demonstrates:
- Basic TCP operations: `tcp-listen`, `tcp-accept`, `tcp-read`, `tcp-write`,
  `tcp-close`
- Single-threaded server handling one connection
- Real network I/O

**Concepts:**
- `tcp-listen` returns a listener socket (Int)
- `tcp-accept` blocks until connection arrives, returns connection socket
- `tcp-read` and `tcp-write` operate on connection sockets
- `tcp-close` cleans up connection

**Run it:** ```bash ./target/release/cem3 --output tmp/03_echo_server
examples/http/03_echo_server.cem ./tmp/03_echo_server &

# In another terminal: curl http://localhost:8080/ curl
http://localhost:8080/health ```

**Current limitation:** Only handles one connection then exits. Full
multi-connection server requires `spawn` with data passing (see below).

---

## Current Limitations and Future Work

### spawn with Data Passing

The current `spawn` builtin has signature: ``` spawn: ( ..a [ -- ] -- ..a Int )
```

This means the spawned quotation must have effect `( -- )` - it can't receive
data from the stack.

**Example that doesn't work yet:** ```cem : handle-connection ( Int -- ) dup
tcp-read route swap tcp-write tcp-close ;

: accept-loop ( Int -- ) [ 1 ] [ dup tcp-accept [ handle-connection ] spawn  #
ERROR: handle-connection needs Int! drop ] until ; ```

**Planned solutions:**

1. **Add `curry` operation** (like Factor): ```cem dup tcp-accept [
handle-connection ] curry spawn  # Captures Int in closure ```

2. **Generalize spawn signature**: ``` spawn: ( ..a ..b [ ..b -- ] -- ..a Int )
``` This would allow spawn to consume stack values and pass them to the new
strand.

3. **Use channels for data passing**: ```cem make-channel dup tcp-accept swap [
receive handle-connection ] spawn send ```

## Syntax Notes

### Comments Comments start with `#` and continue to the end of the line.
**Important:** Comments must appear on their own line, not inline with code:

**Good:** ```cem # This is a comment : route ( String -- String ) [ dup "GET / "
string-starts-with ] [ drop "OK" ] 2 cond ; ```

**Bad (will cause parse errors):** ```cem : route ( String -- String ) [ dup
"GET / " string-starts-with ]  # This inline comment breaks! [ drop "OK" ] 2
cond ; ```

### Type System - Row Polymorphism

All examples demonstrate **implicit row polymorphism**. When you write: ```cem :
route ( String -- String ) ... ; ```

The compiler interprets this as: ```cem : route ( ..rest String -- ..rest String
) ```

This allows `route` to be called when there are other values below the String on
the stack. This is essential for word composition in concatenative languages!

## Next Steps

Once spawn with data passing is implemented, we'll add:
- `04_concurrent_server.cem` - Multi-connection HTTP server
- `05_http_client.cem` - Making HTTP requests
- `06_rest_api.cem` - RESTful API with state management

## References

- [cem3 Roadmap](../../docs/ROADMAP.md)
- [Type System Assessment](../../tmp/TYPE_SYSTEM_ASSESSMENT.md)
- [Type System Fix Results](../../tmp/TYPE_SYSTEM_FIX_RESULTS.md)
