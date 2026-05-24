//! Test runner for Seq test files
//!
//! Discovers and executes tests in `test-*.seq` files, reporting results.

use crate::parser::Parser;
use crate::types::{Effect, StackType};
use crate::{CompilerConfig, compile_file_with_config};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Result of running a single test
#[derive(Debug)]
pub struct TestResult {
    /// Name of the test function
    pub name: String,
    /// Whether the test passed
    pub passed: bool,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Error output if test failed
    pub error_output: Option<String>,
}

/// Summary of all test results
#[derive(Debug, Default)]
pub struct TestSummary {
    /// Total tests run
    pub total: usize,
    /// Tests passed
    pub passed: usize,
    /// Tests failed
    pub failed: usize,
    /// Files that failed to compile
    pub compile_failures: usize,
    /// Results by file
    pub file_results: Vec<FileTestResults>,
}

impl TestSummary {
    /// Returns true if any tests failed or any files failed to compile
    pub fn has_failures(&self) -> bool {
        self.failed > 0 || self.compile_failures > 0
    }
}

/// Results for a single test file
#[derive(Debug)]
pub struct FileTestResults {
    /// Path to the test file
    pub path: PathBuf,
    /// Individual test results
    pub tests: Vec<TestResult>,
    /// Words named `test-*` whose stack effect isn't `( -- )` and were
    /// therefore not promoted to test entry points. Surfaced so the user
    /// can tell the difference between "helper picked up by accident"
    /// (issue #435) and "test silently disappeared".
    pub skipped: Vec<SkippedTest>,
    /// Compilation error if file failed to compile
    pub compile_error: Option<String>,
}

impl FileTestResults {
    /// Early-return shape for any compile/read/run failure: empty
    /// tests, empty skipped, the error message stashed in
    /// `compile_error`.
    fn with_compile_error(path: &Path, error: String) -> Self {
        Self {
            path: path.to_path_buf(),
            tests: vec![],
            skipped: vec![],
            compile_error: Some(error),
        }
    }

    /// Shape for "file parsed fine but produced no tests to run" —
    /// either it has its own `main`, or no `test-*` words with effect
    /// `( -- )`. `skipped` carries any name-matching helpers so the
    /// reporter can explain the absence.
    fn no_tests(path: &Path, skipped: Vec<SkippedTest>) -> Self {
        Self {
            path: path.to_path_buf(),
            tests: vec![],
            skipped,
            compile_error: None,
        }
    }
}

/// A `test-*` word that was discovered by name but skipped because its
/// stack effect isn't `( -- )`. Used for diagnostics, not execution.
#[derive(Debug, Clone)]
pub struct SkippedTest {
    /// Word name as written in source
    pub name: String,
    /// Human-readable reason — either a surface stack effect like
    /// `( Int Int -- Bool )` or "no stack effect declared".
    pub reason: String,
}

/// Test runner configuration
pub struct TestRunner {
    /// Show verbose output
    pub verbose: bool,
    /// Filter pattern for test names
    pub filter: Option<String>,
    /// Compiler configuration
    pub config: CompilerConfig,
}

impl TestRunner {
    pub fn new(verbose: bool, filter: Option<String>) -> Self {
        Self {
            verbose,
            filter,
            config: CompilerConfig::default(),
        }
    }

    /// Discover test files in the given paths
    pub fn discover_test_files(&self, paths: &[PathBuf]) -> Vec<PathBuf> {
        let mut test_files = Vec::new();

        for path in paths {
            if path.is_file() {
                if self.is_test_file(path) {
                    test_files.push(path.clone());
                }
            } else if path.is_dir() {
                self.discover_in_directory(path, &mut test_files);
            }
        }

        test_files.sort();
        test_files
    }

    /// Validate explicit paths before discovery. Any path ending in
    /// `.seq` is treated as a file path and must match `test-*.seq`;
    /// directories are fine (descent already filters). The check is
    /// name-based, not filesystem-state-based, so it's deterministic in
    /// tests and surfaces the naming rule even before the file is
    /// stat'd. Converts the previously silent zero-tests outcome on
    /// misnamed files into a loud error.
    pub fn validate_paths(&self, paths: &[PathBuf]) -> Result<(), String> {
        for path in paths {
            let looks_like_seq_file = path.extension().and_then(|e| e.to_str()) == Some("seq");
            if looks_like_seq_file && !self.is_test_file(path) {
                return Err(format!(
                    "Test files must be named `test-*.seq`. Got: `{}`",
                    path.display()
                ));
            }
        }
        Ok(())
    }

