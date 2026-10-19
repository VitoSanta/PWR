//! Exercise bounded recovery through the product turn, with faults mixed with
//! successful actions. These sequences defeated consecutive fault counters.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{DenyWithoutAsking, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Mutex, atomic::AtomicBool},
};

struct Sequence {
    replies: Mutex<VecDeque<Result<ModelChunk, ProviderError>>>,
    requests: Mutex<Vec<ModelRequest>>,
}
#[async_trait]
impl ModelProvider for Sequence {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected extra generation");
        Ok(Box::pin(futures_util::stream::iter([reply])))
    }
}
fn call(name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments: args,
        id: None,
    }
}
fn calls(mut calls: Vec<ToolCall>) -> Result<ModelChunk, ProviderError> {
    for (index, call) in calls.iter_mut().enumerate() {
        call.id = Some(format!("call-{index}"));
    }
    Ok(ModelChunk {
        tool_calls: calls,
        done: true,
        ..Default::default()
    })
}
fn read() -> Result<ModelChunk, ProviderError> {
    calls(vec![call(
        "read_file",
        serde_json::json!({"path":"kept.txt"}),
    )])
}
fn empty() -> Result<ModelChunk, ProviderError> {
    Ok(ModelChunk {
        done: true,
        ..Default::default()
    })
}
fn fault() -> Result<ModelChunk, ProviderError> {
    Err(ProviderError::ModelOutput {
        safe_context: "invalid call".into(),
    })
}
fn looped() -> Result<ModelChunk, ProviderError> {
    Err(ProviderError::Looping {
        safe_context: "repeated passage".into(),
    })
}
fn complete() -> Result<ModelChunk, ProviderError> {
    calls(vec![call(
        "complete",
        serde_json::json!({"rationale":"done"}),
    )])
}
fn malformed() -> ToolCall {
    call("read_file", serde_json::json!({}))
}

async fn run(
    replies: Vec<Result<ModelChunk, ProviderError>>,
    sampling: BTreeMap<String, serde_json::Value>,
) -> (
    converse::TurnReport,
    Vec<ChatMessage>,
    Vec<ModelRequest>,
    pwr_store::Store,
    pwr_domain::Id,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("kept.txt"), "before").unwrap();
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "gemma-4-12B-it".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let mut messages = vec![
        ChatMessage::text("system", "work"),
        ChatMessage::text("user", "inspect the workspace"),
    ];
    let provider = Sequence {
        replies: Mutex::new(replies.into()),
        requests: Mutex::new(vec![]),
    };
    let report = converse::take_turn(
        &provider,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        id,
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        16384,
        &[],
        sampling,
        pwr_compat::render_tools(&converse::chat_tool_catalog()),
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    let requests = provider.requests.into_inner().unwrap();
    (report, messages, requests, store, id, root)
}

#[tokio::test]
async fn alternating_faults_and_successful_actions_cannot_reset_recovery() {
    let mut replies = vec![calls(vec![call(
        "write_file",
        serde_json::json!({"path":"new.txt","content":"saved"}),
    )])];
    for i in 0..converse::RECOVERIES_PER_TURN {
        replies.push(if i % 2 == 0 { fault() } else { empty() });
        replies.push(read());
    }
    replies.push(fault());
    let (report, _, requests, store, id, root) = run(replies, Default::default()).await;
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::RecoveryExhausted)
    );
    assert!(!report.completed);
    assert!(report.edited);
    assert_eq!(report.actions, 1 + converse::RECOVERIES_PER_TURN);
    assert_eq!(
        std::fs::read_to_string(root.path().join("new.txt")).unwrap(),
        "saved"
    );
    assert_eq!(requests.len(), 2 + 2 * converse::RECOVERIES_PER_TURN);
    let events = store.events_for_run(id).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "turn.recovery")
            .count(),
        converse::RECOVERIES_PER_TURN
    );
    let end = events
        .iter()
        .find(|e| e.event_type == "turn.recovery_exhausted")
        .unwrap();
    assert_eq!(end.payload["spent"], converse::RECOVERIES_PER_TURN);
    assert_eq!(end.payload["cause"], "reply_fault");
    assert_eq!(
        report.outcome.terminal,
        pwr_domain::TurnTerminal::Failed {
            class: pwr_domain::TerminalClass::Recovery
        }
    );
}

#[tokio::test]
async fn provider_protocol_and_reasoning_recoveries_share_one_allowance() {
    let faults = vec![
        Err(ProviderError::Unavailable {
            safe_context: "temporary failure".into(),
        }),
        empty(),
        calls(vec![malformed()]),
        Err(ProviderError::ReasoningUnfinished {
            safe_context: "no final answer".into(),
        }),
        looped(),
        Err(ProviderError::Truncated {
            safe_context: "reply cut off".into(),
        }),
    ];
    let mut replies = vec![];
    for fault in faults {
        replies.push(fault);
        replies.push(read());
    }
    replies.push(empty());
    let (report, _, _, store, id, _) = run(replies, Default::default()).await;
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::RecoveryExhausted)
    );
    let events = store.events_for_run(id).unwrap();
    let causes: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "turn.recovery")
        .map(|event| event.payload["cause"].as_str().unwrap())
        .collect();
    assert_eq!(
        causes,
        [
            "backend_fault",
            "silent",
            "malformed_call",
            "reasoning_unfinished",
            "reply_fault",
            "reply_fault"
        ]
    );
}

