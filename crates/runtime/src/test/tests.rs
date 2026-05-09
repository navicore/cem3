use super::*;

#[test]
fn test_context_reset() {
    let mut ctx = TestContext::new();
    ctx.record_pass();
    ctx.record_failure("test".to_string(), None, None);

    assert_eq!(ctx.passes, 1);
    assert_eq!(ctx.failures.len(), 1);

    ctx.reset(Some("new-test".to_string()));

    assert_eq!(ctx.passes, 0);
    assert!(ctx.failures.is_empty());
    assert_eq!(ctx.current_test, Some("new-test".to_string()));
}

#[test]
fn test_context_has_failures() {
    let mut ctx = TestContext::new();
    assert!(!ctx.has_failures());

    ctx.record_failure("error".to_string(), None, None);
    assert!(ctx.has_failures());
}

#[test]
fn escape_renders_control_bytes_as_source_escapes() {
    assert_eq!(escape_for_display("a\nb"), "a\\nb");
    assert_eq!(escape_for_display("tab\there"), "tab\\there");
    assert_eq!(escape_for_display("quote\"here"), "quote\\\"here");
    assert_eq!(escape_for_display("back\\slash"), "back\\\\slash");
    // Non-printable, non-recognized bytes use \xNN.
    assert_eq!(escape_for_display("bell\x07"), "bell\\x07");
    // ASCII printables and high bytes pass through unchanged.
    assert_eq!(escape_for_display("hello world"), "hello world");
}

#[test]
fn string_eq_failure_detail_is_multiline_and_indented() {
    let mut ctx = TestContext::new();
    ctx.current_line = Some(18);
    ctx.record_string_eq_failure(
        "> alpha\n> beta\n> gamma".to_string(),
        "> alpha\n> beta\n> gamma\n".to_string(),
    );
    let detail = format_failure_detail(&ctx.failures[0]);

    // Every emitted line must start with whitespace so
    // collect_failure_block in the test runner attaches the entire
    // block to the preceding FAILED header (no truncation at embedded
    // newlines).
    for line in detail.lines() {
        assert!(
            line.starts_with(char::is_whitespace),
            "every detail line must be whitespace-prefixed; got: {:?}",
            line
        );
    }

    assert!(detail.contains("at line 18:"));
    assert!(detail.contains("assertion failed: strings not equal"));
    assert!(detail.contains("expected (22 bytes):"));
    assert!(detail.contains("actual   (23 bytes):"));
    assert!(detail.contains("> alpha\\n> beta\\n> gamma"));
    assert!(detail.contains("first differ at byte 22"));
}

#[test]
fn string_eq_failure_with_no_line_omits_at_line_prefix() {
    let mut ctx = TestContext::new();
    ctx.record_string_eq_failure("a".to_string(), "b".to_string());
    let detail = format_failure_detail(&ctx.failures[0]);
    assert!(!detail.contains("at line"));
    assert!(detail.contains("assertion failed: strings not equal"));
    // Strings of equal length differing at byte 0.
    assert!(detail.contains("first differ at byte 0"));
}

#[test]
fn numeric_failure_keeps_single_line_format() {
    let mut ctx = TestContext::new();
    ctx.current_line = Some(7);
    ctx.record_failure(
        "assertion failed: values not equal".to_string(),
        Some("99".to_string()),
        Some("5".to_string()),
    );
    let detail = format_failure_detail(&ctx.failures[0]);
    // Backwards-compatible single-line form for the numeric path.
    assert_eq!(detail, "  at line 7: expected 99, got 5\n");
}
