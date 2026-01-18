# Seq Standard Library Reference

Complete reference for all 152 built-in operations.

## Table of Contents

- [I/O Operations](#io-operations)
- [Command-line Arguments](#command-line-arguments)
- [File Operations](#file-operations)
- [Type Conversions](#type-conversions)
- [Integer Arithmetic](#integer-arithmetic)
- [Integer Comparison](#integer-comparison)
- [Boolean Operations](#boolean-operations)
- [Bitwise Operations](#bitwise-operations)
- [Stack Operations](#stack-operations)
- [Control Flow](#control-flow)
- [Concurrency](#concurrency)
- [Channel Operations](#channel-operations)
- [TCP Operations](#tcp-operations)
- [OS Operations](#os-operations)
- [Terminal Operations](#terminal-operations)
- [String Operations](#string-operations)
- [Encoding Operations](#encoding-operations)
- [Crypto Operations](#crypto-operations)
- [HTTP Client](#http-client)
- [Regular Expressions](#regular-expressions)
- [Compression](#compression)
- [Variant Operations](#variant-operations)
- [List Operations](#list-operations)
- [Map Operations](#map-operations)
- [Float Arithmetic](#float-arithmetic)
- [Float Comparison](#float-comparison)
- [Test Framework](#test-framework)
- [Time Operations](#time-operations)
- [Serialization](#serialization)
- [Stack Introspection](#stack-introspection)

---

## I/O Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `io.write` | `( String -- )` | Write string to stdout without newline |
| `io.write-line` | `( String -- )` | Write string to stdout with newline |
| `io.read-line` | `( -- String Bool )` | Read line from stdin. Returns (line, success) |
| `io.read-line+` | `( -- String Int )` | Read line from stdin. Returns (line, status_code) |
| `io.read-n` | `( Int -- String Int )` | Read N bytes from stdin. Returns (bytes, status) |

## Command-line Arguments

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `args.count` | `( -- Int )` | Get number of command-line arguments |
| `args.at` | `( Int -- String )` | Get argument at index N |

## File Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `file.slurp` | `( String -- String Bool )` | Read entire file. Returns (content, success) |
| `file.exists?` | `( String -- Bool )` | Check if file exists at path |
| `file.for-each-line+` | `( String [String --] -- String Bool )` | Execute quotation for each line in file |

## Type Conversions

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `int->string` | `( Int -- String )` | Convert integer to string |
| `int->float` | `( Int -- Float )` | Convert integer to float |
| `float->int` | `( Float -- Int )` | Truncate float to integer |
| `float->string` | `( Float -- String )` | Convert float to string |
| `string->int` | `( String -- Int Bool )` | Parse string as integer. Returns (value, success) |
| `string->float` | `( String -- Float Bool )` | Parse string as float. Returns (value, success) |
| `char->string` | `( Int -- String )` | Convert Unicode codepoint to single-char string |
| `symbol->string` | `( Symbol -- String )` | Convert symbol to string |
| `string->symbol` | `( String -- Symbol )` | Intern string as symbol |

## Integer Arithmetic

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `i.add` / `i.+` | `( Int Int -- Int )` | Add two integers |
| `i.subtract` / `i.-` | `( Int Int -- Int )` | Subtract second from first |
| `i.multiply` / `i.*` | `( Int Int -- Int )` | Multiply two integers |
| `i.divide` / `i./` | `( Int Int -- Int )` | Integer division (truncates toward zero) |
| `i.modulo` / `i.%` | `( Int Int -- Int )` | Integer modulo (remainder) |

## Integer Comparison

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `i.=` / `i.eq` | `( Int Int -- Bool )` | Test equality |
| `i.<` / `i.lt` | `( Int Int -- Bool )` | Test less than |
| `i.>` / `i.gt` | `( Int Int -- Bool )` | Test greater than |
| `i.<=` / `i.lte` | `( Int Int -- Bool )` | Test less than or equal |
| `i.>=` / `i.gte` | `( Int Int -- Bool )` | Test greater than or equal |
| `i.<>` / `i.neq` | `( Int Int -- Bool )` | Test not equal |

## Boolean Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `and` | `( Bool Bool -- Bool )` | Logical AND |
| `or` | `( Bool Bool -- Bool )` | Logical OR |
| `not` | `( Bool -- Bool )` | Logical NOT |

## Bitwise Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `band` | `( Int Int -- Int )` | Bitwise AND |
| `bor` | `( Int Int -- Int )` | Bitwise OR |
| `bxor` | `( Int Int -- Int )` | Bitwise XOR |
| `bnot` | `( Int -- Int )` | Bitwise NOT (complement) |
| `shl` | `( Int Int -- Int )` | Shift left by N bits |
| `shr` | `( Int Int -- Int )` | Shift right by N bits (logical) |
| `popcount` | `( Int -- Int )` | Count number of set bits |
| `clz` | `( Int -- Int )` | Count leading zeros |
| `ctz` | `( Int -- Int )` | Count trailing zeros |
| `int-bits` | `( -- Int )` | Push bit width of integers (64) |

## Stack Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `dup` | `( T -- T T )` | Duplicate top value |
| `drop` | `( T -- )` | Remove top value |
| `swap` | `( T U -- U T )` | Swap top two values |
| `over` | `( T U -- T U T )` | Copy second value to top |
| `rot` | `( T U V -- U V T )` | Rotate third to top |
| `nip` | `( T U -- U )` | Remove second value |
| `tuck` | `( T U -- U T U )` | Copy top below second |
| `2dup` | `( T U -- T U T U )` | Duplicate top two values |
| `3drop` | `( T U V -- )` | Remove top three values |
| `pick` | `( T Int -- T T )` | Copy value at depth N to top |
| `roll` | `( T Int -- T )` | Rotate N+1 items, bringing depth N to top |

## Control Flow

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `call` | `( Quotation -- ... )` | Call a quotation or closure |
| `cond` | `( ... Int -- ... )` | Multi-way conditional |
| `times` | `( [--] Int -- )` | Execute quotation N times |
| `while` | `( [-- Bool] [--] -- )` | Loop while condition is true |
| `until` | `( [--] [-- Bool] -- )` | Loop until condition is true |

## Concurrency

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `strand.spawn` | `( Quotation -- Int )` | Spawn concurrent strand. Returns strand ID |
| `strand.weave` | `( Quotation -- handle )` | Create generator/coroutine. Returns handle |
| `strand.resume` | `( handle T -- handle T Bool )` | Resume weave with value. Returns (handle, value, has_more) |
| `yield` | `( ctx T -- ctx T )` | Yield value from weave and receive resume value |
| `strand.weave-cancel` | `( handle -- )` | Cancel weave and release resources |

## Channel Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `chan.make` | `( -- Channel )` | Create new channel |
| `chan.send` | `( T Channel -- Bool )` | Send value on channel. Returns success |
| `chan.receive` | `( Channel -- T Bool )` | Receive from channel. Returns (value, success) |
| `chan.close` | `( Channel -- )` | Close channel |
| `chan.yield` | `( -- )` | Yield control to scheduler |

## TCP Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `tcp.listen` | `( Int -- Int )` | Listen on port. Returns socket ID |
| `tcp.accept` | `( Int -- Int )` | Accept connection. Returns client socket |
| `tcp.read` | `( Int -- String )` | Read from socket |
| `tcp.write` | `( String Int -- )` | Write to socket |
| `tcp.close` | `( Int -- )` | Close socket |

## OS Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `os.getenv` | `( String -- String Bool )` | Get env variable. Returns (value, exists) |
| `os.home-dir` | `( -- String Bool )` | Get home directory. Returns (path, success) |
| `os.current-dir` | `( -- String Bool )` | Get current directory. Returns (path, success) |
| `os.path-exists` | `( String -- Bool )` | Check if path exists |
| `os.path-is-file` | `( String -- Bool )` | Check if path is regular file |
| `os.path-is-dir` | `( String -- Bool )` | Check if path is directory |
| `os.path-join` | `( String String -- String )` | Join two path components |
| `os.path-parent` | `( String -- String Bool )` | Get parent directory. Returns (path, success) |
| `os.path-filename` | `( String -- String Bool )` | Get filename. Returns (name, success) |
| `os.exit` | `( Int -- )` | Exit program with status code |
| `os.name` | `( -- String )` | Get OS name (e.g., "macos", "linux") |
| `os.arch` | `( -- String )` | Get CPU architecture (e.g., "aarch64", "x86_64") |

## Terminal Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `terminal.raw-mode` | `( Bool -- )` | Enable/disable raw mode. Raw: no buffering, no echo, Ctrl+C = byte 3 |
| `terminal.read-char` | `( -- Int )` | Read single byte (blocking). Returns 0-255 or -1 on EOF |
| `terminal.read-char?` | `( -- Int )` | Read single byte (non-blocking). Returns 0-255 or -1 if none |
| `terminal.width` | `( -- Int )` | Get terminal width in columns. Returns 80 if unknown |
| `terminal.height` | `( -- Int )` | Get terminal height in rows. Returns 24 if unknown |
| `terminal.flush` | `( -- )` | Flush stdout |

## String Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `string.concat` | `( String String -- String )` | Concatenate two strings |
| `string.length` | `( String -- Int )` | Get character length |
| `string.byte-length` | `( String -- Int )` | Get byte length |
| `string.char-at` | `( String Int -- Int )` | Get Unicode codepoint at index |
| `string.substring` | `( String Int Int -- String )` | Extract substring (start, length) |
| `string.find` | `( String String -- Int )` | Find substring. Returns index or -1 |
| `string.split` | `( String String -- List )` | Split by delimiter |
| `string.contains` | `( String String -- Bool )` | Check if contains substring |
| `string.starts-with` | `( String String -- Bool )` | Check if starts with prefix |
| `string.empty?` | `( String -- Bool )` | Check if empty |
| `string.equal?` | `( String String -- Bool )` | Check equality |
| `string.trim` | `( String -- String )` | Remove leading/trailing whitespace |
| `string.chomp` | `( String -- String )` | Remove trailing newline |
| `string.to-upper` | `( String -- String )` | Convert to uppercase |
| `string.to-lower` | `( String -- String )` | Convert to lowercase |
| `string.json-escape` | `( String -- String )` | Escape for JSON |
| `symbol.=` | `( Symbol Symbol -- Bool )` | Check symbol equality |

## Encoding Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `encoding.base64-encode` | `( String -- String )` | Encode to Base64 (standard, with padding) |
| `encoding.base64-decode` | `( String -- String Bool )` | Decode Base64. Returns (decoded, success) |
| `encoding.base64url-encode` | `( String -- String )` | Encode to URL-safe Base64 (no padding) |
| `encoding.base64url-decode` | `( String -- String Bool )` | Decode URL-safe Base64 |
| `encoding.hex-encode` | `( String -- String )` | Encode to lowercase hex |
| `encoding.hex-decode` | `( String -- String Bool )` | Decode hex string |

## Crypto Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `crypto.sha256` | `( String -- String )` | SHA-256 hash. Returns 64-char hex |
| `crypto.hmac-sha256` | `( String String -- String )` | HMAC-SHA256. (message, key) |
| `crypto.constant-time-eq` | `( String String -- Bool )` | Timing-safe comparison |
| `crypto.random-bytes` | `( Int -- String )` | Generate N random bytes as hex |
| `crypto.random-int` | `( Int Int -- Int )` | Uniform random in [min, max). Rejection sampling |
| `crypto.uuid4` | `( -- String )` | Generate random UUID v4 |
| `crypto.aes-gcm-encrypt` | `( String String -- String Bool )` | AES-256-GCM encrypt. (plaintext, hex-key) |
| `crypto.aes-gcm-decrypt` | `( String String -- String Bool )` | AES-256-GCM decrypt. (ciphertext, hex-key) |
| `crypto.pbkdf2-sha256` | `( String String Int -- String Bool )` | Derive key. (password, salt, iterations) |
| `crypto.ed25519-keypair` | `( -- String String )` | Generate keypair. Returns (public, private) |
| `crypto.ed25519-sign` | `( String String -- String Bool )` | Sign message. (message, private-key) |
| `crypto.ed25519-verify` | `( String String String -- Bool )` | Verify signature. (message, signature, public-key) |

## HTTP Client

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `http.get` | `( String -- Map )` | GET request. Map has status, body, ok, error |
| `http.post` | `( String String String -- Map )` | POST request. (url, body, content-type) |
| `http.put` | `( String String String -- Map )` | PUT request. (url, body, content-type) |
| `http.delete` | `( String -- Map )` | DELETE request |

## Regular Expressions

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `regex.match?` | `( String String -- Bool )` | Check if pattern matches. (text, pattern) |
| `regex.find` | `( String String -- String Bool )` | Find first match |
| `regex.find-all` | `( String String -- List )` | Find all matches |
| `regex.replace` | `( String String String -- String )` | Replace first match. (text, pattern, replacement) |
| `regex.replace-all` | `( String String String -- String )` | Replace all matches |
| `regex.captures` | `( String String -- List Bool )` | Extract capture groups |
| `regex.split` | `( String String -- List )` | Split by pattern |
| `regex.valid?` | `( String -- Bool )` | Check if valid regex |

## Compression

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `compress.gzip` | `( String -- String Bool )` | Gzip compress. Returns base64-encoded |
| `compress.gzip-level` | `( String Int -- String Bool )` | Gzip at level 1-9 |
| `compress.gunzip` | `( String -- String Bool )` | Gzip decompress |
| `compress.zstd` | `( String -- String Bool )` | Zstd compress. Returns base64-encoded |
| `compress.zstd-level` | `( String Int -- String Bool )` | Zstd at level 1-22 |
| `compress.unzstd` | `( String -- String Bool )` | Zstd decompress |

## Variant Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `variant.field-count` | `( Variant -- Int )` | Get number of fields |
| `variant.tag` | `( Variant -- Symbol )` | Get tag (constructor name) |
| `variant.field-at` | `( Variant Int -- T )` | Get field at index |
| `variant.append` | `( Variant T -- Variant )` | Append value to variant |
| `variant.last` | `( Variant -- T )` | Get last field |
| `variant.init` | `( Variant -- Variant )` | Get all fields except last |
| `variant.make-0` / `wrap-0` | `( Symbol -- Variant )` | Create variant with 0 fields |
| `variant.make-1` / `wrap-1` | `( T Symbol -- Variant )` | Create variant with 1 field |
| `variant.make-2` / `wrap-2` | `( T T Symbol -- Variant )` | Create variant with 2 fields |
| `variant.make-3` / `wrap-3` | `( T T T Symbol -- Variant )` | Create variant with 3 fields |
| `variant.make-4` / `wrap-4` | `( T T T T Symbol -- Variant )` | Create variant with 4 fields |

## List Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `list.make` | `( -- List )` | Create empty list |
| `list.push` | `( List T -- List )` | Push value onto list |
| `list.get` | `( List Int -- T Bool )` | Get value at index. Returns (value, success) |
| `list.set` | `( List Int T -- List Bool )` | Set value at index. Returns (list, success) |
| `list.length` | `( List -- Int )` | Get number of elements |
| `list.empty?` | `( List -- Bool )` | Check if empty |
| `list.map` | `( List [T -- U] -- List )` | Apply quotation to each element |
| `list.filter` | `( List [T -- Bool] -- List )` | Keep elements where quotation returns true |
| `list.fold` | `( List Acc [Acc T -- Acc] -- Acc )` | Reduce with accumulator |
| `list.each` | `( List [T --] -- )` | Execute quotation for each element |

## Map Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `map.make` | `( -- Map )` | Create empty map |
| `map.get` | `( Map K -- V Bool )` | Get value for key. Returns (value, success) |
| `map.set` | `( Map K V -- Map )` | Set key to value |
| `map.has?` | `( Map K -- Bool )` | Check if key exists |
| `map.remove` | `( Map K -- Map )` | Remove key |
| `map.keys` | `( Map -- List )` | Get all keys |
| `map.values` | `( Map -- List )` | Get all values |
| `map.size` | `( Map -- Int )` | Get number of entries |
| `map.empty?` | `( Map -- Bool )` | Check if empty |

## Float Arithmetic

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.add` / `f.+` | `( Float Float -- Float )` | Add two floats |
| `f.subtract` / `f.-` | `( Float Float -- Float )` | Subtract second from first |
| `f.multiply` / `f.*` | `( Float Float -- Float )` | Multiply two floats |
| `f.divide` / `f./` | `( Float Float -- Float )` | Divide first by second |

## Float Comparison

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `f.=` / `f.eq` | `( Float Float -- Bool )` | Test equality |
| `f.<` / `f.lt` | `( Float Float -- Bool )` | Test less than |
| `f.>` / `f.gt` | `( Float Float -- Bool )` | Test greater than |
| `f.<=` / `f.lte` | `( Float Float -- Bool )` | Test less than or equal |
| `f.>=` / `f.gte` | `( Float Float -- Bool )` | Test greater than or equal |
| `f.<>` / `f.neq` | `( Float Float -- Bool )` | Test not equal |

## Test Framework

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `test.init` | `( String -- )` | Initialize with test name |
| `test.finish` | `( -- )` | Finish and print results |
| `test.has-failures` | `( -- Bool )` | Check if any tests failed |
| `test.assert` | `( Bool -- )` | Assert boolean is true |
| `test.assert-not` | `( Bool -- )` | Assert boolean is false |
| `test.assert-eq` | `( Int Int -- )` | Assert two integers equal |
| `test.assert-eq-str` | `( String String -- )` | Assert two strings equal |
| `test.fail` | `( String -- )` | Mark test as failed with message |
| `test.pass-count` | `( -- Int )` | Get passed assertion count |
| `test.fail-count` | `( -- Int )` | Get failed assertion count |

## Time Operations

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `time.now` | `( -- Int )` | Current Unix timestamp in seconds |
| `time.nanos` | `( -- Int )` | High-resolution monotonic time in nanoseconds |
| `time.sleep-ms` | `( Int -- )` | Sleep for N milliseconds |

## Serialization

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `son.dump` | `( T -- String )` | Serialize value to SON format (compact) |
| `son.dump-pretty` | `( T -- String )` | Serialize value to SON format (pretty) |

## Stack Introspection

| Word | Stack Effect | Description |
|------|--------------|-------------|
| `stack.dump` | `( ... -- )` | Print all stack values and clear (REPL) |

---

*Generated from builtins.rs - 152 operations total*
