//! Include Resolver for Seq
//!
//! Resolves include statements, loads and parses included files,
//! and merges everything into a single Program.
//!
//! Supports:
//! - `include std:name` - loads from stdlib directory
//! - `include "path"` - loads relative to current file

use crate::ast::{Include, Program, SourceLocation, WordDef};
use crate::parser::Parser;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Resolver for include statements
pub struct Resolver {
    /// Set of files already included (canonical paths to prevent double-include)
    included: HashSet<PathBuf>,
    /// Path to stdlib directory
    stdlib_path: PathBuf,
}

impl Resolver {
    /// Create a new resolver with the given stdlib path
    pub fn new(stdlib_path: PathBuf) -> Self {
        Resolver {
            included: HashSet::new(),
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
        self.included.insert(source_path.clone());

        // Add source location to all words in main program
        let source_dir = source_path.parent().unwrap_or(Path::new("."));
        let mut all_words = Vec::new();

        for mut word in program.words {
            // Set source location if not already set
            if word.source.is_none() {
                word.source = Some(SourceLocation {
                    file: source_path.clone(),
                    line: 0, // TODO: Track actual line numbers
                });
            }
            all_words.push(word);
        }

        // Process includes
        for include in &program.includes {
            let included_path = self.resolve_include_path(include, source_dir)?;

            // Skip if already included (prevents diamond dependency issues)
            let canonical = included_path.canonicalize().map_err(|e| {
                format!("Failed to canonicalize {}: {}", included_path.display(), e)
            })?;

            if self.included.contains(&canonical) {
                continue;
            }

            // Read and parse the included file
            let content = std::fs::read_to_string(&included_path)
                .map_err(|e| format!("Failed to read {}: {}", included_path.display(), e))?;

            let mut parser = Parser::new(&content);
            let included_program = parser.parse()?;

            // Recursively resolve includes in the included file
            let resolved = self.resolve(&included_path, included_program)?;

            // Add all words from the resolved program
            all_words.extend(resolved.words);
        }

        Ok(Program {
            includes: Vec::new(), // Includes are resolved, no longer needed
            words: all_words,
        })
    }

    /// Resolve an include to a file path
    fn resolve_include_path(
        &self,
        include: &Include,
        source_dir: &Path,
    ) -> Result<PathBuf, String> {
        match include {
            Include::Std(name) => {
                let path = self.stdlib_path.join(format!("{}.seq", name));
                if !path.exists() {
                    return Err(format!(
                        "Standard library module '{}' not found at {}",
                        name,
                        path.display()
                    ));
                }
                Ok(path)
            }
            Include::Relative(rel_path) => {
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

    Err(
        "Could not find stdlib directory. Set SEQ_STDLIB environment variable \
         or ensure stdlib/ exists in the project root."
            .to_string(),
    )
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
                source: Some(SourceLocation {
                    file: PathBuf::from("a.seq"),
                    line: 1,
                }),
            },
            WordDef {
                name: "bar".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation {
                    file: PathBuf::from("b.seq"),
                    line: 1,
                }),
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
                source: Some(SourceLocation {
                    file: PathBuf::from("a.seq"),
                    line: 1,
                }),
            },
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation {
                    file: PathBuf::from("b.seq"),
                    line: 5,
                }),
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
                source: Some(SourceLocation {
                    file: PathBuf::from("a.seq"),
                    line: 1,
                }),
            },
            WordDef {
                name: "foo".to_string(),
                effect: None,
                body: vec![],
                source: Some(SourceLocation {
                    file: PathBuf::from("a.seq"),
                    line: 5,
                }),
            },
        ];

        // This IS a collision - same name defined twice
        let result = check_collisions(&words);
        assert!(result.is_err());
    }
}
