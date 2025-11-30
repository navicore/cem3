//! Include resolution for LSP completions
//!
//! Parses included files and extracts word definitions for completion.

use seqc::Effect;
use seqc::ast::Include;
use seqc::parser::Parser;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tracing::{debug, warn};

/// A word extracted from an included module
#[derive(Debug, Clone)]
pub struct IncludedWord {
    pub name: String,
    pub effect: Option<Effect>,
    /// Source module name (e.g., "std:json" or "utils")
    pub source: String,
    /// File path where the word is defined
    pub file_path: Option<PathBuf>,
    /// Line number where the word is defined (0-indexed)
    pub start_line: usize,
}

/// A word defined in the current document
#[derive(Debug, Clone)]
pub struct LocalWord {
    pub name: String,
    pub effect: Option<Effect>,
    /// Line number where the word is defined (0-indexed)
    pub start_line: usize,
    /// Line number where the word ends (0-indexed)
    pub end_line: usize,
}

/// Find the stdlib path by checking common locations
pub fn find_stdlib_path() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(path) = std::env::var("SEQ_STDLIB_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. Relative to current executable
    if let Ok(exe) = std::env::current_exe() {
        // Try ../stdlib (for installed binaries)
        if let Some(parent) = exe.parent() {
            let stdlib = parent.join("../stdlib");
            if stdlib.exists() {
                return Some(stdlib);
            }
            // Try ../../stdlib (for target/release/seq-lsp)
            let stdlib = parent.join("../../stdlib");
            if stdlib.exists() {
                return Some(stdlib);
            }
        }
    }

    // 3. Common install locations
    if let Ok(home) = std::env::var("HOME") {
        let paths = [
            format!("{}/.local/share/seq/stdlib", home),
            format!("{}/seq/stdlib", home),
        ];
        for path in paths {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(p);
            }
        }
    }

    // 4. Development location - check if we're in the repo
    let dev_paths = ["./stdlib", "../stdlib", "../../stdlib"];
    for path in dev_paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p.canonicalize().unwrap_or(p));
        }
    }

    None
}

/// Extract include statements and local words from source code
pub fn parse_document(source: &str) -> (Vec<Include>, Vec<LocalWord>) {
    let mut parser = Parser::new(source);
    match parser.parse() {
        Ok(program) => {
            // Extract local words with source locations from the parser
            let local_words = program
                .words
                .iter()
                .map(|w| {
                    let (start_line, end_line) = w
                        .source
                        .as_ref()
                        .map(|s| (s.start_line, s.end_line))
                        .unwrap_or((0, 0));

                    LocalWord {
                        name: w.name.clone(),
                        effect: w.effect.clone(),
                        start_line,
                        end_line,
                    }
                })
                .collect();
            (program.includes, local_words)
        }
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// Resolve includes and extract words from included files
pub fn resolve_includes(
    includes: &[Include],
    doc_path: Option<&Path>,
    stdlib_path: Option<&Path>,
) -> Vec<IncludedWord> {
    let mut words = Vec::new();
    let mut visited = HashSet::new();

    for include in includes {
        resolve_include_recursive(include, doc_path, stdlib_path, &mut words, &mut visited, 0);
    }

    words
}

/// Recursively resolve an include, with cycle detection and depth limit
fn resolve_include_recursive(
    include: &Include,
    doc_path: Option<&Path>,
    stdlib_path: Option<&Path>,
    words: &mut Vec<IncludedWord>,
    visited: &mut HashSet<PathBuf>,
    depth: usize,
) {
    // Depth limit to prevent runaway recursion
    if depth > 10 {
        warn!("Include depth limit reached");
        return;
    }

    let (path, source_name) = match resolve_include_path(include, doc_path, stdlib_path) {
        Some(result) => result,
        None => {
            debug!("Could not resolve include: {:?}", include);
            return;
        }
    };

    // Canonicalize for cycle detection
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            debug!("Could not canonicalize: {:?}", path);
            return;
        }
    };

    // Skip if already visited
    if visited.contains(&canonical) {
        return;
    }
    visited.insert(canonical.clone());

    // Read and parse the file
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            debug!("Could not read {}: {}", path.display(), e);
            return;
        }
    };

    let mut parser = Parser::new(&content);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => {
            debug!("Could not parse {}: {}", path.display(), e);
            return;
        }
    };

    // Extract words from this file
    for word in &program.words {
        let start_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);
        words.push(IncludedWord {
            name: word.name.clone(),
            effect: word.effect.clone(),
            source: source_name.clone(),
            file_path: Some(canonical.clone()),
            start_line,
        });
    }

    // Recursively resolve nested includes
    let include_dir = path.parent();
    for nested_include in &program.includes {
        resolve_include_recursive(
            nested_include,
            include_dir,
            stdlib_path,
            words,
            visited,
            depth + 1,
        );
    }
}