#[tokio::test]
async fn exhaustion_inside_a_batch_accounts_for_all_declared_calls() {
    let mut replies = vec![];
    for _ in 0..converse::RECOVERIES_PER_TURN {
        replies.push(fault());
        replies.push(read());
    }
    replies.push(calls(vec![
        call(
            "write_file",
            serde_json::json!({"path":"new.txt","content":"saved"}),
        ),
        malformed(),
        call(
            "write_file",
            serde_json::json!({"path":"must-not-exist.txt","content":"wrong"}),
        ),
    ]));
    let (report, messages, _, _, _, root) = run(replies, Default::default()).await;
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::RecoveryExhausted)
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("new.txt")).unwrap(),
        "saved"
    );
    assert!(!root.path().join("must-not-exist.txt").exists());
    let batch = messages
        .iter()
        .rposition(|m| m.role == "assistant" && m.tool_calls.len() == 3)
        .unwrap();
    for call in &messages[batch].tool_calls {
        assert_eq!(
            messages[batch + 1..]
                .iter()
                .filter(|m| m.role == "tool" && m.tool_call_id == call.id)
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn transient_faults_can_recover_and_complete() {
    let (report, _, _, _, _, _) = run(
        vec![fault(), empty(), read(), complete()],
        Default::default(),
    )
    .await;
    assert!(report.completed);
    assert_eq!(report.stopped, None);
}

#[tokio::test]
async fn repetition_recovery_preserves_requested_sampling() {
    for sampling in [
        BTreeMap::new(),
        BTreeMap::from([
            ("presence_penalty".into(), serde_json::json!(0.25)),
            ("presence_context_size".into(), serde_json::json!(64)),
        ]),
    ] {
        let (report, _, requests, _, _, _) =
            run(vec![looped(), read(), complete()], sampling.clone()).await;
        assert!(report.completed);
        for request in requests {
            assert_eq!(
                request.sampling.get("presence_penalty"),
                sampling.get("presence_penalty")
            );
            assert_eq!(
                request.sampling.get("presence_context_size"),
                sampling.get("presence_context_size")
            );
        }
    }
}

#[tokio::test]
async fn gemma_calls_only_in_thinking_have_their_own_stop_reason() {
    let replies = (0..3)
        .map(|_| {
            Ok(ModelChunk {
                thinking: Some("<|tool_call>call:list_tree{}<tool_call|>".into()),
                done: true,
                ..Default::default()
            })
        })
        .collect();
    let (report, _, requests, _, _, _) = run(replies, Default::default()).await;
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::ToolCallInReasoning)
    );
    assert_eq!(report.actions, 0);
    assert_eq!(requests.len(), 3);
}

#[tokio::test]
async fn empty_search_spellings_share_the_conversation_progress_guard() {
    let replies = (0..40)
        .map(|i| {
            calls(vec![call(
                "search",
                serde_json::json!({"query":format!("absent_symbol_{i}")}),
            )])
        })
        .collect();
    let (report, _, requests, _, _, _) = run(replies, Default::default()).await;
    assert_eq!(report.stopped, Some(converse::StopReason::NoProgress));
    assert!(!report.completed);
    assert!(report.actions <= 24, "{} actions", report.actions);
    assert!(requests.len() <= 24);
}

#[tokio::test]
async fn identical_refused_writes_cannot_hide_behind_successful_reads() {
    let mut replies = vec![read()];
    for _ in 0..20 {
        replies.push(calls(vec![call(
            "write_file",
            serde_json::json!({"path":"kept.txt","content":"before"}),
        )]));
        replies.push(read());
    }
    replies.push(complete());
    let (report, _, requests, _, _, root) = run(replies, Default::default()).await;
    assert_eq!(report.stopped, Some(converse::StopReason::NoProgress));
    assert!(!report.completed);
    assert!(requests.len() <= 20, "{} generations", requests.len());
    assert_eq!(
        std::fs::read_to_string(root.path().join("kept.txt")).unwrap(),
        "before"
    );
}

#[tokio::test]
async fn a_real_file_change_allows_a_fresh_recovery_after_a_refusal_warning() {
    let write = |text: &str| {
        calls(vec![call(
            "write_file",
            serde_json::json!({"path":"kept.txt","content":text}),
        )])
    };
    let mut replies = vec![read()];
    for phase in 0..2 {
        for _ in 0..4 {
            replies.push(write("before"));
            replies.push(read());
        }
        if phase == 0 {
            replies.extend([write("after"), read(), write("before"), read()]);
        }
    }
    replies.push(complete());
    let (report, _, _, _, _, root) = run(replies, Default::default()).await;
    assert!(report.completed, "{:?}", report.stopped);
    assert_eq!(
        std::fs::read_to_string(root.path().join("kept.txt")).unwrap(),
        "before"
    );
}
