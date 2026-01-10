//! LLVM IR Code Generation via Text
//!
//! Generates LLVM IR as text (.ll files) and invokes clang to produce executables.
//! This approach is simpler and more portable than using FFI bindings (inkwell).
//!
//! # Code Generation Strategy
//!
//! Stack is threaded through all operations as a pointer:
//! 1. Start with null stack pointer
//! 2. Each operation takes stack, returns new stack
//! 3. Final stack is ignored (should be null for well-typed programs)
//!
//! # Runtime Function Declarations
//!
//! All runtime functions follow the pattern:
//! - `define ptr @name(ptr %stack) { ... }` for stack operations
//! - `define ptr @push_int(ptr %stack, i64 %value) { ... }` for literals
//! - Stack type is represented as `ptr` (opaque pointer in modern LLVM)

// Submodules
mod control_flow;
mod error;
mod ffi_wrappers;
mod globals;
mod inline_dispatch;
mod inline_ops;
mod platform;
mod runtime;
mod statements;
mod types;
mod virtual_stack;
mod words;

// Re-exports
pub use error::CodeGenError;
pub use platform::{ffi_c_args, ffi_return_type, get_target_triple};
pub use runtime::{BUILTIN_SYMBOLS, RUNTIME_DECLARATIONS, emit_runtime_decls};

use crate::ast::{Program, UnionDef};
use crate::config::CompilerConfig;
use crate::ffi::FfiBindings;
use crate::types::Type;
use std::collections::HashMap;
use std::fmt::Write as _;

/// Sentinel value for unreachable predecessors in phi nodes.
/// Used when a branch ends with a tail call (which emits ret directly).
const UNREACHABLE_PREDECESSOR: &str = "unreachable";

/// Maximum number of values to keep in virtual registers (Issue #189).
/// Values beyond this are spilled to memory.
///
/// Tuned for common patterns:
/// - Binary ops need 2 values (`a b i.+`)
/// - Dup patterns need 3 values (`a dup i.* b i.+`)
/// - Complex expressions may use 4 (`a b i.+ c d i.* i.-`)
///
/// Larger values increase register pressure with diminishing returns,
/// as most operations trigger spills (control flow, function calls, etc.).
const MAX_VIRTUAL_STACK: usize = 4;

/// Tracks whether a statement is in tail position.
///
/// A statement is in tail position when its result is directly returned
/// from the function without further processing. For tail calls, we can
/// use LLVM's `musttail` to guarantee tail call optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TailPosition {
    /// This is the last operation before return - can use musttail
    Tail,
    /// More operations follow - use regular call
    NonTail,
}

/// Result of generating code for an if-statement branch.
struct BranchResult {
    /// The stack variable after executing the branch
    stack_var: String,
    /// Whether the branch emitted a tail call (and thus a ret)
    emitted_tail_call: bool,
    /// The predecessor block label for the phi node (or UNREACHABLE_PREDECESSOR)
    predecessor: String,
}

/// Mangle a Seq word name into a valid LLVM IR identifier.
///
/// LLVM IR identifiers can contain: letters, digits, underscores, dollars, periods.
/// Seq words can contain: letters, digits, hyphens, question marks, arrows, etc.
///
/// We escape special characters using underscore-based encoding:
/// - `-` (hyphen) -> `_` (hyphens not valid in LLVM IR identifiers)
/// - `?` -> `_Q_` (question)
/// - `>` -> `_GT_` (greater than, for ->)
/// - `<` -> `_LT_` (less than)
/// - `!` -> `_BANG_`
/// - `*` -> `_STAR_`
/// - `/` -> `_SLASH_`
/// - `+` -> `_PLUS_`
/// - `=` -> `_EQ_`
/// - `.` -> `_DOT_`
fn mangle_name(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        match c {
            '?' => result.push_str("_Q_"),
            '>' => result.push_str("_GT_"),
            '<' => result.push_str("_LT_"),
            '!' => result.push_str("_BANG_"),
            '*' => result.push_str("_STAR_"),
            '/' => result.push_str("_SLASH_"),
            '+' => result.push_str("_PLUS_"),
            '=' => result.push_str("_EQ_"),
            // Hyphens converted to underscores (hyphens not valid in LLVM IR)
            '-' => result.push('_'),
            // Keep these as-is (valid in LLVM IR)
            '_' | '.' | '$' => result.push(c),
            // Alphanumeric kept as-is
            c if c.is_alphanumeric() => result.push(c),
            // Any other character gets hex-encoded
            _ => result.push_str(&format!("_x{:02X}_", c as u32)),
        }
    }
    result
}

