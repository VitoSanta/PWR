//! The regression suites run as part of `cargo test`, so a harness change that
//! breaks a recorded case fails the build the same day.

use std::path::Path;

fn suite(name: &str) -> pwr_eval::suite::SuiteReport {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../suites")
        .join(name);
    let suite = pwr_eval::suite::load(&path).unwrap();
    pwr_eval::suite::run(&suite, "test", None)
}

#[test]
fn a1_tool_calls_has_no_regression() {
    let report = suite("a1-tool-calls.json");
    let regressions: Vec<_> = report
        .regressions()
        .iter()
        .map(|result| format!("{}: {:?}", result.id, result.observed))
        .collect();
    assert!(regressions.is_empty(), "{regressions:#?}");
    let closed: Vec<_> = report
        .results
        .iter()
        .filter(|result| result.verdict == pwr_eval::suite::Verdict::GapClosed)
        .map(|result| result.id.as_str())
        .collect();
    assert!(
        closed.is_empty(),
        "these gaps have closed; drop their known_gap in the suite file: {closed:?}"
    );
}

#[test]
fn a_case_that_regresses_is_reported_as_one() {
    let mut suite = pwr_eval::suite::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../suites/a1-tool-calls.json"),
    )
    .unwrap();
    let pwr_eval::suite::Case::Replay(first) = &mut suite.cases[0] else {
        panic!("suite A1 opens with a replay case");
    };
    first.content = "no call at all".into();
    let report = pwr_eval::suite::run(&suite, "test", None);
    assert_eq!(report.regressions().len(), 1);
}
