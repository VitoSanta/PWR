//! What a pair has to be before a delta between two campaigns means anything.
//!
//! `compare` keys on deployment, task and seed. Every other field a report
//! records about the conditions it ran under -- corpus revision, harness
//! revision, sampling, hardware, execution profile -- was written by the runner
//! and read by nobody, and a `BTreeMap::extend` on the way in meant a repeated
//! trial overwrote its earlier self without a word. These fixtures are the
//! cases that produced a number nobody should have believed.

use pwr_eval::{
    Attempts, ConditionField, PairingFault, Side, SuiteReport, TaskKind, TaskOutcome, TrialClass,
    TrialKey, compare, compare_strict,
};
use std::str::FromStr;

fn resolved(task: &str, seed: u64) -> TaskOutcome {
    TaskOutcome {
        task_id: task.into(),
        kind: TaskKind::Bugfix,
        seed,
        declared_complete: true,
        hidden_verifier_passed: true,
        tool_attempts: 5,
        duration_secs: 10.0,
        prompt_tokens: 300,
        generated_tokens: 100,
        ..Default::default()
    }
}

fn report(outcomes: Vec<TaskOutcome>) -> SuiteReport {
    SuiteReport {
        suite: "s".into(),
        corpus_rev: "rev-a".into(),
        harness_rev: "harness-a".into(),
        model_digest: "digest-0123456789".into(),
        deployment_fingerprint: "fp".into(),
        hardware_compatibility_key: "hw".into(),
        execution_profile_id: pwr_domain::Id::nil(),
        turn_timeout_secs: Some(900),
        mode: Default::default(),
        arm: Default::default(),
        oracle_context: false,
        context_policy: "current".into(),
        seeds: vec![1],
        sampling: Default::default(),
        outcomes,
        generated_at: chrono::Utc::now(),
    }
}

/// The control and the treatment as they differ in the ordinary case: one
/// harness revision against another, everything else held.
fn sides() -> (SuiteReport, SuiteReport) {
    let control = report(vec![resolved("t1", 1)]);
    let mut treatment = report(vec![resolved("t1", 1)]);
    treatment.harness_rev = "harness-b".into();
    (control, treatment)
}

const HARNESS: [ConditionField; 1] = [ConditionField::HarnessRev];

#[test]
fn a_declared_difference_is_the_treatment() {
    let (control, treatment) = sides();
    let comparison = compare_strict(&[control], &[treatment], &HARNESS).expect("pairs");
    assert_eq!(comparison.pairs.len(), 1);
    assert_eq!(comparison.treatment, vec![ConditionField::HarnessRev]);
    assert!(comparison.declared_but_identical.is_empty());
    assert!(comparison.unpaired_control.is_empty());
}

/// `compare` builds each side with `BTreeMap::extend`, so a trial recorded
/// twice keeps whichever report was read last and says nothing. Reading a
/// directory that holds a re-run of one seed is enough to produce it, and the
/// campaign that did would report a rate over a denominator it never ran.
#[test]
fn a_repeated_trial_is_refused_rather_than_silently_overwritten() {
    let (control, treatment) = sides();
    let again = report(vec![resolved("t1", 1)]);

    let faults = compare_strict(
        &[control.clone(), again.clone()],
        std::slice::from_ref(&treatment),
        &HARNESS,
    )
    .expect_err("a trial recorded twice is not a denominator")
    .faults;
    assert_eq!(
        faults,
        vec![PairingFault::DuplicateTrial {
            side: Side::Control,
            key: TrialKey {
                model_digest: "digest-0123456789".into(),
                task_id: "t1".into(),
                seed: 1,
            },
        }]
    );

    // The behaviour this replaces, asserted rather than described: the legacy
    // comparator forms the pair and reports nothing wrong with it.
    let legacy = compare(&[control, again], &[treatment]);
    assert_eq!(legacy.pairs.len(), 1);
    assert!(legacy.unpaired_control.is_empty());
}

/// Two campaigns that also changed their corpus revision produce a delta that
/// is not attributable to the harness. Both revisions are named, because a
/// rejection that says only `corpus_rev` sends the reader back to the files.
#[test]
fn an_undeclared_difference_refuses_the_comparison() {
    let (control, mut treatment) = sides();
    treatment.corpus_rev = "rev-b".into();

    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("the corpus moved under the comparison")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.field, ConditionField::CorpusRev);
    assert_eq!(
        (&*difference.control, &*difference.treatment),
        ("rev-a", "rev-b")
    );
}

