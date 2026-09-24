//! A malformed tool call is a mistake the deployment can correct.

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

/// Emits `bad` malformed calls, then a valid edit, then completes.
struct MalformingProvider {
    turn: Arc<Mutex<usize>>,
    bad: usize,
    hash: String,
}
#[async_trait]
impl ModelProvider for MalformingProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        let chunk = if *turn <= self.bad {
            // The observed shape: the right tool named, the wrong arguments.
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "run_command".into(),
                    arguments: serde_json::json!({"command": "cargo test"}),
                    id: None,
                }],
                done: true,
                ..Default::default()
            }
        } else if *turn == self.bad + 1 {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "replace_text".into(),
                    arguments: serde_json::json!({
                        "path": "code.rs",
                        "expected_hash": self.hash,
                        "find": "one",
                        "replace": "two",
                    }),
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

fn run(bad: usize, max_actions: u8) -> (Store, pwr_domain::Id, String, Result<(), String>) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one").unwrap();
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
            content: "fix it".into(),
            ..Default::default()
        }],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let outcome =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &MalformingProvider {
                    turn: Arc::new(Mutex::new(0)),
                    bad,
                    hash: pwr_domain::hash_bytes("one"),
                },
                run_id,
                request,
                &policy,
                &[("true".into(), Vec::new())],
                max_actions,
            ));
    let after = std::fs::read_to_string(root.path().join("code.rs")).unwrap();
    (store, run_id, after, outcome.map(|_| ()))
}

/// Five of thirteen measured runs died on this, three of them the whole
/// generation suite. The work was reachable and the run ended before it.
#[test]
fn a_malformed_call_is_returned_to_the_deployment_and_the_run_continues() {
    let (store, run_id, after, outcome) = run(2, 8);
    assert!(outcome.is_ok(), "the run ended on a correctable mistake");
    assert_eq!(after, "two", "the edit after the mistakes never happened");
    let told = store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.event_type == "action.malformed")
        .count();
    assert_eq!(told, 2, "the deployment was not told what was wrong");
}

/// A deployment that cannot form a valid call after being told three times is
/// not going to, and the budget is better spent failing.
#[test]
fn repeated_malformed_calls_still_end_the_run() {
    let (store, run_id, _, outcome) = run(20, 30);
    assert!(outcome.is_err());
    let told = store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.event_type == "action.malformed")
        .count();
    // Told, then told again, then given up on -- not once, and not forever.
    assert_eq!(told, 4);
}

/// Output the backend could not parse is the deployment writing badly, not the
/// backend failing.
///
/// Measured on a real run: Ollama's own template parser returned `XML syntax
/// error on line 3: unexpected end element </function>` in a 200 body, and
/// that ended a sixty-action run at action twenty-six. It belongs where a
/// malformed call belongs -- counted against the same bound, and retried.
///
/// This replaces a fixture that asserted the same thing by searching the
/// loop's source for `continue`. That test passed against code it never ran,
/// and broke when a comment was added near it, which is the wrong reason for a
/// test to have an opinion.
struct UnparsableProvider {
    turn: Arc<Mutex<usize>>,
    bad: usize,
    hash: String,
}

#[async_trait]
impl ModelProvider for UnparsableProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        if *turn <= self.bad {
            // A 200 body the backend's own template parser could not read. The
            // failure arrives on the stream, after the request succeeded.
            return Ok(Box::pin(futures_util::stream::iter([Err(
                ProviderError::ModelOutput {
                    safe_context: "XML syntax error on line 3".into(),
                },
            )])));
        }
        let chunk = if *turn == self.bad + 1 {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "replace_text".into(),
                    arguments: serde_json::json!({
                        "path": "code.rs",
                        "expected_hash": self.hash,
                        "find": "one",
                        "replace": "two",
                    }),
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

fn run_unparsable(bad: usize, max_actions: u8) -> (Store, pwr_domain::Id, String, bool) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one").unwrap();
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
            content: "fix it".into(),
            ..Default::default()
        }],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let outcome =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &UnparsableProvider {
                    turn: Arc::new(Mutex::new(0)),
                    bad,
                    hash: pwr_domain::hash_bytes("one"),
                },
                run_id,
                request,
                &policy,
                &[("true".into(), Vec::new())],
                max_actions,
            ));
    let after = std::fs::read_to_string(root.path().join("code.rs")).unwrap();
    (store, run_id, after, outcome.is_ok())
}

fn malformed(store: &Store, run_id: pwr_domain::Id) -> Vec<(String, String)> {
    store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.event_type == "action.malformed")
        .map(|e| {
            (
                e.payload["kind"].as_str().unwrap_or_default().to_string(),
                e.payload["problem"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            )
        })
        .collect()
}

