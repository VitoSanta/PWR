//! H2's contract, end to end through B1's loop with a scripted deployment.
//!
//! A revision delivered early and a large file read later force a compaction
//! that drops the exchange carrying the revision. Today's compaction then sends
//! a prompt that no longer states what the task became; the evidence-state
//! treatment still does. An edit made behind the run's back is recorded, and the
//! compaction event says whether any kept tool result carried content the file
//! no longer has.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_orchestrator::evidence::{ActionBoundary, BoundaryEvent, ContextPolicy};
use pwr_orchestrator::{DenyWithoutAsking, RunTuning};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const REVISION: &str = "the limit is 20, not 10";

struct Script {
    replies: Mutex<Vec<&'static str>>,
    prompts: Mutex<Vec<Vec<ChatMessage>>>,
}

#[async_trait]
impl ModelProvider for Script {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.prompts.lock().unwrap().push(request.messages.clone());
        let reply = {
            let mut left = self.replies.lock().unwrap();
            if left.is_empty() {
                r#"{"capability":"complete","rationale":"done"}"#
            } else {
                left.remove(0)
            }
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: reply.into(),
            done: true,
            ..Default::default()
        })])))
    }
}

/// A revision after the first action; an edit to `small.py` after the second.
struct Injections {
    root: PathBuf,
}

impl ActionBoundary for Injections {
    fn after_action(&self, step: u8) -> Vec<BoundaryEvent> {
        match step {
            1 => vec![BoundaryEvent::Revision(REVISION.into())],
            2 => {
                std::fs::write(self.root.join("small.py"), "limit = 15\n").unwrap();
                vec![BoundaryEvent::ExternalEdit {
                    path: "small.py".into(),
                    detail: "applied".into(),
                }]
            }
            _ => Vec::new(),
        }
    }
}

fn run(policy: ContextPolicy) -> (Store, pwr_domain::Id, Vec<Vec<ChatMessage>>) {
    let dir = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("small.py"), "limit = 10\n").unwrap();
    let big: String = (1..=500)
        .map(|n| format!("def f{n}(): return {n}\n"))
        .collect();
    std::fs::write(root.join("big.py"), big).unwrap();
    let tools = ToolPolicy {
        root: root.clone(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["sh".into()],
        output_limit: 64 * 1024,
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
        messages: vec![ChatMessage::text("user", "set the limit in small.py")],
        context_tokens: 2048,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let provider = Script {
        replies: Mutex::new(vec![
            r#"{"capability":"read_file","path":"small.py"}"#,
            // Each read is bounded to about a kilobyte at this window, so it
            // takes several before the history passes its budget.
            r#"{"capability":"read_file","path":"big.py","first_line":1,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":41,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":81,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":121,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":161,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":201,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":241,"max_lines":40}"#,
            r#"{"capability":"read_file","path":"big.py","first_line":281,"max_lines":40}"#,
        ]),
        prompts: Mutex::new(Vec::new()),
    };
    let tuning = RunTuning {
        context_policy: policy,
        boundary: Some(Arc::new(Injections { root: root.clone() })),
        ..RunTuning::default()
    };
    let check = ("sh".to_string(), vec!["-c".to_string(), "true".to_string()]);
    let _ = tokio::runtime::Runtime::new().unwrap().block_on(
        pwr_orchestrator::run_action_loop_with_prompt_budget_and_context_tiers(
            &store,
            &provider,
            run_id,
            request,
            &tools,
            std::slice::from_ref(&check),
            20,
            &pwr_verify::RecoveryBudget::default(),
            &[],
            &DenyWithoutAsking,
            false,
            &tuning,
        ),
    );
    let prompts = provider.prompts.into_inner().unwrap();
    (store, run_id, prompts)
}

fn compactions(store: &Store, run_id: pwr_domain::Id) -> Vec<serde_json::Value> {
    store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "context.compacted")
        .map(|event| event.payload)
        .collect()
}

#[test]
fn a_revision_survives_compaction_only_under_the_evidence_state() {
    let (store, run_id, prompts) = run(ContextPolicy::Current);
    let events = store.events_for_run(run_id).unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "task.revision")
    );
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "injection.external_edit")
    );
    let compacted = compactions(&store, run_id);
    assert!(
        !compacted.is_empty(),
        "the fixture never forced a compaction"
    );
    assert!(compacted.iter().all(|c| c["policy"] == "current"));
    assert!(
        compacted
            .iter()
            .all(|c| c.get("stale_file_contents_kept").is_some())
    );
    // The revision reached the deployment when it was made...
    assert!(
        prompts[1].iter().any(|m| m.content.contains(REVISION)),
        "the revision was never delivered"
    );
    // ...and today's compaction lost it.
    let last = prompts.last().unwrap();
    assert!(
        !last.iter().any(|m| m.content.contains(REVISION)),
        "the control kept the revision, so this fixture does not test H2"
    );

    let (store, run_id, prompts) = run(ContextPolicy::EvidenceState { share_percent: 60 });
    let compacted = compactions(&store, run_id);
    assert!(compacted.iter().all(|c| c["policy"] == "evidence-state-60"));
    let last = prompts.last().unwrap();
    assert!(
        last.iter().any(|m| m.content.contains(REVISION)),
        "the treatment lost the revision too"
    );
    let metrics = pwr_eval_metrics(&store, run_id);
    assert_eq!(metrics.0, 1, "revisions delivered");
    assert_eq!(metrics.1, 1, "external edits applied");
    assert_eq!(metrics.2, 0, "the treatment kept stale file contents");
}

/// The fold `pwr-eval` reports, done here from the same events so this crate
/// does not depend on the evaluator.
fn pwr_eval_metrics(store: &Store, run_id: pwr_domain::Id) -> (usize, usize, usize) {
    let events = store.events_for_run(run_id).unwrap();
    let count = |kind: &str| events.iter().filter(|e| e.event_type == kind).count();
    let stale = events
        .iter()
        .filter(|e| e.event_type == "context.compacted")
        .map(|e| e.payload["stale_file_contents_kept"].as_u64().unwrap_or(0) as usize)
        .sum();
    (
        count("task.revision"),
        count("injection.external_edit"),
        stale,
    )
}
