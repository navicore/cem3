//! File I/O Operations for Seq
//!
//! Provides file reading operations for Seq programs.
//!
//! # Usage from Seq
//!
//! ```seq
//! "config.json" file-slurp  # ( String -- String ) read entire file
//! "config.json" file-exists?  # ( String -- Int ) 1 if exists, 0 otherwise
//! "data.txt" [ process-line ] file-for-each-line+  # ( String Quotation -- String Int )
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

use crate::stack::{Stack, pop, push};
use crate::value::{Value, VariantData};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Arc;

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
        Value::String(path) => match fs::read_to_string(path.as_str()) {
            Ok(contents) => {
                let stack = unsafe { push(rest, Value::String(contents.into())) };
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
            let exists = Path::new(path.as_str()).exists();
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
/// Stack effect: ( String Quotation -- String Int )
///
/// Opens the file, calls the quotation with each line (including newline),
/// then closes the file.
///
/// Returns:
/// - Success: ( "" 1 )
/// - Error: ( "error message" 0 )
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
/// "data.txt" [ string-chomp process-line ] file-for-each-line+
/// if
///     "Done processing" write_line
/// else
///     "Error: " swap string-concat write_line
/// then
/// ```
///
/// # Safety
/// - `stack` must be a valid, non-null stack pointer
/// - Top of stack must be a Quotation or Closure
/// - Second on stack must be a String (file path)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_file_for_each_line_plus(stack: Stack) -> Stack {
    assert!(!stack.is_null(), "file-for-each-line+: stack is empty");

    // Pop quotation
    let (stack, quot_value) = unsafe { pop(stack) };

    // Pop path
    let (stack, path_value) = unsafe { pop(stack) };
    let path = match path_value {
        Value::String(s) => s,
        _ => panic!(
            "file-for-each-line+: expected String path, got {:?}",
            path_value
        ),
    };

    // Open file
    let file = match File::open(path.as_str()) {
        Ok(f) => f,
        Err(e) => {
            // Return error: ( "error message" 0 )
            let stack = unsafe { push(stack, Value::String(e.to_string().into())) };
            return unsafe { push(stack, Value::Int(0)) };
        }
    };

    // Extract function pointer and optionally closure environment
    let (wrapper, env_data, env_len): (usize, *const Value, usize) = match quot_value {
        Value::Quotation { wrapper, .. } => {
            if wrapper == 0 {
                panic!("file-for-each-line+: quotation wrapper function pointer is null");
            }
            (wrapper, std::ptr::null(), 0)
        }
        Value::Closure { fn_ptr, ref env } => {
            if fn_ptr == 0 {
                panic!("file-for-each-line+: closure function pointer is null");
            }
            (fn_ptr, env.as_ptr(), env.len())
        }
        _ => panic!(
            "file-for-each-line+: expected Quotation or Closure, got {:?}",
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
            Err(e) => {
                // I/O error mid-file
                let stack = unsafe { push(current_stack, Value::String(e.to_string().into())) };
                return unsafe { push(stack, Value::Bool(false)) };
            }
        }
    }

    // Success: ( "" true )
    let stack = unsafe { push(current_stack, Value::String("".into())) };
    unsafe { push(stack, Value::Bool(true)) }
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

    match fs::write(path.as_str(), content.as_str()) {
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
        .open(path.as_str())
        .and_then(|mut file| file.write_all(content.as_str().as_bytes()));

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

    match fs::remove_file(path.as_str()) {
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

    match fs::metadata(path.as_str()) {
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

    let exists = Path::new(path.as_str()).is_dir();
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

    match fs::create_dir_all(path.as_str()) {
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

    match fs::remove_dir(path.as_str()) {
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

    match fs::read_dir(path.as_str()) {
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
pub use patch_seq_file_for_each_line_plus as file_for_each_line_plus;
pub use patch_seq_file_size as file_size;
pub use patch_seq_file_slurp as file_slurp;
pub use patch_seq_file_spit as file_spit;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_file_slurp() {
        // Create a temporary file with known contents
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "Hello, file!").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            // file-slurp now returns (contents Bool)
            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            let (_stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str().trim(), "Hello, file!"),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_file_exists_true() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_exists(stack);

            let (_stack, value) = pop(stack);
            assert_eq!(value, Value::Bool(true));
        }
    }

    #[test]
    fn test_file_exists_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/path/to/file.txt".into()));
            let stack = patch_seq_file_exists(stack);

            let (_stack, value) = pop(stack);
            assert_eq!(value, Value::Bool(false));
        }
    }

    #[test]
    fn test_file_slurp_utf8() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "Hello, 世界! 🌍").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            // file-slurp returns (contents Bool)
            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            let (_stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str(), "Hello, 世界! 🌍"),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_file_slurp_empty() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_slurp(stack);

            // file-slurp returns (contents Bool)
            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true)); // Empty file is still success
            let (_stack, value) = pop(stack);
            match value {
                Value::String(s) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected String"),
            }
        }
    }

    #[test]
    fn test_file_slurp_not_found() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/path/to/file.txt".into()));
            let stack = patch_seq_file_slurp(stack);

            let (stack, success) = pop(stack);
            let (_stack, contents) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            match contents {
                Value::String(s) => assert_eq!(s.as_str(), ""),
                _ => panic!("Expected String"),
            }
        }
    }

    // ==========================================================================
    // Tests for file.spit
    // ==========================================================================

    #[test]
    fn test_file_spit_creates_new_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test.txt");
        let path_str = path.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("hello world".into()));
            let stack = push(stack, Value::String(path_str.clone().into()));
            let stack = patch_seq_file_spit(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        // Verify file was created with correct contents
        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn test_file_spit_overwrites_existing() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "old content").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("new content".into()));
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_file_spit(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "new content");
    }

    #[test]
    fn test_file_spit_invalid_path() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("content".into()));
            let stack = push(stack, Value::String("/nonexistent/dir/file.txt".into()));
            let stack = patch_seq_file_spit(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));
        }
    }

    // ==========================================================================
    // Tests for file.append
    // ==========================================================================

    #[test]
    fn test_file_append_to_existing() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "hello").unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(" world".into()));
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_file_append(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "hello world");
    }

    #[test]
    fn test_file_append_creates_new() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("new.txt");
        let path_str = path.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("content".into()));
            let stack = push(stack, Value::String(path_str.clone().into()));
            let stack = patch_seq_file_append(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        let contents = std::fs::read_to_string(&path).unwrap();
        assert_eq!(contents, "content");
    }

    // ==========================================================================
    // Tests for file.delete
    // ==========================================================================

    #[test]
    fn test_file_delete_existing() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();
        // Keep path but drop temp_file so we control the file
        let path_copy = path.clone();
        drop(temp_file);
        std::fs::write(&path_copy, "content").unwrap();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path_copy.clone().into()));
            let stack = patch_seq_file_delete(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        assert!(!std::path::Path::new(&path_copy).exists());
    }

    #[test]
    fn test_file_delete_nonexistent() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/file.txt".into()));
            let stack = patch_seq_file_delete(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));
        }
    }

    // ==========================================================================
    // Tests for file.size
    // ==========================================================================

    #[test]
    fn test_file_size_existing() {
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "hello world").unwrap(); // 11 bytes
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_file_size(stack);

            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
            let (_stack, size) = pop(stack);
            assert_eq!(size, Value::Int(11));
        }
    }

    #[test]
    fn test_file_size_nonexistent() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/file.txt".into()));
            let stack = patch_seq_file_size(stack);

            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));
            let (_stack, size) = pop(stack);
            assert_eq!(size, Value::Int(0));
        }
    }

    // ==========================================================================
    // Tests for dir.exists?
    // ==========================================================================

    #[test]
    fn test_dir_exists_true() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_dir_exists(stack);

            let (_stack, exists) = pop(stack);
            assert_eq!(exists, Value::Bool(true));
        }
    }

    #[test]
    fn test_dir_exists_false() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/directory".into()));
            let stack = patch_seq_dir_exists(stack);

            let (_stack, exists) = pop(stack);
            assert_eq!(exists, Value::Bool(false));
        }
    }

    #[test]
    fn test_dir_exists_file_is_not_dir() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_dir_exists(stack);

            let (_stack, exists) = pop(stack);
            assert_eq!(exists, Value::Bool(false)); // file is not a directory
        }
    }

    // ==========================================================================
    // Tests for dir.make
    // ==========================================================================

    #[test]
    fn test_dir_make_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let new_dir = temp_dir.path().join("newdir");
        let path = new_dir.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_dir_make(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        assert!(new_dir.is_dir());
    }

    #[test]
    fn test_dir_make_nested() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested = temp_dir.path().join("a").join("b").join("c");
        let path = nested.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_dir_make(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        assert!(nested.is_dir());
    }

    // ==========================================================================
    // Tests for dir.delete
    // ==========================================================================

    #[test]
    fn test_dir_delete_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let to_delete = temp_dir.path().join("to_delete");
        std::fs::create_dir(&to_delete).unwrap();
        let path = to_delete.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_dir_delete(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));
        }

        assert!(!to_delete.exists());
    }

    #[test]
    fn test_dir_delete_nonempty_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let to_delete = temp_dir.path().join("nonempty");
        std::fs::create_dir(&to_delete).unwrap();
        std::fs::write(to_delete.join("file.txt"), "content").unwrap();
        let path = to_delete.to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.clone().into()));
            let stack = patch_seq_dir_delete(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false)); // can't delete non-empty
        }

        assert!(to_delete.exists());
    }

    #[test]
    fn test_dir_delete_nonexistent() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/dir".into()));
            let stack = patch_seq_dir_delete(stack);

            let (_stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));
        }
    }

    // ==========================================================================
    // Tests for dir.list
    // ==========================================================================

    #[test]
    fn test_dir_list_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        std::fs::write(temp_dir.path().join("a.txt"), "a").unwrap();
        std::fs::write(temp_dir.path().join("b.txt"), "b").unwrap();
        let path = temp_dir.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_dir_list(stack);

            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));

            let (_stack, list) = pop(stack);
            match list {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
                    assert_eq!(v.fields.len(), 2);
                }
                _ => panic!("Expected Variant(List)"),
            }
        }
    }

    #[test]
    fn test_dir_list_empty() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().to_str().unwrap().to_string();

        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String(path.into()));
            let stack = patch_seq_dir_list(stack);

            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(true));

            let (_stack, list) = pop(stack);
            match list {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
                    assert_eq!(v.fields.len(), 0);
                }
                _ => panic!("Expected Variant(List)"),
            }
        }
    }

    #[test]
    fn test_dir_list_nonexistent() {
        unsafe {
            let stack = crate::stack::alloc_test_stack();
            let stack = push(stack, Value::String("/nonexistent/dir".into()));
            let stack = patch_seq_dir_list(stack);

            let (stack, success) = pop(stack);
            assert_eq!(success, Value::Bool(false));

            let (_stack, list) = pop(stack);
            match list {
                Value::Variant(v) => {
                    assert_eq!(v.tag.as_str(), "List");
                    assert_eq!(v.fields.len(), 0); // empty list on failure
                }
                _ => panic!("Expected Variant(List)"),
            }
        }
    }
}