/// Result of generating a quotation: wrapper and impl function names
/// For closures, both names are the same (no TCO support yet)
struct QuotationFunctions {
    /// C-convention wrapper function (for runtime calls)
    wrapper: String,
    /// tailcc implementation function (for TCO via musttail)
    impl_: String,
}

/// A value held in an LLVM virtual register instead of memory (Issue #189).
///
/// This optimization keeps recently-pushed values in SSA variables,
/// avoiding memory stores/loads for common patterns like `2 3 i.+`.
/// Values are spilled to memory at control flow points and function calls.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Float and Bool variants for Phase 2
enum VirtualValue {
    /// Integer value in an SSA variable (i64)
    Int {
        ssa_var: String,
        #[allow(dead_code)] // Used for constant folding in Phase 2
        value: i64,
    },
    /// Float value in an SSA variable (double)
    Float { ssa_var: String },
    /// Boolean value in an SSA variable (i64: 0 or 1)
    Bool { ssa_var: String },
}

#[allow(dead_code)] // ssa_var method used in spill_virtual_stack
impl VirtualValue {
    /// Get the SSA variable name
    fn ssa_var(&self) -> &str {
        match self {
            VirtualValue::Int { ssa_var, .. } => ssa_var,
            VirtualValue::Float { ssa_var } => ssa_var,
            VirtualValue::Bool { ssa_var } => ssa_var,
        }
    }

    /// Get the discriminant for this value type
    fn discriminant(&self) -> i64 {
        match self {
            VirtualValue::Int { .. } => 0,
            VirtualValue::Float { .. } => 1,
            VirtualValue::Bool { .. } => 2,
        }
    }
}

pub struct CodeGen {
    output: String,
    string_globals: String,
    temp_counter: usize,
    string_counter: usize,
    block_counter: usize, // For generating unique block labels
    quot_counter: usize,  // For generating unique quotation function names
    string_constants: HashMap<String, String>, // string content -> global name
    quotation_functions: String, // Accumulates generated quotation functions
    type_map: HashMap<usize, Type>, // Maps quotation ID to inferred type (from typechecker)
    external_builtins: HashMap<String, String>, // seq_name -> symbol (for external builtins)
    inside_closure: bool, // Track if we're generating code inside a closure (disables TCO)
    inside_main: bool, // Track if we're generating code for main (uses C convention, no musttail)
    inside_quotation: bool, // Track if we're generating code for a quotation (uses C convention, no musttail)
    unions: Vec<UnionDef>,  // Union type definitions for pattern matching
    ffi_bindings: FfiBindings, // FFI function bindings
    ffi_wrapper_code: String, // Generated FFI wrapper functions
    /// Pure inline test mode: bypasses scheduler, returns top of stack as exit code.
    /// Used for testing pure integer programs without FFI dependencies.
    pure_inline_test: bool,
    // Symbol interning for O(1) equality (Issue #166)
    symbol_globals: String, // LLVM IR for static symbol globals
    symbol_counter: usize,  // Counter for unique symbol names
    symbol_constants: HashMap<String, String>, // symbol name -> global name (deduplication)
    /// Per-statement type info for optimization (Issue #186)
    /// Maps (word_name, statement_index) -> top-of-stack type before statement
    statement_types: HashMap<(String, usize), Type>,
    /// Current word being compiled (for statement type lookup)
    current_word_name: Option<String>,
    /// Current statement index within the word (for statement type lookup)
    current_stmt_index: usize,
    /// Nesting depth for type lookup - only depth 0 can use type info
    /// Nested contexts (if/else, loops) increment this to disable lookups
    codegen_depth: usize,
    /// True if the previous statement was a trivially-copyable literal (Issue #195)
    /// Used to optimize `dup` after literal push (e.g., `42 dup`)
    prev_stmt_is_trivial_literal: bool,
    /// If previous statement was IntLiteral, stores its value (Issue #192)
    /// Used to optimize `roll`/`pick` with constant N (e.g., `2 roll` -> rot)
    prev_stmt_int_value: Option<i64>,
    /// Virtual register stack for top N values (Issue #189)
    /// Values here are in SSA variables, not yet written to memory.
    /// The memory stack pointer tracks where memory ends; virtual values are "above" it.
    virtual_stack: Vec<VirtualValue>,
}

