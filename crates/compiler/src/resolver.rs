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
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Result of resolving includes
pub struct ResolveResult {
    /// The resolved program with all includes merged
    pub program: Program,
    /// FFI library names that were included (e.g., ["readline"])
    pub ffi_includes: Vec<String>,
    /// Filesystem .seq files that were included (for cache invalidation)
    pub source_files: Vec<PathBuf>,
    /// Embedded stdlib modules that were included
    pub embedded_modules: Vec<String>,
}

/// Words and unions collected from a resolved include
struct ResolvedContent {
    words: Vec<WordDef>,
    unions: Vec<UnionDef>,
}

impl ResolvedContent {
    /// An include that contributed no words or unions.
    fn empty() -> Self {
        ResolvedContent {
            words: Vec::new(),
            unions: Vec::new(),
        }
    }
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
}

impl Resolver {
    /// Create a new resolver with an optional stdlib path for filesystem fallback
    pub fn new(stdlib_path: Option<PathBuf>) -> Self {
        Resolver {
            included_files: HashSet::new(),
            included_embedded: HashSet::new(),
            stdlib_path,
            ffi_includes: Vec::new(),
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

        let source_dir = source_path.parent().unwrap_or(Path::new("."));
        let mut all_words = Vec::new();
        let mut all_unions = Vec::new();

        for mut word in program.words {
            stamp_source(&mut word.source, &source_path);
            all_words.push(word);
        }

        for mut union_def in program.unions {
            stamp_source(&mut union_def.source, &source_path);
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
            source_files: self.included_files.iter().cloned().collect(),
            embedded_modules: self.included_embedded.iter().cloned().collect(),
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
            return Ok(ResolvedContent::empty());
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
            return Ok(ResolvedContent::empty());
        }
        self.included_embedded.insert(name.to_string());

        // Parse the embedded content
        let mut parser = Parser::new(content);
        let included_program = parser
            .parse()
            .map_err(|e| format!("Failed to parse embedded module '{}': {}", name, e))?;

        // Create a pseudo-path for source locations
        let pseudo_path = PathBuf::from(format!("<stdlib:{}>", name));

        // Collect words and unions with updated source locations
        let mut all_words = Vec::new();
        for mut word in included_program.words {
            stamp_source(&mut word.source, &pseudo_path);
            all_words.push(word);
        }

        let mut all_unions = Vec::new();
        for mut union_def in included_program.unions {
            stamp_source(&mut union_def.source, &pseudo_path);
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
            return Ok(ResolvedContent::empty());
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
        // Validate non-empty path
        if rel_path.is_empty() {
            return Err("Include path cannot be empty".to_string());
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

        // Canonicalize to resolve symlinks and normalize the path
        let canonical_path = path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve include path '{}': {}", rel_path, e))?;

        Ok(canonical_path)
    }
}

/// Stamp `file` onto a definition's source location, creating one at line 0
/// if the definition has none yet.
fn stamp_source(source: &mut Option<SourceLocation>, file: &Path) {
    match source {
        Some(loc) => loc.file = file.to_path_buf(),
        None => *source = Some(SourceLocation::new(file.to_path_buf(), 0)),
    }
}

mod helpers;

#[cfg(test)]
mod tests;

pub use helpers::{check_collisions, check_union_collisions, find_stdlib};
