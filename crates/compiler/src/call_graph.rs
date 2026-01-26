//! Call graph analysis for detecting mutual recursion
//!
//! This module builds a call graph from a Seq program and detects
//! strongly connected components (SCCs) to identify mutual recursion cycles.
//!
//! # Usage
//!
//! ```ignore
//! let call_graph = CallGraph::build(&program);
//! let cycles = call_graph.recursive_cycles();
//! ```
//!
//! # Primary Use Cases
//!
//! 1. **Type checker divergence detection**: The type checker uses the call graph
//!    to identify mutually recursive tail calls, enabling correct type inference
//!    for patterns like even/odd that would otherwise require branch unification.
//!
//! 2. **Future optimizations**: The call graph infrastructure can support dead code
//!    detection, inlining decisions, and diagnostic tools.
//!
//! # Implementation Details
//!
//! - **Algorithm**: Tarjan's SCC algorithm, O(V + E) time complexity
//! - **Builtins**: Calls to builtins/external words are excluded from the graph
//!   (they don't affect recursion detection since they always return)
//! - **Quotations**: Calls within quotations are included in the analysis
//! - **Match arms**: Calls within match arms are included in the analysis
//!
//! # Note on Tail Call Optimization
//!
//! The existing codegen already emits `musttail` for all tail calls to user-defined
//! words (see `codegen/statements.rs`). This means mutual TCO works automatically
//! without needing explicit call graph checks in codegen. The call graph is primarily
//! used for type checking, not for enabling TCO.

use crate::ast::{Program, Statement};
use std::collections::{HashMap, HashSet};

/// A call graph representing which words call which other words.
#[derive(Debug, Clone)]
pub struct CallGraph {
    /// Map from word name to the set of words it calls
    edges: HashMap<String, HashSet<String>>,
    /// All word names in the program
    words: HashSet<String>,
    /// Strongly connected components with more than one member (mutual recursion)
    /// or single members that call themselves (direct recursion)
    recursive_sccs: Vec<HashSet<String>>,
}

impl CallGraph {
    /// Build a call graph from a program.
    ///
    /// This extracts all word-to-word call relationships, including calls
    /// within quotations, if branches, and match arms.
    pub fn build(program: &Program) -> Self {
        let mut edges: HashMap<String, HashSet<String>> = HashMap::new();
        let words: HashSet<String> = program.words.iter().map(|w| w.name.clone()).collect();

        for word in &program.words {
            let callees = extract_calls(&word.body, &words);
            edges.insert(word.name.clone(), callees);
        }

        let mut graph = CallGraph {
            edges,
            words,
            recursive_sccs: Vec::new(),
        };

        // Compute SCCs and identify recursive cycles
        graph.recursive_sccs = graph.find_sccs();

        graph
    }

    /// Check if a word is part of any recursive cycle (direct or mutual).
    pub fn is_recursive(&self, word: &str) -> bool {
        self.recursive_sccs.iter().any(|scc| scc.contains(word))
    }

    /// Check if two words are in the same recursive cycle (mutually recursive).
    pub fn are_mutually_recursive(&self, word1: &str, word2: &str) -> bool {
        self.recursive_sccs
            .iter()
            .any(|scc| scc.contains(word1) && scc.contains(word2))
    }

    /// Get all recursive cycles (SCCs with recursion).
    pub fn recursive_cycles(&self) -> &[HashSet<String>] {
        &self.recursive_sccs
    }

    /// Get the words that a given word calls.
    pub fn callees(&self, word: &str) -> Option<&HashSet<String>> {
        self.edges.get(word)
    }

    /// Find strongly connected components using Tarjan's algorithm.
    ///
    /// Returns only SCCs that represent recursion:
    /// - Multi-word SCCs (mutual recursion)
    /// - Single-word SCCs where the word calls itself (direct recursion)
    fn find_sccs(&self) -> Vec<HashSet<String>> {
        let mut index_counter = 0;
        let mut stack: Vec<String> = Vec::new();
        let mut on_stack: HashSet<String> = HashSet::new();
        let mut indices: HashMap<String, usize> = HashMap::new();
        let mut lowlinks: HashMap<String, usize> = HashMap::new();
        let mut sccs: Vec<HashSet<String>> = Vec::new();

        for word in &self.words {
            if !indices.contains_key(word) {
                self.tarjan_visit(
                    word,
                    &mut index_counter,
                    &mut stack,
                    &mut on_stack,
                    &mut indices,
                    &mut lowlinks,
                    &mut sccs,
                );
            }
        }

        // Filter to only recursive SCCs
        sccs.into_iter()
            .filter(|scc| {
                if scc.len() > 1 {
                    // Multi-word SCC = mutual recursion
                    true
                } else if scc.len() == 1 {
                    // Single-word SCC: check if it calls itself
                    let word = scc.iter().next().unwrap();
                    self.edges
                        .get(word)
                        .map(|callees| callees.contains(word))
                        .unwrap_or(false)
                } else {
                    false
                }
            })
            .collect()
    }

