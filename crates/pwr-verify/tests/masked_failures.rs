use pwr_domain::ChecksOutcome;
use pwr_tools::ToolResult;
use pwr_verify::{CheckRecord, VerificationBaseline};

fn record(command: &str, output: &str) -> CheckRecord {
    CheckRecord {
        command: command.into(),
        result: ToolResult {
            exit_code: Some(0),
            stdout: output.into(),
            stderr: String::new(),
            duration_ms: 1,
            redacted: false,
            artifact_hash: pwr_domain::hash_bytes(output),
            stdout_truncated: false,
            stderr_truncated: false,
            sandboxed: false,
            failing_files: None,
            ran_in: None,
        },
    }
}
fn baseline(checks: Vec<CheckRecord>) -> VerificationBaseline {
    VerificationBaseline {
        id: pwr_domain::new_id(),
        captured_at: pwr_domain::now(),
        checks,
        environment_hash: "fixture".into(),
        provenance: None,
    }
}

#[test]
fn a_successful_wrapper_cannot_hide_a_reported_test_failure() {
    for (command, output) in [
        (
            "python -m unittest",
            "Ran 1 test in 0.001s\n\nFAILED (failures=1)",
        ),
        (
            "cargo test",
            "running 1 test\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; finished in 0.01s",
        ),
        (
            "pytest",
            "================= 1 failed, 2 passed in 0.03s =================",
        ),
        ("node --test", "# tests 2\n# pass 1\n# fail 1"),
        (
            "cargo test",
            "\u{1b}[31mtest result: FAILED. 0 passed; 1 failed\u{1b}[0m",
        ),
        ("vitest run", "Tests  1 failed | 2 passed (3)"),
    ] {
        let after = baseline(vec![record(command, output)]);
        assert!(
            matches!(
                pwr_verify::evidence::checks(&after),
                ChecksOutcome::Failed { .. }
            ),
            "{command}"
        );
        assert_eq!(after.checks[0].result.exit_code, Some(0));
        assert_eq!(after.checks[0].result.stdout, output);
    }
}

#[test]
fn omitting_a_previous_check_does_not_establish_a_preserved_baseline() {
    let before = baseline(vec![record("check A", "ok"), record("check B", "ok")]);
    let after = baseline(vec![record("check A", "ok")]);
    assert_eq!(
        pwr_verify::compare(&before, &after).missing_checks,
        vec!["check B"]
    );
    assert!(pwr_verify::compare(&before, &after).new_failures.is_empty());
    assert!(matches!(
        pwr_verify::evidence::baseline(Some(&before), &after),
        pwr_domain::BaselineOutcome::Incomplete { .. }
    ));
    assert!(!pwr_verify::compare_read_only_status(&before, &after).regression_free);
    assert_eq!(
        serde_json::to_value(pwr_verify::evidence::baseline(Some(&before), &after)).unwrap()["status"],
        "incomplete"
    );
}

#[test]
fn read_only_status_preservation_cannot_waive_a_contradictory_zero_exit() {
    let before = baseline(vec![record(
        "cargo test",
        "test result: FAILED. 0 passed; 1 failed",
    )]);
    assert!(!pwr_verify::compare_read_only_status(&before, &before).regression_free);
}

#[test]
fn passing_summaries_and_prose_do_not_invent_failures() {
    for output in [
        "Ran 2 tests in 0.001s\n\nOK",
        "test result: ok. 2 passed; 0 failed; 0 ignored",
        "2 passed in 0.03s",
        "# tests 2\n# fail 0",
        "documentation: FAILED (failures=1) is an example",
        "Build succeeded; tests failed before the fix",
    ] {
        assert!(
            matches!(
                pwr_verify::evidence::checks(&baseline(vec![record("check", output)])),
                ChecksOutcome::Passed
            ),
            "{output}"
        );
    }
}
