# Testing Guide

Seq includes a built-in test framework for writing and running tests. Tests are discovered automatically and run with `seqc test`.

## Quick Start

Create a file named `test-math.seq`:

```seq
: test-addition ( -- )
  "Addition" test.init
  1 2 i.+ 3 test.assert-eq
  test.finish
;

: test-multiplication ( -- )
  "Multiplication" test.init
  3 4 i.* 12 test.assert-eq
  test.finish
;
```

Run tests:

```bash
seqc test
```

Output:
```
test-math.seq
  test-addition ... ok
  test-multiplication ... ok

2 tests passed, 0 failed
```

## Test Discovery

The test runner uses two naming conventions plus a signature check:

1. **Test files**: Files named `test-*.seq` are discovered automatically.
2. **Test functions**: Words named `test-*` are run as tests **only when
   their declared stack effect is exactly `( -- )`**. A `test-*` word
   with a different signature (e.g. a `test-flag ( Int Int -- Bool )`
   helper) is skipped, not promoted to an entry point.

```
myproject/
  src/
    parser.seq
    eval.seq
  tests/
    test-parser.seq    # Discovered
    test-eval.seq      # Discovered
    helpers.seq        # NOT discovered (no test- prefix)
```

Run tests in a directory:

```bash
seqc test tests/           # Run all test-*.seq files in tests/
seqc test test-parser.seq  # Run specific file
seqc test .                # Run all tests in current directory (recursive)
```

If you pass an explicit file path that doesn't match `test-*.seq`, the
runner errors instead of silently producing zero results:

```bash
$ seqc test tests/parser.seq
Test files must be named `test-*.seq`. Got: `tests/parser.seq`
```

### Why `test-` predicates are safe

Inside a `test-*.seq` file, you can still define helpers whose names
start with `test-` — for example, predicate-style words ending in `?`
or domain probes that take arguments. The signature filter excludes any
`test-*` word that doesn't match `( -- )`, so it never gets called with
an empty stack, and the runner prints a one-line note explaining why it
was skipped:

```
test-flag ... skipped — name starts with `test-` but stack effect is
( Int Int -- Bool ), not ( -- ). Rename if it's a helper; fix the
signature if it's a test.
```

The note distinguishes "you have a helper that happens to start with
`test-`" from "your test silently disappeared" — if you wrote what you
thought was a test and see this skip note, fix the signature.

## Test Framework Builtins

| Word | Effect | Description |
|------|--------|-------------|
| `test.init` | `( String -- )` | Initialize test with a name |
| `test.finish` | `( -- )` | Complete test and report results |
| `test.assert` | `( Bool -- )` | Assert condition is true |
| `test.assert-not` | `( Bool -- )` | Assert condition is false |
| `test.assert-eq` | `( Int Int -- )` | Assert two integers are equal |
| `test.assert-eq-str` | `( String String -- )` | Assert two strings are equal |
| `test.fail` | `( String -- )` | Explicitly fail with message |
| `test.pass-count` | `( -- Int )` | Get number of passed assertions |
| `test.fail-count` | `( -- Int )` | Get number of failed assertions |
| `test.has-failures` | `( -- Bool )` | Check if any assertions failed |

## Writing Tests

### Basic Structure

Every test function should:
1. Call `test.init` with a descriptive name
2. Run assertions
3. Call `test.finish`

```seq
: test-string-operations ( -- )
  "String operations" test.init

  # Test concatenation
  "hello" " " string.concat "world" string.concat
  "hello world" test.assert-eq-str

  # Test length
  "abc" string.length 3 test.assert-eq

  # Test empty check
  "" string.empty? test.assert
  "x" string.empty? test.assert-not

  test.finish
;
```

### Testing with Setup

For tests needing setup, extract helpers:

```seq
: make-test-list ( -- List )
  list.make
  1 list.push
  2 list.push
  3 list.push
;

: test-list-length ( -- )
  "List length" test.init
  make-test-list list.length 3 test.assert-eq
  test.finish
;

: test-list-sum ( -- )
  "List sum" test.init
  make-test-list 0 [ i.+ ] list.fold
  6 test.assert-eq
  test.finish
;
```

### Testing Error Cases

Use `test.fail` for cases that shouldn't be reached:

```seq
: test-option-handling ( -- )
  "Option handling" test.init

  Make-None match
    None -> "none handled" drop
    Some { >value } -> "Should not reach Some" test.fail
  end

  42 Make-Some match
    None -> "Should not reach None" test.fail
    Some { >value } -> value 42 test.assert-eq
  end

  test.finish
;
```

