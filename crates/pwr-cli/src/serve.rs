//! `pwr serve --stdio`: a conversation over the Agent Client Protocol.
//!
//! Design: `docs/pwr-serve.md`. A front end launches this process and speaks
//! newline-delimited JSON-RPC 2.0 on its stdin and stdout. Each session is a
//! conversation run by the same turn the console runs; what the turn reports as
//! it works becomes `session/update` notifications, and an approval the shared
//! gate needs becomes a `session/request_permission` request to the client.
//!
//! Covered: initialize; new, loaded, resumed, listed and closed sessions; prompt
//! turns with tool calls and diffs; permission requests; cancellation; the
//! console's commands, offered as ACP commands and as `_pwr/*` extension
//! requests; steering a running turn with `_pwr/steer`; and the workspace's
//! models and approval settings with `_pwr/models` and `_pwr/approvals`.

use pwr_domain::ChatMessage;
use pwr_orchestrator::conversation::{Listed, Resumed};
use pwr_orchestrator::converse::{self, ToolPhase, TurnReport, TurnStep};
use pwr_orchestrator::{ApprovalDecision, ApprovalPrompt};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};

/// Where a turn's steps go, shared between the turn and the notifier that
/// forwards them to the client.
type SharedStepSink = Rc<RefCell<Box<dyn FnMut(TurnStep)>>>;

/// The ACP version this server speaks.
pub const PROTOCOL_VERSION: u64 = 1;

/// Sessions per `session/list` page.
const LIST_PAGE: usize = 50;

/// What a turn needs from the server.
pub struct TurnInput {
    pub root: PathBuf,
    pub conversation_id: pwr_domain::Id,
    pub messages: Vec<ChatMessage>,
    pub stop: Arc<AtomicBool>,
    /// Where the turn reports each step, called in order on the turn's own
    /// task: a step queued for another task to forward can reach the client
    /// after a permission request about it.
    pub steps: Box<dyn FnMut(TurnStep)>,
    pub continuity: converse::Continuity,
    pub approvals: Arc<dyn ApprovalPrompt>,
    pub session_grants: Vec<pwr_tools::Approval>,
    /// Goal mode keeps a single task moving across ordinary turn checkpoints.
    /// The runner still owns completion evidence; the server owns the bounded
    /// continuation policy and the operator's stop control.
    pub goal_mode: bool,
}

/// Evidence the core gathered after a model declared a goal complete.
#[derive(Clone, Default)]
pub struct GoalVerification {
    /// True only when both repository checks and an explicit product-level
    /// acceptance check have passed.
    pub passed: bool,
    /// Compilation, unit, integration, and other repository checks passed.
    pub technical_passed: bool,
    /// The repository declared at least one executed acceptance check.
    pub acceptance_available: bool,
    pub summary: String,
    /// The checks that failed, by command.
    pub failing: Vec<String>,
}

/// What a goal is told about checks that failed before it started.
///
/// Seen 2026-09-23 (web_pwr, Qwen3.6, goal mode): asked why a page showed
/// no text, the model fixed it, then found `npm test` failing -- it had been
/// failing since before the request, with no test target configured -- and
/// spent the next fifty actions and three check-ins rebuilding the test setup
/// (karma, then vitest, rewriting package.json), while the engineer asked
/// "why are you still changing things?". A check broken before the goal is
/// not the goal's to repair unless the engineer asks.
fn already_failing_note(failing: &[String]) -> String {
    format!(
        "Before this goal started, the core ran the repository's checks and these were already failing: {}. \
         They are not part of this goal. Do not repair them -- not their configuration, not their dependencies -- \
         unless the engineer asks; mention them in your answer instead. They do not block completion.",
        failing.join(", ")
    )
}

/// Progress emitted while a download is written to its `.part` files.
pub type DownloadProgress = Box<dyn FnMut(&pwr_models::download::Progress)>;

/// Quick Calibration's progress: step, steps, and what it is checking.
pub type CalibrationProgress = Box<dyn FnMut(usize, usize, &str)>;

/// What a client asks to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadRequest {
    /// An artifact declared in the registry (`strategies/artifacts.json`).
    Artifact(String),
    /// A variant found by the Model Manager, pinned to a commit. The core
    /// re-reads its files, sizes and checksums from the Hub at that commit;
    /// the client names what, never what to trust.
    Hub {
        repository: String,
        revision: String,
        variant: String,
        format: pwr_models::catalog::Format,
    },
}

/// A Model Manager search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRequest {
    pub query: String,
    pub format: Option<pwr_models::catalog::Format>,
    pub cursor: Option<String>,
    pub filters: pwr_models::Filters,
}

struct DownloadHandle {
    stop: Arc<AtomicBool>,
    session_id: Option<String>,
    model: Option<(pwr_models::catalog::Format, String)>,
}

/// What the server needs from PWR: the console's own turn and commands in
/// production, scripted ones in tests.
#[async_trait::async_trait(?Send)]
pub trait TurnRunner {
    /// The opening messages of a new session in `root`, or why the workspace
    /// cannot hold one -- no model chosen, or the chosen one not prepared.
    async fn open(&self, root: &Path) -> Result<Vec<ChatMessage>, String>;
    async fn run(&self, turn: TurnInput) -> Result<(TurnReport, Vec<ChatMessage>), String>;
    /// The conversations saved in `root`, most recently active first.
    async fn list(&self, root: &Path) -> Result<Vec<Listed>, String>;
    /// A saved conversation restored and reconciled, `None` if `root` holds no
    /// such conversation; refused like `open` when the workspace cannot hold it.
    async fn resume(&self, root: &Path, id: pwr_domain::Id) -> Result<Option<Resumed>, String>;
    /// Hide a saved conversation and refuse future resumes of it.
    async fn delete(&self, _root: &Path, _id: pwr_domain::Id) -> Result<bool, String> {
        Err("deleting conversations is not available for this runner".into())
    }
    /// One of the console's commands, answered in prose.
    async fn command(&self, command: Command, context: CommandContext) -> Result<String, String>;
    /// One short generation with no tools and no reasoning, for the
    /// workspace's wiki: (the text, the model that wrote it).
    async fn summarise(&self, _root: &Path, _prompt: String) -> Result<(String, String), String> {
        Err("summaries are not available for this runner".into())
    }
    /// Full repository verification for a goal completion. Kept separate from
    /// the user-facing `verify` command so this is structured evidence rather
    /// than prose the server would need to parse.
    async fn verify_goal(&self, _context: CommandContext) -> Result<GoalVerification, String> {
        Err("goal verification is not available for this runner".into())
    }
    /// The workspace's settings a client reads or changes, as JSON.
    async fn settings(&self, root: &Path, request: SettingsRequest) -> Result<Value, String>;
    /// Quick Calibration of the workspace's model. `stop` cancels it,
    /// including the generation in progress; nothing is recorded then.
    async fn quick_calibrate(
        &self,
        _root: &Path,
        _progress: CalibrationProgress,
        _stop: Arc<AtomicBool>,
    ) -> Result<Value, String> {
        Err("quick calibration is not available for this runner".into())
    }
    /// Download and verify an artifact for a workspace. The server owns
    /// cancellation (`stop`); a runner leaves an interrupted `.part` for
    /// resume.
    async fn download(
        &self,
        _root: &Path,
        _request: DownloadRequest,
        _progress: DownloadProgress,
        _stop: Arc<AtomicBool>,
    ) -> Result<Value, pwr_models::download::DownloadError> {
        Err(pwr_models::download::DownloadError {
            kind: pwr_models::download::FailureKind::Io,
            message: "artifact downloads are not available for this runner".into(),
        })
    }
    /// This machine, normalized, for the app and the Model Manager.
    async fn hardware(&self) -> Result<Value, String> {
        Err("hardware detection is not available for this runner".into())
    }
    /// Models this machine could download and run.
    async fn catalog(&self, _root: &Path, _request: CatalogRequest) -> Result<Value, String> {
        Err("the model catalog is not available for this runner".into())
    }
    /// The models on this machine, for every engine, with the one in use.
    async fn local_models(&self, _root: &Path) -> Result<Value, String> {
        Err("listing local models is not available for this runner".into())
    }
    /// Effective backend sampler, plus a replaceable user profile for one
    /// installed model. The core validates values before saving them.
    async fn model_sampling(
        &self,
        _root: &Path,
        _model_ref: &str,
        _values: Option<BTreeMap<String, Value>>,
    ) -> Result<Value, String> {
        Err("model sampling profiles are not available for this runner".into())
    }
    /// Deletes one model from its engine's folder, refusing the one in use.
    async fn delete_model(
        &self,
        _root: &Path,
        _format: pwr_models::catalog::Format,
        _model_ref: &str,
    ) -> Result<Value, String> {
        Err("deleting models is not available for this runner".into())
    }
    /// Records a compaction the person asked for -- the audit event and the
    /// compacted conversation, so a reload resumes from it.
    fn record_compaction(
        &self,
        _root: &Path,
        _conversation_id: pwr_domain::Id,
        _compaction: &pwr_orchestrator::compaction::Compaction,
        _messages: &[ChatMessage],
    ) -> Result<(), String> {
        Ok(())
    }
    /// Records a rewind in the audit and the conversation it left, so a
    /// reload resumes from it.
    fn record_rewind(
        &self,
        _root: &Path,
        _conversation_id: pwr_domain::Id,
        _messages: &[ChatMessage],
        _detail: &Value,
    ) -> Result<(), String> {
        Ok(())
    }
    /// The last compaction the conversation recorded, if any.
    fn last_compaction(&self, _root: &Path, _conversation_id: pwr_domain::Id) -> Option<Value> {
        None
    }
    /// The conversation's last generation: tokens by phase, never content.
    fn last_generation(&self, _root: &Path, _conversation_id: pwr_domain::Id) -> Option<Value> {
        None
    }
    /// An attachment as the deployment reads it -- extracted, snapshotted and
    /// labelled as the console's `/attach` does -- or why it is refused.
    fn attach(&self, root: &Path, attachment: Attachment) -> Result<String, String>;
    /// Chat mode's folder, when this runner offers chat mode: a client opens
    /// sessions there to talk without a workspace.
    fn chat_home(&self) -> Option<PathBuf> {
        None
    }
    /// An image the person attached, stored where the deployment reads it,
    /// or why it is refused -- above all, a model that cannot see (C.25).
    fn store_image(&self, _root: &Path, _image: &PromptImage) -> Result<PathBuf, String> {
        Err("the chosen model cannot read images".into())
    }
}

/// An image a prompt carries: ACP's `image` block, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptImage {
    pub mime: String,
    pub bytes: Vec<u8>,
}

impl PromptImage {
    /// An attached file that is an image, read as one: the app attaches
    /// images by path, like any file. `None` for any other file.
    pub fn from_file(path: &Path) -> Option<Result<Self, String>> {
        let extension = path.extension()?.to_str()?.to_ascii_lowercase();
        let mime = match extension.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            "gif" => "image/gif",
            _ => return None,
        };
        Some(
            std::fs::read(path)
                .map(|bytes| Self {
                    mime: mime.to_owned(),
                    bytes,
                })
                .map_err(|error| format!("{}: {error}", path.display())),
        )
    }

    /// The file extension for the image's type, or `None` for a type the
    /// engine does not read.
    pub fn extension(&self) -> Option<&'static str> {
        match self.mime.as_str() {
            "image/png" => Some("png"),
            "image/jpeg" | "image/jpg" => Some("jpg"),
            "image/webp" => Some("webp"),
            "image/gif" => Some("gif"),
            _ => None,
        }
    }
}

/// A file a prompt carries beside its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attachment {
    /// A `resource_link` to a file on this machine, read where it is.
    File(PathBuf),
    /// An embedded `resource`: the client read it -- an unsaved buffer, say.
    Embedded { uri: String, bytes: Vec<u8> },
}

/// The prompt's text, what the deployment is sent: the request, then the
/// attachments for this turn only, in the console's form.
pub fn with_attachments(task: String, attachments: &[String]) -> String {
    if attachments.is_empty() {
        task
    } else {
        format!(
            "{task}\n\n--- Attachments for this task only ---\n{}",
            attachments.join("\n\n")
        )
    }
}

/// What a client asks of a workspace's settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsRequest {
    /// The backend, the models it has, and the selected model. Passing a model
    /// selects it when it is one of the backend's discovered artifacts.
    Models {
        selected: Option<String>,
        context_tokens: Option<u32>,
        reasoning_effort: Option<pwr_domain::ReasoningEffort>,
        /// The person chose "Use Conservative Defaults" for the selected,
        /// untested model: remembered so the app stops offering the choice.
        acknowledge_provisional: bool,
    },
    /// The window in force, the model, and the auto-compaction threshold
    /// (replaced first when given, in percent of the window).
    Context { compact_at_percent: Option<u8> },
    /// The kinds of action the conversation asks about and the permission
    /// mode, each replaced first when given -- the choice Settings makes, with
    /// the same authority.
    Approvals {
        ask_before: Option<Vec<pwr_tools::Approval>>,
        mode: Option<crate::PermissionMode>,
    },
}

/// The console's commands a client can run in a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Changes,
    Verify,
    Report,
    Diagnose,
    Doctor,
}

impl Command {
    pub const ALL: [Command; 5] = [
        Command::Changes,
        Command::Verify,
        Command::Report,
        Command::Diagnose,
        Command::Doctor,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Command::Changes => "changes",
            Command::Verify => "verify",
            Command::Report => "report",
            Command::Diagnose => "diagnose",
            Command::Doctor => "doctor",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Command::Changes => "Files this session changed, and their diff",
            Command::Verify => "Run the repository's own checks",
            Command::Report => "What this session recorded, reconstructed from its log",
            Command::Diagnose => "Loops and stalls the session's events show",
            Command::Doctor => "Whether the backend is serving the model",
        }
    }

    fn named(name: &str) -> Option<Command> {
        Command::ALL
            .into_iter()
            .find(|command| command.name() == name)
    }
}

/// What a command knows about the session it runs in.
pub struct CommandContext {
    pub root: PathBuf,
    pub conversation_id: pwr_domain::Id,
    /// Hash of the acceptance contract present before this conversation began.
    /// A model must not be able to create or rewrite its own completion proof.
    pub acceptance_contract_hash: Option<String>,
    /// Files the session changed, by the content it left them with.
    pub changed_files: BTreeMap<String, String>,
}

struct Session {
    root: PathBuf,
    conversation_id: pwr_domain::Id,
    acceptance_contract_hash: Option<String>,
    messages: Vec<ChatMessage>,
    continuity: converse::Continuity,
    stop: Arc<AtomicBool>,
    busy: bool,
    turns: u32,
    grants: Arc<Mutex<Vec<pwr_tools::Approval>>>,
    /// The person's last request, in their words, for the workspace's wiki.
    last_request: String,
    /// Where each of the person's messages in this session began, for
    /// `_pwr/rewind`.
    rewind_points: Vec<RewindPoint>,
}

/// One of the person's messages, as a place the conversation can go back to.
#[derive(Debug, Clone)]
struct RewindPoint {
    turn: u32,
    /// The length of the conversation before the message.
    at: usize,
    /// The message as sent, to recognise it is still where it was: a
    /// compaction since has folded it away.
    text: String,
}

const GOAL_MAX_ACTIONS: usize = 208;

impl Session {
    fn new(
        root: PathBuf,
        conversation_id: pwr_domain::Id,
        messages: Vec<ChatMessage>,
        continuity: converse::Continuity,
    ) -> Session {
        let acceptance_contract_hash = acceptance_contract_hash(&root);
        Session {
            root,
            conversation_id,
            acceptance_contract_hash,
            messages,
            continuity,
            stop: Arc::new(AtomicBool::new(false)),
            busy: false,
            turns: 0,
            grants: Arc::default(),
            last_request: String::new(),
            rewind_points: Vec::new(),
        }
    }

    fn command_context(&self) -> CommandContext {
        CommandContext {
            root: self.root.clone(),
            conversation_id: self.conversation_id,
            acceptance_contract_hash: self.acceptance_contract_hash.clone(),
            changed_files: self
                .continuity
                .checkpoint
                .lock()
                .map(|checkpoint| checkpoint.changed_files.clone())
                .unwrap_or_default(),
        }
    }
}

/// The audit event a revert from the app records.
pub const REVERTED_EVENT: &str = "conversation.reverted";

/// Puts one file back as it was before a turn changed it, if it still holds
/// what the turn wrote. See `_pwr/revert`.
fn revert_file(
    root: &Path,
    conversation_id: pwr_domain::Id,
    path: &str,
    expected: &str,
    restore: Option<&str>,
) -> Result<(), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", root.display()))?;
    let policy = pwr_tools::PolicyProfile::Safe.build(root.clone());
    let target = policy
        .resolve(Path::new(path))
        .map_err(|error| error.to_string())?;
    if target.is_symlink() {
        return Err(format!("{path} is a symbolic link; not reverted"));
    }
    let current = match std::fs::read(&target) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("{path}: {error}")),
    };
    if current.as_deref() != Some(expected.as_bytes()) {
        return Err(format!(
            "{path} has changed since PWR edited it, so it was not reverted: reverting would \
             discard those changes"
        ));
    }
    match restore {
        Some(text) => std::fs::write(&target, text),
        None => std::fs::remove_file(&target),
    }
    .map_err(|error| format!("{path}: {error}"))?;
    std::fs::create_dir_all(root.join(".pwr")).map_err(|error| error.to_string())?;
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
        .map_err(|error| error.to_string())?;
    store
        .append(
            Some(conversation_id),
            REVERTED_EVENT,
            json!({
                "path": path,
                "removed": restore.is_none(),
                "expected_hash": pwr_domain::hash_bytes(expected.as_bytes()),
                "restored_hash": restore.map(|text| pwr_domain::hash_bytes(text.as_bytes())),
            }),
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Only a pre-existing explicit acceptance contract can certify a goal. The
/// full verifier compares this hash again at completion, so a task cannot
/// promote a newly-created or self-relaxed check into evidence for itself.
pub fn acceptance_contract_hash(root: &Path) -> Option<String> {
    let checks = pwr_verify::declared_acceptance_checks(root).ok()?;
    if checks.is_empty() {
        return None;
    }
    std::fs::read(root.join(".pwr/checks.json"))
        .ok()
        .map(pwr_domain::hash_bytes)
}

type Sessions = Rc<RefCell<HashMap<String, Session>>>;

/// Requests this server sent and is waiting on, with the session that sent them.
type Pending = Arc<Mutex<HashMap<i64, (String, oneshot::Sender<Value>)>>>;

/// What every request handler shares.
struct Server<R> {
    runner: Rc<R>,
    out: mpsc::UnboundedSender<Value>,
    sessions: Sessions,
    pending: Pending,
    next_id: Arc<AtomicI64>,
    downloads: Rc<RefCell<HashMap<String, DownloadHandle>>>,
    calibrations: Rc<RefCell<HashMap<String, Arc<AtomicBool>>>>,
    /// The engine's own count after each session's last reply: (used, window).
    usage: Rc<RefCell<HashMap<String, (u64, u64)>>>,
    /// Whether wiki summaries are being written in the background.
    summarising: Rc<std::cell::Cell<bool>>,
}

/// Serves one client until its input closes.
pub async fn serve<R, In, Out>(runner: Rc<R>, input: In, output: Out) -> Result<(), String>
where
    R: TurnRunner + 'static,
    In: AsyncBufRead + Unpin,
    Out: AsyncWrite + Unpin + 'static,
{
    let (out, mut outgoing) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::task::spawn_local(async move {
        let mut output = output;
        while let Some(message) = outgoing.recv().await {
            let mut line = message.to_string();
            line.push('\n');
            if output.write_all(line.as_bytes()).await.is_err() || output.flush().await.is_err() {
                break;
            }
        }
    });
    let server = Rc::new(Server {
        runner,
        out,
        sessions: Rc::default(),
        pending: Arc::default(),
        next_id: Arc::new(AtomicI64::new(1)),
        downloads: Rc::default(),
        calibrations: Rc::default(),
        usage: Rc::default(),
        summarising: Rc::default(),
    });
    let mut lines = input.lines();
    while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                server.send(error_response(
                    Value::Null,
                    -32700,
                    &format!("parse error: {error}"),
                ));
                continue;
            }
        };
        let id = message.get("id").cloned();
        let Some(method) = message.get("method").and_then(Value::as_str) else {
            // A response to a request this server sent.
            if let Some(id) = id.as_ref().and_then(Value::as_i64)
                && let Some((_, reply)) = server
                    .pending
                    .lock()
                    .ok()
                    .and_then(|mut waiting| waiting.remove(&id))
            {
                let _ = reply.send(message);
            }
            continue;
        };
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        match id {
            Some(id) => server.request(id, method, params).await,
            None => server.notified(method, &params),
        }
    }
    // Input closed: turns still running are asked to stop, and the writer
    // finishes what is queued once every sender is gone.
    for session in server.sessions.borrow().values() {
        session.stop.store(true, Ordering::Relaxed);
    }
    for download in server.downloads.borrow().values() {
        download.stop.store(true, Ordering::Relaxed);
    }
    drop(server);
    let _ = writer.await;
    Ok(())
}