impl Default for CodeGen {
    fn default() -> Self {
        Self::new()
    }
}

impl CodeGen {
    pub fn new() -> Self {
        CodeGen {
            output: String::new(),
            string_globals: String::new(),
            temp_counter: 0,
            string_counter: 0,
            block_counter: 0,
            inside_closure: false,
            inside_main: false,
            inside_quotation: false,
            quot_counter: 0,
            string_constants: HashMap::new(),
            quotation_functions: String::new(),
            type_map: HashMap::new(),
            external_builtins: HashMap::new(),
            unions: Vec::new(),
            ffi_bindings: FfiBindings::new(),
            ffi_wrapper_code: String::new(),
            pure_inline_test: false,
            symbol_globals: String::new(),
            symbol_counter: 0,
            symbol_constants: HashMap::new(),
            statement_types: HashMap::new(),
            current_word_name: None,
            current_stmt_index: 0,
            codegen_depth: 0,
            prev_stmt_is_trivial_literal: false,
            prev_stmt_int_value: None,
            virtual_stack: Vec::new(),
        }
    }

    /// Create a CodeGen for pure inline testing.
    /// Bypasses the scheduler, returning top of stack as exit code.
    /// Only supports operations that are fully inlined (integers, arithmetic, stack ops).
    #[allow(dead_code)]
    pub fn new_pure_inline_test() -> Self {
        let mut cg = Self::new();
        cg.pure_inline_test = true;
        cg
    }

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

        // Value type (Rust enum with #[repr(C)], 40 bytes: discriminant + largest variant payload)
        // We define concrete size so LLVM can pass by value (required for Alpine/musl)
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

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

        // Value type (Rust enum with #[repr(C)], 40 bytes: discriminant + largest variant payload)
        writeln!(&mut ir, "; Value type (Rust enum - 40 bytes)")?;
        writeln!(&mut ir, "%Value = type {{ i64, i64, i64, i64, i64 }}")?;
        writeln!(&mut ir)?;

