//! File I/O Operations for Seq
//!
//! Provides file reading operations for Seq programs.
//!
//! # Usage from Seq
//!
//! ```seq
//! "config.json" file-slurp  # ( String -- String ) read entire file
//! "config.json" file-exists?  # ( String -- Int ) 1 if exists, 0 otherwise
//! "data.txt" [ process-line ] file-for-each-line  # ( String Quotation -- Bool )
//! ```
//!
//! # Example
//!
//! ```seq
//! : main ( -- Int )
//!   "config.json" file-exists? if
//!     "config.json" file-slurp write_line
//!   else
//!     "File not found" write_line
//!   then
//!   0
//! ;
//! ```

use crate::seqstring::global_bytes;
use crate::stack::{Stack, pop, push};
use crate::value::{Value, VariantData};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Arc;

/// Path conversion idiom: paths are inherently text on the OS APIs we
/// target (Linux/macOS POSIX, which expose `&str` via Rust's `Path`),
/// so non-UTF-8 path bytes can't be handed to the OS as-is.
/// `SeqString::as_str_or_empty()` returns `""` for non-UTF-8 input,
/// which routes the call through the OS error path and produces the
/// standard `(empty, false)` failure tuple — same observable result
/// as if we'd validated upfront. Helper kept for readability.
fn path_str(s: &crate::seqstring::SeqString) -> &str {
    s.as_str_or_empty()
}

/// Read entire file contents as a string
///
/// Stack effect: ( String -- String Bool )
///
/// Takes a file path, attempts to read the entire file.
/// Returns (contents true) on success, or ("" false) on failure.
/// Errors are values, not crashes.
/// Panics only for internal bugs (wrong stack type).
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with a String value on top
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_slurp(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-slurp: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        // Read the file as raw bytes — `fs::read` returns `Vec<u8>` and
        // imposes no UTF-8 requirement, so binary file slurp now works.
        // Wrap the bytes directly into a byte-clean SeqString.
        Value::String(path) => match fs::read(path_str(&path)) {
            Ok(contents) => {
                let stack = unsafe { push(rest, Value::String(global_bytes(contents))) };
                unsafe { push(stack, Value::Bool(true)) }
            }
            Err(_) => {
                let stack = unsafe { push(rest, Value::String("".into())) };
                unsafe { push(stack, Value::Bool(false)) }
            }
        },
        _ => panic!("file-slurp: expected String path on stack, got {:?}", value),
    }
}

/// Check if a file exists
///
/// Stack effect: ( String -- Int )
///
/// Takes a file path and returns 1 if the file exists, 0 otherwise.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer with a String value on top
/// - Caller must ensure stack is not concurrently modified
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_exists(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-exists?: stack is empty");

    let (rest, value) = unsafe { pop(stack) };

    match value {
        Value::String(path) => {
            let exists = Path::new(path_str(&path)).exists();
            unsafe { push(rest, Value::Bool(exists)) }
        }
        _ => panic!(
            "file-exists?: expected String path on stack, got {:?}",
            value
        ),
    }
}

