//! Include resolution for LSP completions
//!
//! Parses included files and extracts word definitions for completion.
//! Uses the embedded stdlib from the compiler - no filesystem search needed.

use seqc::Effect;
use seqc::ast::Include;
use seqc::parser::Parser;
use seqc::stdlib_embed;
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

/// Resolve includes and extract words from included files.
/// Uses embedded stdlib for std: includes, filesystem for relative includes.
pub fn resolve_includes(includes: &[Include], doc_path: Option<&Path>) -> Vec<IncludedWord> {
    let mut words = Vec::new();
    let mut visited = HashSet::new();

    // Convert file path to directory for relative include resolution
    let doc_dir = doc_path.and_then(|p| p.parent());

    for include in includes {
        resolve_include_recursive(include, doc_dir, &mut words, &mut visited, 0);
    }

    words
}

/// Recursively resolve an include, with cycle detection and depth limit
fn resolve_include_recursive(
    include: &Include,
    doc_dir: Option<&Path>,
    words: &mut Vec<IncludedWord>,
    visited: &mut HashSet<String>,
    depth: usize,
) {
    // Depth limit to prevent runaway recursion
    if depth > 10 {
        warn!("Include depth limit reached");
        return;
    }

    match include {
        Include::Std(name) => {
            // Use embedded stdlib
            let key = format!("std:{}", name);
            if visited.contains(&key) {
                return;
            }
            visited.insert(key.clone());

            let content = match stdlib_embed::get_stdlib(name) {
                Some(c) => c,
                None => {
                    debug!("Stdlib module not found: {}", name);
                    return;
                }
            };

            let mut parser = Parser::new(content);
            let program = match parser.parse() {
                Ok(p) => p,
                Err(e) => {
                    debug!("Could not parse stdlib {}: {}", name, e);
                    return;
                }
            };

            // Extract words from stdlib module
            for word in &program.words {
                let start_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);
                words.push(IncludedWord {
                    name: word.name.clone(),
                    effect: word.effect.clone(),
                    source: format!("std:{}", name),
                    file_path: None, // Embedded, no file path
                    start_line,
                });
            }

            // Recursively resolve nested includes (stdlib can include other stdlib)
            for nested_include in &program.includes {
                resolve_include_recursive(nested_include, None, words, visited, depth + 1);
            }
        }
        Include::Relative(name) => {
            // Filesystem-based resolution for relative includes
            let dir = match doc_dir {
                Some(d) => d,
                None => {
                    debug!("No document directory for relative include: {}", name);
                    return;
                }
            };

            let path = dir.join(format!("{}.seq", name));
            let canonical = match path.canonicalize() {
                Ok(p) => p,
                Err(_) => {
                    debug!("Could not resolve relative include: {}", name);
                    return;
                }
            };

            let key = canonical.to_string_lossy().to_string();
            if visited.contains(&key) {
                return;
            }
            visited.insert(key);

            let content = match std::fs::read_to_string(&canonical) {
                Ok(c) => c,
                Err(e) => {
                    debug!("Could not read {}: {}", canonical.display(), e);
                    return;
                }
            };

            let mut parser = Parser::new(&content);
            let program = match parser.parse() {
                Ok(p) => p,
                Err(e) => {
                    debug!("Could not parse {}: {}", canonical.display(), e);
                    return;
                }
            };

            // Extract words from this file
            for word in &program.words {
                let start_line = word.source.as_ref().map(|s| s.start_line).unwrap_or(0);
                words.push(IncludedWord {
                    name: word.name.clone(),
                    effect: word.effect.clone(),
                    source: name.clone(),
                    file_path: Some(canonical.clone()),
                    start_line,
                });
            }

            // Recursively resolve nested includes
            let include_dir = canonical.parent();
            for nested_include in &program.includes {
                resolve_include_recursive(nested_include, include_dir, words, visited, depth + 1);
            }
        }
        Include::Ffi(name) => {
            // FFI includes don't contribute words directly - they're handled at compile time
            debug!("FFI include: {} (no words added to LSP)", name);
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
        // Parse a document that includes std:json
        let source = "include std:json\n";
        let (includes, _) = parse_document(source);
        assert_eq!(includes.len(), 1);

        // Resolve the includes using embedded stdlib
        let words = resolve_includes(&includes, None);

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
