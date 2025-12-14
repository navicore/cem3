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
                push(stack, Value::Int(1)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // failure
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
            return push(stack, Value::Int(1));
        }

        // On Windows, try USERPROFILE
        #[cfg(windows)]
        if let Ok(home) = std::env::var("USERPROFILE") {
            let stack = push(stack, Value::String(global_string(home)));
            return push(stack, Value::Int(1));
        }

        // Fallback: return empty string with failure flag
        let stack = push(stack, Value::String(global_string(String::new())));
        push(stack, Value::Int(0))
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
                push(stack, Value::Int(1)) // success
            }
            Err(_) => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // failure
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
        push(stack, Value::Int(if exists { 1 } else { 0 }))
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
        push(stack, Value::Int(if is_file { 1 } else { 0 }))
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
        push(stack, Value::Int(if is_dir { 1 } else { 0 }))
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
/// Returns the parent directory and 1 on success, "" and 0 if no parent.
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
                push(stack, Value::Int(1)) // success
            }
            None => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // no parent
            }
        }
    }
}

/// Get the filename component of a path
///
/// Stack effect: ( path -- filename success )
///
/// Returns the filename and 1 on success, "" and 0 if no filename.
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
                push(stack, Value::Int(1)) // success
            }
            None => {
                let stack = push(stack, Value::String(global_string(String::new())));
                push(stack, Value::Int(0)) // no filename
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
