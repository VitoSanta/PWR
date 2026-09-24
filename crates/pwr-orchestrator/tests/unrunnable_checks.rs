//! A check that could not run here is not a check that failed.
//!
//! Observed on more-itertools at `247e15b3` with `qwen/qwen3.6-35b-a3b`: CI
//! declares `make requirements check`, its first target pip-installs, the
//! sandbox denies the network, and the check failed on every turn whatever the
//! deployment did. The deployment made the correct fix on its tenth action and
//! the run ended `action budget of 26 exhausted before verified completion`,
//! because acceptance required a check that could never pass here.
//!
//! The baseline is captured before the deployment acts, so an environment
//! failure in it cannot be the deployment's doing. That is what makes it safe
//! to exempt there and nowhere else.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::Mutex;
use std::time::Duration;

/// Reads a file, then says it is done.
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

fn run_with(check: (String, Vec<String>)) -> (Store, pwr_domain::Id, bool, bool) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one\n").unwrap();
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
        replies: Mutex::new(vec![
            r#"{"capability":"read_file","path":"code.rs"}"#,
            r#"{"capability":"complete","rationale":"nothing needed changing"}"#,
        ]),
    };
    let result =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &provider,
                run_id,
                request,
                &policy,
                std::slice::from_ref(&check),
                12,
            ));
    let (verified, verifiable) = match &result {
        Ok(outcome) => (outcome.verified, outcome.verifiable),
        Err(_) => (false, true),
    };
    (store, run_id, verified, verifiable)
}

/// A command that runs, exits non-zero, and says why in the words a denied
/// sandbox and a missing interpreter use.
fn unrunnable_check() -> (String, Vec<String>) {
    (
        "sh".into(),
        vec![
            "-c".into(),
            "echo 'make: python: No such file or directory' >&2; exit 2".into(),
        ],
    )
}

/// A command that runs and disagrees, which is a failing check and stays one.
fn failing_check() -> (String, Vec<String>) {
    (
        "sh".into(),
        vec!["-c".into(), "echo 'assert 1 == 2' >&2; exit 1".into()],
    )
}

#[test]
fn a_check_that_cannot_run_here_is_named_and_exempted() {
    let (store, run_id, _, verifiable) = run_with(unrunnable_check());
    let events = store.events_for_run(run_id).unwrap();
    let named = events
        .iter()
        .find(|event| event.event_type == "verification.unrunnable")
        .expect("the unrunnable check was never named");
    assert!(
        named.payload["why"]
            .as_str()
            .unwrap_or_default()
            .contains("before the run acted"),
        "{:?}",
        named.payload
    );
    // Nothing runnable is the same position as nothing declared, and the run
    // has an honest ending for that already.
    assert!(
        !verifiable,
        "a run whose only check could not run reported itself verifiable"
    );
}

/// Two ways to have no verifier, and they are not the same fact. A workspace
/// that declares none has nothing to repair; one whose checks could not run has
/// an environment to repair. Observed after the first fix landed: the run told
/// a repository with a Makefile and a CI workflow that it "declares no checks".
#[test]
fn having_no_runnable_check_is_not_the_same_as_declaring_none() {
    let (store, run_id, _, _) = run_with(unrunnable_check());
    let events = store.events_for_run(run_id).unwrap();
    let said: Vec<String> = events
        .iter()
        .filter(|event| event.event_type == "task.transition")
        .filter_map(|event| {
            event.payload["detail"]
                .as_str()
                .map(std::string::ToString::to_string)
        })
        .collect();
    assert!(
        said.iter()
            .any(|reason| reason.contains("failed to run in this environment")),
        "the run did not say which of the two it was: {said:?}"
    );
    assert!(
        !said
            .iter()
            .any(|reason| reason.contains("no deterministic verifier was available")),
        "a repository that declares a check was told it declares none: {said:?}"
    );
}

/// The distinction the exemption must not blur. A repair task starts with a red
/// check by design, and exempting that would score an unfinished repair as
/// verified without it having been done.
#[test]
fn a_check_that_fails_on_the_code_is_still_required() {
    let (store, run_id, verified, verifiable) = run_with(failing_check());
    let events = store.events_for_run(run_id).unwrap();
    assert!(
        !events
            .iter()
            .any(|event| event.event_type == "verification.unrunnable"),
        "a failing assertion was exempted as an environment failure"
    );
    assert!(verifiable, "a runnable check was reported as no verifier");
    assert!(!verified, "a run whose check fails is not verified");
}
