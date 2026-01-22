//! Script mode for running .seq files directly
//!
//! Enables `.seq` files to run directly with shebangs:
//! ```bash
//! #!/usr/bin/env seqc
//! : main ( -- Int ) "Hello from script!" io.write-line 0 ;
//! ```
//!
//! Running `seqc script.seq arg1 arg2` or `./script.seq` (with shebang) will:
//! 1. Detect script mode (first arg is a `.seq` file)
//! 2. Compile with `-O0` for fast compilation
//! 3. Cache compiled binary (keyed by source + include hashes)
//! 4. Run cached binary or compile -> cache -> run
//! 5. Pass remaining argv to the script

use crate::CompilerConfig;
use crate::config::OptimizationLevel;
use crate::parser::Parser;
use crate::resolver::{Resolver, find_stdlib};
use crate::stdlib_embed;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Get cache directory: $XDG_CACHE_HOME/seq/ or ~/.cache/seq/
pub fn get_cache_dir() -> Option<PathBuf> {
    // Try XDG_CACHE_HOME first
    if let Ok(xdg_cache) = std::env::var("XDG_CACHE_HOME") {
        let path = PathBuf::from(xdg_cache);
        if path.is_absolute() {
            return Some(path.join("seq"));
        }
    }

    // Fall back to ~/.cache/seq/
    if let Ok(home) = std::env::var("HOME") {
        return Some(PathBuf::from(home).join(".cache").join("seq"));
    }

    None
}