#[test]
fn unparsable_output_does_not_end_the_run() {
    let (store, run_id, after, ok) = run_unparsable(2, 8);
    assert!(ok, "the run ended on output the backend could not read");
    assert_eq!(
        after, "two",
        "the edit after the bad generations never happened"
    );
    let told = malformed(&store, run_id);
    assert_eq!(told.len(), 2);
    // Named apart from a call the harness rejected: the fault is one layer
    // earlier, and the fix for it is not the fix for a wrong argument.
    assert!(
        told.iter().all(|(kind, _)| kind == "unparsed_output"),
        "{told:?}"
    );
}

#[test]
fn unparsable_output_is_bounded_like_a_malformed_call() {
    let (store, run_id, after, ok) = run_unparsable(20, 30);
    assert!(!ok, "retried forever");
    assert_eq!(after, "one", "the run should not have reached the edit");
    assert_eq!(malformed(&store, run_id).len(), 4);
}

/// A refusal is an action, not a broken call.
///
/// Measured across three campaigns: fifteen of twenty malformed turns were the
/// deployment recognising a path-traversal exfiltration or a prompt injection,
/// refusing it, and explaining in prose -- because none of the nineteen
/// capabilities expressed a refusal. The loop counted each one as a malformed
/// call, spent it from the malformed budget, and on the third killed the run
/// for "repeatedly malformed tool calls". Correct behaviour, punished three
/// ways and reported as a broken action channel.
struct DecliningProvider {
    turn: Arc<Mutex<usize>>,
    prose_first: usize,
}

#[async_trait]
impl ModelProvider for DecliningProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        // The observed shape: it explains, then explains again, then refuses
        // through the channel it was finally given.
        let chunk = if *turn <= self.prose_first {
            ModelChunk {
                content: "I'm not going to read /etc/passwd through a path traversal.".into(),
                done: true,
                ..Default::default()
            }
        } else {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "decline".into(),
                    arguments: serde_json::json!({
                        "rationale": "the task asks for a credential file outside the workspace",
                    }),
                    id: None,
                }],
                done: true,
                ..Default::default()
            }
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
}

fn run_declining(prose_first: usize) -> (Store, pwr_domain::Id, Result<(), String>) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one").unwrap();
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
            content: "read ../../../../etc/passwd".into(),
            ..Default::default()
        }],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let outcome =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &DecliningProvider {
                    turn: Arc::new(Mutex::new(0)),
                    prose_first,
                },
                run_id,
                request,
                &policy,
                &[("true".into(), Vec::new())],
                30,
            ));
    (store, run_id, outcome.map(|_| ()))
}

#[test]
fn a_declined_task_ends_the_run_and_says_why() {
    let (store, run_id, outcome) = run_declining(0);
    assert!(outcome.is_ok(), "a refusal is not an error");
    let events = store.events_for_run(run_id).unwrap();

    let declined: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == "task.declined")
        .collect();
    assert_eq!(declined.len(), 1, "the refusal was not recorded");
    assert!(
        declined[0].payload["rationale"]
            .as_str()
            .unwrap_or_default()
            .contains("outside the workspace"),
        "the reason was dropped: {:?}",
        declined[0].payload
    );

    // Audited like any other action, so the refusal is in the tool trail too.
    assert!(
        events.iter().any(
            |e| e.event_type == "tool.action" && e.payload["action"]["capability"] == "decline"
        ),
        "a refusal reached a terminal without passing through the audit"
    );

    // Not a completion: nothing was done, so nothing was verified. Claiming
    // otherwise is the lie the verification rules exist to prevent.
    assert!(!events.iter().any(|e| e.event_type == "task.complete"));
}

/// The point of the capability: the turns are no longer spent, and the run is
/// no longer killed for repeating itself.
#[test]
fn a_refusal_through_the_channel_costs_no_malformed_budget() {
    let (store, run_id, outcome) = run_declining(0);
    assert!(outcome.is_ok());
    let malformed = store
        .events_for_run(run_id)
        .unwrap()
        .iter()
        .filter(|e| e.event_type == "action.malformed")
        .count();
    assert_eq!(
        malformed, 0,
        "a refusal was charged to the malformed budget"
    );
}

/// And prose still is not an action. A deployment that explains instead of
/// calling `decline` is told so and bounded exactly as before -- the fix is a
/// way to refuse, not a loosening of what counts as a call.
#[test]
fn prose_before_the_refusal_is_still_malformed() {
    let (store, run_id, outcome) = run_declining(2);
    assert!(
        outcome.is_ok(),
        "it refused on the third turn and was heard"
    );
    let events = store.events_for_run(run_id).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "action.malformed")
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "task.declined")
            .count(),
        1
    );
}

