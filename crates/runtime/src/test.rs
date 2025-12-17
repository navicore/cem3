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

/// A single test failure with context
#[derive(Debug, Clone)]
pub struct TestFailure {
    pub message: String,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

/// Test context that tracks assertion results
#[derive(Debug, Default)]
pub struct TestContext {
    /// Current test name being executed
    pub current_test: Option<String>,
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
        self.passes = 0;
        self.failures.clear();
    }

    pub fn record_pass(&mut self) {
        self.passes += 1;
    }

    pub fn record_failure(
        &mut self,
        message: String,
        expected: Option<String>,
        actual: Option<String>,
    ) {
        self.failures.push(TestFailure {
            message,
            expected,
            actual,
        });
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// Global test context protected by mutex
static TEST_CONTEXT: Mutex<TestContext> = Mutex::new(TestContext {
    current_test: None,
    passes: 0,
    failures: Vec::new(),
});

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
            Value::String(s) => s.as_str().to_string(),
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
        // Output failure in parseable format
        println!("{} ... FAILED", test_name);
        // Print failure details to stderr
        for failure in &ctx.failures {
            eprintln!("    {}", failure.message);
            if let Some(ref expected) = failure.expected {
                eprintln!("      expected: {}", expected);
            }
            if let Some(ref actual) = failure.actual {
                eprintln!("      actual: {}", actual);
            }
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
    let has_failures = if ctx.has_failures() { 1 } else { 0 };
    unsafe { push(stack, Value::Int(has_failures)) }
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
                "assertion failed: expected truthy value".to_string(),
                Some("non-zero".to_string()),
                Some("0".to_string()),
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
                "assertion failed: expected falsy value".to_string(),
                Some("0".to_string()),
                Some(format!("{:?}", val)),
            );
        }

        stack
    }
}

/// Assert that two integers are equal
///
/// Stack effect: ( expected actual -- )
///
/// Records failure if values differ, records pass otherwise.
///
/// # Safety
/// Stack must have two Ints on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert_eq(stack: Stack) -> Stack {
    unsafe {
        let (stack, actual_val) = pop(stack);
        let (stack, expected_val) = pop(stack);

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
/// Stack effect: ( expected actual -- )
///
/// Records failure if strings differ, records pass otherwise.
///
/// # Safety
/// Stack must have two Strings on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_test_assert_eq_str(stack: Stack) -> Stack {
    unsafe {
        let (stack, actual_val) = pop(stack);
        let (stack, expected_val) = pop(stack);

        let (expected, actual) = match (&expected_val, &actual_val) {
            (Value::String(e), Value::String(a)) => {
                (e.as_str().to_string(), a.as_str().to_string())
            }
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
            Value::String(s) => s.as_str().to_string(),
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
mod tests {
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
}