        // String and symbol constants
        self.emit_string_and_symbol_globals(&mut ir)?;

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
    fn emit_runtime_declarations(&self, ir: &mut String) -> Result<(), CodeGenError> {
        emit_runtime_decls(ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Program, Statement, WordDef};

    #[test]
    fn test_codegen_hello_world() {
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("Hello, World!".to_string()),
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        assert!(ir.contains("define i32 @main(i32 %argc, ptr %argv)"));
        // main uses C calling convention (no tailcc) since it's called from C runtime
        assert!(ir.contains("define ptr @seq_main(ptr %stack)"));
        assert!(ir.contains("call ptr @patch_seq_push_string"));
        assert!(ir.contains("call ptr @patch_seq_write_line"));
        assert!(ir.contains("\"Hello, World!\\00\""));
    }

    #[test]
    fn test_codegen_io_write() {
        // Test io.write (write without newline)
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::StringLiteral("no newline".to_string()),
                    Statement::WordCall {
                        name: "io.write".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        assert!(ir.contains("call ptr @patch_seq_push_string"));
        assert!(ir.contains("call ptr @patch_seq_write"));
        assert!(ir.contains("\"no newline\\00\""));
    }

    #[test]
    fn test_codegen_arithmetic() {
        // Test inline tagged stack arithmetic with virtual registers (Issue #189)
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(2),
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Issue #189: With virtual registers, integers are kept in SSA variables
        // Using identity add: %n = add i64 0, <value>
        assert!(ir.contains("add i64 0, 2"), "Should create SSA var for 2");
        assert!(ir.contains("add i64 0, 3"), "Should create SSA var for 3");
        // The add operation uses virtual registers directly
        assert!(ir.contains("add i64 %"), "Should add SSA variables");
    }

    #[test]
    fn test_pure_inline_test_mode() {
        let mut codegen = CodeGen::new_pure_inline_test();

        // Simple program: 5 3 add (should return 8)
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(5),
                    Statement::IntLiteral(3),
                    Statement::WordCall {
                        name: "i.add".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Pure inline test mode should:
        // 1. NOT CALL the scheduler (declarations are ok, calls are not)
        assert!(!ir.contains("call void @patch_seq_scheduler_init"));
        assert!(!ir.contains("call i64 @patch_seq_strand_spawn"));

        // 2. Have main allocate tagged stack and call seq_main directly
        assert!(ir.contains("call ptr @seq_stack_new_default()"));
        assert!(ir.contains("call ptr @seq_main(ptr %stack_base)"));

        // 3. Read result from stack and return as exit code
        assert!(ir.contains("trunc i64 %result to i32"));
        assert!(ir.contains("ret i32 %exit_code"));

        // 4. Use inline push with virtual registers (Issue #189)
        assert!(!ir.contains("call ptr @patch_seq_push_int"));
        // Values are kept in SSA variables via identity add
        assert!(ir.contains("add i64 0, 5"), "Should create SSA var for 5");
        assert!(ir.contains("add i64 0, 3"), "Should create SSA var for 3");

        // 5. Use inline add with virtual registers (add i64 %, not call patch_seq_add)
        assert!(!ir.contains("call ptr @patch_seq_add"));
        assert!(ir.contains("add i64 %"), "Should add SSA variables");
    }

    #[test]
    fn test_escape_llvm_string() {
        assert_eq!(CodeGen::escape_llvm_string("hello").unwrap(), "hello");
        assert_eq!(CodeGen::escape_llvm_string("a\nb").unwrap(), r"a\0Ab");
        assert_eq!(CodeGen::escape_llvm_string("a\tb").unwrap(), r"a\09b");
        assert_eq!(CodeGen::escape_llvm_string("a\"b").unwrap(), r"a\22b");
    }

    #[test]
    fn test_external_builtins_declared() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::IntLiteral(42),
                    Statement::WordCall {
                        name: "my-external-op".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("my-external-op", "test_runtime_my_op"));

        let ir = codegen
            .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
            .unwrap();

        // Should declare the external builtin
        assert!(
            ir.contains("declare ptr @test_runtime_my_op(ptr)"),
            "IR should declare external builtin"
        );

        // Should call the external builtin
        assert!(
            ir.contains("call ptr @test_runtime_my_op"),
            "IR should call external builtin"
        );
    }

    #[test]
    fn test_multiple_external_builtins() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::WordCall {
                        name: "actor-self".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "journal-append".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("actor-self", "seq_actors_self"))
            .with_builtin(ExternalBuiltin::new(
                "journal-append",
                "seq_actors_journal_append",
            ));

        let ir = codegen
            .codegen_program_with_config(&program, HashMap::new(), HashMap::new(), &config)
            .unwrap();

        // Should declare both external builtins
        assert!(ir.contains("declare ptr @seq_actors_self(ptr)"));
        assert!(ir.contains("declare ptr @seq_actors_journal_append(ptr)"));

        // Should call both
        assert!(ir.contains("call ptr @seq_actors_self"));
        assert!(ir.contains("call ptr @seq_actors_journal_append"));
    }

    #[test]
    fn test_external_builtins_with_library_paths() {
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let config = CompilerConfig::new()
            .with_builtin(ExternalBuiltin::new("my-op", "runtime_my_op"))
            .with_library_path("/custom/lib")
            .with_library("myruntime");

        assert_eq!(config.external_builtins.len(), 1);
        assert_eq!(config.library_paths, vec!["/custom/lib"]);
        assert_eq!(config.libraries, vec!["myruntime"]);
    }

    #[test]
    fn test_external_builtin_full_pipeline() {
        // Test that external builtins work through the full compile pipeline
        // including parser, AST validation, type checker, and codegen
        use crate::compile_to_ir_with_config;
        use crate::config::{CompilerConfig, ExternalBuiltin};

        let source = r#"
            : main ( -- Int )
              42 my-transform
              0
            ;
        "#;

        let config = CompilerConfig::new().with_builtin(ExternalBuiltin::new(
            "my-transform",
            "ext_runtime_transform",
        ));

        // This should succeed - the external builtin is registered
        let result = compile_to_ir_with_config(source, &config);
        assert!(
            result.is_ok(),
            "Compilation should succeed: {:?}",
            result.err()
        );

        let ir = result.unwrap();
        assert!(ir.contains("declare ptr @ext_runtime_transform(ptr)"));
        assert!(ir.contains("call ptr @ext_runtime_transform"));
    }

    #[test]
    fn test_external_builtin_without_config_fails() {
        // Test that using an external builtin without config fails validation
        use crate::compile_to_ir;

        let source = r#"
            : main ( -- Int )
              42 unknown-builtin
              0
            ;
        "#;

        // This should fail - unknown-builtin is not registered
        let result = compile_to_ir(source);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown-builtin"));
    }

    #[test]
    fn test_match_exhaustiveness_error() {
        use crate::compile_to_ir;

        let source = r#"
            union Result { Ok { value: Int } Err { msg: String } }

            : handle ( Variant -- Int )
              match
                Ok -> drop 1
                # Missing Err arm!
              end
            ;

            : main ( -- ) 42 Make-Ok handle drop ;
        "#;

        let result = compile_to_ir(source);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Non-exhaustive match"));
        assert!(err.contains("Result"));
        assert!(err.contains("Err"));
    }

    #[test]
    fn test_match_exhaustive_compiles() {
        use crate::compile_to_ir;

        let source = r#"
            union Result { Ok { value: Int } Err { msg: String } }

            : handle ( Variant -- Int )
              match
                Ok -> drop 1
                Err -> drop 0
              end
            ;

            : main ( -- ) 42 Make-Ok handle drop ;
        "#;

        let result = compile_to_ir(source);
        assert!(
            result.is_ok(),
            "Exhaustive match should compile: {:?}",
            result
        );
    }

    #[test]
    fn test_codegen_symbol() {
        // Test symbol literal codegen
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    Statement::Symbol("hello".to_string()),
                    Statement::WordCall {
                        name: "symbol->string".to_string(),
                        span: None,
                    },
                    Statement::WordCall {
                        name: "io.write-line".to_string(),
                        span: None,
                    },
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        assert!(ir.contains("call ptr @patch_seq_push_interned_symbol"));
        assert!(ir.contains("call ptr @patch_seq_symbol_to_string"));
        assert!(ir.contains("\"hello\\00\""));
    }

    #[test]
    fn test_symbol_interning_dedup() {
        // Issue #166: Test that duplicate symbol literals share the same global
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![WordDef {
                name: "main".to_string(),
                effect: None,
                body: vec![
                    // Use :hello twice - should share the same .sym global
                    Statement::Symbol("hello".to_string()),
                    Statement::Symbol("hello".to_string()),
                    Statement::Symbol("world".to_string()), // Different symbol
                ],
                source: None,
            }],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Should have exactly one .sym global for "hello" and one for "world"
        // Count occurrences of symbol global definitions (lines starting with @.sym)
        let sym_defs: Vec<_> = ir
            .lines()
            .filter(|l| l.trim().starts_with("@.sym."))
            .collect();

        // There should be 2 definitions: .sym.0 for "hello" and .sym.1 for "world"
        assert_eq!(
            sym_defs.len(),
            2,
            "Expected 2 symbol globals, got: {:?}",
            sym_defs
        );

        // Verify deduplication: :hello appears twice but .sym.0 is reused
        let hello_uses: usize = ir.matches("@.sym.0").count();
        assert_eq!(
            hello_uses, 3,
            "Expected 3 occurrences of .sym.0 (1 def + 2 uses)"
        );

        // The IR should contain static symbol structure with capacity=0
        assert!(
            ir.contains("i64 0, i8 1"),
            "Symbol global should have capacity=0 and global=1"
        );
    }

    #[test]
    fn test_dup_optimization_for_int() {
        // Test that dup on Int uses optimized load/store instead of clone_value
        // This verifies the Issue #186 optimization actually fires
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "test_dup".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(42), // stmt 0: push Int
                        Statement::WordCall {
                            // stmt 1: dup
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "test_dup".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        // Provide type info: before statement 1 (dup), top of stack is Int
        let mut statement_types = HashMap::new();
        statement_types.insert(("test_dup".to_string(), 1), Type::Int);

        let ir = codegen
            .codegen_program(&program, HashMap::new(), statement_types)
            .unwrap();

        // Extract just the test_dup function
        let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
        let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
        let test_dup_fn = &ir[func_start..func_end];

        // The optimized path should use load/store %Value directly
        assert!(
            test_dup_fn.contains("load %Value"),
            "Optimized dup should use 'load %Value', got:\n{}",
            test_dup_fn
        );
        assert!(
            test_dup_fn.contains("store %Value"),
            "Optimized dup should use 'store %Value', got:\n{}",
            test_dup_fn
        );

        // The optimized path should NOT call clone_value
        assert!(
            !test_dup_fn.contains("@patch_seq_clone_value"),
            "Optimized dup should NOT call clone_value for Int, got:\n{}",
            test_dup_fn
        );
    }

    #[test]
    fn test_dup_optimization_after_literal() {
        // Test Issue #195: dup after literal push uses optimized path
        // Pattern: `42 dup` should be optimized even without type map info
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "test_dup".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(42), // Previous statement is Int literal
                        Statement::WordCall {
                            // dup should be optimized
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "test_dup".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        // No type info provided - but literal heuristic should optimize
        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Extract just the test_dup function
        let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
        let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
        let test_dup_fn = &ir[func_start..func_end];

        // With literal heuristic, should use optimized path
        assert!(
            test_dup_fn.contains("load %Value"),
            "Dup after int literal should use optimized load, got:\n{}",
            test_dup_fn
        );
        assert!(
            test_dup_fn.contains("store %Value"),
            "Dup after int literal should use optimized store, got:\n{}",
            test_dup_fn
        );
        assert!(
            !test_dup_fn.contains("@patch_seq_clone_value"),
            "Dup after int literal should NOT call clone_value, got:\n{}",
            test_dup_fn
        );
    }

    #[test]
    fn test_dup_no_optimization_after_word_call() {
        // Test that dup after word call (unknown type) uses safe clone_value path
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "get_value".to_string(),
                    effect: None,
                    body: vec![Statement::IntLiteral(42)],
                    source: None,
                },
                WordDef {
                    name: "test_dup".to_string(),
                    effect: None,
                    body: vec![
                        Statement::WordCall {
                            // Previous statement is word call (unknown type)
                            name: "get_value".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            // dup should NOT be optimized
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "test_dup".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        // No type info provided and no literal before dup
        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Extract just the test_dup function
        let func_start = ir.find("define tailcc ptr @seq_test_dup").unwrap();
        let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
        let test_dup_fn = &ir[func_start..func_end];

        // Without literal or type info, should call clone_value (safe path)
        assert!(
            test_dup_fn.contains("@patch_seq_clone_value"),
            "Dup after word call should call clone_value, got:\n{}",
            test_dup_fn
        );
    }

    #[test]
    fn test_roll_constant_optimization() {
        // Test Issue #192: roll with constant N uses optimized inline code
        // Pattern: `2 roll` should generate rot-like inline code
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "test_roll".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(1),
                        Statement::IntLiteral(2),
                        Statement::IntLiteral(3),
                        Statement::IntLiteral(2), // Constant N for roll
                        Statement::WordCall {
                            // 2 roll = rot
                            name: "roll".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "test_roll".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Extract just the test_roll function
        let func_start = ir.find("define tailcc ptr @seq_test_roll").unwrap();
        let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
        let test_roll_fn = &ir[func_start..func_end];

        // With constant N=2, should NOT do dynamic calculation
        // Should NOT have dynamic add/sub for offset calculation
        assert!(
            !test_roll_fn.contains("= add i64 %"),
            "Constant roll should use constant offset, not dynamic add, got:\n{}",
            test_roll_fn
        );

        // Should NOT call memmove for small N (n=2 uses direct loads/stores)
        assert!(
            !test_roll_fn.contains("@llvm.memmove"),
            "2 roll should not use memmove, got:\n{}",
            test_roll_fn
        );
    }

    #[test]
    fn test_pick_constant_optimization() {
        // Test Issue #192: pick with constant N uses constant offset
        // Pattern: `1 pick` should generate code with constant -3 offset
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "test_pick".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(10),
                        Statement::IntLiteral(20),
                        Statement::IntLiteral(1), // Constant N for pick
                        Statement::WordCall {
                            // 1 pick = over
                            name: "pick".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "drop".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "test_pick".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Extract just the test_pick function
        let func_start = ir.find("define tailcc ptr @seq_test_pick").unwrap();
        let func_end = ir[func_start..].find("\n}\n").unwrap() + func_start + 3;
        let test_pick_fn = &ir[func_start..func_end];

        // With constant N=1, should use constant offset -3
        // Should NOT have dynamic add/sub for offset calculation
        assert!(
            !test_pick_fn.contains("= add i64 %"),
            "Constant pick should use constant offset, not dynamic add, got:\n{}",
            test_pick_fn
        );

        // Should have the constant offset -3 in getelementptr
        assert!(
            test_pick_fn.contains("i64 -3"),
            "1 pick should use offset -3 (-(1+2)), got:\n{}",
            test_pick_fn
        );
    }

    #[test]
    fn test_small_word_marked_alwaysinline() {
        // Test Issue #187: Small words get alwaysinline attribute
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "double".to_string(), // Small word: dup i.+
                    effect: None,
                    body: vec![
                        Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::WordCall {
                            name: "i.+".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(21),
                        Statement::WordCall {
                            name: "double".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
            ],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Small word 'double' should have alwaysinline attribute
        assert!(
            ir.contains("define tailcc ptr @seq_double(ptr %stack) alwaysinline"),
            "Small word should have alwaysinline attribute, got:\n{}",
            ir.lines()
                .filter(|l| l.contains("define"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // main should NOT have alwaysinline (uses C calling convention)
        assert!(
            ir.contains("define ptr @seq_main(ptr %stack) {"),
            "main should not have alwaysinline, got:\n{}",
            ir.lines()
                .filter(|l| l.contains("define"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_recursive_word_not_inlined() {
        // Test Issue #187: Recursive words should NOT get alwaysinline
        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "countdown".to_string(), // Recursive
                    effect: None,
                    body: vec![
                        Statement::WordCall {
                            name: "dup".to_string(),
                            span: None,
                        },
                        Statement::If {
                            then_branch: vec![
                                Statement::IntLiteral(1),
                                Statement::WordCall {
                                    name: "i.-".to_string(),
                                    span: None,
                                },
                                Statement::WordCall {
                                    name: "countdown".to_string(), // Recursive call
                                    span: None,
                                },
                            ],
                            else_branch: Some(vec![]),
                        },
                    ],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![
                        Statement::IntLiteral(5),
                        Statement::WordCall {
                            name: "countdown".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                },
            ],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Recursive word should NOT have alwaysinline
        assert!(
            ir.contains("define tailcc ptr @seq_countdown(ptr %stack) {"),
            "Recursive word should NOT have alwaysinline, got:\n{}",
            ir.lines()
                .filter(|l| l.contains("define"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    #[test]
    fn test_recursive_word_in_match_not_inlined() {
        // Test Issue #187: Recursive calls inside match arms should prevent inlining
        use crate::ast::{MatchArm, Pattern, UnionDef, UnionVariant};

        let mut codegen = CodeGen::new();

        let program = Program {
            includes: vec![],
            unions: vec![UnionDef {
                name: "Option".to_string(),
                variants: vec![
                    UnionVariant {
                        name: "Some".to_string(),
                        fields: vec![],
                        source: None,
                    },
                    UnionVariant {
                        name: "None".to_string(),
                        fields: vec![],
                        source: None,
                    },
                ],
                source: None,
            }],
            words: vec![
                WordDef {
                    name: "process".to_string(), // Recursive in match arm
                    effect: None,
                    body: vec![Statement::Match {
                        arms: vec![
                            MatchArm {
                                pattern: Pattern::Variant("Some".to_string()),
                                body: vec![Statement::WordCall {
                                    name: "process".to_string(), // Recursive call
                                    span: None,
                                }],
                            },
                            MatchArm {
                                pattern: Pattern::Variant("None".to_string()),
                                body: vec![],
                            },
                        ],
                    }],
                    source: None,
                },
                WordDef {
                    name: "main".to_string(),
                    effect: None,
                    body: vec![Statement::WordCall {
                        name: "process".to_string(),
                        span: None,
                    }],
                    source: None,
                },
            ],
        };

        let ir = codegen
            .codegen_program(&program, HashMap::new(), HashMap::new())
            .unwrap();

        // Recursive word (via match arm) should NOT have alwaysinline
        assert!(
            ir.contains("define tailcc ptr @seq_process(ptr %stack) {"),
            "Recursive word in match should NOT have alwaysinline, got:\n{}",
            ir.lines()
                .filter(|l| l.contains("define"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
