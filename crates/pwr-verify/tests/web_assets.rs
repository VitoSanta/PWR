//! The check running as a check, not as a function.

use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::time::Duration;

fn policy(root: &std::path::Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: Vec::new(),
        output_limit: 100_000,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    }
}

#[tokio::test]
async fn the_delivered_page_fails_its_baseline_and_the_rename_makes_it_pass() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("index.html"),
        "<link rel=\"stylesheet\" href=\"style.css\">\n<script src=\"script.js\" defer></script>\n",
    )
    .unwrap();
    std::fs::write(root.path().join("style.css"), "body{}").unwrap();
    std::fs::write(root.path().join("main.js"), "//").unwrap();

    let checks = pwr_verify::discover_checks(root.path(), "full").unwrap();
    let policy = policy(root.path());

    let red = pwr_verify::baseline(&policy, &checks).await.unwrap();
    let record = red.checks.first().expect("one check");
    assert_eq!(record.command, "pwr:web-assets");
    assert_eq!(record.result.exit_code, Some(1));
    assert!(
        record.result.stdout.contains("index.html:2"),
        "recovery reads a file and a line out of this: {}",
        record.result.stdout
    );
    // No process ran, and the audit says so rather than claiming a sandbox.
    assert!(!record.result.sandboxed);

    std::fs::rename(root.path().join("main.js"), root.path().join("script.js")).unwrap();
    let green = pwr_verify::baseline(&policy, &checks).await.unwrap();
    assert_eq!(green.checks[0].result.exit_code, Some(0));

    // A green run and a red run must not hash alike, or a comparison could not
    // tell the fix from the defect.
    assert_ne!(red.environment_hash, green.environment_hash);
}
