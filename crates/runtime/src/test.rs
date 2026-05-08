//! Test framework support for Seq
//!
//! Provides assertion primitives and test context management for the `seqc test` runner.
//! Assertions collect failures instead of panicking, allowing all tests to run and
//! report comprehensive results.
//!
//! These functions are exported with C ABI for LLVM codegen to call.

use crate::stack::{Stack, pop, push};
use crate::value::Value;
use std::sync::Mutex;

/// Render a stack `Value` for a failure message — prefers the natural
/// form for `Int` / `Bool`, falls back to debug for anything else.
fn display_value(val: &Value) -> String {
    match val {
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        other => format!("{:?}", other),
    }
}

/// Maximum number of per-test assertion failures to print in the run
/// summary. Additional failures are rolled up into a `+N more failure(s)`
/// footer so noisy tests (loop-like assertions over lists) don't drown
/// the overall report. Tune here if feedback suggests a different value.
const MAX_PRINTED_FAILURES_PER_TEST: usize = 5;

/// A single test failure with context
#[derive(Debug, Clone)]
pub struct TestFailure {
    /// Source line of the assertion (1-indexed), if codegen set one.
    pub line: Option<u32>,
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Test context that tracks assertion results
#[derive(Debug, Default)]
pub struct TestContext {
    /// Current test name being executed
    pub current_test: Option<String>,
    /// Source line of the assertion most recently announced by codegen.
    /// Set by `patch_seq_test_set_line` just before each `test.assert*`
    /// call; captured into a `TestFailure` if the assertion fails.
    pub current_line: Option<u32>,
    /// Number of passed assertions
    pub passes: usize,
    /// Collected failures
    pub failures: Vec<TestFailure>,
}

impl TestContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self, test_name: Option<String>) {
        self.current_test = test_name;
        self.current_line = None;
        self.passes = 0;
        self.failures.clear();
    }

    pub fn record_pass(&mut self) {
        self.passes += 1;
        // Consume the line so a following assertion without a `set_line`
        // hook (defensive — span-less `WordCall`s don't emit one) can't
        // inherit this one's attribution.
        self.current_line = None;
    }

    pub fn record_failure(
        &mut self,
        message: String,
        expected: Option<String>,
        actual: Option<String>,
    ) {
        self.failures.push(TestFailure {
            line: self.current_line,
            message,
            expected,
            actual,
        });
        // Same rationale as `record_pass`: don't let this line bleed into
        // the next assertion's record.
        self.current_line = None;
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// Global test context protected by mutex
static TEST_CONTEXT: Mutex<TestContext> = Mutex::new(TestContext {
    current_test: None,
    current_line: None,
    passes: 0,
    failures: Vec::new(),
});

/// Announce the source line of the next `test.assert*` call.
///
/// Called by generated code immediately before each assertion so the
/// runtime can attribute a failure to its source position. `line` is
/// 1-indexed; pass 0 to clear.
///
/// This helper takes a raw `i64` rather than a stack argument because it
/// is a compiler-emitted diagnostic, not a user-callable Seq builtin.
///
/// # Safety
///
/// Safe to call from any thread. Acquires the global test-context
/// mutex; no other preconditions.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_set_line(line: i64) {
    let mut ctx = TEST_CONTEXT.lock().unwrap();
    // Reject 0 (the agreed "clear" sentinel) and any value that can't
    // fit in a u32 (no real source file has 4B lines, but be explicit
    // about truncation intent rather than silently wrapping).
    ctx.current_line = if line > 0 {
        u32::try_from(line).ok()
    } else {
        None
    };
}

/// Set the current test's display name without touching any other state.
///
/// Used by the `seqc test` runner to reassert the word-level test name
/// after the user's test word has run, in case the user called
/// `test.init "friendly name"` internally and overwrote the header.
/// Unlike `test.init`, this does NOT clear `failures`, `passes`, or
/// `current_line`.
///
/// Stack effect: ( ..a String -- ..a )
///
/// # Safety
/// Stack must have a String (test name) on top.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_set_name(stack: Stack) -> Stack {
    unsafe {
        let (stack, name_val) = pop(stack);
        let name = match name_val {
            Value::String(s) => s.as_str_or_empty().to_string(),
            _ => panic!("test.set-name: expected String (test name) on stack"),
        };
        let mut ctx = TEST_CONTEXT.lock().unwrap();
        ctx.current_test = Some(name);
        stack
    }
}

/// Initialize test context for a new test
///
/// Stack effect: ( name -- )
///
/// # Safety
/// Stack must have a String (test name) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_init(stack: Stack) -> Stack {
    unsafe {
        let (stack, name_val) = pop(stack);
        let name = match name_val {
            Value::String(s) => s.as_str_or_empty().to_string(),
            _ => panic!("test.init: expected String (test name) on stack"),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        ctx.reset(Some(name));
        stack
    }
}

