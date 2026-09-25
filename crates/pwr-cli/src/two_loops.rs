//! Where the conversation and the scripted run decide differently.
//!
//! R1 replaces two loops with one session runtime. Before anything moves, this
//! module drives both of them from the same script against the same workspace
//! and writes down what each does, so that an extraction which changes
//! behaviour fails here rather than in a campaign three weeks later.
//!
//! `converse::take_turn` had no end-to-end fixture at all: it is called from
//! exactly one place in `main.rs` and nothing drove it. Its parts were tested --
//! compaction, decoding, refusal text -- and the loop that assembles them was
//! not, which is how the false completion claim in `chat_turn` survived being
//! read. So the first thing here is coverage, and the comparison is the second.
//!
//! Every divergence below is labelled. **DECLARED** is a difference the two
//! modes are entitled to, and R1 must preserve it and say so. **DEFECT** is a
//! difference nobody chose, and R1 removes it. A divergence with no label is a
//! divergence nobody has decided about yet, and there should be none.
//!
//! Where R1 left it, 2026-09-15. Both loops call the same step for every
//! action -- `pwr_orchestrator::session::gate` for repetition and approval,
//! `session::perform` for intent, execution against a read history, receipt and
//! checkpoint -- and share the result envelope, reply-fault handling, the
//! measured-tier rule, stall and repetition detection, and terminal classes. The
//! product-path evaluator runs the scripted loop itself. What remains in each
//! loop is declared:
//!
//! - a run has a plan, verification at completion and a hard action cap; a turn
//!   has none of those and a soft budget that checks in with the person;
//! - a run fails on a backend fault, which campaigns measure as `provider`; a
//!   turn retries three in a row;
//! - a run's malformed-call bound comes from the deployment's measured emission
//!   rate; a turn's is three;
//! - a run gets its approvals from the caller's prompt; the console grants what
//!   Settings allow before a turn and refuses the rest without asking;
//! - compaction: the run's ledger compaction and the conversation's summary are
//!   different algorithms. Unifying them changes R3's control arm, C0, which
//!   must be today's compaction for the H2 comparison, so it waits for R3.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::converse;
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::{SandboxPolicy, ToolPolicy};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

/// Replies in order and keeps what it was asked.
///
/// The requests matter as much as the replies: half of what separates the two
/// loops is what reaches the deployment, not what it says back.
pub(crate) struct Scripted {
    replies: Mutex<VecDeque<ModelChunk>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Scripted {
    pub(crate) fn new(replies: Vec<ModelChunk>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

/// A backend that refuses the first request as too large and records every
/// window it was asked to serve.
struct Cramped {
    refusals: Mutex<usize>,
    prepared: Mutex<Vec<u32>>,
    inner: Scripted,
}

#[async_trait]
impl ModelProvider for Cramped {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn prepare_context(
        &self,
        _: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        self.prepared.lock().unwrap().push(context_tokens);
        Ok(context_tokens)
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let refuse = {
            let mut left = self.refusals.lock().unwrap();
            let refuse = *left > 0;
            *left = left.saturating_sub(1);
            refuse
        };
        if refuse {
            return Err(ProviderError::ContextLimit {
                safe_context: "prompt exceeds the loaded window".into(),
            });
        }
        self.inner.chat(request).await
    }
}

impl Recording for Cramped {
    fn requests(&self) -> Vec<ModelRequest> {
        self.inner.requests()
    }
}

/// Serves `stop_after` replies, then signals that the operator pressed stop.
///
/// The flag is the console's, so the fixture drives it the way the console
/// does: set between the backend answering and the turn asking again.
struct Stopping {
    stop_after: Mutex<usize>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    inner: Scripted,
}

impl Recording for Stopping {
    fn requests(&self) -> Vec<ModelRequest> {
        self.inner.requests()
    }
}

#[async_trait]
impl ModelProvider for Stopping {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let reply = self.inner.chat(request).await;
        // Set after serving the reply, so the turn sees it at the top of its
        // next iteration -- which is where the console's stop arrives, between
        // the backend answering and the turn asking again. Setting it a reply
        // later would let the turn finish normally and test nothing.
        let mut left = self.stop_after.lock().unwrap();
        *left = left.saturating_sub(1);
        if *left == 0 {
            self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        reply
    }
}

/// A backend that fails the first `faults` requests and then serves the script.
struct Faulty {
    faults: Mutex<usize>,
    inner: Scripted,
}

#[async_trait]
impl ModelProvider for Faulty {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        // The guard is dropped before the await: a `MutexGuard` held across
        // one makes the future non-Send, and the trait requires Send.
        let fault = {
            let mut left = self.faults.lock().unwrap();
            let fault = *left > 0;
            *left = left.saturating_sub(1);
            fault
        };
        if fault {
            return Err(ProviderError::Unavailable {
                safe_context: "connection refused".into(),
            });
        }
        self.inner.chat(request).await
    }
}

/// A backend that rejects the first `faults` generations as unparseable and then
/// serves the script: the deployment wrote a tool call the backend could not
/// read.
struct Unreadable {
    faults: Mutex<usize>,
    inner: Scripted,
}

#[async_trait]
impl ModelProvider for Unreadable {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let fault = {
            let mut left = self.faults.lock().unwrap();
            let fault = *left > 0;
            *left = left.saturating_sub(1);
            fault
        };
        if fault {
            // Recorded like a served request, so the fixture can read what the
            // next one carried.
            self.inner.requests.lock().unwrap().push(request);
            return Err(ProviderError::ModelOutput {
                safe_context: "XML syntax error on line 3".into(),
            });
        }
        self.inner.chat(request).await
    }
}

impl Recording for Unreadable {
    fn requests(&self) -> Vec<ModelRequest> {
        self.inner.requests()
    }
}

#[async_trait]
impl ModelProvider for Scripted {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        // An exhausted script is a fixture that did not say what happens next,
        // which is worth failing on rather than filling in with silence: an
        // empty reply is itself a behaviour both loops treat specially.
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("the script ran out of replies before the loop ran out of turns");
        Ok(Box::pin(futures_util::stream::iter([Ok(reply)])))
    }
}

/// One reply carrying a tool call, in the shape a native-tools backend sends.
pub(crate) fn calls(name: &str, arguments: serde_json::Value) -> ModelChunk {
    ModelChunk {
        content: String::new(),
        tool_calls: vec![ToolCall {
            name: name.to_owned(),
            arguments,
            id: None,
        }],
        done: true,
        ..Default::default()
    }
}

/// One reply that only speaks.
pub(crate) fn says(text: &str) -> ModelChunk {
    ModelChunk {
        content: text.to_owned(),
        done: true,
        ..Default::default()
    }
}

pub(crate) fn deployment() -> DeploymentDescriptor {
    DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    }
}

pub(crate) fn policy_for(root: &std::path::Path) -> ToolPolicy {
    ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec![],
        output_limit: 4096,
        timeout: Duration::from_secs(5),
        sandbox: SandboxPolicy::Disabled,
        approvals: crate::all_approvals(),
    }
}

/// What a turn of the conversation did, as an observer outside it can see.
struct ChatOutcome {
    report: converse::TurnReport,
    steps: Vec<String>,
    requests: Vec<ModelRequest>,
}

/// A provider a fixture can ask what it was sent.
///
/// The requests matter as much as the replies, so every provider here keeps
/// them and the driver is generic over that rather than over `ModelProvider`
/// alone.
trait Recording: ModelProvider {
    fn requests(&self) -> Vec<ModelRequest>;
}

impl Recording for Scripted {
    fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Recording for Faulty {
    fn requests(&self) -> Vec<ModelRequest> {
        self.inner.requests()
    }
}

fn drive_chat(root: &std::path::Path, script: Vec<ModelChunk>) -> ChatOutcome {
    drive_chat_with(root, Scripted::new(script), &[])
}

fn drive_chat_stopping(root: &std::path::Path, mut provider: Stopping) -> ChatOutcome {
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    provider.stop = std::sync::Arc::clone(&stop);
    drive_chat_inner(root, provider, &[], &stop)
}

fn drive_chat_with<P: Recording>(
    root: &std::path::Path,
    provider: P,
    context_tiers: &[u32],
) -> ChatOutcome {
    drive_chat_inner(
        root,
        provider,
        context_tiers,
        &std::sync::atomic::AtomicBool::new(false),
    )
}

fn drive_chat_inner<P: Recording>(
    root: &std::path::Path,
    provider: P,
    context_tiers: &[u32],
    stop: &std::sync::atomic::AtomicBool,
) -> ChatOutcome {
    drive_chat_under(policy_for(root), provider, context_tiers, stop)
}

fn drive_chat_under<P: Recording>(
    policy: ToolPolicy,
    provider: P,
    context_tiers: &[u32],
    stop: &std::sync::atomic::AtomicBool,
) -> ChatOutcome {
    let adapter = pwr_compat::adapter_for(None, "fake");
    let store = pwr_store::Store::open(":memory:").unwrap();
    let catalog = converse::chat_tool_catalog();
    let tools = pwr_compat::render_tools(&catalog);
    let mut messages = vec![ChatMessage::text("user", "do the thing")];
    let mut steps = Vec::new();
    let report = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &provider,
            adapter.as_ref(),
            &deployment(),
            &store,
            pwr_domain::new_id(),
            &policy,
            &mut messages,
            8192,
            context_tiers,
            Default::default(),
            tools,
            stop,
            &converse::Continuity::default(),
            &pwr_orchestrator::DenyWithoutAsking,
            |step| {
                // As the console shows them: actions as lines, not as objects.
                steps.push(match step {
                    converse::TurnStep::Acted { capability, detail } => {
                        format!("acted {capability} {detail}")
                    }
                    converse::TurnStep::Refused(why) => format!("refused {why}"),
                    converse::TurnStep::Compacted(note) => format!("compacted {note}"),
                    converse::TurnStep::Steered(text) => format!("steered {text}"),
                    converse::TurnStep::Note(text) => format!("note {text}"),
                    converse::TurnStep::ToolCall(_)
                    | converse::TurnStep::Streaming { .. }
                    | converse::TurnStep::Usage { .. }
                    | converse::TurnStep::Retry { .. }
                    | converse::TurnStep::Recovered { .. }
                    | converse::TurnStep::Generation(_) => return,
                });
            },
        ))
        .expect("the turn returned an error rather than a report");
    ChatOutcome {
        report,
        steps,
        requests: provider.requests(),
    }
}

/// The same script through the scripted loop, which returns a run result rather
/// than an answer.
fn drive_run(
    root: &std::path::Path,
    script: Vec<ModelChunk>,
    checks: &[(String, Vec<String>)],
) -> (pwr_orchestrator::TaskRunResult, Vec<ModelRequest>) {
    drive_run_with(root, Scripted::new(script), checks)
}