    fn is_test_file(&self, path: &Path) -> bool {
        path.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name.starts_with("test-") && name.ends_with(".seq"))
    }

    fn discover_in_directory(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && self.is_test_file(&path) {
                    files.push(path);
                } else if path.is_dir() {
                    self.discover_in_directory(&path, files);
                }
            }
        }
    }

    /// Discover test functions in a source file.
    ///
    /// A word is a test entry point iff its name starts with `test-` AND
    /// its declared stack effect is exactly `( -- )`. Words matching the
    /// name pattern but with a different effect (e.g. a `test-flag`
    /// helper with `( Int Int -- Bool )`) are reported as `skipped` so
    /// the runner can tell the user why the synthetic main isn't calling
    /// them — see issue #435 for the original confusion.
    ///
    /// Returns (test_names, skipped, has_main).
    pub fn discover_test_functions(
        &self,
        source: &str,
    ) -> Result<(Vec<String>, Vec<SkippedTest>, bool), String> {
        let mut parser = Parser::new(source);
        let program = parser.parse()?;

        let has_main = program.words.iter().any(|w| w.name == "main");

        let mut test_names: Vec<String> = Vec::new();
        let mut skipped: Vec<SkippedTest> = Vec::new();

        for w in &program.words {
            if !w.name.starts_with("test-") {
                continue;
            }
            if !self.matches_filter(&w.name) {
                continue;
            }
            match &w.effect {
                Some(eff) if is_unit_effect(eff) => {
                    test_names.push(w.name.clone());
                }
                Some(eff) => {
                    skipped.push(SkippedTest {
                        name: w.name.clone(),
                        reason: format_effect_surface(eff),
                    });
                }
                None => {
                    skipped.push(SkippedTest {
                        name: w.name.clone(),
                        reason: "no stack effect declared".to_string(),
                    });
                }
            }
        }

        test_names.sort();
        skipped.sort_by(|a, b| a.name.cmp(&b.name));
        Ok((test_names, skipped, has_main))
    }

    fn matches_filter(&self, name: &str) -> bool {
        match &self.filter {
            Some(pattern) => name.contains(pattern),
            None => true,
        }
    }

    /// Run all tests in a file
    pub fn run_file(&self, path: &Path) -> FileTestResults {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return FileTestResults::with_compile_error(
                    path,
                    format!("Failed to read file: {}", e),
                );
            }
        };

        let (test_names, skipped, has_main) = match self.discover_test_functions(&source) {
            Ok(result) => result,
            Err(e) => {
                return FileTestResults::with_compile_error(path, format!("Parse error: {}", e));
            }
        };

        // Skip files that have their own main - they are standalone test suites
        if has_main {
            return FileTestResults::no_tests(path, skipped);
        }

        if test_names.is_empty() {
            return FileTestResults::no_tests(path, skipped);
        }

        let mut results = self.run_all_tests_in_file(path, &source, &test_names);
        results.skipped = skipped;
        results
    }

    fn run_all_tests_in_file(
        &self,
        path: &Path,
        source: &str,
        test_names: &[String],
    ) -> FileTestResults {
        let start = Instant::now();

        // Generate wrapper main that runs ALL tests in sequence.
        //
        // `test.set-name` after the user's test word guarantees the
        // `test.finish` header matches the word name the parser discovered,
        // even if the user called `test.init` with a different friendly
        // name inside their test word. Without this, `collect_failure_block`
        // (which keys on the word name) would orphan detail lines.
        let mut test_calls = String::new();
        for test_name in test_names {
            test_calls.push_str(&format!(
                "  \"{0}\" test.init {0} \"{0}\" test.set-name test.finish\n",
                test_name
            ));
        }

        let wrapper = format!(
            r#"{}

: main ( -- )
{}  test.has-failures [ 1 os.exit ] [ ] if
;
"#,
            source, test_calls
        );

        // Create temp file for the wrapper
        let temp_dir = std::env::temp_dir();
        let file_id = sanitize_name(&path.to_string_lossy());
        let wrapper_path = temp_dir.join(format!("seq_test_{}.seq", file_id));
        let binary_path = temp_dir.join(format!("seq_test_{}", file_id));

        if let Err(e) = fs::write(&wrapper_path, &wrapper) {
            return FileTestResults::with_compile_error(
                path,
                format!("Failed to write temp file: {}", e),
            );
        }

        // Compile the wrapper (ONE compilation for all tests in file)
        if let Err(e) = compile_file_with_config(&wrapper_path, &binary_path, false, &self.config) {
            let _ = fs::remove_file(&wrapper_path);
            return FileTestResults::with_compile_error(path, format!("Compilation error: {}", e));
        }

        let output = Command::new(&binary_path).output();

        let _ = fs::remove_file(&wrapper_path);
        let _ = fs::remove_file(&binary_path);

        let compile_time = start.elapsed().as_millis() as u64;

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                // Parse output to determine which tests passed/failed
                // Output format: "test-name ... ok" or "test-name ... FAILED"
                let results = self.parse_test_output(&stdout, test_names, compile_time);

                // If we couldn't parse results but process failed, mark all as failed
                if results.iter().all(|r| r.passed) && !output.status.success() {
                    return FileTestResults {
                        path: path.to_path_buf(),
                        tests: test_names
                            .iter()
                            .map(|name| TestResult {
                                name: name.clone(),
                                passed: false,
                                duration_ms: 0,
                                error_output: Some(format!("{}{}", stderr, stdout)),
                            })
                            .collect(),
                        skipped: vec![],
                        compile_error: None,
                    };
                }

                FileTestResults {
                    path: path.to_path_buf(),
                    tests: results,
                    skipped: vec![],
                    compile_error: None,
                }
            }
            Err(e) => {
                FileTestResults::with_compile_error(path, format!("Failed to run tests: {}", e))
            }
        }
    }

    fn parse_test_output(
        &self,
        output: &str,
        test_names: &[String],
        _compile_time: u64,
    ) -> Vec<TestResult> {
        let mut results = Vec::new();

        for test_name in test_names {
            // Look for "test-name ... ok" or "test-name ... FAILED"
            let passed = output
                .lines()
                .any(|line| line.contains(test_name) && line.contains("... ok"));

            // For failures, capture the FAILED header line plus any
            // indented detail lines that immediately follow it (runtime
            // emits `expected X, got Y`-style lines indented under the
            // header on the same stdout stream).
            let error_output = if !passed {
                collect_failure_block(output, test_name)
            } else {
                None
            };

            results.push(TestResult {
                name: test_name.clone(),
                passed,
                duration_ms: 0, // Individual timing not available in batch mode
                error_output,
            });
        }

        results
    }

    /// Run tests and return summary
    pub fn run(&self, paths: &[PathBuf]) -> TestSummary {
        let test_files = self.discover_test_files(paths);
        let mut summary = TestSummary::default();

        for path in test_files {
            let file_results = self.run_file(&path);

            if file_results.compile_error.is_some() {
                summary.compile_failures += 1;
            }

            for test in &file_results.tests {
                summary.total += 1;
                if test.passed {
                    summary.passed += 1;
                } else {
                    summary.failed += 1;
                }
            }

            summary.file_results.push(file_results);
        }

        summary
    }

    /// Print test results
    pub fn print_results(&self, summary: &TestSummary) {
        for file_result in &summary.file_results {
            if let Some(ref error) = file_result.compile_error {
                eprintln!("\nFailed to process {}:", file_result.path.display());
                eprintln!("  {}", error);
                continue;
            }

            if file_result.tests.is_empty() && file_result.skipped.is_empty() {
                continue;
            }

            println!("\nRunning tests in {}...", file_result.path.display());

            for test in &file_result.tests {
                let status = if test.passed { "ok" } else { "FAILED" };
                if self.verbose {
                    println!("  {} ... {} ({}ms)", test.name, status, test.duration_ms);
                } else {
                    println!("  {} ... {}", test.name, status);
                }
            }

            for s in &file_result.skipped {
                println!(
                    "  {} ... skipped — name starts with `test-` but stack effect is {}, not ( -- ). Rename if it's a helper; fix the signature if it's a test.",
                    s.name, s.reason
                );
            }
        }

        println!("\n========================================");
        if summary.compile_failures > 0 {
            println!(
                "Results: {} passed, {} failed, {} failed to compile",
                summary.passed, summary.failed, summary.compile_failures
            );
        } else {
            println!(
                "Results: {} passed, {} failed",
                summary.passed, summary.failed
            );
        }

        // Print test failures in detail
        let failures: Vec<_> = summary
            .file_results
            .iter()
            .flat_map(|fr| fr.tests.iter().filter(|t| !t.passed).map(|t| (&fr.path, t)))
            .collect();

        if !failures.is_empty() {
            println!("\nTEST FAILURES:\n");
            for (path, test) in failures {
                println!("{}::{}", path.display(), test.name);
                if let Some(ref error) = test.error_output {
                    for line in error.lines() {
                        println!("  {}", line);
                    }
                }
                println!();
            }
        }

        // Print compilation failures in detail
        let compile_failures: Vec<_> = summary
            .file_results
            .iter()
            .filter(|fr| fr.compile_error.is_some())
            .collect();

        if !compile_failures.is_empty() {
            println!("\nCOMPILATION FAILURES:\n");
            for fr in compile_failures {
                println!("{}:", fr.path.display());
                if let Some(ref error) = fr.compile_error {
                    for line in error.lines() {
                        println!("  {}", line);
                    }
                }
                println!();
            }
        }
    }
}