/// Resolve an include to a file path and source name
fn resolve_include_path(
    include: &Include,
    doc_dir: Option<&Path>,
    stdlib_path: Option<&Path>,
) -> Option<(PathBuf, String)> {
    match include {
        Include::Std(name) => {
            let stdlib = stdlib_path?;
            let path = stdlib.join(format!("{}.seq", name));
            if path.exists() {
                Some((path, format!("std:{}", name)))
            } else {
                None
            }
        }
        Include::Relative(name) => {
            let dir = doc_dir?;
            let path = dir.join(format!("{}.seq", name));
            if path.exists() {
                Some((path, name.clone()))
            } else {
                None
            }
        }
    }
}

/// Convert a file:// URI to a PathBuf
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    if let Some(path_str) = uri.strip_prefix("file://") {
        // On Windows, URIs look like file:///C:/path
        // On Unix, file:///path
        #[cfg(windows)]
        let path_str = path_str.trim_start_matches('/');

        // URL decode the path
        let decoded = percent_decode(path_str);
        Some(PathBuf::from(decoded))
    } else {
        None
    }
}

/// Percent decoding for file paths with proper UTF-8 handling.
///
/// URIs encode multi-byte UTF-8 characters as multiple %XX sequences
/// (e.g., `é` becomes `%C3%A9`). We collect all bytes and decode as UTF-8.
fn percent_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2
                && let Ok(byte) = u8::from_str_radix(&hex, 16)
            {
                bytes.push(byte);
                continue;
            }
            // Invalid escape sequence, keep as-is
            bytes.push(b'%');
            bytes.extend(hex.as_bytes());
        } else if c.is_ascii() {
            bytes.push(c as u8);
        } else {
            // Non-ASCII char not percent-encoded, add its UTF-8 bytes
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            bytes.extend(encoded.as_bytes());
        }
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_to_path_unix() {
        let uri = "file:///Users/test/code/example.seq";
        let path = uri_to_path(uri).unwrap();
        assert_eq!(path, PathBuf::from("/Users/test/code/example.seq"));
    }

    #[test]
    fn test_uri_to_path_with_spaces() {
        let uri = "file:///Users/test/my%20code/example.seq";
        let path = uri_to_path(uri).unwrap();
        assert_eq!(path, PathBuf::from("/Users/test/my code/example.seq"));
    }

    #[test]
    fn test_uri_to_path_with_utf8() {
        // é is encoded as %C3%A9 in UTF-8
        let uri = "file:///Users/test/caf%C3%A9/example.seq";
        let path = uri_to_path(uri).unwrap();
        assert_eq!(path, PathBuf::from("/Users/test/café/example.seq"));
    }

    #[test]
    fn test_parse_document_with_includes() {
        let source = r#"
include std:json
include "utils"

: main ( -- )
  "hello" write_line
;
"#;
        let (includes, words) = parse_document(source);
        assert_eq!(includes.len(), 2);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].name, "main");
    }

    #[test]
    fn test_parse_document_with_effect() {
        let source = r#"
: double ( Int -- Int )
  dup +
;
"#;
        let (_, words) = parse_document(source);
        assert_eq!(words.len(), 1);
        assert!(words[0].effect.is_some());
    }

    #[test]
    fn test_resolve_stdlib_json() {
        // Find stdlib path
        let stdlib_path = find_stdlib_path();
        assert!(stdlib_path.is_some(), "Could not find stdlib path");
        let stdlib_path = stdlib_path.unwrap();

        // Parse a document that includes std:json
        let source = "include std:json\n";
        let (includes, _) = parse_document(source);
        assert_eq!(includes.len(), 1);

        // Resolve the includes
        let words = resolve_includes(&includes, None, Some(&stdlib_path));

        // Check that json-serialize is in the resolved words
        let names: Vec<&str> = words.iter().map(|w| w.name.as_str()).collect();
        assert!(
            names.contains(&"json-serialize"),
            "Expected json-serialize in {:?}",
            names
        );
        assert!(
            names.contains(&"json-parse"),
            "Expected json-parse in {:?}",
            names
        );
    }
}
