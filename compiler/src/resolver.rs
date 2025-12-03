//! Include Resolver for Seq
//!
//! Resolves include statements, loads and parses included files,
//! and merges everything into a single Program.
//!
//! Supports:
//! - `include std:name` - loads from embedded stdlib (or filesystem fallback)
//! - `include "path"` - loads relative to current file

use crate::ast::{Include, Program, SourceLocation, WordDef};
use crate::parser::Parser;
use crate::stdlib_embed;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Result of resolving an include - either embedded content or a file path
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
    /// Path to stdlib directory (fallback for non-embedded modules)
    stdlib_path: PathBuf,
}

impl Resolver {
    /// Create a new resolver with the given stdlib path
    pub fn new(stdlib_path: PathBuf) -> Self {
        Resolver {
            included_files: HashSet::new(),
            included_embedded: HashSet::new(),
            stdlib_path,
        }
    }

    /// Resolve all includes in a program and return a merged program
    ///
    /// Takes the source file path and its already-parsed program.
    /// Recursively resolves includes and merges all word definitions.
    pub fn resolve(&mut self, source_path: &Path, program: Program) -> Result<Program, String> {
        let source_path = source_path
            .canonicalize()
            .map_err(|e| format!("Failed to canonicalize {}: {}", source_path.display(), e))?;

        // Mark this file as included
        self.included_files.insert(source_path.clone());

        // Add source location to all words in main program
        let source_dir = source_path.parent().unwrap_or(Path::new("."));
        let mut all_words = Vec::new();

        for mut word in program.words {
            // Update source location with file path
            if let Some(ref mut source) = word.source {
                source.file = source_path.clone();
            } else {
                word.source = Some(SourceLocation::new(source_path.clone(), 0));
            }
            all_words.push(word);
        }

        // Process includes
        for include in &program.includes {
            let resolved = self.resolve_include(include, source_dir)?;

            match resolved {
                ResolvedInclude::Embedded(name, content) => {
                    // Skip if already included
                    if self.included_embedded.contains(&name) {
                        continue;
                    }
                    self.included_embedded.insert(name.clone());

                    // Parse the embedded content
                    let mut parser = Parser::new(content);
                    let included_program = parser.parse()?;

                    // Create a pseudo-path for source locations
                    let pseudo_path = PathBuf::from(format!("<stdlib:{}>", name));

                    // Recursively resolve includes (embedded modules can include others)
                    let resolved_words =
                        self.resolve_embedded(&pseudo_path, included_program, source_dir)?;
                    all_words.extend(resolved_words);
                }
                ResolvedInclude::FilePath(included_path) => {
                    // Skip if already included (prevents diamond dependency issues)
                    let canonical = included_path.canonicalize().map_err(|e| {
                        format!("Failed to canonicalize {}: {}", included_path.display(), e)
                    })?;

                    if self.included_files.contains(&canonical) {
                        continue;
                    }

                    // Read and parse the included file
                    let content = std::fs::read_to_string(&included_path).map_err(|e| {
                        format!("Failed to read {}: {}", included_path.display(), e)
                    })?;

                    let mut parser = Parser::new(&content);
                    let included_program = parser.parse()?;

                    // Recursively resolve includes in the included file
                    let resolved = self.resolve(&included_path, included_program)?;

                    // Add all words from the resolved program
                    all_words.extend(resolved.words);
                }
            }
        }

        Ok(Program {
            includes: Vec::new(), // Includes are resolved, no longer needed
            words: all_words,
        })
    }

