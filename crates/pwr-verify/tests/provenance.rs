use pwr_tools::{SandboxPolicy, ToolPolicy};
use pwr_verify::{baseline, evidence};
use std::time::Duration;
fn policy(root: &std::path::Path) -> ToolPolicy {
    ToolPolicy {
        root: root.into(),
        extra_readable: vec![],
        protected: vec![],
        allow_commands: vec!["sh".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(2),
        sandbox: SandboxPolicy::Disabled,
        approvals: vec![],
    }
}
#[tokio::test]
async fn green_results_are_bound_to_exact_argv_and_source_and_expire_after_an_edit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.py"), "value = 1\n").unwrap();
    let commands = vec![("sh".into(), vec!["-c".into(), "printf 'checked\\n'".into()])];
    let before = baseline(&policy(root.path()), &commands).await.unwrap();
    let proof = before.provenance.as_ref().unwrap();
    assert_eq!(proof.commands, commands);
    assert_eq!(proof.before, proof.after);
    assert!(proof.after.files.contains_key("source.py"));
    assert!(matches!(
        evidence::checks(&before),
        pwr_domain::ChecksOutcome::Passed
    ));
    // Same length: timestamps/size are not a substitute for content identity.
    std::fs::write(root.path().join("source.py"), "value = 2\n").unwrap();
    assert!(
        before
            .integrity_error()
            .unwrap()
            .contains("after verification")
    );
    assert!(matches!(
        evidence::checks(&before),
        pwr_domain::ChecksOutcome::CouldNotRun { .. }
    ));
    let after = baseline(&policy(root.path()), &commands).await.unwrap();
    assert!(after.integrity_error().is_none());
    assert!(pwr_verify::compare(&before, &after).regression_free);
    assert_ne!(proof.after.digest, after.provenance.unwrap().after.digest);
}
#[tokio::test]
async fn a_green_checker_that_mutates_source_cannot_certify_the_new_bytes() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.txt"), "original").unwrap();
    let result = baseline(
        &policy(root.path()),
        &[(
            "sh".into(),
            vec!["-c".into(), "printf changed > source.txt; exit 0".into()],
        )],
    )
    .await
    .unwrap();
    assert_eq!(result.checks[0].result.exit_code, Some(0));
    assert!(
        result
            .integrity_error()
            .unwrap()
            .contains("while checks ran")
    );
    assert!(matches!(
        evidence::checks(&result),
        pwr_domain::ChecksOutcome::CouldNotRun { .. }
    ));
    assert!(!pwr_verify::compare(&result, &result).regression_free);
}
#[test]
fn scope_includes_direct_ignored_checkers_and_declarations_but_excludes_build_products() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(".pwr")).unwrap();
    std::fs::create_dir(root.path().join("target")).unwrap();
    std::fs::write(root.path().join(".gitignore"), "checker.sh\n").unwrap();
    std::fs::write(root.path().join("checker.sh"), "exit 0").unwrap();
    std::fs::write(root.path().join(".pwr/checks.json"), "{}").unwrap();
    std::fs::write(root.path().join("target/output"), "binary").unwrap();
    let proof =
        pwr_verify::provenance::snapshot(root.path(), &[("sh".into(), vec!["checker.sh".into()])])
            .unwrap();
    assert!(proof.files.contains_key("checker.sh"));
    assert!(proof.files.contains_key(".pwr/checks.json"));
    assert!(!proof.files.contains_key("target/output"));
    std::fs::write(root.path().join("checker.sh"), "exit 1").unwrap();
    assert_ne!(
        proof,
        pwr_verify::provenance::snapshot(root.path(), &[("sh".into(), vec!["checker.sh".into()])])
            .unwrap()
    );
}
#[cfg(unix)]
#[test]
fn escaped_or_symlinked_source_is_not_silently_omitted_from_a_proof() {
    let root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink("/etc/hosts", root.path().join("external")).unwrap();
    assert!(pwr_verify::provenance::snapshot(root.path(), &[]).is_err());
}

#[tokio::test]
async fn a_false_before_baseline_and_ambiguous_command_labels_do_not_establish_preservation() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("source.txt"), "original").unwrap();
    let mut before = baseline(
        &policy(root.path()),
        &[(
            "sh".into(),
            vec!["-c".into(), "printf changed > source.txt".into()],
        )],
    )
    .await
    .unwrap();
    let after = baseline(
        &policy(root.path()),
        &[("sh".into(), vec!["-c".into(), "exit 0".into()])],
    )
    .await
    .unwrap();
    assert!(!pwr_verify::compare(&before, &after).regression_free);
    before = after.clone();
    // Identical display labels cannot stand in for a different argv boundary.
    before.provenance.as_mut().unwrap().commands = vec![("checker".into(), vec!["a b".into()])];
    let mut current = after;
    current.provenance.as_mut().unwrap().commands =
        vec![("checker".into(), vec!["a".into(), "b".into()])];
    let comparison = pwr_verify::compare(&before, &current);
    assert!(!comparison.regression_free);
    assert!(
        comparison
            .missing_checks
            .iter()
            .any(|command| command.starts_with("argv "))
    );
}
