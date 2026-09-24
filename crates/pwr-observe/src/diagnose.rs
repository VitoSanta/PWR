//! Naming, from a run's own log, the failures somebody has already found once.
//!
//! Six runs of 2026-09-06/07 were read by hand, a row at a time, to find out
//! why each ended where it did. Every answer turned out to be a pattern over
//! events the log already carried: turns thrown away for asking to read four
//! files at once, thirty-six re-reads of files the harness itself had marked
//! unchanged, nine reads of a path that was never in the workspace, four turns
//! that reasoned and never answered with the prompt at 97% of the authorised
//! context, and a deployment told four times that its build was broken which
//! read on until the stall guard stopped it.
//!
//! None of that needed new instrumentation. It needed someone to look, and
//! looking took twenty minutes per run and produced numbers nobody could
//! compare with the next one. These detectors are that reading, written down.
//!
//! What they are not: a way of finding something new. A detector exists because
//! a person found the pathology first. The property worth having is that nobody
//! has to find the same one twice, and that a fix can be shown to hold on the
//! next run by a number going to zero rather than by reading timestamps.

use crate::ExportedEvent;
use pwr_domain::{RunEvent, ToolActionStatus};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One pathology, as found in one run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Stable name, so the same pathology is comparable across runs.
    pub detector: &'static str,
    /// How many times it happened. Zero-count findings are not reported: a run
    /// that did not exhibit a pathology says nothing about it.
    pub count: usize,
    /// One sentence a person reads without knowing the schema.
    pub says: String,
    /// The specifics, grouped the way the pathology is acted on.
    pub detail: serde_json::Value,
}

/// Everything the detectors found, worst first.
pub fn diagnose(events: &[ExportedEvent]) -> Vec<Finding> {
    let mut findings: Vec<Finding> = [
        batchable_turns_rejected(events),
        rereads_of_unchanged_files(events),
        reads_of_absent_paths(events),
        denied_commands(events),
        thinking_only_turns(events),
        failed_generations(events),
        context_headroom(events),
        told_failing_never_repaired(events),
        stall_length(events),
    ]
    .into_iter()
    .flatten()
    .collect();
    findings.sort_by_key(|finding| std::cmp::Reverse(finding.count));
    findings
}

/// Turns refused whole because they asked for several reads at once.
///
/// The pathology that motivated allowing read batches: twelve of fourteen
/// malformed turns in the first Angular run were this, on a 26-file project.
/// On a build that allows them, this count is zero, which is the cheapest
/// possible regression test for that change.
fn batchable_turns_rejected(events: &[ExportedEvent]) -> Option<Finding> {
    let mut wasted = Vec::new();
    for event in events {
        if let RunEvent::ActionMalformed { kind, detail, .. } = &event.event
            && kind == "multiple_calls"
        {
            let detail = detail.clone().unwrap_or_default();
            // The detail is the calls with their targets, and it is bounded:
            // a turn asking for eight files arrives with its tail cut off. The
            // partial call at the end is dropped rather than judged, because
            // reading `rea…` as "not a read" undercounts the pathology -- which
            // is how this detector first reported eight where the log has
            // nine, and how the hand count that preceded it reported twelve by
            // counting every multiple-call turn including the ones carrying an
            // edit. What was cut off is unknown and the finding says so.
            let truncated = detail.ends_with('…') || detail.ends_with("...");
            let mut calls: Vec<&str> = detail.split(", ").collect();
            if truncated {
                calls.pop();
            }
            if !calls.is_empty()
                && calls.iter().all(|call| {
                    call.starts_with("read_file")
                        || call.starts_with("search")
                        || call.starts_with("list_tree")
                        || call.starts_with("vcs_status")
                        || call.starts_with("vcs_diff")
                })
            {
                wasted.push(serde_json::json!({
                    "calls": detail,
                    "tail_not_recorded": truncated,
                }));
            }
        }
    }
    (!wasted.is_empty()).then(|| Finding {
        detector: "batchable_turns_rejected",
        count: wasted.len(),
        says: format!(
            "{} turns asked only for reads and were refused whole; each cost a turn and returned nothing",
            wasted.len()
        ),
        detail: serde_json::json!({"turns": wasted}),
    })
}

/// Reads the harness itself answered with "you have already seen this".
fn rereads_of_unchanged_files(events: &[ExportedEvent]) -> Option<Finding> {
    let mut per_path: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        if let RunEvent::ToolAction {
            action, outcome, ..
        } = &event.event
            && let Some(outcome) = outcome
            && outcome.get("already_read").is_some()
        {
            let path = action
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("(unnamed)")
                .to_string();
            *per_path.entry(path).or_default() += 1;
        }
    }
    let total: usize = per_path.values().sum();
    (total > 0).then(|| Finding {
        detector: "rereads_of_unchanged_files",
        count: total,
        says: format!(
            "{total} reads returned a file the run had already been shown unchanged, across {} paths",
            per_path.len()
        ),
        detail: serde_json::json!({"per_path": per_path}),
    })
}