impl<R: TurnRunner + 'static> Server<R> {
    fn send(&self, message: Value) {
        let _ = self.out.send(message);
    }

    fn update(&self, session_id: &str, update: Value) {
        self.send(notification(
            "session/update",
            json!({"sessionId": session_id, "update": update}),
        ));
    }

    fn notified(&self, method: &str, params: &Value) {
        // Other notifications, `_`-prefixed extensions included, are ignored
        // as JSON-RPC requires.
        if method == "session/cancel" {
            let session_id = session_param(params);
            if let Some(session) = self.sessions.borrow().get(session_id) {
                session.stop.store(true, Ordering::Relaxed);
            }
            for download in self.downloads.borrow().values() {
                if download.session_id.as_deref() == Some(session_id) {
                    download.stop.store(true, Ordering::Relaxed);
                }
            }
            cancel_questions(&self.pending, session_id);
        } else if method == "_pwr/download_cancel"
            && let Some(download) = params.get("downloadId").and_then(Value::as_str)
            && let Some(handle) = self.downloads.borrow().get(download)
        {
            handle.stop.store(true, Ordering::Relaxed);
        } else if method == "_pwr/quick_calibration_cancel"
            && let Some(handle) = params.get("calibrationId").and_then(Value::as_str)
            && let Some(stop) = self.calibrations.borrow().get(handle)
        {
            stop.store(true, Ordering::Relaxed);
        }
    }

    async fn request(self: &Rc<Self>, id: Value, method: &str, params: Value) {
        match method {
            "initialize" => self.send(result(
                id,
                json!({
                    "protocolVersion": PROTOCOL_VERSION,
                    "agentCapabilities": {
                        "loadSession": true,
                        "promptCapabilities": {"image": true, "audio": false, "embeddedContext": true},
                        "sessionCapabilities": {"list": {}, "resume": {}, "close": {}},
                    },
                    "authMethods": [],
                    "agentInfo": {"name": "pwr", "version": env!("CARGO_PKG_VERSION")},
                    // Chat mode: a session opened with this as its `cwd` has
                    // no workspace -- it reads only what is attached to it.
                    "_meta": {"pwr": {"chatHome": self.runner.chat_home()}},
                }),
            )),
            "session/new" => self.new_session(id, &params).await,
            "session/load" => self.load_session(id, &params, true).await,
            "session/resume" => self.load_session(id, &params, false).await,
            "session/list" => self.list_sessions(id, &params).await,
            "session/close" => self.close_session(id, &params),
            "_pwr/session_delete" => self.delete_session(id, &params).await,
            "session/prompt" => self.prompt(id, &params),
            "_pwr/steer" => self.steer(id, &params),
            "_pwr/models" => {
                let selected = match params.get("model") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(model)) if !model.trim().is_empty() => Some(model.clone()),
                    Some(_) => {
                        return self.send(error_response(
                            id,
                            -32602,
                            "model must be a non-empty installed model reference",
                        ));
                    }
                };
                let context_tokens = match params.get("contextTokens") {
                    None | Some(Value::Null) => None,
                    Some(Value::Number(tokens)) => tokens.as_u64().and_then(|tokens| u32::try_from(tokens).ok()),
                    Some(_) => {
                        return self.send(error_response(
                            id,
                            -32602,
                            "contextTokens must be a positive integer",
                        ));
                    }
                };
                if params.get("contextTokens").is_some()
                    && context_tokens.is_none()
                {
                    return self.send(error_response(
                        id,
                        -32602,
                        "contextTokens must be a positive integer",
                    ));
                }
                self.settings(
                    id.clone(),
                    &params,
                    SettingsRequest::Models {
                        selected,
                        context_tokens,
                        reasoning_effort: match params.get("reasoningEffort") {
                            None | Some(Value::Null) => None,
                            Some(value) => {
                                match value.as_str().and_then(pwr_domain::ReasoningEffort::parse)
                                {
                                    Some(effort) => Some(effort),
                                    None => {
                                        return self.send(error_response(
                                            id.clone(),
                                            -32602,
                                            "reasoningEffort must be low, medium or high",
                                        ));
                                    }
                                }
                            }
                        },
                        acknowledge_provisional: params.get("acknowledgeProvisional")
                            == Some(&Value::Bool(true)),
                    },
                );
            }
            "_pwr/download" => self.download(id, &params),
            "_pwr/hardware" => {
                let server = Rc::clone(self);
                tokio::task::spawn_local(async move {
                    server.send(match server.runner.hardware().await {
                        Ok(value) => result(id, value),
                        Err(why) => error_response(id, -32000, &why),
                    });
                });
            }
            "_pwr/catalog" => self.catalog(id, &params),
            "_pwr/local_models" | "_pwr/model_delete" => self.local(id, method, &params),
            "_pwr/model_sampling" => self.model_sampling(id, &params),
            "_pwr/profile" => self.send(profile_request(id, &params)),
            "_pwr/memory" => self.send(memory_request(id, &params)),
            "_pwr/projects" => self.send(projects_request(id, &params)),
            "_pwr/wiki" => self.send(wiki_request(id, &params)),
            "_pwr/rewind" => self.rewind(id, &params),
            "_pwr/files" => self.send(files_request(id, &params)),
            "_pwr/file" => self.send(file_request(id, &params)),
            "_pwr/wiki_summarise" => {
                let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from)
                else {
                    return self.send(error_response(id, -32602, "name the workspace with cwd"));
                };
                if self.is_chat_home(&root) {
                    return self.send(error_response(id, -32000, "chat mode has no wiki"));
                }
                let started = !self.summarising.get();
                self.summarise_while_idle(root);
                self.send(result(id, json!({"started": started})));
            }
            "_pwr/quick_calibration" => self.quick_calibration(id, &params),
            "_pwr/context" => self.context(id, &params).await,
            "_pwr/compact" => self.compact(id, &params).await,
            "_pwr/revert" => self.revert(id, &params),
            "_pwr/approvals" => {
                let change = match params.get("askBefore") {
                    None | Some(Value::Null) => None,
                    Some(kinds) => match serde_json::from_value(kinds.clone()) {
                        Ok(kinds) => Some(kinds),
                        Err(error) => {
                            return self.send(error_response(
                                id,
                                -32602,
                                &format!("askBefore is a list of approval kinds: {error}"),
                            ));
                        }
                    },
                };
                let mode = match params.get("mode") {
                    None | Some(Value::Null) => None,
                    Some(mode) => match serde_json::from_value(mode.clone()) {
                        Ok(mode) => Some(mode),
                        Err(error) => {
                            return self.send(error_response(
                                id,
                                -32602,
                                &format!("mode is \"ask\" or \"auto\": {error}"),
                            ));
                        }
                    },
                };
                self.settings(
                    id,
                    &params,
                    SettingsRequest::Approvals {
                        ask_before: change,
                        mode,
                    },
                );
            }
            _ => match method.strip_prefix("_pwr/").and_then(Command::named) {
                Some(command) => self.extension_command(id, command, &params),
                None => self.send(error_response(
                    id,
                    -32601,
                    &format!("method not found: {method}"),
                )),
            },
        }
    }

    async fn new_session(&self, id: Value, params: &Value) {
        let Some(cwd) = params.get("cwd").and_then(Value::as_str) else {
            return self.send(error_response(id, -32602, "session/new needs a cwd"));
        };
        let root = PathBuf::from(cwd);
        match self.runner.open(&root).await {
            Ok(messages) => {
                let conversation_id = pwr_domain::new_id();
                let session_id = conversation_id.to_string();
                let continuity = converse::Continuity {
                    chat_only: self.is_chat_home(&root),
                    ..Default::default()
                };
                self.sessions.borrow_mut().insert(
                    session_id.clone(),
                    Session::new(root, conversation_id, messages, continuity),
                );
                self.send(result(id, json!({"sessionId": session_id})));
                self.send(available_commands(&session_id));
            }
            Err(why) => self.send(error_response(id, -32000, &why)),
        }
    }

    /// `session/load` when `replaying`, `session/resume` otherwise.
    async fn load_session(&self, id: Value, params: &Value, replaying: bool) {
        let method = if replaying {
            "session/load"
        } else {
            "session/resume"
        };
        let Some(cwd) = params.get("cwd").and_then(Value::as_str) else {
            return self.send(error_response(id, -32602, &format!("{method} needs a cwd")));
        };
        let session_id = session_param(params).to_owned();
        let Ok(conversation_id) = uuid::Uuid::parse_str(&session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if self
            .sessions
            .borrow()
            .get(&session_id)
            .is_some_and(|session| session.busy)
        {
            return self.send(error_response(
                id,
                -32000,
                "this session has a turn running; cancel it before loading it again",
            ));
        }
        let root = PathBuf::from(cwd);
        let resumed = match self.runner.resume(&root, conversation_id).await {
            Ok(Some(resumed)) => resumed,
            Ok(None) => {
                return self.send(error_response(
                    id,
                    -32002,
                    "no saved session with that id in this workspace",
                ));
            }
            Err(why) => return self.send(error_response(id, -32000, &why)),
        };
        if replaying {
            for update in replay(&resumed.messages) {
                self.update(&session_id, update);
            }
        }
        // Said on resume as well as load: a client that keeps its own history
        // still has to learn what changed while the session was away.
        if let Some(note) = &resumed.note {
            self.update(&session_id, message_chunk("agent_message_chunk", note));
        }
        let continuity = converse::Continuity {
            chat_only: self.is_chat_home(&root),
            ..Default::default()
        };
        if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
            *checkpoint = resumed.checkpoint;
        }
        self.sessions.borrow_mut().insert(
            session_id.clone(),
            Session::new(root, conversation_id, resumed.messages, continuity),
        );
        self.send(result(id, json!({})));
        self.send(available_commands(&session_id));
    }

    async fn list_sessions(&self, id: Value, params: &Value) {
        // Sessions live in each workspace's own log, so there is no list
        // without a workspace to read it from.
        let Some(cwd) = params.get("cwd").and_then(Value::as_str) else {
            return self.send(error_response(
                id,
                -32602,
                "session/list needs a cwd: sessions are kept in each workspace",
            ));
        };
        let offset = match params.get("cursor").and_then(Value::as_str) {
            None => 0,
            Some(cursor) => match cursor.parse::<usize>() {
                Ok(offset) => offset,
                Err(_) => return self.send(error_response(id, -32602, "unrecognised cursor")),
            },
        };
        let listed = match self.runner.list(Path::new(cwd)).await {
            Ok(listed) => listed,
            Err(why) => return self.send(error_response(id, -32000, &why)),
        };
        let page: Vec<Value> = listed
            .iter()
            .skip(offset)
            .take(LIST_PAGE)
            .map(|session| {
                json!({
                    "sessionId": session.id.to_string(),
                    "cwd": cwd,
                    "title": session.title,
                    "updatedAt": session.updated_at,
                    "_meta": {"pwr": {"messages": session.messages}},
                })
            })
            .collect();
        let mut response = json!({ "sessions": page });
        if offset + LIST_PAGE < listed.len() {
            response["nextCursor"] = json!((offset + LIST_PAGE).to_string());
        }
        self.send(result(id, response));
    }

    fn close_session(&self, id: Value, params: &Value) {
        let session_id = session_param(params);
        let Some(session) = self.sessions.borrow_mut().remove(session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        // A turn still running is stopped; its reply is still sent, and nothing
        // is kept for a session that is gone.
        session.stop.store(true, Ordering::Relaxed);
        cancel_questions(&self.pending, session_id);
        self.send(result(id, json!({})));
    }

    async fn delete_session(&self, id: Value, params: &Value) {
        let Some(cwd) = params.get("cwd").and_then(Value::as_str) else {
            return self.send(error_response(id, -32602, "session deletion needs a cwd"));
        };
        let session_id = session_param(params).to_owned();
        let Ok(conversation_id) = uuid::Uuid::parse_str(&session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if self
            .sessions
            .borrow()
            .get(&session_id)
            .is_some_and(|session| session.busy)
        {
            return self.send(error_response(
                id,
                -32000,
                "cannot delete a running session",
            ));
        }
        match self.runner.delete(Path::new(cwd), conversation_id).await {
            Ok(true) => {
                self.sessions.borrow_mut().remove(&session_id);
                self.send(result(id, json!({ "deleted": true })));
            }
            Ok(false) => self.send(error_response(
                id,
                -32002,
                "no saved session with that id in this workspace",
            )),
            Err(why) => self.send(error_response(id, -32000, &why)),
        }
    }

    /// Settings belong to a workspace: named by `cwd`, or by a session in it.
    ///
    /// Answered on a task of its own: choosing a model computes its window
    /// and can take seconds, and the loop that reads the client's messages
    /// must not wait on it -- every click after it would queue behind it.
    fn settings(self: &Rc<Self>, id: Value, params: &Value, request: SettingsRequest) {
        let root = match params.get("cwd").and_then(Value::as_str) {
            Some(cwd) => Some(PathBuf::from(cwd)),
            None => self
                .sessions
                .borrow()
                .get(session_param(params))
                .map(|session| session.root.clone()),
        };
        let Some(root) = root else {
            return self.send(error_response(
                id,
                -32602,
                "name the workspace with cwd or a sessionId",
            ));
        };
        let server = Rc::clone(self);
        tokio::task::spawn_local(async move {
            server.send(match server.runner.settings(&root, request).await {
                Ok(settings) => result(id, settings),
                Err(why) => error_response(id, -32000, &why),
            });
        });
    }

    fn steer(&self, id: Value, params: &Value) {
        let text = params
            .get("text")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let sessions = self.sessions.borrow();
        let reply = match sessions.get(session_param(params)) {
            None => error_response(id, -32602, "no such session"),
            Some(_) if text.trim().is_empty() => {
                error_response(id, -32602, "_pwr/steer needs text")
            }
            // Between turns there is nothing to steer, and the message is a
            // prompt: the console makes the same distinction.
            Some(session) if !session.busy => {
                error_response(id, -32000, "no turn is running; send this as a prompt")
            }
            Some(session) => match session.continuity.steering.lock() {
                Ok(mut queue) => {
                    queue.push(text.to_owned());
                    result(id, json!({}))
                }
                Err(_) => error_response(id, -32603, "the steering queue is unusable"),
            },
        };
        self.send(reply);
    }

    /// Start a workspace artifact download. The response is held until the
    /// operation ends, while progress notifications keep the client informed.
    /// A client-supplied id lets it cancel before a session exists.
    fn download(self: &Rc<Self>, id: Value, params: &Value) {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        let request = match (text("artifact"), text("repository")) {
            (Some(artifact), None) => DownloadRequest::Artifact(artifact),
            (None, Some(repository)) => {
                let format = text("format").and_then(|f| pwr_models::catalog::Format::parse(&f));
                match (text("revision"), text("variant"), format) {
                    (Some(revision), Some(variant), Some(format)) => DownloadRequest::Hub {
                        repository,
                        revision,
                        variant,
                        format,
                    },
                    _ => {
                        return self.send(error_response(
                            id,
                            -32602,
                            "a Hub download needs repository, revision, variant and format",
                        ));
                    }
                }
            }
            _ => {
                return self.send(error_response(
                    id,
                    -32602,
                    "_pwr/download needs artifact, or repository with revision, variant and format",
                ));
            }
        };
        let download_id = match params.get("downloadId").and_then(Value::as_str) {
            Some(download_id) if !download_id.trim().is_empty() => download_id.to_owned(),
            _ => {
                return self.send(error_response(id, -32602, "_pwr/download needs downloadId"));
            }
        };
        if self.downloads.borrow().contains_key(&download_id) {
            return self.send(error_response(
                id,
                -32000,
                "that downloadId is already running",
            ));
        }
        let session_id = match params.get("sessionId").and_then(Value::as_str) {
            Some(session_id) if !session_id.is_empty() => Some(session_id.to_owned()),
            _ => None,
        };
        let root = match params.get("cwd").and_then(Value::as_str) {
            Some(cwd) => PathBuf::from(cwd),
            None => match session_id.as_deref().and_then(|session_id| {
                self.sessions
                    .borrow()
                    .get(session_id)
                    .map(|session| session.root.clone())
            }) {
                Some(root) => root,
                None => {
                    return self.send(error_response(
                        id,
                        -32602,
                        "_pwr/download needs cwd or a sessionId",
                    ));
                }
            },
        };
        let stop = Arc::new(AtomicBool::new(false));
        let model = match &request {
            DownloadRequest::Hub {
                repository,
                variant,
                format,
                ..
            } => Some((
                *format,
                if *format == pwr_models::catalog::Format::Mlx {
                    repository.clone()
                } else {
                    format!("{repository}/{variant}")
                },
            )),
            DownloadRequest::Artifact(_) => None,
        };
        self.downloads.borrow_mut().insert(
            download_id.clone(),
            DownloadHandle {
                stop: Arc::clone(&stop),
                session_id: session_id.clone(),
                model,
            },
        );
        let server = Rc::clone(self);
        let progress_server = Rc::clone(self);
        let progress_id = download_id.clone();
        let operation_id = download_id.clone();
        // The state the app draws, advanced by the same machine the tests pin:
        // a late progress report cannot revive a cancelled download.
        use pwr_models::download::{DownloadEvent, DownloadState};
        let state = Rc::new(RefCell::new(DownloadState::Preparing));
        let progress_state = Rc::clone(&state);
        self.send(download_notification(
            &download_id,
            &DownloadState::Preparing,
            None,
        ));
        tokio::task::spawn_local(async move {
            // The state advances on every chunk; the client hears of it a few
            // times a second. One notification per chunk was thousands a
            // second at full speed, which a window in the background (App
            // Nap) could not drain.
            let mut reported: Option<(std::time::Instant, String, String)> = None;
            let progress: DownloadProgress = Box::new(move |event| {
                let next = progress_state
                    .borrow()
                    .clone()
                    .on(&DownloadEvent::Progress(event.clone()));
                *progress_state.borrow_mut() = next.clone();
                let phase = format!("{:?}", event.phase);
                let due = match &reported {
                    None => true,
                    Some((at, last_phase, last_file)) => {
                        at.elapsed() >= PROGRESS_INTERVAL
                            || *last_phase != phase
                            || *last_file != event.file
                            || event.file_bytes >= event.file_total
                    }
                };
                if due {
                    reported = Some((std::time::Instant::now(), phase, event.file.clone()));
                    progress_server.send(download_notification(&progress_id, &next, Some(event)));
                }
            });
            let cancelled = Arc::clone(&stop);
            let outcome = tokio::select! {
                result = server.runner.download(&root, request, progress, Arc::clone(&stop)) => result,
                _ = wait_for_stop(cancelled) => Err(pwr_models::download::DownloadError {
                    kind: pwr_models::download::FailureKind::Cancelled,
                    message: "download cancelled; partial data was kept for resume".into(),
                }),
            };
            server.downloads.borrow_mut().remove(&operation_id);
            let current = state.borrow().clone();
            let last = match &outcome {
                Ok(_) => current.on(&DownloadEvent::Finished),
                Err(error) => current.on(&DownloadEvent::Failed(error.clone())),
            };
            server.send(download_notification(&operation_id, &last, None));
            let reply = match outcome {
                Ok(value) => result(id, value),
                Err(error) => {
                    let mut reply = error_response(id, -32000, &error.message);
                    reply["error"]["data"] = json!({"kind": error.kind});
                    reply
                }
            };
            server.send(reply);
        });
    }

    /// `_pwr/quick_calibration`: runs Quick Calibration on the workspace's
    /// model, reporting `_pwr/calibration_progress`; cancelled by
    /// `_pwr/quick_calibration_cancel` with the same `calibrationId`.
    fn quick_calibration(self: &Rc<Self>, id: Value, params: &Value) {
        let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
            return self.send(error_response(id, -32602, "quick calibration needs a cwd"));
        };
        let Some(calibration_id) = params
            .get("calibrationId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 80)
            .map(str::to_owned)
        else {
            return self.send(error_response(
                id,
                -32602,
                "quick calibration needs a calibrationId",
            ));
        };
        if !self.calibrations.borrow().is_empty() {
            return self.send(error_response(
                id,
                -32602,
                "a calibration is already running",
            ));
        }
        let stop = Arc::new(AtomicBool::new(false));
        self.calibrations
            .borrow_mut()
            .insert(calibration_id.clone(), stop.clone());
        let server = Rc::clone(self);
        let progress_server = Rc::clone(self);
        let progress_id = calibration_id.clone();
        tokio::task::spawn_local(async move {
            let progress: CalibrationProgress = Box::new(move |step, total, name| {
                progress_server.send(json!({
                    "jsonrpc": "2.0",
                    "method": "_pwr/calibration_progress",
                    "params": {"calibrationId": progress_id, "step": step, "total": total, "name": name},
                }));
            });
            let outcome = server.runner.quick_calibrate(&root, progress, stop).await;
            server.calibrations.borrow_mut().remove(&calibration_id);
            server.send(match outcome {
                Ok(value) => result(id, value),
                Err(why) => error_response(id, -32000, &why),
            });
        });
    }

    /// `_pwr/local_models` and `_pwr/model_delete`: the models on this
    /// machine, and removing one of them.
    fn local(self: &Rc<Self>, id: Value, method: &str, params: &Value) {
        let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
            return self.send(error_response(id, -32602, "name the workspace with cwd"));
        };
        let delete = if method == "_pwr/model_delete" {
            let model_ref = params
                .get("modelRef")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let format = params
                .get("format")
                .and_then(Value::as_str)
                .and_then(pwr_models::catalog::Format::parse);
            match (model_ref.trim(), format) {
                ("", _) | (_, None) => {
                    return self.send(error_response(
                        id,
                        -32602,
                        "_pwr/model_delete needs modelRef and format (mlx or gguf)",
                    ));
                }
                (model_ref, Some(format)) => Some((format, model_ref.to_owned())),
            }
        } else {
            None
        };
        let server = Rc::clone(self);
        if let Some((format, model_ref)) = &delete
            && self.downloads.borrow().values().any(|handle| {
                handle
                    .model
                    .as_ref()
                    .is_some_and(|(running_format, running_ref)| {
                        running_format == format && running_ref == model_ref
                    })
            })
        {
            return self.send(error_response(
                id,
                -32000,
                "Pause this download before discarding its files",
            ));
        }
        tokio::task::spawn_local(async move {
            let outcome = match delete {
                Some((format, model_ref)) => {
                    server.runner.delete_model(&root, format, &model_ref).await
                }
                None => server.runner.local_models(&root).await,
            };
            server.send(match outcome {
                Ok(value) => result(id, value),
                Err(why) => error_response(id, -32000, &why),
            });
        });
    }

    fn model_sampling(self: &Rc<Self>, id: Value, params: &Value) {
        let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
            return self.send(error_response(id, -32602, "name the workspace with cwd"));
        };
        let Some(model_ref) = params
            .get("modelRef")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            return self.send(error_response(
                id,
                -32602,
                "name an installed model with modelRef",
            ));
        };
        let values = match params.get("values") {
            None => None,
            Some(Value::Object(values)) => Some(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
            ),
            Some(_) => {
                return self.send(error_response(
                    id,
                    -32602,
                    "values must be an object of sampling parameters",
                ));
            }
        };
        let model_ref = model_ref.to_owned();
        let server = Rc::clone(self);
        tokio::task::spawn_local(async move {
            let outcome = server
                .runner
                .model_sampling(&root, &model_ref, values)
                .await;
            server.send(match outcome {
                Ok(value) => result(id, value),
                Err(why) => error_response(id, -32000, &why),
            });
        });
    }

    /// `_pwr/catalog`: a Model Manager search, filtered in the core.
    fn catalog(self: &Rc<Self>, id: Value, params: &Value) {
        let root = match params.get("cwd").and_then(Value::as_str) {
            Some(cwd) => PathBuf::from(cwd),
            None => match self
                .sessions
                .borrow()
                .get(session_param(params))
                .map(|session| session.root.clone())
            {
                Some(root) => root,
                None => {
                    return self.send(error_response(
                        id,
                        -32602,
                        "name the workspace with cwd or a sessionId",
                    ));
                }
            },
        };
        let format = match params.get("format").and_then(Value::as_str) {
            None | Some("" | "any") => None,
            Some(raw) => match pwr_models::catalog::Format::parse(raw) {
                Some(format) => Some(format),
                None => {
                    return self.send(error_response(id, -32602, "format is mlx, gguf or any"));
                }
            },
        };
        let filters = match params.get("filters") {
            None | Some(Value::Null) => pwr_models::Filters::default(),
            Some(value) => match serde_json::from_value(value.clone()) {
                Ok(filters) => filters,
                Err(error) => {
                    return self.send(error_response(id, -32602, &format!("filters: {error}")));
                }
            },
        };
        let request = CatalogRequest {
            query: params
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            format,
            cursor: params
                .get("cursor")
                .and_then(Value::as_str)
                .map(str::to_owned),
            filters,
        };
        let server = Rc::clone(self);
        tokio::task::spawn_local(async move {
            server.send(match server.runner.catalog(&root, request).await {
                Ok(value) => result(id, value),
                Err(why) => error_response(id, -32000, &why),
            });
        });
    }

    /// `_pwr/context`: how full the session's window is and of what, the
    /// auto-compaction threshold (settable with `autoCompactPercent`) and the
    /// last compaction. Composition is estimated from the messages; `used` is
    /// the engine's own count after the last reply, when there has been one.
    async fn context(&self, id: Value, params: &Value) {
        let session_id = session_param(params).to_owned();
        let snapshot = self.sessions.borrow().get(&session_id).map(|session| {
            (
                session.root.clone(),
                session.conversation_id,
                pwr_orchestrator::compaction::composition(&session.messages),
                session.busy,
            )
        });
        let Some((root, conversation_id, composition, busy)) = snapshot else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        let compact_at_percent = match params.get("autoCompactPercent") {
            None | Some(Value::Null) => None,
            Some(value) => match value.as_u64().and_then(|p| u8::try_from(p).ok()) {
                Some(percent)
                    if (converse::COMPACT_AT_BOUNDS.0..=converse::COMPACT_AT_BOUNDS.1)
                        .contains(&percent) =>
                {
                    Some(percent)
                }
                _ => {
                    return self.send(error_response(
                        id,
                        -32602,
                        &format!(
                            "autoCompactPercent is a whole percent between {} and {}",
                            converse::COMPACT_AT_BOUNDS.0,
                            converse::COMPACT_AT_BOUNDS.1
                        ),
                    ));
                }
            },
        };
        let settings = match self
            .runner
            .settings(&root, SettingsRequest::Context { compact_at_percent })
            .await
        {
            Ok(settings) => settings,
            Err(why) => return self.send(error_response(id, -32000, &why)),
        };
        let reported = self.usage.borrow().get(&session_id).copied();
        let window = reported
            .map(|(_, window)| window)
            .or_else(|| settings.get("window").and_then(Value::as_u64))
            .unwrap_or(0);
        let percent = settings
            .get("compactAtPercent")
            .and_then(Value::as_u64)
            .unwrap_or(75);
        let estimated = composition.total() as u64;
        let (used, source) = match reported {
            Some((used, _)) => (used, "engine"),
            None => (estimated, "estimate"),
        };
        self.send(result(
            id,
            json!({
                "window": window,
                "used": used,
                "usedSource": source,
                "estimatedTokens": estimated,
                "estimateBasis": "characters divided by 4; not a tokenizer count",
                "composition": composition,
                "model": settings.get("model"),
                "autoCompact": {
                    "enabled": true,
                    "thresholdPercent": percent,
                    "thresholdTokens": window * percent / 100,
                    "bounds": [converse::COMPACT_AT_BOUNDS.0, converse::COMPACT_AT_BOUNDS.1],
                    "custom": settings.get("compactAtCustom"),
                },
                "lastCompaction": self.runner.last_compaction(&root, conversation_id),
                "lastGeneration": self.runner.last_generation(&root, conversation_id),
                "busy": busy,
            }),
        ));
    }

    /// `_pwr/revert`: the person's "Revert" on one changed file.
    ///
    /// The app used to write the old content itself, from the Tauri shell --
    /// outside the core's audit, and without checking that the file still held
    /// what the model wrote, so a manual edit made after the turn was silently
    /// overwritten (the v0.1.0-alpha readiness assessment, blocking item 4).
    /// Now the core does it: the path is resolved by the workspace's policy,
    /// the file must hold exactly `expected` (the model's version) or nothing
    /// is touched, and the revert is recorded in the conversation's log.
    ///
    /// `restore` is the content to put back; `null` removes a file the turn
    /// created.
    fn revert(&self, id: Value, params: &Value) {
        let session_id = session_param(params).to_owned();
        let Some((root, conversation_id, busy)) = self
            .sessions
            .borrow()
            .get(&session_id)
            .map(|session| (session.root.clone(), session.conversation_id, session.busy))
        else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if busy {
            return self.send(error_response(
                id,
                -32000,
                "a turn is running; revert once it ends",
            ));
        }
        let (Some(path), Some(expected)) = (
            params.get("path").and_then(Value::as_str),
            params.get("expected").and_then(Value::as_str),
        ) else {
            return self.send(error_response(
                id,
                -32602,
                "revert needs a workspace-relative path and the expected current content",
            ));
        };
        let restore = match params.get("restore") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(text.as_str()),
            Some(_) => {
                return self.send(error_response(
                    id,
                    -32602,
                    "restore is the previous content, or null to remove a created file",
                ));
            }
        };
        match revert_file(&root, conversation_id, path, expected, restore) {
            Ok(()) => self.send(result(id, json!({"reverted": true, "path": path}))),
            Err(why) => self.send(error_response(id, -32000, &why)),
        }
    }

    /// `_pwr/compact`: the person's "Compact now", through the same
    /// compaction the conversation performs by itself, recorded the same way.
    async fn compact(&self, id: Value, params: &Value) {
        let session_id = session_param(params).to_owned();
        let Some(root) = self
            .sessions
            .borrow()
            .get(&session_id)
            .map(|session| session.root.clone())
        else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if self
            .sessions
            .borrow()
            .get(&session_id)
            .is_some_and(|session| session.busy)
        {
            return self.send(error_response(
                id,
                -32000,
                "a turn is running; the conversation compacts itself between actions when it needs \
                 to, or compact once the turn ends",
            ));
        }
        let settings = self
            .runner
            .settings(
                &root,
                SettingsRequest::Context {
                    compact_at_percent: None,
                },
            )
            .await
            .unwrap_or(Value::Null);
        let window = self
            .usage
            .borrow()
            .get(&session_id)
            .map(|(_, window)| *window)
            .or_else(|| settings.get("window").and_then(Value::as_u64))
            .unwrap_or(32_768);
        let percent = settings
            .get("compactAtPercent")
            .and_then(Value::as_u64)
            .unwrap_or(75);
        let room = usize::try_from(window * percent / 100).unwrap_or(usize::MAX);
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(&session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        let carry = session
            .continuity
            .checkpoint
            .lock()
            .map(|checkpoint| pwr_orchestrator::compaction::Carry::from_checkpoint(&checkpoint))
            .unwrap_or_default();
        let mut messages = session.messages.clone();
        let tail = pwr_orchestrator::compaction::manual_tail_budget(&messages, room);
        let Some(done) = pwr_orchestrator::compaction::compact(
            &mut messages,
            tail,
            &carry,
            pwr_orchestrator::compaction::Trigger::Manual,
        ) else {
            return self.send(result(
                id,
                json!({
                    "compacted": false,
                    "reason": "Nothing older than the most recent exchanges to summarise yet.",
                }),
            ));
        };
        if let Err(why) =
            self.runner
                .record_compaction(&root, session.conversation_id, &done, &messages)
        {
            return self.send(error_response(id, -32000, &why));
        }
        session.messages = messages;
        drop(sessions);
        // The engine's count described the prompt before; until the next
        // reply, the estimate is the better number.
        self.usage.borrow_mut().remove(&session_id);
        self.send(notification(
            "_pwr/compacted",
            json!({"sessionId": session_id, "trigger": "manual", "note": done.note()}),
        ));
        self.send(result(
            id,
            json!({
                "compacted": true,
                "note": done.note(),
                "tokensBefore": done.tokens_before,
                "tokensAfter": done.tokens_after,
                "foldedMessages": done.folded_messages,
                "preserved": {
                    "changedFiles": done.preserved.changed_files.len(),
                    "paths": done.preserved.paths.len(),
                    "unresolved": done.preserved.unresolved,
                    "verification": done.preserved.verification,
                },
            }),
        ));
    }

    /// `_pwr/<command>`: the command's prose as `text`, outside any turn.
    fn extension_command(self: &Rc<Self>, id: Value, command: Command, params: &Value) {
        let Some(context) = self
            .sessions
            .borrow()
            .get(session_param(params))
            .map(Session::command_context)
        else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        let server = Rc::clone(self);
        tokio::task::spawn_local(async move {
            let reply = match server.runner.command(command, context).await {
                Ok(text) => result(id, json!({ "text": text })),
                Err(why) => error_response(id, -32000, &why),
            };
            server.send(reply);
        });
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_prompt_turns(
        self: &Rc<Self>,
        id: Value,
        session_id: String,
        root: PathBuf,
        conversation_id: pwr_domain::Id,
        mut messages: Vec<ChatMessage>,
        stop: Arc<AtomicBool>,
        steps: SharedStepSink,
        continuity: converse::Continuity,
        approvals: Arc<dyn ApprovalPrompt>,
        session_grants: Vec<pwr_tools::Approval>,
        goal_mode: bool,
    ) -> Value {
        let mut total_actions = 0usize;
        // Each turn numbers its calls from one, and a goal runs several turns
        // under one prompt: without an offset the second turn's first call
        // reused `turnN-call1`, and a client merged two different actions.
        let highest_call = Rc::new(std::cell::Cell::new(0u64));
        // The checks already failing when the goal starts, so the goal is
        // neither sent to repair them nor held open by them.
        let mut already_failing: Vec<String> = Vec::new();
        if goal_mode {
            let context = self
                .sessions
                .borrow()
                .get(&session_id)
                .map(Session::command_context);
            if let Some(context) = context
                && let Ok(baseline) = self.runner.verify_goal(context).await
                && !baseline.failing.is_empty()
            {
                already_failing = baseline.failing;
                if let Some(last) = messages.last_mut().filter(|last| last.role == "user") {
                    last.content
                        .push_str(&format!("\n\n{}", already_failing_note(&already_failing)));
                }
            }
        }
        loop {
            let base = highest_call.get();
            let outcome = self
                .runner
                .run(TurnInput {
                    root: root.clone(),
                    conversation_id,
                    messages,
                    stop: Arc::clone(&stop),
                    steps: Box::new({
                        let steps = Rc::clone(&steps);
                        let highest_call = Rc::clone(&highest_call);
                        move |mut step| {
                            if let TurnStep::ToolCall(call) = &mut step {
                                call.id += base;
                                highest_call.set(highest_call.get().max(call.id));
                            }
                            if let Ok(mut sink) = steps.try_borrow_mut() {
                                sink(step);
                            }
                        }
                    }),
                    continuity: continuity.clone(),
                    approvals: Arc::clone(&approvals),
                    session_grants: session_grants.clone(),
                    goal_mode,
                })
                .await;
            let (report, next_messages) = match outcome {
                Ok(value) => value,
                Err(problem) => return error_response(id, -32000, &problem),
            };
            total_actions = total_actions.saturating_add(report.actions);
            messages = next_messages;
            if let Some(session) = self.sessions.borrow_mut().get_mut(&session_id) {
                session.messages = messages.clone();
            }

            if !goal_mode
                || report.declined
                || report.stopped.is_some()
                    && !matches!(report.stopped, Some(converse::StopReason::BudgetSpent))
            {
                return self.turn_reply(id, &session_id, report, total_actions, goal_mode, None);
            }

            if report.completed {
                let context = self
                    .sessions
                    .borrow()
                    .get(&session_id)
                    .map(Session::command_context);
                let Some(context) = context else {
                    return error_response(id, -32602, "no such session");
                };
                match self.runner.verify_goal(context).await {
                    Ok(verification) if verification.passed => {
                        return self.turn_reply(
                            id,
                            &session_id,
                            report,
                            total_actions,
                            true,
                            Some(verification),
                        );
                    }
                    Ok(verification)
                        if !verification.technical_passed
                            && !verification.failing.is_empty()
                            && verification
                                .failing
                                .iter()
                                .all(|check| already_failing.contains(check)) =>
                    {
                        self.update(
                            &session_id,
                            message_chunk(
                                "agent_message_chunk",
                                &format!(
                                    "The checks that fail were already failing before this goal started, and were left alone: {}.\n{}",
                                    verification.failing.join(", "),
                                    verification.summary
                                ),
                            ),
                        );
                        return self.turn_reply(
                            id,
                            &session_id,
                            report,
                            total_actions,
                            true,
                            Some(verification),
                        );
                    }
                    Ok(verification)
                        if verification.technical_passed && !verification.acceptance_available =>
                    {
                        self.update(
                            &session_id,
                            message_chunk(
                                "agent_message_chunk",
                                &format!(
                                    "Technical checks passed, but the goal is not verified because this workspace has no declared acceptance check.\n{}",
                                    verification.summary
                                ),
                            ),
                        );
                        return self.turn_reply(
                            id,
                            &session_id,
                            report,
                            total_actions,
                            true,
                            Some(verification),
                        );
                    }
                    Ok(verification) => {
                        self.update(
                            &session_id,
                            message_chunk(
                                "agent_message_chunk",
                                &format!(
                                    "Goal verification did not pass:\n{}\n\nContinuing from the current workspace.",
                                    verification.summary
                                ),
                            ),
                        );
                        messages.push(ChatMessage::text(
                            "user",
                            format!(
                                "The goal is not complete: full repository verification failed. Fix the remaining issue. Evidence:\n{}",
                                verification.summary
                            ),
                        ));
                    }
                    Err(problem) => {
                        return error_response(
                            id,
                            -32000,
                            &format!("goal verification could not run: {problem}"),
                        );
                    }
                }
            } else if total_actions >= GOAL_MAX_ACTIONS {
                self.update(
                    &session_id,
                    message_chunk(
                        "agent_message_chunk",
                        &format!(
                            "Goal mode paused after {total_actions} actions without verified completion. Review the current changes, then continue deliberately if the objective still needs work."
                        ),
                    ),
                );
                return result(
                    id,
                    json!({
                        "stopReason": "max_turn_requests",
                        "_meta": {"pwr": {
                            "terminal": "budget",
                            "actions": report.actions,
                            "totalActions": total_actions,
                            "edited": report.edited,
                            "goal": {"enabled": true, "completed": false, "verified": false, "guardReached": true},
                        }},
                    }),
                );
            } else {
                self.update(
                    &session_id,
                    message_chunk(
                        "agent_message_chunk",
                        &format!(
                            "Checkpoint after {total_actions} action(s). Continuing toward the goal; use Stop to interrupt."
                        ),
                    ),
                );
                messages.push(ChatMessage::text(
                    "user",
                    "Continue the same goal from the saved workspace state. Do not stop with prose: either make the next necessary change, investigate an unmet requirement, or call complete only when the full objective is ready for verification.",
                ));
            }
        }
    }

    /// `_pwr/rewind`: the conversation back to just before the person's
    /// message `turn`, and with `restoreFiles` the files PWR edited since back
    /// to how they were. A file changed since by someone else stops the
    /// rewind and is named, unless `force`. Commands' own effects (an install,
    /// files a script wrote) are not undone, and a conversation reopened from
    /// disk can rewind its messages but not its files.
    fn rewind(&self, id: Value, params: &Value) {
        let session_id = session_param(params).to_owned();
        let Some(turn) = params
            .get("turn")
            .and_then(Value::as_u64)
            .and_then(|turn| u32::try_from(turn).ok())
        else {
            return self.send(error_response(id, -32602, "name the message with turn"));
        };
        let restore = params.get("restoreFiles").and_then(Value::as_bool) == Some(true);
        let force = params.get("force").and_then(Value::as_bool) == Some(true);
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(&session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if session.busy {
            return self.send(error_response(
                id,
                -32000,
                "wait for the turn to end, or stop it",
            ));
        }
        let Some(position) = session
            .rewind_points
            .iter()
            .position(|point| point.turn == turn)
        else {
            return self.send(error_response(
                id,
                -32000,
                "this message was sent before the conversation was opened here, so it cannot be rewound to",
            ));
        };
        let point = session.rewind_points[position].clone();
        // Starts with, not equals: a goal appends the checks already failing
        // to the message it answers.
        if !session.messages.get(point.at).is_some_and(|message| {
            message.role == "user" && message.content.starts_with(&point.text)
        }) {
            return self.send(error_response(
                id,
                -32000,
                "the conversation was compacted after this message, so it can no longer be rewound to",
            ));
        }
        let mut restored = Vec::new();
        let mut failed = Vec::new();
        if restore {
            let edits = session
                .continuity
                .edits
                .lock()
                .map(|edits| edits.clone())
                .unwrap_or_default();
            let written = session
                .continuity
                .written
                .lock()
                .map(|written| written.clone())
                .unwrap_or_default();
            let (plan, conflicts) = converse::rewind_plan(&session.root, &edits, &written, turn);
            if !conflicts.is_empty() && !force {
                return self.send(result(
                    id,
                    json!({"rewound": false, "conflicts": conflicts}),
                ));
            }
            for (path, before) in &plan {
                let outcome = match rewind_target(&session.root, path) {
                    Err(error) => Err(error),
                    Ok(target) => match before {
                        converse::Before::Content(bytes) => target
                            .parent()
                            .map_or(Ok(()), std::fs::create_dir_all)
                            .and_then(|()| std::fs::write(&target, bytes)),
                        converse::Before::Created => match std::fs::remove_file(&target) {
                            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            other => other,
                        },
                        converse::Before::NotKept => Err(std::io::Error::other(
                            "its earlier content was too large to keep, so it was left as it is",
                        )),
                    },
                };
                match outcome {
                    Ok(()) => restored.push(path.clone()),
                    Err(error) => failed.push(format!("{path}: {error}")),
                }
            }
            // What PWR wrote is now what an earlier turn left, or nothing.
            if let Ok(mut written) = session.continuity.written.lock() {
                for (path, before) in &plan {
                    let earlier = edits
                        .iter()
                        .any(|edit| edit.turn < turn && &edit.path == path);
                    match before {
                        converse::Before::Content(bytes) if earlier => {
                            written.insert(path.clone(), pwr_domain::hash_bytes(bytes));
                        }
                        // Left as PWR wrote it: still PWR's.
                        converse::Before::NotKept => {}
                        _ => {
                            written.remove(path);
                        }
                    }
                }
            }
            if let Ok(mut edits) = session.continuity.edits.lock() {
                edits.retain(|edit| edit.turn < turn);
            }
        }
        session.messages.truncate(point.at);
        converse::forget_reasoning(&mut session.messages);
        session.rewind_points.truncate(position);
        let root = session.root.clone();
        let conversation_id = session.conversation_id;
        let messages = session.messages.clone();
        drop(sessions);
        self.usage.borrow_mut().remove(&session_id);
        let detail = json!({
            "turn": turn,
            "restoreFiles": restore,
            "restored": restored,
            "failed": failed,
        });
        if let Err(why) = self
            .runner
            .record_rewind(&root, conversation_id, &messages, &detail)
        {
            eprintln!("pwr serve: the rewind was not recorded: {why}");
        }
        self.send(result(
            id,
            json!({"rewound": true, "turn": turn, "restored": restored, "failed": failed}),
        ));
    }

    /// Whether `root` is chat mode's folder rather than a workspace: it has
    /// no wiki, no project entry and no summaries.
    fn is_chat_home(&self, root: &Path) -> bool {
        match (self.runner.chat_home(), root.canonicalize()) {
            (Some(home), Ok(root)) => home == root,
            _ => false,
        }
    }

    /// Brings the workspace's wiki up to date after a turn, and logs the turn
    /// when it changed or finished something (`pwr_orchestrator::wiki`). A
    /// wiki that cannot be written costs the turn nothing.
    fn remember_work(&self, session_id: &str, report: &TurnReport, answer: &str) {
        let Some((root, request, files, chat_only)) =
            self.sessions.borrow().get(session_id).map(|session| {
                (
                    session.root.clone(),
                    session.last_request.clone(),
                    session
                        .continuity
                        .written
                        .lock()
                        .map(|written| written.keys().cloned().collect::<Vec<_>>())
                        .unwrap_or_default(),
                    session.continuity.chat_only,
                )
            })
        else {
            return;
        };
        if chat_only {
            return;
        }
        let Ok(home) = pwr_orchestrator::personal::Home::from_env() else {
            return;
        };
        let worked =
            (report.edited || report.completed).then_some(pwr_orchestrator::wiki::Worked {
                request: &request,
                answer,
                files,
            });
        if let Err(why) = pwr_orchestrator::wiki::refresh(&home, &root, worked) {
            eprintln!("pwr serve: the workspace wiki was not updated: {why}");
        }
    }

    fn turn_reply(
        &self,
        id: Value,
        session_id: &str,
        report: TurnReport,
        total_actions: usize,
        goal_mode: bool,
        goal_verification: Option<GoalVerification>,
    ) -> Value {
        let mut answer = match report.stopped {
            // The console's turn already says why it stopped; saying it again
            // showed the same paragraph twice around the check verdict.
            Some(reason) if report.answer.contains(&reason.said()) => report.answer.clone(),
            Some(reason) if report.answer.trim().is_empty() => reason.said(),
            Some(reason) => format!("{}\n\n{}", report.answer.trim(), reason.said()),
            None => report.answer.clone(),
        };
        if let Some(verification) = goal_verification.as_ref() {
            if verification.passed {
                answer.push_str("\n\nGoal verified by declared acceptance checks:\n");
            } else if verification.technical_passed {
                answer.push_str(
                    "\n\nTechnical checks passed, but goal acceptance is not verified:\n",
                );
            }
            answer.push_str(&verification.summary);
        }
        if !answer.trim().is_empty() {
            self.update(session_id, message_chunk("agent_message_chunk", &answer));
        }
        self.remember_work(session_id, &report, &answer);
        let (stop_reason, terminal) = stop_reason(&report);
        let mut meta = if goal_mode {
            json!({
                "terminal": terminal,
                "actions": report.actions,
                "totalActions": total_actions,
                "edited": report.edited,
                "completed": report.completed,
                "goal": {
                    "enabled": true,
                    "completed": report.completed,
                    "verified": goal_verification.as_ref().is_some_and(|verification| verification.passed),
                    "technicalPassed": goal_verification.as_ref().is_some_and(|verification| verification.technical_passed),
                    "acceptanceAvailable": goal_verification.as_ref().is_some_and(|verification| verification.acceptance_available),
                    "needsAcceptance": goal_verification.as_ref().is_some_and(|verification| verification.technical_passed && !verification.acceptance_available),
                    "verification": goal_verification.as_ref().map(|verification| &verification.summary),
                },
            })
        } else {
            json!({
                "terminal": terminal,
                "actions": report.actions,
                "edited": report.edited,
            })
        };
        // The stop's own words, which the answer above ends with, so a client
        // can show them as the run's state instead of as the model's prose.
        if let Some(reason) = report.stopped {
            meta["stoppedBecause"] = json!(reason.said());
        }
        result(
            id,
            json!({
                "stopReason": stop_reason,
                "_meta": {"pwr": meta},
            }),
        )
    }

    fn prompt(self: &Rc<Self>, id: Value, params: &Value) {
        let session_id = session_param(params).to_owned();
        let (text, attachments, images) = match prompt_parts(params) {
            Ok(parts) => parts,
            Err(why) => return self.send(error_response(id, -32602, &why)),
        };
        let goal_mode = match params.get("goalMode") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(enabled)) => *enabled,
            Some(_) => return self.send(error_response(id, -32602, "goalMode must be a boolean")),
        };
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(&session_id) else {
            return self.send(error_response(id, -32602, "no such session"));
        };
        if session.busy {
            return self.send(error_response(
                id,
                -32000,
                "a turn is already running in this session; cancel it or wait for it",
            ));
        }
        // Every attachment is read before the turn starts, and one refused
        // refuses the prompt: a turn run without a file the person attached
        // answers a question they did not ask.
        let mut attached = Vec::new();
        let mut images = images;
        for attachment in attachments {
            if let Attachment::File(path) = &attachment
                && let Some(image) = PromptImage::from_file(path)
            {
                match image {
                    Ok(image) => images.push(image),
                    Err(why) => {
                        return self.send(error_response(
                            id,
                            -32602,
                            &format!("attachment refused: {why}"),
                        ));
                    }
                }
                continue;
            }
            match self.runner.attach(&session.root, attachment) {
                Ok(content) => attached.push(content),
                Err(why) => {
                    return self.send(error_response(
                        id,
                        -32602,
                        &format!("attachment refused: {why}"),
                    ));
                }
            }
        }
        let mut stored = Vec::new();
        for image in &images {
            match self.runner.store_image(&session.root, image) {
                Ok(path) => stored.push(path),
                Err(why) => {
                    return self.send(error_response(id, -32602, &format!("image refused: {why}")));
                }
            }
        }
        session.last_request = text.to_owned();
        let text = if goal_mode {
            // A single `\` continues the line; `\\` had put a literal
            // backslash and the next line's indentation into every goal-mode
            // prompt the model read (seen 2026-09-22 in a replayed message).
            format!(
                "{}\n\nGoal mode is enabled. Keep working until the requested outcome is actually complete. \
                 Do not end with prose alone: call `complete` only after inspecting the resulting workspace. \
                 The core will run the repository's full declared checks when you call it; if they fail, use the \
                 evidence to repair the work. For behaviour the repository does not already test, add or improve \
                 focused deterministic checks when that is appropriate. Never claim a page, route, integration, \
                 or cleanup is finished without evidence from the workspace.",
                with_attachments(text, &attached)
            )
        } else {
            with_attachments(text, &attached)
        };
        session.busy = true;
        let server = Rc::clone(self);
        // A prompt that is only `/name` runs the command, as the console does,
        // rather than sending the deployment a slash it cannot act on.
        if let Some(command) = slash_command(&text) {
            let context = session.command_context();
            drop(sessions);
            tokio::task::spawn_local(async move {
                let said = server
                    .runner
                    .command(command, context)
                    .await
                    .unwrap_or_else(|why| format!("could not run /{}: {why}", command.name()));
                server.update(&session_id, message_chunk("agent_message_chunk", &said));
                server.finish(&session_id, result(id, json!({"stopReason": "end_turn"})));
            });
            return;
        }
        session.turns += 1;
        session.stop = Arc::new(AtomicBool::new(false));
        session.continuity.operator_spoke();
        converse::forget_reasoning(&mut session.messages);
        session
            .continuity
            .person_turn
            .store(session.turns, Ordering::Relaxed);
        session.rewind_points.push(RewindPoint {
            turn: session.turns,
            at: session.messages.len(),
            text: text.clone(),
        });
        self.send(notification(
            "_pwr/turn_started",
            json!({"sessionId": session_id, "turn": session.turns}),
        ));
        let mut message = ChatMessage::text("user", text);
        message.images = stored;
        session.messages.push(message);
        let number = session.turns;
        let root = session.root.clone();
        let summary_root = root.clone();
        let chat_only = session.continuity.chat_only;
        let conversation_id = session.conversation_id;
        let messages = session.messages.clone();
        let continuity = session.continuity.clone();
        let stop = Arc::clone(&session.stop);
        let grants = Arc::clone(&session.grants);
        drop(sessions);
        tokio::task::spawn_local(async move {
            let asking_about: Arc<Mutex<Option<String>>> = Arc::default();
            let steps: Box<dyn FnMut(TurnStep)> = {
                let server = Rc::clone(&server);
                let session_id = session_id.clone();
                let root = root.clone();
                let asking_about = Arc::clone(&asking_about);
                Box::new(move |step| {
                    if let TurnStep::ToolCall(call) = &step
                        && call.phase == ToolPhase::Proposed
                        && let Ok(mut proposed) = asking_about.lock()
                    {
                        *proposed = Some(tool_call_id(number, call.id));
                    }
                    if let TurnStep::MemoryProposed { text, scope } = &step {
                        server.send(notification(
                            "_pwr/memory_proposed",
                            json!({"sessionId": session_id, "text": text, "scope": scope}),
                        ));
                        return;
                    }
                    if let TurnStep::Compacted(note) = &step {
                        server.send(notification(
                            "_pwr/compacted",
                            json!({"sessionId": session_id, "trigger": "automatic", "note": note}),
                        ));
                        return;
                    }
                    if let TurnStep::Usage {
                        used,
                        window,
                        estimated,
                    } = &step
                    {
                        // Only the engine's own count answers `_pwr/context`'s
                        // "counted by engine"; an estimate is for the meter.
                        if !estimated {
                            server
                                .usage
                                .borrow_mut()
                                .insert(session_id.clone(), (*used, *window));
                        }
                        server.send(notification(
                            "_pwr/usage",
                            json!({
                                "sessionId": session_id,
                                "used": used,
                                "window": window,
                                "estimated": estimated,
                            }),
                        ));
                        return;
                    }
                    if let TurnStep::Streaming { thinking, content } = &step {
                        if thinking.is_empty() && content.is_empty() {
                            server.send(notification(
                                "_pwr/model_progress",
                                json!({"sessionId": session_id}),
                            ));
                            return;
                        }
                        // ACP's own channels: reasoning as a thought, the
                        // answer as a message chunk marked live, so a client
                        // can tell it from the whole answer the turn sends at
                        // its end.
                        if !thinking.is_empty() {
                            server.update(
                                &session_id,
                                message_chunk("agent_thought_chunk", thinking),
                            );
                        }
                        if !content.is_empty() {
                            let mut update = message_chunk("agent_message_chunk", content);
                            update["_meta"] = json!({"pwr": {"live": true}});
                            server.update(&session_id, update);
                        }
                        return;
                    }
                    if let Some(mut event) = turn_event(&step) {
                        event["sessionId"] = json!(session_id);
                        server.send(notification("_pwr/turn_event", event));
                        return;
                    }
                    if let Some(update) = tool_update(&root, number, step) {
                        server.update(&session_id, update);
                    }
                })
            };
            let approvals: Arc<dyn ApprovalPrompt> = Arc::new(ClientApproval {
                out: server.out.clone(),
                pending: Arc::clone(&server.pending),
                next_id: Arc::clone(&server.next_id),
                session_id: session_id.clone(),
                grants: Arc::clone(&grants),
                asking_about,
            });
            let session_grants = grants
                .lock()
                .map(|grants| grants.clone())
                .unwrap_or_default();
            let reply = server
                .run_prompt_turns(
                    id,
                    session_id.clone(),
                    root,
                    conversation_id,
                    messages,
                    stop,
                    Rc::new(RefCell::new(steps)),
                    continuity,
                    approvals,
                    session_grants,
                    goal_mode,
                )
                .await;
            server.finish(&session_id, reply);
            if !chat_only {
                server.summarise_while_idle(summary_root);
            }
        });
    }

    /// Writes the wiki's missing or stale module summaries while no session
    /// is working, one module at a time, and stops as soon as one is: the
    /// engine serves one generation at a time, and a person's next message
    /// must not wait behind more than one short summary. What is left is
    /// picked up after the next turn.
    fn summarise_while_idle(self: &Rc<Self>, root: PathBuf) {
        if self.summarising.replace(true) {
            return;
        }
        let server = Rc::clone(self);
        tokio::task::spawn_local(async move {
            server.summarise_modules(&root).await;
            server.summarising.set(false);
        });
    }

    async fn summarise_modules(&self, root: &Path) {
        use pwr_orchestrator::{graph, personal, wiki};
        const PER_IDLE: usize = 8;
        let Ok(home) = personal::Home::from_env() else {
            return;
        };
        let Ok(index) = wiki::index(root) else {
            return;
        };
        let written_before = wiki::summaries(root);
        let language = personal::load_profile(&home)
            .map(|profile| profile.language)
            .unwrap_or_default();
        let mut written = 0;
        let cwd = root.display().to_string();
        self.send(notification(
            "_pwr/wiki_summarising",
            json!({"cwd": cwd, "state": "started"}),
        ));
        for id in graph::summary_candidates(&index).into_iter().take(40) {
            if written >= PER_IDLE || self.sessions.borrow().values().any(|session| session.busy) {
                break;
            }
            let Some(hash) = graph::source_hash(&index, &id) else {
                continue;
            };
            if written_before
                .get(&id)
                .is_some_and(|summary| summary.source_hash == hash)
            {
                continue;
            }
            let prompt = wiki::summary_prompt(root, &index, &id, &language);
            self.send(notification(
                "_pwr/wiki_summarising",
                json!({"cwd": cwd, "state": "writing", "module": id, "written": written}),
            ));
            match self.runner.summarise(root, prompt).await {
                Ok((text, model)) if !text.trim().is_empty() => {
                    let summary = graph::Summary {
                        text: text.trim().to_owned(),
                        source_hash: hash,
                        model,
                        at: chrono::Utc::now().to_rfc3339(),
                    };
                    if wiki::save_summary(root, &id, summary).is_err() {
                        break;
                    }
                    written += 1;
                }
                // No engine, or it failed: the next turn tries again.
                _ => break,
            }
        }
        if written > 0 {
            let _ = wiki::refresh(&home, root, None);
            self.send(notification(
                "_pwr/wiki_updated",
                json!({"cwd": root, "summaries": written}),
            ));
        }
        self.send(notification(
            "_pwr/wiki_summarising",
            json!({"cwd": cwd, "state": "finished", "written": written}),
        ));
    }

    /// Frees the session for its next prompt, then answers the one that ran.
    fn finish(&self, session_id: &str, reply: Value) {
        if let Some(session) = self.sessions.borrow_mut().get_mut(session_id) {
            session.busy = false;
        }
        self.send(reply);
    }
}

