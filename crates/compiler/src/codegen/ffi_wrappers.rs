//! FFI Wrapper Code Generation
//!
//! Generates LLVM IR wrapper functions that bridge between Seq's stack-based
//! calling convention and C's register-based calling convention.

use super::{CodeGen, CodeGenError, ffi_return_type, mangle_name};
use crate::ffi::{FfiType, Ownership, PassMode};
use std::fmt::Write as _;

impl CodeGen {
    /// Generate FFI wrapper functions
    pub(super) fn generate_ffi_wrappers(&mut self) -> Result<(), CodeGenError> {
        // Collect functions to avoid borrowing self.ffi_bindings while mutating self
        let funcs: Vec<_> = self.ffi_bindings.functions.values().cloned().collect();
        for func in funcs {
            self.generate_ffi_wrapper(&func)?;
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // FFI Wrapper Helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Allocate storage for a by_ref out parameter
    fn write_ffi_by_ref_alloca(
        &mut self,
        i: usize,
        ffi_type: &FfiType,
    ) -> Result<String, CodeGenError> {
        let alloca_var = format!("out_param_{}", i);
        let llvm_type = match ffi_type {
            FfiType::Ptr => "ptr",
            FfiType::Int => "i64",
            _ => {
                return Err(CodeGenError::Logic(format!(
                    "Unsupported type {:?} for by_ref parameter",
                    ffi_type
                )));
            }
        };
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = alloca {}",
            alloca_var, llvm_type
        )?;
        Ok(alloca_var)
    }

    /// Pop an FFI argument from the stack and return (c_arg_string, optional_cstr_var_to_free)
    fn write_ffi_pop_arg(
        &mut self,
        i: usize,
        arg: &crate::ffi::FfiArg,
        stack_var: &mut String,
    ) -> Result<(String, Option<String>), CodeGenError> {
        // Handle fixed value arguments
        if let Some(ref value) = arg.value {
            return match value.as_str() {
                "null" | "NULL" => Ok(("ptr null".to_string(), None)),
                _ => value
                    .parse::<i64>()
                    .map(|int_val| (format!("i64 {}", int_val), None))
                    .map_err(|e| {
                        CodeGenError::Logic(format!(
                            "Invalid fixed value '{}' for argument {}: {}. \
                         Expected 'null' or a 64-bit integer.",
                            value, i, e
                        ))
                    }),
            };
        }

        match (&arg.arg_type, &arg.pass) {
            (_, PassMode::ByRef) => {
                // by_ref args don't pop from stack - just reference the alloca
                Ok((format!("ptr %out_param_{}", i), None))
            }
            (FfiType::String, PassMode::CString) => self.write_ffi_pop_cstring(i, stack_var),
            (FfiType::Int, _) => self.write_ffi_pop_int(i, stack_var).map(|s| (s, None)),
            (FfiType::Ptr, PassMode::Ptr) => {
                self.write_ffi_pop_ptr(i, stack_var).map(|s| (s, None))
            }
            _ => Err(CodeGenError::Logic(format!(
                "Unsupported FFI argument type {:?} with pass mode {:?}",
                arg.arg_type, arg.pass
            ))),
        }
    }

    /// Pop a C string argument from the stack - returns (c_arg, cstr_var_to_free)
    fn write_ffi_pop_cstring(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<(String, Option<String>), CodeGenError> {
        let cstr_var = format!("cstr_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_string_to_cstring(ptr %{}, ptr null)",
            cstr_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok((format!("ptr %{}", cstr_var), Some(cstr_var)))
    }

    /// Pop an integer argument from the stack
    fn write_ffi_pop_int(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<String, CodeGenError> {
        let int_var = format!("int_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
            int_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok(format!("i64 %{}", int_var))
    }

    /// Pop a pointer argument from the stack
    fn write_ffi_pop_ptr(
        &mut self,
        i: usize,
        stack_var: &mut String,
    ) -> Result<String, CodeGenError> {
        let int_var = format!("ptr_int_{}", i);
        let ptr_var = format!("ptr_{}", i);
        let new_stack = format!("stack_after_pop_{}", i);

        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call i64 @patch_seq_peek_int_value(ptr %{})",
            int_var, stack_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = inttoptr i64 %{} to ptr",
            ptr_var, int_var
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %{} = call ptr @patch_seq_pop_stack(ptr %{})",
            new_stack, stack_var
        )?;

        *stack_var = new_stack;
        Ok(format!("ptr %{}", ptr_var))
    }

    /// Push a by_ref out parameter result onto the stack
    fn write_ffi_push_by_ref_result(
        &mut self,
        alloca_var: &str,
        ffi_type: &FfiType,
        stack_var: &mut String,
    ) -> Result<(), CodeGenError> {
        let new_stack = format!("stack_after_byref_{}", alloca_var);
        match ffi_type {
            FfiType::Ptr => {
                let loaded_var = format!("{}_val", alloca_var);
                let int_var = format!("{}_int", alloca_var);
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = load ptr, ptr %{}",
                    loaded_var, alloca_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = ptrtoint ptr %{} to i64",
                    int_var, loaded_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                    new_stack, stack_var, int_var
                )?;
            }
            FfiType::Int => {
                let loaded_var = format!("{}_val", alloca_var);
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = load i64, ptr %{}",
                    loaded_var, alloca_var
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %{} = call ptr @patch_seq_push_int(ptr %{}, i64 %{})",
                    new_stack, stack_var, loaded_var
                )?;
            }
            _ => return Ok(()), // Other types not supported for by_ref
        }
        *stack_var = new_stack;
        Ok(())
    }

    /// Handle FFI return value - string type (with NULL check)
    fn write_ffi_return_string(
        &mut self,
        stack_var: &str,
        caller_frees: bool,
    ) -> Result<(), CodeGenError> {
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %is_null = icmp eq ptr %c_result, null"
        )?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  br i1 %is_null, label %null_case, label %valid_case"
        )?;

        // NULL case - push empty string
        writeln!(&mut self.ffi_wrapper_code, "null_case:")?;
        let empty_str = self.get_string_global("")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %stack_null = call ptr @patch_seq_push_string(ptr %{}, ptr {})",
            stack_var, empty_str
        )?;
        writeln!(&mut self.ffi_wrapper_code, "  br label %done")?;

