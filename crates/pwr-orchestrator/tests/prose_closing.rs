//! A closing call written as prose -- `Decline`, then `Rationale: ...` -- taken
//! through the real turn: it ends the turn as the call it is, and only where
//! that tool is offered.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_orchestrator::{DenyWithoutAsking, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::sync::atomic::AtomicBool;

struct Says(&'static str);

#[async_trait]
impl ModelProvider for Says {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: self.0.into(),
            done: true,
            ..Default::default()
        })])))
    }
}

async fn turn(says: &'static str, tools: serde_json::Value) -> converse::TurnReport {
    let root = tempfile::tempdir().unwrap();
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
        ChatMessage::text("user", "build the site"),
    ];
    converse::take_turn(
        &Says(says),
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        16384,
        &[],
        Default::default(),
        tools,
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap()
}

const SAID: &str = "Decline\n\nRationale: there is no runtime here to build with.";

#[tokio::test]
async fn a_decline_written_as_prose_ends_the_turn_as_a_decline() {
    let offered = pwr_compat::render_tools(&converse::chat_tool_catalog());
    assert!(offered.to_string().contains("\"decline\""));
    let report = turn(SAID, offered).await;
    assert!(report.declined);
    assert_eq!(report.answer, "there is no runtime here to build with.");
}

#[tokio::test]
async fn where_decline_is_not_offered_the_same_words_are_an_answer() {
    let report = turn(SAID, serde_json::json!([])).await;
    assert!(!report.declined);
    assert!(report.answer.starts_with("Decline"));
}