/// The most often a running download is reported to the client.
const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// A download's state, as `_pwr/download_progress` carries it.
fn download_notification(
    download_id: &str,
    state: &pwr_models::download::DownloadState,
    progress: Option<&pwr_models::download::Progress>,
) -> Value {
    let mut params = json!({"downloadId": download_id, "state": state});
    if let Some(progress) = progress {
        // The fields clients written against the first version read.
        params["file"] = json!(progress.file);
        params["bytes"] = json!(progress.file_bytes);
        params["expectedBytes"] = json!(progress.file_total);
        params["totalBytes"] = json!(progress.bytes);
        params["totalExpectedBytes"] = json!(progress.total);
        params["status"] = json!("running");
    }
    notification("_pwr/download_progress", params)
}

fn session_param(params: &Value) -> &str {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

async fn wait_for_stop(stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// A question the session's turn is waiting on is answered as cancelled, which
/// the turn reads as a refusal.
fn cancel_questions(pending: &Pending, session_id: &str) {
    if let Ok(mut waiting) = pending.lock() {
        let open: Vec<i64> = waiting
            .iter()
            .filter(|(_, (session, _))| session == session_id)
            .map(|(id, _)| *id)
            .collect();
        for id in open {
            if let Some((_, reply)) = waiting.remove(&id) {
                let _ = reply.send(json!({"result": {"outcome": {"outcome": "cancelled"}}}));
            }
        }
    }
}

/// The command a prompt names, when it is only `/name`.
fn slash_command(text: &str) -> Option<Command> {
    Command::named(text.trim().strip_prefix('/')?)
}

fn message_chunk(kind: &str, text: &str) -> Value {
    json!({"sessionUpdate": kind, "content": {"type": "text", "text": text}})
}

fn available_commands(session_id: &str) -> Value {
    let commands: Vec<Value> = Command::ALL
        .iter()
        .map(|command| json!({"name": command.name(), "description": command.description()}))
        .collect();
    notification(
        "session/update",
        json!({
            "sessionId": session_id,
            "update": {"sessionUpdate": "available_commands_update", "availableCommands": commands},
        }),
    )
}

/// A restored conversation as a client shows it: what the person asked and what
/// the deployment said. Harness context -- the system prompt, ledgers, excerpts,
/// tool results, the resume note -- is the deployment's, not the transcript's.
fn replay(messages: &[ChatMessage]) -> Vec<Value> {
    messages
        .iter()
        .filter(|message| !message.content.trim().is_empty())
        .filter_map(|message| match (message.role.as_str(), message.purpose) {
            ("user", None | Some(pwr_domain::MessagePurpose::Task)) => {
                Some(message_chunk("user_message_chunk", &message.content))
            }
            ("assistant", _) => Some(message_chunk("agent_message_chunk", &message.content)),
            _ => None,
        })
        .collect()
}

/// Puts an approval to the client as `session/request_permission`.
struct ClientApproval {
    out: mpsc::UnboundedSender<Value>,
    pending: Pending,
    next_id: Arc<AtomicI64>,
    session_id: String,
    grants: Arc<Mutex<Vec<pwr_tools::Approval>>>,
    /// The tool call the turn last proposed, which is the one being asked
    /// about: the turn announces an action before putting it to the policy,
    /// on the same task, so nothing can come between them.
    asking_about: Arc<Mutex<Option<String>>>,
}

#[async_trait::async_trait]
impl ApprovalPrompt for ClientApproval {
    async fn ask(&self, approval: pwr_tools::Approval, description: &str) -> ApprovalDecision {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply, answer) = oneshot::channel();
        if let Ok(mut waiting) = self.pending.lock() {
            waiting.insert(id, (self.session_id.clone(), reply));
        }
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "session/request_permission",
            "params": {
                "sessionId": self.session_id,
                "toolCall": {
                    "toolCallId": self
                        .asking_about
                        .lock()
                        .ok()
                        .and_then(|call| call.clone())
                        .unwrap_or_else(|| format!("permission-{id}")),
                    "title": description,
                    "status": "pending",
                },
                "options": [
                    {"optionId": "allow_once", "name": "Allow once", "kind": "allow_once"},
                    {"optionId": "allow_always", "name": "Allow for this session", "kind": "allow_always"},
                    {"optionId": "reject_once", "name": "Refuse", "kind": "reject_once"},
                ],
                "_meta": {"pwr": {"approval": approval}},
            },
        });
        if self.out.send(request).is_err() {
            return ApprovalDecision::Deny;
        }
        let decision = match answer.await {
            Ok(response) => match response["result"]["outcome"]["optionId"].as_str() {
                Some("allow_once") => ApprovalDecision::AllowOnce,
                Some("allow_always") => ApprovalDecision::AllowForRun,
                _ => ApprovalDecision::Deny,
            },
            Err(_) => ApprovalDecision::Deny,
        };
        if decision == ApprovalDecision::AllowForRun
            && let Ok(mut grants) = self.grants.lock()
            && !grants.contains(&approval)
        {
            grants.push(approval);
        }
        decision
    }
}