fn drive_run_with<P: Recording>(
    root: &std::path::Path,
    provider: P,
    checks: &[(String, Vec<String>)],
) -> (pwr_orchestrator::TaskRunResult, Vec<ModelRequest>) {
    let store = pwr_store::Store::open(":memory:").unwrap();
    let policy = policy_for(root);
    let request = ModelRequest {
        deployment: deployment(),
        messages: vec![ChatMessage::text("user", "do the thing")],
        context_tokens: 8192,
        tools: None,
        seed: None,
        sampling: Default::default(),
    };
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(pwr_orchestrator::run_action_loop(
            &store,
            &provider,
            pwr_domain::new_id(),
            request,
            &policy,
            checks,
            12,
        ))
        .expect("the run returned an error rather than a result");
    (result, provider.requests())
}

fn workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("code.rs"), "one\n").unwrap();
    std::fs::write(dir.path().join("parser.rs"), "two\n").unwrap();
    dir
}

// ------------------------------------------------------- what each is offered

/// DECLARED, and decided on 2026-09-12 rather than left open.
///
/// `record_progress` and `propose_verifier` answer to a plan and a verification
/// contract, and a conversation has neither. The question behind that -- should
/// a conversation have a plan at all -- was the last thing between the two
/// loops that nobody had decided.
///
/// It stays as it is, and not for want of interest. The research contract's
/// fourth open question says to compare no plan, a model-written plan and
/// deterministic dependency state separately, and that there is **no universal
/// plan-first default**. Giving the conversation a plan now would be exactly
/// that default, adopted without the comparison that was preregistered to
/// settle it -- and it would grow the catalogue a small deployment has to
/// choose from, which is the cost H1 exists to measure rather than to assume.
///
/// What a run has that a conversation does not, and does not miss: a plan
/// substitutes for an absent person. A conversation has one, and the objective
/// is restated every turn by the only party entitled to change it.
///
/// So this is a declared difference pending evidence, not a defect. What would
/// change it: open question 4, run on long multi-file tasks, where a
/// conversation with deterministic dependency state completes more of them than
/// one without at equal budget.
#[test]
fn the_conversation_is_offered_a_smaller_catalogue_than_a_run() {
    let chat: Vec<String> = converse::chat_tool_catalog()
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();
    let run: Vec<String> = pwr_orchestrator::action_tool_catalog()
        .tools
        .iter()
        .map(|tool| tool.name.clone())
        .collect();

    let only_in_run: Vec<&String> = run.iter().filter(|name| !chat.contains(name)).collect();
    assert_eq!(only_in_run, ["record_progress", "propose_verifier"]);
    assert!(
        chat.iter().all(|name| run.contains(name)),
        "the conversation gained a capability the run does not have: {chat:?}"
    );
}

// --------------------------------------------------------- the ordinary turn

/// The first end-to-end drive of the conversation loop. A read, then an answer.
#[test]
fn a_turn_reads_and_answers() {
    let dir = workspace();
    let outcome = drive_chat(
        dir.path(),
        vec![
            calls("read_file", serde_json::json!({"path": "code.rs"})),
            says("It says one."),
        ],
    );
    assert_eq!(outcome.report.answer, "It says one.");
    assert_eq!(outcome.report.actions, 1);
    assert!(!outcome.report.edited, "a read is not a change");
    assert!(outcome.report.stopped.is_none());
    assert_eq!(outcome.steps.len(), 1, "{:?}", outcome.steps);
    assert!(outcome.steps[0].starts_with("acted read_file"));
}

/// Both loops apply the same edit to the same workspace from the same script.
/// The equivalence R1 has to preserve, stated before anything moves.
#[test]
fn both_loops_apply_the_same_edit() {
    let edit = || {
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": pwr_domain::hash_bytes("one\n"),
                "find": "one",
                "replace": "two",
            }),
        )
    };

    let chat_dir = workspace();
    let chat = drive_chat(chat_dir.path(), vec![edit(), says("Changed it.")]);
    assert!(chat.report.edited);
    assert_eq!(
        std::fs::read_to_string(chat_dir.path().join("code.rs")).unwrap(),
        "two\n"
    );

    let run_dir = workspace();
    let (result, _) = drive_run(
        run_dir.path(),
        vec![
            edit(),
            calls("complete", serde_json::json!({"rationale": "changed it"})),
        ],
        &[],
    );
    assert_eq!(
        std::fs::read_to_string(run_dir.path().join("code.rs")).unwrap(),
        "two\n"
    );
    assert!(
        !result.verifiable,
        "a workspace declaring no checks cannot be verifiable"
    );
}

/// A refusal is not a change, on either side. A stale hash is the commonest
/// one: the file moved under the deployment between reading and editing.
#[test]
fn a_stale_hash_is_refused_by_both_and_changes_nothing() {
    let stale = || {
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": "not-the-hash",
                "find": "one",
                "replace": "two",
            }),
        )
    };

    let chat_dir = workspace();
    let chat = drive_chat(chat_dir.path(), vec![stale(), says("It moved under me.")]);
    assert!(!chat.report.edited, "a refused edit is not a change");
    assert_eq!(
        std::fs::read_to_string(chat_dir.path().join("code.rs")).unwrap(),
        "one\n"
    );

    let run_dir = workspace();
    let (_, _) = drive_run(
        run_dir.path(),
        vec![
            stale(),
            calls("decline", serde_json::json!({"rationale": "it moved"})),
        ],
        &[],
    );
    assert_eq!(
        std::fs::read_to_string(run_dir.path().join("code.rs")).unwrap(),
        "one\n"
    );
}

// ----------------------------------------------------- what reaches the model

/// CLOSED, 2026-09-12. Was the sharpest statement of the divergence R1 exists
/// to remove, and is now an equivalence.
///
/// `pwr_orchestrator::context::compose` says of itself that it "composes the
/// opening prompt used for both an interactive run and an eval", and that
/// "repository retrieval is intentionally performed here so section order,
/// retrieval budget and compaction accounting cannot diverge by caller". Its
/// callers were the eval path and the scripted `run`. The conversation was a
/// third caller that did not call it and opened with a bare system string, so
/// the loop where the work happens received no repository excerpts at all --
/// an H3 retrieval result measured through eval could not have reached the
/// product.
///
/// `compose_chat_turn` puts the conversation through the same composer, for the
/// turn rather than for a whole run. This asserts that it delivers.
#[test]
fn the_conversation_composes_its_turn_the_way_a_run_composes_its_opening() {
    let dir = workspace();
    let root = dir.path();
    std::fs::write(
        root.join("parser.rs"),
        "fn parse_manifest(text: &str) -> Manifest { todo!() }\n",
    )
    .unwrap();
    let request = "fix parse_manifest in the parser";
    let profile = pwr_orchestrator::TaskProfile::resolve(None, None);

    let mut messages = vec![
        ChatMessage::text("system", converse::chat_system_prompt(root)),
        ChatMessage::text("user", request),
    ];
    let delivered = crate::compose_chat_turn(root, 8192, &profile, None, &mut messages)
        .expect("indexing a temporary workspace")
        .expect("no passages were delivered");
    assert!(delivered > 0);

    // The request survives as the last message, and the passages arrive before
    // it, tagged for what they are so compaction can tell them from a request.
    let purposes: Vec<Option<pwr_domain::MessagePurpose>> =
        messages.iter().map(|m| m.purpose).collect();
    assert_eq!(
        purposes,
        [
            None,
            Some(pwr_domain::MessagePurpose::RepositoryExcerpts),
            Some(pwr_domain::MessagePurpose::Task),
        ]
    );
    assert_eq!(messages.last().unwrap().content, request);
    assert!(
        messages[1].content.contains("parse_manifest"),
        "{:?}",
        messages[1].content
    );

    // Composing twice would rank passages against passages.
    let again = crate::compose_chat_turn(root, 8192, &profile, None, &mut messages).unwrap();
    assert_eq!(again, None, "a composed turn was composed again");
}

/// The last two sections a run gets and the conversation did not: the
/// deployment's own instructions, and what this conversation has established.
///
/// The suffix is merged into the system message rather than sent beside it,
/// because a deployment told to behave one way on one turn and not told on the
/// next is being given two different agents -- which is why `ModelSuffix` is a
/// required section. The ledger is composed per turn, tagged so compaction can
/// tell a superseded copy from a request.
#[test]
fn the_conversation_gets_the_deployments_instructions_and_its_own_ledger() {
    let dir = workspace();
    let root = dir.path();
    let mut profile = pwr_orchestrator::TaskProfile::resolve(None, None);
    profile.prompt_suffix = "Answer in one paragraph.".into();

    let mut messages = vec![
        ChatMessage::text("system", converse::chat_system_prompt(root)),
        ChatMessage::text("user", "what changed?"),
    ];
    crate::compose_chat_turn(
        root,
        8192,
        &profile,
        Some("Files this session changed:\n  - code.rs (expected_hash abc)\n"),
        &mut messages,
    )
    .expect("indexing a temporary workspace");

    assert_eq!(messages[0].role, "system");
    assert!(
        messages[0].content.contains("Answer in one paragraph."),
        "the deployment's instructions did not reach the system message"
    );
    assert!(
        messages[0].content.contains("PWR"),
        "the shared instructions were replaced rather than merged"
    );

    let ledger = messages
        .iter()
        .find(|m| m.purpose == Some(pwr_domain::MessagePurpose::SessionLedger))
        .expect("no ledger reached the turn");
    assert!(ledger.content.contains("expected_hash abc"));
    assert_eq!(messages.last().unwrap().content, "what changed?");
}

/// The request is what the turn is for. Passages can be searched for again, so
/// under pressure they are what goes -- and a conversation whose history has
/// filled the window still gets to ask its question.
#[test]
fn a_full_conversation_keeps_the_request_and_drops_the_passages() {
    let dir = workspace();
    let root = dir.path();
    std::fs::write(
        root.join("parser.rs"),
        "fn parse_manifest(text: &str) -> Manifest { todo!() }\n",
    )
    .unwrap();
    let request = "fix parse_manifest in the parser";
    let profile = pwr_orchestrator::TaskProfile::resolve(None, None);

    let mut messages = vec![
        ChatMessage::text("system", converse::chat_system_prompt(root)),
        // Four characters to the token, so this is the window and then some.
        // In the history rather than the system message, which the composer
        // now owns and refreshes.
        ChatMessage::text("assistant", "x".repeat(40_000)),
        ChatMessage::text("user", request),
    ];
    let delivered = crate::compose_chat_turn(root, 8192, &profile, None, &mut messages)
        .expect("indexing a temporary workspace");
    assert_eq!(
        delivered, None,
        "passages were delivered into a full window"
    );
    assert_eq!(messages.last().unwrap().content, request);
    assert_eq!(
        messages.last().unwrap().purpose,
        Some(pwr_domain::MessagePurpose::Task)
    );
}

