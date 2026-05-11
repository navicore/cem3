//! `chan.yield` Reachability Lint
//!
//! Errors when `chan.yield` is called from a word with no cooperative
//! peer — i.e. a word that is not reachable from any `strand.spawn` or
//! `strand.weave`. Such a call is a no-op masquerading as concurrency
//! machinery and almost always indicates a deleted/forgotten spawn.
//!
//! # Cooperative set
//!
//! A user word is *cooperative* iff at least one of the following holds:
//!
//! 1. Its body contains a `strand.spawn` or `strand.weave` call (the
//!    "spawner-self" rule — yielding from the same word that just
//!    spawned a peer is canonical).
//! 2. It appears as a literal-quotation body passed directly to
//!    `strand.spawn` / `strand.weave` (the seed roots).
//! 3. It is transitively called by some cooperative word.
//!
//! Quotations passed to other combinators (`if`, `when`, `map`, `dip`,
//! ...) inherit their enclosing word's classification; we do not try to
//! track quotations stored in data and later invoked via `call`. The
//! design doc accepts that conservatism as a false-positive risk that
//! has not appeared in practice.

use std::path::{Path, PathBuf};

use crate::ast::{Program, Span, Statement, WordDef};
use crate::call_graph::CallGraph;
use crate::lint::{LintDiagnostic, Severity};

const LINT_ID: &str = "unreachable-chan-yield";

pub struct ChanYieldAnalyzer {
    file: PathBuf,
}

impl ChanYieldAnalyzer {
    pub fn new(file: &Path) -> Self {
        ChanYieldAnalyzer {
            file: file.to_path_buf(),
        }
    }

    pub fn analyze_program(
        &self,
        program: &Program,
        call_graph: &CallGraph,
    ) -> Vec<LintDiagnostic> {
        let coop = cooperative_set(program, call_graph);

        let mut diagnostics = Vec::new();
        for word in &program.words {
            if coop.contains(&word.name) {
                continue;
            }
            let mut sites = Vec::new();
            collect_chan_yield_sites(&word.body, &mut sites);
            for span in sites {
                diagnostics.push(self.diagnostic(word, span));
            }
        }
        diagnostics
    }

    fn diagnostic(&self, word: &WordDef, span: Option<Span>) -> LintDiagnostic {
        let line = span.as_ref().map(|s| s.line).unwrap_or(0);
        let column = span.as_ref().map(|s| s.column);
        LintDiagnostic {
            id: LINT_ID.to_string(),
            message: format!(
                "`chan.yield` in `{}` has no peer to yield to — this code path is not \
                 reachable from any `strand.spawn` or `strand.weave`. Either remove the \
                 call, or run this word under `[ ... ] strand.spawn`.",
                word.name
            ),
            severity: Severity::Error,
            replacement: String::new(),
            file: self.file.clone(),
            line,
            end_line: None,
            start_column: column,
            end_column: None,
            word_name: word.name.clone(),
            start_index: 0,
            end_index: 0,
        }
    }
}

/// Compute the set of user-word names that are cooperative.
fn cooperative_set(program: &Program, call_graph: &CallGraph) -> std::collections::HashSet<String> {
    use std::collections::HashSet;

    let user_words: HashSet<&str> = program.words.iter().map(|w| w.name.as_str()).collect();

    // Seeds: every word that either (a) contains a spawn/weave call, or
    // (b) is the literal-quotation body passed directly to spawn/weave
    // and resolves to a single user word call. We approximate (b) by
    // collecting all user-word calls reachable from each spawn-quotation
    // body — they all become seeds.
    let mut seeds: HashSet<String> = HashSet::new();
    for word in &program.words {
        let mut spawner = false;
        scan_for_seeds(&word.body, &user_words, &mut seeds, &mut spawner);
        if spawner {
            seeds.insert(word.name.clone());
        }
    }

    // Transitive closure: every user word reachable from a seed via
    // user→user call edges joins the cooperative set.
    let mut coop: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = seeds.into_iter().collect();
    while let Some(w) = frontier.pop() {
        if !coop.insert(w.clone()) {
            continue;
        }
        if let Some(callees) = call_graph.callees(&w) {
            for callee in callees {
                if !coop.contains(callee) {
                    frontier.push(callee.clone());
                }
            }
        }
    }
    coop
}

/// Walk a statement list and:
/// - record `spawner = true` if any `strand.spawn` / `strand.weave` call
///   is encountered in this lexical scope (recurses into quotations,
///   if branches, and match arms — because seeing a spawn anywhere in
///   the enclosing word still triggers the spawner-self rule);
/// - for every `strand.spawn` / `strand.weave` whose immediately
///   preceding statement is a literal `Quotation`, add every user-word
///   call inside that quotation body to `seeds`.
fn scan_for_seeds(
    statements: &[Statement],
    user_words: &std::collections::HashSet<&str>,
    seeds: &mut std::collections::HashSet<String>,
    spawner: &mut bool,
) {
    for (i, stmt) in statements.iter().enumerate() {
        match stmt {
            Statement::WordCall { name, .. } if is_spawn_or_weave(name) => {
                *spawner = true;
                if let Some(Statement::Quotation { body, .. }) = statements.get(i.wrapping_sub(1))
                    && i > 0
                {
                    collect_user_word_calls(body, user_words, seeds);
                }
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                scan_for_seeds(then_branch, user_words, seeds, spawner);
                if let Some(else_stmts) = else_branch {
                    scan_for_seeds(else_stmts, user_words, seeds, spawner);
                }
            }
            Statement::Quotation { body, .. } => {
                scan_for_seeds(body, user_words, seeds, spawner);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    scan_for_seeds(&arm.body, user_words, seeds, spawner);
                }
            }
            _ => {}
        }
    }
}

/// Collect every user-word name called inside `body`, recursing into
/// quotations, if branches, and match arms.
fn collect_user_word_calls(
    body: &[Statement],
    user_words: &std::collections::HashSet<&str>,
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in body {
        match stmt {
            Statement::WordCall { name, .. } if user_words.contains(name.as_str()) => {
                out.insert(name.clone());
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_user_word_calls(then_branch, user_words, out);
                if let Some(else_stmts) = else_branch {
                    collect_user_word_calls(else_stmts, user_words, out);
                }
            }
            Statement::Quotation { body, .. } => {
                collect_user_word_calls(body, user_words, out);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_user_word_calls(&arm.body, user_words, out);
                }
            }
            _ => {}
        }
    }
}

/// Record the span of every `chan.yield` call in `body`, recursing into
/// quotations, if branches, and match arms.
fn collect_chan_yield_sites(body: &[Statement], out: &mut Vec<Option<Span>>) {
    for stmt in body {
        match stmt {
            Statement::WordCall { name, span } if name == "chan.yield" => {
                out.push(span.clone());
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_chan_yield_sites(then_branch, out);
                if let Some(else_stmts) = else_branch {
                    collect_chan_yield_sites(else_stmts, out);
                }
            }
            Statement::Quotation { body, .. } => {
                collect_chan_yield_sites(body, out);
            }
            Statement::Match { arms, .. } => {
                for arm in arms {
                    collect_chan_yield_sites(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

fn is_spawn_or_weave(name: &str) -> bool {
    name == "strand.spawn" || name == "strand.weave"
}

#[cfg(test)]
mod tests;
