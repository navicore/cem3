//! Include Resolver for Seq
//!
//! Resolves include statements, loads and parses included files,
//! and merges everything into a single Program.
//!
//! Supports:
//! - `include std:name` - loads from embedded stdlib (or filesystem fallback)
//! - `include ffi:name` - loads FFI manifest (collected but not processed here)
//! - `include "path"` - loads relative to current file

use crate::ast::{Include, Program, SourceLocation, UnionDef, WordDef};
use crate::parser::Parser;
use crate::stdlib_embed;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Result of resolving includes
pub struct ResolveResult {
    /// The resolved program with all includes merged
    pub program: Program,
    /// FFI library names that were included (e.g., ["readline"])
    pub ffi_includes: Vec<String>,
}

/// Words and unions collected from a resolved include
struct ResolvedContent {
    words: Vec<WordDef>,
    unions: Vec<UnionDef>,
}

/// Result of resolving an include - either embedded content or a file path
#[derive(Debug)]
enum ResolvedInclude {
    /// Embedded stdlib content (name, content)
    Embedded(String, &'static str),
    /// File system path
    FilePath(PathBuf),
}

/// Resolver for include statements
pub struct Resolver {
    /// Set of files already included (canonical paths to prevent double-include)
    included_files: HashSet<PathBuf>,
    /// Set of embedded stdlib modules already included
    included_embedded: HashSet<String>,
    /// Path to stdlib directory (fallback for non-embedded modules), if available
    stdlib_path: Option<PathBuf>,
    /// FFI libraries that were included
    ffi_includes: Vec<String>,
    /// Project root directory (directory of the main source file)
    /// All includes must resolve within this directory tree
    project_root: Option<PathBuf>,
}

impl Resolver {
    /// Create a new resolver with an optional stdlib path for filesystem fallback
    pub fn new(stdlib_path: Option<PathBuf>) -> Self {
        Resolver {
            included_files: HashSet::new(),
            included_embedded: HashSet::new(),
            stdlib_path,
            ffi_includes: Vec::new(),
            project_root: None,
        }
    }

    /// Resolve all includes in a program and return a merged program with FFI includes
    ///
    /// Takes the source file path and its already-parsed program.
    /// Recursively resolves includes and merges all word and union definitions.
    /// FFI includes are collected but not processed (they don't produce words/unions).
    pub fn resolve(
        &mut self,
        source_path: &Path,
        program: Program,
    ) -> Result<ResolveResult, String> {
        let source_path = source_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize {}: {}", source_path.display(), e))?;

        // Mark this file as included
        self.included_files.insert(source_path.clone());

        // Set project root on first call (main source file's directory)
        // All includes must resolve within this directory tree
        let source_dir = source_path.parent().unwrap_or(Path::new("."));
        if self.project_root.is_none() {
            self.project_root = Some(source_dir.to_path_buf());
        }
        let mut all_words = Vec::new();
        let mut all_unions = Vec::new();

        for mut word in program.words {
            // Update source location with file path
            if let Some(ref mut source) = word.source {
                source.file = source_path.clone();
            } else {
                word.source = Some(SourceLocation::new(source_path.clone(), 0));
            }
            all_words.push(word);
        }

        for mut union_def in program.unions {
            // Update source location with file path
            if let Some(ref mut source) = union_def.source {
                source.file = source_path.clone();
            } else {
                union_def.source = Some(SourceLocation::new(source_path.clone(), 0));
            }
            all_unions.push(union_def);
        }

        // Process includes
        for include in &program.includes {
            let content = self.process_include(include, source_dir)?;
            all_words.extend(content.words);
            all_unions.extend(content.unions);
        }

        let resolved_program = Program {
            includes: Vec::new(), // Includes are resolved, no longer needed
            unions: all_unions,
            words: all_words,
        };

        // Note: Constructor generation is done in lib.rs after resolution
        // to keep all constructor generation in one place

        Ok(ResolveResult {
            program: resolved_program,
            ffi_includes: std::mem::take(&mut self.ffi_includes),
        })
    }

