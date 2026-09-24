//! Compaction must not discard the result the deployment is about to act on.
//!
//! Measured on `monotonic-table`: a 96 KB file, a 64 KB read bound, and a
//! history budget of half a 32,768-token context. One read at the bound
//! estimates at 16,384 tokens and the budget is 16,384 — so a single maximal
//! read consumes the whole budget exactly, compaction fires before the next
//! request is built, and the deployment never sees what it read.
//!
//! The run made thirty-nine turns and eighteen compactions, peaked at 3,697
//! prompt tokens against 32,768 authorised, and ended on the no-progress
//! detector. It was reading a file it could not keep.

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

/// Reads the big file, then reports whether the read came back to it.
struct ReadingProvider {
    turn: Arc<Mutex<usize>>,
    /// What the second request contained, recorded for the assertion.
    saw_the_read: Arc<Mutex<Option<bool>>>,
}

#[async_trait]
impl ModelProvider for ReadingProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        // Compaction refuses to run on three messages or fewer, so the big read
        // has to arrive with history already behind it -- otherwise the test
        // passes without compaction ever firing, which is what the first draft
        // of it did.
        if *turn == 5 {
            // The needle is one line deep inside the file that was just read.
            let seen = request
                .messages
                .iter()
                .any(|m| m.content.contains("NEEDLE_AT_LINE_4211"));
            *self.saw_the_read.lock().unwrap() = Some(seen);
        }
        let chunk = if *turn <= 3 {
            // Small reads first, to build the history compaction needs.
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": format!("small_{turn}.rs")}),
                    id: None,
                }],
                done: true,
                ..Default::default()
            }
        } else if *turn == 4 {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "table.rs"}),
                    id: None,
                }],
                done: true,
                ..Default::default()
            }
        } else {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "complete".into(),
                    arguments: serde_json::json!({"rationale": "done"}),
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
fn a_read_survives_to_the_turn_that_must_use_it() {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    // Large enough that one read fills the history budget, which is what the
    // measured task did.
    let mut table = String::from("pub const T: [(u32, i64); 5200] = [\n");
    for i in 0..5200 {
        // Inside the first 64 KB, so the read returns it: past that the read
        // itself truncates, and the test would be measuring the read bound
        // rather than what compaction does with the result.
        if i == 100 {
            table.push_str("    (100, 0), // NEEDLE_AT_LINE_4211\n");
        } else {
            table.push_str(&format!("    ({i}, {}),\n", i * 8));
        }
    }
    table.push_str("];\n");
    std::fs::write(root.path().join("table.rs"), &table).unwrap();
    for n in 1..=3 {
        let filler: String = (0..400)
            .map(|i| format!("// small {n} line {i}\n"))
            .collect();
        std::fs::write(root.path().join(format!("small_{n}.rs")), filler).unwrap();
    }

    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["true".into()],
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let saw = Arc::new(Mutex::new(None));
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
            content: "find the entry that is out of order".into(),
            ..Default::default()
        }],
        // The measured context, so the measured budget.
        context_tokens: 32768,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(pwr_orchestrator::run_action_loop(
            &store,
            &ReadingProvider {
                turn: Arc::new(Mutex::new(0)),
                saw_the_read: saw.clone(),
            },
            run_id,
            request,
            &policy,
            &[("true".into(), Vec::new())],
            8,
        ));

    let saw = saw.lock().unwrap().expect("the second turn never happened");
    assert!(
        saw,
        "the file was read and the read was compacted away before the \
         deployment could use it: a read the harness allows is a read it \
         immediately discards, and a task that must hold a file cannot be done"
    );
}
