//! A reply that never stops is not a run that must end.
//!
//! The conversation has bounded this since the chunk bound was raised: it tells
//! the deployment its reply ran away, asks for less per turn, and keeps the work
//! already done. The scripted run -- the path the benchmarks measure -- let it
//! be fatal, so the measured path was the weaker of the two.
//!
//! Observed on `qwen/qwen3.6-35b-a3b`: two of four `external-v1` tasks ended
//! `provider reply was truncated: reply exceeded the chunk bound`, one of them
//! on its first turn after 8,488 characters of thinking. A campaign that loses
//! half its tasks to a recovery the product already has is not measuring the
//! product.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::Mutex;
use std::time::Duration;

/// Runs away for the first `runaway` replies, then works.
struct Runaway {
    left: Mutex<usize>,
    replies: Mutex<Vec<&'static str>>,
    /// What reached the deployment. The run tells it through the message list
    /// rather than through an event, so the audit records that a runaway reply
    /// happened and not what was said about it -- and "was it told" can only be
    /// answered from the next request.
    requests: Mutex<Vec<ModelRequest>>,
}

#[async_trait]
impl ModelProvider for Runaway {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let runaway = {
            let mut left = self.left.lock().unwrap();
            let runaway = *left > 0;
            *left = left.saturating_sub(1);
            runaway
        };
        if runaway {
            // Truncated on the stream, which is the only way it happens: it is
            // what `collect_reply` returns when the chunks end without a
            // terminal one. Returning it from `chat` instead would exercise the
            // stream-opening match, which never sees this error -- and the
            // first version of this fixture did exactly that, passed, and
            // certified an arm that could not fire.
            return Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
                content: "thinking, and thinking, and".into(),
                done: false,
                ..Default::default()
            })])));
        }
        let reply = {
            let mut replies = self.replies.lock().unwrap();
            if replies.is_empty() {
                r#"{"capability":"complete","rationale":"done"}"#
            } else {
                replies.remove(0)
            }
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: reply.into(),
            done: true,
            ..Default::default()
        })])))
    }
}

fn run(
    runaway_replies: usize,
) -> (
    Store,
    pwr_domain::Id,
    Result<(), String>,
    Vec<ModelRequest>,
) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one\n").unwrap();
    let policy = ToolPolicy {
        root: root.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec![],
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
    let provider = Runaway {
        left: Mutex::new(runaway_replies),
        requests: Mutex::new(Vec::new()),
        replies: Mutex::new(vec![
            r#"{"capability":"read_file","path":"code.rs"}"#,
            r#"{"capability":"complete","rationale":"read it"}"#,
        ]),
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
                &[],
                12,
            ));
    let requests = provider.requests.lock().unwrap().clone();
    (store, run_id, outcome.map(|_| ()), requests)
}

/// One runaway reply costs a turn, not the run.
#[test]
fn a_reply_that_ran_away_is_told_and_retried() {
    let (store, run_id, outcome, requests) = run(1);
    assert!(
        outcome.is_ok(),
        "one runaway reply ended the run: {outcome:?}"
    );
    let events = store.events_for_run(run_id).unwrap();
    let named = events
        .iter()
        .find(|event| event.payload["kind"] == "runaway_reply")
        .expect("the runaway reply was never named in the audit");
    assert_eq!(named.event_type, "action.malformed");
    // The deployment is told what to do differently, not only that it failed --
    // checked where it arrives, which is the next request rather than the audit.
    let told = requests.iter().any(|request| {
        request
            .messages
            .iter()
            .any(|message| message.content.contains("Do less in one turn"))
    });
    assert!(
        told,
        "the deployment was never told how to avoid it: {:?}",
        requests.last().map(|request| request.messages.len())
    );
}

/// It is bounded, like every other way a deployment can produce nothing usable.
/// A backend that truncates every reply is not one to keep asking.
#[test]
fn replies_that_keep_running_away_still_end_the_run() {
    let (_, _, outcome, _) = run(99);
    let error = outcome.expect_err("an endlessly truncating backend ran to completion");
    // Named, counted, and carrying the backend's own words after the colon.
    // The wording covers both faults since they share one arm, so what the
    // assertion holds is that the run says how many and which, not a phrase.
    assert!(
        error.contains("unusable replies in a row"),
        "the run ended without saying how many: {error}"
    );
    assert!(
        error.contains("terminal chunk"),
        "the run ended without saying which fault: {error}"
    );
}

// ------------------------------------------- one definition of the fault

/// The two faults that leave a turn with nothing, and the two that do not.
///
/// `ReplyFault::of` is what both loops branch on now, so what it declines to
/// classify decides what is *not* told to the deployment. A backend that is
/// down, a prompt that did not fit and a cancellation are not the deployment
/// writing badly, and telling it to write differently about a fault it did not
/// cause sends it to correct something it never did.
#[test]
fn only_a_reply_the_turn_cannot_use_is_the_deployments_to_fix() {
    use pwr_orchestrator::ReplyFault;

    let unparsed = ProviderError::ModelOutput {
        safe_context: "XML syntax error on line 3".into(),
    };
    let ran_away = ProviderError::Truncated {
        safe_context: "reply exceeded the chunk bound".into(),
    };
    assert_eq!(
        ReplyFault::of(&unparsed),
        Some(ReplyFault::Unparsed("XML syntax error on line 3".into()))
    );
    assert_eq!(
        ReplyFault::of(&ran_away),
        Some(ReplyFault::RanAway("reply exceeded the chunk bound".into()))
    );

    for not_the_deployments in [
        ProviderError::Unavailable {
            safe_context: "connection refused".into(),
        },
        ProviderError::ContextLimit {
            safe_context: "prompt too large".into(),
        },
        ProviderError::Timeout {
            safe_context: "took too long".into(),
        },
        ProviderError::Cancelled,
    ] {
        assert_eq!(
            ReplyFault::of(&not_the_deployments),
            None,
            "{not_the_deployments} was handed to the deployment to fix"
        );
    }

    // What reaches the deployment carries what the harness knows and names the
    // way out, for both.
    let told = ReplyFault::Unparsed("bad escape".into()).told();
    assert!(told.contains("bad escape"), "{told}");
    assert!(told.contains("backslash"), "{told}");
    let told = ReplyFault::RanAway("cut off".into()).told();
    assert!(told.contains("cut off"), "{told}");
    assert!(told.contains("Do less in one turn"), "{told}");
}