/// Finalize test and print results
///
/// Stack effect: ( -- )
///
/// Prints pass/fail summary for the current test in a format parseable by the test runner.
/// Output format: "test-name ... ok" or "test-name ... FAILED"
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_finish(stack: Stack) -> Stack {
    let ctx = TEST_CONTEXT.lock().unwrap();
    let test_name = ctx.current_test.as_deref().unwrap_or("unknown");

    if ctx.failures.is_empty() {
        // Output pass in parseable format
        println!("{} ... ok", test_name);
    } else {
        // Output failure in parseable format. Detail lines are emitted on
        // STDOUT, indented, so the test runner can associate them with the
        // preceding FAILED header on the same stream.
        // Cap the per-test output so a flood of failures (e.g. a loop-like
        // test walking a list) doesn't drown the summary. The first
        // `MAX_PRINTED_FAILURES_PER_TEST` are printed in full; a footer
        // counts anything suppressed.
        println!("{} ... FAILED", test_name);
        for failure in ctx.failures.iter().take(MAX_PRINTED_FAILURES_PER_TEST) {
            let detail = match (&failure.expected, &failure.actual) {
                (Some(e), Some(a)) => format!("expected {}, got {}", e, a),
                _ => failure.message.clone(),
            };
            match failure.line {
                Some(line) => println!("  at line {}: {}", line, detail),
                None => println!("  {}", detail),
            }
        }
        if ctx.failures.len() > MAX_PRINTED_FAILURES_PER_TEST {
            let remaining = ctx.failures.len() - MAX_PRINTED_FAILURES_PER_TEST;
            let s = if remaining == 1 { "" } else { "s" };
            println!("  +{} more failure{}", remaining, s);
        }
    }

    stack
}

/// Check if any assertions failed
///
/// Stack effect: ( -- Int )
///
/// Returns 1 if there are failures, 0 if all passed.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_has_failures(stack: Stack) -> Stack {
    let ctx = TEST_CONTEXT.lock().unwrap();
    let has_failures = ctx.has_failures();
    unsafe { push(stack, Value::Bool(has_failures)) }
}

/// Assert that a value is truthy (non-zero)
///
/// Stack effect: ( Int -- )
///
/// Records failure if value is 0, records pass otherwise.
///
/// # Safety
/// Stack must have an Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert(stack: Stack) -> Stack {
    unsafe {
        let (stack, val) = pop(stack);
        let condition = match val {
            Value::Int(n) => n != 0,
            Value::Bool(b) => b,
            _ => panic!("test.assert: expected Int or Bool on stack, got {:?}", val),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        if condition {
            ctx.record_pass();
        } else {
            ctx.record_failure(
                "assertion failed".to_string(),
                Some("true".to_string()),
                Some(display_value(&val)),
            );
        }

        stack
    }
}

/// Assert that a value is falsy (zero)
///
/// Stack effect: ( Int -- )
///
/// Records failure if value is non-zero, records pass otherwise.
///
/// # Safety
/// Stack must have an Int on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert_not(stack: Stack) -> Stack {
    unsafe {
        let (stack, val) = pop(stack);
        let is_falsy = match val {
            Value::Int(n) => n == 0,
            Value::Bool(b) => !b,
            _ => panic!(
                "test.assert-not: expected Int or Bool on stack, got {:?}",
                val
            ),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        if is_falsy {
            ctx.record_pass();
        } else {
            ctx.record_failure(
                "assertion failed".to_string(),
                Some("false".to_string()),
                Some(display_value(&val)),
            );
        }

        stack
    }
}

/// Assert that two integers are equal
///
/// Stack effect: ( actual expected -- )
///
/// Records failure if values differ, records pass otherwise.
///
/// # Safety
/// Stack must have two Ints on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert_eq(stack: Stack) -> Stack {
    unsafe {
        let (stack, expected_val) = pop(stack);
        let (stack, actual_val) = pop(stack);

        let (expected, actual) = match (&expected_val, &actual_val) {
            (Value::Int(e), Value::Int(a)) => (*e, *a),
            _ => panic!(
                "test.assert-eq: expected two Ints on stack, got {:?} and {:?}",
                expected_val, actual_val
            ),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        if expected == actual {
            ctx.record_pass();
        } else {
            ctx.record_failure(
                "assertion failed: values not equal".to_string(),
                Some(expected.to_string()),
                Some(actual.to_string()),
            );
        }

        stack
    }
}

/// Assert that two strings are equal
///
/// Stack effect: ( actual expected -- )
///
/// Records failure if strings differ, records pass otherwise.
///
/// # Safety
/// Stack must have two Strings on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert_eq_str(stack: Stack) -> Stack {
    unsafe {
        let (stack, expected_val) = pop(stack);
        let (stack, actual_val) = pop(stack);

        let (expected, actual) = match (&expected_val, &actual_val) {
            (Value::String(e), Value::String(a)) => (
                e.as_str_or_empty().to_string(),
                a.as_str_or_empty().to_string(),
            ),
            _ => panic!(
                "test.assert-eq-str: expected two Strings on stack, got {:?} and {:?}",
                expected_val, actual_val
            ),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        if expected == actual {
            ctx.record_pass();
        } else {
            ctx.record_failure(
                "assertion failed: strings not equal".to_string(),
                Some(format!("\"{}\"", expected)),
                Some(format!("\"{}\"", actual)),
            );
        }

        stack
    }
}

/// Explicitly fail a test with a message
///
/// Stack effect: ( message -- )
///
/// Always records a failure with the given message.
///
/// # Safety
/// Stack must have a String on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_fail(stack: Stack) -> Stack {
    unsafe {
        let (stack, msg_val) = pop(stack);
        let message = match msg_val {
            Value::String(s) => s.as_str_or_empty().to_string(),
            _ => panic!("test.fail: expected String (message) on stack"),
        };

        let mut ctx = TEST_CONTEXT.lock().unwrap();
        ctx.record_failure(message, None, None);

        stack
    }
}

/// Get the number of passed assertions
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_pass_count(stack: Stack) -> Stack {
    let ctx = TEST_CONTEXT.lock().unwrap();
    unsafe { push(stack, Value::Int(ctx.passes as i64)) }
}

/// Get the number of failed assertions
///
/// Stack effect: ( -- Int )
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_fail_count(stack: Stack) -> Stack {
    let ctx = TEST_CONTEXT.lock().unwrap();
    unsafe { push(stack, Value::Int(ctx.failures.len() as i64)) }
}

#[cfg(test)]
mod tests;