/// Reads of paths the workspace does not have, grouped by path.
///
/// One path asked for six times is a different problem from six paths asked
/// for once: the first is a deployment holding a wrong model of the project.
fn reads_of_absent_paths(events: &[ExportedEvent]) -> Option<Finding> {
    let mut per_path: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        if let RunEvent::ToolAction {
            action,
            status,
            failure,
            denial,
            ..
        } = &event.event
        {
            let missing = matches!(status, ToolActionStatus::Failed)
                && failure
                    .as_deref()
                    .is_some_and(|f| f.contains("No such file"))
                || denial
                    .as_deref()
                    .is_some_and(|d| d.contains("does not exist in this workspace"));
            if missing {
                let path = action
                    .get("path")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("(unnamed)")
                    .to_string();
                *per_path.entry(path).or_default() += 1;
            }
        }
    }
    let total: usize = per_path.values().sum();
    (total > 0).then(|| Finding {
        detector: "reads_of_absent_paths",
        count: total,
        says: format!(
            "{total} reads asked for paths the workspace does not have; most repeated: {}",
            per_path
                .iter()
                .max_by_key(|(_, count)| **count)
                .map(|(path, count)| format!("{path} ({count}x)"))
                .unwrap_or_default()
        ),
        detail: serde_json::json!({"per_path": per_path}),
    })
}

/// Commands the policy refused, grouped by executable.
fn denied_commands(events: &[ExportedEvent]) -> Option<Finding> {
    let mut per_executable: BTreeMap<String, usize> = BTreeMap::new();
    for event in events {
        if let RunEvent::ToolAction {
            action,
            status,
            denial,
            ..
        } = &event.event
            && matches!(status, ToolActionStatus::Denied)
            && denial
                .as_deref()
                .is_some_and(|d| d.contains("not allowlisted"))
        {
            let executable = action
                .get("executable")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("(unnamed)")
                .to_string();
            *per_executable.entry(executable).or_default() += 1;
        }
    }
    let total: usize = per_executable.values().sum();
    (total > 0).then(|| Finding {
        detector: "denied_commands",
        count: total,
        says: format!(
            "{total} commands were refused by the allowlist: {}",
            per_executable
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ),
        detail: serde_json::json!({"per_executable": per_executable}),
    })
}

/// Turns that generated reasoning and produced no action.
///
/// Not simply "content was empty": a deployment that reasons and then answers
/// through a tool call has an empty content field on every successful turn, and
/// counting those reported sixty-six silent turns in a run that had none. What
/// makes a turn silent is that nothing came of it, so each turn is paired with
/// what the loop recorded next -- an action means it answered, a malformed call
/// means it did not.
fn thinking_only_turns(events: &[ExportedEvent]) -> Option<Finding> {
    let mut turns = Vec::new();
    for (index, event) in events.iter().enumerate() {
        let RunEvent::TurnGenerated {
            turn,
            thinking_chars,
            content_chars,
            metrics,
            ..
        } = &event.event
        else {
            continue;
        };
        if *content_chars > 0 || *thinking_chars == 0 {
            continue;
        }
        // What the loop did with this turn, before the next one began.
        let answered = events[index + 1..]
            .iter()
            .take_while(|later| !matches!(later.event, RunEvent::TurnGenerated { .. }))
            .any(|later| matches!(later.event, RunEvent::ToolAction { .. }));
        if answered {
            continue;
        }
        turns.push(serde_json::json!({
            "turn": turn,
            "thinking_chars": thinking_chars,
            "prompt_tokens": metrics.as_ref().and_then(|m| m.prompt_tokens),
        }));
    }
    (!turns.is_empty()).then(|| Finding {
        detector: "thinking_only_turns",
        count: turns.len(),
        says: format!(
            "{} turns produced reasoning and no action; check the prompt size on each",
            turns.len()
        ),
        detail: serde_json::json!({"turns": turns}),
    })
}