/// Process each line of a file with a quotation
///
/// Stack effect: ( String Quotation -- Bool )
///
/// Opens the file, calls the quotation with each line (including newline),
/// then closes the file.
///
/// Returns `true` on success (including empty files), `false` if the file
/// could not be opened or a read error occurred mid-stream.
///
/// The quotation should have effect ( String -- ), receiving each line
/// and consuming it. Empty files return success without calling the quotation.
///
/// # Line Ending Normalization
///
/// Line endings are normalized to `\n` regardless of platform. Windows-style
/// `\r\n` endings are converted to `\n`. This ensures consistent behavior
/// when processing files across different operating systems.
///
/// # Example
///
/// ```seq
/// "data.txt" [ string-chomp process-line ] file-for-each-line
/// [ "Done processing" write_line ]
/// [ "Error reading file" write_line ]
/// if
/// ```
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be a Quotation or Closure
/// - Second on stack must be a String (file path)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_for_each_line(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file.for-each-line: stack is empty");

    // Pop quotation
    let (stack, quot_value) = unsafe { pop(stack) };

    // Pop path
    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!(
            "file.for-each-line: expected String path, got {:?}",
            path_value
        ),
    };

    // Open file
    let file = match File::open(path_str(&path)) {
        Ok(f) => f,
        Err(_) => return unsafe { push(stack, Value::Bool(false)) },
    };

    // Extract function pointer and optionally closure environment
    let (wrapper, env_data, env_len): (usize, *const Value, usize) = match quot_value {
        Value::Quotation { wrapper, .. } => {
            if wrapper == 0 {
                panic!("file.for-each-line: quotation wrapper function pointer is null");
            }
            (wrapper, std::ptr::null(), 0)
        }
        Value::Closure { fn_ptr, ref env } => {
            if fn_ptr == 0 {
                panic!("file.for-each-line: closure function pointer is null");
            }
            (fn_ptr, env.as_ptr(), env.len())
        }
        _ => panic!(
            "file.for-each-line: expected Quotation or Closure, got {:?}",
            quot_value
        ),
    };

    // Read lines and call quotation/closure for each
    let reader = BufReader::new(file);
    let mut current_stack = stack;

    for line_result in reader.lines() {
        match line_result {
            Ok(mut line_str) => {
                // `BufReader::lines()` strips all line endings (\n, \r\n, \r)
                // We add back \n to match read_line behavior and ensure consistent newlines
                line_str.push('\n');

                // Push line onto stack
                current_stack = unsafe { push(current_stack, Value::String(line_str.into())) };

                // Call the quotation or closure
                if env_data.is_null() {
                    // Quotation: just stack -> stack
                    let fn_ref: unsafe extern "C" fn(Stack) -> Stack =
                        unsafe { std::mem::transmute(wrapper) };
                    current_stack = unsafe { fn_ref(current_stack) };
                } else {
                    // Closure: stack, env_ptr, env_len -> stack
                    let fn_ref: unsafe extern "C" fn(Stack, *const Value, usize) -> Stack =
                        unsafe { std::mem::transmute(wrapper) };
                    current_stack = unsafe { fn_ref(current_stack, env_data, env_len) };
                }

                // Yield to scheduler for cooperative multitasking
                may::coroutine::yield_now();
            }
            Err(_) => {
                // I/O error mid-file
                return unsafe { push(current_stack, Value::Bool(false)) };
            }
        }
    }

    unsafe { push(current_stack, Value::Bool(true)) }
}

/// Write string to file (creates or overwrites)
///
/// Stack effect: ( String String -- Bool )
///
/// Takes content and path, writes content to file.
/// Creates the file if it doesn't exist, overwrites if it does.
/// Returns true on success, false on failure.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String), second must be content (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_spit(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file.spit: stack is empty");

    // Pop path (top of stack)
    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("file.spit: expected String path, got {:?}", path_value),
    };

    // Pop content
    let (stack, content_value) = unsafe { pop(stack) };
    let content = match content_value {
        Value::String(s) => s,
        _ => panic!(
            "file.spit: expected String content, got {:?}",
            content_value
        ),
    };

    // Content is byte-clean — `fs::write` accepts any `AsRef<[u8]>`
    // so we don't need UTF-8 validation here. Binary file write works.
    match fs::write(path_str(&path), content.as_bytes()) {
        Ok(()) => unsafe { push(stack, Value::Bool(true)) },
        Err(_) => unsafe { push(stack, Value::Bool(false)) },
    }
}

/// Append string to file (creates if doesn't exist)
///
/// Stack effect: ( String String -- Bool )
///
/// Takes content and path, appends content to file.
/// Creates the file if it doesn't exist.
/// Returns true on success, false on failure.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String), second must be content (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_append(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file.append: stack is empty");

    // Pop path (top of stack)
    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("file.append: expected String path, got {:?}", path_value),
    };

    // Pop content
    let (stack, content_value) = unsafe { pop(stack) };
    let content = match content_value {
        Value::String(s) => s,
        _ => panic!(
            "file.append: expected String content, got {:?}",
            content_value
        ),
    };

    let result = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path_str(&path))
        .and_then(|mut file| file.write_all(content.as_bytes()));

    match result {
        Ok(()) => unsafe { push(stack, Value::Bool(true)) },
        Err(_) => unsafe { push(stack, Value::Bool(false)) },
    }
}

