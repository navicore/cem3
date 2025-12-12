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
use std::path::PathBuf;

/// Definition of an external builtin function
///
/// External builtins are functions provided by a runtime extension
/// (like an actor system) that should be callable from Seq code.
///
/// # Type Safety
///
/// External builtins can optionally specify their stack effect for type checking.
/// This affects how the type checker validates code using the builtin:
///
/// - **With effect**: The type checker enforces the declared signature.
///   Use this when you know the exact stack effect (recommended).
///
/// - **Without effect (`None`)**: The type checker assigns a maximally polymorphic
///   signature `( ..a -- ..b )`, meaning "accepts any stack, produces any stack".
///   This **disables type checking** for calls to this builtin - type errors
///   involving this builtin will only be caught at runtime.
///
/// For best type safety, always provide an explicit effect when possible.
#[derive(Debug, Clone)]
pub struct ExternalBuiltin {
    /// The name used in Seq code (e.g., "journal-append")
    pub seq_name: String,

    /// The symbol name for linking (e.g., "seq_actors_journal_append")
    ///
    /// Must contain only alphanumeric characters, underscores, and periods.
    /// This is validated at construction time to prevent LLVM IR injection.
    pub symbol: String,

    /// Optional stack effect for type checking.
    ///
    /// - `Some(effect)`: Type checker enforces this signature
    /// - `None`: Type checker uses maximally polymorphic `( ..a -- ..b )`,
    ///   effectively disabling type checking for this builtin
    ///
    /// **Warning**: Using `None` can hide type errors until runtime.
    /// Prefer providing an explicit effect when the signature is known.
    pub effect: Option<Effect>,
}

impl ExternalBuiltin {
    /// Validate that a symbol name is safe for LLVM IR
    ///
    /// Valid symbols contain only: alphanumeric characters, underscores, and periods.
    /// This prevents injection of arbitrary LLVM IR directives.
    fn validate_symbol(symbol: &str) -> Result<(), String> {
        if symbol.is_empty() {
            return Err("Symbol name cannot be empty".to_string());
        }
        for c in symbol.chars() {
            if !c.is_alphanumeric() && c != '_' && c != '.' {
                return Err(format!(
                    "Invalid character '{}' in symbol '{}'. \
                     Symbols may only contain alphanumeric characters, underscores, and periods.",
                    c, symbol
                ));
            }
        }
        Ok(())
    }

    /// Create a new external builtin with just name and symbol
    ///
    /// # Panics
    ///
    /// Panics if the symbol contains invalid characters for LLVM IR.
    /// Valid symbols contain only alphanumeric characters, underscores, and periods.
    pub fn new(seq_name: impl Into<String>, symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        Self::validate_symbol(&symbol).expect("Invalid symbol name");
        ExternalBuiltin {
            seq_name: seq_name.into(),
            symbol,
            effect: None,
        }
    }

    /// Create a new external builtin with a stack effect
    ///
    /// # Panics
    ///
    /// Panics if the symbol contains invalid characters for LLVM IR.
    pub fn with_effect(
        seq_name: impl Into<String>,
        symbol: impl Into<String>,
        effect: Effect,
    ) -> Self {
        let symbol = symbol.into();
        Self::validate_symbol(&symbol).expect("Invalid symbol name");
        ExternalBuiltin {
            seq_name: seq_name.into(),
            symbol,
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

    /// External FFI manifest paths to load
    ///
    /// These manifests are loaded in addition to any `include ffi:*` statements
    /// in the source code. Use this to provide custom FFI bindings without
    /// embedding them in the compiler.
    pub ffi_manifest_paths: Vec<PathBuf>,
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

    /// Add an external FFI manifest path
    ///
    /// The manifest will be loaded and its functions made available
    /// during compilation, in addition to any `include ffi:*` statements.
    pub fn with_ffi_manifest(mut self, path: impl Into<PathBuf>) -> Self {
        self.ffi_manifest_paths.push(path.into());
        self
    }

    /// Add multiple external FFI manifest paths
    pub fn with_ffi_manifests(mut self, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        self.ffi_manifest_paths.extend(paths);
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

    #[test]
    fn test_symbol_validation_valid() {
        // Valid symbols: alphanumeric, underscores, periods
        let _ = ExternalBuiltin::new("test", "valid_symbol");
        let _ = ExternalBuiltin::new("test", "valid.symbol.123");
        let _ = ExternalBuiltin::new("test", "ValidCamelCase");
        let _ = ExternalBuiltin::new("test", "seq_actors_journal_append");
    }

    #[test]
    #[should_panic(expected = "Invalid symbol name")]
    fn test_symbol_validation_rejects_hyphen() {
        // Hyphens are not valid in LLVM symbols
        let _ = ExternalBuiltin::new("test", "invalid-symbol");
    }

    #[test]
    #[should_panic(expected = "Invalid symbol name")]
    fn test_symbol_validation_rejects_at() {
        // @ could be used for LLVM IR injection
        let _ = ExternalBuiltin::new("test", "@malicious");
    }

    #[test]
    #[should_panic(expected = "Invalid symbol name")]
    fn test_symbol_validation_rejects_empty() {
        let _ = ExternalBuiltin::new("test", "");
    }
}
