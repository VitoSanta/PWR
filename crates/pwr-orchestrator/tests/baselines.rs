//! The controls PWR is measured against, driven end to end.
//!
//! A control that records its work differently from B1 cannot be scored by the
//! same evaluator, and one that is weaker at the protocol makes B1 look better
//! for reasons that are not the harness. These fixtures hold both: the events
//! the evaluator reads are there, and the arm-specific rules -- B0 finishes
//! when told, B2 localizes before it edits and validates before it accepts --
//! are what the loop does.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
};
use pwr_orchestrator::RunTuning;
use pwr_orchestrator::baseline::{
    drop_oldest_exchanges, localized_paths, run_conventional_loop, run_staged_loop,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_store::Store;
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::sync::Mutex;
use std::time::Duration;

/// Replies from a script, recording every request it was sent.
struct Script {
    replies: Mutex<Vec<String>>,
    requests: Mutex<Vec<ModelRequest>>,
    /// The window this backend serves whatever it is asked for, as a backend
    /// whose window is fixed at load time does. `None` honours the request.
    serves: Option<u32>,
    asked_for: Mutex<Vec<u32>>,
}

impl Script {
    fn new(replies: &[&str]) -> Self {
        Self {
            replies: Mutex::new(replies.iter().map(|reply| (*reply).to_string()).collect()),
            requests: Mutex::new(Vec::new()),
            serves: None,
            asked_for: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ModelProvider for Script {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn prepare_context(
        &self,
        _: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        self.asked_for.lock().unwrap().push(context_tokens);
        Ok(self.serves.unwrap_or(context_tokens))
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = {
            let mut left = self.replies.lock().unwrap();
            if left.is_empty() {
                r#"{"capability":"complete","rationale":"done"}"#.to_string()
            } else {
                left.remove(0)
            }
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content: reply,
            done: true,
            ..Default::default()
        })])))
    }
}

struct Workspace {
    _dir: tempfile::TempDir,
    policy: ToolPolicy,
}

