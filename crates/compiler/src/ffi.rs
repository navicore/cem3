//! FFI (Foreign Function Interface) Support
//!
//! This module handles parsing of FFI manifests and generating the LLVM IR
//! for calling external C functions from Seq code.
//!
//! FFI is purely a compiler/linker concern - the runtime remains free of
//! external dependencies.
//!
//! # Usage
//!
//! ```seq
//! include ffi:readline
//!
//! : repl ( -- )
//!   "prompt> " readline
//!   dup string-empty not if
//!     dup add-history
//!     process-input
//!     repl
//!   else
//!     drop
//!   then
//! ;
//! ```

use crate::types::{Effect, StackType, Type};
use serde::Deserialize;
use std::collections::HashMap;

/// FFI type mapping for C interop
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FfiType {
    /// C int/long mapped to Seq Int (i64)
    Int,
    /// C char* mapped to Seq String
    String,
    /// C void* as raw pointer (represented as Int)
    Ptr,
    /// C void - no return value
    Void,
    /// Callback function pointer (requires `callback` field to reference callback definition)
    Callback,
}

/// Argument passing mode
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PassMode {
    /// Convert Seq String to null-terminated char*
    CString,
    /// Pass raw pointer value
    Ptr,
    /// Pass as C integer
    Int,
    /// Pass pointer to value (for out parameters)
    ByRef,
}

/// Memory ownership annotation for return values
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Ownership {
    /// C function allocated memory, caller must free
    CallerFrees,
    /// Library owns the memory, don't free
    Static,
    /// Valid only during call, copy immediately
    Borrowed,
}

/// An argument to an FFI function
#[derive(Debug, Clone, Deserialize)]
pub struct FfiArg {
    /// The type of the argument
    #[serde(rename = "type")]
    pub arg_type: FfiType,
    /// How to pass the argument to C
    #[serde(default = "default_pass_mode")]
    pub pass: PassMode,
    /// Fixed value (for parameters like NULL callbacks)
    pub value: Option<String>,
    /// Reference to a callback type (when type = "callback")
    pub callback: Option<String>,
}

/// An argument in a callback signature
#[derive(Debug, Clone, Deserialize)]
pub struct FfiCallbackArg {
    /// The type of the argument
    #[serde(rename = "type")]
    pub arg_type: FfiType,
    /// Optional name for documentation
    pub name: Option<String>,
}

/// A callback type definition
#[derive(Debug, Clone, Deserialize)]
pub struct FfiCallback {
    /// Callback name for reference
    pub name: String,
    /// C arguments the callback receives
    #[serde(default)]
    pub args: Vec<FfiCallbackArg>,
    /// Return type for the callback
    #[serde(rename = "return")]
    pub return_spec: Option<FfiReturn>,
    /// Seq stack effect when called
    pub seq_effect: String,
}

fn default_pass_mode() -> PassMode {
    PassMode::CString
}

/// Return value specification
#[derive(Debug, Clone, Deserialize)]
pub struct FfiReturn {
    /// The type of the return value
    #[serde(rename = "type")]
    pub return_type: FfiType,
    /// Memory ownership
    #[serde(default = "default_ownership")]
    pub ownership: Ownership,
}

fn default_ownership() -> Ownership {
    Ownership::Borrowed
}

/// A function binding in an FFI manifest
#[derive(Debug, Clone, Deserialize)]
pub struct FfiFunction {
    /// C function name (e.g., "readline")
    pub c_name: String,
    /// Seq word name (e.g., "readline")
    pub seq_name: String,
    /// Stack effect annotation (e.g., "( String -- String )")
    pub stack_effect: String,
    /// Function arguments
    #[serde(default)]
    pub args: Vec<FfiArg>,
    /// Return value specification
    #[serde(rename = "return")]
    pub return_spec: Option<FfiReturn>,
}

/// A library binding in an FFI manifest
#[derive(Debug, Clone, Deserialize)]
pub struct FfiLibrary {
    /// Library name for reference
    pub name: String,
    /// Linker flag (e.g., "readline" for -lreadline)
    pub link: String,
    /// Callback type definitions
    #[serde(rename = "callback", default)]
    pub callbacks: Vec<FfiCallback>,
    /// Function bindings
    #[serde(rename = "function", default)]
    pub functions: Vec<FfiFunction>,
}

/// Top-level FFI manifest structure
#[derive(Debug, Clone, Deserialize)]
pub struct FfiManifest {
    /// Library definitions (usually just one per manifest)
    #[serde(rename = "library")]
    pub libraries: Vec<FfiLibrary>,
}