/// The H2 arms differ only in what compaction keeps, so that is the one field
/// their comparison may declare. Undeclared, it refuses; declared alone, it
/// pairs. A report written before the field existed reads as `current`.
#[test]
fn the_context_policy_is_a_treatment_to_declare() {
    let control = report(vec![resolved("t1", 1)]);
    let mut treatment = report(vec![resolved("t1", 1)]);
    treatment.context_policy = "evidence-state-60".into();

    let faults = compare_strict(
        std::slice::from_ref(&control),
        &[treatment.clone()],
        &HARNESS,
    )
    .expect_err("the compaction moved under the comparison")
    .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.field, ConditionField::ContextPolicy);
    assert_eq!(
        (&*difference.control, &*difference.treatment),
        ("current", "evidence-state-60")
    );

    let comparison = compare_strict(&[control], &[treatment], &[ConditionField::ContextPolicy])
        .expect("declared");
    assert_eq!(comparison.pairs.len(), 1);
    assert_eq!(
        "context_policy".parse::<ConditionField>(),
        Ok(ConditionField::ContextPolicy)
    );

    let legacy: serde_json::Value = serde_json::to_value(report(vec![])).unwrap();
    let mut legacy = legacy.as_object().unwrap().clone();
    legacy.remove("context_policy");
    let read: SuiteReport = serde_json::from_value(serde_json::Value::Object(legacy)).unwrap();
    assert_eq!(read.context_policy, "current");
}

/// A sampling parameter is named. A comparison whose temperature moved has
/// changed one specific thing, and `sampling` would not say which.
#[test]
fn a_differing_sampling_parameter_is_named() {
    let (mut control, mut treatment) = sides();
    control.sampling.insert(
        "temperature".into(),
        pwr_domain::ResolvedParameter {
            value: serde_json::json!(0.0),
            source: pwr_domain::ParameterSource::PwrOverride,
        },
    );
    treatment.sampling.insert(
        "temperature".into(),
        pwr_domain::ResolvedParameter {
            value: serde_json::json!(0.7),
            source: pwr_domain::ParameterSource::PwrOverride,
        },
    );

    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("temperature is not the harness")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(
        difference.field,
        ConditionField::Sampling("temperature".into())
    );
    assert!(difference.control.starts_with("0.0"), "{difference:?}");
}

/// A parameter set on one side and absent on the other is a difference. An
/// absent entry is the case where nobody knows what the backend chose, which is
/// not the same condition as a value someone set.
#[test]
fn a_sampling_parameter_present_on_one_side_only_is_a_difference() {
    let (control, mut treatment) = sides();
    treatment.sampling.insert(
        "top_p".into(),
        pwr_domain::ResolvedParameter {
            value: serde_json::json!(0.9),
            source: pwr_domain::ParameterSource::BackendDefault,
        },
    );

    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("one side set a parameter the other did not")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.control, "absent");
}

/// Two runs of the same harness against the same corpus are one campaign
/// measured twice. The seed variance is worth knowing and the delta is not a
/// treatment effect, so the comparator refuses to present it as one.
#[test]
fn identical_conditions_are_not_a_comparison() {
    let control = report(vec![resolved("t1", 1)]);
    let treatment = report(vec![resolved("t1", 1)]);
    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("nothing differs")
        .faults;
    assert_eq!(faults, vec![PairingFault::NothingUnderTest]);
}

/// A declaration is a permission, not an assertion: declaring two fields and
/// varying one is allowed. It is still worth saying, because a campaign that
/// meant to vary both should find out here rather than in the write-up.
#[test]
fn a_declared_field_that_never_differs_is_reported_not_refused() {
    let (control, treatment) = sides();
    let comparison = compare_strict(
        &[control],
        &[treatment],
        &[
            ConditionField::HarnessRev,
            ConditionField::ExecutionProfileId,
        ],
    )
    .expect("pairs");
    assert_eq!(
        comparison.declared_but_identical,
        vec![ConditionField::ExecutionProfileId]
    );
}