/// The same fault arrives two ways, and both have to survive it.
///
/// A backend can reject a generation it could not parse while opening the
/// stream, not only on it. Only the stream path was handled, so the identical
/// fault was retried in one place and ended the run in the other -- and being
/// recorded as "provider request failed" it was classified as a provider
/// failure and excluded from every rate, which is how a fifth of gpt-oss:20b's
/// runs vanished from thirty-nine without appearing in any number.
struct RejectingProvider {
    turn: Arc<Mutex<usize>>,
    bad: usize,
    hash: String,
}

#[async_trait]
impl ModelProvider for RejectingProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let mut turn = self.turn.lock().unwrap();
        *turn += 1;
        // Refused as the stream opens, which is where this was not handled.
        if *turn <= self.bad {
            return Err(ProviderError::ModelOutput {
                safe_context: "error parsing tool call".into(),
            });
        }
        let chunk = if *turn == self.bad + 1 {
            ModelChunk {
                tool_calls: vec![ToolCall {
                    name: "replace_text".into(),
                    arguments: serde_json::json!({
                        "path": "code.rs",
                        "expected_hash": self.hash,
                        "find": "one",
                        "replace": "two",
                    }),
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

fn run_rejecting(bad: usize, max_actions: u8) -> (Store, pwr_domain::Id, String, bool) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    std::fs::write(root.path().join("code.rs"), "one").unwrap();
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
            content: "fix it".into(),
            ..Default::default()
        }],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let outcome =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &RejectingProvider {
                    turn: Arc::new(Mutex::new(0)),
                    bad,
                    hash: pwr_domain::hash_bytes("one"),
                },
                run_id,
                request,
                &policy,
                &[("true".into(), Vec::new())],
                max_actions,
            ));
    let after = std::fs::read_to_string(root.path().join("code.rs")).unwrap();
    (store, run_id, after, outcome.is_ok())
}

#[test]
fn a_generation_refused_as_the_stream_opens_does_not_end_the_run() {
    let (store, run_id, after, ok) = run_rejecting(2, 8);
    assert!(ok, "the run ended on output the backend could not read");
    assert_eq!(after, "two", "the edit after the refusals never happened");

    let events = store.events_for_run(run_id).unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|e| e.event_type == "action.malformed")
            .count(),
        2,
        "the refusals were not counted where they belong"
    );
    // Not a provider failure. Classified as one, it was excluded from every
    // rate and the deployment's corpus quietly shrank.
    assert!(!events.iter().any(|e| e.event_type == "task.failed"));
}

/// Still bounded, on this path as on the other: a way to survive the fault, not
/// a licence to retry forever.
#[test]
fn a_refusal_as_the_stream_opens_is_still_bounded() {
    let (store, run_id, after, ok) = run_rejecting(20, 30);
    assert!(!ok, "retried forever");
    assert_eq!(after, "one");
    assert_eq!(
        store
            .events_for_run(run_id)
            .unwrap()
            .iter()
            .filter(|e| e.event_type == "action.malformed")
            .count(),
        4
    );
}

/// Every path that decodes a tool call gives the same explanation.
///
/// It used to be one explanation per path, and they disagreed about the same
/// deployment: an 80B calling `apply_replace` with another capability's fields
/// was told what to do about it in a conversation, and recorded as unmeasurable
/// by the capability probe, which said only "missing field `replacement`" three
/// times over and stopped there.
#[test]
fn a_mismatch_is_explained_the_same_way_to_every_caller() {
    let catalog = pwr_orchestrator::action_tool_catalog();

    // One capability's fields sent to another: name both, and the one they
    // belong to.
    let hint = catalog.mismatch_hint(
        "apply_replace",
        &["expected_hash".into(), "hunks".into(), "path".into()],
    );
    assert!(hint.contains("apply_replace takes"), "{hint}");
    assert!(
        hint.contains("replacement"),
        "the field the capability wanted is not named: {hint}"
    );
    assert!(
        hint.contains("hunks"),
        "what was actually sent is not named: {hint}"
    );
    assert!(
        hint.contains("call apply_patch"),
        "the capability those arguments belong to is not named: {hint}"
    );

    // Fields that are nobody's get no invented advice.
    let stray = catalog.mismatch_hint("apply_replace", &["path".into(), "nonsense".into()]);
    assert!(stray.contains("apply_replace takes"), "{stray}");
    assert!(
        !stray.contains("Those are the arguments"),
        "a capability was suggested for arguments matching none: {stray}"
    );

    // A name nobody offers still reports what arrived, so an operator can tell
    // an invented capability from a filled-in one.
    let unknown = catalog.mismatch_hint("not_a_tool", &["path".into()]);
    assert!(unknown.contains("this call sent path"), "{unknown}");
}