    /// Process a single include and return the resolved words and unions
    fn process_include(
        &mut self,
        include: &Include,
        source_dir: &Path,
    ) -> Result<ResolvedContent, String> {
        // Handle FFI includes specially - they don't produce words/unions,
        // they're collected for later processing by the FFI system
        if let Include::Ffi(name) = include {
            // Check if we have the FFI manifest
            if !crate::ffi::has_ffi_manifest(name) {
                return Err(format!(
                    "FFI library '{}' not found. Available: {}",
                    name,
                    crate::ffi::list_ffi_manifests().join(", ")
                ));
            }
            // Avoid duplicate FFI includes
            if !self.ffi_includes.contains(name) {
                self.ffi_includes.push(name.clone());
            }
            // FFI includes don't add words/unions directly
            return Ok(ResolvedContent {
                words: Vec::new(),
                unions: Vec::new(),
            });
        }

        let resolved = self.resolve_include(include, source_dir)?;

        match resolved {
            ResolvedInclude::Embedded(name, content) => {
                self.process_embedded_include(&name, content, source_dir)
            }
            ResolvedInclude::FilePath(path) => self.process_file_include(&path),
        }
    }

    /// Process an embedded stdlib include
    fn process_embedded_include(
        &mut self,
        name: &str,
        content: &str,
        source_dir: &Path,
    ) -> Result<ResolvedContent, String> {
        // Skip if already included
        if self.included_embedded.contains(name) {
            return Ok(ResolvedContent {
                words: Vec::new(),
                unions: Vec::new(),
            });
        }
        self.included_embedded.insert(name.to_string());

        // Parse the embedded content
        let mut parser = Parser::new(content);
        let included_program = parser
            .parse()
            .map_err(|e| format!("Failed to parse embedded module '{}': {}", name, e))?;

        // Create a pseudo-path for source locations
        let pseudo_path = PathBuf::from(format!("<stdlib:{}>", name));

        // Collect words with updated source locations
        let mut all_words = Vec::new();
        for mut word in included_program.words {
            if let Some(ref mut source) = word.source {
                source.file = pseudo_path.clone();
            } else {
                word.source = Some(SourceLocation::new(pseudo_path.clone(), 0));
            }
            all_words.push(word);
        }

        // Collect unions with updated source locations
        let mut all_unions = Vec::new();
        for mut union_def in included_program.unions {
            if let Some(ref mut source) = union_def.source {
                source.file = pseudo_path.clone();
            } else {
                union_def.source = Some(SourceLocation::new(pseudo_path.clone(), 0));
            }
            all_unions.push(union_def);
        }

        // Recursively process includes from embedded module
        for include in &included_program.includes {
            let content = self.process_include(include, source_dir)?;
            all_words.extend(content.words);
            all_unions.extend(content.unions);
        }

        Ok(ResolvedContent {
            words: all_words,
            unions: all_unions,
        })
    }

    /// Process a filesystem include
    fn process_file_include(&mut self, path: &Path) -> Result<ResolvedContent, String> {
        // Skip if already included (prevents diamond dependency issues)
        let canonical = path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize {}: {}", path.display(), e))?;

        if self.included_files.contains(&canonical) {
            return Ok(ResolvedContent {
                words: Vec::new(),
                unions: Vec::new(),
            });
        }

        // Read and parse the included file
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

        let mut parser = Parser::new(&content);
        let included_program = parser.parse()?;

        // Recursively resolve includes in the included file
        let resolved = self.resolve(path, included_program)?;

        Ok(ResolvedContent {
            words: resolved.program.words,
            unions: resolved.program.unions,
        })
    }

    /// Resolve an include to either embedded content or a file path
    fn resolve_include(
        &self,
        include: &Include,
        source_dir: &Path,
    ) -> Result<ResolvedInclude, String> {
        match include {
            Include::Std(name) => {
                // Check embedded stdlib first
                if let Some(content) = stdlib_embed::get_stdlib(name) {
                    return Ok(ResolvedInclude::Embedded(name.clone(), content));
                }

                // Fall back to filesystem if stdlib_path is available
                if let Some(ref stdlib_path) = self.stdlib_path {
                    let path = stdlib_path.join(format!("{}.seq", name));
                    if path.exists() {
                        return Ok(ResolvedInclude::FilePath(path));
                    }
                }

                // Not found anywhere
                Err(format!(
                    "Standard library module '{}' not found (not embedded{})",
                    name,
                    if self.stdlib_path.is_some() {
                        " and not in stdlib directory"
                    } else {
                        ""
                    }
                ))
            }
            Include::Relative(rel_path) => Ok(ResolvedInclude::FilePath(
                self.resolve_relative_path(rel_path, source_dir)?,
            )),
            Include::Ffi(_) => {
                // FFI includes are handled separately in process_include
                unreachable!("FFI includes should be handled before resolve_include is called")
            }
        }
    }