impl FfiManifest {
    /// Parse an FFI manifest from TOML content
    ///
    /// Validates the manifest after parsing to catch:
    /// - Empty library names or linker flags
    /// - Empty function names (c_name or seq_name)
    /// - Malformed stack effects
    pub fn parse(content: &str) -> Result<Self, String> {
        let manifest: Self =
            toml::from_str(content).map_err(|e| format!("Failed to parse FFI manifest: {}", e))?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validate the manifest for common errors
    fn validate(&self) -> Result<(), String> {
        if self.libraries.is_empty() {
            return Err("FFI manifest must define at least one library".to_string());
        }

        for (lib_idx, lib) in self.libraries.iter().enumerate() {
            // Validate library name
            if lib.name.trim().is_empty() {
                return Err(format!("FFI library {} has empty name", lib_idx + 1));
            }

            // Validate linker flag (security: prevent injection of arbitrary flags)
            if lib.link.trim().is_empty() {
                return Err(format!("FFI library '{}' has empty linker flag", lib.name));
            }
            // Only allow safe characters in linker flag: alphanumeric, dash, underscore, dot
            for c in lib.link.chars() {
                if !c.is_alphanumeric() && c != '-' && c != '_' && c != '.' {
                    return Err(format!(
                        "FFI library '{}' has invalid character '{}' in linker flag '{}'. \
                         Only alphanumeric, dash, underscore, and dot are allowed.",
                        lib.name, c, lib.link
                    ));
                }
            }

            // Collect callback names for reference validation
            let callback_names: Vec<&str> = lib.callbacks.iter().map(|c| c.name.as_str()).collect();

            // Validate each callback
            for (cb_idx, callback) in lib.callbacks.iter().enumerate() {
                if callback.name.trim().is_empty() {
                    return Err(format!(
                        "FFI callback {} in library '{}' has empty name",
                        cb_idx + 1,
                        lib.name
                    ));
                }

                if callback.seq_effect.trim().is_empty() {
                    return Err(format!(
                        "FFI callback '{}' has empty seq_effect",
                        callback.name
                    ));
                }

                // Validate seq_effect parses correctly
                if let Err(e) = callback.effect() {
                    return Err(format!(
                        "FFI callback '{}' has malformed seq_effect '{}': {}",
                        callback.name, callback.seq_effect, e
                    ));
                }
            }

            // Validate each function
            for (func_idx, func) in lib.functions.iter().enumerate() {
                // Validate c_name
                if func.c_name.trim().is_empty() {
                    return Err(format!(
                        "FFI function {} in library '{}' has empty c_name",
                        func_idx + 1,
                        lib.name
                    ));
                }

                // Validate seq_name
                if func.seq_name.trim().is_empty() {
                    return Err(format!(
                        "FFI function '{}' in library '{}' has empty seq_name",
                        func.c_name, lib.name
                    ));
                }

                // Validate stack_effect is not empty
                if func.stack_effect.trim().is_empty() {
                    return Err(format!(
                        "FFI function '{}' has empty stack_effect",
                        func.seq_name
                    ));
                }

                // Validate stack_effect parses correctly
                if let Err(e) = func.effect() {
                    return Err(format!(
                        "FFI function '{}' has malformed stack_effect '{}': {}",
                        func.seq_name, func.stack_effect, e
                    ));
                }

                // Validate callback references
                for arg in &func.args {
                    if arg.arg_type == FfiType::Callback {
                        match &arg.callback {
                            None => {
                                return Err(format!(
                                    "FFI function '{}' has callback argument without 'callback' field",
                                    func.seq_name
                                ));
                            }
                            Some(cb_name) => {
                                if !callback_names.contains(&cb_name.as_str()) {
                                    return Err(format!(
                                        "FFI function '{}' references undefined callback '{}'",
                                        func.seq_name, cb_name
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get all linker flags needed for this manifest
    pub fn linker_flags(&self) -> Vec<String> {
        self.libraries.iter().map(|lib| lib.link.clone()).collect()
    }

    /// Get all function bindings from this manifest
    pub fn functions(&self) -> impl Iterator<Item = &FfiFunction> {
        self.libraries.iter().flat_map(|lib| lib.functions.iter())
    }
}

impl FfiFunction {
    /// Parse the stack effect string into an Effect
    pub fn effect(&self) -> Result<Effect, String> {
        parse_stack_effect(&self.stack_effect)
    }
}

impl FfiCallback {
    /// Parse the seq_effect string into an Effect
    pub fn effect(&self) -> Result<Effect, String> {
        parse_stack_effect(&self.seq_effect)
    }
}

/// Parse a stack effect string like "( String -- String )" into an Effect
fn parse_stack_effect(s: &str) -> Result<Effect, String> {
    // Strip parentheses and trim
    let s = s.trim();
    let s = s
        .strip_prefix('(')
        .ok_or("Stack effect must start with '('")?;
    let s = s
        .strip_suffix(')')
        .ok_or("Stack effect must end with ')'")?;
    let s = s.trim();

    // Split on "--"
    let parts: Vec<&str> = s.split("--").collect();
    if parts.len() != 2 {
        return Err(format!(
            "Stack effect must contain exactly one '--', got: {}",
            s
        ));
    }

    let inputs_str = parts[0].trim();
    let outputs_str = parts[1].trim();

    // Parse input types
    let mut inputs = StackType::RowVar("a".to_string());
    for type_name in inputs_str.split_whitespace() {
        let ty = parse_type_name(type_name)?;
        inputs = inputs.push(ty);
    }

    // Parse output types
    let mut outputs = StackType::RowVar("a".to_string());
    for type_name in outputs_str.split_whitespace() {
        let ty = parse_type_name(type_name)?;
        outputs = outputs.push(ty);
    }

    Ok(Effect::new(inputs, outputs))
}

/// Parse a type name string into a Type
fn parse_type_name(name: &str) -> Result<Type, String> {
    match name {
        "Int" => Ok(Type::Int),
        "Float" => Ok(Type::Float),
        "Bool" => Ok(Type::Bool),
        "String" => Ok(Type::String),
        _ => Err(format!("Unknown type '{}' in stack effect", name)),
    }
}

// ============================================================================
// Embedded FFI Manifests
// ============================================================================

/// Embedded libedit FFI manifest (BSD-licensed)
pub const LIBEDIT_MANIFEST: &str = include_str!("../ffi/libedit.toml");

/// Get an embedded FFI manifest by name
pub fn get_ffi_manifest(name: &str) -> Option<&'static str> {
    match name {
        "libedit" => Some(LIBEDIT_MANIFEST),
        _ => None,
    }
}

/// Check if an FFI manifest exists
pub fn has_ffi_manifest(name: &str) -> bool {
    get_ffi_manifest(name).is_some()
}

/// List all available embedded FFI manifests
pub fn list_ffi_manifests() -> &'static [&'static str] {
    &["libedit"]
}

// ============================================================================
// FFI Code Generation
// ============================================================================

/// Resolved FFI bindings ready for code generation
#[derive(Debug, Clone)]
pub struct FfiBindings {
    /// Map from Seq word name to C function info
    pub functions: HashMap<String, FfiFunctionInfo>,
    /// Map from callback name to callback info
    pub callbacks: HashMap<String, FfiCallbackInfo>,
    /// Linker flags to add
    pub linker_flags: Vec<String>,
}

/// Information about an FFI function for code generation
#[derive(Debug, Clone)]
pub struct FfiFunctionInfo {
    /// C function name
    pub c_name: String,
    /// Seq word name
    pub seq_name: String,
    /// Stack effect for type checking
    pub effect: Effect,
    /// Arguments
    pub args: Vec<FfiArg>,
    /// Return specification
    pub return_spec: Option<FfiReturn>,
}

/// Information about an FFI callback for code generation
#[derive(Debug, Clone)]
pub struct FfiCallbackInfo {
    /// Callback name
    pub name: String,
    /// C arguments the callback receives
    pub args: Vec<FfiCallbackArg>,
    /// Return type for the callback
    pub return_spec: Option<FfiReturn>,
    /// Seq stack effect
    pub effect: Effect,
}

impl FfiBindings {
    /// Create empty bindings
    pub fn new() -> Self {
        FfiBindings {
            functions: HashMap::new(),
            callbacks: HashMap::new(),
            linker_flags: Vec::new(),
        }
    }

    /// Add bindings from a manifest
    pub fn add_manifest(&mut self, manifest: &FfiManifest) -> Result<(), String> {
        // Add linker flags
        self.linker_flags.extend(manifest.linker_flags());

        // Add callback definitions
        for lib in &manifest.libraries {
            for callback in &lib.callbacks {
                let effect = callback.effect()?;
                let info = FfiCallbackInfo {
                    name: callback.name.clone(),
                    args: callback.args.clone(),
                    return_spec: callback.return_spec.clone(),
                    effect,
                };

                if self.callbacks.contains_key(&callback.name) {
                    return Err(format!(
                        "FFI callback '{}' is already defined",
                        callback.name
                    ));
                }

                self.callbacks.insert(callback.name.clone(), info);
            }
        }

        // Add function bindings
        for func in manifest.functions() {
            let mut effect = func.effect()?;

            // Add callback argument types to the effect
            // Callbacks consume a quotation from the stack
            for arg in &func.args {
                if arg.arg_type == FfiType::Callback
                    && let Some(cb_name) = &arg.callback
                    && let Some(cb_info) = self.callbacks.get(cb_name)
                {
                    // Create a quotation type from the callback's effect
                    let quot_type = crate::types::Type::Quotation(Box::new(cb_info.effect.clone()));
                    effect.inputs = effect.inputs.push(quot_type);
                }
            }

            let info = FfiFunctionInfo {
                c_name: func.c_name.clone(),
                seq_name: func.seq_name.clone(),
                effect,
                args: func.args.clone(),
                return_spec: func.return_spec.clone(),
            };

            if self.functions.contains_key(&func.seq_name) {
                return Err(format!(
                    "FFI function '{}' is already defined",
                    func.seq_name
                ));
            }

            self.functions.insert(func.seq_name.clone(), info);
        }

        Ok(())
    }

    /// Get callback info by name
    pub fn get_callback(&self, name: &str) -> Option<&FfiCallbackInfo> {
        self.callbacks.get(name)
    }

    /// Check if a word is an FFI function
    pub fn is_ffi_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Get FFI function info
    pub fn get_function(&self, name: &str) -> Option<&FfiFunctionInfo> {
        self.functions.get(name)
    }

    /// Get all FFI function names for AST validation
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for FfiBindings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let content = r#"
[[library]]
name = "readline"
link = "readline"

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
args = [
  { type = "string", pass = "c_string" }
]
return = { type = "string", ownership = "caller_frees" }
"#;

        let manifest = FfiManifest::parse(content).unwrap();
        assert_eq!(manifest.libraries.len(), 1);
        assert_eq!(manifest.libraries[0].name, "readline");
        assert_eq!(manifest.libraries[0].link, "readline");
        assert_eq!(manifest.libraries[0].functions.len(), 1);

        let func = &manifest.libraries[0].functions[0];
        assert_eq!(func.c_name, "readline");
        assert_eq!(func.seq_name, "readline");
        assert_eq!(func.args.len(), 1);
        assert_eq!(func.args[0].arg_type, FfiType::String);
        assert_eq!(func.args[0].pass, PassMode::CString);
    }

    #[test]
    fn test_parse_stack_effect() {
        let effect = parse_stack_effect("( String -- String )").unwrap();
        // Input: ( ..a String )
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
        // Output: ( ..a String )
        let (rest, top) = effect.outputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_parse_stack_effect_void() {
        let effect = parse_stack_effect("( String -- )").unwrap();
        // Input: ( ..a String )
        let (rest, top) = effect.inputs.clone().pop().unwrap();
        assert_eq!(top, Type::String);
        assert_eq!(rest, StackType::RowVar("a".to_string()));
        // Output: ( ..a )
        assert_eq!(effect.outputs, StackType::RowVar("a".to_string()));
    }

    #[test]
    fn test_ffi_bindings() {
        let content = r#"
[[library]]
name = "readline"
link = "readline"

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
args = [{ type = "string", pass = "c_string" }]
return = { type = "string", ownership = "caller_frees" }

[[library.function]]
c_name = "add_history"
seq_name = "add-history"
stack_effect = "( String -- )"
args = [{ type = "string", pass = "c_string" }]
return = { type = "void" }
"#;

        let manifest = FfiManifest::parse(content).unwrap();
        let mut bindings = FfiBindings::new();
        bindings.add_manifest(&manifest).unwrap();

        assert!(bindings.is_ffi_function("readline"));
        assert!(bindings.is_ffi_function("add-history"));
        assert!(!bindings.is_ffi_function("not-defined"));

        assert_eq!(bindings.linker_flags, vec!["readline"]);
    }

    // Validation tests

    #[test]
    fn test_validate_empty_library_name() {
        let content = r#"
[[library]]
name = ""
link = "readline"

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty name"));
    }

    #[test]
    fn test_validate_empty_link() {
        let content = r#"
[[library]]
name = "readline"
link = "  "

[[library.function]]
c_name = "readline"
seq_name = "readline"
stack_effect = "( String -- String )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty linker flag"));
    }

    #[test]
    fn test_validate_empty_c_name() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = ""
seq_name = "my-func"
stack_effect = "( -- Int )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty c_name"));
    }

    #[test]
    fn test_validate_empty_seq_name() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "my_func"
seq_name = ""
stack_effect = "( -- Int )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty seq_name"));
    }

    #[test]
    fn test_validate_empty_stack_effect() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "my_func"
seq_name = "my-func"
stack_effect = ""
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty stack_effect"));
    }

