//! The two controls PWR is measured against.
//!
//! `docs/evaluation.md` names three baselines. B1 is PWR as it stands:
//! `run_action_loop_with_prompt_budget_and_context_tiers`. B0 and B2 are here.
//!
//! Both are built from the parts B1 is built from -- the action protocol and
//! its parser, the family adapter, the reply faults, the policy-enforcing
//! executor, the result envelope and the event log the evaluator scores from.
//! What differs is only the loop. A difference between arms is therefore a
//! difference in how the work is driven, and not in what an action may do or
//! how its outcome is recorded; a control that enforced less policy or parsed
//! replies worse would make B1 look better for reasons that are not the
//! harness.
//!
//! What each arm honours from `RunTuning`: both take the malformed-call limit,
//! the turn timeout, the host probe and the adapter. B2 also takes
//! `known_failures`. Neither plans, compacts by summary, detects loops or
//! recovers at a lower context tier -- those are B1's mechanisms, and a control
//! that had them would not be a control.

use crate::run_state::bounded_result;
use crate::{
    CHARS_PER_TOKEN, ReadHistory, ReplyFault, RunTuning, TURNS_PER_ACTION, TaskRunResult,
    TaskState, action_outcome, actions_from_reply, estimated_tokens, execute_action_recorded,
    persist_failure, persist_failure_as, persist_transition, prompt_delivery, tool_result_message,
};
use pwr_domain::{ChatMessage, ModelRequest, RunEvent, ToolActionStatus};
use pwr_provider::ModelProvider;
use pwr_store::Store;
use pwr_tools::{ActionProposal, ToolPolicy};

/// Share of the context window the transcript may fill before B0 drops its
/// oldest exchanges. The rest is the reply's.
const TRANSCRIPT_SHARE: usize = 3;
const TRANSCRIPT_OF: usize = 4;

/// Largest serialized tool result B0 and B2 pass back, as a share of the window.
const RESULT_SHARE_OF_WINDOW: usize = 8;

/// How many times B2 validates before it stops.
pub const STAGED_VALIDATION_ROUNDS: u8 = 3;

/// How many files B2's localization may name.
pub const STAGED_LOCALIZED_FILES: usize = 5;

/// What one turn produced.
enum Turn {
    /// Actions to perform, and the call id the first result answers.
    Actions(Vec<ActionProposal>, Option<String>),
    /// Nothing usable; the deployment has been told and may try again.
    Retry,
}