fn workspace(files: &[(&str, &str)]) -> Workspace {
    let dir = tempfile::tempdir().unwrap();
    for (path, content) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    let policy = ToolPolicy {
        root: dir.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["sh".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    Workspace { _dir: dir, policy }
}

fn request(task: &str) -> ModelRequest {
    ModelRequest {
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "fake".into(),
            endpoint: "http://localhost/".into(),
            model_ref: "fake".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
        messages: vec![
            ChatMessage::text("system", "you are a coding agent"),
            ChatMessage::text("user", task),
        ],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    }
}

fn event_types(store: &Store, run_id: pwr_domain::Id) -> Vec<String> {
    store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .map(|event| event.event_type)
        .collect()
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Runtime::new().unwrap().block_on(future)
}

// ----------------------------------------------------------------- B0

/// B0 finishes when the deployment says so, having run no check. What the
/// evaluator reads -- the completion's rationale, its turns and its actions --
/// is recorded as B1 records it.
#[test]
fn the_conventional_loop_is_finished_when_the_deployment_says_so() {
    let ws = workspace(&[("code.txt", "one\n")]);
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = Script::new(&[
        r#"{"capability":"read_file","path":"code.txt"}"#,
        r#"{"capability":"complete","rationale":"read it"}"#,
    ]);
    let result = block_on(run_conventional_loop(
        &store,
        &provider,
        run_id,
        request("look at code.txt"),
        &ws.policy,
        12,
        &RunTuning::default(),
    ))
    .unwrap();
    assert!(result.verified, "the declaration is B0's acceptance");
    assert!(
        !result.verifiable,
        "B0 ran no check and must not say it did"
    );
    let types = event_types(&store, run_id);
    for expected in [
        "baseline.arm",
        "turn.generated",
        "tool.action",
        "task.complete",
    ] {
        assert!(
            types.iter().any(|t| t == expected),
            "{expected} missing: {types:?}"
        );
    }
    assert!(
        !types.iter().any(|t| t.starts_with("verification.")),
        "B0 verified something: {types:?}"
    );
    // The file's contents reached the deployment as a tool result.
    let second = &provider.requests.lock().unwrap()[1];
    assert!(
        second
            .messages
            .iter()
            .any(|m| m.role == "tool" && m.content.contains("one")),
        "the read's result never reached the deployment"
    );
}

/// A control asks for its window as B1 does, and runs inside the one it is
/// served. Observed in the R2 pilot of 2026-09-14: neither B0 nor B2 asked, so
/// LM Studio served them 8,192 tokens while their reports recorded 16,384, and
/// B2 lost eight trials to a prompt larger than the window it really had.
#[test]
fn both_controls_ask_for_their_window_and_run_inside_what_is_served() {
    for arm in ["b0", "b2"] {
        let ws = workspace(&[("code.txt", "one\n")]);
        let store = Store::open(":memory:").unwrap();
        let run_id = pwr_domain::new_id();
        let mut provider = Script::new(&[
            r#"{"capability":"read_file","path":"code.txt"}"#,
            r#"{"capability":"complete","rationale":"[\"code.txt\"]"}"#,
        ]);
        provider.serves = Some(4096);
        let asked = request("look at code.txt");
        let wanted = asked.context_tokens;
        let check = ("sh".to_string(), vec!["-c".to_string(), "true".to_string()]);
        let result = if arm == "b0" {
            block_on(run_conventional_loop(
                &store,
                &provider,
                run_id,
                asked,
                &ws.policy,
                12,
                &RunTuning::default(),
            ))
        } else {
            block_on(run_staged_loop(
                &store,
                &provider,
                run_id,
                asked,
                &ws.policy,
                std::slice::from_ref(&check),
                12,
                &RunTuning::default(),
            ))
        };
        result.unwrap();
        assert_eq!(
            provider.asked_for.lock().unwrap().first(),
            Some(&wanted),
            "{arm} never asked for its window"
        );
        let requests = provider.requests.lock().unwrap();
        assert!(!requests.is_empty(), "{arm} sent nothing");
        assert!(
            requests
                .iter()
                .all(|request| request.context_tokens == 4096),
            "{arm} recorded a window it was not served: {:?}",
            requests
                .iter()
                .map(|r| r.context_tokens)
                .collect::<Vec<_>>()
        );
        assert!(
            event_types(&store, run_id)
                .iter()
                .any(|t| t == "context.tier_changed"),
            "{arm} did not record the smaller window: {:?}",
            event_types(&store, run_id)
        );
    }
}

/// The protocol is not where a control is weaker: a malformed call is told to
/// the deployment and retried, as B1 does.
#[test]
fn the_conventional_loop_tells_the_deployment_about_a_malformed_call() {
    let ws = workspace(&[("code.txt", "one\n")]);
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = Script::new(&[
        r#"{"capability":"read_file"}"#,
        r#"{"capability":"complete","rationale":"done"}"#,
    ]);
    let result = block_on(run_conventional_loop(
        &store,
        &provider,
        run_id,
        request("t"),
        &ws.policy,
        12,
        &RunTuning::default(),
    ))
    .unwrap();
    assert!(result.verified);
    assert!(
        event_types(&store, run_id)
            .iter()
            .any(|t| t == "action.malformed")
    );
}

/// The conventional bound keeps the system prompt and the task, drops the
/// oldest exchange first, and never leaves a result without its call.
#[test]
fn the_transcript_drops_its_oldest_exchanges_and_keeps_the_task() {
    let big = "x".repeat(4000);
    let mut messages = vec![
        ChatMessage::text("system", "sys"),
        ChatMessage::text("user", "the task"),
        ChatMessage::text("assistant", "call one"),
        ChatMessage::text("tool", &big),
        ChatMessage::text("assistant", "call two"),
        ChatMessage::text("tool", &big),
        ChatMessage::text("assistant", "call three"),
        ChatMessage::text("tool", "small"),
    ];
    // 8192 tokens * 3/4 = 6144 tokens, about 24,576 characters: everything fits.
    assert_eq!(drop_oldest_exchanges(&mut messages, 8192), 0);
    // 2048 * 3/4 = 1536 tokens, about 6,144 characters: one big result must go.
    let dropped = drop_oldest_exchanges(&mut messages, 2048);
    assert_eq!(dropped, 2, "{messages:?}");
    assert_eq!(messages[0].content, "sys");
    assert_eq!(messages[1].content, "the task");
    assert_eq!(messages[2].content, "call two");
    assert_ne!(
        messages[2].role, "tool",
        "a result was left without its call"
    );
}

// ----------------------------------------------------------------- B2

#[test]
fn a_localization_is_read_from_json_or_from_a_list() {
    assert_eq!(
        localized_paths(r#"["src/a.py", "./src/b.py", "src/a.py"]"#),
        vec!["src/a.py", "src/b.py"]
    );
    assert_eq!(
        localized_paths("The change belongs in:\n- `src/core.py`\n- src/util.py"),
        vec!["src/core.py", "src/util.py"]
    );
    assert_eq!(
        localized_paths("[\"a\",\"b\",\"c\",\"d\",\"e\",\"f.x\"]").len(),
        5
    );
}

/// B2 in order: an edit before localization is refused, the localized file is
/// shown whole, a failing check sends it back to repair, and a passing one is
/// accepted.
#[test]
fn the_staged_workflow_localizes_repairs_and_validates_in_that_order() {
    let ws = workspace(&[("value.txt", "the answer goes in answer.txt\n")]);
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let check = (
        "sh".to_string(),
        vec![
            "-c".to_string(),
            "cat answer*.txt 2>/dev/null | grep -q right".to_string(),
        ],
    );
    let provider = Script::new(&[
        // An edit during localization is refused.
        r#"{"capability":"write_file","path":"answer.txt","content":"right\n"}"#,
        r#"{"capability":"complete","rationale":"[\"value.txt\"]"}"#,
        // Repair with the wrong content first: validation fails.
        r#"{"capability":"write_file","path":"answer.txt","content":"still wrong\n"}"#,
        r#"{"capability":"complete","rationale":"fixed"}"#,
        r#"{"capability":"write_file","path":"answer2.txt","content":"right\n"}"#,
        r#"{"capability":"complete","rationale":"fixed now"}"#,
    ]);
    let result = block_on(run_staged_loop(
        &store,
        &provider,
        run_id,
        request("an answer file must say right"),
        &ws.policy,
        std::slice::from_ref(&check),
        20,
        &RunTuning::default(),
    ))
    .unwrap();
    assert!(result.verified && result.verifiable);
    assert_eq!(
        std::fs::read_to_string(ws.policy.root.join("answer.txt")).unwrap(),
        "still wrong\n",
        "the edit refused during localization went through"
    );
    let events = store.events_for_run(run_id).unwrap();
    let validated: Vec<&serde_json::Value> = events
        .iter()
        .filter(|event| event.event_type == "staged.validated")
        .map(|event| &event.payload)
        .collect();
    assert_eq!(validated.len(), 2, "{validated:?}");
    assert_eq!(validated[0]["failing"].as_array().unwrap().len(), 1);
    assert!(validated[1]["failing"].as_array().unwrap().is_empty());
    let denied = events
        .iter()
        .find(|event| event.event_type == "tool.action" && event.payload["status"] == "denied");
    assert!(
        denied.is_some(),
        "the edit before localization was not refused"
    );
    // The repair stage showed the localized file.
    let requests = provider.requests.lock().unwrap();
    assert!(requests.iter().any(|request| {
        request
            .messages
            .iter()
            .any(|m| m.content.contains("Stage 2 of 3") && m.content.contains("answer goes in"))
    }));
}

/// A localization naming nothing that exists is sent back rather than
/// carried into repair with no files.
#[test]
fn a_localization_of_files_that_do_not_exist_is_refused() {
    let ws = workspace(&[("value.txt", "right\n")]);
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let check = ("sh".to_string(), vec!["-c".to_string(), "true".to_string()]);
    let provider = Script::new(&[
        r#"{"capability":"complete","rationale":"[\"nowhere.txt\"]"}"#,
        r#"{"capability":"complete","rationale":"[\"value.txt\"]"}"#,
        r#"{"capability":"complete","rationale":"nothing to change"}"#,
    ]);
    let result = block_on(run_staged_loop(
        &store,
        &provider,
        run_id,
        request("t"),
        &ws.policy,
        std::slice::from_ref(&check),
        20,
        &RunTuning::default(),
    ))
    .unwrap();
    assert!(result.verified);
    let localized: Vec<serde_json::Value> = store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "staged.localized")
        .map(|event| event.payload)
        .collect();
    assert_eq!(localized.len(), 2);
    assert!(localized[0]["existing"].as_array().unwrap().is_empty());
}

/// A question localizes and answers, and is accepted only with the workspace
/// untouched.
#[test]
fn a_staged_question_answers_without_changing_anything() {
    let ws = workspace(&[("notes.txt", "the answer is 42\n")]);
    let store = Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let provider = Script::new(&[
        r#"{"capability":"complete","rationale":"[\"notes.txt\"]"}"#,
        r#"{"capability":"write_file","path":"notes.txt","content":"changed"}"#,
        r#"{"capability":"complete","rationale":"42, in notes.txt"}"#,
    ]);
    let tuning = RunTuning {
        preserve_baseline: true,
        ..Default::default()
    };
    let result = block_on(run_staged_loop(
        &store,
        &provider,
        run_id,
        request("what is the answer?"),
        &ws.policy,
        &[],
        20,
        &tuning,
    ))
    .unwrap();
    assert!(result.verified);
    assert_eq!(
        std::fs::read_to_string(ws.policy.root.join("notes.txt")).unwrap(),
        "the answer is 42\n",
        "the answer stage edited a file"
    );
    // Refused by the stage, not merely by the tool's own overwrite guard.
    let refused_by_stage = store
        .events_for_run(run_id)
        .unwrap()
        .into_iter()
        .any(|event| {
            event.event_type == "tool.action"
                && event.payload["denial"]
                    .as_str()
                    .is_some_and(|denial| denial.contains("does not edit"))
        });
    assert!(
        refused_by_stage,
        "the edit was not refused by the answer stage"
    );
}
