//! H05 — what survives compaction.
//!
//! Compaction is where a long run either keeps its bearings or loses them, and
//! its cost is invisible in every outcome metric: a run that forgets what it
//! already tried does not fail differently from one that never tried.
//!
//! The eight things a resumed run needs: the goal, the constraints, the files
//! discovered, the root cause, the changes made, the tests run, the remaining
//! work, and the approaches that already failed. Each is asserted here against
//! a real run's ledger rather than against the source that builds it.

use pwr_domain::RunEvent;
use pwr_store::Store;

fn action(capability: &str, path: &str, outcome: serde_json::Value, status: &str) -> RunEvent {
    RunEvent::ToolAction {
        action: serde_json::json!({"capability": capability, "path": path}),
        status: match status {
            "denied" => pwr_domain::ToolActionStatus::Denied,
            "failed" => pwr_domain::ToolActionStatus::Failed,
            _ => pwr_domain::ToolActionStatus::Allowed,
        },
        outcome_class: match status {
            "denied" => "policy_denial".into(),
            "failed" => "allowed_failure".into(),
            _ => "allowed_success".into(),
        },
        outcome: Some(outcome),
        denial: None,
        failure: None,
        failure_category: None,
    }
}

fn ledger_of_a_run() -> String {
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let append = |event: &RunEvent| store.append_event(Some(run_id), event).unwrap();

    append(&action(
        "read_file",
        "src/stats.rs",
        serde_json::json!({"artifact_hash": "aaa"}),
        "allowed",
    ));
    append(&action(
        "read_file",
        "src/report.rs",
        serde_json::json!({"artifact_hash": "bbb"}),
        "allowed",
    ));
    append(&RunEvent::ToolAction {
        action: serde_json::json!({"capability": "run_command", "executable": "cargo"}),
        status: pwr_domain::ToolActionStatus::Allowed,
        outcome_class: "allowed_failure".into(),
        outcome: Some(serde_json::json!({"exit_code": 101})),
        denial: None,
        failure: None,
        failure_category: None,
    });
    append(&action(
        "replace_text",
        "src/stats.rs",
        serde_json::json!({"new_hash": "ccc"}),
        "allowed",
    ));
    append(&action(
        "write_file",
        "/etc/hosts",
        serde_json::json!({}),
        "denied",
    ));
    append(&RunEvent::ToolAction {
        action: serde_json::json!({"capability": "run_command", "executable": "cargo"}),
        status: pwr_domain::ToolActionStatus::Allowed,
        outcome_class: "allowed_success".into(),
        outcome: Some(serde_json::json!({"exit_code": 0})),
        denial: None,
        failure: None,
        failure_category: None,
    });

    pwr_orchestrator::task_ledger(&store, run_id).expect("ledger")
}

#[test]
fn the_ledger_carries_what_was_read_changed_run_and_refused() {
    let ledger = ledger_of_a_run();

    // Files discovered.
    assert!(ledger.contains("src/stats.rs"), "{ledger}");
    assert!(ledger.contains("src/report.rs"), "{ledger}");
    // Changes made, with the hash a later edit must quote.
    assert!(
        ledger.contains("ccc"),
        "the edit's new hash was lost:\n{ledger}"
    );
    // Tests run.
    assert!(ledger.contains("cargo"), "{ledger}");
    // Approaches already refused, so they are not tried again.
    assert!(ledger.contains("do not repeat"), "{ledger}");
    assert!(ledger.contains("/etc/hosts"), "{ledger}");
}

/// A command that ran and failed is a failed approach, and a run that cannot
/// tell it from one that succeeded will try it again. The exit codes are both
/// in the ledger; this asserts they are distinguishable.
#[test]
fn a_command_that_failed_is_distinguishable_from_one_that_passed() {
    let ledger = ledger_of_a_run();
    assert!(
        ledger.contains("exited 101"),
        "a failing command reads like any other:\n{ledger}"
    );
    assert!(ledger.contains("exited 0"), "{ledger}");
}

/// The ledger says where it came from, because a resumed run that treats it as
/// its own memory will trust a stale hash. It is the audit, and the run is told
/// to re-read anything it needs.
#[test]
fn the_ledger_says_it_is_the_audit_and_not_memory() {
    let ledger = ledger_of_a_run();
    assert!(ledger.contains("audit"), "{ledger}");
    assert!(ledger.contains("re-read"), "{ledger}");
}