/// The defect that makes an uplift claim unsafe: `compare` builds its sides
/// from `measured_outcomes`, which drops provider failures, so a condition
/// whose backend fell over on half its trials is compared on the half that
/// survived. The trial was assigned and the budget was spent on it.
#[test]
fn a_provider_failure_is_an_unresolved_trial_not_an_absent_one() {
    let mut dropped = resolved("t2", 1);
    dropped.provider_failure = true;
    dropped.declared_complete = false;
    dropped.hidden_verifier_passed = false;

    let control = report(vec![resolved("t1", 1), dropped.clone()]);
    let mut treatment = report(vec![resolved("t1", 1), resolved("t2", 1)]);
    treatment.harness_rev = "harness-b".into();

    let comparison = compare_strict(
        std::slice::from_ref(&control),
        std::slice::from_ref(&treatment),
        &HARNESS,
    )
    .expect("pairs");
    assert_eq!(comparison.control.assigned, 2);
    assert_eq!(comparison.control.provider_failed, 1);
    assert_eq!(comparison.control.resolved, 1);
    assert_eq!(comparison.control.rate(), 0.5);
    let pair = comparison
        .pairs
        .iter()
        .find(|p| p.key.task_id == "t2")
        .expect("the failed trial is still a pair");
    assert_eq!(pair.control.class, TrialClass::ProviderFailed);
    assert_eq!(pair.treatment.class, TrialClass::Resolved);

    // What it used to do: the pair disappears, and the surviving side is
    // compared against nothing.
    let legacy = compare(&[control], &[treatment]);
    assert_eq!(legacy.pairs.len(), 1);
    assert_eq!(legacy.unpaired_treatment.len(), 1);
}

/// A trial that leaves the denominator without appearing anywhere else is the
/// defect the classes exist to prevent, so the sum is asserted rather than
/// assumed.
#[test]
fn the_classes_account_for_every_assigned_trial() {
    let mut dropped = resolved("t2", 1);
    dropped.provider_failure = true;
    let mut late = resolved("t3", 1);
    late.timed_out = true;
    late.declared_complete = false;
    let mut refused = resolved("t4", 1);
    refused.declined = true;
    refused.declared_complete = false;
    refused.hidden_verifier_passed = false;
    let mut wrong = resolved("t5", 1);
    wrong.hidden_verifier_passed = false;

    let control = report(vec![resolved("t1", 1), dropped, late, refused, wrong]);
    let mut treatment = report(control.outcomes.clone());
    treatment.harness_rev = "harness-b".into();

    let attempts = compare_strict(&[control], &[treatment], &HARNESS)
        .expect("pairs")
        .control;
    assert_eq!(attempts.assigned, 5);
    assert_eq!(
        (
            attempts.resolved,
            attempts.provider_failed,
            attempts.timed_out,
            attempts.declined,
            attempts.unresolved
        ),
        (1, 1, 1, 1, 1)
    );
    assert!(attempts.reconciles(), "{attempts:?}");
    assert_eq!(Attempts::default().rate(), 0.0);
}

/// An artifact stores `0` both for a run that generated nothing and for a run
/// whose backend reported nothing, and nothing in the file separates them. So
/// the totals carry their coverage and claim no more than that.
#[test]
fn a_run_without_counters_is_coverage_rather_than_a_zero() {
    let mut uncounted = resolved("t2", 1);
    uncounted.prompt_tokens = 0;
    uncounted.generated_tokens = 0;

    let control = report(vec![resolved("t1", 1), uncounted]);
    let mut treatment = report(control.outcomes.clone());
    treatment.harness_rev = "harness-b".into();

    let comparison = compare_strict(&[control], &[treatment], &HARNESS).expect("pairs");
    let attempts = &comparison.control;
    assert_eq!((attempts.assigned, attempts.with_counters), (2, 1));
    // Summed over the covered runs only: the uncounted one contributes no
    // tokens and is not thereby a run that generated none.
    assert_eq!(attempts.generated_tokens, 100);
    // Wall clock and actions are recorded either way, because they were spent.
    assert_eq!(attempts.actions, 10);
    assert_eq!(attempts.secs, 20.0);
    assert!(comparison.markdown().contains("Counter coverage"));
    assert!(comparison.markdown().contains("1/2"));
}

/// Two campaigns with no task in common are two campaigns, and the delta
/// between them is not a paired estimate.
#[test]
fn campaigns_with_nothing_in_common_are_refused() {
    let control = report(vec![resolved("t1", 1)]);
    let mut treatment = report(vec![resolved("t9", 1)]);
    treatment.harness_rev = "harness-b".into();
    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("no shared trial")
        .faults;
    assert_eq!(faults, vec![PairingFault::NoPairs]);
}

