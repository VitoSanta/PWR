use pwr_tools::ToolResult;
use pwr_verify::{
    CheckRecord, VerificationBaseline, compare, declared_acceptance_checks, known_failure_checks,
    same_failure,
};

fn failure() -> ToolResult {
    ToolResult {
        exit_code: Some(1),
        stdout: "test existing_bug FAILED".into(),
        stderr: String::new(),
        duration_ms: 1,
        redacted: false,
        artifact_hash: "first".into(),
        stdout_truncated: false,
        stderr_truncated: false,
        sandboxed: true,
        failing_files: None,
    }
}

#[test]
fn red_checks_require_complete_equal_diagnostics() {
    let before = failure();
    let mut after = before.clone();
    after.duration_ms = 20;
    after.artifact_hash = "different timing".into();
    assert!(same_failure(&before, &after));
    after.stdout.push_str("\ntest new_bug FAILED");
    assert!(!same_failure(&before, &after));
    for hidden in 0..3 {
        let mut after = before.clone();
        after.stdout_truncated = hidden == 0;
        after.stderr_truncated = hidden == 1;
        after.redacted = hidden == 2;
        assert!(!same_failure(&before, &after));
        assert!(!same_failure(&after, &before));
    }
}

#[test]
fn a_new_red_check_without_a_baseline_is_not_regression_free() {
    let before = VerificationBaseline {
        id: pwr_domain::new_id(),
        captured_at: pwr_domain::now(),
        checks: vec![],
        environment_hash: "environment".into(),
    };
    let mut after = before.clone();
    after.id = pwr_domain::new_id();
    after.checks.push(CheckRecord {
        command: "new-check".into(),
        result: failure(),
    });
    assert_eq!(compare(&before, &after).new_failures, vec!["new-check"]);
}

#[test]
fn read_only_status_comparison_does_not_waive_new_failures() {
    let before = VerificationBaseline {
        id: pwr_domain::new_id(),
        captured_at: pwr_domain::now(),
        checks: vec![CheckRecord {
            command: "test".into(),
            result: failure(),
        }],
        environment_hash: "environment".into(),
    };
    let mut after = before.clone();
    after.checks[0]
        .result
        .stdout
        .push_str(" elapsed: 0.2 seconds");
    assert!(!compare(&before, &after).regression_free);
    assert!(pwr_verify::compare_read_only_status(&before, &after).regression_free);
    let mut green_before = before.clone();
    green_before.checks[0].result.exit_code = Some(0);
    assert!(!pwr_verify::compare_read_only_status(&green_before, &after).regression_free);
    after.checks[0].result.exit_code = None;
    assert!(!pwr_verify::compare_read_only_status(&before, &after).regression_free);
}

#[test]
fn exceptions_are_explicit_and_invalid_configuration_is_an_error() {
    let root = tempfile::tempdir().unwrap();
    assert!(known_failure_checks(root.path()).unwrap().is_empty());
    std::fs::create_dir(root.path().join(".pwr")).unwrap();
    let path = root.path().join(".pwr/checks.json");
    std::fs::write(
        &path,
        r#"{"checks":[{"executable":"cargo","args":["test"]}]}"#,
    )
    .unwrap();
    assert!(known_failure_checks(root.path()).unwrap().is_empty());
    std::fs::write(&path, r#"{"checks":[],"known_failures":[{"executable":"existing-check","args":["quarantined"]}]}"#).unwrap();
    assert_eq!(
        known_failure_checks(root.path()).unwrap(),
        vec![("existing-check".into(), vec!["quarantined".into()])]
    );
    assert!(pwr_verify::required_executables(root.path()).contains(&"existing-check".into()));
    std::fs::write(&path, r#"{"known_failures":[{"args":[]}]}"#).unwrap();
    assert!(known_failure_checks(root.path()).is_err());
}

#[test]
fn acceptance_evidence_is_explicit_and_stack_agnostic() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".pwr")).unwrap();
    let path = root.path().join(".pwr/checks.json");
    std::fs::write(
        &path,
        r#"{"checks":[
            {"executable":"cargo","args":["test"],"kind":"technical"},
            {"executable":"npm","args":["run","test:e2e"],"kind":"acceptance"},
            {"executable":"curl","args":["--fail","http://127.0.0.1:3000/health"],"kind":"acceptance"}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        declared_acceptance_checks(root.path()).unwrap(),
        vec![
            ("npm".into(), vec!["run".into(), "test:e2e".into()]),
            (
                "curl".into(),
                vec!["--fail".into(), "http://127.0.0.1:3000/health".into()],
            ),
        ]
    );

    std::fs::write(
        &path,
        r#"{"checks":[{"executable":"npm","kind":"wishful-thinking"}]}"#,
    )
    .unwrap();
    assert!(declared_acceptance_checks(root.path()).is_err());
}