    /// Tarjan's algorithm recursive visit.
    #[allow(clippy::too_many_arguments)]
    fn tarjan_visit(
        &self,
        word: &str,
        index_counter: &mut usize,
        stack: &mut Vec<String>,
        on_stack: &mut HashSet<String>,
        indices: &mut HashMap<String, usize>,
        lowlinks: &mut HashMap<String, usize>,
        sccs: &mut Vec<HashSet<String>>,
    ) {
        let index = *index_counter;
        *index_counter += 1;
        indices.insert(word.to_string(), index);
        lowlinks.insert(word.to_string(), index);
        stack.push(word.to_string());
        on_stack.insert(word.to_string());

        // Visit all callees
        if let Some(callees) = self.edges.get(word) {
            for callee in callees {
                if !self.words.contains(callee) {
                    // External word (builtin), skip
                    continue;
                }
                if !indices.contains_key(callee) {
                    // Not yet visited
                    self.tarjan_visit(
                        callee,
                        index_counter,
                        stack,
                        on_stack,
                        indices,
                        lowlinks,
                        sccs,
                    );
                    let callee_lowlink = *lowlinks.get(callee).unwrap();
                    let word_lowlink = lowlinks.get_mut(word).unwrap();
                    *word_lowlink = (*word_lowlink).min(callee_lowlink);
                } else if on_stack.contains(callee) {
                    // Callee is on stack, part of current SCC
                    let callee_index = *indices.get(callee).unwrap();
                    let word_lowlink = lowlinks.get_mut(word).unwrap();
                    *word_lowlink = (*word_lowlink).min(callee_index);
                }
            }
        }

        // If word is a root node, pop the SCC
        if lowlinks.get(word) == indices.get(word) {
            let mut scc = HashSet::new();
            loop {
                let w = stack.pop().unwrap();
                on_stack.remove(&w);
                scc.insert(w.clone());
                if w == word {
                    break;
                }
            }
            sccs.push(scc);
        }
    }
}

/// Extract all word calls from a list of statements.
///
/// This recursively descends into quotations, if branches, and match arms.
fn extract_calls(statements: &[Statement], known_words: &HashSet<String>) -> HashSet<String> {
    let mut calls = HashSet::new();

    for stmt in statements {
        extract_calls_from_statement(stmt, known_words, &mut calls);
    }

    calls
}

