//! H04 — does the original requirement survive a long run?
//!
//! The guide asks this of the model: after thirty tool calls, has it lost the
//! goal? That half needs a deployment. This half does not, and it comes first:
//! the deployment cannot hold a requirement the harness has stopped sending.
//!
//! The failure mode is not hypothetical. `compact_history` carries a comment
//! about a fixed index that "silently replaced the real task" with the session
//! ledger, and nothing in the suite held it to that afterwards. A run whose
//! goal has been quietly swapped for a summary of its own actions still looks
//! like a run; it just answers a different question.

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

const TASK: &str = "Rename total() to product() in src/math.rs and update its caller. \
                    Do not change behaviour and change no other file.";

/// Reads a file every turn, so the history grows until compaction has to run,
/// and records what it was sent each time.
struct WatchfulProvider {
    turn: Arc<Mutex<usize>>,
    seen: Arc<Mutex<Vec<Vec<ChatMessage>>>>,
    reads: usize,
}

#[async_trait]
impl ModelProvider for WatchfulProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.seen.lock().unwrap().push(request.messages.clone());
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        let chunk = if *turn <= self.reads {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "read_file".into(),
                    // A different file each turn: reading the same one over
                    // and over is a loop, and the loop detector is right to
                    // end that run. This has to be work, or it measures the
                    // detector instead of the goal.
                    arguments: serde_json::json!({"path": format!("padding_{turn:02}.txt")}),
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

fn run_long(reads: usize) -> (Store, pwr_domain::Id, Vec<Vec<ChatMessage>>) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    // Big enough that a few reads push the history past its budget.
    for file in 1..=40 {
        let padding: String = (0..400)
            .map(|n| format!("file {file} line {n} of padding\n"))
            .collect();
        std::fs::write(root.path().join(format!("padding_{file:02}.txt")), padding).unwrap();
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
    let seen = Arc::new(Mutex::new(Vec::new()));
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
        messages: {
            use pwr_orchestrator::context::{Section, SectionKind, compile};
            compile(
                vec![
                    Section::new(SectionKind::System, "Use tools to do the task."),
                    Section::new(
                        SectionKind::SessionLedger,
                        "Previous work: inspected the build.",
                    ),
                    Section::new(
                        SectionKind::RepositoryExcerpts,
                        "Repository excerpts: padding files.",
                    ),
                    Section::new(SectionKind::Task, TASK),
                ],
                2048,
            )
            .0
        },
        // Small on purpose: compaction has to run, and often.
        context_tokens: 2048,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(pwr_orchestrator::run_action_loop(
            &store,
            &WatchfulProvider {
                turn: Arc::new(Mutex::new(0)),
                seen: seen.clone(),
                reads,
            },
            run_id,
            request,
            &policy,
            &[("true".into(), Vec::new())],
            40,
        ));
    let seen = seen.lock().unwrap().clone();
    (store, run_id, seen)
}

#[test]
fn the_original_requirement_is_still_sent_after_thirty_tool_calls() {
    let (store, run_id, seen) = run_long(30);
    assert!(
        seen.len() >= 30,
        "the fixture made only {} turns",
        seen.len()
    );

    let compactions = store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.event_type == "context.compacted")
        .count();
    assert!(
        compactions > 0,
        "nothing was compacted, so this proves nothing about compaction"
    );

    // Every turn, not just the last: a goal that survives to the end but went
    // missing in the middle was missing for the decisions taken there.
    for (turn, messages) in seen.iter().enumerate() {
        let carries_task = messages.iter().any(|m| m.content.contains(TASK));
        assert!(
            carries_task,
            "turn {turn} of {} was sent without the original requirement, after \
             {compactions} compaction(s); the run continued and answered a \
             different question",
            seen.len()
        );
    }
}

/// And the goal is sent as the user's own words rather than as a paraphrase.
/// A summary of the task is a second author's version of it, and the run would
/// be measured against the summary.
#[test]
fn the_requirement_is_sent_verbatim_not_summarised() {
    let (_, _, seen) = run_long(30);
    let last = seen.last().expect("no turns");
    let user_messages: Vec<&ChatMessage> = last.iter().filter(|m| m.role == "user").collect();
    assert!(
        user_messages.iter().any(|m| m.content.contains(TASK)),
        "the user message no longer contains the task as given:\n{:?}",
        user_messages.iter().map(|m| &m.content).collect::<Vec<_>>()
    );
}