/// One request and its reply, recorded exactly as B1 records a turn.
///
/// A fault the deployment can correct is told to it and retried within the
/// malformed-call limit; anything else ends the run with the terminal B1 would
/// record. The protocol is not where a control should be weaker.
#[allow(clippy::too_many_arguments)]
async fn take_turn<P: ModelProvider>(
    store: &Store,
    provider: &P,
    run_id: pwr_domain::Id,
    request: &mut ModelRequest,
    tuning: &RunTuning,
    step: u8,
    turns: u32,
    malformed: &mut usize,
    state: &mut TaskState,
) -> Result<Turn, String> {
    let cancel = pwr_provider::Cancel::new();
    let opened = provider
        .chat_cancellable(request.clone(), cancel.clone())
        .await;
    let collected = match opened {
        Ok(stream) => match tuning.turn_timeout {
            Some(limit) => {
                match tokio::time::timeout(limit, pwr_provider::collect_reply(stream)).await {
                    Ok(collected) => collected,
                    Err(_) => {
                        cancel.cancel();
                        Err(pwr_provider::ProviderError::Timeout {
                            safe_context: format!("turn exceeded {} seconds", limit.as_secs()),
                        })
                    }
                }
            }
            None => pwr_provider::collect_reply(stream).await,
        },
        Err(error) => Err(error),
    };
    let reply = match collected {
        Ok(reply) => reply,
        Err(ref error) if ReplyFault::of(error).is_some() => {
            let fault = ReplyFault::of(error).expect("guarded above");
            store
                .append_event(
                    Some(run_id),
                    &RunEvent::ActionMalformed {
                        step,
                        problem: fault.detail().to_owned(),
                        kind: fault.kind().into(),
                        detail: None,
                    },
                )
                .map_err(|e| e.to_string())?;
            *malformed += 1;
            if *malformed > tuning.malformed_call_limit {
                persist_failure(
                    store,
                    run_id,
                    state,
                    "replies the turn could not use",
                    serde_json::json!({"problem": fault.detail(), "kind": fault.kind()}),
                )?;
                return Err(format!(
                    "{} unusable replies in a row: {}",
                    tuning.malformed_call_limit,
                    fault.detail()
                ));
            }
            request.messages.push(ChatMessage {
                role: "tool".into(),
                content: serde_json::json!({fault.kind(): fault.told()}).to_string(),
                ..Default::default()
            });
            return Ok(Turn::Retry);
        }
        Err(error) => {
            persist_failure(
                store,
                run_id,
                state,
                "provider request failed",
                serde_json::json!({"error": error.to_string()}),
            )?;
            return Err(error.to_string());
        }
    };
    let reply = tuning.adapter.normalize(&reply);
    if let Some(host) = &tuning.host {
        let pressure = host.memory_pressure().await;
        store
            .append_event(
                Some(run_id),
                &RunEvent::ResourceSampled {
                    step,
                    turn: turns,
                    pressure,
                },
            )
            .map_err(|e| e.to_string())?;
    }
    let delivery = prompt_delivery(
        estimated_tokens(&request.messages),
        request.context_tokens,
        reply.metrics.as_ref(),
    );
    store
        .append_event(
            Some(run_id),
            &RunEvent::TurnGenerated {
                step,
                turn: turns,
                metrics: reply.metrics.clone(),
                tokens_per_second: reply
                    .metrics
                    .as_ref()
                    .and_then(pwr_domain::GenerationMetrics::tokens_per_second),
                thinking_chars: reply.thinking.len(),
                content_chars: reply.narrative.len(),
                prompt_delivery: delivery,
                normalizations: reply
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!("{}: {}", diagnostic.kind, diagnostic.detail))
                    .collect(),
            },
        )
        .map_err(|e| e.to_string())?;
    request.messages.push(ChatMessage {
        role: "assistant".into(),
        content: reply.narrative.clone(),
        tool_calls: reply.tool_calls.clone(),
        tool_call_id: None,
        purpose: None,
        images: Vec::new(),
    });
    let answering = reply.tool_calls.first().and_then(|call| call.id.clone());
    match actions_from_reply(&reply) {
        Ok(actions) => {
            *malformed = 0;
            Ok(Turn::Actions(actions, answering))
        }
        Err(problem) => {
            store
                .append_event(
                    Some(run_id),
                    &RunEvent::ActionMalformed {
                        step,
                        problem: problem.problem.clone(),
                        kind: problem.kind.into(),
                        detail: problem.detail.clone(),
                    },
                )
                .map_err(|e| e.to_string())?;
            *malformed += 1;
            if *malformed > tuning.malformed_call_limit {
                persist_failure(
                    store,
                    run_id,
                    state,
                    "repeatedly malformed tool calls",
                    serde_json::json!({"problem": problem.problem, "kind": problem.kind}),
                )?;
                return Err(format!(
                    "{} malformed tool calls in a row: {problem}",
                    tuning.malformed_call_limit
                ));
            }
            request.messages.push(ChatMessage {
                role: "tool".into(),
                content: serde_json::json!({
                    "rejected": problem.problem,
                    "hint": "Your tool call did not match the schema you were given. Check the \
                             required arguments and their types, then call again.",
                })
                .to_string(),
                tool_call_id: answering,
                ..Default::default()
            });
            Ok(Turn::Retry)
        }
    }
}

