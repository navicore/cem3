# Input/Output

Networking, file I/O, terminal, and text processing.

## HTTP Server (http/)

**http_server.seq** - TCP server with HTTP routing:

```seq
include std:http

: handle-request ( TcpStream -- )
  tcp.read-request
  request-path "/" string.equal? if
    "Hello from Seq!" 200 make-response
  else
    "Not Found" 404 make-response
  then
  tcp.write-response ;
```

**test_simple.seq** - Basic HTTP request/response testing.

## HTTP Client (http-client.seq)

Making HTTP requests using the std:http module:

```seq
include std:http

"https://api.example.com/data" http.get
http.body io.write-line
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
