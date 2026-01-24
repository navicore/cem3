# Migration Guide: 2.x to 3.0.0

This guide covers breaking changes in Seq 3.0.0 and how to update your code.

## Overview

Version 3.0.0 standardizes error handling across the language using the `(value Bool)` pattern. Operations that can fail now return a success flag instead of panicking or silently returning invalid values.

## Division Operations

Division and modulo now return `(Int Bool)` to handle division by zero.

### Before (2.x)
```seq
: divide-example ( -- )
  10 2 i./              # Returns 5 (or crashes on divide by zero)
  5 test.assert-eq
;
```

### After (3.0)
```seq
: divide-example ( -- )
  10 2 i./              # Returns (5, true)
  test.assert           # Check success
  5 test.assert-eq      # Check result
;

# Handle division by zero gracefully
: safe-divide ( Int Int -- Int )
  i./                   # ( result success )
  if
    # Success - result is valid
  else
    drop 0              # Failure - drop invalid result, use default
  then
;
```

**Affected words:** `i./`, `i.%`, `i.divide`, `i.modulo`

## TCP Operations

All TCP operations now return Bool for error handling.

### Before (2.x)
```seq
: server ( -- )
  8080 tcp.listen       # Returns listener_id (panics on error)
  accept-loop
;

: handle-client ( Int -- )
  dup tcp.read          # Returns string (panics on error)
  process-request
  over tcp.write        # No return value
  tcp.close             # No return value
;
```

### After (3.0)
```seq
: server ( -- )
  8080 tcp.listen       # Returns (listener_id, success)
  not if
    drop "Failed to bind" io.write-line
  else
    accept-loop
  then
;

: handle-client ( Int -- )
  dup tcp.read          # Returns (string, success)
  not if
    drop tcp.close drop
  else
    process-request
    over tcp.write drop # Returns success (drop it)
    tcp.close drop      # Returns success (drop it)
  then
;
```

**Affected words:**
| Word | Old Signature | New Signature |
|------|---------------|---------------|
| `tcp.listen` | `( Int -- Int )` | `( Int -- Int Bool )` |
| `tcp.accept` | `( Int -- Int )` | `( Int -- Int Bool )` |
| `tcp.read` | `( Int -- String )` | `( Int -- String Bool )` |
| `tcp.write` | `( String Int -- )` | `( String Int -- Bool )` |
| `tcp.close` | `( Int -- )` | `( Int -- Bool )` |

## Regex Operations

Several regex operations now return Bool to indicate invalid regex patterns.

### Before (2.x)
```seq
: find-numbers ( String -- )
  "[0-9]+" regex.find-all    # Returns list (empty on invalid regex)
  list.length int->string io.write-line
;

: clean-whitespace ( String -- String )
  "\\s+" " " regex.replace-all  # Returns string
;
```

### After (3.0)
```seq
: find-numbers ( String -- )
  "[0-9]+" regex.find-all    # Returns (list, success)
  not if
    drop "Invalid regex" io.write-line
  else
    list.length int->string io.write-line
  then
;

: clean-whitespace ( String -- String )
  "\\s+" " " regex.replace-all  # Returns (string, success)
  drop                          # Drop success if you don't need it
;
```

**Affected words:**
| Word | Old Signature | New Signature |
|------|---------------|---------------|
| `regex.find-all` | `( String String -- List )` | `( String String -- List Bool )` |
| `regex.replace` | `( String String String -- String )` | `( String String String -- String Bool )` |
| `regex.replace-all` | `( String String String -- String )` | `( String String String -- String Bool )` |
| `regex.split` | `( String String -- List )` | `( String String -- List Bool )` |

**Unchanged:** `regex.match?`, `regex.find`, `regex.captures`, `regex.valid?` (already returned Bool)

## std:result Removed

The `std:result` module has been removed from the standard library. The `(value Bool)` pattern is now the standard way to handle errors in Seq.

### Before (2.x)
```seq
include std:result

: parse-number ( String -- Int )
  string->int
  result-unwrap          # This didn't actually work correctly
;
```

### After (3.0)
```seq
# No include needed - use (value Bool) directly

: parse-number ( String -- Int Bool )
  string->int            # Already returns (Int, Bool)
;

: parse-or-default ( String Int -- Int )
  swap string->int       # ( default value success )
  if nip else drop then  # Keep value on success, default on failure
;
```

If you need Result-like types for specific use cases, see `examples/paradigms/functional/result.seq` for a pattern you can adapt.

## std:imath mod

The `mod` function in std:imath now returns `(Int Bool)` since it wraps `i.modulo`.

### Before (2.x)
```seq
include std:imath

: is-even ( Int -- Bool )
  2 mod 0 i.=
;
```

### After (3.0)
```seq
include std:imath

: is-even ( Int -- Bool )
  2 mod              # Returns (remainder, success)
  drop               # Drop success (2 is never zero)
  0 i.=
;
```

## Quick Reference

Common pattern for handling `(value Bool)` returns:

```seq
# Check and use
operation
if
  # success - value is valid, use it
else
  drop  # failure - drop invalid value
then

# Ignore errors (when you know it won't fail)
operation drop  # Just drop the Bool

# Propagate errors
: my-word ( ... -- ... Bool )
  operation         # ( value success )
  not if
    drop false      # Propagate failure
  else
    # process value
    true            # Return success
  then
;
```

## Need Help?

If you encounter issues migrating, please open an issue at https://github.com/navicore/patch-seq/issues