        // Valid case - convert C string to Seq string
        writeln!(&mut self.ffi_wrapper_code, "valid_case:")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %stack_with_result = call ptr @patch_seq_cstring_to_string(ptr %{}, ptr %c_result)",
            stack_var
        )?;
        if caller_frees {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call void @free(ptr %c_result)"
            )?;
        }
        writeln!(&mut self.ffi_wrapper_code, "  br label %done")?;

        // Join paths
        writeln!(&mut self.ffi_wrapper_code, "done:")?;
        writeln!(
            &mut self.ffi_wrapper_code,
            "  %final_stack = phi ptr [ %stack_null, %null_case ], [ %stack_with_result, %valid_case ]"
        )?;
        writeln!(&mut self.ffi_wrapper_code, "  ret ptr %final_stack")?;
        Ok(())
    }

    /// Handle FFI return value - simple types (Int, Ptr, Void)
    fn write_ffi_return_simple(
        &mut self,
        return_type: &FfiType,
        stack_var: &str,
    ) -> Result<(), CodeGenError> {
        match return_type {
            FfiType::Int => {
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %stack_with_result = call ptr @patch_seq_push_int(ptr %{}, i64 %c_result)",
                    stack_var
                )?;
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %stack_with_result")?;
            }
            FfiType::Void => {
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %{}", stack_var)?;
            }
            FfiType::Ptr => {
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %ptr_as_int = ptrtoint ptr %c_result to i64"
                )?;
                writeln!(
                    &mut self.ffi_wrapper_code,
                    "  %stack_with_result = call ptr @patch_seq_push_int(ptr %{}, i64 %ptr_as_int)",
                    stack_var
                )?;
                writeln!(&mut self.ffi_wrapper_code, "  ret ptr %stack_with_result")?;
            }
            FfiType::String => {
                // String is handled by write_ffi_return_string
                unreachable!("String return should use write_ffi_return_string");
            }
        }
        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Main FFI Wrapper Generator
    // ─────────────────────────────────────────────────────────────────────────

    /// Generate a single FFI wrapper function
    ///
    /// The wrapper:
    /// 1. Pops arguments from the Seq stack
    /// 2. Converts Seq types to C types
    /// 3. Calls the C function
    /// 4. Converts result back to Seq type
    /// 5. Pushes result onto Seq stack
    /// 6. Frees memory if needed (caller_frees)
    fn generate_ffi_wrapper(
        &mut self,
        func: &crate::ffi::FfiFunctionInfo,
    ) -> Result<(), CodeGenError> {
        let wrapper_name = format!("seq_ffi_{}", mangle_name(&func.seq_name));

        writeln!(
            &mut self.ffi_wrapper_code,
            "define ptr @{}(ptr %stack) {{",
            wrapper_name
        )?;
        writeln!(&mut self.ffi_wrapper_code, "entry:")?;

        let mut stack_var = "stack".to_string();
        let mut c_args: Vec<String> = Vec::new();
        let mut c_string_vars: Vec<String> = Vec::new();
        let mut by_ref_vars: Vec<(String, FfiType)> = Vec::new();

        // First pass: allocate storage for by_ref out parameters
        for (i, arg) in func.args.iter().enumerate() {
            if arg.pass == PassMode::ByRef {
                let alloca_var = self.write_ffi_by_ref_alloca(i, &arg.arg_type)?;
                by_ref_vars.push((alloca_var, arg.arg_type.clone()));
            }
        }

        // Second pass: pop arguments from stack (in reverse order - last arg on top)
        for (i, arg) in func.args.iter().enumerate().rev() {
            let (c_arg, cstr_var) = self.write_ffi_pop_arg(i, arg, &mut stack_var)?;
            c_args.push(c_arg);
            if let Some(var) = cstr_var {
                c_string_vars.push(var);
            }
        }

        // Reverse args back to correct order for C call
        c_args.reverse();

        // Generate the C function call
        let c_ret_type = ffi_return_type(&func.return_spec);
        let c_args_str = c_args.join(", ");
        let has_return = func
            .return_spec
            .as_ref()
            .is_some_and(|r| r.return_type != FfiType::Void);

        if has_return {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  %c_result = call {} @{}({})",
                c_ret_type, func.c_name, c_args_str
            )?;
        } else {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call {} @{}({})",
                c_ret_type, func.c_name, c_args_str
            )?;
        }

        // Free C strings we allocated for arguments
        for cstr_var in &c_string_vars {
            writeln!(
                &mut self.ffi_wrapper_code,
                "  call void @free(ptr %{})",
                cstr_var
            )?;
        }

        // Push by_ref out parameter values onto stack
        for (alloca_var, ffi_type) in &by_ref_vars {
            self.write_ffi_push_by_ref_result(alloca_var, ffi_type, &mut stack_var)?;
        }

        // Handle return value
        if let Some(ref return_spec) = func.return_spec {
            if return_spec.return_type == FfiType::String {
                self.write_ffi_return_string(
                    &stack_var,
                    return_spec.ownership == Ownership::CallerFrees,
                )?;
            } else {
                self.write_ffi_return_simple(&return_spec.return_type, &stack_var)?;
            }
        } else {
            writeln!(&mut self.ffi_wrapper_code, "  ret ptr %{}", stack_var)?;
        }

        writeln!(&mut self.ffi_wrapper_code, "}}")?;
        writeln!(&mut self.ffi_wrapper_code)?;

        Ok(())
    }
}
