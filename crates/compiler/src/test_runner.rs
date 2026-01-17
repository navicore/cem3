//! Test runner for Seq test files
//!
//! Discovers and executes tests in `test-*.seq` files, reporting results.

use crate::parser::Parser;
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
    /// Results by file
    pub file_results: Vec<FileTestResults>,
}

/// Results for a single test file
#[derive(Debug)]
pub struct FileTestResults {
    /// Path to the test file
    pub path: PathBuf,
    /// Individual test results
    pub tests: Vec<TestResult>,
    /// Compilation error if file failed to compile
    pub compile_error: Option<String>,
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

    fn is_test_file(&self, path: &Path) -> bool {
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            name.starts_with("test-") && name.ends_with(".seq")
        } else {
            false
        }
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

    /// Discover test functions in a source file
    /// Returns (test_names, has_main) - test names and whether file has its own main
    pub fn discover_test_functions(&self, source: &str) -> Result<(Vec<String>, bool), String> {
        let mut parser = Parser::new(source);
        let program = parser.parse()?;

        let has_main = program.words.iter().any(|w| w.name == "main");

        let mut test_names: Vec<String> = program
            .words
            .iter()
            .filter(|w| w.name.starts_with("test-"))
            .filter(|w| self.matches_filter(&w.name))
            .map(|w| w.name.clone())
            .collect();

        test_names.sort();
        Ok((test_names, has_main))
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
                return FileTestResults {
                    path: path.to_path_buf(),
                    tests: vec![],
                    compile_error: Some(format!("Failed to read file: {}", e)),
                };
            }
        };

        let (test_names, has_main) = match self.discover_test_functions(&source) {
            Ok(result) => result,
            Err(e) => {
                return FileTestResults {
                    path: path.to_path_buf(),
                    tests: vec![],
                    compile_error: Some(format!("Parse error: {}", e)),
                };
            }
        };

        // Skip files that have their own main - they are standalone test suites
        if has_main {
            return FileTestResults {
                path: path.to_path_buf(),
                tests: vec![],
                compile_error: None,
            };
        }

        if test_names.is_empty() {
            return FileTestResults {
                path: path.to_path_buf(),
                tests: vec![],
                compile_error: None,
            };
        }

        // Compile once and run all tests in the file
        self.run_all_tests_in_file(path, &source, &test_names)
    }

    fn run_all_tests_in_file(
        &self,
        path: &Path,
        source: &str,
        test_names: &[String],
    ) -> FileTestResults {
        let start = Instant::now();

        // Generate wrapper main that runs ALL tests in sequence
        let mut test_calls = String::new();
        for test_name in test_names {
            test_calls.push_str(&format!(
                "  \"{}\" test.init {} test.finish\n",
                test_name, test_name
            ));
        }

        let wrapper = format!(
            r#"{}

: main ( -- )
{}  test.has-failures if
    1 os.exit
  then
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
            return FileTestResults {
                path: path.to_path_buf(),
                tests: vec![],
                compile_error: Some(format!("Failed to write temp file: {}", e)),
            };
        }

        // Compile the wrapper (ONE compilation for all tests in file)
        if let Err(e) = compile_file_with_config(&wrapper_path, &binary_path, false, &self.config) {
            let _ = fs::remove_file(&wrapper_path);
            return FileTestResults {
                path: path.to_path_buf(),
                tests: vec![],
                compile_error: Some(format!("Compilation error: {}", e)),
            };
        }

        // Run the compiled tests
        let output = Command::new(&binary_path).output();

        // Clean up temp files
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
                        compile_error: None,
                    };
                }

                FileTestResults {
                    path: path.to_path_buf(),
                    tests: results,
                    compile_error: None,
                }
            }
            Err(e) => FileTestResults {
                path: path.to_path_buf(),
                tests: vec![],
                compile_error: Some(format!("Failed to run tests: {}", e)),
            },
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
            // Look for "test-name ... ok" or "test-name ... FAILED: message"
            let passed = output
                .lines()
                .any(|line| line.contains(test_name) && line.contains("... ok"));

            let error_output = if !passed {
                // Find failure message
                output
                    .lines()
                    .find(|line| line.contains(test_name) && line.contains("FAILED"))
                    .map(|s| s.to_string())
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

            if file_result.tests.is_empty() {
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
        }

        // Print summary
        println!("\n========================================");
        println!(
            "Results: {} passed, {} failed",
            summary.passed, summary.failed
        );

        // Print failures in detail
        let failures: Vec<_> = summary
            .file_results
            .iter()
            .flat_map(|fr| fr.tests.iter().filter(|t| !t.passed).map(|t| (&fr.path, t)))
            .collect();

        if !failures.is_empty() {
            println!("\nFAILURES:\n");
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
    }
}

/// Sanitize a test name for use as a filename
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
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
        let (tests, has_main) = runner.discover_test_functions(source).unwrap();
        assert_eq!(tests.len(), 2);
        assert!(tests.contains(&"test-addition".to_string()));
        assert!(tests.contains(&"test-subtraction".to_string()));
        assert!(!tests.contains(&"helper".to_string()));
        assert!(!has_main);
    }

    #[test]
    fn test_discover_with_main() {
        let runner = TestRunner::new(false, None);
        let source = r#"
: test-foo ( -- ) ;
: main ( -- ) ;
"#;
        let (tests, has_main) = runner.discover_test_functions(source).unwrap();
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
        let (tests, _) = runner.discover_test_functions(source).unwrap();
        assert_eq!(tests.len(), 1);
        assert!(tests.contains(&"test-addition".to_string()));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("test-foo"), "test_foo");
        assert_eq!(sanitize_name("test-foo-bar"), "test_foo_bar");
    }
}
