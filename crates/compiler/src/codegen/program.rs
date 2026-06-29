//! Program Code Generation
//!
//! This module contains the main entry points for generating LLVM IR
//! from a complete Seq program.

use super::{
    CodeGen, CodeGenError, emit_runtime_decls, ffi_c_args, ffi_return_type, get_target_triple,
};
use crate::ast::{Program, WordDef};
use crate::config::CompilerConfig;
use crate::ffi::FfiBindings;
use crate::types::{StackType, Type};
use std::collections::HashMap;
use std::fmt::Write as _;

/// Detect whether `main` was declared with effect `( -- Int )`.
///
/// Returns true if main's declared output is a single Int (with no row
/// variable below it). Returns false for `( -- )` or anything else.
/// The typechecker is responsible for rejecting other shapes; this just
/// reads the declared effect.
fn main_returns_int_effect(word: &WordDef) -> bool {
    let Some(effect) = &word.effect else {
        return false;
    };
    // Inputs must be empty (or just a row var) — main takes no inputs
    // Outputs must be exactly one Int on top of the row var
    matches!(
        &effect.outputs,
        StackType::Cons { rest, top: Type::Int }
            if matches!(**rest, StackType::Empty | StackType::RowVar(_))
    )
}

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

    /// Generate LLVM IR for entire program with custom configuration.
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
        self.prepare_program_state(program, type_map, statement_types, config)?;
        self.generate_words_and_main(program)?;

        let mut ir = String::new();
        self.emit_ir_header(&mut ir)?;
        self.emit_ir_type_and_globals(&mut ir)?;
        emit_runtime_decls(&mut ir)?;
        self.emit_external_builtins(&mut ir)?;
        self.emit_quotation_functions(&mut ir)?;
        ir.push_str(&self.output);
        self.dbg_emit_module_metadata(&mut ir);
        Ok(ir)
    }

    /// Generate LLVM IR for entire program with FFI support.
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
        self.ffi_bindings = ffi_bindings.clone();
        self.generate_ffi_wrappers()?;

        self.prepare_program_state(program, type_map, statement_types, config)?;
        self.generate_words_and_main(program)?;

        let mut ir = String::new();
        self.emit_ir_header(&mut ir)?;
        self.emit_ir_type_and_globals(&mut ir)?;
        emit_runtime_decls(&mut ir)?;
        self.emit_ffi_c_declarations(&mut ir)?;
        self.emit_external_builtins(&mut ir)?;
        self.emit_ffi_wrappers_section(&mut ir)?;
        self.emit_quotation_functions(&mut ir)?;
        ir.push_str(&self.output);
        self.dbg_emit_module_metadata(&mut ir);
        Ok(ir)
    }

    // =========================================================================
    // Shared program-generation helpers
    // =========================================================================

    /// Copy typechecker outputs and config-derived state onto the CodeGen,
    /// and sanity-check the presence/shape of `main`.
    fn prepare_program_state(
        &mut self,
        program: &Program,
        type_map: HashMap<usize, Type>,
        statement_types: HashMap<(String, usize), Type>,
        config: &CompilerConfig,
    ) -> Result<(), CodeGenError> {
        self.type_map = type_map;
        self.statement_types = statement_types;
        // resolved_sugar is set separately via set_resolved_sugar()
        self.unions = program.unions.clone();
        self.external_builtins = config
            .external_builtins
            .iter()
            .map(|b| (b.seq_name.clone(), b.symbol.clone()))
            .collect();

        self.instrument = config.instrument;
        self.loop_opt_enabled = config.loop_opt;
        self.loop_yield_cadence = config.loop_yield_cadence;
        if self.instrument {
            for (id, word) in program.words.iter().enumerate() {
                self.word_instrument_ids.insert(word.name.clone(), id);
            }
        }

        // Issue #355: detect `main ( -- Int )` so seq_main writes the top-of-
        // stack int into the exit-code global before tearing down.
        let main_word = program
            .find_word("main")
            .ok_or_else(|| CodeGenError::Logic("No main word defined".to_string()))?;
        self.main_returns_int = main_returns_int_effect(main_word);

        Ok(())
    }

    /// Generate code for every user-defined word, then emit `main`.
    fn generate_words_and_main(&mut self, program: &Program) -> Result<(), CodeGenError> {
        for word in &program.words {
            self.codegen_word(word)?;
        }
        self.codegen_main()
    }

    /// Module ID and target triple — the first lines of the IR file.
    fn emit_ir_header(&self, ir: &mut String) -> Result<(), CodeGenError> {
        writeln!(ir, "; ModuleID = 'main'")?;
        writeln!(ir, "target triple = \"{}\"", get_target_triple())?;
        writeln!(ir)?;
        Ok(())
    }

    /// Value type definition, string/symbol globals, and instrumentation
    /// globals when `--instrument` is enabled.
    fn emit_ir_type_and_globals(&self, ir: &mut String) -> Result<(), CodeGenError> {
        self.emit_value_type_def(ir)?;
        self.emit_string_and_symbol_globals(ir)?;
        if self.instrument {
            self.emit_instrumentation_globals(ir)?;
        }
        Ok(())
    }

    fn emit_external_builtins(&self, ir: &mut String) -> Result<(), CodeGenError> {
        if self.external_builtins.is_empty() {
            return Ok(());
        }
        writeln!(ir, "; External builtin declarations")?;
        // All external builtins follow the standard stack convention: ptr -> ptr
        for symbol in self.external_builtins.values() {
            writeln!(ir, "declare ptr @{}(ptr)", symbol)?;
        }
        writeln!(ir)?;
        Ok(())
    }

    fn emit_quotation_functions(&self, ir: &mut String) -> Result<(), CodeGenError> {
        if self.quotation_functions.is_empty() {
            return Ok(());
        }
        writeln!(ir, "; Quotation functions")?;
        ir.push_str(&self.quotation_functions);
        writeln!(ir)?;
        Ok(())
    }

    fn emit_ffi_c_declarations(&self, ir: &mut String) -> Result<(), CodeGenError> {
        if self.ffi_bindings.functions.is_empty() {
            return Ok(());
        }
        writeln!(ir, "; FFI C function declarations")?;
        writeln!(ir, "declare ptr @malloc(i64)")?;
        writeln!(ir, "declare void @free(ptr)")?;
        writeln!(ir, "declare i64 @strlen(ptr)")?;
        writeln!(ir, "declare ptr @memcpy(ptr, ptr, i64)")?;
        // FFI string helpers from runtime
        writeln!(ir, "declare ptr @patch_seq_string_to_cstring(ptr, ptr)")?;
        writeln!(ir, "declare ptr @patch_seq_cstring_to_string(ptr, ptr)")?;
        for func in self.ffi_bindings.functions.values() {
            let c_ret_type = ffi_return_type(&func.return_spec);
            let c_args = ffi_c_args(&func.args);
            writeln!(ir, "declare {} @{}({})", c_ret_type, func.c_name, c_args)?;
        }
        writeln!(ir)?;
        Ok(())
    }

    fn emit_ffi_wrappers_section(&self, ir: &mut String) -> Result<(), CodeGenError> {
        if self.ffi_wrapper_code.is_empty() {
            return Ok(());
        }
        writeln!(ir, "; FFI wrapper functions")?;
        ir.push_str(&self.ffi_wrapper_code);
        writeln!(ir)?;
        Ok(())
    }

    /// Emit instrumentation globals for `--instrument` mode:
    /// - `@seq_word_counters`: array of i64 counters (one per word)
    /// - `@seq_word_name_K`: per-word C string constants
    /// - `@seq_word_names`: array of pointers to name strings
    fn emit_instrumentation_globals(&self, ir: &mut String) -> Result<(), CodeGenError> {
        let n = self.word_instrument_ids.len();
        if n == 0 {
            return Ok(());
        }

        writeln!(ir, "; Instrumentation globals (--instrument)")?;

        writeln!(
            ir,
            "@seq_word_counters = global [{} x i64] zeroinitializer",
            n
        )?;

        // Sort by id for deterministic output
        let mut words: Vec<(usize, &str)> = self
            .word_instrument_ids
            .iter()
            .map(|(name, &id)| (id, name.as_str()))
            .collect();
        words.sort_by_key(|&(id, _)| id);

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
