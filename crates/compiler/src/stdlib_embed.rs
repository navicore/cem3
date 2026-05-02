//! Embedded Standard Library
//!
//! Contains stdlib modules embedded at compile time.
//! This makes seqc fully self-contained - no need for external stdlib files.

use std::collections::HashMap;
use std::sync::LazyLock;

/// Embedded stdlib files (name -> content)
static STDLIB: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("imath", include_str!("../stdlib/imath.seq"));
    m.insert("fmath", include_str!("../stdlib/fmath.seq"));
    m.insert("json", include_str!("../stdlib/json.seq"));
    m.insert("yaml", include_str!("../stdlib/yaml.seq"));
    m.insert("http", include_str!("../stdlib/http.seq"));
    m.insert("stack-utils", include_str!("../stdlib/stack-utils.seq"));
    m.insert("map", include_str!("../stdlib/map.seq"));
    m.insert("list", include_str!("../stdlib/list.seq"));
    m.insert("son", include_str!("../stdlib/son.seq"));
    m.insert("signal", include_str!("../stdlib/signal.seq"));
    m.insert("zipper", include_str!("../stdlib/zipper.seq"));
    m.insert("loops", include_str!("../stdlib/loops.seq"));
    m.insert("control", include_str!("../stdlib/control.seq"));
    m
});

/// Get an embedded stdlib module by name
pub fn get_stdlib(name: &str) -> Option<&'static str> {
    STDLIB.get(name).copied()
}

/// Check if a stdlib module exists (embedded)
pub fn has_stdlib(name: &str) -> bool {
    STDLIB.contains_key(name)
}

/// Names of all embedded stdlib modules, sorted alphabetically.
pub fn stdlib_module_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = STDLIB.keys().copied().collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_imath_stdlib_exists() {
        assert!(has_stdlib("imath"));
        let content = get_stdlib("imath").unwrap();
        assert!(content.contains("abs"));
    }

    #[test]
    fn test_fmath_stdlib_exists() {
        assert!(has_stdlib("fmath"));
        let content = get_stdlib("fmath").unwrap();
        assert!(content.contains("f.abs"));
    }

    #[test]
    fn test_loops_stdlib_exists() {
        assert!(has_stdlib("loops"));
        let content = get_stdlib("loops").unwrap();
        assert!(content.contains("times"));
        assert!(content.contains("each-integer"));
        assert!(content.contains("integer-fold"));
    }

    #[test]
    fn test_control_stdlib_exists() {
        assert!(has_stdlib("control"));
        let content = get_stdlib("control").unwrap();
        assert!(content.contains("when"));
        assert!(content.contains("unless"));
    }

    #[test]
    fn test_nonexistent_stdlib() {
        assert!(!has_stdlib("nonexistent"));
        assert!(get_stdlib("nonexistent").is_none());
    }
}
