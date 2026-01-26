//! OS operations for Seq
//!
//! Provides portable OS interaction primitives: environment variables,
//! paths, and system information.
//!
//! These functions are exported with C ABI for LLVM codegen to call.

use crate::seqstring::global_string;
use crate::stack::{Stack, pop, push};
use crate::value::Value;

/// Get an environment variable
///
/// Stack effect: ( name -- value success )
///
/// Returns the value and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack must have a String (variable name) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_getenv(stack: Stack) -> Stack {
    unsafe {
        let (stack, name_val) = pop(stack);
        let name = match name_val {
            Value::String(s) => s,
            _ => panic!(
                "getenv: expected String (name) on stack, got {:?}",
                name_val
            ),
        };

        match std::env::var(name.as_str()) {
            Ok(value) => {
                let stack = push(stack, Value::String(global_string(value)));
                push(stack, Value::Bool(true)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Bool(false)) // failure
            }
        }
    }
}

/// Get the user's home directory
///
/// Stack effect: ( -- path success )
///
/// Returns the path and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_home_dir(stack: Stack) -> Stack {
    unsafe {
        // Try HOME env var first (works on Unix and some Windows configs)
        if let Ok(home) = std::env::var("HOME") {
            let stack = push(stack, Value::String(global_string(home)));
            return push(stack, Value::Bool(true));
        }

        // On Windows, try USERPROFILE
        #[cfg(windows)]
        if let Ok(home) = std::env::var("USERPROFILE") {
            let stack = push(stack, Value::String(global_string(home)));
            return push(stack, Value::Bool(true));
        }

        // Fallback: return empty string with failure flag
        let stack = push(stack, Value::String(global_string(String::new())));
        push(stack, Value::Bool(false))
    }
}

/// Get the current working directory
///
/// Stack effect: ( -- path success )
///
/// Returns the path and 1 on success, "" and 0 on failure.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_current_dir(stack: Stack) -> Stack {
    unsafe {
        match std::env::current_dir() {
            Ok(path) => {
                let path_str = path.to_string_lossy().into_owned();
                let stack = push(stack, Value::String(global_string(path_str)));
                push(stack, Value::Bool(true)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Bool(false)) // failure
            }
        }
    }
}

/// Check if a path exists
///
/// Stack effect: ( path -- exists )
///
/// Returns 1 if path exists, 0 otherwise.
///
/// # Safety
/// Stack must have a String (path) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_exists(stack: Stack) -> Stack {
    unsafe {
        let (stack, path_val) = pop(stack);
        let path = match path_val {
            Value::String(s) => s,
            _ => panic!(
                "path-exists: expected String (path) on stack, got {:?}",
                path_val
            ),
        };

        let exists = std::path::Path::new(path.as_str()).exists();
        push(stack, Value::Bool(exists))
    }
}

/// Check if a path is a regular file
///
/// Stack effect: ( path -- is-file )
///
/// Returns 1 if path is a regular file, 0 otherwise.
///
/// # Safety
/// Stack must have a String (path) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_is_file(stack: Stack) -> Stack {
    unsafe {
        let (stack, path_val) = pop(stack);
        let path = match path_val {
            Value::String(s) => s,
            _ => panic!(
                "path-is-file: expected String (path) on stack, got {:?}",
                path_val
            ),
        };

        let is_file = std::path::Path::new(path.as_str()).is_file();
        push(stack, Value::Bool(is_file))
    }
}

/// Check if a path is a directory
///
/// Stack effect: ( path -- is-dir )
///
/// Returns 1 if path is a directory, 0 otherwise.
///
/// # Safety
/// Stack must have a String (path) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_is_dir(stack: Stack) -> Stack {
    unsafe {
        let (stack, path_val) = pop(stack);
        let path = match path_val {
            Value::String(s) => s,
            _ => panic!(
                "path-is-dir: expected String (path) on stack, got {:?}",
                path_val
            ),
        };

        let is_dir = std::path::Path::new(path.as_str()).is_dir();
        push(stack, Value::Bool(is_dir))
    }
}

