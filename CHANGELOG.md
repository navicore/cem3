# Changelog

All notable changes to Seq will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Removed

- **Breaking:** Removed `times`, `while`, and `until` combinators ([#273](https://github.com/navicore/patch-seq/issues/273))

  These combinators required stack-neutral quotations (`[..a -- ..a]`), making them unsuitable for most real-world use cases. Use explicit recursion instead, which benefits from guaranteed tail call optimization.

  **Migration guide:**

  ```seq
  # Before: times
  [ "hello" io.write-line ] 3 times

  # After: recursion
  : say-hello ( Int -- )
    dup 0 i.> if
      "hello" io.write-line
      1 i.- say-hello
    else
      drop
    then
  ;
  3 say-hello
  ```

  ```seq
  # Before: while
  [ dup 0 i.> ] [ 1 i.- ] while

  # After: recursion
  : countdown ( Int -- Int )
    dup 0 i.> if
      1 i.- countdown
    then
  ;
  ```

  ```seq
  # Before: until
  [ 1 i.- ] [ dup 0 i.<= ] until

  # After: recursion
  : countdown ( Int -- Int )
    1 i.-
    dup 0 i.<= if
    else
      countdown
    then
  ;
  ```

### Added

- `crypto.random-int` for cryptographically secure random integers ([#275](https://github.com/navicore/patch-seq/issues/275))
- Hex escape sequences (`\xNN`) in string literals ([#279](https://github.com/navicore/patch-seq/issues/279))

## [1.0.6] - 2025-01-19

- Initial changelog entry (prior changes not documented here)