### Testing Stack Effects

Test that operations produce expected stack results:

```seq
: test-stack-ops ( -- )
  "Stack operations" test.init

  # Test dup
  5 dup
  5 test.assert-eq  # top should be 5
  5 test.assert-eq  # second should also be 5

  # Test swap
  1 2 swap
  1 test.assert-eq  # top should be 1
  2 test.assert-eq  # second should be 2

  test.finish
;
```

## Running Tests

### Basic Usage

```bash
seqc test                    # Run all test-*.seq in current directory
seqc test tests/             # Run all tests in tests/ directory
seqc test test-parser.seq    # Run specific test file
```

### Filtering Tests

Run only tests matching a pattern:

```bash
seqc test -f parse           # Run tests with "parse" in the name
seqc test -f test-add        # Run tests starting with "test-add"
```

### Verbose Output

See timing for each test:

```bash
seqc test -v
```

Output:
```
test-math.seq
  test-addition ... ok (2ms)
  test-multiplication ... ok (1ms)
  test-division ... ok (1ms)

3 tests passed, 0 failed (4ms total)
```

### Failure Output

When an assertion fails, the runner reports the source line, the
expected value, and the actual value on the stack:

```
tests/test-math.seq::test-addition
  test-addition ... FAILED
    at line 6: expected 8, got 13
```

Multiple failures within a single test each get their own line.
Tests that fire many assertions (e.g. loop-like comparisons over a
list) cap the output at the first five failures and append a
`+N more failures` footer so the real signal isn't buried:

```
tests/test-math.seq::test-many
  test-many ... FAILED
    at line 3: expected 1, got 99
    at line 4: expected 2, got 99
    at line 5: expected 3, got 99
    at line 6: expected 4, got 99
    at line 7: expected 5, got 99
    +2 more failures
```

## Standalone Test Files

If your test file has a `main` function, it runs as a standalone program instead of using the test runner:

```seq
# test-manual.seq - has main, runs standalone
: test-helper ( -- ) ... ;

: main ( -- )
  # Custom test harness
  "Running manual tests" io.write-line
  test-helper
  "All tests passed!" io.write-line
;
```

This is useful for tests requiring custom setup or integration tests.

## Best Practices

### 1. One Assertion Per Concept

Group related assertions, but keep tests focused:

```seq
# Good - focused test
: test-empty-list-length ( -- )
  "Empty list length" test.init
  list.make list.length 0 test.assert-eq
  test.finish
;

# Good - related assertions grouped
: test-list-push ( -- )
  "List push" test.init
  list.make
  1 list.push list.length 1 test.assert-eq
  2 list.push list.length 2 test.assert-eq
  test.finish
;
```

### 2. Descriptive Test Names

Use names that describe what's being tested:

```seq
: test-parser-handles-empty-input ( -- ) ... ;
: test-parser-rejects-invalid-syntax ( -- ) ... ;
: test-eval-arithmetic-precedence ( -- ) ... ;
```

### 3. Clean Up State

Tests run sequentially. Clean up any global effects:

```seq
: test-with-cleanup ( -- )
  "Test with cleanup" test.init
  # ... test code ...
  test.finish
  # Clean up any channels, files, etc.
;
```

### 4. Test Edge Cases

Cover boundaries and special cases:

```seq
: test-division-edge-cases ( -- )
  "Division edge cases" test.init
  0 5 i./ drop 0 test.assert-eq        # 0 / n = 0
  5 1 i./ drop 5 test.assert-eq        # n / 1 = n
  0 7 i.- 3 i./ drop 0 2 i.- test.assert-eq  # negative division
  test.finish
;
```

## Example: Testing a Parser

```seq
include "parser"

: test-parse-number ( -- )
  "Parse number" test.init
  "42" parse match
    ParseOk { >value } -> value 42 test.assert-eq
    ParseErr { >msg } -> msg test.fail
  end
  test.finish
;

: test-parse-invalid ( -- )
  "Parse invalid input" test.init
  "not-a-number" parse match
    ParseOk { >value } -> drop "Should have failed" test.fail
    ParseErr { >msg } -> drop  # Expected error
  end
  test.finish
;

: test-parse-empty ( -- )
  "Parse empty string" test.init
  "" parse match
    ParseOk { >value } -> drop "Should have failed" test.fail
    ParseErr { >msg } -> drop  # Expected error
  end
  test.finish
;
```

## See Also

- [examples/](https://github.com/navicore/patch-seq/tree/main/examples) - Many examples include tests
- [Standard Library](STDLIB_REFERENCE.md) - Full builtin reference