    /// Resolve a relative include path to a file path
    ///
    /// Paths can contain `..` to reference parent directories, but the resolved
    /// path must stay within the project root (main source file's directory).
    fn resolve_relative_path(&self, rel_path: &str, source_dir: &Path) -> Result<PathBuf, String> {
        // Cross-platform absolute path detection
        let rel_as_path = std::path::Path::new(rel_path);
        if rel_as_path.is_absolute() {
            return Err(format!(
                "Include path '{}' is invalid: paths cannot be absolute",
                rel_path
            ));
        }

        let path = source_dir.join(format!("{}.seq", rel_path));
        if !path.exists() {
            return Err(format!(
                "Include file '{}' not found at {}",
                rel_path,
                path.display()
            ));
        }

        // Security: Verify resolved path is within project root
        // This catches any bypass attempts (symlinks, encoded paths, etc.)
        // while allowing legitimate cross-directory includes within the project
        let canonical_path = path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve include path '{}': {}", rel_path, e))?;

        // Use project root for containment check (falls back to source_dir if not set)
        let source_dir_buf = source_dir.to_path_buf();
        let root = self.project_root.as_ref().unwrap_or(&source_dir_buf);
        let canonical_root = root
            .canonicalize()
            .map_err(|e| format!("Failed to resolve project root: {}", e))?;

        if !canonical_path.starts_with(&canonical_root) {
            return Err(format!(
                "Include path '{}' resolves outside the project directory",
                rel_path
            ));
        }

        Ok(canonical_path)
    }
}

/// Check for word name collisions across all definitions
///
/// Returns an error with helpful message if any word is defined multiple times.
pub fn check_collisions(words: &[WordDef]) -> Result<(), String> {
    let mut definitions: HashMap<&str, Vec<&SourceLocation>> = HashMap::new();

    for word in words {
        if let Some(ref source) = word.source {
            definitions.entry(&word.name).or_default().push(source);
        }
    }

    // Find collisions (words defined in multiple places)
    let mut errors = Vec::new();
    for (name, locations) in definitions {
        if locations.len() > 1 {
            let mut msg = format!("Word '{}' is defined multiple times:\n", name);
            for loc in &locations {
                msg.push_str(&format!("  - {}\n", loc));
            }
            msg.push_str("\nHint: Rename one of the definitions to avoid collision.");
            errors.push(msg);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n\n"))
    }
}

/// Check for union name collisions across all definitions
///
/// Returns an error with helpful message if any union is defined multiple times.
pub fn check_union_collisions(unions: &[UnionDef]) -> Result<(), String> {
    let mut definitions: HashMap<&str, Vec<&SourceLocation>> = HashMap::new();

    for union_def in unions {
        if let Some(ref source) = union_def.source {
            definitions.entry(&union_def.name).or_default().push(source);
        }
    }

    // Find collisions (unions defined in multiple places)
    let mut errors = Vec::new();
    for (name, locations) in definitions {
        if locations.len() > 1 {
            let mut msg = format!("Union '{}' is defined multiple times:\n", name);
            for loc in &locations {
                msg.push_str(&format!("  - {}\n", loc));
            }
            msg.push_str("\nHint: Rename one of the definitions to avoid collision.");
            errors.push(msg);
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n\n"))
    }
}

/// Find the stdlib directory for filesystem fallback
///
/// Searches in order:
/// 1. SEQ_STDLIB environment variable
/// 2. Relative to the current executable (for installed compilers)
/// 3. Relative to current directory (for development)
///
/// Returns None if no stdlib directory is found (embedded stdlib will be used).
pub fn find_stdlib() -> Option<PathBuf> {
    // Check environment variable first
    if let Ok(path) = std::env::var("SEQ_STDLIB") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Some(path);
        }
        // If SEQ_STDLIB is set but invalid, log warning but continue
        eprintln!(
            "Warning: SEQ_STDLIB is set to '{}' but that directory doesn't exist",
            path.display()
        );
    }

    // Check relative to executable
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let stdlib_path = exe_dir.join("stdlib");
        if stdlib_path.is_dir() {
            return Some(stdlib_path);
        }
        // Also check one level up (for development builds)
        if let Some(parent) = exe_dir.parent() {
            let stdlib_path = parent.join("stdlib");
            if stdlib_path.is_dir() {
                return Some(stdlib_path);
            }
        }
    }

