//! A goal's plan (plan W2.14) through the real turn: written by the model,
//! held by the harness, shown before every reply and kept out of the
//! conversation.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{DenyWithoutAsking, board, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

/// Replies played in order; what it was sent is kept.
struct Recorder {
    replies: Mutex<VecDeque<ModelChunk>>,
    sent: Mutex<Vec<Vec<ChatMessage>>>,
}

#[async_trait]
impl ModelProvider for Recorder {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.sent.lock().unwrap().push(request.messages);
        let mut chunk = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(ModelChunk {
                content: "nothing more to do".into(),
                ..Default::default()
            });
        chunk.done = true;
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
}

fn plan_call(steps: serde_json::Value) -> ModelChunk {
    ModelChunk {
        tool_calls: vec![ToolCall {
            name: board::TOOL.into(),
            arguments: serde_json::json!({"steps": steps}),
            id: Some("p1".into()),
        }],
        ..Default::default()
    }
}

struct Ran {
    report: converse::TurnReport,
    kept: Vec<ChatMessage>,
    sent: Vec<Vec<ChatMessage>>,
}

async fn run(replies: Vec<ModelChunk>, continuity: &converse::Continuity) -> Ran {
    let root = tempfile::tempdir().unwrap();
    let provider = Recorder {
        replies: Mutex::new(replies.into()),
        sent: Mutex::default(),
    };
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "make the tests pass"),
    ];
    let report = converse::take_turn(
        &provider,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        16384,
        &[],
        Default::default(),
        serde_json::json!([]),
        &AtomicBool::new(false),
        continuity,
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    Ran {
        report,
        kept: messages,
        sent: provider.sent.into_inner().unwrap(),
    }
}

fn with_plan() -> converse::Continuity {
    converse::Continuity {
        plan: Some(Default::default()),
        ..Default::default()
    }
}

#[tokio::test]
async fn a_plan_is_recorded_shown_before_the_next_reply_and_kept_out_of_the_conversation() {
    let continuity = with_plan();
    let ran = run(
        vec![plan_call(serde_json::json!([
            {"step": "fix rounding in src/money.ts", "status": "in_progress"},
            {"step": "run the tests", "status": "pending"},
        ]))],
        &continuity,
    )
    .await;
    // Before the first reply there is no plan, and the model is told so at
    // the end of the person's own message, not in a second one.
    let first = ran.sent[0].last().unwrap();
    assert_eq!(first.role, "user");
    assert!(first.content.starts_with("make the tests pass"));
    assert!(first.content.ends_with(board::NO_PLAN));
    // Before the second, the plan as written.
    let second = ran.sent[1].last().unwrap();
    assert_eq!(second.role, "user");
    assert!(
        second
            .content
            .starts_with("YOUR PLAN, as you last wrote it")
    );
    assert!(
        second
            .content
            .contains("[>] 1. fix rounding in src/money.ts")
    );
    assert!(second.content.contains("[ ] 2. run the tests"));
    // The goal holds it for its next turn.
    let held = continuity
        .plan
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(held.steps.len(), 2);
    // What is kept is the conversation: the call, its result, the answer.
    assert!(
        ran.kept
            .iter()
            .all(|message| !message.content.contains("YOUR PLAN")
                && !message.content.contains("NO PLAN YET"))
    );
    assert_eq!(ran.kept[1].content, "make the tests pass");
    assert!(
        ran.kept
            .iter()
            .any(|message| message.content.contains("recorded"))
    );
    assert_eq!(
        ran.report.actions, 1,
        "writing a plan is bounded like any action"
    );
    assert_eq!(ran.kept.last().unwrap().content, "nothing more to do");
}

#[tokio::test]
async fn a_plan_that_cannot_be_held_to_is_refused_and_nothing_is_recorded() {
    let continuity = with_plan();
    let ran = run(
        vec![plan_call(serde_json::json!([
            {"step": "a", "status": "in_progress"},
            {"step": "b", "status": "in_progress"},
        ]))],
        &continuity,
    )
    .await;
    assert!(continuity.plan.as_ref().unwrap().lock().unwrap().is_none());
    assert!(
        ran.kept
            .iter()
            .any(|message| message.content.contains("exactly one step is in_progress"))
    );
    assert!(
        ran.sent[1]
            .last()
            .unwrap()
            .content
            .ends_with(board::NO_PLAN)
    );
}

#[tokio::test]
async fn a_goal_without_a_plan_is_sent_its_conversation_and_nothing_else() {
    let ran = run(Vec::new(), &converse::Continuity::default()).await;
    assert_eq!(ran.sent[0].last().unwrap().content, "make the tests pass");
    assert_eq!(ran.sent[0].len(), 2);
}
