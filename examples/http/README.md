# HTTP Server Example for Seq

A complete concurrent HTTP server demonstrating Seq's capabilities:
- TCP socket operations
- Concurrent request handling with strands (green threads)
- Channel-based communication (CSP)
- Closure capture for spawned workers
- HTTP routing with pattern matching

## Prerequisites

Build the compiler:
```bash
cargo build --release
```

## Running the Server

```bash
./target/release/seqc --output /tmp/http_server examples/http/http_server.seq
/tmp/http_server
```

The server listens on port 8080 and handles multiple concurrent connections.

## Testing

In another terminal:

```bash
# Test root endpoint
curl http://localhost:8080/
# Output: Hello from Seq!

# Test health endpoint
curl http://localhost:8080/health
# Output: OK

# Test echo endpoint
curl http://localhost:8080/echo
# Output: Echo!

# Test 404 handling
curl http://localhost:8080/invalid
# Output: 404 Not Found
```

## How It Works

The server demonstrates several Seq features:

1. **TCP Operations**: `tcp-listen`, `tcp-accept`, `tcp-read`, `tcp-write`, `tcp-close`
2. **Routing**: Uses `cond` combinator for multi-way branching on request paths
3. **Concurrency**: Each connection is handled in a separate strand (green thread)
4. **Channels**: Spawned workers receive socket IDs via channels
5. **Closures**: The `[ worker ]` quotation captures the channel ID when spawned

### Architecture

```
main
  ├─ tcp-listen (creates listener socket)
  └─ accept-loop (infinite)
       ├─ tcp-accept (waits for connection)
       ├─ make-channel (creates communication channel)
       ├─ spawn [ worker ] (launches handler strand with channel)
       └─ send (passes socket ID to worker via channel)

worker strand
  ├─ receive (gets socket ID from channel)
  └─ handle-connection
       ├─ tcp-read (reads HTTP request)
       ├─ route (pattern matches to response)
       ├─ tcp-write (sends HTTP response)
       └─ tcp-close (cleanup)
```

## Key Features

**Non-blocking I/O**: All TCP operations cooperate with May's coroutine scheduler, yielding instead of blocking OS threads.

**Efficient Concurrency**: The server can handle thousands of concurrent connections using lightweight strands.

**Stack-based Routing**: HTTP routing is implemented using Seq's `cond` combinator, demonstrating clean concatenative style.

## Next Steps

This example serves as a foundation for:
- RESTful APIs with JSON
- WebSocket servers
- HTTP client implementations
- More sophisticated routing (path parameters, query strings)

## References

- [Seq Roadmap](../../docs/ROADMAP.md)
- [Concatenative Design](../../docs/CLEAN_CONCATENATIVE_DESIGN.md)