/// CLOSED, 2026-09-12. Was a defect and is now the declared difference it
/// should always have been.
///
/// The two loops rendered an action's outcome differently, and the conversation
/// rendered it worse. A run turned a refusal into `{"denied": ...}` and any
/// other fault into `{"tool_failure": ..., "failure_category": ...}`, both JSON
/// a deployment can branch on, and wrapped the whole thing as
/// `{"result": ..., "status": ...}`. The conversation sent the bare result
/// object on success and the string `refused: {problem}` on any failure --
/// losing the difference between a policy decision and a broken tool, losing
/// the category, and not being JSON at all. A deployment answering both loops
/// was reading two protocols.
///
/// One envelope now, from `tool_result_message`. What remains is `status`,
/// which a run has and a conversation does not: no plan, no action budget. The
/// key is absent rather than present and empty.
#[test]
fn both_loops_send_one_envelope_and_differ_only_in_having_a_status() {
    let dir = workspace();
    let script = || {
        vec![
            calls("read_file", serde_json::json!({"path": "code.rs"})),
            calls("complete", serde_json::json!({"rationale": "read it"})),
        ]
    };

    let chat = drive_chat(dir.path(), script());
    let (_, run_requests) = drive_run(dir.path(), script(), &[]);

    let envelope = |request: &ModelRequest| -> serde_json::Value {
        let text = request
            .messages
            .iter()
            .find(|m| m.role == "tool")
            .expect("no tool result reached the second turn")
            .content
            .clone();
        pwr_orchestrator::tool_result_json(&text).expect("a tool result that is not JSON")
    };

    let from_chat = envelope(&chat.requests[1]);
    let from_run = envelope(&run_requests[1]);

    // The same observation, reached the same way, on both sides.
    assert_eq!(
        from_chat["result"]["artifact_hash"],
        from_run["result"]["artifact_hash"]
    );
    assert!(from_chat["result"]["content"].is_string());

    // The one declared difference.
    assert!(from_chat.get("status").is_none(), "{from_chat}");
    assert!(from_run.get("status").is_some(), "{from_run}");
    assert_eq!(
        from_chat.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["result"]
    );
}

/// A refusal is JSON on both sides, carries the same words, and says which kind
/// of fault it was. The conversation used to send prose that said neither.
#[test]
fn a_refusal_tells_the_deployment_what_kind_of_fault_it_was() {
    let dir = workspace();
    // A path outside the workspace: refused by policy, not by the filesystem.
    let outcome = drive_chat(
        dir.path(),
        vec![
            calls("read_file", serde_json::json!({"path": "../secrets"})),
            says("I cannot read that."),
        ],
    );
    let tool = outcome.requests[1]
        .messages
        .iter()
        .find(|m| m.role == "tool")
        .expect("no refusal reached the deployment");
    let envelope: serde_json::Value =
        pwr_orchestrator::tool_result_json(&tool.content).expect("a refusal that is not JSON");
    let result = &envelope["result"];
    assert!(
        result.get("denied").is_some() || result.get("failure_category").is_some(),
        "a refusal with neither a denial nor a category: {envelope}"
    );
    assert!(!envelope.to_string().contains("refused: "), "{envelope}");
}

/// CLOSED, 2026-09-12, in the direction the conversation needed.
///
/// A deployment repeating a refused action is not short of budget; it is not
/// reading the refusal, and more actions buy more repeats. The scripted loop has
/// named that since the campaign that found it. The conversation did not: it
/// could propose the same stale-hash edit until its turn ran out, with nothing
/// naming the repetition and no `loop.detected` in the audit -- so the pathology
/// detectors in `pwr-observe`, which read that event, were blind to a stuck
/// conversation.
///
/// `repetition::RefusalStreak` is the one definition now. This drives the
/// conversation into a loop and asserts it is named, stopped and audited.
#[test]
fn a_conversation_that_repeats_a_refused_action_is_named_and_stopped() {
    let dir = workspace();
    let stale = || {
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": "not-the-hash",
                "find": "one",
                "replace": "two",
            }),
        )
    };
    // Four identical proposals: the first three are refused and counted, the
    // fourth is not run at all.
    let outcome = drive_chat(
        dir.path(),
        vec![
            stale(),
            stale(),
            stale(),
            stale(),
            says("I could not do it."),
        ],
    );

    let notices: Vec<&ModelRequest> = outcome
        .requests
        .iter()
        .filter(|request| {
            request
                .messages
                .iter()
                .any(|message| message.content.contains("proposed this same action"))
        })
        .collect();
    assert!(
        !notices.is_empty(),
        "the repetition was never named to the deployment"
    );
    assert!(
        outcome
            .steps
            .iter()
            .any(|step| step.contains("proposed again after being refused")),
        "{:?}",
        outcome.steps
    );
    // The workspace is untouched either way, which is what makes the repetition
    // the only thing that happened.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("code.rs")).unwrap(),
        "one\n"
    );
}

/// CLOSED, 2026-09-12. The other half of being stuck.
///
/// Repetition is one proposal sent again after a refusal. This is a window of
/// actions that all ran, all succeeded, and left the workspace exactly where it
/// was -- the same files read in a circle being the commonest shape. The
/// scripted loop has named it; the conversation could spend a whole turn that
/// way in silence.
///
/// The second condition is what keeps it honest: nothing in the window may be
/// novel. Six files never read before is investigation, and interrupting that
/// would break exactly the behaviour a hard task needs -- which the second
/// fixture here holds it to.
#[test]
fn a_conversation_reading_in_a_circle_is_told_so() {
    let dir = workspace();
    let read = |path: &str| calls("read_file", serde_json::json!({"path": path}));
    // The same two files round and round. The first pass is novel, so a window
    // of six non-novel actions needs eight reads: the window is what is judged,
    // and it must hold nothing the deployment had not already tried.
    let outcome = drive_chat(
        dir.path(),
        vec![
            read("code.rs"),
            read("parser.rs"),
            read("code.rs"),
            read("parser.rs"),
            read("code.rs"),
            read("parser.rs"),
            read("code.rs"),
            read("parser.rs"),
            says("I keep reading the same things."),
        ],
    );

    assert!(
        outcome.requests.iter().any(|request| request
            .messages
            .iter()
            .any(|message| message.content.contains("no_progress"))),
        "the circle was never named to the deployment"
    );
}

/// Measured on 2026-09-21: a goal-mode session ran 169 actions, the last
/// hundred alternating the same no-op commands, and five no-progress windows
/// never stopped it -- each automatic check-in began a turn with a fresh
/// tracker. The count now lives in the conversation's continuity, so a turn
/// that continues a goal inherits it; a message from the operator resets it.
#[test]
fn stalled_windows_accumulate_across_turns_until_the_work_is_stopped() {
    let dir = workspace();
    let store = pwr_store::Store::open(":memory:").unwrap();
    let policy = policy_for(dir.path());
    let continuity = converse::Continuity::default();
    let read = |path: &str| calls("read_file", serde_json::json!({"path": path}));
    let circle = |reads: usize| -> Vec<ModelChunk> {
        (0..reads)
            .map(|index| {
                read(if index % 2 == 0 {
                    "code.rs"
                } else {
                    "parser.rs"
                })
            })
            .collect()
    };
    let turn = |script: Vec<ModelChunk>| {
        let mut messages = vec![ChatMessage::text("user", "keep going")];
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(converse::take_turn(
                &Scripted::new(script),
                pwr_compat::adapter_for(None, "fake").as_ref(),
                &deployment(),
                &store,
                pwr_domain::new_id(),
                &policy,
                &mut messages,
                8192,
                &[],
                Default::default(),
                pwr_compat::render_tools(&converse::chat_tool_catalog()),
                &std::sync::atomic::AtomicBool::new(false),
                &continuity,
                &pwr_orchestrator::DenyWithoutAsking,
                |_| {},
            ))
            .unwrap()
    };

    // Two stalled windows: named to the deployment, not yet a stop.
    let mut first = circle(14);
    first.push(says("Still looking."));
    assert_eq!(turn(first).stopped, None);

    // A goal's check-in is not the operator: the third window stops it.
    let mut second = circle(6);
    second.push(says("Still looking."));
    assert_eq!(turn(second).stopped, Some(converse::StopReason::NoProgress));

    // The operator speaking is a new request, and gets a fresh count.
    continuity.operator_spoke();
    let mut third = circle(6);
    third.push(says("Looked again."));
    assert_eq!(turn(third).stopped, None);
}

/// Reading files it has not read before is investigation, and a harness that
/// interrupts it has broken the behaviour a hard task needs.
#[test]
fn a_conversation_reading_new_files_is_left_alone() {
    let dir = workspace();
    for name in ["a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs"] {
        std::fs::write(dir.path().join(name), format!("// {name}\n")).unwrap();
    }
    let read = |path: &str| calls("read_file", serde_json::json!({"path": path}));
    let outcome = drive_chat(
        dir.path(),
        vec![
            read("a.rs"),
            read("b.rs"),
            read("c.rs"),
            read("d.rs"),
            read("e.rs"),
            read("f.rs"),
            read("code.rs"),
            says("Here is what they do."),
        ],
    );

    assert!(
        !outcome.requests.iter().any(|request| request
            .messages
            .iter()
            .any(|message| message.content.contains("no_progress"))),
        "investigation was interrupted as though it were a circle"
    );
}

/// CLOSED, 2026-09-12, and the cost of not having it was the worst shape a bug
/// can take: the workspace moved and the conversation forgot.
///
/// Any provider error other than a truncated or unparseable reply ended the
/// turn with `Err`. A failed turn returns no messages to the console, while its
/// edits are already on disk -- so a backend that dropped one connection after
/// the deployment had edited three files left the files edited and the
/// conversation with no record that it had touched them. The next turn would
/// read hashes it had never been told about.
///
/// A backend fault is not the deployment failing, and the two are bounded
/// separately now, consecutively, like every other counter in the loop.
#[test]
fn a_backend_that_drops_a_connection_does_not_cost_the_turn_its_memory() {
    let dir = workspace();
    let provider = Faulty {
        faults: Mutex::new(2),
        inner: Scripted::new(vec![
            calls(
                "replace_text",
                serde_json::json!({
                    "path": "code.rs",
                    "expected_hash": pwr_domain::hash_bytes("one\n"),
                    "find": "one",
                    "replace": "two",
                }),
            ),
            says("Changed it."),
        ]),
    };

    let outcome = drive_chat_with(dir.path(), provider, &[]);
    assert!(
        outcome.report.stopped.is_none(),
        "{:?}",
        outcome.report.stopped
    );
    assert_eq!(outcome.report.answer, "Changed it.");
    assert!(outcome.report.edited);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("code.rs")).unwrap(),
        "two\n"
    );
    // Reported rather than absorbed: two faults happened and the operator can
    // see they did.
    assert_eq!(
        outcome
            .steps
            .iter()
            .filter(|step| step.contains("connection refused"))
            .count(),
        2,
        "{:?}",
        outcome.steps
    );
}