/// Extract word calls from a single statement.
fn extract_calls_from_statement(
    stmt: &Statement,
    known_words: &HashSet<String>,
    calls: &mut HashSet<String>,
) {
    match stmt {
        Statement::WordCall { name, .. } => {
            // Only track calls to user-defined words
            if known_words.contains(name) {
                calls.insert(name.clone());
            }
        }
        Statement::If {
            then_branch,
            else_branch,
        } => {
            for s in then_branch {
                extract_calls_from_statement(s, known_words, calls);
            }
            if let Some(else_stmts) = else_branch {
                for s in else_stmts {
                    extract_calls_from_statement(s, known_words, calls);
                }
            }
        }
        Statement::Quotation { body, .. } => {
            for s in body {
                extract_calls_from_statement(s, known_words, calls);
            }
        }
        Statement::Match { arms } => {
            for arm in arms {
                for s in &arm.body {
                    extract_calls_from_statement(s, known_words, calls);
                }
            }
        }
        // Literals don't contain calls
        Statement::IntLiteral(_)
        | Statement::FloatLiteral(_)
        | Statement::BoolLiteral(_)
        | Statement::StringLiteral(_)
        | Statement::Symbol(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::WordDef;

    fn make_word(name: &str, calls: Vec<&str>) -> WordDef {
        let body = calls
            .into_iter()
            .map(|c| Statement::WordCall {
                name: c.to_string(),
                span: None,
            })
            .collect();
        WordDef {
            name: name.to_string(),
            effect: None,
            body,
            source: None,
            allowed_lints: vec![],
        }
    }

    #[test]
    fn test_no_recursion() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                make_word("foo", vec!["bar"]),
                make_word("bar", vec![]),
                make_word("baz", vec!["foo"]),
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(!graph.is_recursive("foo"));
        assert!(!graph.is_recursive("bar"));
        assert!(!graph.is_recursive("baz"));
        assert!(graph.recursive_cycles().is_empty());
    }

    #[test]
    fn test_direct_recursion() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                make_word("countdown", vec!["countdown"]),
                make_word("helper", vec![]),
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(graph.is_recursive("countdown"));
        assert!(!graph.is_recursive("helper"));
        assert_eq!(graph.recursive_cycles().len(), 1);
    }

    #[test]
    fn test_mutual_recursion_pair() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                make_word("ping", vec!["pong"]),
                make_word("pong", vec!["ping"]),
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(graph.is_recursive("ping"));
        assert!(graph.is_recursive("pong"));
        assert!(graph.are_mutually_recursive("ping", "pong"));
        assert_eq!(graph.recursive_cycles().len(), 1);
        assert_eq!(graph.recursive_cycles()[0].len(), 2);
    }

    #[test]
    fn test_mutual_recursion_triple() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                make_word("a", vec!["b"]),
                make_word("b", vec!["c"]),
                make_word("c", vec!["a"]),
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(graph.is_recursive("a"));
        assert!(graph.is_recursive("b"));
        assert!(graph.is_recursive("c"));
        assert!(graph.are_mutually_recursive("a", "b"));
        assert!(graph.are_mutually_recursive("b", "c"));
        assert!(graph.are_mutually_recursive("a", "c"));
        assert_eq!(graph.recursive_cycles().len(), 1);
        assert_eq!(graph.recursive_cycles()[0].len(), 3);
    }

    #[test]
    fn test_multiple_independent_cycles() {
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                // Cycle 1: ping <-> pong
                make_word("ping", vec!["pong"]),
                make_word("pong", vec!["ping"]),
                // Cycle 2: even <-> odd
                make_word("even", vec!["odd"]),
                make_word("odd", vec!["even"]),
                // Non-recursive
                make_word("main", vec!["ping", "even"]),
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(graph.is_recursive("ping"));
        assert!(graph.is_recursive("pong"));
        assert!(graph.is_recursive("even"));
        assert!(graph.is_recursive("odd"));
        assert!(!graph.is_recursive("main"));

        assert!(graph.are_mutually_recursive("ping", "pong"));
        assert!(graph.are_mutually_recursive("even", "odd"));
        assert!(!graph.are_mutually_recursive("ping", "even"));

        assert_eq!(graph.recursive_cycles().len(), 2);
    }

    #[test]
    fn test_calls_to_unknown_words() {
        // Calls to builtins or external words should be ignored
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![make_word("foo", vec!["dup", "drop", "unknown_builtin"])],
        };

        let graph = CallGraph::build(&program);
        assert!(!graph.is_recursive("foo"));
        // Callees should only include known words
        assert!(graph.callees("foo").unwrap().is_empty());
    }

    #[test]
    fn test_cycle_with_builtins_interspersed() {
        // Cycles should be detected even when builtins are called between user words
        // e.g., : foo dup drop bar ;  : bar swap foo ;
        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                make_word("foo", vec!["dup", "drop", "bar"]),
                make_word("bar", vec!["swap", "foo"]),
            ],
        };

        let graph = CallGraph::build(&program);
        // foo and bar should still form a cycle despite builtin calls
        assert!(graph.is_recursive("foo"));
        assert!(graph.is_recursive("bar"));
        assert!(graph.are_mutually_recursive("foo", "bar"));

        // Builtins should not appear in callees
        let foo_callees = graph.callees("foo").unwrap();
        assert!(foo_callees.contains("bar"));
        assert!(!foo_callees.contains("dup"));
        assert!(!foo_callees.contains("drop"));
    }

    #[test]
    fn test_cycle_through_quotation() {
        // Calls inside quotations should be detected
        // e.g., : foo [ bar ] call ;  : bar foo ;
        use crate::ast::Statement;

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "foo".to_string(),
                    effect: None,
                    body: vec![
                        Statement::Quotation {
                            id: 0,
                            body: vec![Statement::WordCall {
                                name: "bar".to_string(),
                                span: None,
                            }],
                            span: None,
                        },
                        Statement::WordCall {
                            name: "call".to_string(),
                            span: None,
                        },
                    ],
                    source: None,
                    allowed_lints: vec![],
                },
                make_word("bar", vec!["foo"]),
            ],
        };

        let graph = CallGraph::build(&program);
        // foo calls bar (inside quotation), bar calls foo
        assert!(graph.is_recursive("foo"));
        assert!(graph.is_recursive("bar"));
        assert!(graph.are_mutually_recursive("foo", "bar"));
    }

    #[test]
    fn test_cycle_through_if_branch() {
        // Calls inside if branches should be detected
        use crate::ast::Statement;

        let program = Program {
            includes: vec![],
            unions: vec![],
            words: vec![
                WordDef {
                    name: "even".to_string(),
                    effect: None,
                    body: vec![Statement::If {
                        then_branch: vec![],
                        else_branch: Some(vec![Statement::WordCall {
                            name: "odd".to_string(),
                            span: None,
                        }]),
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
                WordDef {
                    name: "odd".to_string(),
                    effect: None,
                    body: vec![Statement::If {
                        then_branch: vec![],
                        else_branch: Some(vec![Statement::WordCall {
                            name: "even".to_string(),
                            span: None,
                        }]),
                    }],
                    source: None,
                    allowed_lints: vec![],
                },
            ],
        };

        let graph = CallGraph::build(&program);
        assert!(graph.is_recursive("even"));
        assert!(graph.is_recursive("odd"));
        assert!(graph.are_mutually_recursive("even", "odd"));
    }
}
