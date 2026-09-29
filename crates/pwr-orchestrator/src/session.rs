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
    /// Run it. Grants for this action alone are returned so the caller can
    /// revoke them once the action has had its effect.
    Proceed { granted_once: Vec<Approval> },
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
    // A program outside the workspace's list is asked about first, then what
    // the action itself reaches (a push, the network, a manifest): `git push`
    // in a workspace that does not list git needs both answers.
    let mut needed: Vec<(Approval, String)> = Vec::new();
    for requirement in [
        pwr_tools::unlisted_program(action, policy),
        pwr_tools::required_approval(action),
        pwr_tools::names_a_url(action, policy),
        pwr_tools::drives_containers(action, policy),
        pwr_tools::leaves_workspace(action, policy),
    ]
    .into_iter()
    .flatten()
    {
        if !policy.approvals.contains(&requirement.0)
            && !needed
                .iter()
                .any(|(approval, _)| *approval == requirement.0)
        {
            needed.push(requirement);
        }
    }
    let mut granted_once = Vec::new();
    for (approval, description) in needed {
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
                // What was granted a moment ago for this same action goes with it.
                policy
                    .approvals
                    .retain(|granted| !granted_once.contains(granted));
                // Enforced here rather than left to the tool to notice: a tool
                // that forgot to re-check would make the gate advisory. Still a
                // tool result -- ending the run would discard work already done.
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
                return Ok(Gate::Refused(ChatMessage {
                    role: "tool".into(),
                    content: serde_json::json!({"denied": denial}).to_string(),
                    tool_call_id: call_id,
                    ..Default::default()
                }));
            }
            ApprovalDecision::AllowOnce => {
                policy.approvals.push(approval);
                granted_once.push(approval);
            }
            ApprovalDecision::AllowForRun => policy.approvals.push(approval),
        }
    }
    Ok(Gate::Proceed { granted_once })
}

/// What a command the sandbox kept from something it needed is owed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Withheld {
    /// It did not fail for want of something the sandbox withholds, or
    /// asking would not change it: nothing to ask.
    Nothing,
    /// The person granted `approval`; run it again. `once` when the grant is
    /// for this command alone and goes when it has run.
    Allowed { approval: Approval, once: bool },
    /// The person kept it withheld, and the command's result now says so.
    Refused,
}