/// CLOSED, 2026-09-15. A reply the backend could not parse reached the
/// deployment in two shapes: the run sent `{"unparsed_output": "..."}` and the
/// conversation the bare sentence. The words were already one definition; the
/// envelope is now one too, and the run's two identical copies of the branch
/// that sends it are one function.
#[test]
fn an_unreadable_reply_is_told_the_same_way_by_both_loops() {
    let told = |requests: &[ModelRequest]| -> ChatMessage {
        requests[1]
            .messages
            .iter()
            .rev()
            .find(|message| message.content.contains("could not be read"))
            .cloned()
            .expect("the fault was never told")
    };

    let dir = workspace();
    let chat = drive_chat_with(
        dir.path(),
        Unreadable {
            faults: Mutex::new(1),
            inner: Scripted::new(vec![says("Done.")]),
        },
        &[],
    );
    assert_eq!(chat.report.answer, "Done.");

    let dir = workspace();
    let (_, run_requests) = drive_run_with(
        dir.path(),
        Unreadable {
            faults: Mutex::new(1),
            inner: Scripted::new(vec![calls(
                "complete",
                serde_json::json!({"rationale": "nothing to do"}),
            )]),
        },
        &[],
    );

    let from_chat = told(&chat.requests);
    let from_run = told(&run_requests);
    assert_eq!(from_chat.role, from_run.role);
    assert_eq!(from_chat.content, from_run.content);
    let parsed: serde_json::Value = pwr_orchestrator::tool_result_json(&from_run.content).unwrap();
    assert!(
        parsed["unparsed_output"]
            .as_str()
            .unwrap()
            .contains("XML syntax error")
    );
}

#[test]
fn a_measured_window_is_chosen_by_one_rule() {
    assert_eq!(
        pwr_orchestrator::lower_measured_tier(8192, &[2048, 4096, 16384]),
        Some(4096)
    );
    assert_eq!(
        pwr_orchestrator::lower_measured_tier(8192, &[0, 16384]),
        None
    );
    assert_eq!(pwr_orchestrator::lower_measured_tier(2048, &[2048]), None);
}

/// A backend that is down stays down, and the turn says which of the two it is.
#[test]
fn a_backend_that_stays_down_stops_the_turn_and_says_it_was_the_server() {
    let dir = workspace();
    let provider = Faulty {
        faults: Mutex::new(99),
        inner: Scripted::new(vec![says("never reached")]),
    };
    let outcome = drive_chat_with(dir.path(), provider, &[]);
    assert_eq!(
        outcome.report.stopped,
        Some(converse::StopReason::BackendFailing)
    );
    let said = converse::StopReason::BackendFailing.said();
    assert!(said.contains("the server, not the model"), "{said}");
}

/// CLOSED, 2026-09-12. The last recovery the scripted loop had and a turn did
/// not, and the reason it was missing was plumbing rather than evidence.
///
/// When the backend itself says the prompt does not fit, dropping to a window
/// calibration measured keeps the conversation whole, where compacting spends
/// part of it. The turn could only compact, because it was never given the
/// tiers -- while the chat config has carried a calibration profile all along,
/// and its preparation already reads the same stable points to choose the
/// window in the first place.
///
/// Requested and then taken as granted, the way the console asks for its window
/// at startup: what the backend serves is what the turn is measured against.
#[test]
fn a_refused_prompt_drops_to_a_measured_window_before_it_spends_the_conversation() {
    let dir = workspace();
    let provider = Cramped {
        refusals: Mutex::new(1),
        prepared: Mutex::new(Vec::new()),
        inner: Scripted::new(vec![says("That fits now.")]),
    };

    // Measured tiers below the 8192 the driver asks for, and one above it that
    // must not be chosen: the point is to go down.
    let outcome = drive_chat_with(dir.path(), provider, &[2048, 4096, 16384]);

    assert_eq!(outcome.report.answer, "That fits now.");
    assert!(outcome.report.stopped.is_none());
    assert!(
        outcome
            .steps
            .iter()
            .any(|step| step.contains("4096") && step.contains("calibration measured")),
        "the drop was not reported, or went to the wrong tier: {:?}",
        outcome.steps
    );
    // Compaction was not spent on a problem a measured window solved.
    assert!(
        !outcome
            .steps
            .iter()
            .any(|step| step.starts_with("compacted")),
        "{:?}",
        outcome.steps
    );
}

/// A backend that cannot be calibrated has no ladder to descend, and the turn
/// falls back to the prompt rather than inventing a window nobody measured.
#[test]
fn without_measured_tiers_a_refused_prompt_is_compacted_instead() {
    let dir = workspace();
    let provider = Cramped {
        refusals: Mutex::new(1),
        prepared: Mutex::new(Vec::new()),
        inner: Scripted::new(vec![says("Smaller now.")]),
    };
    let outcome = drive_chat_with(dir.path(), provider, &[]);
    assert!(
        outcome.report.stopped.is_some() || outcome.report.answer == "Smaller now.",
        "{:?}",
        outcome.report.stopped
    );
    assert!(
        !outcome
            .steps
            .iter()
            .any(|step| step.contains("calibration measured")),
        "a window was invented where none was measured: {:?}",
        outcome.steps
    );
}

/// Stop reaches a command that is running, not only the gap after it. Seen
/// 2026-09-23: stop pressed during an `npm install` that hung, and nothing
/// happened until the command's timeout.
#[test]
fn stop_ends_a_running_command_without_waiting_for_it() {
    let dir = workspace();
    let policy = ToolPolicy {
        allow_commands: vec!["sleep".into()],
        timeout: Duration::from_secs(30),
        ..policy_for(dir.path())
    };
    let provider = Scripted::new(vec![calls(
        "run_command",
        serde_json::json!({"executable": "sleep", "args": ["20"]}),
    )]);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pressing = std::sync::Arc::clone(&stop);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        pressing.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let started = std::time::Instant::now();
    let outcome = drive_chat_under(policy, provider, &[], &stop);
    assert_eq!(
        outcome.report.stopped,
        Some(converse::StopReason::Interrupted)
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "stop waited {:?} for the command",
        started.elapsed()
    );
}

/// R1's exit names cancellation among the cases that must pass. Stop reaches a
/// generation already in flight -- abandoning the read drops the stream, which
/// drops the body, which closes the socket -- and the turn reports what it did
/// before that rather than discarding it.
#[test]
fn a_cancelled_turn_stops_and_still_reports_what_it_had_done() {
    let dir = workspace();
    // Two replies served, then the flag: the edit of the first is executed, and
    // the stop arrives while the second generation is in flight. That is where
    // an operator's stop actually lands, and the turn takes it between actions
    // rather than inside one -- so the second reply carries a call that is
    // never run.
    let provider = Stopping {
        stop_after: Mutex::new(2),
        stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        inner: Scripted::new(vec![
            calls(
                "replace_text",
                serde_json::json!({
                    "path": "code.rs",
                    "expected_hash": pwr_domain::hash_bytes("one\n"),
                    "find": "one",
                    "replace": "two",
                }),
            ),
            calls("read_file", serde_json::json!({"path": "code.rs"})),
        ]),
    };
    let outcome = drive_chat_stopping(dir.path(), provider);

    assert_eq!(
        outcome.report.stopped,
        Some(converse::StopReason::Interrupted)
    );
    // The edit that landed before the stop is still reported as a change, and
    // the file on disk agrees. A turn that discarded it would leave the
    // workspace moved and the conversation saying it had not moved.
    assert!(outcome.report.edited, "the edit before the stop was lost");
    assert_eq!(
        std::fs::read_to_string(dir.path().join("code.rs")).unwrap(),
        "two\n"
    );
    assert_eq!(outcome.report.actions, 1);
}

/// R1's exit names user redirection. A conversation's objective is restated
/// every turn by the person, and what a redirect must not do is lose what came
/// before it or rank the new request against the old one's passages.
#[test]
fn a_redirected_conversation_keeps_its_history_and_ranks_the_new_request() {
    let dir = workspace();
    let root = dir.path();
    std::fs::write(
        root.join("parser.rs"),
        "fn parse_manifest(text: &str) -> Manifest { todo!() }\n",
    )
    .unwrap();
    let profile = pwr_orchestrator::TaskProfile::resolve(None, None);

    let mut messages = vec![
        ChatMessage::text("system", converse::chat_system_prompt(root)),
        ChatMessage::text("user", "look at code.rs"),
    ];
    crate::compose_chat_turn(root, 8192, &profile, None, &mut messages).unwrap();
    messages.push(ChatMessage::text("assistant", "It says one."));

    // The redirect: a new request, on the same conversation.
    messages.push(ChatMessage::text(
        "user",
        "actually, fix parse_manifest in the parser",
    ));
    let before = messages.len();
    crate::compose_chat_turn(root, 8192, &profile, None, &mut messages).unwrap();

    // What came before survives, including the first turn's own composition.
    assert!(
        messages.iter().any(|m| m.content == "It says one."),
        "the redirect discarded the conversation"
    );
    assert!(messages.len() > before, "the new request composed nothing");
    // The passages are ranked against what was just asked, not against what was
    // asked first: a redirect that retrieved for the old objective would send
    // the deployment back to the file the person just moved off.
    let fresh = messages
        .iter()
        .rev()
        .find(|m| m.purpose == Some(pwr_domain::MessagePurpose::RepositoryExcerpts))
        .expect("no passages for the redirected request");
    assert!(
        fresh.content.contains("parse_manifest"),
        "{}",
        fresh.content
    );
    assert_eq!(
        messages.last().unwrap().content,
        "actually, fix parse_manifest in the parser"
    );
}

/// DECLARED, deliberately, and the difference is the person in front of it.
///
/// A conversation had no action budget at all. Its only bounds were the
/// operator pressing stop, three silent turns, three unreadable replies and two
/// compactions; between one action and the next, nothing counted. MASTER_SPEC's
/// eighth principle says to bound resources and count every underlying action,
/// and this was the path that did not.
///
/// A soft bound rather than the run's hard one. A run is unattended and its
/// budget is a cap; a conversation has someone in front of it, so the budget is
/// a place to check in. The turn ends with a reason and the work intact, and
/// the next message carries on.
#[test]
fn a_turn_stops_to_check_in_rather_than_running_on_unasked() {
    let dir = workspace();
    let read = |n: usize| {
        std::fs::write(dir.path().join(format!("f{n}.rs")), format!("// {n}\n")).unwrap();
        calls("read_file", serde_json::json!({"path": format!("f{n}.rs")}))
    };
    // More actions than a turn is given, every one of them novel, so nothing
    // else in the loop would have stopped it.
    let script: Vec<_> = (0..40).map(read).collect();

    let outcome = drive_chat(dir.path(), script);
    assert_eq!(
        outcome.report.stopped,
        Some(converse::StopReason::BudgetSpent)
    );
    // The count is the budget, and the turn did not quietly overrun it.
    assert_eq!(outcome.report.actions, 26);
    // What the operator reads has to sound like a check-in and not a failure.
    let said = converse::StopReason::BudgetSpent.said();
    assert!(said.contains("carry on"), "{said}");
    assert!(said.contains("discarded"), "{said}");
}

