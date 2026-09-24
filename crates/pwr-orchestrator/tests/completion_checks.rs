//! Completion discovers checks again, for the verifiers a task creates -- and
//! only for those.
//!
//! Measured in a verifier-supplied campaign on `glm-4.7-flash:q8_0` against
//! `external-v1`: the corpus supplied `python3 -m unittest`, the mode excludes
//! check discovery by declaration, and completion discovered more-itertools'
//! CI target `make requirements check` anyway. It cannot run in the sandbox.
//! Three fixes that passed the hidden verifier and one repository answer that
//! matched ended "verification failed and recovery budget exhausted", and the
//! campaign scored 0 of 5 on work that was mostly done.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::Mutex;
use std::time::Duration;

struct Worker {
    replies: Mutex<Vec<&'static str>>,
}

#[async_trait]
impl ModelProvider for Worker {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
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

/// Runs `replies` in a workspace prepared by `setup`, with `supplied` as the
/// only check the caller chose, and returns whether the run verified.
fn run(
    setup: impl FnOnce(&std::path::Path),
    replies: Vec<&'static str>,
    supplied: (String, Vec<String>),
) -> (Store, pwr_domain::Id, bool) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one\n").unwrap();
    setup(root.path());
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["sh".into()],
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
        messages: vec![ChatMessage::text("user", "fix it")],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let provider = Worker {
        replies: Mutex::new(replies),
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
                std::slice::from_ref(&supplied),
                12,
            ));
    let verified = outcome.map(|result| result.verified).unwrap_or(false);
    (store, run_id, verified)
}

fn passing() -> (String, Vec<String>) {
    ("sh".into(), vec!["-c".into(), "exit 0".into()])
}

fn completion_commands(store: &Store, run_id: pwr_domain::Id) -> Vec<String> {
    store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|event| event.event_type == "verification.result")
        .flat_map(|event| {
            event.payload["after"]["checks"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|check| check["command"].as_str().map(str::to_string))
        .collect()
}

/// A check the workspace already had, which the caller did not choose, is not
/// the caller's to be held to at completion -- here one that cannot run.
#[test]
fn a_check_the_caller_left_out_is_not_added_back_at_completion() {
    let (store, run_id, verified) = run(
        |root| {
            std::fs::create_dir_all(root.join(".pwr")).unwrap();
            std::fs::write(
                root.join(".pwr/checks.json"),
                r#"{"checks":[{"executable":"sh","args":["-c","echo 'make: python: No such file or directory' >&2; exit 2"]}]}"#,
            )
            .unwrap();
        },
        vec![
            r#"{"capability":"read_file","path":"code.rs"}"#,
            r#"{"capability":"complete","rationale":"nothing needed changing"}"#,
        ],
        passing(),
    );
    let ran = completion_commands(&store, run_id);
    assert_eq!(
        ran,
        vec!["sh -c exit 0".to_string()],
        "completion ran checks the caller did not choose"
    );
    assert!(
        verified,
        "the supplied check passed and the run was not verified"
    );
}

/// The case completion discovery exists for, which must survive the fix: a
/// verifier the task itself brought into being is required to pass.
#[test]
fn a_verifier_the_task_created_is_still_required() {
    let (store, run_id, verified) = run(
        |_| {},
        vec![
            r#"{"capability":"write_file","path":"index.html","content":"<script src=\"app.js\"></script>"}"#,
            r#"{"capability":"complete","rationale":"added a page"}"#,
        ],
        passing(),
    );
    let ran = completion_commands(&store, run_id);
    assert!(
        ran.iter().any(|command| command == "pwr:web-assets"),
        "a verifier created during the run was not run at completion: {ran:?}"
    );
    assert!(
        !verified,
        "a page that loads a file that does not exist was verified"
    );
}
