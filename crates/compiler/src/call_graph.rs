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
        let mut state = TarjanState::new();

        for word in &self.words {
            if !state.indices.contains_key(word) {
                self.tarjan_visit(word, &mut state);
            }
        }

        // Filter to only recursive SCCs
        state
            .sccs
            .into_iter()
            .filter(|scc| {
                if scc.len() > 1 {
                    // Multi-word SCC = mutual recursion
                    true
                } else if scc.len() == 1 {
                    // Single-word SCC: check if it calls itself
                    let word = scc.iter().next().expect("scc.len() == 1");
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
    fn tarjan_visit(&self, word: &str, state: &mut TarjanState) {
        let index = state.index_counter;
        state.index_counter += 1;
        state.indices.insert(word.to_string(), index);
        state.lowlinks.insert(word.to_string(), index);
        state.stack.push(word.to_string());
        state.on_stack.insert(word.to_string());

        // Visit all callees
        if let Some(callees) = self.edges.get(word) {
            for callee in callees {
                if !self.words.contains(callee) {
                    // External word (builtin), skip
                    continue;
                }
                if !state.indices.contains_key(callee) {
                    // Not yet visited
                    self.tarjan_visit(callee, state);
                    let callee_lowlink = *state
                        .lowlinks
                        .get(callee)
                        .expect("Tarjan invariant: callee was just visited");
                    state.relax_lowlink(word, callee_lowlink);
                } else if state.on_stack.contains(callee) {
                    // Callee is on stack, part of current SCC
                    let callee_index = *state
                        .indices
                        .get(callee)
                        .expect("Tarjan invariant: on-stack callee is indexed");
                    state.relax_lowlink(word, callee_index);
                }
            }
        }

        // If word is a root node, pop the SCC
        if state.lowlinks.get(word) == state.indices.get(word) {
            let mut scc = HashSet::new();
            loop {
                let w = state
                    .stack
                    .pop()
                    .expect("Tarjan invariant: stack non-empty until root");
                state.on_stack.remove(&w);
                scc.insert(w.clone());
                if w == word {
                    break;
                }
            }
            state.sccs.push(scc);
        }
    }
}

/// Mutable working state for Tarjan's SCC algorithm, threaded through the
/// recursive `tarjan_visit`.
struct TarjanState {
    index_counter: usize,
    stack: Vec<String>,
    on_stack: HashSet<String>,
    indices: HashMap<String, usize>,
    lowlinks: HashMap<String, usize>,
    sccs: Vec<HashSet<String>>,
}

impl TarjanState {
    fn new() -> Self {
        TarjanState {
            index_counter: 0,
            stack: Vec::new(),
            on_stack: HashSet::new(),
            indices: HashMap::new(),
            lowlinks: HashMap::new(),
            sccs: Vec::new(),
        }
    }

    /// Lower `word`'s lowlink to `candidate` if it is smaller.
    fn relax_lowlink(&mut self, word: &str, candidate: usize) {
        let lowlink = self
            .lowlinks
            .get_mut(word)
            .expect("Tarjan invariant: word has a lowlink");
        *lowlink = (*lowlink).min(candidate);
    }
}

/// Extract all word calls from a list of statements.
///
/// This recursively descends into quotations, if branches, and match arms.
fn extract_calls(statements: &[Statement], known_words: &HashSet<String>) -> HashSet<String> {
    let mut calls = HashSet::new();
    extract_each(statements, known_words, &mut calls);
    calls
}

/// Run `extract_calls_from_statement` over every statement in `statements`.
fn extract_each(
    statements: &[Statement],
    known_words: &HashSet<String>,
    calls: &mut HashSet<String>,
) {
    for stmt in statements {
        extract_calls_from_statement(stmt, known_words, calls);
    }
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
            span: _,
        } => {
            extract_each(then_branch, known_words, calls);
            if let Some(else_stmts) = else_branch {
                extract_each(else_stmts, known_words, calls);
            }
        }
        Statement::Quotation { body, .. } => {
            extract_each(body, known_words, calls);
        }
        Statement::Match { arms, span: _ } => {
            for arm in arms {
                extract_each(&arm.body, known_words, calls);
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
mod tests;