/// CLOSED, 2026-09-15. A run's audit ends with its terminal class and a turn's
/// ended with nothing, so a turn that ran out of actions and a run that did
/// read differently to everything downstream of the log. Both classify it as
/// `budget` now. How much budget each has stays DECLARED: soft for a turn, a
/// cap for a run.
#[test]
fn a_turn_and_a_run_that_run_out_of_actions_are_classified_alike() {
    let script = |dir: &std::path::Path, actions: usize| -> Vec<ModelChunk> {
        (0..actions)
            .map(|n| {
                std::fs::write(dir.join(format!("f{n}.rs")), format!("// {n}\n")).unwrap();
                calls("read_file", serde_json::json!({"path": format!("f{n}.rs")}))
            })
            .collect()
    };
    let (chat_dir, run_dir) = (workspace(), workspace());
    let chat_script = script(chat_dir.path(), 40);
    let run_script = script(run_dir.path(), 40);
    let [chat, run] = drive_both(
        chat_dir.path(),
        run_dir.path(),
        policy_for,
        chat_script,
        run_script,
    );
    let chat_terminal = chat
        .0
        .events_for_run(chat.1)
        .unwrap()
        .into_iter()
        .find(|event| event.event_type == converse::TURN_ENDED_EVENT)
        .expect("the turn recorded no ending")
        .payload["terminal"]
        .clone();
    let run_terminal = run
        .0
        .events_for_run(run.1)
        .unwrap()
        .into_iter()
        .find(|event| event.event_type == "task.failed")
        .expect("the run recorded no ending")
        .payload["class"]
        .clone();
    assert_eq!(chat_terminal, "budget");
    assert_eq!(chat_terminal, run_terminal);
}

/// CLOSED, 2026-09-15. A call that does not decode performed nothing. The run
/// did not charge it as an action and stopped after a few in a row; a turn
/// charged each to its budget and could spend all of it on calls that never
/// decoded. Both now leave the budget alone and end on repeated malformed calls
/// with the same terminal class. The run's bound comes from the deployment's
/// measured emission rate and the turn's is fixed at three -- DECLARED, since a
/// conversation is not given a capability profile to measure it from.
#[test]
fn repeated_malformed_calls_end_both_loops_as_protocol_without_spending_actions() {
    let broken = || calls("read_file", serde_json::json!({"no_path": true}));
    let (chat_dir, run_dir) = (workspace(), workspace());
    let [chat, run] = drive_both(
        chat_dir.path(),
        run_dir.path(),
        policy_for,
        vec![broken(), broken(), broken(), says("unreached")],
        (0..6).map(|_| broken()).collect(),
    );
    let ending = |(store, id, _): &(pwr_store::Store, pwr_domain::Id, Vec<ModelRequest>)| {
        store
            .events_for_run(*id)
            .unwrap()
            .into_iter()
            .rev()
            .find(|event| {
                event.event_type == converse::TURN_ENDED_EVENT || event.event_type == "task.failed"
            })
            .expect("no ending recorded")
            .payload
    };
    let chat_end = ending(&chat);
    let run_end = ending(&run);
    assert_eq!(chat_end["terminal"], "protocol", "{chat_end}");
    assert_eq!(chat_end["actions"], 0, "{chat_end}");
    assert_eq!(run_end["class"], "protocol", "{run_end}");
    let run_actions = run
        .0
        .events_for_run(run.1)
        .unwrap()
        .iter()
        .filter(|event| event.event_type == "tool.action")
        .count();
    assert_eq!(run_actions, 0);
}

/// DECLARED, 2026-09-15, as R1 closes. A backend that drops one connection ends
/// a run and not a turn. The run is unattended and a provider failure is a
/// measurement class every campaign reports separately, so absorbing it would
/// hide an unhealthy backend inside a slower trial. The turn has a person in
/// front of it, retries up to three in a row, and says each one.
#[test]
fn a_dropped_connection_ends_a_run_and_is_retried_by_a_turn() {
    let dir = workspace();
    let chat = drive_chat_with(
        dir.path(),
        Faulty {
            faults: Mutex::new(1),
            inner: Scripted::new(vec![says("Recovered.")]),
        },
        &[],
    );
    assert_eq!(chat.report.answer, "Recovered.");

    let dir = workspace();
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let provider = Faulty {
        faults: Mutex::new(1),
        inner: Scripted::new(vec![calls(
            "complete",
            serde_json::json!({"rationale": "x"}),
        )]),
    };
    let result =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(pwr_orchestrator::run_action_loop(
                &store,
                &provider,
                id,
                ModelRequest {
                    deployment: deployment(),
                    messages: vec![ChatMessage::text("user", "do the thing")],
                    context_tokens: 8192,
                    tools: None,
                    seed: None,
                    sampling: Default::default(),
                },
                &policy_for(dir.path()),
                &[],
                12,
            ));
    assert!(result.is_err());
    let class = store
        .events_for_run(id)
        .unwrap()
        .into_iter()
        .find(|event| event.event_type == "task.failed")
        .unwrap()
        .payload["class"]
        .clone();
    assert_eq!(class, "provider");
}

/// A turn reports each action as an object a front end can track: announced,
/// then completed with the file before and after for an edit, or refused. The
/// console keeps rendering lines; `pwr serve` turns these into ACP tool calls.
#[test]
fn a_turn_reports_actions_as_tool_calls_with_diffs() {
    let dir = workspace();
    std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
    let provider = Scripted::new(vec![
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": pwr_domain::hash_bytes("one\n"),
                "find": "one",
                "replace": "two",
            }),
        ),
        calls(
            "replace_text",
            serde_json::json!({
                "path": "Cargo.toml",
                "expected_hash": pwr_domain::hash_bytes("[package]\n"),
                "find": "[package]",
                "replace": "[package]\nx = 1",
            }),
        ),
        says("Done."),
    ]);
    let store = pwr_store::Store::open(":memory:").unwrap();
    let policy = ToolPolicy {
        approvals: Vec::new(),
        ..policy_for(dir.path())
    };
    let mut messages = vec![ChatMessage::text("user", "do the thing")];
    let mut calls_seen: Vec<converse::ToolCallStep> = Vec::new();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &provider,
            pwr_compat::adapter_for(None, "fake").as_ref(),
            &deployment(),
            &store,
            pwr_domain::new_id(),
            &policy,
            &mut messages,
            8192,
            &[],
            Default::default(),
            pwr_compat::render_tools(&converse::chat_tool_catalog()),
            &std::sync::atomic::AtomicBool::new(false),
            &converse::Continuity::default(),
            &pwr_orchestrator::DenyWithoutAsking,
            |step| {
                if let converse::TurnStep::ToolCall(call) = step {
                    calls_seen.push(call);
                }
            },
        ))
        .unwrap();

    assert_eq!(calls_seen.len(), 5, "{calls_seen:?}");
    // Proposed before the policy is asked, so a front end has something to
    // attach an approval question to.
    assert_eq!(calls_seen[0].phase, converse::ToolPhase::Proposed);
    assert_eq!(calls_seen[1].id, calls_seen[0].id);
    assert_eq!(calls_seen[1].phase, converse::ToolPhase::Started);
    assert_eq!(calls_seen[2].id, calls_seen[0].id);
    assert_eq!(calls_seen[2].phase, converse::ToolPhase::Completed);
    assert_eq!(
        calls_seen[2].diff,
        Some(converse::FileDiff {
            path: "code.rs".into(),
            old_text: Some("one\n".into()),
            new_text: "two\n".into(),
        })
    );
    assert_ne!(calls_seen[3].id, calls_seen[0].id);
    assert_eq!(calls_seen[3].phase, converse::ToolPhase::Proposed);
    assert_eq!(calls_seen[4].id, calls_seen[3].id);
    assert_eq!(calls_seen[4].path.as_deref(), Some("Cargo.toml"));
    assert!(
        matches!(&calls_seen[4].phase, converse::ToolPhase::Refused(why) if why.contains("a person refused this")),
        "{:?}",
        calls_seen[4].phase
    );
}

/// Reads one file per turn, then answers, and reports the prompt count a real
/// backend reports: the messages plus the fixed cost of the tool schemas and
/// the chat template, which are in every request and in none of the messages.
struct Counting {
    files: Vec<String>,
    turn: Mutex<usize>,
    overhead: u64,
}

#[async_trait]
impl ModelProvider for Counting {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let estimated: u64 = request
            .messages
            .iter()
            .map(|message| message.content.len().div_ceil(4) as u64)
            .sum();
        let turn = {
            let mut turn = self.turn.lock().unwrap();
            *turn += 1;
            *turn - 1
        };
        let mut chunk = match self.files.get(turn) {
            Some(path) => calls("read_file", serde_json::json!({"path": path})),
            None => says("Done."),
        };
        chunk.metrics = Some(pwr_domain::GenerationMetrics {
            prompt_tokens: Some(estimated + self.overhead),
            ..Default::default()
        });
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
}

/// D6: the history budget is enforced on what the backend counts, and what it
/// counts beyond the messages is a fixed cost, not a multiple.
///
/// The first correction read the gap as a ratio. A campaign caught it in three
/// trials: after a compaction the history is small and the fixed cost is not,
/// so the ratio rose, the budget shrank, and the next compaction followed --
/// twenty-seven of them in three trials against two. Here a backend with a
/// large fixed cost must compact when the history genuinely outgrows what is
/// left, and must not compact again and again on a history that is already
/// small.
#[test]
fn a_fixed_prompt_cost_is_not_read_as_a_shrinking_budget() {
    fn compactions_with(overhead: u64, files: usize, bytes: usize) -> usize {
        let dir = workspace();
        let names: Vec<String> = (0..files).map(|n| format!("part{n}.rs")).collect();
        for name in &names {
            std::fs::write(dir.path().join(name), "fn one() {}\n".repeat(bytes)).unwrap();
        }
        let store = pwr_store::Store::open(":memory:").unwrap();
        let provider = Counting {
            files: names,
            turn: Mutex::new(0),
            overhead,
        };
        let run_id = pwr_domain::new_id();
        let request = ModelRequest {
            deployment: deployment(),
            messages: vec![ChatMessage::text("user", "do the thing")],
            context_tokens: 8192,
            tools: None,
            seed: None,
            sampling: Default::default(),
        };
        let _ =
            tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(pwr_orchestrator::run_action_loop(
                    &store,
                    &provider,
                    run_id,
                    request,
                    &policy_for(dir.path()),
                    &[],
                    8,
                ));
        store
            .events_for_run(run_id)
            .unwrap()
            .into_iter()
            .filter(|event| event.event_type == "context.compacted")
            .count()
    }

    // A small history and a large fixed cost: the request is nowhere near the
    // window, and nothing should be folded away.
    assert_eq!(
        compactions_with(2_900, 6, 20),
        0,
        "a fixed cost was read as a reason to compact a short history"
    );
    // The same fixed cost with a history that really does outgrow its share.
    assert!(
        compactions_with(2_900, 6, 200) > 0,
        "a history past its budget was not compacted"
    );
    // And it is the count that decides, not the estimate: without the fixed
    // cost the same history still fits.
    assert_eq!(compactions_with(0, 6, 90), 0);
}