    #[test]
    fn test_validate_malformed_stack_effect_no_parens() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "my_func"
seq_name = "my-func"
stack_effect = "String -- Int"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("malformed stack_effect"));
    }

    #[test]
    fn test_validate_malformed_stack_effect_no_separator() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "my_func"
seq_name = "my-func"
stack_effect = "( String Int )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("malformed stack_effect"));
        assert!(err.contains("--"));
    }

    #[test]
    fn test_validate_malformed_stack_effect_unknown_type() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "my_func"
seq_name = "my-func"
stack_effect = "( UnknownType -- Int )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("malformed stack_effect"));
        assert!(err.contains("Unknown type"));
    }

    #[test]
    fn test_validate_no_libraries() {
        // TOML requires the `library` field to be present since it's not marked with #[serde(default)]
        // An empty manifest will fail TOML parsing, not our custom validation
        // But we can test with an explicit empty array
        let content = r#"
library = []
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("at least one library"));
    }

    #[test]
    fn test_validate_linker_flag_injection() {
        // Security: reject linker flags with potentially dangerous characters
        let content = r#"
[[library]]
name = "evil"
link = "evil -Wl,-rpath,/malicious"

[[library.function]]
c_name = "func"
seq_name = "func"
stack_effect = "( -- )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid character"));
    }

    #[test]
    fn test_validate_linker_flag_valid() {
        // Valid linker flags: alphanumeric, dash, underscore, dot
        let content = r#"
[[library]]
name = "test"
link = "my-lib_2.0"

[[library.function]]
c_name = "func"
seq_name = "func"
stack_effect = "( -- )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_ok());
    }

    // Callback tests

    #[test]
    fn test_parse_callback() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.callback]]
