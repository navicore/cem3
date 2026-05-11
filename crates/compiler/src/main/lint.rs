//! `seqc lint` subcommand: walk a directory tree, run the lint engine on
//! every .seq file, and report diagnostics.

use std::path::{Path, PathBuf};
use std::process;

pub(crate) fn run_lint(
    paths: &[PathBuf],
    config_path: Option<&std::path::Path>,
    errors_only: bool,
    deny_warnings: bool,
) {
    use seqc::lint;
    use std::fs;

    // Load lint configuration
    let config = match config_path {
        Some(path) => {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading lint config: {}", e);
                    process::exit(1);
                }
            };
            match lint::LintConfig::from_toml(&content) {
                Ok(user_config) => {
                    // Merge with defaults
                    let mut default = match lint::LintConfig::default_config() {
                        Ok(d) => d,
                        Err(e) => {
                            eprintln!("Error loading default lint config: {}", e);
                            process::exit(1);
                        }
                    };
                    default.merge(user_config);
                    default
                }
                Err(e) => {
                    eprintln!("Error parsing lint config: {}", e);
                    process::exit(1);
                }
            }
        }
        None => match lint::LintConfig::default_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Error loading default lint config: {}", e);
                process::exit(1);
            }
        },
    };

    let linter = match lint::Linter::new(&config) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error creating linter: {}", e);
            process::exit(1);
        }
    };

    let mut all_diagnostics = Vec::new();
    let mut files_checked = 0;

    for path in paths {
        if path.is_dir() {
            // Recursively find .seq files
            for entry in walkdir(path) {
                if entry.extension().is_some_and(|e| e == "seq") {
                    // Skip files in directories with .toml manifests (require --ffi-manifest)
                    if let Some(parent) = entry.parent() {
                        let has_manifest = std::fs::read_dir(parent)
                            .map(|entries| {
                                entries
                                    .filter_map(|e| e.ok())
                                    .any(|e| e.path().extension().is_some_and(|ext| ext == "toml"))
                            })
                            .unwrap_or(false);
                        if has_manifest {
                            continue;
                        }
                    }
                    lint_file(&entry, &linter, &mut all_diagnostics);
                    files_checked += 1;
                }
            }
        } else if path.exists() {
            lint_file(path, &linter, &mut all_diagnostics);
            files_checked += 1;
        } else {
            eprintln!("Warning: {} does not exist", path.display());
        }
    }

    // Filter if errors_only
    if errors_only {
        all_diagnostics.retain(|d| d.severity == lint::Severity::Error);
    }

    // Print results
    if all_diagnostics.is_empty() {
        println!("No lint issues found in {} file(s)", files_checked);
    } else {
        print!("{}", lint::format_diagnostics(&all_diagnostics));

        let error_count = all_diagnostics
            .iter()
            .filter(|d| d.severity == lint::Severity::Error)
            .count();
        let warning_count = all_diagnostics
            .iter()
            .filter(|d| d.severity == lint::Severity::Warning)
            .count();
        let hint_count = all_diagnostics
            .iter()
            .filter(|d| d.severity == lint::Severity::Hint)
            .count();

        let issue_count = error_count + warning_count;
        let files_with_issues: std::collections::HashSet<_> = all_diagnostics
            .iter()
            .filter(|d| d.severity != lint::Severity::Hint)
            .map(|d| &d.file)
            .collect();

        if issue_count > 0 {
            println!(
                "\n{} issue(s) in {} file(s) ({} file(s) checked)",
                issue_count,
                files_with_issues.len(),
                files_checked
            );
        }
        if hint_count > 0 {
            println!("{} hint(s) ({} file(s) checked)", hint_count, files_checked);
        }
        if issue_count == 0 && hint_count > 0 {
            println!(
                "\nNo errors or warnings ({} file(s) checked)",
                files_checked
            );
        }

        // Exit with error if there are any errors, or any warnings when --deny-warnings is set
        if error_count > 0 || (deny_warnings && warning_count > 0) {
            process::exit(1);
        }
    }
}