/// Drops the oldest exchanges until the transcript fits its share of the window.
///
/// The conventional bound: no summary, no ledger, no notion of which evidence
/// still matters. The system prompt and the task -- the first two messages --
/// are kept, and a tool result is never left behind without the call it
/// answers. Returns how many messages went.
pub fn drop_oldest_exchanges(messages: &mut Vec<ChatMessage>, context_tokens: u32) -> usize {
    let budget = context_tokens as usize * TRANSCRIPT_SHARE / TRANSCRIPT_OF;
    let keep = messages.len().min(2);
    let mut dropped = 0;
    while estimated_tokens(messages) > budget && messages.len() > keep + 1 {
        messages.remove(keep);
        dropped += 1;
        while messages.len() > keep + 1 && messages[keep].role == "tool" {
            messages.remove(keep);
            dropped += 1;
        }
    }
    dropped
}

fn record_completion(
    store: &Store,
    run_id: pwr_domain::Id,
    action: &ActionProposal,
    step: u8,
) -> Result<(), String> {
    store
        .append_event(
            Some(run_id),
            &RunEvent::ToolAction {
                action: serde_json::to_value(action).unwrap_or_default(),
                status: ToolActionStatus::Allowed,
                outcome_class: "allowed_success".into(),
                outcome: Some(serde_json::json!({"declared": true, "step": step})),
                denial: None,
                failure: None,
                failure_category: None,
            },
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A refusal, recorded as B1 records one.
fn decline(
    store: &Store,
    run_id: pwr_domain::Id,
    action: &ActionProposal,
    rationale: &str,
    step: u8,
    state: &mut TaskState,
) -> Result<TaskRunResult, String> {
    store
        .append_event(
            Some(run_id),
            &RunEvent::ToolAction {
                action: serde_json::to_value(action).unwrap_or_default(),
                status: ToolActionStatus::Allowed,
                outcome_class: "allowed_success".into(),
                outcome: Some(serde_json::json!({"declined": true, "step": step})),
                denial: None,
                failure: None,
                failure_category: None,
            },
        )
        .map_err(|e| e.to_string())?;
    store
        .append_event(
            Some(run_id),
            &RunEvent::TaskDeclined {
                rationale: rationale.to_owned(),
            },
        )
        .map_err(|e| e.to_string())?;
    persist_failure_as(
        store,
        run_id,
        state,
        "deployment declined the task",
        pwr_domain::TerminalClass::Declined,
        serde_json::json!({"rationale": rationale}),
    )?;
    Ok(TaskRunResult {
        verifiable: false,
        run_id,
        verified: false,
        action_outcome: serde_json::json!({"declined": true, "rationale": rationale}),
    })
}

fn result_limit(request: &ModelRequest) -> usize {
    (request.context_tokens as usize / RESULT_SHARE_OF_WINDOW * CHARS_PER_TOKEN).max(1024)
}

/// B0: a conventional agent loop.
///
/// The deployment is given the request it was composed with, calls tools one
/// reply at a time, sees each result, and is finished when it says so. The
/// transcript is bounded by dropping its oldest exchanges. Nothing checks the
/// work before the run ends: acceptance is the evaluator's hidden verifier,
/// which is the independent check every arm is scored by.
///
/// `verified` in the result is this arm's acceptance of its own completion,
/// which here is the declaration itself -- the evaluator reads it as "the arm
/// finished", and B1's is its checks passing. `verifiable` is false because no
/// check ran.
pub async fn run_conventional_loop<P: ModelProvider>(
    store: &Store,
    provider: &P,
    run_id: pwr_domain::Id,
    mut request: ModelRequest,
    policy: &ToolPolicy,
    max_actions: u8,
    tuning: &RunTuning,
) -> Result<TaskRunResult, String> {
    store
        .append(
            Some(run_id),
            "baseline.arm",
            serde_json::json!({"arm": "b0_conventional", "max_actions": max_actions}),
        )
        .map_err(|e| e.to_string())?;
    let mut state = persist_transition(
        store,
        run_id,
        TaskState::Plan,
        TaskState::Act,
        "conventional loop entered (B0)",
    )?;
    // The window, asked for as B1 asks for it. On a backend whose window is
    // fixed at load time the controls never asked: measured in the R2 pilot of
    // 2026-09-14, LM Studio served B0 and B2 at 8,192 tokens while their
    // reports recorded the 16,384 B1 was given, and B2 lost eight trials to
    // `exceed_context_size`. A control run in half the window is weaker for a
    // reason that is not the harness.
    crate::prepare_context_tier(store, run_id, provider, &mut request).await?;
    let mut services = pwr_tools::service::ServiceSupervisor::new();
    let limit = result_limit(&request);
    let mut step: u8 = 0;
    let mut turns: u32 = 0;
    let mut malformed = 0usize;
    while step < max_actions {
        turns += 1;
        if turns > u32::from(max_actions) * TURNS_PER_ACTION {
            persist_failure(
                store,
                run_id,
                &mut state,
                "turn ceiling reached",
                serde_json::json!({"actions_used": step, "turns": turns}),
            )?;
            return Err(format!(
                "{turns} turns produced only {step} actions; the deployment is not emitting usable calls"
            ));
        }
        let dropped = drop_oldest_exchanges(&mut request.messages, request.context_tokens);
        if dropped > 0 {
            store
                .append(
                    Some(run_id),
                    "context.compacted",
                    serde_json::json!({"strategy": "drop_oldest", "dropped": dropped, "step": step}),
                )
                .map_err(|e| e.to_string())?;
        }
        let (actions, answering) = match take_turn(
            store,
            provider,
            run_id,
            &mut request,
            tuning,
            step,
            turns,
            &mut malformed,
            &mut state,
        )
        .await?
        {
            Turn::Retry => continue,
            Turn::Actions(actions, answering) => (actions, answering),
        };
        for (index, action) in actions.into_iter().enumerate() {
            if step >= max_actions {
                break;
            }
            match &action {
                ActionProposal::Complete { rationale } => {
                    record_completion(store, run_id, &action, step)?;
                    // Through Verify, as every completion is, with nothing in it:
                    // the log's state machine is the same for all three arms.
                    let verifying = persist_transition(
                        store,
                        run_id,
                        state,
                        TaskState::Verify,
                        "deployment declared completion; B0 runs no verification",
                    )?;
                    persist_transition(
                        store,
                        run_id,
                        verifying,
                        TaskState::Complete,
                        "accepted on the declaration (B0)",
                    )?;
                    store
                        .append_event(
                            Some(run_id),
                            &RunEvent::TaskComplete {
                                step,
                                verified: false,
                            },
                        )
                        .map_err(|e| e.to_string())?;
                    return Ok(TaskRunResult {
                        verifiable: false,
                        run_id,
                        verified: true,
                        action_outcome: serde_json::json!({
                            "declared": true,
                            "arm": "b0_conventional",
                            "rationale": rationale,
                        }),
                    });
                }
                ActionProposal::Decline { rationale } => {
                    return decline(store, run_id, &action, rationale, step, &mut state);
                }
                _ => {
                    let outcome = execute_action_recorded(
                        store,
                        run_id,
                        policy,
                        action,
                        &mut services,
                        &mut ReadHistory::default(),
                        step,
                    )
                    .await;
                    let outcome = bounded_result(action_outcome(outcome), limit);
                    request.messages.push(tool_result_message(
                        outcome,
                        None,
                        if index == 0 { answering.clone() } else { None },
                    ));
                    step = step.saturating_add(1);
                }
            }
        }
    }
    persist_failure(
        store,
        run_id,
        &mut state,
        "action budget exhausted",
        serde_json::json!({"max_actions": max_actions}),
    )?;
    Err(format!(
        "action budget of {max_actions} exhausted before completion"
    ))
}

/// The instruction that opens B2's localization stage.
pub const LOCALIZE_INSTRUCTION: &str = "Stage 1 of 3, localization. Do not edit anything \
    yet. You may read and search the repository. When you know where the change belongs, \
    call complete with a rationale that is only a JSON array of the repository-relative \
    paths of the files that must change, most important first, at most five. For a \
    question that changes no file, list the files that contain the answer.";

/// The instruction that opens B2's repair stage.
pub const REPAIR_INSTRUCTION: &str = "Stage 2 of 3, repair. The files you localized are \
    below. Make the change now with the editing tools. Do not run tests or commands: \
    validation is stage 3 and runs for you when you call complete.";

/// The instruction that opens B2's answer stage for a question.
pub const ANSWER_INSTRUCTION: &str = "Stage 2 of 2, answer. The files you localized are \
    below. Do not change any file. Read what you still need, then call complete with \
    your answer as the rationale.";

/// Paths named by a localization rationale: a JSON array, or failing that
/// every line or comma-separated token that looks like a path.
///
/// Lenient on purpose. The stage's product is a set of files, and a
/// deployment that wrote them as a bulleted list has localized as well as one
/// that wrote JSON; the files are then checked against the workspace, which is
/// the part that must not be lenient.
pub fn localized_paths(rationale: &str) -> Vec<String> {
    let trimmed = rationale.trim();
    let from_json = trimmed
        .find('[')
        .zip(trimmed.rfind(']'))
        .and_then(|(start, end)| serde_json::from_str::<Vec<String>>(&trimmed[start..=end]).ok());
    let candidates: Vec<String> = match from_json {
        Some(paths) => paths,
        None => trimmed
            .split(|c: char| c == '\n' || c == ',' || c.is_whitespace())
            .map(|token| {
                token
                    .trim_matches(|c: char| "-*`'\"()[]:;".contains(c))
                    .to_string()
            })
            .filter(|token| token.contains('.') || token.contains('/'))
            .collect(),
    };
    let mut paths = Vec::new();
    for candidate in candidates {
        let path = candidate.trim().trim_start_matches("./").to_string();
        if !path.is_empty() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths.truncate(STAGED_LOCALIZED_FILES);
    paths
}

/// Whether an action may run in a stage. Reads and searches are always
/// allowed; edits only in repair; commands never, since validation is the
/// harness's stage.
fn permitted(stage: Stage, action: &ActionProposal) -> Result<(), &'static str> {
    let reading = matches!(
        action,
        ActionProposal::ReadFile { .. }
            | ActionProposal::Search { .. }
            | ActionProposal::ListTree { .. }
            | ActionProposal::FindDefinition { .. }
            | ActionProposal::ExtractDocument { .. }
            | ActionProposal::VcsStatus {}
            | ActionProposal::VcsDiff { .. }
            | ActionProposal::RecordProgress { .. }
    );
    if reading {
        return Ok(());
    }
    let editing = matches!(
        action,
        ActionProposal::ApplyReplace { .. }
            | ActionProposal::ApplyPatchHunks { .. }
            | ActionProposal::ReplaceText { .. }
            | ActionProposal::WriteFile { .. }
            | ActionProposal::MakeDirectory { .. }
            | ActionProposal::DeletePath { .. }
            | ActionProposal::MovePath { .. }
            | ActionProposal::RestoreFile { .. }
    );
    match stage {
        Stage::Repair if editing => Ok(()),
        Stage::Localize | Stage::Answer if editing => {
            Err("this stage does not edit; call complete to move on")
        }
        _ => Err("this staged workflow runs no commands; validation runs after complete"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Localize,
    Repair,
    Answer,
}

/// The localized files' contents, bounded, as the repair stage sees them.
fn localized_context(policy: &ToolPolicy, paths: &[String], limit: usize) -> String {
    let per_file = (limit / paths.len().max(1)).max(512);
    let mut shown = Vec::new();
    for path in paths {
        let text = match pwr_tools::read_file_window(policy, std::path::Path::new(path), None, None)
        {
            Ok(window) => serde_json::to_value(window).unwrap_or_default(),
            Err(error) => serde_json::json!({"unreadable": error.to_string()}),
        };
        shown.push(serde_json::json!({"path": path, "file": bounded_result(text, per_file)}));
    }
    serde_json::Value::Array(shown).to_string()
}

/// B2: a strong, simple, fixed staged policy.
///
/// Localization, then repair, then validation, as Agentless stages them: the
/// deployment first names the files the change belongs in, reading but not
/// editing; it then edits those files, shown to it whole, without running
/// anything; and the harness then runs the supplied checks. A failing check is
/// shown back and repair resumes, up to `STAGED_VALIDATION_ROUNDS`. A question
/// localizes and then answers, and is accepted only if nothing changed.
///
/// One representation for every deployment. `docs/evaluation.md` asks for the
/// development-selected best representation per deployment; that selection has
/// not been made, and this arm is the staged workflow at the default one.
#[allow(clippy::too_many_arguments)]
pub async fn run_staged_loop<P: ModelProvider>(
    store: &Store,
    provider: &P,
    run_id: pwr_domain::Id,
    mut request: ModelRequest,
    policy: &ToolPolicy,
    checks: &[(String, Vec<String>)],
    max_actions: u8,
    tuning: &RunTuning,
) -> Result<TaskRunResult, String> {
    let question = tuning.preserve_baseline;
    store
        .append(
            Some(run_id),
            "baseline.arm",
            serde_json::json!({
                "arm": "b2_staged",
                "max_actions": max_actions,
                "validation_rounds": STAGED_VALIDATION_ROUNDS,
                "question": question,
            }),
        )
        .map_err(|e| e.to_string())?;
    let mut state = persist_transition(
        store,
        run_id,
        TaskState::Plan,
        TaskState::Act,
        "staged workflow entered (B2): localization",
    )?;
    // See the same call in `run_conventional_loop`.
    crate::prepare_context_tier(store, run_id, provider, &mut request).await?;
    let before = pwr_verify::baseline(policy, checks)
        .await
        .map_err(|e| e.to_string())?;
    store
        .append(
            Some(run_id),
            "verification.baseline",
            serde_json::to_value(&before).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    let passing_at_start: Vec<bool> = before
        .checks
        .iter()
        .map(|record| record.result.exit_code == Some(0))
        .collect();
    let unrunnable_at_start: Vec<bool> = before
        .checks
        .iter()
        .map(|record| {
            record.result.exit_code != Some(0)
                && pwr_verify::classify(&record.result) == pwr_verify::FailureClass::Environment
        })
        .collect();
    let start_hash = question
        .then(|| crate::repository_content_hash(&policy.root))
        .transpose()?;
    let mut services = pwr_tools::service::ServiceSupervisor::new();
    let limit = result_limit(&request);
    request
        .messages
        .push(ChatMessage::text("user", LOCALIZE_INSTRUCTION));
    let mut stage = Stage::Localize;
    let mut rounds: u8 = 0;
    let mut step: u8 = 0;
    let mut turns: u32 = 0;
    let mut malformed = 0usize;
    while step < max_actions {
        turns += 1;
        if turns > u32::from(max_actions) * TURNS_PER_ACTION {
            persist_failure(
                store,
                run_id,
                &mut state,
                "turn ceiling reached",
                serde_json::json!({"actions_used": step, "turns": turns}),
            )?;
            return Err(format!(
                "{turns} turns produced only {step} actions; the deployment is not emitting usable calls"
            ));
        }
        let (actions, answering) = match take_turn(
            store,
            provider,
            run_id,
            &mut request,
            tuning,
            step,
            turns,
            &mut malformed,
            &mut state,
        )
        .await?
        {
            Turn::Retry => continue,
            Turn::Actions(actions, answering) => (actions, answering),
        };
        for (index, action) in actions.into_iter().enumerate() {
            if step >= max_actions {
                break;
            }
            let answering = if index == 0 { answering.clone() } else { None };
            step = step.saturating_add(1);
            match &action {
                ActionProposal::Decline { rationale } => {
                    return decline(store, run_id, &action, rationale, step, &mut state);
                }
                ActionProposal::Complete { rationale } if stage == Stage::Localize => {
                    let named = localized_paths(rationale);
                    let existing: Vec<String> = named
                        .iter()
                        .filter(|path| policy.root.join(path).is_file())
                        .cloned()
                        .collect();
                    store
                        .append(
                            Some(run_id),
                            "staged.localized",
                            serde_json::json!({"named": named, "existing": existing, "step": step}),
                        )
                        .map_err(|e| e.to_string())?;
                    if existing.is_empty() {
                        request.messages.push(tool_result_message(
                            serde_json::json!({
                                "localization_rejected": "none of the paths you named is a file in \
                                    this repository; read or search, then call complete again with \
                                    a JSON array of existing paths",
                                "named": named,
                            }),
                            None,
                            answering,
                        ));
                        continue;
                    }
                    stage = if question {
                        Stage::Answer
                    } else {
                        Stage::Repair
                    };
                    request.messages.push(tool_result_message(
                        serde_json::json!({"localized": existing}),
                        None,
                        answering,
                    ));
                    request.messages.push(ChatMessage::text(
                        "user",
                        format!(
                            "{}\n{}",
                            if question {
                                ANSWER_INSTRUCTION
                            } else {
                                REPAIR_INSTRUCTION
                            },
                            localized_context(policy, &existing, limit * 2)
                        ),
                    ));
                }
                ActionProposal::Complete { rationale } => {
                    record_completion(store, run_id, &action, step)?;
                    if question {
                        let unchanged = start_hash.as_ref()
                            == Some(&crate::repository_content_hash(&policy.root)?);
                        store
                            .append(
                                Some(run_id),
                                "verification.read_only",
                                serde_json::json!({"source_unchanged": unchanged}),
                            )
                            .map_err(|e| e.to_string())?;
                        if unchanged {
                            return staged_complete(store, run_id, &mut state, step, rationale);
                        }
                        persist_failure(
                            store,
                            run_id,
                            &mut state,
                            "verification failed and recovery budget exhausted",
                            serde_json::json!({"reason": "a question changed the workspace"}),
                        )?;
                        return Err("a question changed the workspace".into());
                    }
                    rounds += 1;
                    state = persist_transition(
                        store,
                        run_id,
                        state,
                        TaskState::Verify,
                        format!("validation round {rounds} of {STAGED_VALIDATION_ROUNDS}"),
                    )?;
                    let after = pwr_verify::baseline(policy, checks)
                        .await
                        .map_err(|e| e.to_string())?;
                    let failing: Vec<&pwr_verify::CheckRecord> = after
                        .checks
                        .iter()
                        .enumerate()
                        .filter(|(i, record)| {
                            record.result.exit_code != Some(0)
                                && !unrunnable_at_start.get(*i).copied().unwrap_or(false)
                                && !tuning.known_failures.iter().any(|(cmd, args)| {
                                    format!("{} {}", cmd, args.join(" ")).trim_end()
                                        == record.command
                                })
                        })
                        .map(|(_, record)| record)
                        .collect();
                    let runnable = unrunnable_at_start.iter().filter(|u| !**u).count();
                    store
                        .append(
                            Some(run_id),
                            "staged.validated",
                            serde_json::json!({
                                "round": rounds,
                                "failing": failing.iter().map(|r| &r.command).collect::<Vec<_>>(),
                                "passing_at_start": passing_at_start,
                            }),
                        )
                        .map_err(|e| e.to_string())?;
                    if failing.is_empty() && runnable > 0 {
                        return staged_complete(store, run_id, &mut state, step, rationale);
                    }
                    if runnable == 0 {
                        // Nothing could check it, which is B1's honest ending
                        // too: completed and not verified.
                        return staged_complete(store, run_id, &mut state, step, rationale);
                    }
                    if rounds >= STAGED_VALIDATION_ROUNDS {
                        persist_failure(
                            store,
                            run_id,
                            &mut state,
                            "verification failed and recovery budget exhausted",
                            serde_json::json!({"rounds": rounds}),
                        )?;
                        return Err(format!(
                            "validation failed in {rounds} rounds; the checks still fail"
                        ));
                    }
                    let recovering = persist_transition(
                        store,
                        run_id,
                        state,
                        TaskState::Recover,
                        "validation failed",
                    )?;
                    state = persist_transition(
                        store,
                        run_id,
                        recovering,
                        TaskState::Act,
                        "back to repair",
                    )?;
                    let shown: Vec<serde_json::Value> = failing
                        .iter()
                        .map(|record| {
                            bounded_result(
                                serde_json::json!({
                                    "command": record.command,
                                    "exit_code": record.result.exit_code,
                                    "stdout": record.result.stdout,
                                    "stderr": record.result.stderr,
                                }),
                                limit / failing.len().max(1),
                            )
                        })
                        .collect();
                    request.messages.push(tool_result_message(
                        serde_json::json!({
                            "validation_failed": shown,
                            "round": rounds,
                            "rounds_left": STAGED_VALIDATION_ROUNDS - rounds,
                        }),
                        None,
                        answering,
                    ));
                }
                _ => {
                    if let Err(reason) = permitted(stage, &action) {
                        store
                            .append_event(
                                Some(run_id),
                                &RunEvent::ToolAction {
                                    action: serde_json::to_value(&action).unwrap_or_default(),
                                    status: ToolActionStatus::Denied,
                                    outcome_class: "denied".into(),
                                    outcome: None,
                                    denial: Some(reason.to_string()),
                                    failure: None,
                                    failure_category: None,
                                },
                            )
                            .map_err(|e| e.to_string())?;
                        request.messages.push(tool_result_message(
                            serde_json::json!({"denied": reason}),
                            None,
                            answering,
                        ));
                        continue;
                    }
                    let outcome = execute_action_recorded(
                        store,
                        run_id,
                        policy,
                        action,
                        &mut services,
                        &mut ReadHistory::default(),
                        step,
                    )
                    .await;
                    let outcome = bounded_result(action_outcome(outcome), limit);
                    request
                        .messages
                        .push(tool_result_message(outcome, None, answering));
                }
            }
        }
    }
    persist_failure(
        store,
        run_id,
        &mut state,
        "action budget exhausted",
        serde_json::json!({"max_actions": max_actions, "stage": format!("{stage:?}")}),
    )?;
    Err(format!(
        "action budget of {max_actions} exhausted before completion"
    ))
}

fn staged_complete(
    store: &Store,
    run_id: pwr_domain::Id,
    state: &mut TaskState,
    step: u8,
    rationale: &str,
) -> Result<TaskRunResult, String> {
    if *state == TaskState::Act {
        *state = persist_transition(
            store,
            run_id,
            state.clone(),
            TaskState::Verify,
            "answer declared; the workspace is unchanged",
        )?;
    }
    *state = persist_transition(
        store,
        run_id,
        state.clone(),
        TaskState::Complete,
        "staged workflow accepted its completion (B2)",
    )?;
    store
        .append_event(
            Some(run_id),
            &RunEvent::TaskComplete {
                step,
                verified: true,
            },
        )
        .map_err(|e| e.to_string())?;
    Ok(TaskRunResult {
        verifiable: true,
        run_id,
        verified: true,
        action_outcome: serde_json::json!({
            "declared": true,
            "arm": "b2_staged",
            "rationale": rationale,
        }),
    })
}
