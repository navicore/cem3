//! `seqc test` subcommand: run the Seq test runner over a path list.

use std::path::PathBuf;
use std::process;

pub(crate) fn run_test(paths: &[PathBuf], filter: Option<String>, verbose: bool) {
    use seqc::test_runner::TestRunner;

    let runner = TestRunner::new(verbose, filter);

    if let Err(e) = runner.validate_paths(paths) {
        eprintln!("{}", e);
        process::exit(2);
    }

    let summary = runner.run(paths);

    runner.print_results(&summary);

    if summary.has_failures() {
        process::exit(1);
    } else if summary.total == 0 && summary.compile_failures == 0 {
        eprintln!("No tests found");
        process::exit(2);
    }
}