/// Delete a file
///
/// Stack effect: ( String -- Bool )
///
/// Takes a file path and deletes the file.
/// Returns true on success, false on failure (including if file doesn't exist).
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_delete(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file.delete: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("file.delete: expected String path, got {:?}", path_value),
    };

    match fs::remove_file(path_str(&path)) {
        Ok(()) => unsafe { push(stack, Value::Bool(true)) },
        Err(_) => unsafe { push(stack, Value::Bool(false)) },
    }
}

/// Get file size in bytes
///
/// Stack effect: ( String -- Int Bool )
///
/// Takes a file path and returns (size, success).
/// Returns (size, true) on success, (0, false) on failure.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_size(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file.size: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("file.size: expected String path, got {:?}", path_value),
    };

    match fs::metadata(path_str(&path)) {
        Ok(metadata) => {
            let size = metadata.len() as i64;
            let stack = unsafe { push(stack, Value::Int(size)) };
            unsafe { push(stack, Value::Bool(true)) }
        }
        Err(_) => {
            let stack = unsafe { push(stack, Value::Int(0)) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

// =============================================================================
// Directory Operations
// =============================================================================

/// Check if a directory exists
///
/// Stack effect: ( String -- Bool )
///
/// Takes a path and returns true if it exists and is a directory.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dir_exists(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "dir.exists?: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("dir.exists?: expected String path, got {:?}", path_value),
    };

    let exists = Path::new(path_str(&path)).is_dir();
    unsafe { push(stack, Value::Bool(exists)) }
}

/// Create a directory (and parent directories if needed)
///
/// Stack effect: ( String -- Bool )
///
/// Takes a path and creates the directory and any missing parent directories.
/// Returns true on success, false on failure.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dir_make(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "dir.make: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("dir.make: expected String path, got {:?}", path_value),
    };

    match fs::create_dir_all(path_str(&path)) {
        Ok(()) => unsafe { push(stack, Value::Bool(true)) },
        Err(_) => unsafe { push(stack, Value::Bool(false)) },
    }
}

/// Delete an empty directory
///
/// Stack effect: ( String -- Bool )
///
/// Takes a path and deletes the directory (must be empty).
/// Returns true on success, false on failure.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dir_delete(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "dir.delete: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("dir.delete: expected String path, got {:?}", path_value),
    };

    match fs::remove_dir(path_str(&path)) {
        Ok(()) => unsafe { push(stack, Value::Bool(true)) },
        Err(_) => unsafe { push(stack, Value::Bool(false)) },
    }
}

/// List directory contents
///
/// Stack effect: ( String -- List Bool )
///
/// Takes a directory path and returns (list-of-names, success).
/// Returns a list of filenames (strings) on success.
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be path (String)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_dir_list(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "dir.list: stack is empty");

    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!("dir.list: expected String path, got {:?}", path_value),
    };

    match fs::read_dir(path_str(&path)) {
        Ok(entries) => {
            let mut names: Vec<Value> = Vec::new();
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    names.push(Value::String(name.to_string().into()));
                }
            }
            let list = Value::Variant(Arc::new(VariantData::new(
                crate::seqstring::global_string("List".to_string()),
                names,
            )));
            let stack = unsafe { push(stack, list) };
            unsafe { push(stack, Value::Bool(true)) }
        }
        Err(_) => {
            let empty_list = Value::Variant(Arc::new(VariantData::new(
                crate::seqstring::global_string("List".to_string()),
                vec![],
            )));
            let stack = unsafe { push(stack, empty_list) };
            unsafe { push(stack, Value::Bool(false)) }
        }
    }
}

// Public re-exports
pub use patch_seq_dir_delete as dir_delete;
pub use patch_seq_dir_exists as dir_exists;
pub use patch_seq_dir_list as dir_list;
pub use patch_seq_dir_make as dir_make;
pub use patch_seq_file_append as file_append;
pub use patch_seq_file_delete as file_delete;
pub use patch_seq_file_exists as file_exists;
pub use patch_seq_file_for_each_line as file_for_each_line;
pub use patch_seq_file_size as file_size;
pub use patch_seq_file_slurp as file_slurp;
pub use patch_seq_file_spit as file_spit;

#[cfg(test)]
mod tests;
