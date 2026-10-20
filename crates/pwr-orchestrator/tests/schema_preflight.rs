//! S03 preflight: the tool schemas ride in every request and in none of the
//! messages, so a long history is judged with their cost from the first turn,
//! not only after the backend has counted one.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_orchestrator::{DenyWithoutAsking, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::sync::atomic::AtomicBool;

struct Answers;
#[async_trait]
impl ModelProvider for Answers {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: "ok".into(),
            done: true,
            ..Default::default()
        })])))
    }
}

/// How many times the turn compacted, given tool schemas of this size.
async fn compactions(schema_chars: usize) -> usize {
    let root = tempfile::tempdir().unwrap();
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "gemma-4-26b-a4b-it".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    // A history that fills most of a 32k window by itself: ~100 messages of
    // ~100 tokens each is nowhere near, so use fewer, larger ones.
    let mut messages = vec![
        ChatMessage::text("system", "work"),
        ChatMessage::text("user", "first request"),
    ];
    for n in 0..30 {
        messages.push(ChatMessage::text(
            if n % 2 == 0 { "assistant" } else { "user" },
            format!("{n}: {}", "word ".repeat(560)),
        ));
    }
    messages.push(ChatMessage::text("user", "now continue"));
    let tools = serde_json::json!([{"name": "t", "description": "d".repeat(schema_chars)}]);
    let mut compacted = 0usize;
    converse::take_turn(
        &Answers,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        32_768,
        &[],
        Default::default(),
        tools,
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |step| {
            if matches!(step, converse::TurnStep::Compacted(_)) {
                compacted += 1;
            }
        },
    )
    .await
    .unwrap();
    compacted
}

#[tokio::test]
async fn a_history_that_fits_only_without_the_tool_schemas_is_compacted_before_the_first_request() {
    // ~30 * 700 tokens = ~21k estimated: under the compaction trigger on its
    // own, over it once ~6k tokens of schemas are counted.
    let light = compactions(40).await;
    let heavy = compactions(26_000).await;
    assert_eq!(light, 0, "no schema, no reason to compact");
    assert!(
        heavy >= 1,
        "the schemas' cost was left out of the first turn"
    );
}