/// Compute cache key from source + all transitive includes
///
/// Algorithm:
/// 1. Hash main source file content
/// 2. Sort and hash all filesystem includes
/// 3. Sort and hash all embedded stdlib modules
/// 4. Combine into final SHA-256 hex string
pub fn compute_cache_key(
    source_path: &Path,
    source_files: &[PathBuf],
    embedded_modules: &[String],
) -> Result<String, String> {
    let mut hasher = Sha256::new();

    // Hash the main source file content
    let main_content =
        fs::read(source_path).map_err(|e| format!("Failed to read source file: {}", e))?;
    hasher.update(&main_content);

    // Sort and hash all filesystem includes
    let mut sorted_files: Vec<_> = source_files.iter().collect();
    sorted_files.sort();
    for file in sorted_files {
        if file != source_path {
            // Don't double-hash the main file
            let content = fs::read(file)
                .map_err(|e| format!("Failed to read included file '{}': {}", file.display(), e))?;
            hasher.update(&content);
        }
    }

    // Sort and hash all embedded stdlib modules
    let mut sorted_modules: Vec<_> = embedded_modules.iter().collect();
    sorted_modules.sort();
    for module_name in sorted_modules {
        if let Some(content) = stdlib_embed::get_stdlib(module_name) {
            hasher.update(content.as_bytes());
        }
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Strip shebang line from source if present
///
/// Replaces the first line with a comment if it starts with `#!`
/// so that line numbers in error messages remain correct.
fn strip_shebang(source: &str) -> std::borrow::Cow<'_, str> {
    if source.starts_with("#!") {
        // Replace shebang with comment of same length to preserve line numbers
        if let Some(newline_pos) = source.find('\n') {
            let mut result = String::with_capacity(source.len());
            result.push('#');
            result.push_str(&" ".repeat(newline_pos - 1));
            result.push_str(&source[newline_pos..]);
            std::borrow::Cow::Owned(result)
        } else {
            // Single line file with just shebang
            std::borrow::Cow::Borrowed("#")
        }
    } else {
        std::borrow::Cow::Borrowed(source)
    }
}

/// Prepare a script for execution: parse, resolve includes, and compile if needed.
/// Returns the path to the cached binary.
///
/// # Symlink Behavior
///
/// The source path is canonicalized, which resolves symlinks to their target.
/// This means the same script accessed via different symlinks will share one
/// cache entry (based on the resolved path's content hash).
fn prepare_script(source_path: &Path) -> Result<PathBuf, String> {
    // Canonicalize the source path
    let source_path = source_path.canonicalize().map_err(|e| {
        format!(
            "Failed to find source file '{}': {}",
            source_path.display(),
            e
        )
    })?;

    // Get cache directory
    let cache_dir =
        get_cache_dir().ok_or_else(|| "Could not determine cache directory".to_string())?;

    // Parse the source to find includes (strip shebang if present)
    let source_raw = fs::read_to_string(&source_path)
        .map_err(|e| format!("Failed to read source file: {}", e))?;
    let source = strip_shebang(&source_raw);

    let mut parser = Parser::new(&source);
    let program = parser.parse()?;

    // Resolve includes to get list of dependencies
    let (source_files, embedded_modules) = if !program.includes.is_empty() {
        let stdlib_path = find_stdlib();
        let mut resolver = Resolver::new(stdlib_path);
        let result = resolver.resolve(&source_path, program)?;
        (result.source_files, result.embedded_modules)
    } else {
        (vec![source_path.clone()], Vec::new())
    };

    // Compute cache key (use raw source for consistent hashing)
    let cache_key = compute_cache_key(&source_path, &source_files, &embedded_modules)?;
    let cached_binary = cache_dir.join(&cache_key);

    // Check if cached binary exists
    if cached_binary.exists() {
        return Ok(cached_binary);
    }

    // Create cache directory if needed
    fs::create_dir_all(&cache_dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;

    // Use process ID in temp file name to avoid collisions between parallel compilations
    let pid = std::process::id();
    let temp_binary = cache_dir.join(format!("{}.{}.tmp", cache_key, pid));
    let temp_source = cache_dir.join(format!("{}.{}.seq", cache_key, pid));

    // Write preprocessed source to a temp file for compilation
    fs::write(&temp_source, source.as_ref())
        .map_err(|e| format!("Failed to write temp source: {}", e))?;

    // Compile with -O0 for fast compilation
    let config = CompilerConfig::new().with_optimization_level(OptimizationLevel::O0);

    let compile_result =
        crate::compile_file_with_config(&temp_source, &temp_binary, false, &config);

    // Clean up temp source file
    fs::remove_file(&temp_source).ok();

    // Handle compilation result
    if let Err(e) = compile_result {
        // Clean up temp binary on compilation failure
        fs::remove_file(&temp_binary).ok();
        return Err(e);
    }

    // Try to atomically move to final location
    // If another process already created the cached binary, that's fine - use it
    if fs::rename(&temp_binary, &cached_binary).is_err() {
        // Rename failed - check if cached binary now exists (race with another process)
        if cached_binary.exists() {
            // Another process won the race, clean up our temp and use theirs
            fs::remove_file(&temp_binary).ok();
        } else {
            // Rename failed for another reason, clean up and report error
            fs::remove_file(&temp_binary).ok();
            return Err("Failed to cache compiled binary".to_string());
        }
    }

    Ok(cached_binary)
}

/// Run a .seq script (compile if needed, then exec)
///
/// This function does not return on success - it execs the compiled binary.
/// On error, it returns an Err with the error message.
#[cfg(unix)]
pub fn run_script(
    source_path: &Path,
    args: &[OsString],
) -> Result<std::convert::Infallible, String> {
    use std::os::unix::process::CommandExt;

    let cached_binary = prepare_script(source_path)?;

    // Exec the cached binary with script args
    let err = std::process::Command::new(&cached_binary).args(args).exec();

    // If we get here, exec failed
    Err(format!("Failed to execute script: {}", err))
}

/// Run a .seq script on non-Unix platforms (spawn + wait instead of exec)
#[cfg(not(unix))]
pub fn run_script(
    source_path: &Path,
    args: &[OsString],
) -> Result<std::convert::Infallible, String> {
    let cached_binary = prepare_script(source_path)?;

    // Spawn the cached binary and wait for it
    let status = std::process::Command::new(&cached_binary)
        .args(args)
        .status()
        .map_err(|e| format!("Failed to execute script: {}", e))?;

    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_get_cache_dir_with_xdg() {
        // Save original env vars
        let orig_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let orig_home = std::env::var("HOME").ok();

        // SAFETY: These tests must run serially (use cargo test -- --test-threads=1)
        // to avoid race conditions with other tests modifying environment variables.
        unsafe {
            // Test with XDG_CACHE_HOME set
            std::env::set_var("XDG_CACHE_HOME", "/tmp/test-xdg-cache");
        }
        let cache_dir = get_cache_dir();
        assert!(cache_dir.is_some());
        assert_eq!(cache_dir.unwrap(), PathBuf::from("/tmp/test-xdg-cache/seq"));

        // Restore original env vars
        // SAFETY: Restoring environment to original state
        unsafe {
            match orig_xdg {
                Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
                None => std::env::remove_var("XDG_CACHE_HOME"),
            }
            match orig_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    #[serial]
    fn test_get_cache_dir_fallback_to_home() {
        // Save original env vars
        let orig_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let orig_home = std::env::var("HOME").ok();

        // SAFETY: These tests must run serially (use cargo test -- --test-threads=1)
        // to avoid race conditions with other tests modifying environment variables.
        unsafe {
            // Clear XDG_CACHE_HOME, set HOME
            std::env::remove_var("XDG_CACHE_HOME");
            std::env::set_var("HOME", "/tmp/test-home");
        }
        let cache_dir = get_cache_dir();
        assert!(cache_dir.is_some());
        assert_eq!(
            cache_dir.unwrap(),
            PathBuf::from("/tmp/test-home/.cache/seq")
        );

        // Restore original env vars
        // SAFETY: Restoring environment to original state
        unsafe {
            match orig_xdg {
                Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
                None => std::env::remove_var("XDG_CACHE_HOME"),
            }
            match orig_home {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn test_compute_cache_key_deterministic() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let source = temp.path().join("test.seq");
        fs::write(&source, ": main ( -- Int ) 42 ;").unwrap();

        let key1 = compute_cache_key(&source, std::slice::from_ref(&source), &[]).unwrap();
        let key2 = compute_cache_key(&source, std::slice::from_ref(&source), &[]).unwrap();

        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 64); // SHA-256 hex is 64 chars
    }

    #[test]
    fn test_compute_cache_key_changes_with_content() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let source = temp.path().join("test.seq");

        fs::write(&source, ": main ( -- Int ) 42 ;").unwrap();
        let key1 = compute_cache_key(&source, std::slice::from_ref(&source), &[]).unwrap();

        fs::write(&source, ": main ( -- Int ) 43 ;").unwrap();
        let key2 = compute_cache_key(&source, std::slice::from_ref(&source), &[]).unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_compute_cache_key_includes_embedded_modules() {
        use tempfile::tempdir;

        let temp = tempdir().unwrap();
        let source = temp.path().join("test.seq");
        fs::write(&source, ": main ( -- Int ) 42 ;").unwrap();

        let key1 = compute_cache_key(&source, std::slice::from_ref(&source), &[]).unwrap();
        let key2 = compute_cache_key(
            &source,
            std::slice::from_ref(&source),
            &["imath".to_string()],
        )
        .unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_strip_shebang_with_shebang() {
        let source = "#!/usr/bin/env seqc\n: main ( -- Int ) 42 ;";
        let stripped = strip_shebang(source);
        // Should start with # (comment) not #!
        assert!(stripped.starts_with('#'));
        assert!(!stripped.starts_with("#!"));
        // Should preserve the second line
        assert!(stripped.contains(": main ( -- Int ) 42 ;"));
        // Should preserve line count (same length before newline)
        assert_eq!(stripped.matches('\n').count(), source.matches('\n').count());
    }

    #[test]
    fn test_strip_shebang_without_shebang() {
        let source = ": main ( -- Int ) 42 ;";
        let stripped = strip_shebang(source);
        // Should be unchanged
        assert_eq!(stripped.as_ref(), source);
    }

    #[test]
    fn test_strip_shebang_with_comment() {
        let source = "# This is a comment\n: main ( -- Int ) 42 ;";
        let stripped = strip_shebang(source);
        // Should be unchanged (# is not #!)
        assert_eq!(stripped.as_ref(), source);
    }

    #[test]
    fn test_strip_shebang_only_shebang() {
        let source = "#!/usr/bin/env seqc";
        let stripped = strip_shebang(source);
        // Single line file with just shebang becomes just #
        assert_eq!(stripped.as_ref(), "#");
    }
}
