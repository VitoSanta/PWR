//! The action channel: native tool calls, and what happens when one is denied.

use pwr_domain::ToolCall;
use pwr_orchestrator::{action_from_tool_call, action_tool_catalog};
use pwr_tools::ActionProposal;

fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments,
        id: None,
    }
}

#[test]
fn the_catalog_offers_exactly_the_typed_capabilities() {
    let catalog = action_tool_catalog();
    let names: Vec<&str> = catalog
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec![
            "read_file",
            "replace_text",
            // Where a document becomes readable. Offered next to read_file
            // because that is the refusal that sends a run here.
            "extract_document",
            "search",
            // Beside search, because the two answer questions a caller
            // confuses: where a name is used, and where it is declared.
            "find_definition",
            "list_tree",
            "apply_replace",
            "write_file",
            "run_command",
            "fetch_url",
            // Records a claim rather than touching the workspace, like
            // `complete`, and sits beside it for that reason.
            "record_progress",
            "apply_patch",
            "start_service",
            "stop_service",
            "make_directory",
            "delete_path",
            // The way back from a broken edit, beside the other file changes.
            "restore_file",
            "move_path",
            "vcs_status",
            "vcs_diff",
            "propose_verifier",
            "complete",
            // The other terminal. Fifteen of twenty malformed turns across
            // three campaigns were the deployment refusing an attack task in
            // prose, because none of the nineteen capabilities let it refuse
            // through the channel.
            "decline"
        ]
    );
}

#[test]
fn a_native_call_becomes_its_typed_action() {
    let action = action_from_tool_call(&call(
        "apply_replace",
        serde_json::json!({
            "path": "src/lib.rs",
            "expected_hash": "abc",
            "replacement": "fn main() {}",
        }),
    ))
    .unwrap();
    let ActionProposal::ApplyReplace { path, .. } = action else {
        panic!("wrong capability");
    };
    assert_eq!(path, "src/lib.rs");
}

/// A name outside the offered set is refused, not guessed at.
#[test]
fn an_unoffered_tool_name_is_refused() {
    for name in ["exfiltrate", "read_file_v2", "readfile", ""] {
        assert!(
            action_from_tool_call(&call(name, serde_json::json!({"path": "a"}))).is_err(),
            "accepted: {name}"
        );
    }
}

#[test]
fn arguments_that_do_not_match_the_declared_schema_are_refused() {
    // Missing a required field.
    assert!(action_from_tool_call(&call("search", serde_json::json!({"max_matches": 5}))).is_err());
    // Wrong type.
    assert!(
        action_from_tool_call(&call(
            "list_tree",
            serde_json::json!({"max_entries": "many"})
        ))
        .is_err()
    );
    // Not an object at all.
    assert!(action_from_tool_call(&call("list_tree", serde_json::json!([1]))).is_err());
}

/// Policy still applies after a call is typed: the tool channel carries no
/// authority of its own.
#[test]
fn a_typed_call_is_still_subject_to_validation() {
    assert!(action_from_tool_call(&call("read_file", serde_json::json!({"path": ""}))).is_err());
    assert!(
        action_from_tool_call(&call(
            "search",
            serde_json::json!({"query": "x", "max_matches": 0})
        ))
        .is_err()
    );
}

#[test]
fn a_call_naming_complete_without_a_rationale_is_refused() {
    assert!(
        action_from_tool_call(&call("complete", serde_json::json!({"rationale": ""}))).is_err()
    );
    assert!(
        action_from_tool_call(&call("complete", serde_json::json!({"rationale": "done"}))).is_ok()
    );
}

// ------------------------------------------------------- denial feedback

use async_trait::async_trait;
use pwr_domain::{
    BackendState, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Proposes a denied edit first, then a valid one, then completes -- the shape
/// of a run whose first attempt is refused.
struct RecoveringProvider {
    turn: Arc<Mutex<usize>>,
    hash: String,
}
#[async_trait]
impl ModelProvider for RecoveringProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        let call = match *turn {
            // A stale hash: the refusal says "reread before editing".
            1 => serde_json::json!({
                "capability": "apply_replace", "path": "code.rs",
                "expected_hash": "stale", "replacement": "fixed"
            }),
            2 => serde_json::json!({
                "capability": "apply_replace", "path": "code.rs",
                "expected_hash": self.hash, "replacement": "fixed"
            }),
            _ => serde_json::json!({"capability": "complete", "rationale": "done"}),
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: call.to_string(),
            done: true,
            ..Default::default()
        })])))
    }
}