/// Every way a turn stops maps to the run's class for the same stop.
#[test]
fn every_stop_reason_has_a_terminal_class() {
    use converse::StopReason;
    use pwr_domain::TerminalClass;
    // `StopReason::ALL` is test-only inside its own crate; the list is repeated
    // here, and a variant added there fails that crate's exhaustive match.
    for reason in [
        StopReason::Interrupted,
        StopReason::ContextFull,
        StopReason::Looping,
        StopReason::Silent,
        StopReason::ToolCallInReasoning,
        StopReason::Unparseable,
        StopReason::BudgetSpent,
        StopReason::BackendFailing,
        StopReason::NoProgress,
    ] {
        let class = reason.terminal_class();
        assert_ne!(class, TerminalClass::Unclassified, "{reason:?}");
    }
    assert_eq!(
        converse::StopReason::Interrupted.terminal_class(),
        TerminalClass::Interrupted
    );
    assert_eq!(
        converse::StopReason::BackendFailing.terminal_class(),
        TerminalClass::Provider
    );
}

/// DECLARED, and only half so. A conversation is offered every capability and
/// the tools it may call are rendered into the request; the scripted loop
/// carries `tools: None` and puts the catalogue in the prompt text instead.
/// Which of the two a deployment answers better is H1, and it cannot be asked
/// while the answer depends on which loop was used.
#[test]
fn only_one_of_the_two_sends_a_tool_schema() {
    let dir = workspace();
    let script = || vec![calls("complete", serde_json::json!({"rationale": "done"}))];

    let chat = drive_chat(dir.path(), script());
    let (_, run_requests) = drive_run(dir.path(), script(), &[]);

    assert!(chat.requests[0].tools.is_some());
    assert!(run_requests[0].tools.is_none());
}

// ---------------------------------------------- steering and continuity

/// Queues a message while serving its first reply, the way a person types
/// while a turn is generating.
struct Typing {
    typed: Mutex<Option<String>>,
    queue: std::sync::Arc<Mutex<Vec<String>>>,
    inner: Scripted,
}

impl Recording for Typing {
    fn requests(&self) -> Vec<ModelRequest> {
        self.inner.requests()
    }
}

#[async_trait]
impl ModelProvider for Typing {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let reply = self.inner.chat(request).await;
        let typed = self.typed.lock().unwrap().take();
        if let Some(text) = typed {
            self.queue.lock().unwrap().push(text);
        }
        reply
    }
}

/// Drives one conversation turn with the continuity the console would hold,
/// returning the store so its events can be read.
fn drive_continuing<P: Recording>(
    root: &std::path::Path,
    provider: &P,
    continuity: &converse::Continuity,
) -> (pwr_store::Store, pwr_domain::Id, Vec<String>) {
    let adapter = pwr_compat::adapter_for(None, "fake");
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let policy = policy_for(root);
    let tools = pwr_compat::render_tools(&converse::chat_tool_catalog());
    let mut messages = vec![ChatMessage::text("user", "do the thing")];
    let mut steps = Vec::new();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            provider,
            adapter.as_ref(),
            &deployment(),
            &store,
            id,
            &policy,
            &mut messages,
            8192,
            &[],
            Default::default(),
            tools,
            &std::sync::atomic::AtomicBool::new(false),
            continuity,
            &pwr_orchestrator::DenyWithoutAsking,
            |step| {
                if let converse::TurnStep::Steered(text) = step {
                    steps.push(text);
                }
            },
        ))
        .expect("the turn returned an error rather than a report");
    (store, id, steps)
}

/// A message typed while a turn works reaches the deployment before its next
/// action, not after the turn -- and opens a revision of the objective the log
/// records. The console used to answer "wait for this to finish".
#[test]
fn a_message_typed_while_a_turn_works_reaches_it_before_the_next_action() {
    let dir = workspace();
    let continuity = converse::Continuity::default();
    let provider = Typing {
        typed: Mutex::new(Some("look at parser.rs instead".into())),
        queue: std::sync::Arc::clone(&continuity.steering),
        inner: Scripted::new(vec![
            calls("read_file", serde_json::json!({"path": "code.rs"})),
            says("Looked at parser.rs."),
        ]),
    };
    let (store, id, steps) = drive_continuing(dir.path(), &provider, &continuity);
    let requests = provider.requests();
    let carries = |request: &ModelRequest| {
        request
            .messages
            .iter()
            .any(|message| message.role == "user" && message.content.contains("parser.rs instead"))
    };
    assert!(!carries(&requests[0]), "delivered before it was typed");
    assert!(
        carries(&requests[1]),
        "the typed message did not reach the next request"
    );
    assert_eq!(steps, vec!["look at parser.rs instead".to_string()]);
    let steered: Vec<serde_json::Value> = store
        .events_for_run(id)
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == pwr_orchestrator::conversation::STEERED_EVENT)
        .map(|event| event.payload)
        .collect();
    assert_eq!(steered.len(), 1);
    assert_eq!(steered[0]["revision"], 1);
    assert_eq!(steered[0]["after_actions"], 1);
    assert!(continuity.steering.lock().unwrap().is_empty());
}

/// Drives one conversation turn and one run over the same script and policy,
/// returning each side's store, id and requests.
fn drive_both(
    root_chat: &std::path::Path,
    root_run: &std::path::Path,
    policy: impl Fn(&std::path::Path) -> ToolPolicy,
    chat_script: Vec<ModelChunk>,
    run_script: Vec<ModelChunk>,
) -> [(pwr_store::Store, pwr_domain::Id, Vec<ModelRequest>); 2] {
    let chat_provider = Scripted::new(chat_script);
    let chat_store = pwr_store::Store::open(":memory:").unwrap();
    let chat_id = pwr_domain::new_id();
    let mut messages = vec![ChatMessage::text("user", "do the thing")];
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &chat_provider,
            pwr_compat::adapter_for(None, "fake").as_ref(),
            &deployment(),
            &chat_store,
            chat_id,
            &policy(root_chat),
            &mut messages,
            8192,
            &[],
            Default::default(),
            pwr_compat::render_tools(&converse::chat_tool_catalog()),
            &std::sync::atomic::AtomicBool::new(false),
            &converse::Continuity::default(),
            &pwr_orchestrator::DenyWithoutAsking,
            |_| {},
        ))
        .unwrap();

    let run_provider = Scripted::new(run_script);
    let run_store = pwr_store::Store::open(":memory:").unwrap();
    let run_id = pwr_domain::new_id();
    let _ = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(pwr_orchestrator::run_action_loop(
            &run_store,
            &run_provider,
            run_id,
            ModelRequest {
                deployment: deployment(),
                messages: vec![ChatMessage::text("user", "do the thing")],
                context_tokens: 8192,
                tools: None,
                seed: None,
                sampling: Default::default(),
            },
            &policy(root_run),
            &[],
            12,
        ));
    [
        (chat_store, chat_id, chat_provider.requests()),
        (run_store, run_id, run_provider.requests()),
    ]
}

/// CLOSED, 2026-09-15. A turn executed every action against a fresh read
/// history, so a re-read of a file it had just been shown never said so; the
/// run's did. The history belongs to the shared step now, per turn and per run.
#[test]
fn both_loops_say_when_a_file_is_read_again_unchanged() {
    let read = || calls("read_file", serde_json::json!({"path": "code.rs"}));
    let (chat_dir, run_dir) = (workspace(), workspace());
    let [chat, run] = drive_both(
        chat_dir.path(),
        run_dir.path(),
        policy_for,
        vec![read(), read(), says("Read it.")],
        vec![
            read(),
            read(),
            calls("complete", serde_json::json!({"rationale": "read it"})),
        ],
    );
    for (name, (store, id, _)) in [("chat", &chat), ("run", &run)] {
        let reads: Vec<serde_json::Value> = store
            .events_for_run(*id)
            .unwrap()
            .into_iter()
            .filter(|event| {
                event.event_type == "tool.action"
                    && event.payload["action"]["capability"] == "read_file"
            })
            .map(|event| event.payload["outcome"].clone())
            .collect();
        assert_eq!(reads.len(), 2, "{name}");
        assert!(reads[0]["already_read"].is_null(), "{name}");
        assert_eq!(reads[1]["already_read"]["unchanged_since"], true, "{name}");
    }
}

/// CLOSED, 2026-09-15. An action needing an approval the policy did not hold
/// was put to a person by the run and recorded as its decision; a turn sent it
/// straight to the tool, which refused it with no `approval.decision` in the
/// audit. Both go through one gate now, and are told the same thing.
///
/// DECLARED, and unchanged: the console grants what Settings allow before a
/// turn starts, so for the product this gate is reached only for what Settings
/// do not allow, and a turn has no way to ask mid-turn.
#[test]
fn both_loops_gate_an_approval_the_same_way() {
    let unapproved = |root: &std::path::Path| ToolPolicy {
        approvals: Vec::new(),
        ..policy_for(root)
    };
    let edit = || {
        calls(
            "replace_text",
            serde_json::json!({
                "path": "Cargo.toml",
                "expected_hash": pwr_domain::hash_bytes("[package]\n"),
                "find": "[package]",
                "replace": "[package]\nevil = true",
            }),
        )
    };
    let (chat_dir, run_dir) = (workspace(), workspace());
    for dir in [&chat_dir, &run_dir] {
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
    }
    let [chat, run] = drive_both(
        chat_dir.path(),
        run_dir.path(),
        unapproved,
        vec![edit(), says("It was refused.")],
        vec![
            edit(),
            calls("complete", serde_json::json!({"rationale": "refused"})),
        ],
    );
    let decision = |(store, id, _): &(pwr_store::Store, pwr_domain::Id, Vec<ModelRequest>)| {
        let events = store.events_for_run(*id).unwrap();
        let asked: Vec<serde_json::Value> = events
            .iter()
            .filter(|event| event.event_type == "approval.decision")
            .map(|event| event.payload.clone())
            .collect();
        assert_eq!(asked.len(), 1);
        assert_eq!(asked[0]["decision"], "deny");
        asked[0]["description"].clone()
    };
    assert_eq!(decision(&chat), decision(&run));
    for dir in [&chat_dir, &run_dir] {
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Cargo.toml")).unwrap(),
            "[package]\n"
        );
    }
    let told = |requests: &[ModelRequest]| {
        requests[1]
            .messages
            .iter()
            .rev()
            .find(|message| message.content.contains("a person refused this"))
            .map(|message| message.content.clone())
            .expect("the refusal was never told")
    };
    assert_eq!(told(&chat.2), told(&run.2));
}

