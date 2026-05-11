use super::*;
use crate::ast::{Program, Span, Statement, WordDef};
use crate::call_graph::CallGraph;
use crate::types::{Effect, StackType};
use std::path::Path;

fn make_word(name: &str, body: Vec<Statement>) -> WordDef {
    WordDef {
        name: name.to_string(),
        effect: Some(Effect::new(StackType::Empty, StackType::Empty)),
        body,
        source: None,
        allowed_lints: vec![],
    }
}

fn call(name: &str, line: usize) -> Statement {
    Statement::WordCall {
        name: name.to_string(),
        span: Some(Span {
            line,
            column: 0,
            length: 1,
        }),
    }
}

fn quotation(body: Vec<Statement>) -> Statement {
    Statement::Quotation {
        id: 0,
        body,
        span: None,
    }
}

fn analyze(words: Vec<WordDef>) -> Vec<LintDiagnostic> {
    let program = Program {
        includes: vec![],
        unions: vec![],
        words,
    };
    let graph = CallGraph::build(&program);
    let analyzer = ChanYieldAnalyzer::new(Path::new("test.seq"));
    analyzer.analyze_program(&program, &graph)
}

#[test]
fn single_strand_chan_yield_in_main_errors() {
    // : main ( -- ) chan.yield ;
    let main = make_word("main", vec![call("chan.yield", 1)]);
    let diags = analyze(vec![main]);
    assert_eq!(diags.len(), 1, "{:?}", diags);
    assert_eq!(diags[0].id, "unreachable-chan-yield");
    assert_eq!(diags[0].severity, Severity::Error);
    assert_eq!(diags[0].word_name, "main");
    assert!(
        diags[0].message.contains("main"),
        "diagnostic should name the enclosing word: {}",
        diags[0].message
    );
}

#[test]
fn library_word_under_spawn_passes() {
    // : worker chan.yield ;
    // : main [ worker ] strand.spawn drop ;
    let worker = make_word("worker", vec![call("chan.yield", 2)]);
    let main = make_word(
        "main",
        vec![
            quotation(vec![call("worker", 10)]),
            call("strand.spawn", 11),
            call("drop", 11),
        ],
    );
    let diags = analyze(vec![worker, main]);
    assert!(
        diags.is_empty(),
        "worker is reachable via [worker] strand.spawn, should not error: {:?}",
        diags
    );
}

#[test]
fn library_word_not_under_spawn_errors() {
    // : worker chan.yield ;
    // : main worker ;     (no spawn anywhere)
    let worker = make_word("worker", vec![call("chan.yield", 2)]);
    let main = make_word("main", vec![call("worker", 10)]);
    let diags = analyze(vec![worker, main]);
    assert_eq!(
        diags.len(),
        1,
        "worker called directly without spawn should error: {:?}",
        diags
    );
    assert_eq!(diags[0].word_name, "worker");
}

#[test]
fn spawner_self_rule_chan_yield_in_main_alongside_spawn_passes() {
    // : worker 1 drop ;
    // : main [ worker ] strand.spawn drop chan.yield ;
    // The chan.yield in main has the worker as a peer (just spawned),
    // so the spawner-self rule lets it pass even though main is not
    // itself reached from a spawn.
    let worker = make_word("worker", vec![Statement::IntLiteral(1), call("drop", 1)]);
    let main = make_word(
        "main",
        vec![
            quotation(vec![call("worker", 10)]),
            call("strand.spawn", 11),
            call("drop", 11),
            call("chan.yield", 12),
        ],
    );
    let diags = analyze(vec![worker, main]);
    assert!(
        diags.is_empty(),
        "spawner-self rule should permit chan.yield in main alongside strand.spawn: {:?}",
        diags
    );
}

#[test]
fn strand_weave_is_also_a_coop_root() {
    // : gen chan.yield ;
    // : main [ gen ] strand.weave drop ;
    let generator = make_word("generator", vec![call("chan.yield", 2)]);
    let main = make_word(
        "main",
        vec![
            quotation(vec![call("generator", 10)]),
            call("strand.weave", 11),
            call("drop", 11),
        ],
    );
    let diags = analyze(vec![generator, main]);
    assert!(
        diags.is_empty(),
        "strand.weave should be a cooperative root: {:?}",
        diags
    );
}

#[test]
fn chan_yield_inside_if_branch_of_non_coop_word_errors() {
    // : main true [ chan.yield 1 ] [ 2 ] if drop ;
    let main = make_word(
        "main",
        vec![
            Statement::BoolLiteral(true),
            Statement::If {
                then_branch: vec![call("chan.yield", 3), Statement::IntLiteral(1)],
                else_branch: Some(vec![Statement::IntLiteral(2)]),
                span: Some(Span {
                    line: 2,
                    column: 0,
                    length: 2,
                }),
            },
            call("drop", 5),
        ],
    );
    let diags = analyze(vec![main]);
    assert_eq!(
        diags.len(),
        1,
        "chan.yield inside an if-branch of a non-coop word should error: {:?}",
        diags
    );
    assert_eq!(diags[0].word_name, "main");
}

#[test]
fn transitive_reachability_through_two_user_words() {
    // : leaf chan.yield ;
    // : mid leaf ;
    // : main [ mid ] strand.spawn drop ;
    // leaf should inherit coop status via mid via spawn.
    let leaf = make_word("leaf", vec![call("chan.yield", 1)]);
    let mid = make_word("mid", vec![call("leaf", 5)]);
    let main = make_word(
        "main",
        vec![
            quotation(vec![call("mid", 10)]),
            call("strand.spawn", 11),
            call("drop", 11),
        ],
    );
    let diags = analyze(vec![leaf, mid, main]);
    assert!(
        diags.is_empty(),
        "transitive reachability should propagate coop status: {:?}",
        diags
    );
}

#[test]
fn diagnostic_message_explains_the_problem() {
    let main = make_word("orchestrator", vec![call("chan.yield", 7)]);
    let diags = analyze(vec![main]);
    assert_eq!(diags.len(), 1);
    let msg = &diags[0].message;
    assert!(msg.contains("orchestrator"), "message: {}", msg);
    assert!(msg.contains("strand.spawn"), "message: {}", msg);
    assert!(
        msg.contains("not") && msg.contains("reachable"),
        "message should mention non-reachability: {}",
        msg
    );
}