/// The distinction the audit asked for by name. A campaign that hands the agent
/// the corpus's verifier and one that lets the workspace's checks be discovered
/// are measuring different things: the first scores a deployment against a check
/// the corpus chose, the second scores what a user gets. Pooling them would
/// average two experiments.
#[test]
fn the_two_evaluation_modes_are_not_pooled() {
    let (control, mut treatment) = sides();
    treatment.mode = pwr_eval::EvaluationMode::ProductPath;
    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("two modes were compared as one")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.field, ConditionField::Mode);
    assert_eq!(
        (&*difference.control, &*difference.treatment),
        ("verifier_supplied", "product_path")
    );
}

/// Declaring the mode as the treatment is allowed and is a different experiment
/// from a harness comparison: it measures what check discovery costs, not what
/// the harness does. Allowed, and named, so nobody reads it as the other.
#[test]
fn the_mode_may_be_declared_as_the_treatment() {
    let (control, mut treatment) = sides();
    treatment.harness_rev = control.harness_rev.clone();
    treatment.mode = pwr_eval::EvaluationMode::ProductPath;
    let comparison = compare_strict(&[control], &[treatment], &[ConditionField::Mode])
        .expect("a declared mode difference is a treatment");
    assert_eq!(comparison.treatment, vec![ConditionField::Mode]);
}

/// A report written before the mode existed is verifier-supplied, which is what
/// every one of them was. Reading it as anything else would rewrite history.
#[test]
fn an_older_report_reads_as_verifier_supplied() {
    let older: pwr_eval::SuiteReport = serde_json::from_value(serde_json::json!({
        "suite": "s", "corpus_rev": "rev", "harness_rev": "h",
        "model_digest": "d", "deployment_fingerprint": "fp",
        "hardware_compatibility_key": "hw",
        "execution_profile_id": pwr_domain::Id::nil(),
        "seeds": [1], "outcomes": [], "generated_at": "2026-09-01T00:00:00Z",
    }))
    .expect("an older report still parses");
    assert_eq!(older.mode, pwr_eval::EvaluationMode::VerifierSupplied);
    assert_eq!(older.turn_timeout_secs, None);
}

/// A campaign property the corpus cannot carry. Two campaigns whose turns were
/// allowed different lengths are not paired trials, and until this was recorded
/// nothing could tell -- `corpus_rev` covers each task's own budgets, because
/// `Task` holds `max_actions` and `time_budget_secs`, and says nothing about
/// how long one turn of the campaign was permitted.
#[test]
fn a_different_turn_timeout_is_a_different_condition() {
    let (control, mut treatment) = sides();
    treatment.turn_timeout_secs = Some(300);
    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("the turn budget moved under the comparison")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.field, ConditionField::TurnTimeoutSecs);
    assert_eq!(
        (&*difference.control, &*difference.treatment),
        ("900", "300")
    );
}

/// An older report carries no turn timeout at all, and `unrecorded` is not a
/// value that happens to match another `unrecorded`: two campaigns that both
/// failed to record it are not thereby known to agree. They compare equal here
/// because the field is absent on both, and the honest reading of that is in
/// the word the difference prints when only one side has it.
#[test]
fn an_unrecorded_turn_timeout_prints_as_unrecorded() {
    let (mut control, treatment) = sides();
    control.turn_timeout_secs = None;
    let faults = compare_strict(&[control], &[treatment], &HARNESS)
        .expect_err("one side recorded a turn budget and the other did not")
        .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.control, "unrecorded");
}

/// An interruption says nothing about the deployment, and a timeout says the
/// deployment did not answer in time. Counting them together hides an
/// administrative event inside a capability measurement.
#[test]
fn an_interrupted_trial_is_not_a_timed_out_one() {
    let mut stopped = resolved("t2", 1);
    stopped.declared_complete = false;
    stopped.hidden_verifier_passed = false;
    stopped.terminal = Some(pwr_domain::TerminalClass::Interrupted);

    let control = report(vec![resolved("t1", 1), stopped]);
    let mut treatment = report(control.outcomes.clone());
    treatment.harness_rev = "harness-b".into();

    let attempts = compare_strict(&[control], &[treatment], &HARNESS)
        .expect("pairs")
        .control;
    assert_eq!(attempts.interrupted, 1);
    assert_eq!(attempts.timed_out, 0);
    assert_eq!(attempts.unresolved, 0, "it was collapsed into unresolved");
    assert!(attempts.reconciles(), "{attempts:?}");
}