/// A prompt's text, its attachments and its images. Links must name local
/// files, since this process reads them; audio is refused, as `initialize`
/// said it would be. Whether the model can see an image is the runner's to
/// say, when the prompt is taken.
type PromptParts = (String, Vec<Attachment>, Vec<PromptImage>);

fn prompt_parts(params: &Value) -> Result<PromptParts, String> {
    use base64::Engine;
    let mut text = Vec::new();
    let mut attachments = Vec::new();
    let mut images = Vec::new();
    for block in params["prompt"].as_array().into_iter().flatten() {
        match block["type"].as_str().unwrap_or_default() {
            "text" => text.push(block["text"].as_str().unwrap_or_default()),
            "resource_link" => {
                let uri = block["uri"].as_str().unwrap_or_default();
                let path = url::Url::parse(uri)
                    .ok()
                    .filter(|url| url.scheme() == "file")
                    .and_then(|url| url.to_file_path().ok())
                    .ok_or_else(|| {
                        format!("only files on this machine can be attached, not {uri:?}")
                    })?;
                attachments.push(Attachment::File(path));
            }
            "resource" => {
                let resource = &block["resource"];
                let uri = resource["uri"].as_str().unwrap_or_default().to_owned();
                let bytes = if let Some(text) = resource["text"].as_str() {
                    text.as_bytes().to_vec()
                } else if let Some(blob) = resource["blob"].as_str() {
                    base64::engine::general_purpose::STANDARD
                        .decode(blob)
                        .map_err(|error| {
                            format!("the resource {uri:?} is not valid base64: {error}")
                        })?
                } else {
                    return Err(format!("the resource {uri:?} has neither text nor blob"));
                };
                attachments.push(Attachment::Embedded { uri, bytes });
            }
            "image" => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(block["data"].as_str().unwrap_or_default())
                    .map_err(|error| format!("the image is not valid base64: {error}"))?;
                let image = PromptImage {
                    mime: block["mimeType"].as_str().unwrap_or_default().to_owned(),
                    bytes,
                };
                if image.extension().is_none() {
                    return Err(format!(
                        "{:?} images are not supported: attach PNG, JPEG, WebP or GIF",
                        image.mime
                    ));
                }
                images.push(image);
            }
            other => {
                return Err(format!(
                    "{other} content is not supported: attach text files, PDFs or images"
                ));
            }
        }
    }
    Ok((text.join("\n"), attachments, images))
}

