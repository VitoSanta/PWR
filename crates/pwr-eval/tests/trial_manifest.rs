//! What a campaign set out to do, against what it recorded.
//!
//! A campaign was a loop over seeds and tasks appending to a vector, with a
//! report written at the end. Nothing recorded what it had set out to do, so a
//! campaign that died in its ninth trial of twelve left a report of eight
//! successes and no trace of the four never attempted -- and that report reads
//! exactly like a complete campaign with a smaller denominator.
//!
//! R0's exit asks that one small campaign can be exported with no unaccounted
//! assigned trial. This is what makes the question answerable.

use pwr_eval::{Arm, EvaluationMode, TaskKind, TaskOutcome, TrialKey, TrialManifest, reconcile};

fn key(task: &str, seed: u64) -> TrialKey {
    TrialKey {
        model_digest: "digest".into(),
        task_id: task.into(),
        seed,
    }
}

fn outcome(task: &str, seed: u64) -> TaskOutcome {
    TaskOutcome {
        task_id: task.into(),
        kind: TaskKind::Bugfix,
        seed,
        ..Default::default()
    }
}

fn manifest(assigned: Vec<TrialKey>) -> TrialManifest {
    TrialManifest::new(pwr_domain::new_id(), "suite", "rev", "digest", assigned)
}

#[test]
fn a_campaign_that_ran_everything_it_assigned_reconciles() {
    let m = manifest(vec![key("a", 1), key("b", 1)]);
    let found = reconcile(&m, &[outcome("a", 1), outcome("b", 1)]);
    assert!(found.complete(), "{found:?}");
    assert_eq!((found.assigned, found.recorded), (2, 2));
}

/// The case the manifest exists for. Eight of twelve is not a campaign of
/// eight, and the four are named rather than counted, because which ones were
/// lost decides whether what survived is a sample of anything.
#[test]
fn a_campaign_that_died_partway_names_what_it_never_attempted() {
    let m = manifest(vec![key("a", 1), key("b", 1), key("c", 1)]);
    let found = reconcile(&m, &[outcome("a", 1)]);
    assert!(!found.complete());
    assert_eq!(found.unaccounted, vec![key("b", 1), key("c", 1)]);
    assert!(found.unassigned.is_empty());
}

/// A trial nobody scheduled is not a bonus observation: it means the manifest
/// and the run disagree about what this campaign is, and reading the rates
/// before settling that is reading a number from two different experiments.
#[test]
fn an_outcome_nobody_assigned_is_a_disagreement_not_a_bonus() {
    let m = manifest(vec![key("a", 1)]);
    let found = reconcile(&m, &[outcome("a", 1), outcome("z", 9)]);
    assert!(!found.complete());
    assert_eq!(found.unassigned, vec![key("z", 9)]);
    assert!(found.unaccounted.is_empty());
}

/// Assigned twice makes an outcome ambiguous before anything reads it, and the
/// comparator's duplicate rejection is downstream of this: the manifest is
/// where a campaign can still be fixed rather than discarded.
#[test]
fn a_trial_assigned_twice_is_named_before_anyone_pairs_it() {
    let m = manifest(vec![key("a", 1), key("a", 1)]);
    let found = reconcile(&m, &[outcome("a", 1)]);
    assert!(!found.complete());
    assert_eq!(found.duplicated, vec![key("a", 1)]);
}

/// An empty campaign is complete rather than broken, and saying so keeps the
/// flag meaning one thing: every assigned trial has an outcome.
#[test]
fn a_campaign_that_assigned_nothing_is_not_a_campaign_that_lost_everything() {
    let found = reconcile(&manifest(vec![]), &[]);
    assert!(found.complete());
    assert_eq!((found.assigned, found.recorded), (0, 0));
}

#[test]
fn older_manifests_remain_readable_without_campaign_conditions() {
    let older = serde_json::json!({
        "campaign": pwr_domain::new_id(),
        "suite": "suite",
        "corpus_rev": "rev",
        "model_digest": "digest",
        "assigned": [],
        "created_at": "2026-09-13T00:00:00Z"
    });
    let manifest: TrialManifest = serde_json::from_value(older).expect("manifest unreadable");
    assert_eq!(manifest.harness_rev, None);
    assert_eq!(manifest.mode, None);
    assert_eq!(manifest.arm, None);
    assert_eq!(manifest.oracle_context, None);
    assert_eq!(manifest.turn_timeout_secs, None);
}

#[test]
fn a_manifest_can_record_the_campaign_conditions_before_trials_run() {
    let execution = pwr_domain::new_id();
    let manifest = manifest(vec![key("a", 1)]).with_conditions(
        "eval-test",
        "deployment",
        "hardware",
        execution,
        EvaluationMode::ProductPath,
        Arm::Staged,
        true,
        123,
    );
    assert_eq!(manifest.harness_rev.as_deref(), Some("eval-test"));
    assert_eq!(
        manifest.deployment_fingerprint.as_deref(),
        Some("deployment")
    );
    assert_eq!(
        manifest.hardware_compatibility_key.as_deref(),
        Some("hardware")
    );
    assert_eq!(manifest.execution_profile_id, Some(execution));
    assert_eq!(manifest.mode, Some(EvaluationMode::ProductPath));
    assert_eq!(manifest.arm, Some(Arm::Staged));
    assert_eq!(manifest.oracle_context, Some(true));
    assert_eq!(manifest.turn_timeout_secs, Some(123));
}