name = "comparator"
args = [
  { type = "ptr", name = "a" },
  { type = "ptr", name = "b" }
]
return = { type = "int" }
seq_effect = "( Int Int -- Int )"

[[library.function]]
c_name = "qsort"
seq_name = "c-qsort"
stack_effect = "( Int Int Int -- )"
args = [
  { type = "ptr", pass = "ptr" },
  { type = "int", pass = "int" },
  { type = "int", pass = "int" },
  { type = "callback", callback = "comparator" }
]
return = { type = "void" }
"#;

        let manifest = FfiManifest::parse(content).unwrap();
        assert_eq!(manifest.libraries[0].callbacks.len(), 1);
        assert_eq!(manifest.libraries[0].callbacks[0].name, "comparator");
        assert_eq!(manifest.libraries[0].callbacks[0].args.len(), 2);

        let mut bindings = FfiBindings::new();
        bindings.add_manifest(&manifest).unwrap();
        assert!(bindings.get_callback("comparator").is_some());
    }

    #[test]
    fn test_validate_callback_undefined_reference() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "qsort"
seq_name = "c-qsort"
stack_effect = "( Int Int Int -- )"
args = [
  { type = "callback", callback = "undefined_callback" }
]
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("undefined callback"));
    }

    #[test]
    fn test_validate_callback_missing_field() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.function]]
c_name = "qsort"
seq_name = "c-qsort"
stack_effect = "( Int -- )"
args = [
  { type = "callback" }
]
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("without 'callback' field"));
    }

    #[test]
    fn test_validate_callback_empty_name() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.callback]]
name = ""
seq_effect = "( Int -- Int )"
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("empty name"));
    }

    #[test]
    fn test_validate_callback_empty_seq_effect() {
        let content = r#"
[[library]]
name = "mylib"
link = "mylib"

[[library.callback]]
name = "my_callback"
seq_effect = ""
"#;

        let result = FfiManifest::parse(content);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("empty seq_effect"));
    }
}