/// ACP's tool kind for a PWR capability.
pub fn tool_kind(capability: &str) -> &'static str {
    match capability {
        "read_file" => "read",
        "search" | "find_definition" | "list_tree" | "recall_project" | "wiki_query" => "search",
        "remember" => "think",
        "replace_text" | "apply_patch" | "apply_replace" | "write_file" | "make_directory"
        | "restore_file" => "edit",
        "delete_path" => "delete",
        "move_path" => "move",
        "run_command" | "start_service" | "stop_service" => "execute",
        "fetch_url" => "fetch",
        _ => "other",
    }
}

/// A tool call's id, unique across a session's turns.
fn tool_call_id(turn: u32, call: u64) -> String {
    format!("turn{turn}-call{call}")
}

/// The `session/update` payload for a step, where the step has one.
fn tool_update(root: &Path, turn: u32, step: TurnStep) -> Option<Value> {
    let TurnStep::ToolCall(call) = step else {
        return None;
    };
    let id = tool_call_id(turn, call.id);
    let title = match &call.path {
        Some(path) => format!("{} {path}", call.capability),
        None => call.capability.clone(),
    };
    let detail = call.detail;
    let locations: Vec<Value> = call
        .path
        .iter()
        .map(|path| json!({"path": root.join(path)}))
        .collect();
    Some(match call.phase {
        // Announced here, before the policy is asked about it, so a permission
        // request has a tool call to belong to.
        ToolPhase::Proposed => json!({
            "sessionUpdate": "tool_call",
            "toolCallId": id,
            "title": title,
            "_meta": {"pwr": {"detail": detail}},
            "kind": tool_kind(&call.capability),
            "status": "pending",
            "locations": locations,
        }),
        ToolPhase::Started => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": id,
            "status": "in_progress",
            "_meta": {"pwr": {"detail": detail}},
        }),
        ToolPhase::Completed => {
            let content: Vec<Value> = call
                .diff
                .iter()
                .map(|diff| {
                    json!({
                        "type": "diff",
                        "path": root.join(&diff.path),
                        "oldText": diff.old_text,
                        "newText": diff.new_text,
                    })
                })
                .collect();
            json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": id,
                "status": "completed",
                "content": content,
                "_meta": {"pwr": {"detail": detail}},
            })
        }
        // Announced already, so an update.
        ToolPhase::Failed(why) => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": id,
            "status": "failed",
            "content": [{"type": "content", "content": {"type": "text", "text": why}}],
            "_meta": {"pwr": {"detail": detail}},
        }),
        // Proposed and never started.
        ToolPhase::Refused(why) => json!({
            "sessionUpdate": "tool_call_update",
            "toolCallId": id,
            "status": "failed",
            "content": [{"type": "content", "content": {"type": "text", "text": why}}],
            "_meta": {"pwr": {"detail": detail}},
        }),
    })
}

/// `_pwr/turn_event`: what a turn did that ACP has no update for -- an
/// automatic retry, the recovery after one, a generation's counts and timings,
/// a note the console prints -- so a client can show the run as state rather
/// than parse prose. Figures the backend did not report are `null`.
fn turn_event(step: &TurnStep) -> Option<Value> {
    let millis = |duration: Option<std::time::Duration>| {
        duration.map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
    };
    let from_nanos = |nanos: Option<u64>| nanos.map(|nanos| nanos / 1_000_000);
    Some(match step {
        TurnStep::Retry {
            cause,
            attempt,
            limit,
            detail,
        } => json!({
            "event": "retry",
            "cause": cause,
            "attempt": attempt,
            "limit": limit,
            "detail": detail,
        }),
        TurnStep::Recovered { retries } => json!({"event": "recovered", "retries": retries}),
        TurnStep::Generation(stats) => {
            let metrics = stats.metrics.clone().unwrap_or_default();
            json!({
                "event": "generation",
                "promptTokens": metrics.prompt_tokens,
                "generatedTokens": metrics.generated_tokens,
                "reasoningTokens": metrics.reasoning_tokens,
                "answerTokens": metrics.answer_tokens,
                "tokenAccounting": metrics.token_accounting,
                "promptEvalMs": from_nanos(metrics.prompt_eval_duration_ns),
                "generationMs": from_nanos(metrics.generation_duration_ns),
                "elapsedMs": millis(Some(stats.elapsed)),
                "firstChunkMs": millis(stats.first_chunk),
            })
        }
        TurnStep::Refused(text) => json!({"event": "note", "level": "warning", "text": text}),
        TurnStep::Note(text) => json!({"event": "note", "level": "info", "text": text}),
        _ => return None,
    })
}

/// ACP's stop reason for a turn, and PWR's terminal class beside it.
pub fn stop_reason(report: &TurnReport) -> (&'static str, Option<pwr_domain::TerminalClass>) {
    use converse::StopReason;
    match report.stopped {
        None if report.declined => ("refusal", Some(pwr_domain::TerminalClass::Declined)),
        None => ("end_turn", None),
        Some(reason) => (
            match reason {
                StopReason::Interrupted => "cancelled",
                StopReason::BudgetSpent => "max_turn_requests",
                StopReason::ContextFull | StopReason::Looping => "max_tokens",
                StopReason::Silent
                | StopReason::ToolCallInReasoning
                | StopReason::Unparseable
                | StopReason::BackendFailing
                | StopReason::NoProgress
                | StopReason::ReasoningUnfinished => "end_turn",
            },
            Some(reason.terminal_class()),
        ),
    }
}

fn result(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn notification(method: &str, params: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": method, "params": params})
}

/// `_pwr/wiki`: the workspace `cwd`'s wiki -- its overview, its log, its
/// modules with their summaries, the graph's outline -- and, with `query`, a
/// node of the graph and its neighbours. Built on first ask when there is none.
fn wiki_request(id: Value, params: &Value) -> Value {
    use pwr_orchestrator::{graph::NodeKind, personal, wiki};
    let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
        return error_response(id, -32602, "name the workspace with cwd");
    };
    let dir = root.join(".pwr/wiki");
    if !dir.join("graph.json").is_file() {
        let built = personal::Home::from_env().and_then(|home| wiki::refresh(&home, &root, None));
        if let Err(why) = built {
            return error_response(id, -32000, &format!("the wiki could not be built: {why}"));
        }
    }
    let Some(graph) = wiki::load_graph(&root) else {
        return error_response(id, -32000, "the workspace's graph is unreadable");
    };
    let modules: Vec<Value> = pwr_orchestrator::graph::summary_candidates_in(&graph)
        .iter()
        .filter_map(|module| graph.nodes.iter().find(|node| &node.id == module))
        .filter(|node| matches!(node.kind, NodeKind::Module | NodeKind::Project))
        .take(40)
        .map(|node| {
            json!({
                "id": node.id,
                "label": node.label,
                "summary": node.attrs.get("summary"),
                "stale": node.attrs.get("summaryStale"),
                "model": node.attrs.get("summaryModel"),
            })
        })
        .collect();
    let work: Vec<Value> = wiki::work_entries(&root)
        .into_iter()
        .rev()
        .take(30)
        .map(|entry| json!({"when": entry.when, "request": entry.request, "files": entry.files}))
        .collect();
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // For the 3D view: symbols only when asked for, since a large project has
    // thousands and they bury the structure.
    let symbols = params.get("includeSymbols").and_then(Value::as_bool) == Some(true);
    let shown: std::collections::HashSet<&str> = graph
        .nodes
        .iter()
        .filter(|node| symbols || node.kind != NodeKind::Symbol)
        .take(4_000)
        .map(|node| node.id.as_str())
        .collect();
    let view_nodes: Vec<Value> = graph
        .nodes
        .iter()
        .filter(|node| shown.contains(node.id.as_str()))
        .map(|node| {
            json!({
                "id": node.id,
                "kind": node.kind,
                "label": node.label,
                "summary": node.attrs.get("summary"),
                "stale": node.attrs.get("summaryStale"),
                "builtin": node.attrs.get("builtin"),
            })
        })
        .collect();
    let view_edges: Vec<&pwr_orchestrator::graph::Edge> = graph
        .edges
        .iter()
        .filter(|edge| shown.contains(edge.from.as_str()) && shown.contains(edge.to.as_str()))
        .collect();
    result(
        id,
        json!({
            "graph": {"nodes": view_nodes, "edges": view_edges},
            "overview": std::fs::read_to_string(dir.join("overview.md")).unwrap_or_default(),
            "outline": graph.outline(),
            "generatedAt": graph.generated_at,
            "nodes": graph.nodes.len(),
            "edges": graph.edges.len(),
            "modules": modules,
            "work": work,
            "answer": (!query.trim().is_empty()).then(|| graph.neighbourhood(query)),
        }),
    )
}

/// Folders the Files card does not list: dependencies, builds, VCS internals.
const FILES_SKIPPED: [&str; 10] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".pwr-scratch",
    ".angular",
];

/// A workspace-relative path inside `root`, or why not.
fn inside(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative = Path::new(relative.trim_start_matches("./"));
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("the path must be inside the workspace".into());
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(&root) {
        return Err("the path must be inside the workspace".into());
    }
    Ok(path)
}

/// Where a rewind puts a file back: inside `root`, even when a folder on the
/// way was replaced by a link since the edit (the file need not exist).
fn rewind_target(root: &Path, relative: &str) -> std::io::Result<PathBuf> {
    let outside = || std::io::Error::other("it is no longer inside the workspace");
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|part| {
            !matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(outside());
    }
    let root = root.canonicalize()?;
    let target = root.join(relative);
    // The nearest part of the path that exists, resolved, decides.
    let mut existing = target.as_path();
    while std::fs::symlink_metadata(existing).is_err() {
        existing = existing.parent().ok_or_else(outside)?;
    }
    if !existing.canonicalize()?.starts_with(&root) {
        return Err(outside());
    }
    Ok(target)
}

/// `_pwr/files`: one folder of the workspace `cwd` (`path`, the root when
/// omitted), folders first. For the person to look at; the model reads files
/// through its own tools and policy.
fn files_request(id: Value, params: &Value) -> Value {
    let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
        return error_response(id, -32602, "name the workspace with cwd");
    };
    let relative = params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let dir = match inside(&root, relative) {
        Ok(dir) => dir,
        Err(why) => return error_response(id, -32602, &why),
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return error_response(id, -32000, "the folder could not be read");
    };
    let base = relative.trim_start_matches("./").trim_end_matches('/');
    let mut listed: Vec<(bool, String, u64)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if FILES_SKIPPED.contains(&name.as_str()) || name == ".DS_Store" {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((metadata.is_dir(), name, metadata.len()))
        })
        .collect();
    listed.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| a.1.to_lowercase().cmp(&b.1.to_lowercase()))
    });
    listed.truncate(2_000);
    let entries: Vec<Value> = listed
        .into_iter()
        .map(|(dir, name, bytes)| {
            let path = if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            };
            json!({"name": name, "path": path, "dir": dir, "bytes": bytes})
        })
        .collect();
    result(id, json!({ "path": base, "entries": entries }))
}

/// `_pwr/file`: one text file of the workspace `cwd`, up to 1 MB.
fn file_request(id: Value, params: &Value) -> Value {
    const LIMIT: usize = 1024 * 1024;
    let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
        return error_response(id, -32602, "name the workspace with cwd");
    };
    let relative = params
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let path = match inside(&root, relative) {
        Ok(path) if path.is_file() => path,
        Ok(_) => return error_response(id, -32602, "not a file"),
        Err(why) => return error_response(id, -32602, &why),
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return error_response(id, -32000, "the file could not be read");
    };
    let size = bytes.len();
    let head = &bytes[..size.min(LIMIT)];
    if head.iter().take(8_000).any(|byte| *byte == 0) {
        return result(id, json!({"path": relative, "bytes": size, "binary": true}));
    }
    result(
        id,
        json!({
            "path": relative,
            "bytes": size,
            "binary": false,
            "truncated": size > LIMIT,
            "text": String::from_utf8_lossy(head),
        }),
    )
}

/// `_pwr/projects`: the workspaces PWR keeps a wiki for, most recent first;
/// with `forget` (a path), that one is dropped from the list first. The
/// folder and its wiki are left alone.
fn projects_request(id: Value, params: &Value) -> Value {
    use pwr_orchestrator::{personal, wiki};
    let home = match personal::Home::from_env() {
        Ok(home) => home,
        Err(why) => return error_response(id, -32000, &why),
    };
    if let Some(path) = params.get("forget").and_then(Value::as_str)
        && let Err(why) = wiki::forget(&home, Path::new(path))
    {
        return error_response(id, -32000, &why);
    }
    result(id, json!({ "projects": wiki::projects(&home) }))
}

/// `_pwr/profile`: the person's profile, and with `profile` in the params,
/// saved first. See `pwr_orchestrator::personal`.
fn profile_request(id: Value, params: &Value) -> Value {
    use pwr_orchestrator::personal;
    let home = match personal::Home::from_env() {
        Ok(home) => home,
        Err(why) => return error_response(id, -32000, &why),
    };
    let outcome = match params.get("profile") {
        None | Some(Value::Null) => personal::load_profile(&home),
        Some(value) => match serde_json::from_value::<personal::Profile>(value.clone()) {
            Ok(profile) => personal::save_profile(&home, &profile),
            Err(error) => {
                return error_response(id, -32602, &format!("profile is not readable: {error}"));
            }
        },
    };
    match outcome {
        Ok(profile) => result(id, json!({ "profile": profile })),
        Err(why) => error_response(id, -32000, &why),
    }
}

