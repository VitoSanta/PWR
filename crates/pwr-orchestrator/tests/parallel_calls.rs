//! Several calls in one reply are carried out one after another, as a
//! conversation carries them out, rather than refused whole.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Writes two files in one reply, as Ornith-1.5-9B did in suite A3, then
/// completes. Counts the replies it was asked for.
struct TwoWritesProvider {
    turn: Arc<Mutex<usize>>,
    seen: Arc<Mutex<Vec<ModelRequest>>>,
}

#[async_trait]
impl ModelProvider for TwoWritesProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.seen.lock().unwrap().push(request);
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        let write = |path: &str, id: &str| ToolCall {
            name: "write_file".into(),
            arguments: serde_json::json!({"path": path, "content": format!("{path}\n")}),
            id: Some(id.into()),
        };
        let chunk = if *turn == 1 {
            ModelChunk {
                tool_calls: vec![write("money.py", "call_1"), write("invoice.py", "call_2")],
                done: true,
                ..Default::default()
            }
        } else {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "complete".into(),
                    arguments: serde_json::json!({"rationale": "both written"}),
                    id: None,
                }],
                done: true,
                ..Default::default()
            }
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
}

#[test]
fn two_edits_in_one_reply_are_both_carried_out() {
    let root = tempfile::tempdir().unwrap();
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
        messages: vec![ChatMessage {
            role: "user".into(),
            content: "write both".into(),
            ..Default::default()
        }],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let provider = TwoWritesProvider {
        turn: Arc::new(Mutex::new(0)),
        seen: seen.clone(),
    };
    let outcome =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &provider,
                run_id,
                request,
                &policy,
                &[("true".into(), Vec::new())],
                8,
            ));
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(root.path().join("money.py").exists());
    assert!(root.path().join("invoice.py").exists());
    let events = store.events_for_run(run_id).unwrap();
    assert!(
        !events.iter().any(|e| e.event_type == "action.malformed"),
        "the reply was refused"
    );
    // The second write came from the queue, not from asking again: two
    // replies in all, and the second saw each result under its own call id.
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let answered: Vec<Option<String>> = seen[1]
        .messages
        .iter()
        .filter(|message| message.role == "tool")
        .map(|message| message.tool_call_id.clone())
        .collect();
    assert_eq!(
        answered,
        vec![Some("call_1".to_string()), Some("call_2".to_string())]
    );
}