/// Join two path components
///
/// Stack effect: ( base component -- joined )
///
/// Joins the base path with the component using the platform's path separator.
///
/// # Safety
/// Stack must have two Strings on top (base, then component)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_join(stack: Stack) -> Stack {
    unsafe {
        let (stack, component_val) = pop(stack);
        let (stack, base_val) = pop(stack);

        let base = match base_val {
            Value::String(s) => s,
            _ => panic!(
                "path-join: expected String (base) on stack, got {:?}",
                base_val
            ),
        };

        let component = match component_val {
            Value::String(s) => s,
            _ => panic!(
                "path-join: expected String (component) on stack, got {:?}",
                component_val
            ),
        };

        let joined = std::path::Path::new(base.as_str())
            .join(component.as_str())
            .to_string_lossy()
            .into_owned();

        push(stack, Value::String(global_string(joined)))
    }
}

/// Get the parent directory of a path
///
/// Stack effect: ( path -- parent success )
///
/// Returns the parent directory and true on success, "" and false if no parent.
///
/// # Safety
/// Stack must have a String (path) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_parent(stack: Stack) -> Stack {
    unsafe {
        let (stack, path_val) = pop(stack);
        let path = match path_val {
            Value::String(s) => s,
            _ => panic!(
                "path-parent: expected String (path) on stack, got {:?}",
                path_val
            ),
        };

        match std::path::Path::new(path.as_str()).parent() {
            Some(parent) => {
                let parent_str = parent.to_string_lossy().into_owned();
                let stack = push(stack, Value::String(global_string(parent_str)));
                push(stack, Value::Bool(true)) // success
            }
            None => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Bool(false)) // no parent
            }
        }
    }
}

/// Get the filename component of a path
///
/// Stack effect: ( path -- filename success )
///
/// Returns the filename and true on success, "" and false if no filename.
///
/// # Safety
/// Stack must have a String (path) on top
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_path_filename(stack: Stack) -> Stack {
    unsafe {
        let (stack, path_val) = pop(stack);
        let path = match path_val {
            Value::String(s) => s,
            _ => panic!(
                "path-filename: expected String (path) on stack, got {:?}",
                path_val
            ),
        };

        match std::path::Path::new(path.as_str()).file_name() {
            Some(filename) => {
                let filename_str = filename.to_string_lossy().into_owned();
                let stack = push(stack, Value::String(global_string(filename_str)));
                push(stack, Value::Bool(true)) // success
            }
            None => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Bool(false)) // no filename
            }
        }
    }
}

/// Valid exit code range for Unix compatibility (only low 8 bits are meaningful)
const EXIT_CODE_MIN: i64 = 0;
const EXIT_CODE_MAX: i64 = 255;

/// Exit the process with the given exit code
///
/// Stack effect: ( code -- )
///
/// Exit code must be in range 0-255 for Unix compatibility.
/// This function does not return.
///
/// # Safety
/// Stack must have an Int (exit code) on top.
///
/// Note: Returns `Stack` for LLVM ABI compatibility even though it never returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_exit(stack: Stack) -> Stack {
    unsafe {
        let (_stack, code_val) = pop(stack);
        let code = match code_val {
            Value::Int(n) => {
                if !(EXIT_CODE_MIN..=EXIT_CODE_MAX).contains(&n) {
                    panic!(
                        "os.exit: exit code must be in range {}-{}, got {}",
                        EXIT_CODE_MIN, EXIT_CODE_MAX, n
                    );
                }
                n as i32
            }
            _ => panic!(
                "os.exit: expected Int (exit code) on stack, got {:?}",
                code_val
            ),
        };

        std::process::exit(code);
    }
}

/// Get the operating system name
///
/// Stack effect: ( -- name )
///
/// Returns one of: "darwin", "linux", "windows", "freebsd", "openbsd", "netbsd",
/// or "unknown" for unrecognized platforms.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_os_name(stack: Stack) -> Stack {
    let name = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "freebsd") {
        "freebsd"
    } else if cfg!(target_os = "openbsd") {
        "openbsd"
    } else if cfg!(target_os = "netbsd") {
        "netbsd"
    } else {
        "unknown"
    };

    unsafe { push(stack, Value::String(global_string(name.to_owned()))) }
}