/// Generations that produced nothing the turn could use, and what they cost.
///
/// Recorded since D.E2E-22: twelve minutes of generation in a conversation had
/// left no trace, so the time went unexplained by every other detector.
fn failed_generations(events: &[ExportedEvent]) -> Option<Finding> {
    let mut by_outcome: std::collections::BTreeMap<String, usize> = Default::default();
    let mut elapsed_ms = 0u64;
    for event in events {
        if let RunEvent::GenerationFailed {
            outcome,
            elapsed_ms: spent,
            ..
        } = &event.event
        {
            *by_outcome.entry(outcome.clone()).or_default() += 1;
            elapsed_ms = elapsed_ms.saturating_add(*spent);
        }
    }
    let count: usize = by_outcome.values().sum();
    (count > 0).then(|| Finding {
        detector: "failed_generations",
        count,
        says: format!(
            "{count} generations produced nothing usable, {:.0} s in all: {}",
            elapsed_ms as f64 / 1000.0,
            by_outcome
                .iter()
                .map(|(outcome, n)| format!("{n} {outcome}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        detail: serde_json::json!({"by_outcome": by_outcome, "elapsed_ms": elapsed_ms}),
    })
}

/// How close the prompt came to the authorised context.
///
/// Reported as a finding only when it got close enough to matter. A run with
/// room to spare has nothing to say here, and saying it anyway would make the
/// report something people skim.
fn context_headroom(events: &[ExportedEvent]) -> Option<Finding> {
    let mut peak = 0u64;
    let mut authorised = None;
    for event in events {
        match &event.event {
            RunEvent::TurnGenerated { metrics, .. } => {
                if let Some(tokens) = metrics.as_ref().and_then(|m| m.prompt_tokens) {
                    peak = peak.max(tokens);
                }
            }
            RunEvent::RunStarted(payload) | RunEvent::ContextCompiled(payload) => {
                if let Some(tokens) = payload
                    .get("context_tokens")
                    .and_then(serde_json::Value::as_u64)
                {
                    authorised = Some(tokens);
                }
            }
            _ => {}
        }
    }
    let authorised = authorised?;
    if authorised == 0 || peak * 100 < authorised * 90 {
        return None;
    }
    Some(Finding {
        detector: "context_headroom",
        count: usize::try_from(peak * 100 / authorised).unwrap_or(100),
        says: format!(
            "the prompt reached {peak} tokens of {authorised} authorised, leaving little room to answer"
        ),
        detail: serde_json::json!({"peak_prompt_tokens": peak, "authorised": authorised}),
    })
}

/// A run told its checks were failing that never edited anything again.
///
/// The behaviour two of three Angular runs ended on, and the one the terminal
/// classes cannot currently distinguish: "stopped making progress" is also what
/// a run that broke nothing is called.
fn told_failing_never_repaired(events: &[ExportedEvent]) -> Option<Finding> {
    let mut last_failing_at: Option<u8> = None;
    let mut edits_after = 0usize;
    let mut reads_after = 0usize;
    for event in events {
        match &event.event {
            RunEvent::VerificationInterim { step, passing } => {
                if *passing {
                    last_failing_at = None;
                } else {
                    last_failing_at = Some(*step);
                }
                edits_after = 0;
                reads_after = 0;
            }
            RunEvent::ToolAction { action, .. } if last_failing_at.is_some() => {
                match action.get("capability").and_then(serde_json::Value::as_str) {
                    Some(
                        "write_file" | "replace_text" | "apply_replace" | "apply_patch"
                        | "delete_path" | "move_path" | "restore_file",
                    ) => edits_after += 1,
                    Some(_) => reads_after += 1,
                    None => {}
                }
            }
            _ => {}
        }
    }
    let step = last_failing_at?;
    (edits_after == 0).then(|| Finding {
        detector: "told_failing_never_repaired",
        count: reads_after,
        says: format!(
            "the run was last told its checks were failing at step {step} and never edited again, \
             taking {reads_after} further actions without one"
        ),
        detail: serde_json::json!({
            "last_failing_step": step,
            "actions_after_without_an_edit": reads_after,
        }),
    })
}

/// Actions taken after the workspace last changed.
fn stall_length(events: &[ExportedEvent]) -> Option<Finding> {
    let mut since_change = 0usize;
    let mut ever_changed = false;
    for event in events {
        if let RunEvent::ToolAction {
            action, outcome, ..
        } = &event.event
        {
            let changed = outcome
                .as_ref()
                .is_some_and(|outcome| outcome.get("new_hash").is_some());
            if changed {
                ever_changed = true;
                since_change = 0;
            } else if action.get("capability").is_some() {
                since_change += 1;
            }
        }
    }
    (ever_changed && since_change >= 6).then(|| Finding {
        detector: "stall_length",
        count: since_change,
        says: format!("the run took {since_change} actions after the last change to the workspace"),
        detail: serde_json::json!({"actions_since_last_change": since_change}),
    })
}