/// Aborting on the first refusal discards work already done. The action
/// budget, not the first denial, is what bounds the loop.
#[tokio::test]
async fn a_denied_action_is_returned_to_the_model_rather_than_ending_the_run() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("code.rs"), "broken").unwrap();
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["true".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = RecoveringProvider {
        turn: Arc::new(Mutex::new(0)),
        hash: pwr_domain::hash_bytes("broken"),
    };
    let request = ModelRequest {
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "fake".into(),
            endpoint: "http://localhost/".into(),
            model_ref: "fake".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
        messages: vec![],
        context_tokens: 1024,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let checks = vec![("true".into(), Vec::new())];
    let result = pwr_orchestrator::run_action_loop(
        &store, &provider, run_id, request, &policy, &checks, 6,
    )
    .await
    .unwrap();
    assert!(result.verified);
    assert_eq!(
        std::fs::read_to_string(root.path().join("code.rs")).unwrap(),
        "fixed"
    );
    let actions: Vec<_> = store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter(|e| e.event_type == "tool.action")
        .collect();
    // The refusal is in the audit, and the run continued past it.
    assert_eq!(actions[0].payload["status"], "denied");
    assert_eq!(actions[1].payload["status"], "allowed");
}

/// Every event of one run shares its identifier, or the audit is split in two
/// and `report` shows only half of it.
#[tokio::test]
async fn the_whole_run_is_recorded_under_one_identifier() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("code.rs"), "broken").unwrap();
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["true".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = RecoveringProvider {
        turn: Arc::new(Mutex::new(2)),
        hash: pwr_domain::hash_bytes("broken"),
    };
    let request = ModelRequest {
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "fake".into(),
            endpoint: "http://localhost/".into(),
            model_ref: "fake".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
        messages: vec![],
        context_tokens: 512,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let checks = vec![("true".into(), Vec::new())];
    pwr_orchestrator::run_action_loop(&store, &provider, run_id, request, &policy, &checks, 4)
        .await
        .unwrap();
    let events = store.events_for_run(run_id).unwrap();
    assert!(
        events
            .iter()
            .any(|e| e.event_type == "verification.baseline")
    );
    assert!(events.iter().any(|e| e.event_type == "task.complete"));
    for pair in events.windows(2) {
        assert_eq!(pair[1].previous_hash.as_ref(), Some(&pair[0].event_hash));
    }
}

/// verification-recovery.md: "make one hypothesis-linked correction, rerun the
/// narrow check". Without the rerun the deployment cannot learn whether its
/// edit worked, so a correct edit is followed by guessing.
#[tokio::test]
async fn a_successful_edit_is_followed_by_the_narrow_check() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("code.rs"), "broken").unwrap();
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["true".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(10),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = RecoveringProvider {
        turn: Arc::new(Mutex::new(1)),
        hash: pwr_domain::hash_bytes("broken"),
    };
    let request = ModelRequest {
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "fake".into(),
            endpoint: "http://localhost/".into(),
            model_ref: "fake".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
        messages: vec![],
        context_tokens: 512,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let checks = vec![("true".to_string(), vec![])];
    pwr_orchestrator::run_action_loop(&store, &provider, run_id, request, &policy, &checks, 6)
        .await
        .unwrap();
    let interim: Vec<_> = store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter(|e| e.event_type == "verification.interim")
        .collect();
    assert_eq!(interim.len(), 1, "the edit was not followed by a check");
    assert_eq!(interim[0].payload["passing"], true);
}

/// A denied edit changed nothing, so there is nothing to re-check.
#[tokio::test]
async fn a_denied_edit_does_not_trigger_a_check() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("code.rs"), "broken").unwrap();
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["true".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(10),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    // Turn 0 proposes the stale-hash edit, which is refused.
    let provider = RecoveringProvider {
        turn: Arc::new(Mutex::new(0)),
        hash: pwr_domain::hash_bytes("broken"),
    };
    let request = ModelRequest {
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "fake".into(),
            endpoint: "http://localhost/".into(),
            model_ref: "fake".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
        messages: vec![],
        context_tokens: 1024,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let checks = vec![("true".to_string(), vec![])];
    pwr_orchestrator::run_action_loop(&store, &provider, run_id, request, &policy, &checks, 6)
        .await
        .unwrap();
    let events = store.events_for_run(run_id).unwrap();
    let denied = events
        .iter()
        .filter(|e| e.payload["status"] == "denied")
        .count();
    let interim = events
        .iter()
        .filter(|e| e.event_type == "verification.interim")
        .count();
    assert_eq!(denied, 1);
    // One allowed edit followed, so exactly one check -- not two.
    assert_eq!(interim, 1);
}

/// The completion rule must be stated in both directions. A deployment told
/// only when *not* to complete has nothing connecting a passing check to the
/// action it implies.
#[test]
fn the_system_prompt_states_when_to_complete_and_when_not_to() {
    let prompt = pwr_orchestrator::AGENT_SYSTEM_PROMPT;
    assert!(prompt.contains("Call complete when the task is done and required checks pass"));
    assert!(prompt.contains("Do not call complete while a required check is failing"));
    // The exception is the harness's to record. A deployment that may declare
    // its own exemptions can complete over any failure it decides to excuse.
    assert!(prompt.contains("only known-failure exceptions explicitly recorded by the harness"));
    // The hash guard is the most common denial in practice, so the prompt says
    // what to do about it rather than leaving it to be discovered per run.
    assert!(prompt.contains("re-read a file after editing it"));
    // Creation, partial edit and whole-file rewrite are different tools, and a
    // deployment told about only one of them is limited to what that one can
    // reach.
    assert!(prompt.contains("write_file"));
    assert!(prompt.contains("Change part of a file with replace_text"));
    assert!(prompt.contains("only when most of it"));
}

/// A read-only question and a repair have different completion contracts. The
/// repair prompt told a deployment answering a question to fix what it found,
/// which the task forbids; the read-only prompt has to say the opposite in both
/// directions, or the same failure returns.
#[test]
fn the_read_only_prompt_forbids_repair_and_allows_red_checks() {
    let prompt = pwr_orchestrator::AGENT_READ_ONLY_SYSTEM_PROMPT;
    assert!(prompt.contains("Do not change any file"));
    assert!(prompt.contains("evidence to diagnose, not a demand to repair"));
    assert!(prompt.contains("Existing red checks need not become green"));
    // Without somewhere to put it, a correct diagnosis is not observable.
    assert!(prompt.contains("call complete with your answer in rationale"));
}

/// The prompt is assembled from fragments; a missing space between two of them
/// silently changes the words the deployment reads.
#[test]
fn the_system_prompt_has_no_broken_spacing() {
    for prompt in [
        pwr_orchestrator::AGENT_SYSTEM_PROMPT,
        pwr_orchestrator::AGENT_READ_ONLY_SYSTEM_PROMPT,
    ] {
        assert!(!prompt.contains("  "), "double space in the prompt");
        assert!(!prompt.contains(".T") && !prompt.contains("sthe"));
    }
}

/// A turn that proposed an action was recorded as a string, so the next
/// request carried the deployment's own call back to it as prose it had to
/// re-read rather than as the call it made. A backend whose protocol pairs a
/// call with its result cannot do that pairing from text.
#[tokio::test]
async fn an_assistant_turn_carries_its_calls_structurally() {
    let message = pwr_domain::ChatMessage {
        role: "assistant".into(),
        content: String::new(),
        tool_calls: vec![pwr_domain::ToolCall {
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "a.rs"}),
            id: Some("call-1".into()),
        }],
        tool_call_id: None,
        purpose: None,
        images: Vec::new(),
    };
    let wire = serde_json::to_value(&message).unwrap();
    assert_eq!(wire["tool_calls"][0]["name"], "read_file");
    // A text-only message carries neither field, so nothing changes for a
    // backend that has no use for them.
    let plain = pwr_domain::ChatMessage::text("user", "fix it");
    let wire = serde_json::to_value(&plain).unwrap();
    assert!(wire.get("tool_calls").is_none(), "{wire}");
    assert!(wire.get("tool_call_id").is_none(), "{wire}");
}