/// CLOSED, 2026-09-15. The conversation announced a workspace-changing action
/// before it ran, receipted it after and checkpointed what it had changed; the
/// scripted run did none of it, so an interrupted run left nothing to tell a
/// finished write from an uncertain one. Both now leave the same trail, from one
/// definition of which actions need it.
#[test]
fn both_loops_announce_receipt_and_checkpoint_an_edit_the_same_way() {
    let edit = || {
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": pwr_domain::hash_bytes("one\n"),
                "find": "one",
                "replace": "two",
            }),
        )
    };
    let trail = |store: &pwr_store::Store, id| {
        let events = store.events_for_run(id).unwrap();
        let of = |kind: &str| -> Vec<serde_json::Value> {
            events
                .iter()
                .filter(|event| event.event_type == kind)
                .map(|event| event.payload.clone())
                .collect()
        };
        let types: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
        let at = |kind: &str| types.iter().position(|t| *t == kind).expect(kind);
        assert!(at("action.intent") < at("action.receipt"), "{types:?}");
        assert!(
            at("action.receipt") < at("conversation.checkpoint"),
            "{types:?}"
        );
        (
            of("action.intent"),
            of("action.receipt"),
            of("conversation.checkpoint").last().unwrap()["changed_files"].clone(),
        )
    };

    let dir = workspace();
    let continuity = converse::Continuity::default();
    let provider = Scripted::new(vec![edit(), says("Changed it.")]);
    let (store, id, _) = drive_continuing(dir.path(), &provider, &continuity);
    let chat = trail(&store, id);

    let dir = workspace();
    let provider = Scripted::new(vec![
        edit(),
        calls("complete", serde_json::json!({"rationale": "changed it"})),
    ]);
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(pwr_orchestrator::run_action_loop(
            &store,
            &provider,
            id,
            ModelRequest {
                deployment: deployment(),
                messages: vec![ChatMessage::text("user", "do the thing")],
                context_tokens: 8192,
                tools: None,
                seed: None,
                sampling: Default::default(),
            },
            &policy_for(dir.path()),
            &[],
            12,
        ))
        .unwrap();
    let run = trail(&store, id);

    assert_eq!(chat, run);
    assert_eq!(run.0[0]["capability"], "replace_text");
    assert_eq!(run.0[0]["path"], "code.rs");
    assert_eq!(run.2["code.rs"], pwr_domain::hash_bytes("two\n"));
}

/// An edit is announced before it runs, receipted after, and leaves a
/// checkpoint naming the file and the content it left: what `--continue` needs
/// to tell a finished write from an uncertain one and a kept file from one
/// somebody changed since.
#[test]
fn an_edit_leaves_an_intent_a_receipt_and_a_checkpoint() {
    let dir = workspace();
    let continuity = converse::Continuity::default();
    let provider = Scripted::new(vec![
        calls(
            "replace_text",
            serde_json::json!({
                "path": "code.rs",
                "expected_hash": pwr_domain::hash_bytes("one\n"),
                "find": "one",
                "replace": "two",
            }),
        ),
        says("Changed it."),
    ]);
    let (store, id, _) = drive_continuing(dir.path(), &provider, &continuity);
    let types: Vec<String> = store
        .events_for_run(id)
        .unwrap()
        .into_iter()
        .map(|event| event.event_type)
        .collect();
    let at = |kind: &str| types.iter().position(|t| t == kind);
    let intent = at("action.intent").expect("no intent recorded");
    let receipt = at("action.receipt").expect("no receipt recorded");
    let checkpoint = at("conversation.checkpoint").expect("no checkpoint recorded");
    assert!(intent < receipt && receipt < checkpoint, "{types:?}");
    let recorded = continuity.checkpoint.lock().unwrap().clone();
    assert_eq!(recorded.turn, 1);
    assert_eq!(
        recorded.changed_files.get("code.rs"),
        Some(&pwr_domain::hash_bytes("two\n"))
    );
    // Nothing is uncertain after a clean write.
    pwr_orchestrator::conversation::record_snapshot(&store, id, &[]).unwrap();
    let restored = pwr_orchestrator::conversation::restore(&store, id)
        .unwrap()
        .unwrap();
    assert!(restored.unreceipted.is_empty());
    assert!(
        pwr_orchestrator::conversation::reconcile(dir.path(), &restored)
            .changed_since
            .is_empty()
    );
}

/// Refuses the first requests with the errors it was given, then serves.
struct Failing {
    errors: Mutex<VecDeque<ProviderError>>,
    inner: Scripted,
}

#[async_trait]
impl ModelProvider for Failing {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        if let Some(error) = self.errors.lock().unwrap().pop_front() {
            return Err(error);
        }
        self.inner.chat(request).await
    }
}

/// A generation the turn could not use leaves a record with its outcome, so a
/// stall is diagnosable from the log and not only while it streams (D.E2E-22).
#[test]
fn a_failed_generation_is_recorded_with_its_outcome() {
    let dir = workspace();
    let provider = Failing {
        errors: Mutex::new(
            vec![
                ProviderError::ModelOutput {
                    safe_context: "error parsing tool call".into(),
                },
                ProviderError::Truncated {
                    safe_context: "reply cap reached".into(),
                },
            ]
            .into(),
        ),
        inner: Scripted::new(vec![says("Done.")]),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let conversation = pwr_domain::new_id();
    let mut messages = vec![ChatMessage::text("user", "do the thing")];
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &provider,
            pwr_compat::adapter_for(None, "fake").as_ref(),
            &deployment(),
            &store,
            conversation,
            &policy_for(dir.path()),
            &mut messages,
            8192,
            &[],
            Default::default(),
            pwr_compat::render_tools(&converse::chat_tool_catalog()),
            &std::sync::atomic::AtomicBool::new(false),
            &converse::Continuity::default(),
            &pwr_orchestrator::DenyWithoutAsking,
            |_| {},
        ))
        .unwrap();

    let events = store.events_for_run(conversation).unwrap();
    let failed: Vec<_> = events
        .iter()
        .filter(|event| event.event_type == "turn.failed")
        .map(|event| event.payload.clone())
        .collect();
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert_eq!(failed[0]["outcome"], "unparsed_output");
    assert_eq!(failed[1]["outcome"], "runaway_reply");
    assert_eq!(failed[0]["turn"], 1);
    assert_eq!(failed[1]["turn"], 2);
    // The usable reply after them is the third generation, not the first.
    let generated = events
        .iter()
        .find(|event| event.event_type == "turn.generated")
        .expect("the reply was recorded");
    assert_eq!(generated.payload["turn"], 3);
}

/// Each request of a turn extends the one before it and rewrites nothing: the
/// engine reuses its prompt cache only when the previous prompt is an exact
/// prefix of the next, so any rewrite of earlier history costs a full prefill
/// of the conversation (D.E2E-21).
#[test]
fn each_request_of_a_turn_extends_the_one_before() {
    let dir = workspace();
    let outcome = drive_chat(
        dir.path(),
        vec![
            calls("read_file", serde_json::json!({"path": "code.rs"})),
            calls(
                "replace_text",
                serde_json::json!({
                    "path": "code.rs",
                    "expected_hash": pwr_domain::hash_bytes("one\n"),
                    "find": "one",
                    "replace": "uno",
                }),
            ),
            calls("read_file", serde_json::json!({"path": "code.rs"})),
            calls("no_such_tool", serde_json::json!({})),
            calls("read_file", serde_json::json!({"path": "missing.rs"})),
            calls("read_file", serde_json::json!({"path": "parser.rs"})),
            says("Done."),
        ],
    );
    assert!(outcome.requests.len() >= 6, "{}", outcome.requests.len());
    for (index, pair) in outcome.requests.windows(2).enumerate() {
        let (before, after) = (&pair[0].messages, &pair[1].messages);
        assert!(
            after.len() > before.len() && after[..before.len()] == before[..],
            "request {} rewrote the history of request {index}",
            index + 1
        );
    }
}

/// Automatic compaction, through the real turn: a conversation that reads
/// more than its window holds compacts itself between actions, keeps its
/// instructions and its request, and records `context.compacted` with the
/// trigger -- the same record "Compact now" writes.
#[test]
fn a_turn_that_outgrows_its_window_compacts_itself_and_audits_it() {
    let dir = workspace();
    let names: Vec<String> = (0..6).map(|n| format!("part{n}.rs")).collect();
    for name in &names {
        std::fs::write(dir.path().join(name), "fn one() {}\n".repeat(300)).unwrap();
    }
    let mut script: Vec<ModelChunk> = names
        .iter()
        .map(|name| calls("read_file", serde_json::json!({"path": name})))
        .collect();
    script.push(says("Read them all."));
    let provider = Scripted::new(script);
    let adapter = pwr_compat::adapter_for(None, "fake");
    let store = pwr_store::Store::open(":memory:").unwrap();
    let conversation = pwr_domain::new_id();
    let tools = pwr_compat::render_tools(&converse::chat_tool_catalog());
    let mut messages = vec![
        ChatMessage::text("system", "You are PWR."),
        ChatMessage::text("user", "read every part file"),
    ];
    let mut compacted = Vec::new();
    let report = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &provider,
            adapter.as_ref(),
            &deployment(),
            &store,
            conversation,
            &policy_for(dir.path()),
            &mut messages,
            4096,
            &[],
            Default::default(),
            tools,
            &std::sync::atomic::AtomicBool::new(false),
            &converse::Continuity::default(),
            &pwr_orchestrator::DenyWithoutAsking,
            |step| {
                if let converse::TurnStep::Compacted(note) = step {
                    compacted.push(note);
                }
            },
        ))
        .expect("the turn returned an error");
    assert_eq!(report.stopped, None, "{:?}", report.stopped);
    assert!(!compacted.is_empty(), "a 4k window held six file reads");
    let events: Vec<_> = store
        .events_for_run(conversation)
        .unwrap()
        .into_iter()
        .filter(|event| event.event_type == "context.compacted")
        .collect();
    assert_eq!(events.len(), compacted.len());
    assert_eq!(events[0].payload["trigger"], "automatic");
    assert_eq!(events[0].payload["model"], "fake");
    assert_eq!(events[0].payload["window"], 4096);
    // What the model was sent after compacting: the instructions first, then
    // the record, which still carries the request.
    let last = provider.requests().pop().unwrap();
    assert_eq!(last.messages[0].content, "You are PWR.");
    let record = last
        .messages
        .iter()
        .find(|m| m.purpose == Some(pwr_domain::MessagePurpose::CompactedMemory))
        .expect("no compaction record in the prompt");
    assert!(
        record.content.contains("read every part file"),
        "{}",
        record.content
    );
    assert!(record.content.contains("part0.rs"), "{}", record.content);
}

