//! S03 (WEB-07/08): a whole-file replacement refused as a shrink cannot be
//! walked around by deleting the file and writing it again in the same turn.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{DenyWithoutAsking, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::{
    collections::VecDeque,
    sync::{Mutex, atomic::AtomicBool},
};

struct Sequence(Mutex<VecDeque<ModelChunk>>);
#[async_trait]
impl ModelProvider for Sequence {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let reply = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("extra generation");
        Ok(Box::pin(futures_util::stream::iter([Ok(reply)])))
    }
}

fn calls(name: &str, args: serde_json::Value) -> ModelChunk {
    ModelChunk {
        tool_calls: vec![ToolCall {
            name: name.into(),
            arguments: args,
            id: Some(format!("c-{name}")),
        }],
        done: true,
        ..Default::default()
    }
}

async fn turn(replies: Vec<ModelChunk>, existing: &str) -> (tempfile::TempDir, Vec<ChatMessage>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("page.html"), existing).unwrap();
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
    let mut messages = vec![
        ChatMessage::text("system", "work"),
        ChatMessage::text("user", "rewrite the page"),
    ];
    converse::take_turn(
        &Sequence(Mutex::new(replies.into())),
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Development.build(root.path().to_owned()),
        &mut messages,
        16384,
        &[],
        Default::default(),
        pwr_compat::render_tools(&converse::chat_tool_catalog()),
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    (root, messages)
}

#[tokio::test]
async fn deleting_a_file_whose_whole_rewrite_was_just_refused_is_refused() {
    let big: String = (0..40).map(|n| format!("<p>line {n}</p>\n")).collect();
    let hash = pwr_domain::hash_bytes(big.as_bytes());
    let (root, messages) = turn(
        vec![
            calls("read_file", serde_json::json!({"path":"page.html"})),
            calls(
                "write_file",
                serde_json::json!({"path":"page.html","content":"<p>x</p>\n","expected_hash":hash}),
            ),
            calls(
                "delete_path",
                serde_json::json!({"path":"page.html","expected_hash":hash}),
            ),
            ModelChunk {
                content: "stopping".into(),
                done: true,
                ..Default::default()
            },
        ],
        &big,
    )
    .await;
    let transcript: String = messages.iter().map(|m| m.content.as_str()).collect();
    assert!(
        transcript.contains("replaces the whole file"),
        "{transcript}"
    );
    assert!(
        transcript.contains("deleting it to write it again is the same change"),
        "{transcript}"
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("page.html")).unwrap(),
        big
    );
}

#[tokio::test]
async fn deleting_a_file_never_refused_as_a_shrink_still_works() {
    let big: String = (0..40).map(|n| format!("<p>line {n}</p>\n")).collect();
    let hash = pwr_domain::hash_bytes(big.as_bytes());
    let (root, _) = turn(
        vec![
            calls("read_file", serde_json::json!({"path":"page.html"})),
            calls(
                "delete_path",
                serde_json::json!({"path":"page.html","expected_hash":hash}),
            ),
            ModelChunk {
                content: "done".into(),
                done: true,
                ..Default::default()
            },
        ],
        &big,
    )
    .await;
    assert!(!root.path().join("page.html").exists());
}