/// Without an id, a run of results answers a run of calls by position -- and
/// position is exactly what compaction changes.
#[tokio::test]
async fn a_tool_result_names_the_call_it_answers() {
    let answer = pwr_domain::ChatMessage {
        role: "tool".into(),
        content: "{}".into(),
        tool_calls: Vec::new(),
        tool_call_id: Some("call-1".into()),
        purpose: None,
        images: Vec::new(),
    };
    let wire = serde_json::to_value(&answer).unwrap();
    assert_eq!(wire["tool_call_id"], "call-1");
}

/// Found by running a real generation task. A deployment sent
/// `"args": "--version"` where the schema declares a list, and five malformed
/// calls -- three consecutive -- ended the run over a mistake the harness
/// could see through.
#[test]
fn a_lone_string_is_accepted_where_a_list_was_declared() {
    let action = pwr_orchestrator::action_from_tool_call(&pwr_domain::ToolCall {
        name: "run_command".into(),
        arguments: serde_json::json!({"executable": "node", "args": "--version"}),
        id: None,
    })
    .expect("a single argument sent as a string");
    match action {
        pwr_tools::ActionProposal::RunCommand {
            executable, args, ..
        } => {
            assert_eq!(executable, "node");
            assert_eq!(args, vec!["--version".to_string()]);
        }
        other => panic!("wrong action: {other:?}"),
    }
}

