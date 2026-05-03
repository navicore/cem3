use super::*;

#[test]
fn test_is_test_file() {
    let runner = TestRunner::new(false, None);
    assert!(runner.is_test_file(Path::new("test-foo.seq")));
    assert!(runner.is_test_file(Path::new("test-arithmetic.seq")));
    assert!(!runner.is_test_file(Path::new("foo.seq")));
    assert!(!runner.is_test_file(Path::new("test-foo.txt")));
    assert!(!runner.is_test_file(Path::new("my-test.seq")));
}

#[test]
fn test_discover_test_functions() {
    let runner = TestRunner::new(false, None);
    let source = r#"
: test-addition ( -- )
  2 3 add 5 test.assert-eq
;

: test-subtraction ( -- )
  5 3 subtract 2 test.assert-eq
;

: helper ( -- Int )
  42
;
"#;
    let (tests, skipped, has_main) = runner.discover_test_functions(source).unwrap();
    assert_eq!(tests.len(), 2);
    assert!(tests.contains(&"test-addition".to_string()));
    assert!(tests.contains(&"test-subtraction".to_string()));
    assert!(!tests.contains(&"helper".to_string()));
    assert!(skipped.is_empty(), "helper should not be in skip list");
    assert!(!has_main);
}

#[test]
fn test_discover_with_main() {
    let runner = TestRunner::new(false, None);
    let source = r#"
: test-foo ( -- ) ;
: main ( -- ) ;
"#;
    let (tests, _skipped, has_main) = runner.discover_test_functions(source).unwrap();
    assert_eq!(tests.len(), 1);
    assert!(has_main);
}

#[test]
fn test_filter() {
    let runner = TestRunner::new(false, Some("add".to_string()));
    let source = r#"
: test-addition ( -- ) ;
: test-subtraction ( -- ) ;
"#;
    let (tests, _skipped, _has_main) = runner.discover_test_functions(source).unwrap();
    assert_eq!(tests.len(), 1);
    assert!(tests.contains(&"test-addition".to_string()));
}

#[test]
fn discover_skips_test_prefixed_helper_with_non_unit_effect() {
    // Issue #435: a `test-` prefixed word with a non-( -- ) signature
    // (a predicate or validator) was treated as a test entry point and
    // produced a confusing stack-underflow error. The runner now reports
    // it under `skipped` instead.
    let runner = TestRunner::new(false, None);
    let source = r#"
: test-flag ( Int Int -- Bool )
  band 0 i.<>
;

: test-real ( -- )
  5 4 test-flag test.assert
;
"#;
    let (tests, skipped, _has_main) = runner.discover_test_functions(source).unwrap();
    assert_eq!(tests, vec!["test-real".to_string()]);
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].name, "test-flag");
    assert!(
        skipped[0].reason.contains("Int") && skipped[0].reason.contains("Bool"),
        "skip reason should show the actual effect, got: {}",
        skipped[0].reason
    );
}

#[test]
fn discover_skips_test_prefixed_word_with_no_declared_effect() {
    let runner = TestRunner::new(false, None);
    // No `( ... )` after the name — typechecker would infer one, but
    // discovery is a syntactic phase and treats absence as ambiguity.
    let source = r#"
: test-undeclared
  42 drop
;
"#;
    let (tests, skipped, _has_main) = runner.discover_test_functions(source).unwrap();
    assert!(tests.is_empty());
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped[0].name, "test-undeclared");
    assert_eq!(skipped[0].reason, "no stack effect declared");
}

#[test]
fn validate_paths_rejects_misnamed_seq_file() {
    let runner = TestRunner::new(false, None);
    let bad = PathBuf::from("script_mode.seq");
    let err = runner
        .validate_paths(&[bad])
        .expect_err("non-test-prefixed .seq path should be rejected");
    assert!(
        err.contains("test-*.seq"),
        "error should mention the naming requirement, got: {}",
        err
    );
}

#[test]
fn validate_paths_accepts_directory_path() {
    // Directories are passed without a `.seq` extension; descent
    // already filters to `test-*.seq`, so the validator must let them
    // through regardless of whether they exist on disk.
    let runner = TestRunner::new(false, None);
    let dir = PathBuf::from("some/dir");
    assert!(runner.validate_paths(&[dir]).is_ok());
}

#[test]
fn validate_paths_accepts_well_named_seq_file() {
    let runner = TestRunner::new(false, None);
    let good = PathBuf::from("test-foo.seq");
    assert!(runner.validate_paths(&[good]).is_ok());
}

#[test]
fn test_sanitize_name() {
    assert_eq!(sanitize_name("test-foo"), "test_foo");
    assert_eq!(sanitize_name("test-foo-bar"), "test_foo_bar");
}

#[test]
fn collect_failure_block_captures_indented_detail() {
    let output = "\
test-foo ... FAILED
  at line 7: expected 1, got 2
  +1 more failure
other-output
";
    let block = collect_failure_block(output, "test-foo").unwrap();
    assert_eq!(
        block,
        "test-foo ... FAILED\n  at line 7: expected 1, got 2\n  +1 more failure"
    );
}

#[test]
fn collect_failure_block_only_returns_target_block_when_adjacent() {
    let output = "\
test-one ... FAILED
  at line 1: expected 1, got 2
test-two ... FAILED
  at line 5: expected 3, got 4
";
    let one = collect_failure_block(output, "test-one").unwrap();
    let two = collect_failure_block(output, "test-two").unwrap();
    assert_eq!(one, "test-one ... FAILED\n  at line 1: expected 1, got 2");
    assert_eq!(two, "test-two ... FAILED\n  at line 5: expected 3, got 4");
}

#[test]
fn collect_failure_block_returns_none_when_absent() {
    let output = "\
test-foo ... ok
test-bar ... ok
";
    assert!(collect_failure_block(output, "test-foo").is_none());
    assert!(collect_failure_block(output, "missing").is_none());
}

#[test]
fn collect_failure_block_rejects_substring_false_positive() {
    // `add` is a prefix of `add-overflow`. The exact-line match must
    // not attribute `add-overflow`'s FAILED line to `add`.
    let output = "\
add-overflow ... FAILED
  at line 9: expected 0, got 1
";
    assert!(collect_failure_block(output, "add").is_none());
    assert!(collect_failure_block(output, "add-overflow").is_some());
}