/// The names the comparison is declared with on the command line are the names
/// it reports faults under, so a rejection can be acted on without a table.
#[test]
fn a_condition_field_parses_from_the_name_it_prints() {
    for field in [
        ConditionField::Mode,
        ConditionField::Suite,
        ConditionField::CorpusRev,
        ConditionField::HarnessRev,
        ConditionField::DeploymentFingerprint,
        ConditionField::HardwareCompatibilityKey,
        ConditionField::ExecutionProfileId,
        ConditionField::TurnTimeoutSecs,
        ConditionField::Sampling("temperature".into()),
    ] {
        let printed = field.to_string();
        assert_eq!(
            ConditionField::from_str(&printed).unwrap(),
            field,
            "{printed}"
        );
    }
    assert!(ConditionField::from_str("sampling.").is_err());
    assert!(ConditionField::from_str("model_digest").is_err());
}

/// The metrics table is a table, and the sentence above it names the mode the
/// campaign actually ran in. The note used to sit between the header row and
/// the first metric, which ends a Markdown table at its header -- observed in
/// the first `glm-4.7-flash:q8_0` campaign report -- and it said
/// "verifier-supplied" for a product-path campaign too.
#[test]
fn the_metrics_table_is_unbroken_and_names_the_mode_it_ran_in() {
    for (mode, says, never) in [
        (
            pwr_eval::EvaluationMode::VerifierSupplied,
            "Verifier-supplied",
            "Product-path",
        ),
        (
            pwr_eval::EvaluationMode::ProductPath,
            "Product-path",
            "Verifier-supplied",
        ),
    ] {
        let mut campaign = report(vec![resolved("t1", 1)]);
        campaign.mode = mode;
        let markdown = campaign.markdown();
        let lines: Vec<&str> = markdown.lines().collect();
        let header = lines
            .iter()
            .position(|line| line.starts_with("| Metric |"))
            .expect("no metrics table");
        assert!(lines[header + 1].starts_with("|---"), "{markdown}");
        assert!(
            lines[header + 2].starts_with("| "),
            "the first metric does not follow the header: {:?}",
            lines[header + 2]
        );
        assert!(markdown.contains(says), "{mode}: {markdown}");
        assert!(!markdown.contains(never), "{mode}: {markdown}");
    }
}

/// Two arms are two conditions. B0 against B1 is a comparison only when the
/// arm is what was declared to differ, and a report written before arms
/// existed reads as B1, which is what it ran.
#[test]
fn the_arms_are_not_pooled_unless_the_arm_is_the_treatment() {
    let (control, mut treatment) = sides();
    treatment.arm = pwr_eval::Arm::Conventional;
    let faults = compare_strict(
        std::slice::from_ref(&control),
        std::slice::from_ref(&treatment),
        &HARNESS,
    )
    .expect_err("two arms were compared as one")
    .faults;
    let [PairingFault::UndeclaredDifference { difference, .. }] = &faults[..] else {
        panic!("{faults:?}");
    };
    assert_eq!(difference.field, ConditionField::Arm);
    assert!(
        compare_strict(
            &[control],
            &[treatment],
            &[ConditionField::HarnessRev, ConditionField::Arm]
        )
        .is_ok()
    );

    let mut old = serde_json::to_value(report(vec![resolved("t1", 1)])).unwrap();
    old.as_object_mut().unwrap().remove("arm");
    let old: SuiteReport = serde_json::from_value(old).unwrap();
    assert_eq!(old.arm, pwr_eval::Arm::PWR);
    for (text, arm) in [
        ("b0", pwr_eval::Arm::Conventional),
        ("b1", pwr_eval::Arm::PWR),
        ("b2", pwr_eval::Arm::Staged),
    ] {
        assert_eq!(pwr_eval::Arm::from_str(text).unwrap(), arm);
        assert_eq!(pwr_eval::Arm::from_str(&arm.to_string()).unwrap(), arm);
    }
    assert_eq!(
        ConditionField::from_str("arm").unwrap(),
        ConditionField::Arm
    );
}