    // Check relative to current directory (development)
    let local_stdlib = PathBuf::from("stdlib");
    if local_stdlib.is_dir() {
        return Some(local_stdlib.canonicalize().unwrap_or(local_stdlib));
    }

    // No filesystem stdlib found - that's OK, we have embedded stdlib
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collision_detection_no_collision() {
        let words = vec![
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("a.seq"), 1)),
            },
            WordDef {
                name: "bar".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("b.seq"), 1)),
            },
        ];

        assert!(check_collisions(&words).is_ok());
    }

    #[test]
    fn test_collision_detection_with_collision() {
        let words = vec![
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("a.seq"), 1)),
            },
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("b.seq"), 5)),
            },
        ];

        let result = check_collisions(&words);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("foo"));
        assert!(error.contains("a.seq"));
        assert!(error.contains("b.seq"));
        assert!(error.contains("multiple times"));
    }

    #[test]
    fn test_collision_detection_same_file_different_lines() {
        // Same word defined twice in same file on different lines
        // This is still a collision (parser would typically catch this earlier)
        let words = vec![
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("a.seq"), 1)),
            },
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation::new(PathBuf::from("a.seq"), 5)),
            },
        ];

        // This IS a collision - same name defined twice
        let result = check_collisions(&words);
        assert!(result.is_err());
    }

    // Integration tests for embedded stdlib

    #[test]
    fn test_embedded_stdlib_math_available() {
        assert!(stdlib_embed::has_stdlib("math"));
    }

    #[test]
    fn test_embedded_stdlib_resolution() {
        let resolver = Resolver::new(None);
        let include = Include::Std("math".to_string());
        let result = resolver.resolve_include(&include, Path::new("."));
        assert!(result.is_ok());
        match result.unwrap() {
            ResolvedInclude::Embedded(name, content) => {
                assert_eq!(name, "math");
                assert!(content.contains("abs"));
            }
            ResolvedInclude::FilePath(_) => panic!("Expected embedded, got file path"),
        }
    }

    #[test]
    fn test_nonexistent_stdlib_module() {
        let resolver = Resolver::new(None);
        let include = Include::Std("nonexistent".to_string());
        let result = resolver.resolve_include(&include, Path::new("."));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_resolver_with_no_stdlib_path() {
        // Resolver should work with None stdlib_path, using only embedded modules
        let resolver = Resolver::new(None);
        assert!(resolver.stdlib_path.is_none());
    }

    #[test]
    fn test_double_include_prevention_embedded() {
        let mut resolver = Resolver::new(None);

        // First include should work
        let result1 = resolver.process_embedded_include(
            "math",
            stdlib_embed::get_stdlib("math").unwrap(),
            Path::new("."),
        );
        assert!(result1.is_ok());
        let content1 = result1.unwrap();
        assert!(!content1.words.is_empty());

        // Second include of same module should return empty (already included)
        let result2 = resolver.process_embedded_include(
            "math",
            stdlib_embed::get_stdlib("math").unwrap(),
            Path::new("."),
        );
        assert!(result2.is_ok());
        let content2 = result2.unwrap();
        assert!(content2.words.is_empty());
        assert!(content2.unions.is_empty());
    }

    #[test]
    fn test_cross_directory_include_allowed() {
        // Test that ".." is allowed when the resolved path stays within the project root
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let project_root = temp.path();

        // Create directory structure:
        // project_root/
        //   src/
        //     lib/
        //       helper.seq
        //   tests/
        //     test_main.seq (wants to include ../src/lib/helper)
        let src = project_root.join("src");
        let src_lib = src.join("lib");
        let tests = project_root.join("tests");
        fs::create_dir_all(&src_lib).unwrap();
        fs::create_dir_all(&tests).unwrap();

        // Create helper.seq in src/lib
        fs::write(src_lib.join("helper.seq"), ": helper ( -- Int ) 42 ;\n").unwrap();

        // Set up resolver with project_root set to the temp directory
        let mut resolver = Resolver::new(None);
        resolver.project_root = Some(project_root.to_path_buf());

        // Resolve from tests directory: include ../src/lib/helper
        // This should work since the resolved path is within project_root
        let include = Include::Relative("../src/lib/helper".to_string());
        let result = resolver.resolve_include(&include, &tests);

        assert!(
            result.is_ok(),
            "Cross-directory include should succeed: {:?}",
            result.err()
        );

        match result.unwrap() {
            ResolvedInclude::FilePath(path) => {
                assert!(path.ends_with("helper.seq"));
            }
            ResolvedInclude::Embedded(_, _) => panic!("Expected file path, got embedded"),
        }
    }

    #[test]
    fn test_cross_directory_include_outside_project_rejected() {
        // Test that ".." is rejected when it would escape the project root
        use std::fs;
        use tempfile::tempdir;

        // Create a single temp directory with nested structure:
        // temp/
        //   outside/
        //     secret.seq   <- this is outside the project
        //   project/
        //     src/         <- this is the project root
        //       main.seq
        let temp = tempdir().unwrap();
        let outside = temp.path().join("outside");
        let project_src = temp.path().join("project").join("src");
        fs::create_dir_all(&outside).unwrap();
        fs::create_dir_all(&project_src).unwrap();

        // Create a file outside the project
        fs::write(outside.join("secret.seq"), ": secret ( -- ) ;\n").unwrap();

        // Set up resolver with project_root set to project/src directory
        let mut resolver = Resolver::new(None);
        resolver.project_root = Some(project_src.clone());

        // Try to include a file that is outside the project root
        // From project/src, "../../outside/secret" escapes the project
        let include = Include::Relative("../../outside/secret".to_string());
        let result = resolver.resolve_include(&include, &project_src);

        assert!(
            result.is_err(),
            "Include escaping project root should be rejected"
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("outside"),
            "Error should mention path escaping: {}",
            err_msg
        );
    }

    #[test]
    fn test_project_root_set_on_first_resolve() {
        // Verify that project_root is set when the first file is resolved
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let project_root = temp.path();
        let src = project_root.join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("main.seq"), ": main ( -- ) ;\n").unwrap();

        let mut resolver = Resolver::new(None);
        assert!(
            resolver.project_root.is_none(),
            "project_root should start as None"
        );

        // Parse and resolve a file
        let main_source = src.join("main.seq");
        let source_code = fs::read_to_string(&main_source).unwrap();
        let program = crate::parser::Parser::new(&source_code).parse().unwrap();
        let _ = resolver.resolve(&main_source, program);

        // project_root should now be set to the source file's directory
        assert!(
            resolver.project_root.is_some(),
            "project_root should be set after first resolve"
        );

        let pr = resolver.project_root.as_ref().unwrap();
        assert_eq!(
            pr.canonicalize().unwrap(),
            src.canonicalize().unwrap(),
            "project_root should match the source file's directory"
        );
    }

    #[test]
    fn test_dotdot_within_same_directory_structure() {
        // Test that "dir/../file" resolves correctly within the project
        use std::fs;
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let project = temp.path();

        // Create: project/a/b/c/ and project/a/target.seq
        let deep = project.join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();
        fs::write(project.join("a").join("target.seq"), ": target ( -- ) ;\n").unwrap();

        let mut resolver = Resolver::new(None);
        resolver.project_root = Some(project.to_path_buf());

        // From a/b/c, include ../../target should work
        let include = Include::Relative("../../target".to_string());
        let result = resolver.resolve_include(&include, &deep);

        assert!(
            result.is_ok(),
            "Include with .. staying in project should work: {:?}",
            result.err()
        );
    }
}
