//! A scripted turn (plan W2.9): one call PWR decided, taken through the real
//! turn. What matters is what does not change -- the call meets the same
//! policy as a model's own -- and that no model is asked for anything.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::executor::{Scripted, ScriptedTurn};
use pwr_orchestrator::{DenyWithoutAsking, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::PolicyProfile;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// The model behind the script: counts what it is asked.
#[derive(Default)]
struct Model {
    asked: AtomicUsize,
    cancellable: AtomicUsize,
}

#[async_trait]
impl ModelProvider for &Model {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat_cancellable(
        &self,
        request: ModelRequest,
        _: pwr_provider::Cancel,
    ) -> Result<ModelStream, ProviderError> {
        // A backend with its own way of abandoning a reply, as MLX has.
        self.cancellable.fetch_add(1, Ordering::Relaxed);
        self.chat(request).await
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: "the model's own answer".into(),
            done: true,
            ..Default::default()
        })])))
    }
}

struct Ran {
    report: converse::TurnReport,
    messages: Vec<ChatMessage>,
    asked: usize,
    cancellable: usize,
}

async fn run(root: &std::path::Path, scripted: Option<(&str, serde_json::Value)>) -> Ran {
    let model = Model::default();
    let provider = Scripted::new(
        &model,
        scripted.map(|(name, arguments)| ScriptedTurn {
            call: ToolCall {
                name: name.into(),
                arguments,
                id: None,
            },
            said: "Proposed; checking it.".into(),
        }),
    );
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
        &PolicyProfile::Safe.build(root.to_owned()),
        &mut messages,
        16384,
        &[],
        Default::default(),
        serde_json::json!([]),
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    Ran {
        report,
        messages,
        asked: model.asked.load(Ordering::Relaxed),
        cancellable: model.cancellable.load(Ordering::Relaxed),
    }
}

#[tokio::test]
async fn a_scripted_file_is_written_by_the_turn_and_no_model_is_asked() {
    let root = tempfile::tempdir().unwrap();
    let ran = run(
        root.path(),
        Some((
            "write_file",
            serde_json::json!({"path": "todo.py", "content": "print(1)\n"}),
        )),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("todo.py")).unwrap(),
        "print(1)\n"
    );
    assert!(ran.report.edited);
    assert_eq!(ran.report.actions, 1);
    assert_eq!(ran.asked, 0);
    assert_eq!(
        ran.messages.last().unwrap().content,
        "Proposed; checking it."
    );
}

#[tokio::test]
async fn a_scripted_overwrite_is_bound_to_the_version_it_replaces() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("dates.ts");
    std::fs::write(&path, "old\n").unwrap();
    let replace = |hash: String| serde_json::json!({"path": "dates.ts", "expected_hash": hash, "replacement": "new\n"});
    // The file changed after the proposal was read: the turn refuses, as it
    // would refuse a model.
    let stale = run(
        root.path(),
        Some(("apply_replace", replace(pwr_domain::hash_bytes("other\n")))),
    )
    .await;
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old\n");
    assert!(!stale.report.edited);
    assert_eq!(
        stale.asked, 0,
        "a refusal is not handed to the model to retry"
    );
    let fresh = run(
        root.path(),
        Some(("apply_replace", replace(pwr_domain::hash_bytes("old\n")))),
    )
    .await;
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\n");
    assert!(fresh.report.edited);
}

#[tokio::test]
async fn a_scripted_call_meets_the_policy_like_any_other() {
    let root = tempfile::tempdir().unwrap();
    let outside = run(
        root.path(),
        Some((
            "write_file",
            serde_json::json!({"path": "../escaped.py", "content": "x\n"}),
        )),
    )
    .await;
    assert!(!outside.report.edited);
    assert!(!root.path().parent().unwrap().join("escaped.py").exists());
    std::fs::write(root.path().join("made.py"), "x\n").unwrap();
    let removed = run(
        root.path(),
        Some((
            "delete_path",
            serde_json::json!({"path": "made.py", "expected_hash": pwr_domain::hash_bytes("x\n")}),
        )),
    )
    .await;
    assert!(removed.report.edited);
    assert!(!root.path().join("made.py").exists());
}

#[tokio::test]
async fn with_nothing_scripted_the_model_answers_as_itself() {
    let root = tempfile::tempdir().unwrap();
    let ran = run(root.path(), None).await;
    assert_eq!(ran.asked, 1);
    // Through the backend's own cancellable entry, not the default one that
    // only drops the stream: the wrapper is on every turn's path.
    assert_eq!(ran.cancellable, 1);
    assert_eq!(
        ran.messages.last().unwrap().content,
        "the model's own answer"
    );
}

#[tokio::test]
async fn putting_back_a_much_shorter_file_takes_the_guard_s_own_way_round() {
    // What the proposals phase does when a refused proposal was more than
    // twice the size of the file it replaced: the overwrite back is refused by
    // the shrink guard, and a delete by hash followed by a write is not.
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("invoice.ts");
    let long = "const line = 1;\n".repeat(200);
    std::fs::write(&path, &long).unwrap();
    let hash = pwr_domain::hash_bytes(&long);
    let back = run(
        root.path(),
        Some((
            "apply_replace",
            serde_json::json!({"path": "invoice.ts", "expected_hash": hash, "replacement": "const line = 1;\n"}),
        )),
    )
    .await;
    assert!(!back.report.edited);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), long);
    let removed = run(
        root.path(),
        Some((
            "delete_path",
            serde_json::json!({"path": "invoice.ts", "expected_hash": hash}),
        )),
    )
    .await;
    assert!(removed.report.edited);
    let written = run(
        root.path(),
        Some((
            "write_file",
            serde_json::json!({"path": "invoice.ts", "content": "const line = 1;\n"}),
        )),
    )
    .await;
    assert!(written.report.edited);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "const line = 1;\n");
}