    /// Resolve includes in an embedded module
    fn resolve_embedded(
        &mut self,
        pseudo_path: &Path,
        program: Program,
        original_source_dir: &Path,
    ) -> Result<Vec<WordDef>, String> {
        let mut all_words = Vec::new();

        for mut word in program.words {
            // Set source location to the pseudo-path
            if let Some(ref mut source) = word.source {
                source.file = pseudo_path.to_path_buf();
            } else {
                word.source = Some(SourceLocation::new(pseudo_path.to_path_buf(), 0));
            }
            all_words.push(word);
        }

        // Process includes from embedded module
        for include in &program.includes {
            let resolved = self.resolve_include(include, original_source_dir)?;

            match resolved {
                ResolvedInclude::Embedded(name, content) => {
                    if self.included_embedded.contains(&name) {
                        continue;
                    }
                    self.included_embedded.insert(name.clone());

                    let mut parser = Parser::new(content);
                    let included_program = parser.parse()?;
                    let inner_pseudo_path = PathBuf::from(format!("<stdlib:{}>", name));
                    let resolved_words = self.resolve_embedded(
                        &inner_pseudo_path,
                        included_program,
                        original_source_dir,
                    )?;
                    all_words.extend(resolved_words);
                }
                ResolvedInclude::FilePath(included_path) => {
                    let canonical = included_path.canonicalize().map_err(|e| {
                        format!("Failed to canonicalize {}: {}", included_path.display(), e)
                    })?;

                    if self.included_files.contains(&canonical) {
                        continue;
                    }

                    let content = std::fs::read_to_string(&included_path).map_err(|e| {
                        format!("Failed to read {}: {}", included_path.display(), e)
                    })?;

                    let mut parser = Parser::new(&content);
                    let included_program = parser.parse()?;
                    let resolved = self.resolve(&included_path, included_program)?;
                    all_words.extend(resolved.words);
                }
            }
        }

        Ok(all_words)
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

                // Fall back to filesystem
                let path = self.stdlib_path.join(format!("{}.seq", name));
                if !path.exists() {
                    return Err(format!(
                        "Standard library module '{}' not found (not embedded and not at {})",
                        name,
                        path.display()
                    ));
                }
                Ok(ResolvedInclude::FilePath(path))
            }
            Include::Relative(rel_path) => Ok(ResolvedInclude::FilePath(
                self.resolve_relative_path(rel_path, source_dir)?,
            )),
        }
    }

    /// Resolve a relative include path to a file path
    fn resolve_relative_path(&self, rel_path: &str, source_dir: &Path) -> Result<PathBuf, String> {
        // Security: Early rejection of obviously malicious paths
        if rel_path.contains("..") {
            return Err(format!(
                "Include path '{}' is invalid: paths cannot contain '..'",
                rel_path
            ));
        }

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

        // Security: Verify resolved path is within source directory
        // This catches any bypass attempts (symlinks, encoded paths, etc.)
        let canonical_path = path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve include path '{}': {}", rel_path, e))?;
        let canonical_source = source_dir
            .canonicalize()
            .map_err(|e| format!("Failed to resolve source directory: {}", e))?;

        if !canonical_path.starts_with(&canonical_source) {
            return Err(format!(
                "Include path '{}' resolves outside the source directory",
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

/// Find the stdlib directory
///
/// Searches in order:
/// 1. SEQ_STDLIB environment variable
/// 2. Relative to the current executable (for installed compilers)
/// 3. Relative to current directory (for development)
/// 4. Returns a dummy path (embedded stdlib will be used)
///
/// Note: With embedded stdlib support, this function always succeeds.
/// The embedded stdlib is the primary source; filesystem is a fallback
/// for modules not in the embedded set.
pub fn find_stdlib() -> Result<PathBuf, String> {
    // Check environment variable first
    if let Ok(path) = std::env::var("SEQ_STDLIB") {
        let path = PathBuf::from(path);
        if path.is_dir() {
            return Ok(path);
        }
        return Err(format!(
            "SEQ_STDLIB is set to '{}' but that directory doesn't exist",
            path.display()
        ));
    }

    // Check relative to executable
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        let stdlib_path = exe_dir.join("stdlib");
        if stdlib_path.is_dir() {
            return Ok(stdlib_path);
        }
        // Also check one level up (for development builds)
        if let Some(parent) = exe_dir.parent() {
            let stdlib_path = parent.join("stdlib");
            if stdlib_path.is_dir() {
                return Ok(stdlib_path);
            }
        }
    }

    // Check relative to current directory (development)
    let local_stdlib = PathBuf::from("stdlib");
    if local_stdlib.is_dir() {
        return Ok(local_stdlib.canonicalize().unwrap_or(local_stdlib));
    }

    // No filesystem stdlib found - that's OK, we have embedded stdlib
    // Return a non-existent path; the resolver will use embedded modules
    Ok(PathBuf::from("<embedded-stdlib>"))
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
}