/// `_pwr/memory`: the memories of the person and of the workspace `cwd`, and
/// the project's instructions file. `action` is `list` (the default), `add`
/// (`scope`, `text`, optional `source`), `update` (`scope`, `id`, `text`) or
/// `delete` (`scope`, `id`); every reply is the whole list after it.
fn memory_request(id: Value, params: &Value) -> Value {
    use pwr_orchestrator::personal::{self, Scope};
    let Some(root) = params.get("cwd").and_then(Value::as_str).map(PathBuf::from) else {
        return error_response(id, -32602, "name the workspace with cwd");
    };
    let home = match personal::Home::from_env() {
        Ok(home) => home,
        Err(why) => return error_response(id, -32000, &why),
    };
    let text = |name: &str| params.get(name).and_then(Value::as_str).unwrap_or_default();
    let scope = match text("scope") {
        "global" => Some(Scope::Global),
        "workspace" => Some(Scope::Workspace),
        "" => None,
        _ => return error_response(id, -32602, "scope is `workspace` or `global`"),
    };
    let action = match text("action") {
        "" => "list",
        other => other,
    };
    let changed = match (action, scope) {
        ("list", _) => Ok(()),
        ("add", Some(scope)) => personal::add_memory(
            &home,
            scope,
            &root,
            text("text"),
            params
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )
        .map(|_| ()),
        ("update", Some(scope)) => {
            personal::update_memory(&home, scope, &root, text("id"), text("text"))
        }
        ("delete", Some(scope)) => personal::delete_memory(&home, scope, &root, text("id")),
        ("add" | "update" | "delete", None) => {
            return error_response(id, -32602, "name the scope: `workspace` or `global`");
        }
        _ => return error_response(id, -32602, "action is list, add, update or delete"),
    };
    if let Err(why) = changed {
        return error_response(id, -32000, &why);
    }
    let listed = |scope| personal::load_memories(&home, scope, &root);
    match (listed(Scope::Global), listed(Scope::Workspace)) {
        (Ok(global), Ok(workspace)) => result(
            id,
            json!({
                "global": global,
                "workspace": workspace,
                "instructions": personal::project_instructions(&root)
                    .map(|(path, text)| json!({"path": path, "chars": text.len()})),
            }),
        ),
        (Err(why), _) | (_, Err(why)) => error_response(id, -32000, &why),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewind_writes_only_inside_the_workspace() {
        let workspace = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        std::fs::create_dir(workspace.path().join("src")).unwrap();
        assert!(rewind_target(workspace.path(), "src/new/a.py").is_ok());
        assert!(rewind_target(workspace.path(), "../a.py").is_err());
        assert!(rewind_target(workspace.path(), "/etc/hosts").is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(elsewhere.path(), workspace.path().join("out")).unwrap();
            assert!(rewind_target(workspace.path(), "out/a.py").is_err());
            std::os::unix::fs::symlink(
                elsewhere.path().join("x.py"),
                workspace.path().join("x.py"),
            )
            .unwrap();
            std::fs::write(elsewhere.path().join("x.py"), "").unwrap();
            assert!(rewind_target(workspace.path(), "x.py").is_err());
        }
    }

    #[test]
    fn the_files_card_reads_only_inside_the_workspace() {
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.txt"), "x").unwrap();
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(root.path().join("src/a.py"), "print(1)").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("link")).unwrap();
        assert!(inside(root.path(), "src/a.py").is_ok());
        assert!(inside(root.path(), "").is_ok());
        assert!(inside(root.path(), "../secret.txt").is_err());
        assert!(inside(root.path(), "/etc/passwd").is_err());
        #[cfg(unix)]
        assert!(inside(root.path(), "link/secret.txt").is_err());
        let listed = files_request(json!(1), &json!({"cwd": root.path()}));
        assert_eq!(listed["result"]["entries"][0]["path"], "src");
        let read = file_request(json!(2), &json!({"cwd": root.path(), "path": "src/a.py"}));
        assert_eq!(read["result"]["text"], "print(1)");
    }
    use converse::{FileDiff, StopReason, ToolCallStep};
    use std::time::Duration;
    use tokio::io::{BufReader, DuplexStream, Lines};

    /// A turn chosen by the prompt's text, so the protocol is tested apart
    /// from any deployment.
    struct Scripted;

    fn report(answer: &str) -> TurnReport {
        TurnReport {
            answer: answer.into(),
            actions: 0,
            edited: false,
            completed: false,
            stopped: None,
            declined: false,
        }
    }

    #[async_trait::async_trait(?Send)]
    impl TurnRunner for Scripted {
        async fn open(&self, root: &Path) -> Result<Vec<ChatMessage>, String> {
            if root.ends_with("unprepared") {
                return Err("the chosen model is not prepared".into());
            }
            Ok(vec![ChatMessage::text("system", "scripted")])
        }

        async fn run(&self, mut turn: TurnInput) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            let asked = turn
                .messages
                .last()
                .map(|message| message.content.clone())
                .unwrap_or_default();
            let mut messages = turn.messages;
            let outcome = match asked.as_str() {
                "edit" => {
                    let step = |phase, diff| {
                        TurnStep::ToolCall(ToolCallStep {
                            id: 1,
                            capability: "replace_text".into(),
                            detail: "src/lib.rs".into(),
                            path: Some("src/lib.rs".into()),
                            phase,
                            diff,
                        })
                    };
                    (turn.steps)(step(ToolPhase::Proposed, None));
                    (turn.steps)(step(ToolPhase::Started, None));
                    (turn.steps)(TurnStep::Note("not shown".into()));
                    (turn.steps)(step(
                        ToolPhase::Completed,
                        Some(FileDiff {
                            path: "src/lib.rs".into(),
                            old_text: Some("a".into()),
                            new_text: "b".into(),
                        }),
                    ));
                    TurnReport {
                        actions: 1,
                        edited: true,
                        ..report("edited")
                    }
                }
                "refused" => {
                    let step = |phase| {
                        TurnStep::ToolCall(ToolCallStep {
                            id: 1,
                            capability: "run_command".into(),
                            detail: "npm test".into(),
                            path: None,
                            phase,
                            diff: None,
                        })
                    };
                    (turn.steps)(step(ToolPhase::Proposed));
                    (turn.steps)(step(ToolPhase::Refused("not allowed".into())));
                    TurnReport {
                        declined: true,
                        ..report("I will not")
                    }
                }
                "ask" => {
                    let grants = turn.session_grants.len();
                    let decision = turn
                        .approvals
                        .ask(pwr_tools::Approval::Publish, "publish the crate")
                        .await;
                    if turn.stop.load(Ordering::Relaxed) {
                        TurnReport {
                            stopped: Some(StopReason::Interrupted),
                            ..report("")
                        }
                    } else {
                        report(&format!("{decision:?} with {grants} grants"))
                    }
                }
                "wait" => {
                    while !turn.stop.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                    let steered = turn.continuity.steering.lock().unwrap().join("; ");
                    TurnReport {
                        stopped: Some(StopReason::Interrupted),
                        ..report(&steered)
                    }
                }
                "compact" => {
                    (turn.steps)(TurnStep::Compacted(
                        "summarised 6 earlier message(s) covering 2 request(s): 900 → 200 tokens"
                            .into(),
                    ));
                    report("compacted")
                }
                other => report(&format!("you said {other}")),
            };
            messages.push(ChatMessage::text("assistant", outcome.answer.clone()));
            Ok((outcome, messages))
        }

        async fn list(&self, root: &Path) -> Result<Vec<Listed>, String> {
            if root.ends_with("unreadable") {
                return Err("the log is unreadable".into());
            }
            Ok((0..SAVED)
                .map(|n| Listed {
                    id: saved(n),
                    updated_at: format!("2026-09-15T10:{n:02}:00Z"),
                    title: Some(format!("request {n}")),
                    messages: 3,
                })
                .collect())
        }

        async fn resume(
            &self,
            _root: &Path,
            id: pwr_domain::Id,
        ) -> Result<Option<Resumed>, String> {
            if id != saved(0) {
                return Ok(None);
            }
            let mut ledger = ChatMessage::text("user", "ledger");
            ledger.purpose = Some(pwr_domain::MessagePurpose::SessionLedger);
            let note = "Changed outside this conversation since it last wrote them: src/lib.rs.";
            Ok(Some(Resumed {
                messages: vec![
                    ChatMessage::text("system", "scripted"),
                    ledger,
                    ChatMessage::text("user", "fix the parser"),
                    ChatMessage::text("assistant", "fixed"),
                    ChatMessage::text("tool", note),
                ],
                checkpoint: pwr_orchestrator::conversation::Checkpoint {
                    changed_files: [("src/lib.rs".to_string(), "abc".to_string())].into(),
                    ..Default::default()
                },
                note: Some(note.into()),
            }))
        }

        fn attach(&self, _root: &Path, attachment: Attachment) -> Result<String, String> {
            match attachment {
                Attachment::File(path) if path.ends_with("missing.txt") => {
                    Err(format!("cannot resolve attachment {}", path.display()))
                }
                Attachment::File(path) => Ok(format!("[file {}]", path.display())),
                Attachment::Embedded { uri, bytes } => {
                    Ok(format!("[{uri}: {}]", String::from_utf8_lossy(&bytes)))
                }
            }
        }

        async fn settings(&self, root: &Path, request: SettingsRequest) -> Result<Value, String> {
            Ok(match request {
                SettingsRequest::Models { selected, .. } => {
                    json!({"root": root, "model": selected.unwrap_or_else(|| "scripted".into())})
                }
                SettingsRequest::Approvals { ask_before, mode } => {
                    json!({"root": root, "changed": ask_before, "mode": mode})
                }
                SettingsRequest::Context { compact_at_percent } => json!({
                    "window": 8192,
                    "model": "scripted",
                    "compactAtPercent": compact_at_percent.unwrap_or(75),
                    "compactAtCustom": compact_at_percent.is_some(),
                }),
            })
        }

        async fn command(
            &self,
            command: Command,
            context: CommandContext,
        ) -> Result<String, String> {
            match command {
                Command::Doctor => Err("the backend is not answering".into()),
                _ => Ok(format!(
                    "{} in {} for {} changed {:?}",
                    command.name(),
                    context.root.display(),
                    context.conversation_id,
                    context.changed_files.keys().collect::<Vec<_>>()
                )),
            }
        }
    }

    struct GoalScripted {
        runs: std::sync::atomic::AtomicUsize,
        verification: GoalVerification,
        /// The last message each turn was given, in order.
        requests: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait(?Send)]
    impl TurnRunner for GoalScripted {
        async fn open(&self, root: &Path) -> Result<Vec<ChatMessage>, String> {
            Scripted.open(root).await
        }

        async fn run(&self, turn: TurnInput) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            let run = self.runs.fetch_add(1, Ordering::Relaxed);
            let mut messages = turn.messages;
            if let Some(last) = messages.last() {
                self.requests.lock().unwrap().push(last.content.clone());
            }
            let mut steps = turn.steps;
            steps(TurnStep::Streaming {
                thinking: "Reading the routes first.".into(),
                content: "Looking at the router.".into(),
            });
            // Every turn numbers its own calls from one, as the real one does.
            steps(TurnStep::ToolCall(converse::ToolCallStep {
                id: 1,
                capability: "read_file".into(),
                detail: "src/lib.rs".into(),
                path: Some("src/lib.rs".into()),
                phase: ToolPhase::Proposed,
                diff: None,
            }));
            let report = if run == 0 {
                TurnReport {
                    answer: "first checkpoint".into(),
                    actions: 26,
                    edited: true,
                    completed: false,
                    stopped: Some(StopReason::BudgetSpent),
                    declined: false,
                }
            } else {
                TurnReport {
                    answer: "implemented and ready".into(),
                    actions: 3,
                    edited: true,
                    completed: true,
                    stopped: None,
                    declined: false,
                }
            };
            messages.push(ChatMessage::text("assistant", report.answer.clone()));
            Ok((report, messages))
        }

        async fn list(&self, root: &Path) -> Result<Vec<Listed>, String> {
            Scripted.list(root).await
        }

        async fn resume(&self, root: &Path, id: pwr_domain::Id) -> Result<Option<Resumed>, String> {
            Scripted.resume(root, id).await
        }

        async fn command(
            &self,
            command: Command,
            context: CommandContext,
        ) -> Result<String, String> {
            Scripted.command(command, context).await
        }

        async fn verify_goal(&self, _context: CommandContext) -> Result<GoalVerification, String> {
            Ok(self.verification.clone())
        }

        fn attach(&self, root: &Path, attachment: Attachment) -> Result<String, String> {
            Scripted.attach(root, attachment)
        }

        async fn settings(&self, root: &Path, request: SettingsRequest) -> Result<Value, String> {
            Scripted.settings(root, request).await
        }
    }

    const SAVED: usize = 52;

    fn saved(n: usize) -> pwr_domain::Id {
        uuid::Uuid::from_u128(0x5eed_0000 + n as u128)
    }

    struct Client {
        to_server: DuplexStream,
        from_server: Lines<BufReader<DuplexStream>>,
        /// The method of each request sent, so its response can be checked
        /// against the schema for that method.
        asked: HashMap<i64, String>,
        /// The turns `_pwr/turn_started` announced, set aside by `receive`:
        /// the tests read what a turn says, not the numbering around it.
        turns_started: Vec<u64>,
    }

    /// The published ACP schema, one validator per type a server sends.
    fn acp_type(name: &str) -> &'static jsonschema::Validator {
        static VALIDATORS: std::sync::OnceLock<
            Mutex<HashMap<String, &'static jsonschema::Validator>>,
        > = std::sync::OnceLock::new();
        static SCHEMA: &str = include_str!("../tests/fixtures/acp/schema-v1.json");
        let validators = VALIDATORS.get_or_init(Mutex::default);
        let mut validators = validators.lock().unwrap();
        validators.entry(name.to_owned()).or_insert_with(|| {
            let mut schema: Value = serde_json::from_str(SCHEMA).unwrap();
            let object = schema.as_object_mut().unwrap();
            assert!(
                object["$defs"].get(name).is_some(),
                "ACP has no type {name}"
            );
            object.remove("anyOf");
            object.insert("$ref".into(), json!(format!("#/$defs/{name}")));
            Box::leak(Box::new(jsonschema::validator_for(&schema).unwrap()))
        })
    }

    /// Panics, naming each violation, unless `instance` is a valid `name`.
    fn conforms(name: &str, instance: &Value, message: &Value) {
        let errors: Vec<String> = acp_type(name)
            .iter_errors(instance)
            .map(|error| format!("{} at {}", error, error.instance_path()))
            .collect();
        assert!(
            errors.is_empty(),
            "not a valid ACP {name}: {errors:#?}\nin {message:#}"
        );
    }

    /// Checks a message the server sent against the schema: a notification or
    /// request by its method, a response by the method of the request it
    /// answers. Extension methods have no ACP schema and are checked as
    /// JSON-RPC only.
    fn check_against_schema(message: &Value, asked: &HashMap<i64, String>) {
        assert_eq!(message["jsonrpc"], "2.0", "{message}");
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            match method {
                "session/update" => conforms("SessionNotification", &message["params"], message),
                "session/request_permission" => {
                    conforms("RequestPermissionRequest", &message["params"], message)
                }
                // PWR's own notifications: `_`-prefixed, as ACP reserves
                // for implementations, and with no ACP schema to check.
                "_pwr/usage"
                | "_pwr/compacted"
                | "_pwr/download_progress"
                | "_pwr/turn_event"
                | "_pwr/turn_started"
                | "_pwr/model_progress"
                | "_pwr/calibration_progress"
                | "_pwr/memory_proposed"
                | "_pwr/wiki_updated"
                | "_pwr/wiki_summarising" => {
                    assert!(
                        message.get("id").is_none(),
                        "an extension notification with an id"
                    );
                }
                other => panic!("the server sent a method a client does not implement: {other}"),
            }
            return;
        }
        if let Some(error) = message.get("error") {
            conforms("Error", error, message);
            return;
        }
        let id = message["id"]
            .as_i64()
            .expect("a response to a request this client sent");
        let method = asked.get(&id).map(String::as_str).unwrap_or_default();
        let response = match method {
            "initialize" => "InitializeResponse",
            "session/new" => "NewSessionResponse",
            "session/load" => "LoadSessionResponse",
            "session/resume" => "ResumeSessionResponse",
            "session/list" => "ListSessionsResponse",
            "session/close" => "CloseSessionResponse",
            "session/prompt" => "PromptResponse",
            extension if extension.starts_with('_') => return,
            other => panic!("a result for {other:?}, which is not an ACP method"),
        };
        conforms(response, &message["result"], message);
    }

    impl Client {
        async fn send(&mut self, message: Value) {
            let mut line = message.to_string();
            line.push('\n');
            self.to_server.write_all(line.as_bytes()).await.unwrap();
        }

        async fn request(&mut self, id: i64, method: &str, params: Value) {
            self.asked.insert(id, method.to_owned());
            self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
                .await;
        }

        async fn receive(&mut self) -> Value {
            let line = tokio::time::timeout(Duration::from_secs(5), self.from_server.next_line())
                .await
                .expect("the server answers")
                .unwrap()
                .expect("the server is still talking");
            let message: Value = serde_json::from_str(&line).unwrap();
            check_against_schema(&message, &self.asked);
            match message["method"].as_str() {
                Some("_pwr/turn_started") => {
                    self.turns_started
                        .push(message["params"]["turn"].as_u64().expect("a turn"));
                    Box::pin(self.receive()).await
                }
                // The wiki is refreshed in the background once a turn ends,
                // so when these arrive depends on timing, not on the turn.
                Some("_pwr/wiki_summarising" | "_pwr/wiki_updated") => {
                    Box::pin(self.receive()).await
                }
                _ => message,
            }
        }

        /// Every message up to and including the response to `id`.
        async fn until_response(&mut self, id: i64) -> Vec<Value> {
            let mut seen = Vec::new();
            loop {
                let message = self.receive().await;
                let done = message.get("method").is_none() && message["id"] == id;
                seen.push(message);
                if done {
                    return seen;
                }
            }
        }

        async fn new_session(&mut self, id: i64) -> String {
            self.request(
                id,
                "session/new",
                json!({"cwd": "/workspace", "mcpServers": []}),
            )
            .await;
            let response = self.receive().await;
            let commands = self.receive().await;
            assert_eq!(
                commands["params"]["update"]["sessionUpdate"],
                "available_commands_update"
            );
            response["result"]["sessionId"]
                .as_str()
                .expect("a session")
                .to_owned()
        }

        async fn prompt(&mut self, id: i64, session: &str, text: &str) {
            self.request(
                id,
                "session/prompt",
                json!({"sessionId": session, "prompt": [{"type": "text", "text": text}]}),
            )
            .await;
        }
    }

    async fn with_server<F, Fut>(test: F)
    where
        F: FnOnce(Client) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        with_runner(Scripted, test).await;
    }

    async fn with_runner<R, F, Fut>(runner: R, test: F)
    where
        R: TurnRunner + 'static,
        F: FnOnce(Client) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let (to_server, server_input) = tokio::io::duplex(1 << 16);
        let (server_output, from_server) = tokio::io::duplex(1 << 16);
        let client = Client {
            to_server,
            from_server: BufReader::new(from_server).lines(),
            asked: HashMap::new(),
            turns_started: Vec::new(),
        };
        tokio::task::LocalSet::new()
            .run_until(async move {
                let server = tokio::task::spawn_local(serve(
                    Rc::new(runner),
                    BufReader::new(server_input),
                    server_output,
                ));
                test(client).await;
                let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
            })
            .await;
    }

    fn updates(messages: &[Value]) -> Vec<&Value> {
        messages
            .iter()
            .filter(|message| message["method"] == "session/update")
            .map(|message| &message["params"]["update"])
            .collect()
    }

    #[tokio::test]
    async fn context_reports_what_fills_the_window_and_takes_a_bounded_threshold() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "hello").await;
            client.until_response(2).await;
            client
                .request(3, "_pwr/context", json!({"sessionId": session}))
                .await;
            let context = client.until_response(3).await.pop().unwrap();
            let context = &context["result"];
            assert_eq!(context["window"], 8192);
            // No engine count from a scripted turn: the estimate, said as one.
            assert_eq!(context["usedSource"], "estimate");
            assert!(
                context["estimateBasis"]
                    .as_str()
                    .unwrap()
                    .contains("not a tokenizer count")
            );
            assert!(context["composition"]["system"].as_u64().unwrap() > 0);
            assert!(context["composition"]["conversation"].as_u64().unwrap() > 0);
            assert_eq!(context["autoCompact"]["thresholdPercent"], 75);
            assert_eq!(context["autoCompact"]["thresholdTokens"], 6144);
            assert_eq!(context["lastCompaction"], Value::Null);

            client
                .request(
                    4,
                    "_pwr/context",
                    json!({"sessionId": session, "autoCompactPercent": 60}),
                )
                .await;
            let changed = client.until_response(4).await.pop().unwrap();
            assert_eq!(changed["result"]["autoCompact"]["thresholdPercent"], 60);
            for (id, bad) in [(5, json!(20)), (6, json!(95)), (7, json!("half"))] {
                client
                    .request(
                        id,
                        "_pwr/context",
                        json!({"sessionId": session, "autoCompactPercent": bad}),
                    )
                    .await;
                let refused = client.until_response(id).await.pop().unwrap();
                assert_eq!(refused["error"]["code"], -32602, "{bad}");
            }
            client
                .request(8, "_pwr/context", json!({"sessionId": "gone"}))
                .await;
            assert_eq!(
                client.until_response(8).await.pop().unwrap()["error"]["code"],
                -32602
            );
        })
        .await;
    }

    #[tokio::test]
    async fn revert_restores_the_file_only_while_it_holds_what_the_model_wrote() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().canonicalize().unwrap();
        std::fs::write(root.join("a.txt"), "model").unwrap();
        std::fs::write(root.join("new.txt"), "created").unwrap();
        let cwd = root.display().to_string();
        with_server(|mut client| async move {
            client
                .request(1, "session/new", json!({"cwd": cwd, "mcpServers": []}))
                .await;
            let session = client.until_response(1).await.pop().unwrap()["result"]["sessionId"]
                .as_str()
                .unwrap()
                .to_owned();

            // Edited by hand after the turn: refused, and left alone.
            std::fs::write(root.join("a.txt"), "model, then a person").unwrap();
            client
                .request(
                    2,
                    "_pwr/revert",
                    json!({"sessionId": session, "path": "a.txt", "expected": "model", "restore": "before"}),
                )
                .await;
            let refused = client.until_response(2).await.pop().unwrap();
            assert!(refused["error"]["message"].as_str().unwrap().contains("has changed"));
            assert_eq!(
                std::fs::read_to_string(root.join("a.txt")).unwrap(),
                "model, then a person"
            );

            // Still the model's version: put back, and recorded.
            std::fs::write(root.join("a.txt"), "model").unwrap();
            client
                .request(
                    3,
                    "_pwr/revert",
                    json!({"sessionId": session, "path": "a.txt", "expected": "model", "restore": "before"}),
                )
                .await;
            let done = client.until_response(3).await.pop().unwrap();
            assert_eq!(done["result"]["reverted"], true, "{done}");
            assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "before");

            // A file the turn created is removed.
            client
                .request(
                    4,
                    "_pwr/revert",
                    json!({"sessionId": session, "path": "new.txt", "expected": "created", "restore": null}),
                )
                .await;
            client.until_response(4).await;
            assert!(!root.join("new.txt").exists());

            // Outside the workspace: refused by the policy.
            client
                .request(
                    5,
                    "_pwr/revert",
                    json!({"sessionId": session, "path": "../outside.txt", "expected": "", "restore": "x"}),
                )
                .await;
            let outside = client.until_response(5).await.pop().unwrap();
            assert!(outside["error"].is_object(), "{outside}");

            let store = pwr_store::Store::open(root.join(".pwr/state.sqlite")).unwrap();
            let conversation = uuid::Uuid::parse_str(&session).unwrap();
            let recorded = store
                .latest_payload(conversation, REVERTED_EVENT)
                .unwrap()
                .expect("the revert was not recorded");
            assert_eq!(recorded["path"], "new.txt");
            assert_eq!(recorded["removed"], true);
        })
        .await;
    }

    #[tokio::test]
    async fn compact_now_folds_the_older_conversation_and_says_so() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            // A fresh conversation has nothing older to fold.
            client
                .request(2, "_pwr/compact", json!({"sessionId": session}))
                .await;
            let nothing = client.until_response(2).await.pop().unwrap();
            assert_eq!(nothing["result"]["compacted"], false);
            assert!(nothing["result"]["reason"].is_string());

            for (id, n) in (3..7).zip(0..) {
                client
                    .prompt(
                        id,
                        &session,
                        &format!("request {n}: {}", "detail ".repeat(300)),
                    )
                    .await;
                client.until_response(id).await;
            }
            client
                .request(10, "_pwr/context", json!({"sessionId": session}))
                .await;
            let before = client.until_response(10).await.pop().unwrap();
            client
                .request(11, "_pwr/compact", json!({"sessionId": session}))
                .await;
            let messages = client.until_response(11).await;
            let notice = messages
                .iter()
                .find(|message| message["method"] == "_pwr/compacted")
                .expect("no compaction notice");
            assert_eq!(notice["params"]["trigger"], "manual");
            let done = &messages.last().unwrap()["result"];
            assert_eq!(done["compacted"], true);
            assert!(
                done["tokensAfter"].as_u64() < done["tokensBefore"].as_u64(),
                "{done}"
            );

            client
                .request(12, "_pwr/context", json!({"sessionId": session}))
                .await;
            let after = client.until_response(12).await.pop().unwrap();
            assert!(
                after["result"]["composition"]["compactedMemory"]
                    .as_u64()
                    .unwrap()
                    > 0
            );
            assert_eq!(
                after["result"]["composition"]["system"], before["result"]["composition"]["system"],
                "the instructions changed"
            );
            assert!(
                after["result"]["estimatedTokens"].as_u64()
                    < before["result"]["estimatedTokens"].as_u64()
            );
            // The next turn goes on from the compacted conversation.
            client.prompt(13, &session, "carry on").await;
            let reply = client.until_response(13).await;
            assert!(
                updates(&reply)
                    .iter()
                    .any(|u| u["content"]["text"] == "you said carry on")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn an_automatic_compaction_reaches_the_client() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "compact").await;
            let messages = client.until_response(2).await;
            let notice = messages
                .iter()
                .find(|message| message["method"] == "_pwr/compacted")
                .expect("the compaction was not forwarded");
            assert_eq!(notice["params"]["trigger"], "automatic");
            assert_eq!(notice["params"]["sessionId"], session.as_str());
            assert!(
                notice["params"]["note"]
                    .as_str()
                    .unwrap()
                    .contains("900 → 200")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn deleting_a_model_names_what_to_delete_and_says_why_it_cannot() {
        with_server(|mut client| async move {
            client
                .request(
                    1,
                    "_pwr/model_delete",
                    json!({"cwd": "/workspace", "modelRef": "a/b"}),
                )
                .await;
            assert_eq!(
                client.until_response(1).await.pop().unwrap()["error"]["code"],
                -32602
            );
            client
                .request(
                    2,
                    "_pwr/model_delete",
                    json!({"modelRef": "a/b", "format": "mlx"}),
                )
                .await;
            assert_eq!(
                client.until_response(2).await.pop().unwrap()["error"]["code"],
                -32602
            );
            client
                .request(
                    3,
                    "_pwr/model_delete",
                    json!({"cwd": "/workspace", "modelRef": "a/b", "format": "mlx"}),
                )
                .await;
            let refused = client.until_response(3).await.pop().unwrap();
            assert_eq!(refused["error"]["code"], -32000);
            assert!(
                refused["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("not available")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn sampling_profile_request_requires_a_model_and_numeric_value_object() {
        with_server(|mut client| async move {
            for (id, params) in [
                (1, json!({"cwd": "/workspace"})),
                (
                    2,
                    json!({"cwd": "/workspace", "modelRef": "a/b", "values": []}),
                ),
                (
                    3,
                    json!({"modelRef": "a/b", "values": {"temperature": 0.7}}),
                ),
            ] {
                client.request(id, "_pwr/model_sampling", params).await;
                assert_eq!(
                    client.until_response(id).await.pop().unwrap()["error"]["code"],
                    -32602
                );
            }
        })
        .await;
    }

    #[tokio::test]
    async fn a_hub_download_names_everything_the_core_must_re_read() {
        with_server(|mut client| async move {
            client
                .request(
                    1,
                    "_pwr/download",
                    json!({"cwd": "/workspace", "downloadId": "d1", "repository": "a/b", "variant": "mlx"}),
                )
                .await;
            let refused = client.until_response(1).await.pop().unwrap();
            assert_eq!(refused["error"]["code"], -32602);
            client
                .request(
                    2,
                    "_pwr/download",
                    json!({"cwd": "/workspace", "downloadId": "d2", "repository": "a/b",
                           "revision": "0123456789abcdef0123456789abcdef01234567",
                           "variant": "mlx", "format": "mlx"}),
                )
                .await;
            let messages = client.until_response(2).await;
            // The scripted runner cannot download; the state still ends, and
            // the failure carries its kind.
            let last_state = messages
                .iter()
                .rfind(|message| message["method"] == "_pwr/download_progress")
                .expect("no download state");
            assert_eq!(last_state["params"]["state"]["state"], "failed");
            assert_eq!(messages.last().unwrap()["error"]["data"]["kind"], "io");
        })
        .await;
    }

    #[tokio::test]
    async fn initialize_names_the_version_and_declines_what_is_not_there() {
        with_server(|mut client| async move {
            client
                .request(
                    0,
                    "initialize",
                    json!({"protocolVersion": 1, "clientCapabilities": {}}),
                )
                .await;
            let response = client.receive().await;
            assert_eq!(response["id"], 0);
            assert_eq!(response["result"]["protocolVersion"], PROTOCOL_VERSION);
            assert_eq!(response["result"]["agentCapabilities"]["loadSession"], true);
            assert_eq!(response["result"]["agentInfo"]["name"], "pwr");
        })
        .await;
    }

    #[tokio::test]
    async fn a_workspace_that_cannot_hold_a_session_says_why() {
        with_server(|mut client| async move {
            client
                .request(1, "session/new", json!({"cwd": "/unprepared"}))
                .await;
            let response = client.receive().await;
            assert_eq!(
                response["error"]["message"],
                "the chosen model is not prepared"
            );
            client.request(2, "session/new", json!({})).await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
        })
        .await;
    }

    #[tokio::test]
    async fn a_turn_streams_its_edit_as_a_tool_call_with_a_diff_and_then_answers() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "edit").await;
            let messages = client.until_response(2).await;
            let updates = updates(&messages);
            assert_eq!(updates.len(), 4, "{messages:#?}");
            assert_eq!(updates[0]["sessionUpdate"], "tool_call");
            assert_eq!(updates[0]["kind"], "edit");
            assert_eq!(updates[0]["status"], "pending");
            assert_eq!(updates[0]["locations"][0]["path"], "/workspace/src/lib.rs");
            assert_eq!(updates[1]["sessionUpdate"], "tool_call_update");
            assert_eq!(updates[1]["status"], "in_progress");
            assert_eq!(updates[2]["sessionUpdate"], "tool_call_update");
            assert_eq!(updates[2]["toolCallId"], updates[0]["toolCallId"]);
            assert_eq!(updates[2]["status"], "completed");
            assert_eq!(
                updates[2]["content"][0],
                json!({"type": "diff", "path": "/workspace/src/lib.rs", "oldText": "a", "newText": "b"})
            );
            assert_eq!(updates[3]["sessionUpdate"], "agent_message_chunk");
            assert_eq!(updates[3]["content"]["text"], "edited");
            let response = messages.last().unwrap();
            assert_eq!(response["result"]["stopReason"], "end_turn");
            assert_eq!(response["result"]["_meta"]["pwr"]["actions"], 1);
            assert_eq!(response["result"]["_meta"]["pwr"]["edited"], true);
        })
        .await;
    }

    #[tokio::test]
    async fn goal_mode_continues_past_a_checkpoint_and_requires_full_verification() {
        with_runner(
            GoalScripted {
                runs: std::sync::atomic::AtomicUsize::new(0),
                requests: Default::default(),
                verification: GoalVerification {
                    passed: true,
                    technical_passed: true,
                    acceptance_available: true,
                    summary: "1 of 1 full check(s) passing:\n  · npm run build".into(),
                    ..GoalVerification::default()
                },
            },
            |mut client| async move {
                let session = client.new_session(1).await;
                client
                    .request(
                        2,
                        "session/prompt",
                        json!({
                            "sessionId": session,
                            "goalMode": true,
                            "prompt": [{"type": "text", "text": "Build the site"}],
                        }),
                    )
                    .await;
                let messages = client.until_response(2).await;
                assert!(updates(&messages).iter().any(|update| {
                    update["content"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("Checkpoint after 26"))
                }));
                let response = messages.last().unwrap();
                assert_eq!(response["result"]["_meta"]["pwr"]["goal"]["verified"], true);
                assert_eq!(response["result"]["_meta"]["pwr"]["totalActions"], 29);
                // Two turns under one prompt, each with its own first call:
                // two distinct actions, so two distinct ids.
                let ids: Vec<&str> = updates(&messages)
                    .iter()
                    .filter(|update| update["sessionUpdate"] == "tool_call")
                    .filter_map(|update| update["toolCallId"].as_str())
                    .collect();
                assert_eq!(ids, ["turn1-call1", "turn1-call2"], "{messages:#?}");
                // A reply streams while it is generated: reasoning as a
                // thought, text as a message chunk marked live.
                assert!(updates(&messages).iter().any(|update| {
                    update["sessionUpdate"] == "agent_thought_chunk"
                        && update["content"]["text"] == "Reading the routes first."
                }));
                assert!(updates(&messages).iter().any(|update| {
                    update["sessionUpdate"] == "agent_message_chunk"
                        && update["_meta"]["pwr"]["live"] == true
                        && update["content"]["text"] == "Looking at the router."
                }));
                assert!(updates(&messages).iter().any(|update| {
                    update["content"]["text"].as_str().is_some_and(|text| {
                        text.contains("Goal verified by declared acceptance checks")
                    })
                }));
            },
        )
        .await;
    }

    #[tokio::test]
    async fn a_check_failing_before_the_goal_is_named_to_it_and_does_not_hold_it_open() {
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let runner = GoalScripted {
            runs: std::sync::atomic::AtomicUsize::new(0),
            requests: Default::default(),
            verification: GoalVerification {
                passed: false,
                technical_passed: false,
                acceptance_available: false,
                summary: "1 of 2 full check(s) passing:\n  · npm run build\n  ✗ npm test".into(),
                failing: vec!["npm test".into()],
            },
        };
        let runner = GoalScripted {
            requests: Arc::clone(&requests),
            ..runner
        };
        with_runner(runner, |mut client| async move {
            let session = client.new_session(1).await;
            client
                .request(
                    2,
                    "session/prompt",
                    json!({
                        "sessionId": session,
                        "goalMode": true,
                        "prompt": [{"type": "text", "text": "The page shows no text"}],
                    }),
                )
                .await;
            let messages = client.until_response(2).await;
            assert!(updates(&messages).iter().any(|update| {
                update["content"]["text"]
                    .as_str()
                    .is_some_and(|text| text.contains("already failing before this goal started"))
            }));
        })
        .await;
        // Two turns -- the checkpoint and the completion -- and no third one
        // sent to repair `npm test`.
        let requests = requests.lock().unwrap().clone();
        assert_eq!(requests.len(), 2, "{requests:?}");
        let first = &requests[0];
        assert!(first.contains("already failing: npm test"), "{first}");
        assert!(first.contains("Do not repair them"), "{first}");
    }

    #[tokio::test]
    async fn goal_mode_never_calls_technical_checks_alone_a_verified_goal() {
        with_runner(
            GoalScripted {
                runs: std::sync::atomic::AtomicUsize::new(0),
                requests: Default::default(),
                verification: GoalVerification {
                    passed: false,
                    technical_passed: true,
                    acceptance_available: false,
                    summary: "2 of 2 full check(s) passing:\n  · npm run build\n  · npm test".into(),
                    ..GoalVerification::default()
                },
            },
            |mut client| async move {
                let session = client.new_session(1).await;
                client
                    .request(
                        2,
                        "session/prompt",
                        json!({
                            "sessionId": session,
                            "goalMode": true,
                            "prompt": [{"type": "text", "text": "Build the site"}],
                        }),
                    )
                    .await;
                let messages = client.until_response(2).await;
                let response = messages.last().unwrap();
                let goal = &response["result"]["_meta"]["pwr"]["goal"];
                assert_eq!(goal["verified"], false);
                assert_eq!(goal["technicalPassed"], true);
                assert_eq!(goal["acceptanceAvailable"], false);
                assert_eq!(goal["needsAcceptance"], true);
                assert!(updates(&messages).iter().any(|update| {
                    update["content"]["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("goal is not verified because this workspace has no declared acceptance check"))
                }));
            },
        )
        .await;
    }

    #[test]
    fn an_acceptance_contract_must_preexist_and_stay_unchanged() {
        let root = tempfile::tempdir().unwrap();
        assert!(acceptance_contract_hash(root.path()).is_none());
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        let checks = root.path().join(".pwr/checks.json");
        std::fs::write(
            &checks,
            r#"{"checks":[{"executable":"npm","args":["run","test:e2e"],"kind":"acceptance"}]}"#,
        )
        .unwrap();
        let initial = acceptance_contract_hash(root.path()).unwrap();
        std::fs::write(
            &checks,
            r#"{"checks":[{"executable":"npm","args":["run","test:smoke"],"kind":"acceptance"}]}"#,
        )
        .unwrap();
        assert_ne!(acceptance_contract_hash(root.path()).unwrap(), initial);
    }

    #[tokio::test]
    async fn a_declined_request_ends_as_a_refusal_and_a_refused_action_is_announced_failed() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "refused").await;
            let messages = client.until_response(2).await;
            let updates = updates(&messages);
            assert_eq!(updates[0]["sessionUpdate"], "tool_call");
            assert_eq!(updates[0]["kind"], "execute");
            assert_eq!(updates[0]["status"], "pending");
            assert_eq!(updates[1]["sessionUpdate"], "tool_call_update");
            assert_eq!(updates[1]["toolCallId"], updates[0]["toolCallId"]);
            assert_eq!(updates[1]["status"], "failed");
            assert_eq!(updates[1]["content"][0]["content"]["text"], "not allowed");
            let response = messages.last().unwrap();
            assert_eq!(response["result"]["stopReason"], "refusal");
            assert_eq!(response["result"]["_meta"]["pwr"]["terminal"], "declined");
        })
        .await;
    }

    #[tokio::test]
    async fn the_conversation_carries_from_one_turn_to_the_next() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "hello").await;
            let first = client.until_response(2).await;
            assert_eq!(updates(&first)[0]["content"]["text"], "you said hello");
            client.prompt(3, &session, "again").await;
            let second = client.until_response(3).await;
            assert_eq!(updates(&second)[0]["content"]["text"], "you said again");
            // Each message the person sends is numbered, for rewinding to it.
            assert_eq!(client.turns_started, [1, 2]);
        })
        .await;
    }

    #[tokio::test]
    async fn an_approval_is_asked_of_the_client_and_allow_always_holds_for_the_session() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "ask").await;
            let question = client.receive().await;
            assert_eq!(question["method"], "session/request_permission");
            assert_eq!(question["params"]["sessionId"], session.as_str());
            assert_eq!(question["params"]["_meta"]["pwr"]["approval"], json!(pwr_tools::Approval::Publish));
            let kinds: Vec<&str> = question["params"]["options"]
                .as_array()
                .unwrap()
                .iter()
                .map(|option| option["kind"].as_str().unwrap())
                .collect();
            assert_eq!(kinds, ["allow_once", "allow_always", "reject_once"]);
            client
                .send(json!({"jsonrpc": "2.0", "id": question["id"], "result": {"outcome": {"outcome": "selected", "optionId": "allow_always"}}}))
                .await;
            let messages = client.until_response(2).await;
            assert_eq!(updates(&messages)[0]["content"]["text"], "AllowForRun with 0 grants");

            client.prompt(3, &session, "ask").await;
            let question = client.receive().await;
            client
                .send(json!({"jsonrpc": "2.0", "id": question["id"], "result": {"outcome": {"outcome": "selected", "optionId": "reject_once"}}}))
                .await;
            let messages = client.until_response(3).await;
            assert_eq!(updates(&messages)[0]["content"]["text"], "Deny with 1 grants");
        })
        .await;
    }

    #[tokio::test]
    async fn cancelling_answers_an_open_question_and_ends_the_turn_cancelled() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "ask").await;
            let question = client.receive().await;
            assert_eq!(question["method"], "session/request_permission");
            client
                .send(json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": session}}))
                .await;
            let messages = client.until_response(2).await;
            let response = messages.last().unwrap();
            assert_eq!(response["result"]["stopReason"], "cancelled");
            assert_eq!(response["result"]["_meta"]["pwr"]["terminal"], "interrupted");
        })
        .await;
    }

    #[tokio::test]
    async fn a_busy_session_refuses_a_second_prompt_and_cancel_frees_it() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "wait").await;
            client.prompt(3, &session, "hello").await;
            let refused = client.receive().await;
            assert_eq!(refused["id"], 3);
            assert_eq!(refused["error"]["code"], -32000);
            client
                .send(json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": session}}))
                .await;
            let messages = client.until_response(2).await;
            assert_eq!(messages.last().unwrap()["result"]["stopReason"], "cancelled");
            client.prompt(4, &session, "hello").await;
            let messages = client.until_response(4).await;
            assert_eq!(messages.last().unwrap()["result"]["stopReason"], "end_turn");
        })
        .await;
    }

    #[tokio::test]
    async fn unknown_methods_and_unknown_sessions_are_errors_not_silence() {
        with_server(|mut client| async move {
            client
                .request(1, "session/set_mode", json!({"sessionId": "x"}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32601);
            client
                .send(json!({"jsonrpc": "2.0", "method": "pwr/unknown"}))
                .await;
            client.to_server.write_all(b"not json\n").await.unwrap();
            assert_eq!(client.receive().await["error"]["code"], -32700);
            client.prompt(2, "missing", "hello").await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
        })
        .await;
    }

    #[tokio::test]
    async fn loading_replays_what_was_said_and_then_what_changed_since() {
        with_server(|mut client| async move {
            let session = saved(0).to_string();
            client
                .request(
                    1,
                    "session/load",
                    json!({"sessionId": session, "cwd": "/workspace", "mcpServers": []}),
                )
                .await;
            let messages = client.until_response(1).await;
            let shown: Vec<(&str, &str)> = updates(&messages)
                .into_iter()
                .map(|update| {
                    (
                        update["sessionUpdate"].as_str().unwrap(),
                        update["content"]["text"].as_str().unwrap(),
                    )
                })
                .collect();
            assert_eq!(
                shown,
                [
                    ("user_message_chunk", "fix the parser"),
                    ("agent_message_chunk", "fixed"),
                    (
                        "agent_message_chunk",
                        "Changed outside this conversation since it last wrote them: src/lib.rs."
                    ),
                ]
            );
            assert_eq!(messages.last().unwrap()["result"], json!({}));
            assert_eq!(
                client.receive().await["params"]["update"]["sessionUpdate"],
                "available_commands_update"
            );

            // The loaded session continues where it stopped: the next turn
            // runs in it, and its checkpoint is what /changes reads.
            client.prompt(2, &session, "/changes").await;
            let messages = client.until_response(2).await;
            assert_eq!(
                updates(&messages)[0]["content"]["text"],
                format!("changes in /workspace for {session} changed [\"src/lib.rs\"]")
            );
            client.prompt(3, &session, "hello").await;
            let messages = client.until_response(3).await;
            assert_eq!(messages.last().unwrap()["result"]["stopReason"], "end_turn");
        })
        .await;
    }

    #[tokio::test]
    async fn resuming_skips_the_replay_but_not_what_changed() {
        with_server(|mut client| async move {
            let session = saved(0).to_string();
            client
                .request(
                    1,
                    "session/resume",
                    json!({"sessionId": session, "cwd": "/workspace"}),
                )
                .await;
            let messages = client.until_response(1).await;
            let updates = updates(&messages);
            assert_eq!(updates.len(), 1, "{messages:#?}");
            assert!(
                updates[0]["content"]["text"]
                    .as_str()
                    .unwrap()
                    .starts_with("Changed outside")
            );
        })
        .await;
    }

    #[tokio::test]
    async fn a_session_that_cannot_be_loaded_says_which_way() {
        with_server(|mut client| async move {
            let missing = saved(9).to_string();
            client
                .request(
                    1,
                    "session/load",
                    json!({"sessionId": missing, "cwd": "/workspace"}),
                )
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32002);
            client
                .request(
                    2,
                    "session/load",
                    json!({"sessionId": "not-an-id", "cwd": "/workspace"}),
                )
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .request(3, "session/resume", json!({"sessionId": missing}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
        })
        .await;
    }

    #[tokio::test]
    async fn listing_pages_the_workspace_sessions() {
        with_server(|mut client| async move {
            client
                .request(1, "session/list", json!({"cwd": "/workspace"}))
                .await;
            let first = client.receive().await;
            let sessions = first["result"]["sessions"].as_array().unwrap();
            assert_eq!(sessions.len(), LIST_PAGE);
            assert_eq!(sessions[0]["sessionId"], saved(0).to_string());
            assert_eq!(sessions[0]["cwd"], "/workspace");
            assert_eq!(sessions[0]["title"], "request 0");
            assert_eq!(sessions[0]["updatedAt"], "2026-09-15T10:00:00Z");
            assert_eq!(first["result"]["nextCursor"], "50");

            client
                .request(
                    2,
                    "session/list",
                    json!({"cwd": "/workspace", "cursor": "50"}),
                )
                .await;
            let last = client.receive().await;
            assert_eq!(
                last["result"]["sessions"].as_array().unwrap().len(),
                SAVED - LIST_PAGE
            );
            assert!(last["result"].get("nextCursor").is_none());

            client.request(3, "session/list", json!({})).await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .request(
                    4,
                    "session/list",
                    json!({"cwd": "/workspace", "cursor": "later"}),
                )
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .request(5, "session/list", json!({"cwd": "/unreadable"}))
                .await;
            assert_eq!(
                client.receive().await["error"]["message"],
                "the log is unreadable"
            );
        })
        .await;
    }

    #[tokio::test]
    async fn a_closed_session_is_gone_and_its_turn_is_stopped() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client.prompt(2, &session, "wait").await;
            client
                .request(3, "session/close", json!({"sessionId": session}))
                .await;
            let messages = client.until_response(2).await;
            assert!(
                messages
                    .iter()
                    .any(|message| message["id"] == 3 && message["result"] == json!({}))
            );
            assert_eq!(
                messages.last().unwrap()["result"]["stopReason"],
                "cancelled"
            );
            client.prompt(4, &session, "hello").await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .request(5, "session/close", json!({"sessionId": session}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
        })
        .await;
    }

    #[tokio::test]
    async fn steering_reaches_a_running_turn_and_is_refused_between_turns() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client
                .request(2, "_pwr/steer", json!({"sessionId": session, "text": "not that file"}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32000);

            client.prompt(3, &session, "wait").await;
            client
                .request(4, "_pwr/steer", json!({"sessionId": session, "text": "not that file"}))
                .await;
            assert_eq!(client.receive().await["result"], json!({}));
            client
                .request(5, "_pwr/steer", json!({"sessionId": session, "text": " "}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .send(json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": session}}))
                .await;
            let messages = client.until_response(3).await;
            let said = updates(&messages)[0]["content"]["text"].as_str().unwrap().to_owned();
            assert!(said.starts_with("not that file"), "{said}");
        })
        .await;
    }

    #[tokio::test]
    async fn the_console_commands_run_as_extensions_and_as_slash_prompts() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            client
                .request(2, "_pwr/verify", json!({"sessionId": session}))
                .await;
            let verified = client.receive().await;
            assert!(
                verified["result"]["text"]
                    .as_str()
                    .unwrap()
                    .starts_with("verify in /workspace")
            );
            client
                .request(3, "_pwr/doctor", json!({"sessionId": session}))
                .await;
            assert_eq!(
                client.receive().await["error"]["message"],
                "the backend is not answering"
            );
            client
                .request(4, "_pwr/verify", json!({"sessionId": "gone"}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client
                .request(5, "_pwr/nothing", json!({"sessionId": session}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32601);

            client.prompt(6, &session, "/doctor").await;
            let messages = client.until_response(6).await;
            assert_eq!(
                updates(&messages)[0]["content"]["text"],
                "could not run /doctor: the backend is not answering"
            );
            assert_eq!(messages.last().unwrap()["result"]["stopReason"], "end_turn");
            // A slash the console does not know is the deployment's to read.
            client.prompt(7, &session, "/refactor").await;
            let messages = client.until_response(7).await;
            assert_eq!(
                updates(&messages)[0]["content"]["text"],
                "you said /refactor"
            );
        })
        .await;
    }

    #[test]
    fn a_replay_shows_the_conversation_and_not_the_harness_context() {
        let mut excerpts = ChatMessage::text("user", "excerpts");
        excerpts.purpose = Some(pwr_domain::MessagePurpose::RepositoryExcerpts);
        let mut task = ChatMessage::text("user", "the task");
        task.purpose = Some(pwr_domain::MessagePurpose::Task);
        let shown = replay(&[
            ChatMessage::text("system", "prompt"),
            excerpts,
            task,
            ChatMessage::text("assistant", ""),
            ChatMessage::text("tool", "result"),
            ChatMessage::text("assistant", "done"),
        ]);
        assert_eq!(
            shown,
            [
                message_chunk("user_message_chunk", "the task"),
                message_chunk("agent_message_chunk", "done"),
            ]
        );
    }

    #[test]
    fn turn_events_carry_retries_and_what_a_generation_cost() {
        let retry = turn_event(&TurnStep::Retry {
            cause: "reply_fault",
            attempt: 1,
            limit: 2,
            detail: "tool calls were not valid JSON".into(),
        })
        .unwrap();
        assert_eq!(retry["event"], "retry");
        assert_eq!(retry["cause"], "reply_fault");
        assert_eq!(retry["attempt"], 1);
        let generation = turn_event(&TurnStep::Generation(converse::GenerationStats {
            metrics: Some(pwr_domain::GenerationMetrics {
                prompt_tokens: Some(1200),
                generated_tokens: Some(300),
                generation_duration_ns: Some(6_000_000_000),
                ..Default::default()
            }),
            elapsed: std::time::Duration::from_millis(6500),
            first_chunk: None,
        }))
        .unwrap();
        assert_eq!(generation["generatedTokens"], 300);
        assert_eq!(generation["generationMs"], 6000);
        assert_eq!(generation["elapsedMs"], 6500);
        // Not measured is null, never zero.
        assert!(generation["firstChunkMs"].is_null());
        assert!(generation["promptEvalMs"].is_null());
        // What ACP already carries is not repeated.
        assert!(
            turn_event(&TurnStep::Usage {
                used: 1,
                window: 2,
                estimated: false,
            })
            .is_none()
        );
    }

    #[test]
    fn the_schema_check_refuses_what_acp_does_not_define() {
        let valid = |name: &str, instance: Value| acp_type(name).is_valid(&instance);
        let update = |update: Value| json!({"sessionId": "s", "update": update});
        assert!(valid(
            "SessionNotification",
            update(
                tool_update(
                    Path::new("/w"),
                    1,
                    TurnStep::ToolCall(ToolCallStep {
                        id: 1,
                        capability: "read_file".into(),
                        detail: "a".into(),
                        path: Some("a".into()),
                        phase: ToolPhase::Started,
                        diff: None,
                    })
                )
                .unwrap()
            )
        ));
        assert!(!valid(
            "SessionNotification",
            update(
                json!({"sessionUpdate": "tool_call", "toolCallId": "t", "title": "x", "kind": "write"})
            )
        ));
        assert!(!valid(
            "SessionNotification",
            update(json!({"sessionUpdate": "agent_message"}))
        ));
        assert!(!valid("PromptResponse", json!({"stopReason": "stopped"})));
        assert!(!valid("NewSessionResponse", json!({})));
        assert!(valid(
            "PromptResponse",
            json!({"stopReason": "max_turn_requests"})
        ));
    }

    #[tokio::test]
    async fn settings_are_read_and_changed_for_a_workspace_or_a_session_in_it() {
        with_server(|mut client| async move {
            client
                .request(1, "_pwr/models", json!({"cwd": "/elsewhere"}))
                .await;
            assert_eq!(client.receive().await["result"]["root"], "/elsewhere");
            let session = client.new_session(2).await;
            client
                .request(3, "_pwr/models", json!({"sessionId": session}))
                .await;
            assert_eq!(client.receive().await["result"]["root"], "/workspace");
            client
                .request(
                    31,
                    "_pwr/models",
                    json!({"cwd": "/elsewhere", "model": "fixture.gguf"}),
                )
                .await;
            assert_eq!(client.receive().await["result"]["model"], "fixture.gguf");
            client
                .request(32, "_pwr/models", json!({"cwd": "/elsewhere", "model": ""}))
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
            client.request(4, "_pwr/models", json!({})).await;
            assert_eq!(client.receive().await["error"]["code"], -32602);

            client
                .request(5, "_pwr/approvals", json!({"sessionId": session}))
                .await;
            assert_eq!(client.receive().await["result"]["changed"], Value::Null);
            client
                .request(
                    6,
                    "_pwr/approvals",
                    json!({"sessionId": session, "askBefore": ["publish", "network_access"]}),
                )
                .await;
            assert_eq!(
                client.receive().await["result"]["changed"],
                json!(["publish", "network_access"])
            );
            client
                .request(
                    7,
                    "_pwr/approvals",
                    json!({"sessionId": session, "askBefore": ["everything"]}),
                )
                .await;
            assert_eq!(client.receive().await["error"]["code"], -32602);
        })
        .await;
    }

    #[test]
    fn an_image_block_is_decoded_beside_the_text() {
        let (text, attachments, images) = prompt_parts(&json!({"prompt": [
            {"type": "text", "text": "what is wrong here?"},
            {"type": "image", "data": "iVBORw==", "mimeType": "image/png"},
        ]}))
        .unwrap();
        assert_eq!(text, "what is wrong here?");
        assert!(attachments.is_empty());
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].extension(), Some("png"));
        assert_eq!(images[0].bytes, [0x89, b'P', b'N', b'G']);
    }

    #[tokio::test]
    async fn attachments_reach_the_turn_in_the_consoles_form_or_refuse_the_prompt() {
        with_server(|mut client| async move {
            let session = client.new_session(1).await;
            let prompt = |blocks: Value| json!({"sessionId": session, "prompt": blocks});
            client
                .request(
                    2,
                    "session/prompt",
                    prompt(json!([
                        {"type": "text", "text": "summarise"},
                        {"type": "resource_link", "uri": "file:///notes/My%20Plan.md", "name": "My Plan.md"},
                        {"type": "resource", "resource": {"uri": "zed:///buffer/3", "text": "unsaved"}},
                        {"type": "resource", "resource": {"uri": "file:///a.pdf", "blob": "JVBERi0=", "mimeType": "application/pdf"}},
                    ])),
                )
                .await;
            let messages = client.until_response(2).await;
            assert_eq!(
                updates(&messages)[0]["content"]["text"],
                "you said summarise\n\n--- Attachments for this task only ---\n\
                 [file /notes/My Plan.md]\n\n[zed:///buffer/3: unsaved]\n\n[file:///a.pdf: %PDF-]"
            );

            for (id, blocks, refused) in [
                (3, json!([{"type": "resource_link", "uri": "file:///missing.txt", "name": "missing.txt"}]), "attachment refused: cannot resolve"),
                (4, json!([{"type": "resource_link", "uri": "https://example.com/a.md", "name": "a.md"}]), "only files on this machine"),
                (5, json!([{"type": "image", "data": "AAAA", "mimeType": "image/png"}]), "image refused: the chosen model cannot read images"),
                (8, json!([{"type": "image", "data": "AAAA", "mimeType": "image/tiff"}]), "images are not supported"),
                (9, json!([{"type": "audio", "data": "AAAA", "mimeType": "audio/wav"}]), "audio content is not supported"),
                (6, json!([{"type": "resource", "resource": {"uri": "file:///b", "blob": "%%%"}}]), "not valid base64"),
            ] {
                client.request(id, "session/prompt", prompt(blocks)).await;
                let response = client.receive().await;
                let message = response["error"]["message"].as_str().unwrap_or_default();
                assert!(message.contains(refused), "{response}");
            }
            // A refused prompt left the session free.
            client.prompt(7, &session, "hello").await;
            let messages = client.until_response(7).await;
            assert_eq!(messages.last().unwrap()["result"]["stopReason"], "end_turn");
        })
        .await;
    }

    /// The console's turn itself, `converse::take_turn`, driven by a scripted
    /// deployment in a real workspace -- what `serve` does with a model, less
    /// the model.
    struct RealTurn {
        provider: crate::two_loops::Scripted,
        store: pwr_store::Store,
    }

    #[async_trait::async_trait(?Send)]
    impl TurnRunner for RealTurn {
        async fn open(&self, _root: &Path) -> Result<Vec<ChatMessage>, String> {
            Ok(Vec::new())
        }

        async fn run(&self, mut turn: TurnInput) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            let mut messages = turn.messages;
            // What the console grants: everything except what Settings ask
            // about, which is dependency changes here.
            let policy = pwr_tools::ToolPolicy {
                approvals: crate::chat_approvals(
                    &[pwr_tools::Approval::DependencyChange],
                    &turn.session_grants,
                ),
                ..crate::two_loops::policy_for(&turn.root)
            };
            let report = converse::take_turn(
                &self.provider,
                pwr_compat::adapter_for(None, "fake").as_ref(),
                &crate::two_loops::deployment(),
                &self.store,
                turn.conversation_id,
                &policy,
                &mut messages,
                8192,
                &[],
                Default::default(),
                pwr_compat::render_tools(&converse::chat_tool_catalog()),
                &turn.stop,
                &turn.continuity,
                turn.approvals.as_ref(),
                |step| {
                    (turn.steps)(step);
                },
            )
            .await?;
            Ok((report, messages))
        }

        async fn list(&self, _root: &Path) -> Result<Vec<Listed>, String> {
            unreachable!("not listed in this transcript")
        }

        async fn resume(
            &self,
            _root: &Path,
            _id: pwr_domain::Id,
        ) -> Result<Option<Resumed>, String> {
            unreachable!("not resumed in this transcript")
        }

        async fn command(
            &self,
            _command: Command,
            _context: CommandContext,
        ) -> Result<String, String> {
            unreachable!("no command in this transcript")
        }

        async fn settings(&self, _root: &Path, _request: SettingsRequest) -> Result<Value, String> {
            unreachable!("no settings in this transcript")
        }

        fn attach(&self, _root: &Path, _attachment: Attachment) -> Result<String, String> {
            unreachable!("no attachment in this transcript")
        }
    }

    /// A golden transcript: every line the server sent, in order, with the
    /// session id and the workspace path replaced by placeholders. Compared
    /// byte for byte, so any change to what a client sees is a visible diff in
    /// review. `PWR_UPDATE_GOLDEN=1` rewrites the fixture.
    #[tokio::test]
    async fn golden_an_edit_a_permission_granted_and_one_refused() {
        use crate::two_loops::{calls, says};
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path().to_path_buf();
        std::fs::write(root.join("code.rs"), "one\n").unwrap();
        std::fs::write(root.join("Cargo.toml"), "[package]\n").unwrap();
        let manifest_edit = |from: &str, to: &str| {
            calls(
                "replace_text",
                json!({
                    "path": "Cargo.toml",
                    "expected_hash": pwr_domain::hash_bytes(from.as_bytes()),
                    "find": from.trim_end(),
                    "replace": to.trim_end(),
                }),
            )
        };
        let runner = RealTurn {
            provider: crate::two_loops::Scripted::new(vec![
                calls(
                    "replace_text",
                    json!({
                        "path": "code.rs",
                        "expected_hash": pwr_domain::hash_bytes(b"one\n"),
                        "find": "one",
                        "replace": "two",
                    }),
                ),
                manifest_edit("[package]\n", "[package]\nx = 1\n"),
                says("Renamed it and set x."),
                manifest_edit("[package]\nx = 1\n", "[package]\nx = 2\n"),
                says("Left x alone, as you refused."),
            ]),
            store: pwr_store::Store::open(":memory:").unwrap(),
        };
        let cwd = root.display().to_string();
        let mut transcript: Vec<Value> = Vec::new();
        with_runner(runner, |mut client| {
            let transcript = &mut transcript;
            let cwd = cwd.clone();
            async move {
                client
                    .request(1, "session/new", json!({"cwd": cwd, "mcpServers": []}))
                    .await;
                let created = client.receive().await;
                let session = created["result"]["sessionId"].as_str().unwrap().to_owned();
                transcript.push(created);
                transcript.push(client.receive().await);

                for (id, text, answer) in [
                    (2, "rename and set x", "allow_once"),
                    (3, "set x to 2", "reject_once"),
                ] {
                    client.prompt(id, &session, text).await;
                    loop {
                        let message = client.receive().await;
                        let done = message.get("method").is_none() && message["id"] == id;
                        if message["method"] == "session/request_permission" {
                            let reply = json!({
                                "jsonrpc": "2.0",
                                "id": message["id"],
                                "result": {"outcome": {"outcome": "selected", "optionId": answer}},
                            });
                            transcript.push(message);
                            client.send(reply).await;
                            continue;
                        }
                        transcript.push(message);
                        if done {
                            break;
                        }
                    }
                }
                let normalized: Vec<String> = transcript
                    .iter()
                    .map(|message| {
                        // Wall-clock timings differ run to run; their presence
                        // is the contract, not their value.
                        let mut message = message.clone();
                        if message["method"] == "_pwr/turn_event" {
                            for timing in ["elapsedMs", "firstChunkMs"] {
                                if message["params"][timing].is_u64() {
                                    message["params"][timing] = json!("<ms>");
                                }
                            }
                        }
                        message
                            .to_string()
                            .replace(&session, "<session>")
                            .replace(&cwd, "<root>")
                    })
                    .collect();
                *transcript = normalized.into_iter().map(Value::String).collect();
            }
        })
        .await;
        assert_eq!(
            std::fs::read_to_string(root.join("code.rs")).unwrap(),
            "two\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("Cargo.toml")).unwrap(),
            "[package]\nx = 1\n"
        );

        let actual: String = transcript
            .iter()
            .map(|line| format!("{}\n", line.as_str().unwrap()))
            .collect();
        let golden = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/acp/transcripts/edit-permission-granted-and-refused.jsonl");
        if std::env::var_os("PWR_UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
            std::fs::write(&golden, &actual).unwrap();
        }
        let expected = std::fs::read_to_string(&golden)
            .expect("no golden transcript; run once with PWR_UPDATE_GOLDEN=1 and review it");
        assert_eq!(actual, expected, "the transcript a client sees changed");
    }

    #[test]
    fn every_ending_has_an_acp_stop_reason() {
        let ended = |stopped| {
            stop_reason(&TurnReport {
                stopped,
                ..report("")
            })
            .0
        };
        assert_eq!(ended(None), "end_turn");
        assert_eq!(ended(Some(StopReason::Interrupted)), "cancelled");
        assert_eq!(ended(Some(StopReason::BudgetSpent)), "max_turn_requests");
        assert_eq!(ended(Some(StopReason::ContextFull)), "max_tokens");
        assert_eq!(ended(Some(StopReason::Looping)), "max_tokens");
        assert_eq!(ended(Some(StopReason::Silent)), "end_turn");
        assert_eq!(ended(Some(StopReason::BackendFailing)), "end_turn");
        assert_eq!(tool_kind("apply_patch"), "edit");
        assert_eq!(tool_kind("list_tree"), "search");
        assert_eq!(tool_kind("ask_user"), "other");
    }
}