/// Sanitize a test name for use as a filename
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

/// True iff the effect is `( -- )` — neither side has concrete types
/// pushed or popped, and no computational side effects. The parser
/// implicitly row-polymorphises every effect (`( -- )` becomes
/// `..rest -- ..rest`), so the check is "no `Cons` layer on either
/// side" rather than literal `Empty`. The row-var name doesn't matter.
fn is_unit_effect(eff: &Effect) -> bool {
    fn no_concrete_types(st: &StackType) -> bool {
        !matches!(st, StackType::Cons { .. })
    }
    no_concrete_types(&eff.inputs) && no_concrete_types(&eff.outputs) && eff.effects.is_empty()
}

/// Render an effect in surface syntax — `( Int Int -- Bool )` rather
/// than the internal `(..rest Int Int) -- (..rest Bool)` printed by
/// `Display for Effect`. The parser implicitly row-polymorphises every
/// effect; if the same row variable sits at the bottom of both sides
/// it's just the implicit one, and showing it would be noisier than
/// what the user wrote in source. Used only for the skip diagnostic.
fn format_effect_surface(eff: &Effect) -> String {
    // Walk down a stack type to its bottom row var (if any) and the
    // concrete top-down list of types stacked above it.
    fn split(st: &StackType) -> (Option<&str>, Vec<String>) {
        let mut types: Vec<String> = Vec::new();
        let mut cur = st;
        loop {
            match cur {
                StackType::Empty => {
                    types.reverse();
                    return (None, types);
                }
                StackType::RowVar(name) => {
                    types.reverse();
                    return (Some(name.as_str()), types);
                }
                StackType::Cons { rest, top } => {
                    types.push(format!("{}", top));
                    cur = rest;
                }
            }
        }
    }
    let (in_rv, in_types) = split(&eff.inputs);
    let (out_rv, out_types) = split(&eff.outputs);
    // Hide the row variable when both sides share it (implicit
    // polymorphism). Otherwise show it explicitly.
    let show_row = in_rv != out_rv;

    let render = |rv: Option<&str>, types: &[String]| -> String {
        let mut parts: Vec<String> = Vec::new();
        if show_row && let Some(name) = rv {
            parts.push(format!("..{}", name));
        }
        parts.extend(types.iter().cloned());
        parts.join(" ")
    };
    let inp = render(in_rv, &in_types);
    let out = render(out_rv, &out_types);
    let inp_sep = if inp.is_empty() { "" } else { " " };
    let out_sep = if out.is_empty() { "" } else { " " };
    if eff.effects.is_empty() {
        format!("( {}{}-- {}{})", inp, inp_sep, out, out_sep)
    } else {
        let effs: Vec<String> = eff.effects.iter().map(|e| format!("{}", e)).collect();
        format!(
            "( {}{}-- {}{}| {} )",
            inp,
            inp_sep,
            out,
            out_sep,
            effs.join(" ")
        )
    }
}

/// Given the full test-wrapper stdout and a test name, find the
/// `<name> ... FAILED` header line plus any indented detail lines
/// that immediately follow it, and return them as a single block.
///
/// An indented detail line is any line that begins with whitespace.
/// Collection stops at the next non-indented line (typically the next
/// test's header, or the pass/fail summary).
///
/// Matches the header exactly (`{name} ... FAILED`) so one test name
/// being a substring of another (e.g. `add` vs `add-overflow`) cannot
/// cross-attribute the block.
fn collect_failure_block(output: &str, test_name: &str) -> Option<String> {
    let header = format!("{} ... FAILED", test_name);
    let mut lines = output.lines().peekable();
    while let Some(line) = lines.next() {
        if line == header {
            let mut block = String::from(line);
            while let Some(next) = lines.peek() {
                if next.starts_with(char::is_whitespace) {
                    block.push('\n');
                    block.push_str(next);
                    lines.next();
                } else {
                    break;
                }
            }
            return Some(block);
        }
    }
    None
}

#[cfg(test)]
mod tests;
