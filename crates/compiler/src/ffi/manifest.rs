//! FFI manifest parsing and type mapping: the TOML-driven schema plus the
//! embedded manifest lookup (currently just `libedit`).

use serde::Deserialize;

use crate::types::{Effect, StackType, Type};

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
    /// C function name (e.g., "sqlite3_open")
    pub c_name: String,
    /// Seq word name (e.g., "db-open")
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
    /// Linker flag (e.g., "sqlite3" for -lsqlite3)
    pub link: String,
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
    pub(super) fn validate(&self) -> Result<(), String> {
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

/// Parse a stack effect string like "( String -- String )" into an Effect
pub(super) fn parse_stack_effect(s: &str) -> Result<Effect, String> {
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

    let inputs = parse_stack_side(parts[0].trim())?;
    let outputs = parse_stack_side(parts[1].trim())?;

    Ok(Effect::new(inputs, outputs))
}

/// Parse one side of a stack effect (a whitespace-separated type list) into a
/// row-polymorphic `StackType`, bottom-to-top.
fn parse_stack_side(s: &str) -> Result<StackType, String> {
    let mut stack = StackType::RowVar("a".to_string());
    for type_name in s.split_whitespace() {
        stack = stack.push(parse_type_name(type_name)?);
    }
    Ok(stack)
}

/// Parse a type name string into a Type
pub(super) fn parse_type_name(name: &str) -> Result<Type, String> {
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
pub const LIBEDIT_MANIFEST: &str = include_str!("../../ffi/libedit.toml");

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
