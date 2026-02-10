//! Program Code Generation
//!
//! This module contains the main entry points for generating LLVM IR
//! from a complete Seq program.

use super::{
    CodeGen, CodeGenError, emit_runtime_decls, ffi_c_args, ffi_return_type, get_target_triple,
};
use crate::ast::Program;
use crate::config::CompilerConfig;
use crate::ffi::FfiBindings;
use crate::types::Type;
use std::collections::HashMap;
use std::fmt::Write as _;

impl CodeGen {
    /// Generate LLVM IR for entire program
    pub fn codegen_program(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        statement_types: HashMap<(String, usize), Type>,
    ) -> Result<String, CodeGenError> {
        self.codegen_program_with_config(
            program,
            type_map,
            statement_types,
            &CompilerConfig::default(),
        )
    }

    /// Generate LLVM IR for entire program with custom configuration
    ///
    /// This allows external projects to extend the compiler with additional
    /// builtins that will be declared and callable from Seq code.
    pub fn codegen_program_with_config(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        statement_types: HashMap<(String, usize), Type>,
        config: &CompilerConfig,
    ) -> Result<String, CodeGenError> {
        // Store type map for use during code generation
        self.type_map = type_map;
        self.statement_types = statement_types;

        // Store union definitions for pattern matching
        self.unions = program.unions.clone();

        // Build external builtins map from config
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        // Flow instrumentation config
        self.instrument = config.instrument;
        if self.instrument {
            for (id, word) in program.words.iter().enumerate() {
                self.word_instrument_ids.insert(word.name.clone(), id);
            }
        }

        // Verify we have a main word
        if program.find_word("main").is_none() {
            return Err(CodeGenError::Logic("No main word defined".to_string()));
        }

        // Generate all user-defined words
        for word in &program.words {
            self.codegen_word(word)?;
        }

        // Generate main function
        self.codegen_main()?;

        // Assemble final IR
        let mut ir = String::new();

        // Target and type declarations
        writeln!(&mut ir, "; ModuleID = 'main'")?;
        writeln!(&mut ir, "target triple = \"{}\"", get_target_triple())?;
        writeln!(&mut ir)?;

        // Value type definition: Rust enum with #[repr(C)], 40 bytes
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

        // Instrumentation globals (when --instrument)
        if self.instrument {
            self.emit_instrumentation_globals(&mut ir)?;
        }

        // Runtime function declarations
        emit_runtime_decls(&mut ir)?;

        // External builtin declarations (from config)
        if !self.external_builtins.is_empty() {
            writeln!(&mut ir, "; External builtin declarations")?;
            for symbol in self.external_builtins.values() {
                // All external builtins follow the standard stack convention: ptr -> ptr
                writeln!(&mut ir, "declare ptr @{}(ptr)", symbol)?;
            }
            writeln!(&mut ir)?;
        }

        // Quotation functions (generated from quotation literals)
        if !self.quotation_functions.is_empty() {
            writeln!(&mut ir, "; Quotation functions")?;
            ir.push_str(&self.quotation_functions);
            writeln!(&mut ir)?;
        }

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Generate LLVM IR for entire program with FFI support
    ///
    /// This is the main entry point for compiling programs that use FFI.
    pub fn codegen_program_with_ffi(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        statement_types: HashMap<(String, usize), Type>,
        config: &CompilerConfig,
        ffi_bindings: &FfiBindings,
    ) -> Result<String, CodeGenError> {
        // Store FFI bindings
        self.ffi_bindings = ffi_bindings.clone();

        // Generate FFI wrapper functions
        self.generate_ffi_wrappers()?;

        // Store type map for use during code generation
        self.type_map = type_map;
        self.statement_types = statement_types;

        // Store union definitions for pattern matching
        self.unions = program.unions.clone();

        // Build external builtins map from config
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        // Flow instrumentation config
        self.instrument = config.instrument;
        if self.instrument {
            for (id, word) in program.words.iter().enumerate() {
                self.word_instrument_ids.insert(word.name.clone(), id);
            }
        }

        // Verify we have a main word
        if program.find_word("main").is_none() {
            return Err(CodeGenError::Logic("No main word defined".to_string()));
        }

        // Generate all user-defined words
        for word in &program.words {
            self.codegen_word(word)?;
        }

        // Generate main function
        self.codegen_main()?;

        // Assemble final IR
        let mut ir = String::new();

        // Target and type declarations
        writeln!(&mut ir, "; ModuleID = 'main'")?;
        writeln!(&mut ir, "target triple = \"{}\"", get_target_triple())?;
        writeln!(&mut ir)?;

        // Value type definition: Rust enum with #[repr(C)], 40 bytes
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

        // Instrumentation globals (when --instrument)
        if self.instrument {
            self.emit_instrumentation_globals(&mut ir)?;
        }

        // Runtime function declarations (same as codegen_program_with_config)
        self.emit_runtime_declarations(&mut ir)?;

        // FFI C function declarations
        if !self.ffi_bindings.functions.is_empty() {
            writeln!(&mut ir, "; FFI C function declarations")?;
            writeln!(&mut ir, "declare ptr @malloc(i64)")?;
            writeln!(&mut ir, "declare void @free(ptr)")?;
            writeln!(&mut ir, "declare i64 @strlen(ptr)")?;
            writeln!(&mut ir, "declare ptr @memcpy(ptr, ptr, i64)")?;
            // Declare FFI string helpers from runtime
            writeln!(
                &mut ir,
                "declare ptr @patch_seq_string_to_cstring(ptr, ptr)"
            )?;
            writeln!(
                &mut ir,
                "declare ptr @patch_seq_cstring_to_string(ptr, ptr)"
            )?;
            for func in self.ffi_bindings.functions.values() {
                let c_ret_type = ffi_return_type(&func.return_spec);
                let c_args = ffi_c_args(&func.args);
                writeln!(
                    &mut ir,
                    "declare {} @{}({})",
                    c_ret_type, func.c_name, c_args
                )?;
            }
            writeln!(&mut ir)?;
        }

        // External builtin declarations (from config)
        if !self.external_builtins.is_empty() {
            writeln!(&mut ir, "; External builtin declarations")?;
            for symbol in self.external_builtins.values() {
                writeln!(&mut ir, "declare ptr @{}(ptr)", symbol)?;
            }
            writeln!(&mut ir)?;
        }

        // FFI wrapper functions
        if !self.ffi_wrapper_code.is_empty() {
            writeln!(&mut ir, "; FFI wrapper functions")?;
            ir.push_str(&self.ffi_wrapper_code);
            writeln!(&mut ir)?;
        }

        // Quotation functions
        if !self.quotation_functions.is_empty() {
            writeln!(&mut ir, "; Quotation functions")?;
            ir.push_str(&self.quotation_functions);
            writeln!(&mut ir)?;
        }

        // User-defined words and main
        ir.push_str(&self.output);

        Ok(ir)
    }

    /// Emit runtime function declarations
    pub(super) fn emit_runtime_declarations(&self, ir: &mut String) -> Result<(), CodeGenError> {
        emit_runtime_decls(ir)
    }

    /// Emit instrumentation globals for --instrument mode
    ///
    /// Generates:
    /// - @seq_word_counters: array of i64 counters (one per word)
    /// - @seq_word_name_K: per-word C string constants
    /// - @seq_word_names: array of pointers to name strings
    fn emit_instrumentation_globals(&self, ir: &mut String) -> Result<(), CodeGenError> {
        let n = self.word_instrument_ids.len();
        if n == 0 {
            return Ok(());
        }

        writeln!(ir, "; Instrumentation globals (--instrument)")?;

        // Counter array: [N x i64] zeroinitializer
        writeln!(
            ir,
            "@seq_word_counters = global [{} x i64] zeroinitializer",
            n
        )?;

        // Build sorted list of (id, name) for deterministic output
        let mut words: Vec<(usize, &str)> = self
            .word_instrument_ids
            .iter()
            .map(|(name, &id)| (id, name.as_str()))
            .collect();
        words.sort_by_key(|&(id, _)| id);

        // Per-word name string constants
        for &(id, name) in &words {
            let name_bytes = name.as_bytes();
            let len = name_bytes.len() + 1; // +1 for null terminator
            let escaped: String = name_bytes
                .iter()
                .map(|&b| format!("\\{:02X}", b))
                .collect::<String>();
            writeln!(
                ir,
                "@seq_word_name_{} = private constant [{} x i8] c\"{}\\00\"",
                id, len, escaped
            )?;
        }

        // Name pointer table
        let ptrs: Vec<String> = words
            .iter()
            .map(|&(id, _name)| format!("ptr @seq_word_name_{}", id))
            .collect();
        writeln!(
            ir,
            "@seq_word_names = private constant [{} x ptr] [{}]",
            n,
            ptrs.join(", ")
        )?;

        writeln!(ir)?;
        Ok(())
    }
}
