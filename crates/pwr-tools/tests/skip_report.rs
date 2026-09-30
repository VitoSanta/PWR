//! A skipped test leaves a line CI can read (plan W0.2).

mod common;

#[test]
fn a_skip_is_written_to_the_log_ci_reads() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("skips.log");
    common::skip_to(Some(&log), "nothing to exercise here");
    common::skip_to(Some(&log), "a second reason");
    let written = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<&str> = written.lines().collect();
    assert_eq!(
        lines,
        [
            "PWR-SKIP a_skip_is_written_to_the_log_ci_reads nothing to exercise here",
            "PWR-SKIP a_skip_is_written_to_the_log_ci_reads a second reason",
        ]
    );
}

#[test]
fn without_a_log_a_skip_only_prints() {
    common::skip_to(None, "nothing to exercise here");
}
