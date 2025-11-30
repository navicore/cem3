//! Compiler configuration for extensibility
//!
//! This module provides configuration types that allow external projects
//! to extend the Seq compiler with additional builtins without modifying
//! the core compiler.
//!
//! # Example
//!
//! ```rust,ignore
//! use seqc::{CompilerConfig, ExternalBuiltin};
//!
//! // Define builtins provided by your runtime extension
//! let config = CompilerConfig::new()
//!     .with_builtin(ExternalBuiltin::new(
//!         "journal-append",
//!         "my_runtime_journal_append",
//!     ))
//!     .with_builtin(ExternalBuiltin::new(
//!         "actor-send",
//!         "my_runtime_actor_send",
//!     ));
//!
//! // Compile with extended builtins
//! compile_file_with_config(source_path, output_path, false, &config)?;
//! ```

use crate::types::Effect;

/// Definition of an external builtin function
///
/// External builtins are functions provided by a runtime extension
/// (like an actor system) that should be callable from Seq code.
#[derive(Debug, Clone)]
pub struct ExternalBuiltin {
    /// The name used in Seq code (e.g., "journal-append")
    pub seq_name: String,

    /// The symbol name for linking (e.g., "seq_actors_journal_append")
    pub symbol: String,

    /// Optional stack effect for type checking
    /// If None, the builtin is treated as having unknown effect
    pub effect: Option<Effect>,
}

impl ExternalBuiltin {
    /// Create a new external builtin with just name and symbol
    pub fn new(seq_name: impl Into<String>, symbol: impl Into<String>) -> Self {
        ExternalBuiltin {
            seq_name: seq_name.into(),
            symbol: symbol.into(),
            effect: None,
        }
    }

    /// Create a new external builtin with a stack effect
    pub fn with_effect(
        seq_name: impl Into<String>,
        symbol: impl Into<String>,
        effect: Effect,
    ) -> Self {
        ExternalBuiltin {
            seq_name: seq_name.into(),
            symbol: symbol.into(),
            effect: Some(effect),
        }
    }
}

/// Configuration for the Seq compiler
///
/// Allows external projects to extend the compiler with additional
/// builtins and configuration options.
#[derive(Debug, Clone, Default)]
pub struct CompilerConfig {
    /// External builtins to include in compilation
    pub external_builtins: Vec<ExternalBuiltin>,

    /// Additional library paths for linking
    pub library_paths: Vec<String>,

    /// Additional libraries to link
    pub libraries: Vec<String>,
}

impl CompilerConfig {
    /// Create a new empty configuration
    pub fn new() -> Self {
        CompilerConfig::default()
    }

    /// Add an external builtin (builder pattern)
    pub fn with_builtin(mut self, builtin: ExternalBuiltin) -> Self {
        self.external_builtins.push(builtin);
        self
    }

    /// Add multiple external builtins
    pub fn with_builtins(mut self, builtins: impl IntoIterator<Item = ExternalBuiltin>) -> Self {
        self.external_builtins.extend(builtins);
        self
    }

    /// Add a library path for linking
    pub fn with_library_path(mut self, path: impl Into<String>) -> Self {
        self.library_paths.push(path.into());
        self
    }

    /// Add a library to link
    pub fn with_library(mut self, lib: impl Into<String>) -> Self {
        self.libraries.push(lib.into());
        self
    }

    /// Get seq names of all external builtins (for AST validation)
    pub fn external_names(&self) -> Vec<&str> {
        self.external_builtins
            .iter()
            .map(|b| b.seq_name.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_external_builtin_new() {
        let builtin = ExternalBuiltin::new("my-func", "runtime_my_func");
        assert_eq!(builtin.seq_name, "my-func");
        assert_eq!(builtin.symbol, "runtime_my_func");
        assert!(builtin.effect.is_none());
    }

    #[test]
    fn test_config_builder() {
        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("func-a", "sym_a"))
            .with_builtin(ExternalBuiltin::new("func-b", "sym_b"))
            .with_library_path("/custom/lib")
            .with_library("myruntime");

        assert_eq!(config.external_builtins.len(), 2);
        assert_eq!(config.library_paths, vec!["/custom/lib"]);
        assert_eq!(config.libraries, vec!["myruntime"]);
    }

    #[test]
    fn test_external_names() {
        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("func-a", "sym_a"))
            .with_builtin(ExternalBuiltin::new("func-b", "sym_b"));

        let names = config.external_names();
        assert_eq!(names, vec!["func-a", "func-b"]);
    }
}
