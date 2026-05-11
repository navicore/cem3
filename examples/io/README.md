# Input/Output

Networking, file I/O, terminal, and text processing.

## HTTP Server (http/)

**http_server.seq** — TCP server with HTTP routing. Uses the `net.tcp.*`
builtins for the transport layer and `std:http` for response/parsing
helpers:

```seq
include std:http        # http-ok / http-request-path / etc.

: handle-request ( Socket -- )
  dup net.tcp.read drop          # ( socket request )
  http-request-path              # ( socket path )
  "/" string.equal? [
    drop "Hello from Seq!" http-ok
  ] [
    drop "Not Found" http-not-found
  ] if
  over net.tcp.write drop
  net.tcp.close drop ;
```

**test_simple.seq** — Basic HTTP request/response testing.

## HTTP Client (http-client.seq)

Making HTTP requests with the built-in `net.http.*` words. (No `include`
required — `net.http.*` is a builtin, not part of `std:http`.)

```seq
"https://api.example.com/data" net.http.get
"body" map.get drop io.write-line
```

## Terminal (terminal/)

**terminal-demo.seq** - Terminal colors, cursor control, and formatting using ANSI escape sequences.

## Operating System (os/)

**os-demo.seq** - Environment variables, paths, and system information.

## Text Processing (text/)

**log-parser.seq** - Parsing structured log files with string operations.

**regex-demo.seq** - Regular expression matching and extraction.

## Compression (compress-demo.seq)

Zstd compression and decompression for efficient data storage.