/// Replies from a script of stream outcomes, keeping every request: a reply
/// or a provider error, the way the engine ends one.
struct Reasoned {
    script: Mutex<VecDeque<Result<ModelChunk, ProviderError>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Reasoned {
    fn new(script: Vec<Result<ModelChunk, ProviderError>>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl ModelProvider for Reasoned {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("a fixture does not inspect")
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("a fixture has no backend to describe")
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let next = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .expect("the script ran out");
        Ok(Box::pin(futures_util::stream::iter([next])))
    }
}

fn explicit_reasoning() -> pwr_domain::ReasoningProfile {
    pwr_domain::ReasoningProfile {
        capability: pwr_domain::ReasoningCapability::ExplicitThinkingStream,
        switchable: true,
        ..Default::default()
    }
}

fn reasoned_turn(
    provider: &Reasoned,
    effort: pwr_domain::ReasoningEffort,
    reasoning: pwr_domain::ReasoningProfile,
    window: u32,
) -> (converse::TurnReport, pwr_store::Store, pwr_domain::Id) {
    let dir = workspace();
    let store = pwr_store::Store::open(":memory:").unwrap();
    let conversation = pwr_domain::new_id();
    let continuity = converse::Continuity {
        reasoning_effort: effort,
        reasoning,
        profile_status: Some("provisional".into()),
        ..Default::default()
    };
    let mut messages = vec![ChatMessage::text("user", "what is in a.txt?")];
    let report = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            provider,
            pwr_compat::adapter_for(None, "fake").as_ref(),
            &deployment(),
            &store,
            conversation,
            &policy_for(dir.path()),
            &mut messages,
            window,
            &[],
            Default::default(),
            pwr_compat::render_tools(&converse::chat_tool_catalog()),
            &std::sync::atomic::AtomicBool::new(false),
            &continuity,
            &pwr_orchestrator::DenyWithoutAsking,
            |_| {},
        ))
        .expect("the turn returned an error");
    (report, store, conversation)
}

fn sampled(request: &ModelRequest, key: &str) -> Option<u64> {
    request
        .sampling
        .get(key)
        .and_then(serde_json::Value::as_u64)
}

#[test]
fn each_effort_sends_its_own_thinking_budget_and_keeps_the_answers_room() {
    let mut budgets = Vec::new();
    for effort in pwr_domain::ReasoningEffort::ALL {
        let provider = Reasoned::new(vec![Ok(says("It says hello."))]);
        let (report, _, _) = reasoned_turn(&provider, effort, explicit_reasoning(), 131_072);
        assert_eq!(report.stopped, None);
        let request = provider.requests.lock().unwrap()[0].clone();
        let budget = sampled(&request, "reasoning_budget").expect("a budget was sent");
        let max_tokens = sampled(&request, "max_tokens").unwrap();
        // Reasoning and the answer are separate allowances.
        assert!(max_tokens - budget >= u64::from(pwr_domain::ANSWER_RESERVE_MIN));
        budgets.push(budget);
    }
    assert!(
        budgets[0] < budgets[1] && budgets[1] < budgets[2],
        "{budgets:?}"
    );
}

#[test]
fn high_effort_in_a_small_window_is_clamped_to_fit() {
    let provider = Reasoned::new(vec![Ok(says("It says hello."))]);
    reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::High,
        explicit_reasoning(),
        12_288,
    );
    let request = provider.requests.lock().unwrap()[0].clone();
    let budget = sampled(&request, "reasoning_budget").unwrap();
    let max_tokens = sampled(&request, "max_tokens").unwrap();
    assert!(
        budget < u64::from(pwr_domain::ReasoningBudgets::CONSERVATIVE.high),
        "{budget}"
    );
    // Never more than the window can hold beside the prompt.
    assert!(max_tokens < 12_288);
    assert!(max_tokens - budget >= u64::from(pwr_domain::ANSWER_RESERVE_MIN));
}

#[test]
fn a_model_without_a_controllable_phase_gets_no_budget() {
    let provider = Reasoned::new(vec![Ok(says("It says hello."))]);
    reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::High,
        pwr_domain::ReasoningProfile::default(),
        131_072,
    );
    let request = provider.requests.lock().unwrap()[0].clone();
    assert_eq!(sampled(&request, "reasoning_budget"), None);
    assert!(!request.sampling.contains_key("think"));
}

#[test]
fn reasoning_that_ends_without_an_answer_is_retried_once_with_thinking_off() {
    let unfinished = || {
        Err(ProviderError::ReasoningUnfinished {
            safe_context: "no answer followed".into(),
        })
    };
    let provider = Reasoned::new(vec![unfinished(), Ok(says("It says hello."))]);
    let (report, store, conversation) = reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::Medium,
        explicit_reasoning(),
        131_072,
    );
    assert_eq!(report.stopped, None);
    assert_eq!(report.answer, "It says hello.");
    let requests = provider.requests.lock().unwrap().clone();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[1].sampling.get("think"),
        Some(&serde_json::json!(false))
    );
    assert_eq!(sampled(&requests[1], "reasoning_budget"), None);
    let events = store.events_for_run(conversation).unwrap();
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "reasoning.finalization_failed")
    );
}

#[test]
fn a_tool_call_inside_reasoning_is_not_run_and_gets_an_answer_phase_retry() {
    let misplaced = || {
        ModelChunk {
        thinking: Some("<tool_call><function=read_file><parameter=path>code.rs</parameter></function></tool_call>".into()),
        done: true,
        ..Default::default()
    }
    };
    let provider = Reasoned::new(vec![Ok(misplaced()), Ok(says("Ready."))]);
    let (report, store, conversation) = reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::Medium,
        explicit_reasoning(),
        131_072,
    );
    assert_eq!(report.actions, 0);
    assert_eq!(report.answer, "Ready.");
    let requests = provider.requests.lock().unwrap().clone();
    assert_eq!(
        requests[1].sampling.get("think"),
        Some(&serde_json::json!(false))
    );
    assert!(requests[1].messages.iter().any(|message| {
        message
            .content
            .contains("tool call was inside the reasoning phase")
    }));
    assert!(
        !store
            .events_for_run(conversation)
            .unwrap()
            .iter()
            .any(|event| { event.event_type == "action.executed" })
    );
}

#[test]
fn repeated_tool_calls_inside_reasoning_stop_with_the_right_cause() {
    let misplaced = || {
        ModelChunk {
        thinking: Some("<tool_call><function=read_file><parameter=path>code.rs</parameter></function></tool_call>".into()),
        done: true,
        ..Default::default()
    }
    };
    let provider = Reasoned::new(vec![Ok(misplaced()), Ok(misplaced()), Ok(misplaced())]);
    let (report, _, _) = reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::Medium,
        explicit_reasoning(),
        131_072,
    );
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::ToolCallInReasoning)
    );
    assert_eq!(report.actions, 0);
}

#[test]
fn reasoning_enforcement_cannot_loop() {
    let unfinished = || {
        Err(ProviderError::ReasoningUnfinished {
            safe_context: "no answer followed".into(),
        })
    };
    let provider = Reasoned::new(vec![unfinished(), unfinished(), unfinished()]);
    let (report, _, _) = reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::High,
        explicit_reasoning(),
        131_072,
    );
    assert_eq!(
        report.stopped,
        Some(converse::StopReason::ReasoningUnfinished)
    );
    assert_eq!(
        provider.requests.lock().unwrap().len(),
        1 + converse::REASONING_FINALIZATION_RETRIES
    );
}

#[test]
fn a_tool_call_after_a_closed_budget_runs_and_the_audit_holds_counts_not_reasoning() {
    const SECRET: &str = "private chain of thought 7f3a";
    let thought = ModelChunk {
        thinking: Some(SECRET.into()),
        tool_calls: vec![ToolCall {
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "a.txt"}),
            id: None,
        }],
        done: true,
        metrics: Some(pwr_domain::GenerationMetrics {
            reasoning_tokens: Some(4_096),
            answer_tokens: Some(40),
            token_accounting: Some(pwr_domain::TokenAccounting::EngineTokenizer),
            reasoning_budget_reached: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };
    let provider = Reasoned::new(vec![Ok(thought), Ok(says("It says hello."))]);
    let (report, store, conversation) = reasoned_turn(
        &provider,
        pwr_domain::ReasoningEffort::Medium,
        explicit_reasoning(),
        131_072,
    );
    assert_eq!(report.stopped, None);
    assert_eq!(report.actions, 1);
    let events = store.events_for_run(conversation).unwrap();
    let kinds: Vec<&str> = events.iter().map(|e| e.event_type.as_str()).collect();
    for expected in [
        "generation.started",
        "reasoning.started",
        "reasoning.budget_reached",
        "generation.finalizing",
        "reasoning.completed",
        "generation.completed",
    ] {
        assert!(
            kinds.contains(&expected),
            "{expected} missing from {kinds:?}"
        );
    }
    let completed = events
        .iter()
        .find(|e| e.event_type == "generation.completed")
        .unwrap();
    assert_eq!(completed.payload["reasoning_tokens"], 4_096);
    assert_eq!(completed.payload["token_accounting"], "engine_tokenizer");
    let started = events
        .iter()
        .find(|e| e.event_type == "generation.started")
        .unwrap();
    assert_eq!(started.payload["reasoning"]["effort"], "medium");
    assert_eq!(started.payload["profile_status"], "provisional");
    for event in &events {
        assert!(
            !event.payload.to_string().contains(SECRET),
            "{} logged the reasoning",
            event.event_type
        );
    }
}

#[test]
fn a_native_level_replaces_a_profiles_off_switch() {
    let provider = Reasoned::new(vec![Ok(says("It says hello."))]);
    let dir = workspace();
    let store = pwr_store::Store::open(":memory:").unwrap();
    let continuity = converse::Continuity {
        reasoning_effort: pwr_domain::ReasoningEffort::High,
        reasoning: pwr_domain::ReasoningProfile {
            capability: pwr_domain::ReasoningCapability::TemplateControlled,
            ..Default::default()
        },
        ..Default::default()
    };
    // What the packaged gpt-oss profile puts in the sampling.
    let sampling =
        std::collections::BTreeMap::from([("think".to_owned(), serde_json::json!(false))]);
    let mut messages = vec![ChatMessage::text("user", "hi")];
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(converse::take_turn(
            &provider,
            pwr_compat::adapter_for(None, "fake").as_ref(),
            &deployment(),
            &store,
            pwr_domain::new_id(),
            &policy_for(dir.path()),
            &mut messages,
            131_072,
            &[],
            sampling,
            pwr_compat::render_tools(&converse::chat_tool_catalog()),
            &std::sync::atomic::AtomicBool::new(false),
            &continuity,
            &pwr_orchestrator::DenyWithoutAsking,
            |_| {},
        ))
        .unwrap();
    let request = provider.requests.lock().unwrap()[0].clone();
    assert_eq!(
        request.sampling.get("reasoning_effort"),
        Some(&serde_json::json!("high"))
    );
    assert!(!request.sampling.contains_key("think"));
}
