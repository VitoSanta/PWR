//! The detectors, against the runs that taught them.
//!
//! Synthetic events would prove the code compiles. These are the three Angular
//! runs of 2026-09-07 as they happened, so a detector that stops recognising
//! the pathology it was written for fails here.

use pwr_observe::diagnose::{Finding, diagnose};

/// A fixture line as the export writes it, before the typed event is rebuilt.
#[derive(serde::Deserialize)]
struct Line {
    run_id: String,
    sequence: usize,
    at: chrono::DateTime<chrono::Utc>,
    event_type: String,
    event_hash: String,
    payload: serde_json::Value,
}

fn load(name: &str) -> Vec<pwr_observe::ExportedEvent> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.jsonl"));
    let text = std::fs::read_to_string(&path).expect("fixture");
    text.lines()
        .filter_map(|line| {
            let line: Line = serde_json::from_str(line).expect("fixture line");
            Some(pwr_observe::ExportedEvent {
                run_id: line.run_id,
                sequence: line.sequence,
                at: line.at,
                event_type: line.event_type.clone(),
                event_hash: line.event_hash,
                event: pwr_domain::RunEvent::from_stored(&line.event_type, &line.payload)?,
            })
        })
        .collect()
}

fn found<'a>(findings: &'a [Finding], detector: &str) -> Option<&'a Finding> {
    findings.iter().find(|f| f.detector == detector)
}

/// The run that motivated allowing read batches: twelve turns thrown away for
/// asking to read several files at once, and thirty-six reads of files the
/// harness had already answered as unchanged.
#[test]
fn the_run_before_read_batching_shows_why_it_was_needed() {
    let findings = diagnose(&load("angular-ornith-before-batching"));
    let rejected = found(&findings, "batchable_turns_rejected").expect("the pathology is there");
    // Nine, not the twelve counted by hand before this existed: that count
    // included three turns carrying an edit or a command, which are refused
    // for a reason that still holds. One of the nine had its tail cut from the
    // recorded detail, and the finding says so rather than pretending to know.
    assert_eq!(rejected.count, 9, "{}", rejected.says);
    assert!(
        rejected.detail["turns"]
            .as_array()
            .unwrap()
            .iter()
            .any(|turn| turn["tail_not_recorded"] == true),
        "a bounded detail is reported as bounded"
    );
    let rereads = found(&findings, "rereads_of_unchanged_files").expect("re-reads");
    assert_eq!(rereads.count, 36, "{}", rereads.says);
    assert!(found(&findings, "stall_length").is_some_and(|f| f.count >= 6));
}

/// The first version of read batching, which drove the prompt to the ceiling:
/// turns that reasoned and never answered, and a peak prompt against a context
/// that had nothing left in it.
#[test]
fn the_first_batching_run_shows_the_ceiling_it_hit() {
    let findings = diagnose(&load("angular-ornith-first-batching"));
    let silent = found(&findings, "thinking_only_turns").expect("silent turns");
    assert!(silent.count >= 4, "{}", silent.says);
    let peak = silent.detail["turns"]
        .as_array()
        .and_then(|turns| turns.last().cloned())
        .and_then(|turn| turn["prompt_tokens"].as_u64())
        .expect("a prompt size on the last silent turn");
    assert!(
        peak > 31_000,
        "the silent turns happened at the ceiling: {peak}"
    );
}

/// The run that got furthest and broke the build: told four times that its
/// checks were failing, and never edited again.
#[test]
fn the_gpt_oss_run_shows_a_repair_that_never_came() {
    let findings = diagnose(&load("angular-gptoss"));
    let unrepaired =
        found(&findings, "told_failing_never_repaired").expect("the behaviour that ended it");
    assert!(
        unrepaired.count > 0,
        "actions taken after being told, with no edit among them: {}",
        unrepaired.says
    );
    let absent = found(&findings, "reads_of_absent_paths").expect("reads of paths not there");
    assert_eq!(absent.count, 9, "{}", absent.says);
    let worst = absent.detail["per_path"]
        .as_object()
        .expect("grouped by path")
        .iter()
        .max_by_key(|(_, count)| count.as_u64().unwrap_or(0))
        .map(|(path, count)| (path.clone(), count.as_u64().unwrap_or(0)))
        .expect("a worst offender");
    assert!(worst.0.contains("app.module.ts"), "{worst:?}");
    assert!(
        worst.1 >= 6,
        "one path asked for six times or more: {worst:?}"
    );
}

/// A detector that fires on a run with none of its pathology would make the
/// report noise. Nothing is reported at count zero.
#[test]
fn a_run_reports_only_what_it_actually_did() {
    let findings = diagnose(&load("angular-gptoss"));
    assert!(findings.iter().all(|f| f.count > 0));
    // This run never had a turn refused for parallel reads: it never asked.
    assert!(found(&findings, "batchable_turns_rejected").is_none());
}

/// A deployment that reasons and then answers through a tool call has an empty
/// content field on every successful turn. Counting those reported sixty-six
/// silent turns in this run, which had one.
#[test]
fn a_reasoning_model_that_answers_is_not_a_silent_one() {
    let findings = diagnose(&load("angular-gptoss"));
    let silent = found(&findings, "thinking_only_turns").expect("the one that was silent");
    assert_eq!(silent.count, 1, "{}", silent.says);
}

/// Generations that produced nothing are named with what they cost (D.E2E-22).
#[test]
fn failed_generations_are_named_with_their_cost() {
    let failed = |turn: u32, outcome: &str, elapsed_ms: u64| pwr_observe::ExportedEvent {
        run_id: "r".into(),
        sequence: turn as usize,
        at: chrono::Utc::now(),
        event_type: "turn.failed".into(),
        event_hash: format!("h{turn}"),
        event: pwr_domain::RunEvent::GenerationFailed {
            step: 0,
            turn,
            outcome: outcome.into(),
            detail: String::new(),
            thinking_chars: 4000,
            content_chars: 0,
            elapsed_ms,
        },
    };
    let findings = diagnose(&[
        failed(1, "runaway_reply", 360_000),
        failed(2, "runaway_reply", 300_000),
        failed(3, "backend_fault", 1_000),
    ]);
    let finding = found(&findings, "failed_generations").expect("named");
    assert_eq!(finding.count, 3);
    assert!(finding.says.contains("661 s"), "{}", finding.says);
    assert!(finding.says.contains("2 runaway_reply"), "{}", finding.says);
}
