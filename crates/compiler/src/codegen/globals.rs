//! String and Symbol Global Handling
//!
//! This module handles deduplication of string and symbol literals
//! as LLVM IR global constants.

use super::{CodeGen, CodeGenError};
use std::fmt::Write as _;

impl CodeGen {
    /// Escape a string for LLVM IR string literals
    pub(super) fn escape_llvm_string(s: &str) -> Result<String, std::fmt::Error> {
        let mut result = String::new();
        for ch in s.chars() {
            match ch {
                ' '..='!' | '#'..='[' | ']'..='~' => result.push(ch),
                '\\' => result.push_str(r"\\"),
                '"' => result.push_str(r#"\22"#),
                '\n' => result.push_str(r"\0A"),
                '\r' => result.push_str(r"\0D"),
                '\t' => result.push_str(r"\09"),
                _ => {
                    // Non-printable: use hex escape
                    for byte in ch.to_string().as_bytes() {
                        write!(&mut result, r"\{:02X}", byte)?;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Get or create a global string constant
    pub(super) fn get_string_global(&mut self, s: &str) -> Result<String, CodeGenError> {
        if let Some(global_name) = self.string_constants.get(s) {
            return Ok(global_name.clone());
        }

        let global_name = format!("@.str.{}", self.string_counter);
        self.string_counter += 1;

        let escaped = Self::escape_llvm_string(s)?;
        let len = s.len() + 1; // +1 for null terminator

        writeln!(
            &mut self.string_globals,
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            global_name, len, escaped
        )?;

        self.string_constants
            .insert(s.to_string(), global_name.clone());
        Ok(global_name)
    }

    /// Get or create a global interned symbol constant (Issue #166)
    ///
    /// Creates a static SeqString structure with capacity=0 to mark it as interned.
    /// This enables O(1) symbol equality via pointer comparison.
    pub(super) fn get_symbol_global(&mut self, symbol_name: &str) -> Result<String, CodeGenError> {
        // Deduplicate: return existing global if we've seen this symbol
        if let Some(global_name) = self.symbol_constants.get(symbol_name) {
            return Ok(global_name.clone());
        }

        // Get or create the underlying string data
        let str_global = self.get_string_global(symbol_name)?;

        // Create the SeqString structure global
        let sym_global = format!("@.sym.{}", self.symbol_counter);
        self.symbol_counter += 1;

        // SeqString layout: { ptr, i64 len, i64 capacity, i8 global }
        // capacity=0 marks this as an interned symbol (never freed)
        // global=1 marks it as static data
        writeln!(
            &mut self.symbol_globals,
            "{} = private unnamed_addr constant {{ ptr, i64, i64, i8 }} {{ ptr {}, i64 {}, i64 0, i8 1 }}",
            sym_global,
            str_global,
            symbol_name.len()
        )?;

        self.symbol_constants
            .insert(symbol_name.to_string(), sym_global.clone());
        Ok(sym_global)
    }

    /// Generate LLVM IR for entire program
    pub(super) fn emit_string_and_symbol_globals(
        &self,
        ir: &mut String,
    ) -> Result<(), CodeGenError> {
        // String constants
        if !self.string_globals.is_empty() {
            ir.push_str(&self.string_globals);
            writeln!(ir)?;
        }

        // Symbol constants (interned symbols for O(1) equality)
        if !self.symbol_globals.is_empty() {
            ir.push_str(&self.symbol_globals);
            writeln!(ir)?;
        }
        Ok(())
    }
}