/// The first version of the coercion made `"run build"` one argument with a
/// space in it. npm answered `Unknown command: "run build"` and the deployment
/// spent the rest of the run unable to work out why its build would not run.
///
/// Two readings are available -- one argument containing a space, or two
/// arguments -- and choosing silently is guessing. So it is refused, and the
/// refusal carries the list that should have been sent, because a refusal that
/// withholds what it already knows costs a turn to rediscover.
#[test]
fn a_string_of_several_words_is_refused_rather_than_guessed_at() {
    let problem = pwr_orchestrator::action_from_tool_call(&pwr_domain::ToolCall {
        name: "run_command".into(),
        arguments: serde_json::json!({"executable": "npm", "args": "run build"}),
        id: None,
    })
    .unwrap_err();
    assert!(problem.problem.contains(r#"["run", "build"]"#), "{problem}");
    // The kind is what a campaign aggregates; the message is what a person
    // reads. Asserting on the kind is what stops a reworded message from
    // silently changing a measurement.
    assert_eq!(problem.kind, "argument_shape");
}

/// A proper list is untouched.
#[test]
fn a_declared_list_still_arrives_as_a_list() {
    let action = pwr_orchestrator::action_from_tool_call(&pwr_domain::ToolCall {
        name: "run_command".into(),
        arguments: serde_json::json!({"executable": "npm", "args": ["run", "build"]}),
        id: None,
    })
    .unwrap();
    match action {
        pwr_tools::ActionProposal::RunCommand { args, .. } => {
            assert_eq!(args, vec!["run".to_string(), "build".to_string()]);
        }
        other => panic!("wrong action: {other:?}"),
    }
}

/// Every way a turn can fail to produce an action names itself.
///
/// A fifth of turns on the recorded campaigns ended here, under one counter.
/// Prose where a call was expected, a tool the deployment invented, and a real
/// tool filled in wrongly are three faults with three different fixes, and a
/// single number cannot choose between them.
#[test]
fn a_malformed_call_says_which_kind_of_malformed_it_was() {
    use pwr_orchestrator::{action_from_tool_call, parse_action_proposal};

    let kind_of = |name: &str, args: serde_json::Value| {
        action_from_tool_call(&pwr_domain::ToolCall {
            name: name.into(),
            arguments: args,
            id: None,
        })
        .unwrap_err()
        .kind
    };

    // Prose, or anything that is not one action object, through the content
    // channel. This is the deployment answering instead of acting.
    assert_eq!(
        parse_action_proposal("Sure! I'll start by reading the file.")
            .unwrap_err()
            .kind,
        "no_tool_call"
    );
    // A name that was never offered: the deployment inventing a capability.
    assert_eq!(
        kind_of("write_everything", serde_json::json!({"path": "a"})),
        "unknown_capability"
    );
    // An offered name, filled in wrongly. Distinguished from the above because
    // a prompt fixes one and a schema fixes the other.
    assert_eq!(
        kind_of("read_file", serde_json::json!({"wrong": "a"})),
        "schema_mismatch"
    );
    assert_eq!(kind_of("read_file", serde_json::json!([1])), "no_arguments");
    // Decoded cleanly and still refused, by the action's own validation.
    assert_eq!(
        kind_of("read_file", serde_json::json!({"path": ""})),
        "invalid_action"
    );
}

/// The two commonest kinds carry what the turn contained.
///
/// Across three campaigns `multiple_calls` and `no_tool_call` are almost every
/// malformed turn, and the kind alone cannot settle what to do about either.
/// Three reads of different files is a deployment working in parallel, where
/// taking the first would cost nothing; an edit followed by its verification is
/// a plan whose order matters. The decision needs the names, not the count.
#[test]
fn the_two_commonest_kinds_say_what_the_turn_contained() {
    use pwr_orchestrator::parse_action_proposal;

    let call = |name: &str| pwr_domain::ToolCall {
        name: name.into(),
        arguments: serde_json::json!({"path": "a"}),
        id: None,
    };
    let reply = pwr_provider::ModelReply {
        tool_calls: vec![call("read_file"), call("read_file"), call("search")],
        ..Default::default()
    };
    let problem = pwr_orchestrator::action_from_reply_for_test(&reply).unwrap_err();
    assert_eq!(problem.kind, "multiple_calls");
    // In the order proposed, so "parallel reads" can be told from "a plan".
    assert_eq!(
        problem.detail.as_deref(),
        Some("read_file(a), read_file(a), search(a)")
    );

    // Prose instead of a call carries what it said instead: a refusal, a
    // question and a narration of work it believed done need different answers.
    let prose = parse_action_proposal("I cannot do that without network access.").unwrap_err();
    assert_eq!(prose.kind, "no_tool_call");
    assert_eq!(
        prose.detail.as_deref(),
        Some("I cannot do that without network access.")
    );
}

/// An excerpt, not the generation. The events carry bounded excerpts rather
/// than retained content, and this follows that rule instead of making one.
#[test]
fn the_detail_is_bounded() {
    use pwr_orchestrator::parse_action_proposal;
    let long = "x".repeat(10_000);
    let detail = parse_action_proposal(&long).unwrap_err().detail.unwrap();
    assert!(detail.chars().count() <= 241, "{}", detail.chars().count());
    assert!(detail.ends_with('…'), "a cut excerpt says it was cut");
}

/// The names alone were not enough, and one campaign proved it.
///
/// Five occurrences of `read_file, read_file` made "take the first call" look
/// free. The next campaign produced `replace_text, replace_text`, where taking
/// the first performs one edit, silently drops the other, and leaves the
/// deployment believing both landed. Two edits to the same file and two to
/// different files are not the same situation either, and only the target
/// separates them.
#[test]
fn several_calls_carry_what_each_one_aimed_at() {
    let reply = pwr_provider::ModelReply {
        tool_calls: vec![
            call(
                "replace_text",
                serde_json::json!({"path": "src/lib.rs", "find": "a", "replace": "b"}),
            ),
            call(
                "replace_text",
                serde_json::json!({"path": "src/math.rs", "find": "c", "replace": "d"}),
            ),
        ],
        ..Default::default()
    };
    assert_eq!(
        pwr_orchestrator::action_from_reply_for_test(&reply)
            .unwrap_err()
            .detail
            .as_deref(),
        Some("replace_text(src/lib.rs), replace_text(src/math.rs)")
    );

    // Every capability aims at something, and each names its own field.
    let cases = [
        (
            "run_command",
            serde_json::json!({"executable": "cargo", "args": ["test"]}),
            "run_command(cargo)",
        ),
        (
            "search",
            serde_json::json!({"query": "parse_port"}),
            "search(parse_port)",
        ),
        (
            "record_progress",
            serde_json::json!({"step": 2}),
            "record_progress(2)",
        ),
        // Nothing to aim at, and a bare name beats an empty parenthesis.
        (
            "complete",
            serde_json::json!({"rationale": "done"}),
            "complete",
        ),
    ];
    for (name, arguments, expected) in cases {
        let reply = pwr_provider::ModelReply {
            tool_calls: vec![
                call(name, arguments),
                call("complete", serde_json::json!({})),
            ],
            ..Default::default()
        };
        let detail = pwr_orchestrator::action_from_reply_for_test(&reply)
            .unwrap_err()
            .detail
            .unwrap();
        assert!(detail.starts_with(expected), "{detail}");
    }
}

/// A turn that reaches here may hold calls that would not parse, and a detail
/// that goes blank exactly when the call was strange is the wrong way round.
#[test]
fn an_unparseable_call_still_says_what_it_was() {
    let reply = pwr_provider::ModelReply {
        tool_calls: vec![
            call("read_file", serde_json::json!({"path": "src/lib.rs"})),
            call("write_everything", serde_json::json!([1, 2, 3])),
        ],
        ..Default::default()
    };
    assert_eq!(
        pwr_orchestrator::action_from_reply_for_test(&reply)
            .unwrap_err()
            .detail
            .as_deref(),
        Some("read_file(src/lib.rs), write_everything")
    );
}

/// Every kind carries what it saw, not only which fault it was.
///
/// Measured on gpt-oss:20b: twenty-one `unknown_capability` in one screening
/// campaign, and no way to tell a deployment inventing plausible names one at a
/// time from one speaking a different tool convention throughout. Those need
/// opposite fixes -- the first is a prompt, the second is ours -- and a count
/// chooses neither.
#[test]
fn a_rejected_call_says_what_name_and_shape_it_had() {
    let unknown = action_from_tool_call(&call(
        "run_shell",
        serde_json::json!({"cmd": "ls", "cwd": "."}),
    ))
    .unwrap_err();
    assert_eq!(unknown.kind, "unknown_capability");
    // The name and the keys it filled in: enough to recognise a convention.
    let detail = unknown.detail.expect("no detail");
    assert!(detail.starts_with("run_shell("), "{detail}");
    assert!(detail.contains("cmd") && detail.contains("cwd"), "{detail}");

    let mismatch =
        action_from_tool_call(&call("read_file", serde_json::json!({"file": "a"}))).unwrap_err();
    assert_eq!(mismatch.kind, "schema_mismatch");
    assert!(
        mismatch
            .detail
            .as_deref()
            .is_some_and(|d| d.contains("file")),
        "{:?}",
        mismatch.detail
    );

    let shapeless = action_from_tool_call(&call("read_file", serde_json::json!([1]))).unwrap_err();
    assert_eq!(shapeless.kind, "no_arguments");
    assert!(shapeless.detail.is_some());
}

/// Every name the schema offers must be a name the decoder accepts.
///
/// It was not. `apply_patch` has been advertised since the action channel
/// existed, while the typed action's tag said `apply_patch_hunks`, so a
/// deployment calling the tool exactly as offered got "unknown variant" back.
/// Three of those in a row end a run: the ornith-1.5:35b run of 2026-09-06 was
/// killed that way at 1,218 seconds, with a finished and working site sitting
/// in its workspace. Nothing in the suite compared the two lists, so a table
/// that does is the only thing that stops it happening to the next capability.
#[test]
fn every_offered_tool_can_actually_be_called() {
    // Minimal valid arguments per capability. A tool added to the schema
    // without a row here fails the assertion below rather than being skipped.
    let arguments: &[(&str, serde_json::Value)] = &[
        ("read_file", serde_json::json!({"path": "a.rs"})),
        (
            "replace_text",
            serde_json::json!({"path": "a.rs", "expected_hash": "h", "find": "x", "replace": "y"}),
        ),
        ("extract_document", serde_json::json!({"path": "cv.pdf"})),
        (
            "search",
            serde_json::json!({"query": "x", "max_matches": 5}),
        ),
        ("list_tree", serde_json::json!({"max_entries": 10})),
        (
            "apply_replace",
            serde_json::json!({"path": "a.rs", "expected_hash": "h", "replacement": "x"}),
        ),
        (
            "write_file",
            serde_json::json!({"path": "a.rs", "content": "x"}),
        ),
        (
            "run_command",
            serde_json::json!({"executable": "ls", "args": []}),
        ),
        (
            "fetch_url",
            serde_json::json!({"url": "https://example.com"}),
        ),
        (
            "record_progress",
            serde_json::json!({"step": 1, "note": "n"}),
        ),
        (
            "apply_patch",
            serde_json::json!({"path": "a.rs", "expected_hash": "h", "hunks": [{"find": "x", "replace": "y"}]}),
        ),
        (
            "start_service",
            serde_json::json!({"executable": "python3", "args": [], "port": 8000}),
        ),
        ("stop_service", serde_json::json!({"id": 1})),
        ("find_definition", serde_json::json!({"name": "to_cents"})),
        ("make_directory", serde_json::json!({"path": "d"})),
        ("delete_path", serde_json::json!({"path": "d"})),
        ("restore_file", serde_json::json!({"path": "a"})),
        ("move_path", serde_json::json!({"from": "a", "to": "b"})),
        ("vcs_status", serde_json::json!({})),
        ("vcs_diff", serde_json::json!({"paths": []})),
        (
            "propose_verifier",
            serde_json::json!({"executable": "cargo", "args": ["test"], "rationale": "r"}),
        ),
        ("complete", serde_json::json!({"rationale": "r"})),
        ("decline", serde_json::json!({"rationale": "r"})),
    ];
    let catalog = action_tool_catalog();
    let offered: Vec<&str> = catalog
        .tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect();
    for name in &offered {
        let args = arguments
            .iter()
            .find(|(known, _)| known == name)
            .unwrap_or_else(|| panic!("{name} is offered but this test has no arguments for it"))
            .1
            .clone();
        assert!(
            action_from_tool_call(&call(name, args)).is_ok(),
            "{name} is offered to deployments but the decoder rejects it"
        );
    }
}

/// Twelve of fourteen malformed turns in the Angular run of 2026-09-07 were the
/// deployment asking for two to four files at once on a 26-file project. Every
/// one was thrown away whole, and the run died having spent fifty of its
/// seventy-six actions reading one file per turn.
#[test]
fn a_turn_may_ask_for_several_independent_reads() {
    use pwr_provider::ModelReply;
    let reply = ModelReply {
        tool_calls: vec![
            call("read_file", serde_json::json!({"path": "src/app/app.ts"})),
            call("read_file", serde_json::json!({"path": "src/app/app.html"})),
            call("list_tree", serde_json::json!({"max_entries": 50})),
        ],
        ..Default::default()
    };
    let actions = pwr_orchestrator::actions_from_reply_for_test(&reply).expect("read batch");
    assert_eq!(actions.len(), 3);
    assert!(matches!(actions[2], ActionProposal::ListTree { .. }));
}

/// The refusal this relaxes was right about edits: taking the first of two
/// `replace_text` calls performs one, drops the other, and leaves the
/// deployment believing both landed.
#[test]
fn a_turn_mixing_reads_with_an_edit_is_still_refused_whole() {
    use pwr_provider::ModelReply;
    let mixed = ModelReply {
        tool_calls: vec![
            call("read_file", serde_json::json!({"path": "a.rs"})),
            call(
                "replace_text",
                serde_json::json!({"path": "a.rs", "expected_hash": "h", "find": "x", "replace": "y"}),
            ),
        ],
        ..Default::default()
    };
    let problem = pwr_orchestrator::actions_from_reply_for_test(&mixed).unwrap_err();
    assert_eq!(problem.kind, "multiple_calls");

    let two_edits = ModelReply {
        tool_calls: vec![
            call(
                "replace_text",
                serde_json::json!({"path": "a.rs", "expected_hash": "h", "find": "x", "replace": "y"}),
            ),
            call(
                "replace_text",
                serde_json::json!({"path": "b.rs", "expected_hash": "h", "find": "x", "replace": "y"}),
            ),
        ],
        ..Default::default()
    };
    assert!(pwr_orchestrator::actions_from_reply_for_test(&two_edits).is_err());
}

/// A turn asking for more reads than fit is not thrown away: it is bounded
/// where the cost is known, which is the loop, and what did not fit is named.
///
/// Refusing it here is the exact failure this change exists to remove, and the
/// second Angular run of 2026-09-07 produced that turn -- seven reads at once,
/// refused whole under the first version of this code.
#[test]
fn a_turn_asking_for_more_reads_than_fit_is_not_refused_whole() {
    use pwr_provider::ModelReply;
    let reply = ModelReply {
        tool_calls: (0..7)
            .map(|n| call("read_file", serde_json::json!({"path": format!("f{n}.rs")})))
            .collect(),
        ..Default::default()
    };
    let actions = pwr_orchestrator::actions_from_reply_for_test(&reply).expect("accepted");
    assert_eq!(actions.len(), 7);
}

/// A turn that reasoned and never answered is not a turn that answered badly.
#[test]
fn thinking_with_no_answer_is_reported_as_what_it_is() {
    use pwr_provider::ModelReply;
    let reply = ModelReply {
        content: String::new(),
        thinking: "I should read the component first, but the context is nearly full...".into(),
        ..Default::default()
    };
    let problem = pwr_orchestrator::actions_from_reply_for_test(&reply).unwrap_err();
    assert_eq!(problem.kind, "thinking_only");
    assert!(problem.problem.contains("no answer"), "{}", problem.problem);

    // Prose where a call belonged is still its own kind.
    let prose = ModelReply {
        content: "Let me extract the CV PDF.".into(),
        ..Default::default()
    };
    assert_eq!(
        pwr_orchestrator::actions_from_reply_for_test(&prose)
            .unwrap_err()
            .kind,
        "no_tool_call"
    );
}

/// The compatibility layer's whole claim, asserted at the loop's own boundary
/// rather than inside the adapter: a call Qwen wrote into its answer text
/// becomes the same `ActionProposal` as one the backend decoded natively, and
/// a deployment with no declared family is read exactly as it was before the
/// layer existed.
mod family_conventions {
    use pwr_compat::{GenericAdapter, QwenFamilyAdapter, adapter_for};

    /// Qwen's chat template renders a call inside the answer, and any backend
    /// whose parser misses that block hands it over as prose. The loop then
    /// reported "model output must be one valid typed-action JSON object"
    /// about output that contained a perfectly well-formed call, and spent a
    /// turn of the malformed budget doing it.
    #[test]
    fn a_call_written_into_the_answer_reaches_the_loop_as_that_action() {
        let embedded = pwr_provider::ModelReply {
            content: "Reading it now.\n<tool_call>{\"name\": \"read_file\", \"arguments\": \
                      {\"path\": \"src/lib.rs\"}}</tool_call>"
                .into(),
            ..Default::default()
        };
        let native = pwr_provider::ModelReply {
            tool_calls: vec![pwr_domain::ToolCall {
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/lib.rs"}),
                id: Some("call_1".into()),
            }],
            ..Default::default()
        };
        let through_qwen =
            pwr_orchestrator::action_from_reply_through_for_test(&QwenFamilyAdapter, &embedded)
                .expect("a call in the answer text is still a call");
        let natively = pwr_orchestrator::action_from_reply_for_test(&native)
            .expect("a natively decoded call");
        // Same action, whichever channel carried it. That is the property the
        // layer exists to provide.
        assert_eq!(
            serde_json::to_value(&through_qwen).unwrap(),
            serde_json::to_value(&natively).unwrap()
        );
    }

    /// The counterpart, and the more important one: an unknown deployment must
    /// not be read through a convention it may not follow.
    #[test]
    fn an_unknown_family_is_read_exactly_as_it_was_before() {
        let embedded = pwr_provider::ModelReply {
            content: "<tool_call>{\"name\": \"read_file\", \"arguments\": {\"path\": \"a\"}}\
                      </tool_call>"
                .into(),
            ..Default::default()
        };
        let generic =
            pwr_orchestrator::action_from_reply_through_for_test(&GenericAdapter, &embedded);
        let verbatim = pwr_orchestrator::action_from_reply_for_test(&embedded);
        assert!(generic.is_err() && verbatim.is_err());
        assert_eq!(
            generic.unwrap_err().kind,
            verbatim.unwrap_err().kind,
            "the generic adapter changed how an unnormalized reply is read"
        );
    }

    /// A turn that spent its budget reasoning is a context problem, and it was
    /// reported as the deployment writing nonsense. The classification has to
    /// survive the family adapter, because the family adapter is what moves
    /// the reasoning into its own channel in the first place.
    #[test]
    fn reasoning_that_consumed_the_turn_is_still_classified_as_reasoning() {
        let reply = pwr_provider::ModelReply {
            content: "<think>weighing which file to open, and then".into(),
            ..Default::default()
        };
        let problem =
            pwr_orchestrator::action_from_reply_through_for_test(&QwenFamilyAdapter, &reply)
                .unwrap_err();
        assert_eq!(problem.kind, "thinking_only");
    }

    /// Selection is made from observed evidence, and the fallback is the
    /// adapter that assumes nothing.
    #[test]
    fn the_adapter_is_chosen_from_evidence_and_never_assumed() {
        assert_eq!(adapter_for(Some("qwen3_5"), "qwen/qwen3.5-9b").id(), "qwen");
        assert_eq!(adapter_for(Some("granite"), "granite:8b").id(), "granite");
        assert_eq!(adapter_for(Some("llama"), "llama3:8b").id(), "generic");
    }
}

/// A call written against the schema as it stood before `search` grew
/// parameters still means what it meant then.
///
/// The reason those parameters are defaulted rather than required. A
/// deployment does not re-learn a catalogue between turns of the same run, and
/// three unreadable calls in a row end a run for a broken action channel: a
/// schema change that invalidates the call the deployment has been making all
/// along would end runs that were working.
#[test]
fn a_search_call_without_the_new_parameters_is_still_a_literal_search() {
    let action = action_from_tool_call(&call(
        "search",
        serde_json::json!({"query": "to_cents", "max_matches": 10}),
    ))
    .expect("the older shape must still parse");
    assert!(matches!(
        action,
        ActionProposal::Search {
            regex: false,
            path_glob: None,
            ..
        }
    ));
}

#[test]
fn a_search_call_carrying_them_is_a_pattern_over_part_of_the_tree() {
    let action = action_from_tool_call(&call(
        "search",
        serde_json::json!({
            "query": "(fn|def) to_cents",
            "max_matches": 10,
            "regex": true,
            "path_glob": "src/**",
        }),
    ))
    .expect("the fuller shape must parse");
    match action {
        ActionProposal::Search {
            regex, path_glob, ..
        } => {
            assert!(regex);
            assert_eq!(path_glob.as_deref(), Some("src/**"));
        }
        other => panic!("wrong action: {other:?}"),
    }
}

/// Observed 107 times across the R2 pilot traces: `search(path_glob, query)`
/// refused as malformed for want of a bound the harness can supply.
#[test]
fn a_search_call_needs_only_the_query() {
    let action = action_from_tool_call(&call(
        "search",
        serde_json::json!({"query": "numeric_range", "path_glob": "**/*.py"}),
    ))
    .expect("a query must be enough");
    match action {
        ActionProposal::Search { max_matches, .. } => {
            assert_eq!(max_matches, pwr_tools::DEFAULT_SEARCH_MATCHES)
        }
        other => panic!("wrong action: {other:?}"),
    }
}

/// `max_matches` is what a caller cannot answer for a declaration, so it is
/// optional there and the harness supplies the bound.
#[test]
fn a_declaration_call_needs_only_the_name() {
    let action = action_from_tool_call(&call(
        "find_definition",
        serde_json::json!({"name": "to_cents"}),
    ))
    .expect("a bare name must be enough");
    assert!(matches!(
        action,
        ActionProposal::FindDefinition {
            max_matches: None,
            path_glob: None,
            ..
        }
    ));
}

/// `package.json` written with `content` as the object rather than its text:
/// one reading, so it is written as that object's JSON.
#[test]
fn file_content_sent_as_an_object_is_written_as_its_json() {
    let action = pwr_orchestrator::action_from_tool_call(&pwr_domain::ToolCall {
        name: "write_file".into(),
        arguments: serde_json::json!({
            "path": "package.json",
            "content": {"name": "game", "type": "module", "scripts": {"test": "node --test"}},
        }),
        id: None,
    })
    .expect("an object where the file's text was expected");
    match action {
        pwr_tools::ActionProposal::WriteFile { path, content } => {
            assert_eq!(path, "package.json");
            let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
            assert_eq!(parsed["scripts"]["test"], "node --test");
            assert!(content.ends_with('\n'));
        }
        other => panic!("wrong action: {other:?}"),
    }
}