fn lint_file(path: &PathBuf, linter: &seqc::Linter, diagnostics: &mut Vec<seqc::LintDiagnostic>) {
    use seqc::{
        ErrorFlagAnalyzer, Parser, ProgramResourceAnalyzer, TypeChecker, call_graph, lint,
        resolver::Resolver,
    };
    use std::fs;

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {}", path.display(), e);
            return;
        }
    };

    let mut parser = Parser::new(&source);
    let mut program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Parse error in {}: {}", path.display(), e);
            return;
        }
    };

    // Generate ADT constructors
    if let Err(e) = program.generate_constructors() {
        eprintln!("Constructor error in {}: {}", path.display(), e);
        return;
    }

    // Phase 1: Pattern-based linting
    let file_diagnostics = linter.lint_program(&program, path);
    diagnostics.extend(file_diagnostics);

    // Phase 2a: Resource leak detection with cross-word analysis
    let mut resource_analyzer = ProgramResourceAnalyzer::new(path);
    let resource_diagnostics = resource_analyzer.analyze_program(&program);
    diagnostics.extend(resource_diagnostics);

    // Phase 2b: Error flag tracking (unchecked Bool from fallible operations)
    let mut flag_analyzer = ErrorFlagAnalyzer::new(path);
    let flag_diagnostics = flag_analyzer.analyze_program(&program);
    diagnostics.extend(flag_diagnostics);

    // Phase 3: Type checking (catches stack underflows, effect mismatches, etc.)
    // Resolve includes to get external words, then type check the merged program
    let mut resolver = Resolver::new(None);
    let mut resolved = match resolver.resolve(path, program) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Include resolution error in {}: {}", path.display(), e);
            return;
        }
    };

    // Skip type checking for files with FFI dependencies (require --ffi-manifest to compile)
    if !resolved.ffi_includes.is_empty() {
        // FFI files can't be fully type-checked without loading their manifests
        // Pattern linting and resource analysis are still performed above
        return;
    }

    // Generate ADT constructors for the merged program (includes may have unions)
    if let Err(e) = resolved.program.generate_constructors() {
        eprintln!("Constructor error in {}: {}", path.display(), e);
        return;
    }

    // Mirror the main compile pipeline: lower literal-quotation `if`
    // triples to `Statement::If` so the typechecker sees the same shape
    // it sees in `seqc build`. Without this, the lint pass would reject
    // patterns the build accepts (divergent branches, aux-using bodies,
    // etc.) — see `normalize.rs`.
    seqc::normalize::lower_literal_if_combinators(&mut resolved.program);

    // Mirror the main compile pipeline: convert any `Type::Var("UnionName")`
    // produced by parsing one file before its union-defining file is included
    // into proper `Type::Union(name)` references. Without this, `lint-seq`
    // sees cross-file unions as suspect type variables and the validator
    // rejects them (e.g. `TokenList` defined in tokenizer.seq used in
    // parser.seq).
    resolved.program.fixup_union_types();

    let call_graph = call_graph::CallGraph::build(&resolved.program);
    let mut type_checker = TypeChecker::new();
    type_checker.set_call_graph(call_graph);

    if let Err(e) = type_checker.check_program(&resolved.program) {
        // Convert type error to lint diagnostic
        diagnostics.push(lint::LintDiagnostic {
            id: "type-error".to_string(),
            severity: lint::Severity::Error,
            message: e,
            file: path.clone(),
            line: 0, // Line info is now in the error message itself
            start_column: None,
            end_line: None,
            end_column: None,
            word_name: String::new(),
            start_index: 0,
            end_index: 0,
            replacement: String::new(),
        });
    }
}

/// Simple recursive directory walker with error logging
fn walkdir(dir: &Path) -> Vec<PathBuf> {
    use std::fs;

    let mut files = Vec::new();
    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();
                        if path.is_dir() {
                            files.extend(walkdir(&path));
                        } else {
                            files.push(path);
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not read directory entry in {}: {}",
                            dir.display(),
                            e
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Warning: Could not read directory {}: {}", dir.display(), e);
        }
    }
    files
}
