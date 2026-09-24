//! One action through the boundary both loops share.
//!
//! R1's question is whether the conversation and the scripted run observe the
//! same actions, evidence and outcomes. They were two state machines that each
//! decided, for every action, whether it was a repeat of something refused,
//! whether it needed a person's approval, how it was announced and receipted,
//! and what the run had already been shown. Each of those decisions lives here
//! now, once, and both loops call it; what stays in a loop is what the loop is
//! for -- a plan and verification for the run, a person and a check-in for the
//! conversation -- and `two_loops` names each of those as declared.

use crate::conversation;
use crate::repetition::{RefusalStreak, repetition_notice};
use crate::{ActionExecutionError, ApprovalDecision, ApprovalPrompt, ReadHistory};
use pwr_domain::ChatMessage;
use pwr_store::Store;
use pwr_tools::{ActionProposal, Approval, ToolPolicy};

/// Whether an action may run, decided before it does.
#[derive(Debug, Clone, PartialEq)]
pub enum Gate {
    /// Run it. A grant for this action alone is returned so the caller can
    /// revoke it once the action has had its effect.
    Proceed { granted_once: Option<Approval> },
    /// Do not run it; this is what the deployment is told instead.
    Refused(ChatMessage),
}

/// Repetition and approval, in that order, for one proposed action.
///
/// A repeat of a refused action is refused without asking anyone: asking a
/// person to approve the same stale call again is the loop, not a way out of it.
/// An action needing an approval the policy does not hold is put to `prompt`;
/// a refusal is recorded as a denied action and counts toward the streak, and a
/// grant is added to `policy` -- for this action only when that is what was
/// given.
#[allow(clippy::too_many_arguments)]
pub async fn gate(
    store: &Store,
    id: pwr_domain::Id,
    step: u8,
    action: &ActionProposal,
    fingerprint: &str,
    policy: &mut ToolPolicy,
    prompt: &dyn ApprovalPrompt,
    refused: &mut RefusalStreak,
    call_id: Option<String>,
) -> Result<Gate, String> {
    if refused.would_only_repeat(fingerprint) {
        store
            .append_event(
                Some(id),
                &pwr_domain::RunEvent::LoopDetected {
                    step,
                    action: fingerprint.to_owned(),
                },
            )
            .map_err(|e| e.to_string())?;
        return Ok(Gate::Refused(ChatMessage {
            role: "tool".into(),
            content: repetition_notice(),
            tool_call_id: call_id,
            ..Default::default()
        }));
    }
    let Some((approval, description)) = pwr_tools::required_approval(action)
        .filter(|(approval, _)| !policy.approvals.contains(approval))
    else {
        return Ok(Gate::Proceed { granted_once: None });
    };
    let decision = prompt.ask(approval, &description).await;
    store
        .append(
            Some(id),
            "approval.decision",
            serde_json::json!({
                "approval": approval,
                "description": description,
                "decision": decision,
                "step": step,
            }),
        )
        .map_err(|e| e.to_string())?;
    match decision {
        ApprovalDecision::Deny => {
            // Enforced here rather than left to the tool to notice: a tool that
            // forgot to re-check would make the gate advisory. Still a tool
            // result -- ending the run would discard work already done.
            let denial = format!("a person refused this: {description}");
            store
                .append_event(
                    Some(id),
                    &pwr_domain::RunEvent::ToolAction {
                        action: serde_json::to_value(action).unwrap_or_default(),
                        status: pwr_domain::ToolActionStatus::Denied,
                        outcome_class: "policy_denial".into(),
                        outcome: None,
                        denial: Some(denial.clone()),
                        failure: None,
                        failure_category: None,
                    },
                )
                .map_err(|e| e.to_string())?;
            refused.refused(fingerprint);
            Ok(Gate::Refused(ChatMessage {
                role: "tool".into(),
                content: serde_json::json!({"denied": denial}).to_string(),
                tool_call_id: call_id,
                ..Default::default()
            }))
        }
        ApprovalDecision::AllowOnce => {
            policy.approvals.push(approval);
            Ok(Gate::Proceed {
                granted_once: Some(approval),
            })
        }
        ApprovalDecision::AllowForRun => {
            policy.approvals.push(approval);
            Ok(Gate::Proceed { granted_once: None })
        }
    }
}

/// Runs an action the gate let through, with the trail a restart needs.
///
/// Announced before it runs when it can change the workspace and receipted
/// after, so a restart that finds the announcement without the receipt knows
/// the effect is uncertain rather than absent. Executed against `reads`, so a
/// re-read of a file the session has already been shown says so. And followed
/// by a checkpoint of what the session has changed, after every action.
#[allow(clippy::too_many_arguments)]
pub async fn perform(
    store: &Store,
    id: pwr_domain::Id,
    policy: &ToolPolicy,
    action: ActionProposal,
    services: &mut pwr_tools::service::ServiceSupervisor,
    reads: &mut ReadHistory,
    step: u8,
    checkpoint: &mut conversation::Checkpoint,
    actions_taken: usize,
) -> Result<Result<serde_json::Value, ActionExecutionError>, String> {
    let intent = conversation::may_change_workspace(&action).then(|| {
        checkpoint.next_intent += 1;
        conversation::intent_for(&action, checkpoint.next_intent)
    });
    if let Some(intent) = &intent {
        conversation::record_intent(store, id, intent)?;
    }
    let outcome =
        crate::execute_action_recorded(store, id, policy, action, services, reads, step).await;
    if let Some(intent) = &intent {
        conversation::record_receipt(store, id, intent.sequence)?;
    }
    if let Ok(value) = &outcome {
        crate::stall::record_effect(&mut checkpoint.changed_files, value);
    }
    checkpoint.actions = actions_taken;
    conversation::record_checkpoint(store, id, checkpoint)?;
    Ok(outcome)
}