/// Get the CPU architecture
///
/// Stack effect: ( -- arch )
///
/// Returns one of: "x86_64", "aarch64", "arm", "x86", "riscv64",
/// or "unknown" for unrecognized architectures.
///
/// # Safety
/// Stack pointer must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn patch_seq_os_arch(stack: Stack) -> Stack {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "arm") {
        "arm"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "riscv64") {
        "riscv64"
    } else {
        "unknown"
    };

    unsafe { push(stack, Value::String(global_string(arch.to_owned()))) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::{alloc_test_stack, pop, push};
    use std::io::Write;
    use tempfile::{NamedTempFile, TempDir};

    // Helper to create a String value
    fn str_val(s: &str) -> Value {
        Value::String(global_string(s.to_string()))
    }

    // Helper to extract String from Value
    fn as_str(v: &Value) -> &str {
        match v {
            Value::String(s) => s.as_str(),
            _ => panic!("expected String, got {:?}", v),
        }
    }

    // Helper to extract Bool from Value
    fn as_bool(v: &Value) -> bool {
        match v {
            Value::Bool(b) => *b,
            _ => panic!("expected Bool, got {:?}", v),
        }
    }

    // ========================================================================
    // Environment Variable Tests
    // ========================================================================

    #[test]
    fn test_getenv_existing() {
        // PATH should exist on all platforms
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("PATH"));
            let stack = patch_seq_getenv(stack);

            let (stack, success) = pop(stack);
            let (_, value) = pop(stack);

            assert!(as_bool(&success), "PATH should exist");
            assert!(!as_str(&value).is_empty(), "PATH should not be empty");
        }
    }

    #[test]
    fn test_getenv_nonexistent() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("THIS_ENV_VAR_SHOULD_NOT_EXIST_12345"));
            let stack = patch_seq_getenv(stack);

            let (stack, success) = pop(stack);
            let (_, value) = pop(stack);

            assert!(!as_bool(&success), "nonexistent var should fail");
            assert!(as_str(&value).is_empty(), "value should be empty string");
        }
    }

    // ========================================================================
    // Home Directory Tests
    // ========================================================================

    #[test]
    fn test_home_dir() {
        // HOME is typically set on Unix, and we set it in most CI environments
        unsafe {
            let stack = alloc_test_stack();
            let stack = patch_seq_home_dir(stack);

            let (stack, success) = pop(stack);
            let (_, path) = pop(stack);

            // On most systems HOME exists
            if as_bool(&success) {
                assert!(!as_str(&path).is_empty(), "home path should not be empty");
            }
            // If it doesn't exist, that's also valid (just returns false)
        }
    }

    // ========================================================================
    // Current Directory Tests
    // ========================================================================

    #[test]
    fn test_current_dir() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = patch_seq_current_dir(stack);

            let (stack, success) = pop(stack);
            let (_, path) = pop(stack);

            assert!(as_bool(&success), "current_dir should succeed");
            assert!(!as_str(&path).is_empty(), "current dir should not be empty");

            // Verify it matches std::env::current_dir
            let expected = std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert_eq!(as_str(&path), expected);
        }
    }

    // ========================================================================
    // Path Exists Tests
    // ========================================================================

    #[test]
    fn test_path_exists_file() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_exists(stack);

            let (_, exists) = pop(stack);
            assert!(as_bool(&exists), "temp file should exist");
        }
    }

    #[test]
    fn test_path_exists_dir() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_exists(stack);

            let (_, exists) = pop(stack);
            assert!(as_bool(&exists), "temp dir should exist");
        }
    }

    #[test]
    fn test_path_exists_nonexistent() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/this/path/should/not/exist/12345"));
            let stack = patch_seq_path_exists(stack);

            let (_, exists) = pop(stack);
            assert!(!as_bool(&exists), "nonexistent path should not exist");
        }
    }

    // ========================================================================
    // Path Is File Tests
    // ========================================================================

    #[test]
    fn test_path_is_file_true() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_is_file(stack);

            let (_, is_file) = pop(stack);
            assert!(as_bool(&is_file), "temp file should be a file");
        }
    }

    #[test]
    fn test_path_is_file_false_for_dir() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_is_file(stack);

            let (_, is_file) = pop(stack);
            assert!(!as_bool(&is_file), "directory should not be a file");
        }
    }

    #[test]
    fn test_path_is_file_nonexistent() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/this/path/should/not/exist/12345"));
            let stack = patch_seq_path_is_file(stack);

            let (_, is_file) = pop(stack);
            assert!(!as_bool(&is_file), "nonexistent path should not be a file");
        }
    }

    // ========================================================================
    // Path Is Dir Tests
    // ========================================================================

    #[test]
    fn test_path_is_dir_true() {
        let tmp_dir = TempDir::new().unwrap();
        let path = tmp_dir.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_is_dir(stack);

            let (_, is_dir) = pop(stack);
            assert!(as_bool(&is_dir), "temp dir should be a directory");
        }
    }

    #[test]
    fn test_path_is_dir_false_for_file() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path().to_string_lossy().into_owned();

        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&path));
            let stack = patch_seq_path_is_dir(stack);

            let (_, is_dir) = pop(stack);
            assert!(!as_bool(&is_dir), "file should not be a directory");
        }
    }

    #[test]
    fn test_path_is_dir_nonexistent() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/this/path/should/not/exist/12345"));
            let stack = patch_seq_path_is_dir(stack);

            let (_, is_dir) = pop(stack);
            assert!(
                !as_bool(&is_dir),
                "nonexistent path should not be a directory"
            );
        }
    }

    // ========================================================================
    // Path Join Tests
    // ========================================================================

    #[test]
    fn test_path_join_simple() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user"));
            let stack = push(stack, str_val("documents"));
            let stack = patch_seq_path_join(stack);

            let (_, joined) = pop(stack);
            assert_eq!(as_str(&joined), "/home/user/documents");
        }
    }

    #[test]
    fn test_path_join_with_trailing_slash() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user/"));
            let stack = push(stack, str_val("documents"));
            let stack = patch_seq_path_join(stack);

            let (_, joined) = pop(stack);
            assert_eq!(as_str(&joined), "/home/user/documents");
        }
    }

    #[test]
    fn test_path_join_absolute_component() {
        // When joining with an absolute path, the absolute path replaces
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user"));
            let stack = push(stack, str_val("/etc/passwd"));
            let stack = patch_seq_path_join(stack);

            let (_, joined) = pop(stack);
            // Rust's Path::join replaces when component is absolute
            assert_eq!(as_str(&joined), "/etc/passwd");
        }
    }

    #[test]
    fn test_path_join_empty_component() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user"));
            let stack = push(stack, str_val(""));
            let stack = patch_seq_path_join(stack);

            let (_, joined) = pop(stack);
            // Joining with empty string adds trailing slash
            assert_eq!(as_str(&joined), "/home/user/");
        }
    }

    // ========================================================================
    // Path Parent Tests
    // ========================================================================

    #[test]
    fn test_path_parent_normal() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user/documents"));
            let stack = patch_seq_path_parent(stack);

            let (stack, success) = pop(stack);
            let (_, parent) = pop(stack);

            assert!(as_bool(&success), "should have parent");
            assert_eq!(as_str(&parent), "/home/user");
        }
    }

    #[test]
    fn test_path_parent_root() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/"));
            let stack = patch_seq_path_parent(stack);

            let (stack, success) = pop(stack);
            let (_, _parent) = pop(stack);

            // On Unix, root "/" has no parent (returns None from Path::parent)
            assert!(!as_bool(&success), "root has no parent");
        }
    }

    #[test]
    fn test_path_parent_single_component() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("filename"));
            let stack = patch_seq_path_parent(stack);

            let (stack, success) = pop(stack);
            let (_, parent) = pop(stack);

            // Single component has parent "" (empty)
            assert!(as_bool(&success), "single component has empty parent");
            assert_eq!(as_str(&parent), "");
        }
    }

    #[test]
    fn test_path_parent_empty() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(""));
            let stack = patch_seq_path_parent(stack);

            let (stack, success) = pop(stack);
            let (_, _parent) = pop(stack);

            // Empty path has no parent
            assert!(!as_bool(&success), "empty path has no parent");
        }
    }

    // ========================================================================
    // Path Filename Tests
    // ========================================================================

    #[test]
    fn test_path_filename_normal() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user/document.txt"));
            let stack = patch_seq_path_filename(stack);

            let (stack, success) = pop(stack);
            let (_, filename) = pop(stack);

            assert!(as_bool(&success), "should have filename");
            assert_eq!(as_str(&filename), "document.txt");
        }
    }

    #[test]
    fn test_path_filename_no_extension() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/home/user/document"));
            let stack = patch_seq_path_filename(stack);

            let (stack, success) = pop(stack);
            let (_, filename) = pop(stack);

            assert!(as_bool(&success), "should have filename");
            assert_eq!(as_str(&filename), "document");
        }
    }

    #[test]
    fn test_path_filename_root() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("/"));
            let stack = patch_seq_path_filename(stack);

            let (stack, success) = pop(stack);
            let (_, _filename) = pop(stack);

            // Root has no filename
            assert!(!as_bool(&success), "root has no filename");
        }
    }

    #[test]
    fn test_path_filename_empty() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(""));
            let stack = patch_seq_path_filename(stack);

            let (stack, success) = pop(stack);
            let (_, _filename) = pop(stack);

            // Empty path has no filename
            assert!(!as_bool(&success), "empty path has no filename");
        }
    }

    #[test]
    fn test_path_filename_only_filename() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = push(stack, str_val("document.txt"));
            let stack = patch_seq_path_filename(stack);

            let (stack, success) = pop(stack);
            let (_, filename) = pop(stack);

            assert!(as_bool(&success), "should have filename");
            assert_eq!(as_str(&filename), "document.txt");
        }
    }

    // ========================================================================
    // OS Name Tests
    // ========================================================================

    #[test]
    fn test_os_name() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = patch_seq_os_name(stack);

            let (_, name) = pop(stack);
            let name_str = as_str(&name);

            // Should be one of the known values
            let valid_names = [
                "darwin", "linux", "windows", "freebsd", "openbsd", "netbsd", "unknown",
            ];
            assert!(
                valid_names.contains(&name_str),
                "OS name '{}' should be one of {:?}",
                name_str,
                valid_names
            );

            // On the current platform, verify it matches expectations
            #[cfg(target_os = "macos")]
            assert_eq!(name_str, "darwin");
            #[cfg(target_os = "linux")]
            assert_eq!(name_str, "linux");
            #[cfg(target_os = "windows")]
            assert_eq!(name_str, "windows");
        }
    }

    // ========================================================================
    // OS Arch Tests
    // ========================================================================

    #[test]
    fn test_os_arch() {
        unsafe {
            let stack = alloc_test_stack();
            let stack = patch_seq_os_arch(stack);

            let (_, arch) = pop(stack);
            let arch_str = as_str(&arch);

            // Should be one of the known values
            let valid_archs = ["x86_64", "aarch64", "arm", "x86", "riscv64", "unknown"];
            assert!(
                valid_archs.contains(&arch_str),
                "arch '{}' should be one of {:?}",
                arch_str,
                valid_archs
            );

            // On the current platform, verify it matches expectations
            #[cfg(target_arch = "x86_64")]
            assert_eq!(arch_str, "x86_64");
            #[cfg(target_arch = "aarch64")]
            assert_eq!(arch_str, "aarch64");
        }
    }

    // ========================================================================
    // Integration Tests - Real Filesystem Operations
    // ========================================================================

    #[test]
    fn test_path_operations_integration() {
        // Create a temp directory with a file
        let tmp_dir = TempDir::new().unwrap();
        let dir_path = tmp_dir.path().to_string_lossy().into_owned();

        // Create a file in the directory
        let file_path = tmp_dir.path().join("test.txt");
        let mut file = std::fs::File::create(&file_path).unwrap();
        file.write_all(b"test content").unwrap();
        drop(file);

        let file_path_str = file_path.to_string_lossy().into_owned();

        unsafe {
            // Test: path_exists on dir
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&dir_path));
            let stack = patch_seq_path_exists(stack);
            let (_, exists) = pop(stack);
            assert!(as_bool(&exists));

            // Test: path_is_dir on dir
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&dir_path));
            let stack = patch_seq_path_is_dir(stack);
            let (_, is_dir) = pop(stack);
            assert!(as_bool(&is_dir));

            // Test: path_exists on file
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&file_path_str));
            let stack = patch_seq_path_exists(stack);
            let (_, exists) = pop(stack);
            assert!(as_bool(&exists));

            // Test: path_is_file on file
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&file_path_str));
            let stack = patch_seq_path_is_file(stack);
            let (_, is_file) = pop(stack);
            assert!(as_bool(&is_file));

            // Test: path_filename
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&file_path_str));
            let stack = patch_seq_path_filename(stack);
            let (stack, success) = pop(stack);
            let (_, filename) = pop(stack);
            assert!(as_bool(&success));
            assert_eq!(as_str(&filename), "test.txt");

            // Test: path_parent gets back to directory
            let stack = alloc_test_stack();
            let stack = push(stack, str_val(&file_path_str));
            let stack = patch_seq_path_parent(stack);
            let (stack, success) = pop(stack);
            let (_, parent) = pop(stack);
            assert!(as_bool(&success));
            assert_eq!(as_str(&parent), dir_path);
        }
    }

    // Note: patch_seq_exit is not tested because it terminates the process
}