/// Asks for what a command failed without, when the sandbox is what kept it
/// from it -- the network, or the container engine -- and says the answer.
///
/// Whether a command needs either cannot be told from its name -- `npm test`
/// may reach the network, `mvn test` may start a Testcontainers database --
/// and asking before every build in case it does would train a person to
/// click through. So the command runs without first, and only a failure that
/// reads like the sandbox's refusal is put to the person, with the command
/// named. Before this, a model in a workspace without the grant saw
/// `ENOTFOUND` or `NU1301`, took it for a broken mirror and rewrote the
/// project around it.
#[allow(clippy::too_many_arguments)]
pub async fn withheld(
    store: &Store,
    id: pwr_domain::Id,
    step: u8,
    action: &ActionProposal,
    fingerprint: &str,
    outcome: &mut Result<serde_json::Value, ActionExecutionError>,
    policy: &mut ToolPolicy,
    prompt: &dyn ApprovalPrompt,
    refused: &mut RefusalStreak,
) -> Result<Withheld, String> {
    let ActionProposal::RunCommand {
        executable, args, ..
    } = action
    else {
        return Ok(Withheld::Nothing);
    };
    // A dependency fetch that the sandbox kept offline does not always fail:
    // package managers retry. Measured 2026-09-27 (stack matrix c3,
    // react-datatable): `npm install` ran until the time limit three times,
    // the network question never came, and the model fetched sixty-seven
    // packages by hand from the registry for half an hour.
    if let Err(ActionExecutionError::TimedOut(said)) = outcome {
        let offline = !policy.network_allowed() && policy.will_sandbox().unwrap_or(false);
        if !offline
            || !(pwr_tools::fetches_dependencies(executable, args)
                || pwr_tools::looks_like_network_denied(said))
        {
            return Ok(Withheld::Nothing);
        }
        let command = pwr_tools::command_line(executable, args);
        let description = format!(
            "let `{command}` reach the network -- it ran offline until it was stopped at the time \
             limit, as a package manager does while it waits for a network the sandbox withholds"
        );
        let decision = prompt.ask(Approval::NetworkAccess, &description).await;
        store
            .append(
                Some(id),
                "approval.decision",
                serde_json::json!({
                    "approval": Approval::NetworkAccess,
                    "description": description,
                    "decision": decision,
                    "step": step,
                }),
            )
            .map_err(|e| e.to_string())?;
        return Ok(match decision {
            ApprovalDecision::AllowOnce | ApprovalDecision::AllowForRun => {
                policy.approvals.push(Approval::NetworkAccess);
                Withheld::Allowed {
                    approval: Approval::NetworkAccess,
                    once: matches!(decision, ApprovalDecision::AllowOnce),
                }
            }
            ApprovalDecision::Deny => {
                said.push_str(
                    "\nThe engineer did not let this command reach the network; do it another \
                     way or say what it needs.",
                );
                refused.refused(fingerprint);
                Withheld::Refused
            }
        });
    }
    let Ok(value) = outcome else {
        return Ok(Withheld::Nothing);
    };
    let sandboxed = value.get("sandboxed").and_then(serde_json::Value::as_bool) == Some(true);
    let succeeded = value.get("exit_code").and_then(serde_json::Value::as_i64) == Some(0);
    // Without the sandbox nothing withheld it, and asking would not help.
    if !sandboxed || succeeded {
        return Ok(Withheld::Nothing);
    }
    let output = ["stdout", "stderr"]
        .iter()
        .filter_map(|stream| value.get(*stream).and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>()
        .join("\n");
    let command = pwr_tools::command_line(executable, args);
    let (approval, description) = if !policy.approvals.contains(&Approval::ContainerEngine)
        && pwr_tools::looks_like_container_engine_denied(&output)
    {
        (
            Approval::ContainerEngine,
            pwr_tools::container_engine_question(executable, args),
        )
    } else if !policy.network_allowed() && pwr_tools::looks_like_network_denied(&output) {
        (
            Approval::NetworkAccess,
            format!(
                "let `{command}` reach the network -- it failed because the sandbox kept it offline"
            ),
        )
    } else {
        return Ok(Withheld::Nothing);
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
        ApprovalDecision::AllowOnce => {
            policy.approvals.push(approval);
            Ok(Withheld::Allowed {
                approval,
                once: true,
            })
        }
        ApprovalDecision::AllowForRun => {
            policy.approvals.push(approval);
            Ok(Withheld::Allowed {
                approval,
                once: false,
            })
        }
        ApprovalDecision::Deny => {
            // Said in the result, not left to be inferred from `ENOTFOUND`:
            // a model that reads it as a flaky mirror retries it forever.
            let what = match approval {
                Approval::ContainerEngine => "use the container engine",
                _ => "reach the network",
            };
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "withheld".into(),
                    serde_json::Value::String(format!(
                        "the engineer did not let this command {what}; do it another way or \
                         say what it needs"
                    )),
                );
            }
            refused.refused(fingerprint);
            Ok(Withheld::Refused)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    /// Answers every question the same way and remembers what it was asked.
    struct Answer {
        decisions: Mutex<Vec<ApprovalDecision>>,
        asked: Mutex<Vec<(Approval, String)>>,
    }

    impl Answer {
        fn with(decisions: &[ApprovalDecision]) -> Self {
            Self {
                decisions: Mutex::new(decisions.iter().rev().copied().collect()),
                asked: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait::async_trait]
    impl ApprovalPrompt for Answer {
        async fn ask(&self, approval: Approval, description: &str) -> ApprovalDecision {
            self.asked
                .lock()
                .unwrap()
                .push((approval, description.to_owned()));
            self.decisions
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(ApprovalDecision::Deny)
        }
    }

    fn policy(allow: &[&str]) -> ToolPolicy {
        ToolPolicy {
            root: std::env::temp_dir(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: allow.iter().map(|program| (*program).to_owned()).collect(),
            output_limit: 4096,
            timeout: Duration::from_secs(5),
            sandbox: pwr_tools::SandboxPolicy::Disabled,
            approvals: Vec::new(),
        }
    }

    fn command(executable: &str, args: &[&str]) -> ActionProposal {
        ActionProposal::RunCommand {
            executable: executable.to_owned(),
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            stdin: None,
            cwd: None,
            outside_sandbox: false,
        }
    }

    async fn through(action: &ActionProposal, policy: &mut ToolPolicy, prompt: &Answer) -> Gate {
        let store = Store::open(":memory:").unwrap();
        gate(
            &store,
            pwr_domain::new_id(),
            1,
            action,
            "fingerprint",
            policy,
            prompt,
            &mut RefusalStreak::default(),
            None,
        )
        .await
        .unwrap()
    }

    /// Measured 2026-09-27 (stack matrix c3): `npm install` offline ran to the
    /// time limit three times and the network question never came.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_dependency_fetch_stopped_offline_at_the_limit_is_asked_about() {
        let store = Store::open(":memory:").unwrap();
        let mut policy = policy(&["npm", "cargo"]);
        policy.sandbox = pwr_tools::SandboxPolicy::Required;
        if !policy.will_sandbox().unwrap_or(false) {
            return;
        }
        let stopped = || {
            Err(ActionExecutionError::TimedOut(
                "the command did not finish within 120 s and was stopped.".into(),
            ))
        };
        let prompt = Answer::with(&[ApprovalDecision::AllowForRun]);
        let mut outcome = stopped();
        let answered = super::withheld(
            &store,
            pwr_domain::new_id(),
            1,
            &command("npm", &["install"]),
            "npm install",
            &mut outcome,
            &mut policy,
            &prompt,
            &mut RefusalStreak::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            answered,
            Withheld::Allowed {
                approval: Approval::NetworkAccess,
                once: false
            }
        );
        assert!(
            prompt.asked.lock().unwrap()[0]
                .1
                .contains("`npm install` reach the network")
        );
        assert!(policy.network_allowed());

        // A test run that hangs is not a fetch: nothing to ask.
        let mut policy = self::policy(&["cargo"]);
        policy.sandbox = pwr_tools::SandboxPolicy::Required;
        let prompt = Answer::with(&[]);
        let mut outcome = stopped();
        let answered = super::withheld(
            &store,
            pwr_domain::new_id(),
            1,
            &command("pytest", &["-x"]),
            "pytest",
            &mut outcome,
            &mut policy,
            &prompt,
            &mut RefusalStreak::default(),
        )
        .await
        .unwrap();
        assert_eq!(answered, Withheld::Nothing);
        assert!(prompt.asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_program_the_workspace_lists_is_not_asked_about() {
        let prompt = Answer::with(&[]);
        let mut policy = policy(&["cargo"]);
        let gate = through(&command("cargo", &["test"]), &mut policy, &prompt).await;
        assert_eq!(
            gate,
            Gate::Proceed {
                granted_once: Vec::new()
            }
        );
        assert!(prompt.asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_program_outside_the_list_is_a_question_not_a_refusal() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce]);
        let mut policy = policy(&["dotnet"]);
        let gate = through(
            &command("javac", &["-d", "out", "Main.java"]),
            &mut policy,
            &prompt,
        )
        .await;
        assert_eq!(
            gate,
            Gate::Proceed {
                granted_once: vec![Approval::ToolchainInstall]
            }
        );
        let asked = prompt.asked.lock().unwrap();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0].0, Approval::ToolchainInstall);
        // The person sees the whole command and what the workspace lists.
        assert!(
            asked[0].1.contains("javac -d out Main.java"),
            "{}",
            asked[0].1
        );
        assert!(asked[0].1.contains("dotnet"), "{}", asked[0].1);
    }

    #[tokio::test]
    async fn allowed_for_the_session_stops_asking() {
        let prompt = Answer::with(&[ApprovalDecision::AllowForRun]);
        let mut policy = policy(&[]);
        let first = through(&command("make", &["build"]), &mut policy, &prompt).await;
        assert_eq!(
            first,
            Gate::Proceed {
                granted_once: Vec::new()
            }
        );
        let second = through(&command("javac", &["Main.java"]), &mut policy, &prompt).await;
        assert_eq!(
            second,
            Gate::Proceed {
                granted_once: Vec::new()
            }
        );
        assert_eq!(prompt.asked.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn an_unlisted_push_needs_both_answers() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce, ApprovalDecision::AllowOnce]);
        let mut policy = policy(&[]);
        let gate = through(
            &command("git", &["push", "origin", "main"]),
            &mut policy,
            &prompt,
        )
        .await;
        assert_eq!(
            gate,
            Gate::Proceed {
                granted_once: vec![Approval::ToolchainInstall, Approval::Publish]
            }
        );
    }

    #[tokio::test]
    async fn a_command_naming_a_url_asks_for_the_network_first() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce]);
        let mut policy = policy(&["curl"]);
        let gate = through(
            &command("curl", &["-sSf", "https://example.com"]),
            &mut policy,
            &prompt,
        )
        .await;
        assert_eq!(
            gate,
            Gate::Proceed {
                granted_once: vec![Approval::NetworkAccess]
            }
        );
        assert_eq!(prompt.asked.lock().unwrap()[0].0, Approval::NetworkAccess);
    }

    fn failed_offline(
        stderr: &str,
        sandboxed: bool,
    ) -> Result<serde_json::Value, ActionExecutionError> {
        Ok(serde_json::json!({
            "exit_code": 1,
            "stdout": "",
            "stderr": stderr,
            "sandboxed": sandboxed,
        }))
    }

    async fn after(
        outcome: &mut Result<serde_json::Value, ActionExecutionError>,
        policy: &mut ToolPolicy,
        prompt: &Answer,
    ) -> Withheld {
        let store = Store::open(":memory:").unwrap();
        withheld(
            &store,
            pwr_domain::new_id(),
            1,
            &command("npm", &["install"]),
            "fingerprint",
            outcome,
            policy,
            prompt,
            &mut RefusalStreak::default(),
        )
        .await
        .unwrap()
    }

    const NPM_OFFLINE: &str =
        "npm ERR! code ENOTFOUND\nnpm ERR! request to https://registry.npmjs.org/left-pad failed";

    #[tokio::test]
    async fn a_command_the_sandbox_kept_offline_is_offered_the_network() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce]);
        let mut policy = policy(&["npm"]);
        let mut outcome = failed_offline(NPM_OFFLINE, true);
        assert_eq!(
            after(&mut outcome, &mut policy, &prompt).await,
            Withheld::Allowed {
                approval: Approval::NetworkAccess,
                once: true
            }
        );
        assert!(policy.network_allowed());
        let asked = prompt.asked.lock().unwrap();
        assert_eq!(asked[0].0, Approval::NetworkAccess);
        assert!(asked[0].1.contains("npm install"), "{}", asked[0].1);
    }

    #[tokio::test]
    async fn keeping_it_offline_is_said_in_the_result() {
        let prompt = Answer::with(&[ApprovalDecision::Deny]);
        let mut policy = policy(&["npm"]);
        let mut outcome = failed_offline(NPM_OFFLINE, true);
        assert_eq!(
            after(&mut outcome, &mut policy, &prompt).await,
            Withheld::Refused
        );
        assert!(!policy.network_allowed());
        let said = outcome.unwrap()["withheld"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(said.contains("did not let"), "{said}");
    }

    #[tokio::test]
    async fn a_test_run_that_wanted_docker_is_offered_the_engine() {
        let prompt = Answer::with(&[ApprovalDecision::AllowForRun]);
        let mut policy = policy(&["mvn"]);
        let mut outcome = failed_offline(
            "java.lang.IllegalStateException: Could not find a valid Docker environment.",
            true,
        );
        assert_eq!(
            after(&mut outcome, &mut policy, &prompt).await,
            Withheld::Allowed {
                approval: Approval::ContainerEngine,
                once: false
            }
        );
        assert!(policy.approvals.contains(&Approval::ContainerEngine));
        let asked = prompt.asked.lock().unwrap();
        assert!(
            asked[0].1.contains("outside PWR's sandbox"),
            "{}",
            asked[0].1
        );
    }

    #[tokio::test]
    async fn docker_is_asked_about_before_it_runs() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce, ApprovalDecision::AllowOnce]);
        let mut policy = policy(&["docker"]);
        let gate = through(
            &command("docker", &["push", "registry.example/api:1"]),
            &mut policy,
            &prompt,
        )
        .await;
        assert_eq!(
            gate,
            Gate::Proceed {
                granted_once: vec![Approval::Publish, Approval::ContainerEngine]
            }
        );
    }

    #[tokio::test]
    async fn only_a_refused_connection_in_the_sandbox_is_asked_about() {
        let prompt = Answer::with(&[]);
        let mut policy = policy(&["npm"]);
        // A test that failed, a command with no sandbox, one that succeeded.
        for mut outcome in [
            failed_offline("1 failing\n  AssertionError: expected 2 to equal 3", true),
            failed_offline(NPM_OFFLINE, false),
            Ok(
                serde_json::json!({"exit_code": 0, "stdout": NPM_OFFLINE, "stderr": "", "sandboxed": true}),
            ),
        ] {
            assert_eq!(
                after(&mut outcome, &mut policy, &prompt).await,
                Withheld::Nothing
            );
        }
        // And nothing is asked once the network is already granted.
        policy.approvals.push(Approval::NetworkAccess);
        let mut outcome = failed_offline(NPM_OFFLINE, true);
        assert_eq!(
            after(&mut outcome, &mut policy, &prompt).await,
            Withheld::Nothing
        );
        assert!(prompt.asked.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn refusing_the_second_question_takes_back_the_first() {
        let prompt = Answer::with(&[ApprovalDecision::AllowOnce, ApprovalDecision::Deny]);
        let mut policy = policy(&[]);
        let gate = through(&command("git", &["push"]), &mut policy, &prompt).await;
        assert!(matches!(gate, Gate::Refused(_)));
        assert!(policy.approvals.is_empty(), "{:?}", policy.approvals);
    }
}
