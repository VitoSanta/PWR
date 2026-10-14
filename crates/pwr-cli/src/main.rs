use clap::{Args, Parser, Subcommand};
use crossterm::{
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEventKind,
        KeyModifiers,
    },
    execute,
};
use dialoguer::{Input, MultiSelect, Select};
use pwr_orchestrator::converse;
mod semantic;
mod serve;
/// The two loops, driven from one script. See the module's own header.
#[cfg(test)]
mod two_loops;

use futures_util::StreamExt;
use pwr_domain::{
    ChatMessage, DeploymentDescriptor, HardwareProfile, ModelRequest, Observation, ToolCall,
    Validate, hash_bytes, new_id,
};
#[cfg(test)]
use pwr_orchestrator::executor::{CheckVerdict, check_verdict};
use pwr_provider::{InferenceBackend, ModelProvider};
use pwr_runtime::{BackendKind, RuntimeBackend, RuntimeFactory};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, IsTerminal as _, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(name = "pwr", version, about = "Local, evidence-driven coding agent")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(long, global = true)]
    json: bool,
    /// Which inference engine runs the model: `mlx`, or `llama` for GGUF/llama.cpp.
    ///
    /// The HTTP backends (Ollama, LM Studio) were removed on 2026-09-19.
    /// llama.cpp is being added for GGUF and Windows; it reads GGUF metadata
    /// and starts a managed `llama-server` on demand for generation.
    #[arg(long, global = true)]
    backend: Option<String>,
}
#[derive(Subcommand)]
enum Command {
    /// Start an interactive conversation and agent control surface for this workspace.
    Chat(ChatArgs),
    /// Serve conversations to a front end over the Agent Client Protocol.
    ///
    /// A client -- an editor, or a native PWR app -- launches this process
    /// and speaks JSON-RPC on its stdin and stdout. Each session runs the same
    /// turn the console runs, in the workspace's own configuration; approvals
    /// Settings ask about are put to the client. See `docs/pwr-serve.md`.
    Serve {
        /// Speak the protocol on stdin and stdout, the only transport there is.
        #[arg(long)]
        stdio: bool,
    },
    Doctor,
    Models(Models),
    Repo(Repo),
    Eval(Eval),
}

/// The interactive surface keeps its settings in the workspace, while the
/// engine and model flags make a first manual run reproducible from a shell.
#[derive(Args, Default)]
struct ChatArgs {
    /// Model artifact to use for this conversation. This also becomes this
    /// workspace's selected model, so later launches do not need the flag.
    #[arg(long)]
    model: Option<String>,
    /// Attach a local file as read-only context. Repeat for more files.
    #[arg(long = "attach")]
    attachments: Vec<PathBuf>,
    /// Continue the most recent conversation in this workspace.
    ///
    /// Its messages are restored as its last complete turn left them, and
    /// what changed since -- files edited outside it, actions an interrupted
    /// turn took after that point, writes that may or may not have happened --
    /// is told to the deployment and shown before anything is sent.
    #[arg(long = "continue")]
    resume: bool,
}
#[derive(clap::Args)]
struct SessionArgs {
    #[command(subcommand)]
    command: SessionCommand,
}
#[derive(clap::Subcommand)]
enum SessionCommand {
    /// Every session in this workspace, most recently opened last.
    List,
    /// What one session established, checked against the workspace now.
    Show { name: String },
}

/// User-grantable approvals, named on the command line.
#[derive(Clone, Copy, clap::ValueEnum)]
#[value(rename_all = "kebab-case")]
enum ApprovalArg {
    DependencyChange,
    HistoryRewrite,
    Publish,
    NetworkAccess,
    LocalService,
    ToolchainInstall,
    /// Driving Docker or another container engine, whose containers run
    /// outside the sandbox.
    ContainerEngine,
    /// Adopting a check the deployment proposes for a workspace that declares
    /// none. Granting it in advance means the run may adopt one without asking
    /// again, which is the unattended case; without it a proposal is asked
    /// about when a terminal is attached, and refused when none is.
    VerifierProposal,
    /// Running a command outside the sandbox, with your full rights.
    OutsideSandbox,
    /// Reaching a folder outside the workspace.
    OutsideWorkspace,
}
impl From<ApprovalArg> for pwr_tools::Approval {
    fn from(value: ApprovalArg) -> Self {
        match value {
            ApprovalArg::DependencyChange => Self::DependencyChange,
            ApprovalArg::HistoryRewrite => Self::HistoryRewrite,
            ApprovalArg::Publish => Self::Publish,
            ApprovalArg::NetworkAccess => Self::NetworkAccess,
            ApprovalArg::LocalService => Self::LocalService,
            ApprovalArg::ToolchainInstall => Self::ToolchainInstall,
            ApprovalArg::ContainerEngine => Self::ContainerEngine,
            ApprovalArg::VerifierProposal => Self::VerifierProposal,
            ApprovalArg::OutsideSandbox => Self::OutsideSandbox,
            ApprovalArg::OutsideWorkspace => Self::OutsideWorkspace,
        }
    }
}

#[derive(Args)]
struct Models {
    #[command(subcommand)]
    command: ModelsCommand,
}
#[derive(Subcommand)]
enum ModelsCommand {
    /// Render the immutable download plan for a HuggingFace artifact.
    ///
    /// This does not reach the network. It translates the artifact registry
    /// into pinned Hub URLs and local destinations so the downloader can later
    /// execute exactly this plan with resume and verification.
    DownloadPlan {
        artifact: String,
        /// Root under which the repository path is recreated.
        #[arg(long)]
        destination_root: Option<PathBuf>,
    },
    /// Download a HuggingFace artifact only when every file can be verified.
    Download {
        artifact: String,
        /// Root under which the repository path is recreated.
        #[arg(long)]
        destination_root: Option<PathBuf>,
    },
}
#[derive(Args)]
struct Repo {
    #[command(subcommand)]
    command: RepoCommand,
}
#[derive(Subcommand)]
enum RepoCommand {
    Index {
        path: Option<PathBuf>,
    },
    /// The passages a turn would be given for a request, with their scores.
    ///
    /// The ranking a conversation already runs, made visible: what retrieval
    /// chose, why, and what it cost in tokens. Written for the context-filter
    /// experiment (backlog C.22), where an encoder's choices have to be
    /// compared against this one's on the same requests.
    Rank {
        /// The request to rank against, as a turn would send it.
        query: String,
        path: Option<PathBuf>,
        /// Passages at most (the turn's own default is used when absent).
        #[arg(long)]
        max: Option<usize>,
        /// Token budget for the passages (the turn derives it from the window).
        #[arg(long)]
        budget: Option<usize>,
        /// Print each passage's text as well as where it came from.
        #[arg(long)]
        content: bool,
        /// Fuse the lexical ranking of document sections with a local
        /// embedding model's (backlog C.22). Offline: the model must already
        /// be cached; without it the ranking is lexical and says so.
        #[arg(long)]
        semantic: bool,
    },
}
#[derive(Args)]
struct Eval {
    #[command(subcommand)]
    command: EvalCommand,
}
// Parsed once per invocation; boxing its arguments would buy nothing.
#[allow(clippy::large_enum_variant)]
#[derive(Subcommand)]
enum EvalCommand {
    /// Runs a regression suite (areas A1-A4) and reports every case's verdict.
    ///
    /// Seconds to minutes, so a harness change can be checked the same day.
    /// Fails only on a regression: a case declared as a known gap is
    /// measured, and one whose gap has closed is reported for promotion.
    Suite {
        /// A suite file, e.g. `suites/a1-tool-calls.json`.
        file: PathBuf,
        /// Where the suite's task cases were run (`eval run --out-dir`).
        /// Replay cases need nothing; a task case with no report here is
        /// reported as not run.
        #[arg(long)]
        reports: Option<PathBuf>,
        /// Exit with an error when a case regressed. Without it the report is
        /// printed either way and lists the regressions.
        #[arg(long)]
        strict: bool,
    },
}
#[derive(Serialize)]
struct Output<T: Serialize> {
    schema_version: u32,
    ok: bool,
    result: Option<T>,
    error: Option<SafeError>,
}
#[derive(Serialize)]
struct SafeError {
    category: &'static str,
    context: String,
}

/// The exit code a category means.
///
/// `CLI-spec.md` has always declared six codes and the implementation returned
/// 4 for every failure, so a caller scripting around PWR could not tell a
/// policy denial from a backend being down. The category was already carried on
/// every error; only the mapping was missing.
///
/// 1 is reserved for the work failing -- a task or a verification -- which is
/// the one outcome that is not PWR malfunctioning.
fn exit_code(category: &str) -> i32 {
    match category {
        "task_failed" => 1,
        "invalid_input" | "conflict" | "missing_evidence" | "incompatible_model"
        | "calibration" => 2,
        "policy_denied" => 3,
        // The deployment produced output the backend could not parse: the work
        // failed, not the infrastructure.
        "model_output" => 1,
        // Busy is the host refusing to run two models at once, which from the
        // caller's side is the backend being unavailable to it right now.
        "provider_unavailable"
        | "provider_protocol"
        | "provider_context_limit"
        | "provider_truncated"
        | "provider_cancelled"
        | "cancelled"
        | "resource_busy" => 4,
        _ => 5,
    }
}

fn write_immutable_artifact(path: &Path, bytes: &[u8]) -> Result<(), SafeError> {
    use std::io::Write as _;
    let parent = path.parent().ok_or_else(|| SafeError {
        category: "internal",
        context: "artifact path has no parent".into(),
    })?;
    std::fs::create_dir_all(parent).map_err(|error| SafeError {
        category: "internal",
        context: error.to_string(),
    })?;
    if path.exists() {
        return if std::fs::read(path).is_ok_and(|existing| existing == bytes) {
            Ok(())
        } else {
            Err(SafeError {
                category: "conflict",
                context: format!("refusing to overwrite artifact {}", path.display()),
            })
        };
    }
    let temporary = parent.join(format!(".artifact-{}.tmp", new_id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| SafeError {
            category: "internal",
            context: error.to_string(),
        })?;
    if let Err(error) = file.write_all(bytes).and_then(|_| file.sync_all()) {
        let _ = std::fs::remove_file(&temporary);
        return Err(SafeError {
            category: "internal",
            context: error.to_string(),
        });
    }
    match std::fs::hard_link(&temporary, path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !std::fs::read(path).is_ok_and(|existing| existing == bytes) {
                let _ = std::fs::remove_file(&temporary);
                return Err(SafeError {
                    category: "conflict",
                    context: format!("refusing to overwrite artifact {}", path.display()),
                });
            }
        }
        Err(error) => {
            let _ = std::fs::remove_file(&temporary);
            return Err(SafeError {
                category: "internal",
                context: error.to_string(),
            });
        }
    }
    std::fs::remove_file(&temporary).map_err(|error| SafeError {
        category: "internal",
        context: error.to_string(),
    })?;
    Ok(())
}

fn print<T: Serialize>(json: bool, result: Result<T, SafeError>) -> i32 {
    match result {
        Ok(value) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Output {
                        schema_version: 1,
                        ok: true,
                        result: Some(value),
                        error: None
                    })
                    .unwrap()
                );
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
            0
        }
        Err(error) => {
            let code = exit_code(error.category);
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&Output::<serde_json::Value> {
                        schema_version: 1,
                        ok: false,
                        result: None,
                        error: Some(error)
                    })
                    .unwrap()
                );
            } else {
                eprintln!("{}: {}", error.category, error.context);
            }
            code
        }
    }
}

fn probe_required_by_default() -> bool {
    true
}

/// Where a chat starts before a deployment has said what it can serve.
///
/// Only a starting point: choosing a model raises it to whatever the backend
/// reports for that deployment, because a console that quietly uses 8,192 of a
/// 262,144-token window is throwing away most of what the machine is for.
const CHAT_CONTEXT_DEFAULT: u32 = 8_192;

/// Sets the chat's context to the window computed for this model and host.
///
/// Computed by `pwr_orchestrator::window` from the model's config and this
/// machine's memory, as `run` and `eval` do. This used to raise the context to
/// the deployment's declared ceiling -- the model's trained length, with no
/// check that the host could hold it -- which is the "maximum the machine can
/// run" the product thesis refuses (backlog, Part 0d). A declared model profile
/// still bounds it, now as the effective-context ceiling it always was.
///
/// The ceiling is what the model *can* do; it is not what the backend has
/// loaded. This asked for the ceiling and then recorded it as fact, and the
/// gap between the two was invisible and expensive. Measured on an 80B whose
/// tag advertises 262,144 tokens: the chat ran at a declared 65,536 while LM
/// Studio had the instance up at 16,384 with four parallel slots, and LM
/// Studio's answer to a prompt that does not fit is not an error but
/// `TruncateMiddle` -- it keeps the head and the tail and silently deletes the
/// middle. Across one run it did that fifty-nine times, so every tool result,
/// file body and build log the turn had gathered was dropped before the model
/// saw it while the reported prompt sat at exactly 7,090 tokens turn after
/// turn. The deployment re-read files it had just read and reintroduced errors
/// it had just fixed, and the run was recorded as the model failing the task.
///
/// So the window is requested and then taken as granted: `prepare_context`
/// loads it and reads back what the backend actually serves, and that number
/// is what the conversation is measured against. A backend that serves less
/// than was asked is a fact the operator can act on; a backend that serves
/// less than was assumed is a silent corruption of every turn.
struct ContextDecision {
    computed: bool,
    line: String,
    granted_tokens: u32,
    requested_tokens: Option<u32>,
    binding_ceiling: Option<String>,
    context_memory_budget_bytes: Option<u64>,
}

/// Update the observed window only after the backend acknowledges it.
async fn prepare_chat_window(
    provider: &dyn ModelProvider,
    deployment: &DeploymentDescriptor,
    config: &mut ChatConfig,
    requested: u32,
) -> Result<u32, SafeError> {
    let granted = provider
        .prepare_context(deployment, requested)
        .await
        .map_err(provider_error)?;
    config.context_tokens = granted;
    Ok(granted)
}

async fn compute_context(
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
) -> Result<Option<ContextDecision>, SafeError> {
    let Some(model) = config.model.clone() else {
        return Ok(None);
    };
    let pack = pwr_domain::gemma::pack(&model).ok_or_else(|| SafeError {
        category: "invalid_input",
        context: pwr_domain::gemma::not_in_line(&model),
    })?;
    let timeout = Duration::from_secs(config.timeout_secs);
    let selection = runtime
        .select(model.clone(), timeout)
        .map_err(provider_error)?;
    // The window is the line's full context unless the person asked for less,
    // and never more than this Mac's memory holds for the pack: weights, the
    // global layers' cache at the bits the engine keeps it, the sliding
    // layers, a prefill step's scores, the engine and the reserve
    // (`pwr_domain::gemma::memory_for`).
    let kv_bits = if std::env::var("PWR_MLX_KV_BITS").as_deref() == Ok("8") {
        8
    } else {
        16
    };
    let full = pwr_domain::gemma::CONTEXT_TOKENS;
    let wanted = config.context_setting.unwrap_or(full).min(full);
    let memory = probe_hardware().await.total_memory_bytes;
    let held = memory.map(|memory| pwr_domain::gemma::window(pack, memory, kv_bits));
    let tokens = held.map_or(wanted, |held| held.min(wanted)).max(2_048);
    let bound_by = match held {
        Some(held) if held < wanted => Some("memory".to_owned()),
        _ if wanted < full => Some("setting".to_owned()),
        _ => None,
    };
    let rationale = match (held, bound_by.as_deref()) {
        (Some(held), Some("memory")) => format!(
            "{held} of {full} tokens: this Mac's memory holds that much for {} \
             (the full context needs about {:.1} GiB with the cache at {kv_bits} bits)",
            pack.label,
            pwr_domain::gemma::memory_for(pack, full, kv_bits) as f64 / (1u64 << 30) as f64,
        ),
        (_, Some("setting")) => format!("{wanted} tokens, as set for this workspace"),
        _ => format!("the full {full}-token context of {}", pack.label),
    };
    let granted =
        prepare_chat_window(&selection.backend, &selection.deployment, config, tokens).await?;
    Ok(Some(ContextDecision {
        computed: true,
        line: if granted < tokens {
            format!(
                "Context: {granted} tokens -- {rationale}, but the engine loaded only {granted}"
            )
        } else {
            format!("Context: {rationale}")
        },
        granted_tokens: granted,
        requested_tokens: Some(tokens),
        binding_ceiling: bound_by,
        context_memory_budget_bytes: memory
            .map(|memory| memory.saturating_sub(pwr_domain::gemma::RESERVE_BYTES)),
    }))
}

/// Paths the workspace declares part of the task rather than part of the work.
///
/// Read from `.pwr/protected.json`, a list of workspace-relative paths:
///
/// ```json
/// {"protected": ["SPECIFICATION.md", "src/app/app-shell.spec.ts"]}
/// ```
///
/// A specification and its acceptance tests are the question being asked, and
/// nothing stopped a deployment from rewriting them -- the task said they were
/// frozen, in prose, which is not a guard. Measured on an 80B: twelve
/// `write_file` calls aimed at `SPECIFICATION.md`, each carrying the correct
/// contents of a source file that did not exist yet.
///
/// Absent means nothing is protected, which is what every existing workspace
/// expects. **Present but unreadable is an error, not an empty list.** Until
/// 2026-09-23 a file that failed to parse -- or used any key but `protected`
/// -- protected nothing, silently: three experiment runs wrote
/// `{"paths": [...]}` and their acceptance tests were editable throughout.
/// Nobody edited them, which is luck rather than a guard. The same day showed
/// the scripted run never read the file at all.
fn frozen_paths(root: &Path) -> Result<Vec<PathBuf>, SafeError> {
    frozen_paths_with_authorization(root, &Default::default())
}
fn frozen_paths_with_authorization(
    root: &Path,
    authorized: &std::collections::BTreeSet<String>,
) -> Result<Vec<PathBuf>, SafeError> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Declared {
        #[serde(default)]
        protected: Vec<PathBuf>,
    }
    let path = root.join(".pwr/protected.json");
    let body = match std::fs::read_to_string(&path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "{\"protected\": []}".to_owned()
        }
        Err(error) => {
            return Err(SafeError {
                category: "invalid_input",
                context: format!("{} could not be read: {error}", path.display()),
            });
        }
    };
    let mut protected = serde_json::from_str::<Declared>(&body)
        .map(|declared| declared.protected)
        .map_err(|error| SafeError {
            category: "invalid_input",
            context: format!(
                "{} declares what this task must not change, and it could not be read ({error}). \
                 Its shape is {{\"protected\": [\"path\", ...]}}. Nothing runs until it is \
                 fixed: treating it as empty would leave unprotected exactly what it names.",
                path.display()
            ),
        })?;
    if let Some(snapshot) = pwr_verify::acceptance::snapshot(root).map_err(|context| SafeError {
        category: "invalid_input",
        context,
    })? {
        if !authorized.contains(".pwr/checks.json") {
            protected.push(PathBuf::from(".pwr/checks.json"));
        }
        protected.extend(
            snapshot
                .artifacts
                .keys()
                .filter(|path| !authorized.contains(*path))
                .map(PathBuf::from),
        );
    }
    protected.sort();
    protected.dedup();
    Ok(protected)
}

const CHAT_TIMEOUT_DEFAULT: u64 = 900;
const MAX_CHAT_ATTACHMENT_CHARS: usize = 256 * 1024;
const MAX_CHAT_FOLDER_FILES: usize = 32;
const MAX_CHAT_FOLDER_DEPTH: usize = 4;

/// Settings are deliberately workspace-local. A chat about one repository
/// should not quietly make a different deployment or context budget the
/// default for every other project on the machine.
/// Every field defaults. A configuration written by an earlier build is
/// missing whatever has been added since, and refusing it stopped the console
/// from opening at all -- which leaves no way to fix the setting that is
/// wrong, because the settings live inside the console.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(default)]
struct ChatConfig {
    background_summaries: bool,
    /// Goal-wide limits, including baseline verification and review.
    goal_budget: serve::GoalLimits,
    /// Help a goal is given beyond being run, each off unless asked for.
    goal_aids: serve::GoalAids,
    tool_groups: converse::ToolGroups,
    /// The engine selected for this workspace. It remains workspace-local so a
    /// GGUF project need not repeat `--backend llama` on every launch.
    #[serde(default)]
    backend: Option<String>,
    model: Option<String>,
    /// Gemma's thinking, on or off; off unless the person turned it on
    /// (`pwr_domain::gemma::effort`). The Reasoning Effort level that stood
    /// here before 2026-10-14 is ignored when an old file names it.
    thinking: bool,
    /// Actions one turn may take before it stops and asks; a goal has its own
    /// budget (`goal_budget`). 0 or absent is the default.
    actions_per_turn: usize,
    /// Untested models the person chose to use with conservative defaults
    /// rather than calibrate; only stops the app offering the choice again.
    acknowledged_provisional: Vec<String>,
    profile: Option<PathBuf>,
    /// UI-level binding for the profile prepared by this chat. Runtime still
    /// verifies the stronger digest and deployment fingerprint binding.
    prepared_for_model: Option<String>,
    /// Retained to read configurations from the probe-gated console. Probes
    /// are research diagnostics now, never a prerequisite for a manual run.
    #[serde(default = "probe_required_by_default")]
    require_probe: bool,
    /// Set when the selected backend cannot be calibrated at all, so readiness
    /// does not wait on a measurement that can never be taken.
    ///
    /// A context ladder needs a backend that will serve each rung. Where the
    /// window is decided elsewhere there is none, and requiring a calibration
    /// left the console stuck on PREPARE MODEL forever with no way out.
    #[serde(default)]
    prepared_without_calibration: bool,
    context_tokens: u32,
    /// The window the person chose, kept apart from the one in force.
    ///
    /// Without it a choice made in the app was saved and then overwritten by
    /// the very next computation -- which every refresh runs -- so the control
    /// appeared to do nothing. It is a ceiling like the others: the computed
    /// window still refuses to exceed what this host's memory holds.
    #[serde(skip_serializing_if = "Option::is_none")]
    context_setting: Option<u32>,
    /// Folders outside the workspace the conversation may read but never
    /// change, relative to the workspace or absolute: a site in a project's
    /// subfolder reading the project's documentation (`[".."]`). Declared,
    /// never inferred; `.pwr`, `.git` and `.env*` inside them stay closed.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reference_roots: Vec<PathBuf>,
    timeout_secs: u64,
    plan: bool,
    /// Kinds of action the conversation asks about before running, in
    /// [`PermissionMode::Ask`]; every other kind is granted for work in this
    /// workspace.
    #[serde(default = "asked_before_by_default")]
    ask_before: Vec<pwr_tools::Approval>,
    /// Ask before the kinds above, or run with every permission. `None` in a
    /// configuration written before the modes existed, which is migrated to
    /// `Ask` when it is read (see [`load_chat_config`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission_mode: Option<PermissionMode>,
    /// The share of the window, in percent, at which the conversation
    /// compacts itself; `None` for the default (75).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compact_at_percent: Option<u8>,
    /// A ceiling in tokens under that share, for a model that degrades
    /// before its window is three quarters full. Set by hand in
    /// `.pwr/chat-config.json`; `None`, the default, is no ceiling
    /// (D-2026-10-02-3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    compact_ceiling_tokens: Option<usize>,
}

/// The workspace's auto-compaction threshold and ceiling, if it chose them:
/// otherwise three quarters of the granted window, with no ceiling
/// (D-2026-10-02-3).
fn compaction_settings(config: &ChatConfig, continuity: &mut converse::Continuity) {
    continuity.compact_at_percent = config.compact_at_percent;
    continuity.compact_ceiling_tokens = config.compact_ceiling_tokens;
}

/// How much a conversation may do without asking. Decided 2026-09-23 with the
/// maintainer, after an external review found that the conversation granted
/// dependency changes, network access and toolchain installs in advance --
/// which left the installed-dependency guard (backlog D.E2E-29) inert in the
/// app. Three since 2026-09-29, when a third was added at the maintainer's
/// request after the sandbox kept a .NET build from running: the stored names
/// stay, so no saved configuration changes meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PermissionMode {
    /// Protected: ask before what reaches outside the workspace or cannot be
    /// taken back -- the kinds in `ask_before`. The default.
    Ask,
    /// Standard: the network, dependencies and toolchains granted; asks only
    /// before what cannot be taken back or leaves the sandbox
    /// ([`STANDARD_ASKED`]). The sandbox and the policy's own limits hold.
    Auto,
    /// Full access: no sandbox, the person's own environment, nothing asked.
    /// A command runs as it would in their terminal.
    Full,
}

/// What Standard still asks about: publishing and rewriting history, which
/// cannot be taken back, and the container engine, a command run outside
/// the sandbox and a folder outside the workspace, which leave it.
const STANDARD_ASKED: [pwr_tools::Approval; 5] = [
    pwr_tools::Approval::Publish,
    pwr_tools::Approval::HistoryRewrite,
    pwr_tools::Approval::ContainerEngine,
    pwr_tools::Approval::OutsideSandbox,
    pwr_tools::Approval::OutsideWorkspace,
];

/// The boundary a workspace's commands run in, given its mode.
fn sandbox_for(config: &ChatConfig) -> pwr_tools::SandboxPolicy {
    match config.permission_mode.unwrap_or(PermissionMode::Ask) {
        PermissionMode::Full => pwr_tools::SandboxPolicy::FullAccess,
        PermissionMode::Ask | PermissionMode::Auto => pwr_tools::SandboxPolicy::Preferred,
    }
}

/// What `Ask` asks about by default: changing dependencies, reaching the
/// network, running programs the workspace does not list, using the container
/// engine, rewriting history and publishing.
fn asked_before_by_default() -> Vec<pwr_tools::Approval> {
    vec![
        pwr_tools::Approval::DependencyChange,
        pwr_tools::Approval::HistoryRewrite,
        pwr_tools::Approval::Publish,
        pwr_tools::Approval::NetworkAccess,
        pwr_tools::Approval::ToolchainInstall,
        pwr_tools::Approval::ContainerEngine,
    ]
}

/// What `Ask` asks about whatever Settings hold.
///
/// The container engine is the grant that leaves the sandbox. A list saved
/// before it existed does not name it, and "not named" means "granted" --
/// so without this, a person who had only ever chosen `Ask` would have had
/// Docker handed to the model without a question the day it was added. The
/// same holds for running a command outside the sandbox and reaching a
/// folder outside the workspace, both added 2026-09-29.
const ALWAYS_ASKED: [pwr_tools::Approval; 3] = [
    pwr_tools::Approval::ContainerEngine,
    pwr_tools::Approval::OutsideSandbox,
    pwr_tools::Approval::OutsideWorkspace,
];

/// The list the old default asked about, before the modes existed. A saved
/// configuration still holding exactly this was never chosen by anyone, so it
/// is moved to the new default rather than kept as a choice.
fn asked_before_by_default_until_2026_09_23() -> Vec<pwr_tools::Approval> {
    vec![
        pwr_tools::Approval::HistoryRewrite,
        pwr_tools::Approval::Publish,
    ]
}

/// What the conversation actually asks about, given its mode.
fn effective_ask_before(config: &ChatConfig) -> Vec<pwr_tools::Approval> {
    match config.permission_mode.unwrap_or(PermissionMode::Ask) {
        PermissionMode::Full => Vec::new(),
        PermissionMode::Auto => STANDARD_ASKED.to_vec(),
        PermissionMode::Ask => {
            let mut asked = config.ask_before.clone();
            for approval in ALWAYS_ASKED {
                if !asked.contains(&approval) {
                    asked.push(approval);
                }
            }
            asked
        }
    }
}

/// What a conversation turn is granted: everything the console grants for work
/// in this workspace, except what Settings say to ask about, plus what was
/// allowed for the rest of this session when asked.
fn chat_approvals(
    ask_before: &[pwr_tools::Approval],
    session_grants: &[pwr_tools::Approval],
) -> Vec<pwr_tools::Approval> {
    let mut granted: Vec<_> = all_approvals()
        .into_iter()
        .filter(|approval| !ask_before.contains(approval) || session_grants.contains(approval))
        .collect();
    // What no mode grants, reading the person's terminal, is granted only
    // when they allowed it for this session.
    granted.extend(session_grants.iter().copied());
    granted.sort();
    granted.dedup();
    granted
}

/// What a conversation's checks may do: what its turns may, Settings and the
/// session's grants together.
fn session_approvals(root: &Path, context: &serve::CommandContext) -> Vec<pwr_tools::Approval> {
    let config = load_chat_config(root).unwrap_or_default();
    chat_approvals(&effective_ask_before(&config), &context.session_grants)
}

/// The session's grants as they stand.
fn granted_now(grants: &std::sync::Mutex<Vec<pwr_tools::Approval>>) -> Vec<pwr_tools::Approval> {
    grants
        .lock()
        .map(|grants| grants.clone())
        .unwrap_or_default()
}

/// The plain name of a kind of approval, for Settings and the prompt.
fn approval_label(approval: pwr_tools::Approval) -> &'static str {
    use pwr_tools::Approval;
    match approval {
        Approval::DependencyChange => "change dependencies (manifests and lockfiles)",
        Approval::HistoryRewrite => "rewrite git history (rebase, amend, reset --hard)",
        Approval::Publish => "push to a remote or publish a package",
        Approval::NetworkAccess => "reach the network",
        Approval::LocalService => "start and reach services on this machine",
        Approval::ToolchainInstall => {
            "run programs this workspace does not list, and toolchains it installs"
        }
        Approval::ContainerEngine => {
            "use Docker or another container engine (containers run outside the sandbox)"
        }
        Approval::VerifierProposal => "adopt a check the model proposes",
        Approval::OutsideSandbox => "run a command outside the sandbox, with your full rights",
        Approval::OutsideWorkspace => "reach a folder outside the workspace",
        Approval::TerminalRead => "read the recent output of your terminal tabs",
    }
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            background_summaries: false,
            goal_budget: serve::GoalLimits::default(),
            goal_aids: serve::GoalAids::default(),
            tool_groups: converse::ToolGroups::default(),
            backend: None,
            require_probe: true,
            model: None,
            thinking: false,
            actions_per_turn: pwr_orchestrator::converse::DEFAULT_ACTIONS_PER_TURN,
            acknowledged_provisional: Vec::new(),
            profile: None,
            prepared_for_model: None,
            prepared_without_calibration: false,
            context_tokens: CHAT_CONTEXT_DEFAULT,
            context_setting: None,
            reference_roots: Vec::new(),
            timeout_secs: CHAT_TIMEOUT_DEFAULT,
            plan: false,
            ask_before: asked_before_by_default(),
            permission_mode: Some(PermissionMode::Ask),
            compact_at_percent: None,
            compact_ceiling_tokens: None,
        }
    }
}

/// The declared reference folders, resolved against the workspace; ones that
/// do not exist are dropped rather than failing the turn.
/// Where chat mode keeps its conversations, settings and images: a folder of
/// PWR's own, since chat mode has no workspace (decided 2026-09-23 with
/// the maintainer). `PWR_CHAT_HOME` moves it.
fn chat_home() -> Result<PathBuf, String> {
    let home = match std::env::var_os("PWR_CHAT_HOME") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(std::env::var_os("HOME").ok_or("HOME is not set")?).join(".pwr/chat"),
    };
    fs::create_dir_all(&home).map_err(|error| format!("{}: {error}", home.display()))?;
    home.canonicalize()
        .map_err(|error| format!("{}: {error}", home.display()))
}

/// Whether `root` is chat mode's folder rather than a workspace.
fn is_chat_home(root: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    chat_home().is_ok_and(|home| home == root)
}

fn reference_roots(root: &Path, config: &ChatConfig) -> Vec<PathBuf> {
    config
        .reference_roots
        .iter()
        .map(|path| root.join(path))
        .filter_map(|path| path.canonicalize().ok())
        .filter(|path| path.is_dir())
        .collect()
}

/// The conversation's instructions, with the reference folders it may read
/// named and their documents listed: a folder the model is not told about is
/// one it will not look in.
fn chat_system_prompt_for(root: &Path) -> String {
    let mut prompt = chat_system_prompt_without_person(root);
    // The person, what they asked to be remembered and the project's own
    // instructions, re-read every turn (see `refresh_system_prompt`'s caller),
    // so a change in Settings reaches the next reply.
    if let Ok(home) = pwr_orchestrator::personal::Home::from_env()
        && let Some(block) = pwr_orchestrator::personal::prompt_block_with_memory(
            &home,
            root,
            !is_chat_home(root),
            load_chat_config(root)
                .map(|config| config.tool_groups.memory)
                .unwrap_or(false),
        )
    {
        prompt.push_str(&block);
    }
    prompt
}

fn chat_system_prompt_without_person(root: &Path) -> String {
    let chat_only = is_chat_home(root);
    let mut prompt = if chat_only {
        converse::chat_only_system_prompt()
    } else {
        converse::chat_system_prompt(root)
    };
    let Ok(config) = load_chat_config(root) else {
        return prompt;
    };
    let roots = reference_roots(root, &config);
    if roots.is_empty() {
        return prompt;
    }
    prompt.push_str(if chat_only {
        "\n\nRead-only folders the engineer attached. List one with list_tree, passing its \
         path, and read its files with read_file using the paths listed. Their documents:"
    } else {
        "\n\nRead-only reference folders. These may be read with read_file using a path \
         relative to the workspace (for example `../README.md`) and listed with list_tree \
         given that path; they can never be written, and search covers only the workspace. \
         Their documents:"
    });
    for (declared, resolved) in config.reference_roots.iter().zip(&roots) {
        let mut documents = Vec::new();
        collect_reference_documents(resolved, resolved, 0, &mut documents);
        documents.sort();
        prompt.push_str(&format!(
            "\n- `{}` ({}):",
            declared.display(),
            resolved.display()
        ));
        for document in documents.iter().take(80) {
            prompt.push_str(&format!("\n  - {}/{}", declared.display(), document));
        }
        if documents.len() > 80 {
            prompt.push_str(&format!("\n  - ... and {} more", documents.len() - 80));
        }
    }
    prompt
}

fn collect_reference_documents(base: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) {
    const SKIPPED: [&str; 8] = [
        ".git",
        ".pwr",
        "node_modules",
        "target",
        "dist",
        ".venv",
        "experiments",
        "output",
    ];
    if depth > 3 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || SKIPPED.contains(&name.as_str()) {
            continue;
        }
        if path.is_dir() {
            collect_reference_documents(base, &path, depth + 1, out);
        } else if matches!(
            path.extension().and_then(|ext| ext.to_str()),
            Some("md" | "txt")
        ) && let Ok(relative) = path.strip_prefix(base)
        {
            out.push(relative.display().to_string());
        }
    }
}

fn chat_is_prepared(config: &ChatConfig) -> bool {
    config.model.is_some()
}

fn chat_config_path(root: &Path) -> PathBuf {
    root.join(".pwr/chat-config.json")
}

/// Capability evidence describes an Ollama deployment rather than one
/// repository.  Store it in a per-user registry so the same installed model
/// can be reused from every workspace; each workspace retains its own copy as
/// an audit trail when a probe is executed there.
fn global_model_evidence_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".pwr/models"))
}

/// Restore a previously measured, globally cached profile when opening a new
/// workspace.  The artifact is accepted only after the live deployment digest,
/// endpoint/model fingerprint and harness revision all match.
async fn hydrate_chat_readiness(
    _root: &Path,
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
    announce: bool,
) -> Result<(), SafeError> {
    let Some(model) = config.model.clone() else {
        return Ok(());
    };
    // Restoring readiness is best effort, and its failure must not stop the
    // console from opening. A workspace that remembers a model the current
    // backend does not serve -- an old configuration, or a backend switched
    // since -- used to exit with "invalid model name" and leave no way in.
    let unavailable = |config: &mut ChatConfig, reason: String| {
        config.profile = None;
        config.prepared_for_model = None;
        config.prepared_without_calibration = false;
        if announce {
            task_status(
                "!",
                format!("{model} is not available on this backend ({reason}); choose one that is"),
            );
        }
        Ok(())
    };
    let selection = match runtime.select(model.clone(), Duration::from_secs(config.timeout_secs)) {
        Ok(selection) => selection,
        Err(error) => return unavailable(config, error.to_string()),
    };
    let provider = selection.backend;
    let deployment = selection.deployment;
    let _inspection = match provider.inspect(&deployment).await {
        Ok(inspection) => inspection,
        Err(error) => return unavailable(config, error.to_string()),
    };
    // A selected, inspectable model is enough to start a supervised local
    // conversation. Capability probes remain valuable catalogue evidence, but
    // making every first manual task wait for one contradicted B.1.
    config.prepared_without_calibration = true;
    config.prepared_for_model = config.model.clone();
    if announce && let Some(decision) = compute_context(runtime, config).await? {
        task_status(if decision.computed { "✓" } else { "!" }, decision.line);
    }
    Ok(())
}

fn load_chat_config(root: &Path) -> Result<ChatConfig, SafeError> {
    let path = chat_config_path(root);
    if !path.exists() {
        return Ok(ChatConfig::default());
    }
    let bytes = fs::read(&path).map_err(|error| SafeError {
        category: "internal",
        context: format!("could not read {}: {error}", path.display()),
    })?;
    let mut config: ChatConfig = serde_json::from_slice(&bytes).map_err(|error| SafeError {
        category: "invalid_input",
        context: format!(
            "{} is not a valid chat configuration: {error}",
            path.display()
        ),
    })?;
    // Written before the permission modes: it asks about the new defaults
    // unless someone had chosen a list of their own.
    if config.permission_mode.is_none() {
        let mut saved = config.ask_before.clone();
        saved.sort();
        let mut old = asked_before_by_default_until_2026_09_23();
        old.sort();
        if saved == old {
            config.ask_before = asked_before_by_default();
        }
        config.permission_mode = Some(PermissionMode::Ask);
    }
    Ok(config)
}

fn save_chat_config(root: &Path, config: &ChatConfig) -> Result<(), SafeError> {
    let path = chat_config_path(root);
    let parent = path.parent().expect("chat config has a parent");
    fs::create_dir_all(parent).map_err(|error| SafeError {
        category: "internal",
        context: format!("could not create {}: {error}", parent.display()),
    })?;
    fs::write(
        &path,
        serde_json::to_vec_pretty(config).expect("serializable"),
    )
    .map_err(|error| SafeError {
        category: "internal",
        context: format!("could not save {}: {error}", path.display()),
    })
}

fn attach_chat_file(root: &Path, path: &Path) -> Result<ChatMessage, SafeError> {
    let requested = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let path = requested.canonicalize().map_err(|error| SafeError {
        category: "invalid_input",
        context: format!("cannot resolve attachment {}: {error}", requested.display()),
    })?;
    if path.is_dir() {
        let inside = root
            .canonicalize()
            .map(|workspace| path.starts_with(workspace))
            .unwrap_or(false);
        return if inside {
            attach_chat_folder(root, &path)
        } else {
            attach_reference_folder(root, &path)
        };
    }
    if !path.is_file() {
        return Err(SafeError {
            category: "invalid_input",
            context: format!("attachment {} is not a file", path.display()),
        });
    }
    let bytes = fs::read(&path).map_err(|error| SafeError {
        category: "invalid_input",
        context: format!("cannot read attachment {}: {error}", path.display()),
    })?;
    attach_chat_bytes(root, &path.display().to_string(), bytes)
}

/// A folder outside the workspace, attached: declared as a read-only reference
/// folder rather than pasted in.
///
/// The snapshot concatenated at most 32 files and 256K characters in directory
/// order, so a project's `docs/` (870K) arrived cut at an arbitrary file.
/// Measured 2026-09-22: a site built in a project's subfolder, sent the
/// project's documentation, wrote from memory and an out-of-date CV instead.
/// Now the folder is recorded in `reference_roots`, so `read_file` reaches any
/// document in it and nothing in it can be written, and the message names the
/// path to use and the documents there.
fn attach_reference_folder(root: &Path, folder: &Path) -> Result<ChatMessage, SafeError> {
    let workspace = root.canonicalize().map_err(|error| SafeError {
        category: "invalid_input",
        context: format!("{}: {error}", root.display()),
    })?;
    // `..` for an ancestor, which is how the model will write it; otherwise the
    // absolute path.
    let declared = match workspace.strip_prefix(folder) {
        Ok(below) => {
            let depth = below.components().count();
            (0..depth)
                .map(|_| "..")
                .collect::<Vec<_>>()
                .join("/")
                .into()
        }
        Err(_) => folder.to_path_buf(),
    };
    let mut config = load_chat_config(&workspace)?;
    if !reference_roots(&workspace, &config)
        .iter()
        .any(|known| known == folder)
    {
        config.reference_roots.push(declared.clone());
        save_chat_config(&workspace, &config)?;
    }
    let mut documents = Vec::new();
    collect_reference_documents(folder, folder, 0, &mut documents);
    documents.sort();
    let prefix = declared.display().to_string();
    let mut text = format!(
        "[Reference folder: {} -- read-only. List it with list_tree (path `{prefix}`) and \
         read its files with read_file using paths that begin `{prefix}/`; writing there is \
         refused. Read the documents relevant to the task before stating anything about what \
         they describe.]\nDocuments:",
        folder.display()
    );
    for document in documents.iter().take(120) {
        text.push_str(&format!("\n- {prefix}/{document}"));
    }
    if documents.len() > 120 {
        text.push_str(&format!("\n- ... and {} more", documents.len() - 120));
    }
    Ok(ChatMessage::text("user", text))
}

/// A folder attachment is a bounded, read-only text snapshot. It deliberately
/// follows no symlinks and never gives the model a live directory handle.
fn attach_chat_folder(root: &Path, path: &Path) -> Result<ChatMessage, SafeError> {
    let mut files = Vec::new();
    collect_chat_folder_files(path, path, 0, &mut files)?;
    if files.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: format!(
                "folder attachment {} contains no readable text files",
                path.display()
            ),
        });
    }
    let mut text = String::new();
    for file in files {
        if text.chars().count() >= MAX_CHAT_ATTACHMENT_CHARS {
            break;
        }
        let Ok(bytes) = fs::read(&file) else {
            continue;
        };
        let Ok(contents) = String::from_utf8(bytes) else {
            continue;
        };
        let relative = file.strip_prefix(path).unwrap_or(&file).display();
        text.push_str("\n--- ");
        text.push_str(&relative.to_string());
        text.push_str(" ---\n");
        let remaining = MAX_CHAT_ATTACHMENT_CHARS.saturating_sub(text.chars().count());
        text.extend(contents.chars().take(remaining));
    }
    if text.trim().is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: format!(
                "folder attachment {} contains no UTF-8 text",
                path.display()
            ),
        });
    }
    attach_chat_bytes(
        root,
        &format!("folder {}", path.display()),
        text.into_bytes(),
    )
}

fn collect_chat_folder_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    files: &mut Vec<PathBuf>,
) -> Result<(), SafeError> {
    if depth > MAX_CHAT_FOLDER_DEPTH || files.len() >= MAX_CHAT_FOLDER_FILES {
        return Ok(());
    }
    let mut entries = fs::read_dir(directory)
        .map_err(|error| SafeError {
            category: "invalid_input",
            context: format!(
                "cannot read folder attachment {}: {error}",
                directory.display()
            ),
        })?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if files.len() >= MAX_CHAT_FOLDER_FILES {
            break;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let entry_path = entry.path();
        if file_type.is_dir() {
            if is_excluded_folder_attachment(&entry_path) {
                continue;
            }
            collect_chat_folder_files(root, &entry_path, depth + 1, files)?;
        } else if file_type.is_file()
            && entry_path.strip_prefix(root).is_ok()
            && entry
                .metadata()
                .map(|metadata| metadata.len() <= MAX_CHAT_ATTACHMENT_CHARS as u64)
                .unwrap_or(false)
        {
            files.push(entry_path);
        }
    }
    Ok(())
}

fn is_excluded_folder_attachment(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | ".pwr" | "node_modules" | "target" | "dist" | "build" | ".next")
    )
}

/// An attachment's bytes as the deployment reads them: text, or a PDF's
/// extracted text, bounded and snapshotted by content hash. `label` names where
/// it came from -- a path, or the URI a protocol client gave.
fn attach_chat_bytes(root: &Path, label: &str, bytes: Vec<u8>) -> Result<ChatMessage, SafeError> {
    let text = if bytes.starts_with(b"%PDF-") {
        pwr_tools::document::extract(&bytes)
            .map_err(|error| SafeError {
                category: "invalid_input",
                context: format!("cannot extract PDF {label}: {error}"),
            })?
            .text
    } else {
        String::from_utf8(bytes.clone()).map_err(|_| SafeError {
            category: "invalid_input",
            context: format!("attachment {label} is not UTF-8 text or a supported PDF"),
        })?
    };
    let text: String = text.chars().take(MAX_CHAT_ATTACHMENT_CHARS).collect();
    if text.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: format!("attachment {label} contains no usable text"),
        });
    }
    let hash = hash_bytes(&bytes);
    let snapshot = root
        .join(".pwr/chat-attachments")
        .join(format!("{hash}.txt"));
    fs::create_dir_all(snapshot.parent().expect("attachment has a parent")).map_err(|error| {
        SafeError {
            category: "internal",
            context: format!("could not create attachment snapshot directory: {error}"),
        }
    })?;
    if !snapshot.exists() {
        fs::write(&snapshot, &text).map_err(|error| SafeError {
            category: "internal",
            context: format!("could not snapshot attachment: {error}"),
        })?;
    }
    Ok(ChatMessage::text(
        "user",
        format!(
            "[Read-only attachment: {label} | sha256-like blake3: {hash} | snapshot: {}]\n{text}",
            snapshot.display()
        ),
    ))
}

/// The TUI receives paths directly, not through a shell. Still, people often
/// paste a shell-escaped path (for example `/Users/me/My\\ File.pdf`) or quote
/// it out of habit. Accept both spellings, while preserving a backslash that
/// is not escaping a space, quote, or another backslash.
fn parse_attachment_path(input: &str) -> PathBuf {
    let input = input.trim();
    let input = input
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            input
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(input);
    let mut path = String::with_capacity(input.len());
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            match character {
                ' ' | '\\' | '\'' | '"' => path.push(character),
                _ => {
                    path.push('\\');
                    path.push(character);
                }
            }
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else {
            path.push(character);
        }
    }
    if escaped {
        path.push('\\');
    }
    PathBuf::from(path)
}

fn task_status(symbol: &str, detail: impl std::fmt::Display) {
    let color = match symbol {
        "✓" => "\x1b[38;5;78m",
        "→" => "\x1b[38;5;117m",
        "✗" => "\x1b[38;5;203m",
        _ => "\x1b[38;5;252m",
    };
    println!("  {color}{symbol}\x1b[0m {detail}");
}

fn render_progress(detail: impl std::fmt::Display) {
    // Rewrite a single terminal row instead of consuming scrollback while a
    // model is loading or calibrating. The final success/failure still lands
    // as a durable line through `task_status`.
    print!("\r\x1b[2K  \x1b[38;5;117m●\x1b[0m {detail}");
    let _ = io::stdout().flush();
}

fn finish_progress_row() {
    println!();
}

fn clear_agent_screen() {
    print!("\x1b[2J\x1b[H");
    let _ = io::stdout().flush();
}

/// Characters between the box's own borders.
const HEADER_WIDTH: usize = 70;

/// One line of the header box, padded or shortened so the box actually closes.
///
/// A long workspace path used to run past the border and leave the frame open.
/// It is shortened from the left, because the tail of a path is the part that
/// says which project this is.
fn header_line(content: &str) -> String {
    let visible: Vec<char> = content.chars().collect();
    let body: String = if visible.len() <= HEADER_WIDTH {
        format!("{content}{}", " ".repeat(HEADER_WIDTH - visible.len()))
    } else {
        let tail: String = visible[visible.len() - (HEADER_WIDTH - 1)..]
            .iter()
            .collect();
        format!("…{tail}")
    };
    format!("\x1b[38;5;245m│{body}\x1b[1;38;5;117m│\x1b[0m")
}

/// What the console claims about the selected model.
///
/// "Ready" and "ready, but nothing has shown it works" are different claims,
/// and a console that renders them identically is making the second one look
/// like the first.
fn readiness_label(config: &ChatConfig) -> &'static str {
    match (
        chat_is_prepared(config),
        config.prepared_for_model == config.model,
    ) {
        (false, _) => "CHOOSE MODEL",
        (true, true) => "READY",
        (true, false) => "READY · UNMEASURED",
    }
}

fn render_chat_header(root: &Path, config: &ChatConfig, backend: BackendKind) {
    let model = config.model.as_deref().unwrap_or("No model selected");
    println!("\n\x1b[1;38;5;117m╭{}╮\x1b[0m", "─".repeat(HEADER_WIDTH));
    println!(
        "\x1b[1;38;5;117m│{}│\x1b[0m",
        format_args!(
            "  PWR  •  LOCAL AGENT CONSOLE{}",
            " ".repeat(HEADER_WIDTH - "  PWR  •  LOCAL AGENT CONSOLE".chars().count())
        )
    );
    println!(
        "{}",
        header_line(&format!("  Workspace: {}", root.display()))
    );
    // The backend is on the header because it is now something the console
    // lets you change: a model name alone does not say which one served it.
    println!("{}", header_line(&format!("  Backend: {}", backend.id())));
    println!(
        "{}",
        header_line(&format!(
            "  Model: {model}  •  Agent: {}  •  Context {}",
            readiness_label(config),
            config.context_tokens
        ))
    );
    println!("\x1b[1;38;5;117m╰{}╯\x1b[0m", "─".repeat(HEADER_WIDTH));
}

/// Interactive `/run` is intentionally the convenient, fully-authorized path
/// requested by the operator. The tool policy still confines writes to the
/// chosen workspace and keeps the event trail: broad consent is not a reason
/// to make a path outside that workspace an implicit target.
fn all_approvals() -> Vec<pwr_tools::Approval> {
    [
        ApprovalArg::DependencyChange,
        ApprovalArg::HistoryRewrite,
        ApprovalArg::Publish,
        ApprovalArg::NetworkAccess,
        ApprovalArg::LocalService,
        ApprovalArg::ToolchainInstall,
        ApprovalArg::ContainerEngine,
        ApprovalArg::VerifierProposal,
        ApprovalArg::OutsideSandbox,
        ApprovalArg::OutsideWorkspace,
    ]
    .into_iter()
    .map(Into::into)
    .collect()
}

fn prompt_error(error: impl std::fmt::Display) -> SafeError {
    SafeError {
        category: "cancelled",
        context: format!("interactive prompt cancelled or unavailable: {error}"),
    }
}

/// Reads the backend inventory through the portable backend contract. The chat
/// remains Ollama-first for now because endpoint construction is still the
/// legacy composition seam; moving this read first prevents the UI from
/// depending on an Ollama-only `models()` method.
async fn discover_model_refs<B: InferenceBackend>(backend: &B) -> Result<Vec<String>, SafeError> {
    let mut models: Vec<String> = backend
        .discover_models()
        .await
        .map_err(provider_error)?
        .into_iter()
        .map(|model| model.model_ref)
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

async fn select_model_from_backend(
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
) -> Result<(), SafeError> {
    let provider = runtime
        .backend(Duration::from_secs(config.timeout_secs))
        .map_err(provider_error)?;
    let models = discover_model_refs(&provider).await?;
    if models.is_empty() {
        return Err(SafeError {
            category: "provider_unavailable",
            context: format!(
                "the {} backend reports no installed models; install one in that backend first",
                provider.backend_id()
            ),
        });
    }
    let default = config
        .model
        .as_ref()
        .and_then(|model| models.iter().position(|candidate| candidate == model))
        .unwrap_or(0);
    let index = Select::new()
        .with_prompt("Choose the model for this chat")
        .items(&models)
        .default(default)
        .interact()
        .map_err(prompt_error)?;
    let selected = models[index].clone();
    select_discovered_model(runtime, config, &models, &selected).await?;
    task_status(
        "✓",
        format!(
            "Model selected: {} — ready for a supervised local task",
            selected
        ),
    );
    if let Some(decision) = compute_context(runtime, config).await? {
        task_status(if decision.computed { "✓" } else { "!" }, decision.line);
    }
    Ok(())
}

/// Apply a model selection only after the caller has read the active backend's
/// catalog. This is shared by the terminal picker and the local app protocol:
/// a client never gets to turn an arbitrary path into a deployment setting.
async fn select_discovered_model(
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
    installed: &[String],
    selected: &str,
) -> Result<bool, SafeError> {
    if !installed.iter().any(|model| model == selected) {
        return Err(SafeError {
            category: "invalid_input",
            context: format!("{selected} is not installed for the active backend"),
        });
    }
    let changed = config.model.as_deref() != Some(selected);
    config.model = Some(selected.to_owned());
    if changed {
        // A calibration is deployment-specific. The selected model remains
        // usable under B.1's computed window, but no old profile may describe
        // it.
        config.profile = None;
        config.prepared_for_model = Some(selected.to_owned());
        config.prepared_without_calibration = true;
    }
    let _ = compute_context(runtime, config).await?;
    Ok(changed)
}

/// Inspects the selected model and computes its working window. This is kept
/// as an explicit control for someone changing hardware or model files; it
/// does not run a generation or a capability probe.
async fn prepare_model_for_agent(
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
) -> Result<(), SafeError> {
    let model = config.model.clone().ok_or_else(|| SafeError {
        category: "invalid_input",
        context: "choose a model before preparing it".into(),
    })?;
    let selection = runtime
        .select(model.clone(), Duration::from_secs(config.timeout_secs))
        .map_err(provider_error)?;
    let provider = selection.backend;
    let deployment = selection.deployment;
    provider
        .inspect(&deployment)
        .await
        .map_err(provider_error)?;
    config.profile = None;
    config.prepared_without_calibration = true;
    config.prepared_for_model = config.model.clone();
    if let Some(decision) = compute_context(runtime, config).await? {
        task_status(if decision.computed { "✓" } else { "!" }, decision.line);
    }
    task_status(
        "✓",
        format!(
            "{model} is ready for agent tasks; its window is computed from the model and this host"
        ),
    );
    Ok(())
}

/// Runs the expensive empirical capability suite only when the operator is
/// measuring a deployment, rather than making an ordinary chat wait for it.
async fn probe_model_capabilities(
    runtime: &RuntimeFactory,
    config: &ChatConfig,
) -> Result<(), SafeError> {
    let model = config.model.clone().ok_or_else(|| SafeError {
        category: "invalid_input",
        context: "choose a model before running its capability probe".into(),
    })?;
    task_status(
        "→",
        format!("Capability probe started for {model} — this can take several minutes"),
    );
    let started = Instant::now();
    wait_with_progress(
        "Capability probe",
        inspect(runtime, model, true, Duration::from_secs(300), 3),
    )
    .await?;
    task_status(
        "✓",
        format!(
            "Capability probe completed in {}s",
            started.elapsed().as_secs()
        ),
    );
    Ok(())
}

async fn wait_with_progress<T, F>(label: &str, operation: F) -> Result<T, SafeError>
where
    F: std::future::Future<Output = Result<T, SafeError>>,
{
    let started = Instant::now();
    tokio::pin!(operation);
    let mut ticker = tokio::time::interval(Duration::from_secs(10));
    ticker.tick().await;
    loop {
        tokio::select! {
            result = &mut operation => {
                finish_progress_row();
                return result;
            },
            _ = ticker.tick() => render_progress(format!("{label} running — {}s elapsed", started.elapsed().as_secs())),
        }
    }
}

async fn select_context(
    runtime: &RuntimeFactory,
    config: &mut ChatConfig,
) -> Result<(), SafeError> {
    let limit = match config.model.as_deref() {
        Some(model) => runtime
            .backend(Duration::from_secs(config.timeout_secs))
            .map_err(provider_error)?
            .discover_models()
            .await
            .map_err(provider_error)?
            .into_iter()
            .find(|candidate| candidate.model_ref == model)
            .and_then(|candidate| candidate.context_limit),
        None => None,
    };
    let ceiling = limit.unwrap_or(262_144);
    // A backend-reported ceiling is only an upper bound.  Once this chat has
    // a calibration profile, present just the tiers that were actually
    // measured stable for this exact model/runtime.  This prevents the TUI
    // from advertising 262k while the run silently falls back to 32k.
    let measured = config
        .profile
        .as_deref()
        .map(load_calibration)
        .transpose()?;
    let mut contexts: Vec<u32> = measured
        .as_ref()
        .map(|profile| {
            profile
                .stable_points
                .iter()
                .filter(|point| {
                    !point.memory_pressure_observed
                        && profile.thresholds.admits(point)
                        && point.context_tokens <= ceiling
                })
                .map(|point| point.context_tokens)
                .collect()
        })
        .unwrap_or_else(|| {
            let mut choices = vec![
                2_048, 4_096, 8_192, 16_384, 32_768, 65_536, 131_072, 196_608, 262_144,
            ];
            choices.retain(|context| *context <= ceiling);
            if choices.is_empty() {
                choices.push(ceiling);
            }
            choices
        });
    contexts.sort_unstable();
    contexts.dedup();
    if contexts.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: "the selected calibration has no stable context tier; prepare the model again"
                .into(),
        });
    }
    let labels: Vec<String> = contexts
        .iter()
        .map(|tokens| {
            if measured.is_some() {
                format!("{tokens} tokens  •  measured stable")
            } else if Some(*tokens) == limit {
                format!("{tokens} tokens  •  engine-reported maximum")
            } else {
                format!("{tokens} tokens")
            }
        })
        .collect();
    let default = contexts
        .iter()
        .position(|tokens| *tokens == config.context_tokens)
        .unwrap_or(0);
    let index = Select::new()
        .with_prompt(format!(
            "Choose context budget ({})",
            if measured.is_some() {
                "only measured stable tiers are shown".to_string()
            } else {
                format!(
                    "model ceiling: {}",
                    limit
                        .map(|limit| limit.to_string())
                        .unwrap_or_else(|| "not reported; calibrate before execution".into())
                )
            }
        ))
        .items(&labels)
        .default(default)
        .interact()
        .map_err(prompt_error)?;
    config.context_tokens = contexts[index];
    Ok(())
}

fn select_timeout(config: &mut ChatConfig) -> Result<(), SafeError> {
    let choices = [60u64, 120, 300, 600, 900, 1_800];
    let labels: Vec<String> = choices
        .iter()
        .map(|seconds| format!("{seconds} seconds"))
        .collect();
    let default = choices
        .iter()
        .position(|seconds| *seconds == config.timeout_secs)
        .unwrap_or(4);
    let index = Select::new()
        .with_prompt("Choose how long a response may wait with nothing from the backend")
        .items(&labels)
        .default(default)
        .interact()
        .map_err(prompt_error)?;
    config.timeout_secs = choices[index];
    Ok(())
}

fn select_profile(root: &Path, config: &mut ChatConfig) -> Result<(), SafeError> {
    let mut profiles: Vec<PathBuf> = fs::read_dir(root.join(".pwr/calibrations"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    profiles.sort();
    let mut labels: Vec<String> = profiles
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    let enter_path = labels.len();
    labels.push("Enter another calibration-file path…".into());
    let back = labels.len();
    labels.push("← Back to controls".into());
    let default = config
        .profile
        .as_ref()
        .and_then(|profile| profiles.iter().position(|candidate| candidate == profile))
        .unwrap_or(back);
    let index = match Select::new()
        .with_prompt("Prepared model state (advanced)")
        .items(&labels)
        .default(default)
        .interact()
    {
        Ok(index) => index,
        // Escape is a local navigation action, not a reason to terminate the
        // whole process while somebody is selecting settings.
        Err(_) => return Ok(()),
    };
    if index == back {
        return Ok(());
    }
    if index == enter_path {
        let path: Result<String, _> = Input::new()
            .with_prompt("Calibration profile path (Esc to go back)")
            .interact_text();
        match path {
            Ok(path) if !path.trim().is_empty() => {
                config.profile = Some(PathBuf::from(path.trim()));
                // A manually selected artifact has not yet been associated
                // with the selected chat model in this settings flow.
                config.prepared_for_model = None;
            }
            Ok(_) | Err(_) => {}
        }
    } else {
        config.profile = Some(profiles[index].clone());
        config.prepared_for_model = None;
    }
    Ok(())
}

/// Says which engine serves this workspace.
///
/// There was a choice between Ollama, LM Studio and MLX until 2026-09-19; now
/// PWR runs models itself, and the entry stays so the console still answers
/// where the model runs.
fn select_backend(runtime: &mut RuntimeFactory, config: &mut ChatConfig) -> Result<(), SafeError> {
    config.backend = Some(runtime.kind().id().to_owned());
    task_status(
        "✓",
        format!(
            "Models run on PWR's {} engine. Ollama and LM Studio are no longer used.",
            runtime.kind().id()
        ),
    );
    Ok(())
}

/// Which kinds of action the conversation stops to ask about.
fn select_ask_before(config: &mut ChatConfig) -> Result<(), SafeError> {
    let kinds = all_approvals();
    let labels: Vec<&str> = kinds.iter().map(|kind| approval_label(*kind)).collect();
    let checked: Vec<bool> = kinds
        .iter()
        .map(|kind| config.ask_before.contains(kind))
        .collect();
    let chosen = MultiSelect::new()
        .with_prompt(
            "Ask before the conversation does these (Space to toggle, Enter to save, Esc to go back)",
        )
        .items(&labels)
        .defaults(&checked)
        .interact_opt()
        .map_err(prompt_error)?;
    let Some(chosen) = chosen else {
        return Ok(());
    };
    config.ask_before = chosen.into_iter().map(|index| kinds[index]).collect();
    println!(
        "The conversation now asks before: {}.",
        if config.ask_before.is_empty() {
            "nothing -- every kind is granted for work in this workspace".to_owned()
        } else {
            config
                .ask_before
                .iter()
                .map(|approval| approval_label(*approval))
                .collect::<Vec<_>>()
                .join("; ")
        }
    );
    Ok(())
}

/// The discoverable settings surface. Arrow keys and Enter are enough for the
/// normal setup flow; slash commands are kept as shortcuts, not prerequisites.
async fn configure_chat_interactively(
    root: &Path,
    runtime: &mut RuntimeFactory,
    config: &mut ChatConfig,
    _messages: &mut Vec<ChatMessage>,
) -> Result<bool, SafeError> {
    loop {
        clear_agent_screen();
        render_chat_header(root, config, runtime.kind());
        let options = [
            "Start / return to task console",
            "Show selected engine",
            "Choose model",
            "Inspect model and recompute its working window",
            "Run capability probe (research diagnostic)",
            "Choose context budget",
            "Choose calibration profile",
            "Choose response timeout",
            "Toggle planning",
            "Choose which actions ask before running",
            "Show current settings",
            "Exit PWR",
        ];
        let selection = Select::new()
            .with_prompt("PWR controls (↑/↓ then Enter)")
            .items(&options)
            .default(0)
            .interact()
            .map_err(prompt_error)?;
        match selection {
            0 => return Ok(false),
            1 => select_backend(runtime, config)?,
            2 => select_model_from_backend(runtime, config).await?,
            3 => prepare_model_for_agent(runtime, config).await?,
            4 => probe_model_capabilities(runtime, config).await?,
            5 => select_context(runtime, config).await?,
            6 => select_profile(root, config)?,
            7 => select_timeout(config)?,
            8 => {
                config.plan = !config.plan;
                println!(
                    "Planning is now {}.",
                    if config.plan { "on" } else { "off" }
                );
            }
            9 => select_ask_before(config)?,
            10 => println!(
                "backend={}, model={}, agent readiness={}, probe=optional research diagnostic, context={} tokens, \
                 timeout={}s, plan={}, asks before: {}",
                runtime.kind().id(),
                config.model.as_deref().unwrap_or("not selected"),
                if chat_is_prepared(config) {
                    "ready"
                } else {
                    "setup required"
                },
                config.context_tokens,
                config.timeout_secs,
                config.plan,
                if config.ask_before.is_empty() {
                    "nothing".to_owned()
                } else {
                    config
                        .ask_before
                        .iter()
                        .map(|approval| approval_label(*approval))
                        .collect::<Vec<_>>()
                        .join("; ")
                }
            ),
            11 => return Ok(true),
            _ => unreachable!("fixed menu selection"),
        }
        save_chat_config(root, config)?;
    }
}

enum TuiExit {
    Quit,
    Settings,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TuiInputMode {
    Task,
    AttachmentPath,
}

struct TuiState {
    input: String,
    attachments: Vec<String>,
    /// The conversation, which is what the console is for. The agent's own
    /// working notes go to `activity` and are shown in a smaller panel beside
    /// it: a task is something the conversation started, not the thing the
    /// operator came here to read.
    transcript: Vec<String>,
    activity: Vec<String>,
    /// True while a turn is working: reading, editing, running checks. There
    /// is one loop now, so there is one thing to be busy with.
    thinking: bool,
    spinner: usize,
    input_mode: TuiInputMode,
    run_id: Option<pwr_domain::Id>,
    seen_run_events: BTreeSet<pwr_domain::Id>,
    /// A question the turn is waiting on, answered with a key.
    pending_approval: Option<tokio::sync::oneshot::Sender<pwr_orchestrator::ApprovalDecision>>,
}

/// A turn asking the console for a decision it cannot make itself.
struct ApprovalRequest {
    approval: pwr_tools::Approval,
    description: String,
    reply: tokio::sync::oneshot::Sender<pwr_orchestrator::ApprovalDecision>,
}

/// Puts an approval to the person at the console and waits for the answer.
///
/// The turn is on another task and the console owns the keyboard, so the
/// question crosses a channel and the answer comes back on a oneshot. A console
/// that has gone away cannot answer, and that is a refusal. "For this session"
/// is remembered in `session_grants`, which every later turn's policy starts
/// from.
struct ConsoleApproval {
    requests: tokio::sync::mpsc::UnboundedSender<ApprovalRequest>,
    session_grants: Arc<std::sync::Mutex<Vec<pwr_tools::Approval>>>,
}

#[async_trait::async_trait]
impl pwr_orchestrator::ApprovalPrompt for ConsoleApproval {
    async fn ask(
        &self,
        approval: pwr_tools::Approval,
        description: &str,
    ) -> pwr_orchestrator::ApprovalDecision {
        let (reply, answer) = tokio::sync::oneshot::channel();
        let asked = ApprovalRequest {
            approval,
            description: description.to_owned(),
            reply,
        };
        if self.requests.send(asked).is_err() {
            return pwr_orchestrator::ApprovalDecision::Deny;
        }
        let decision = answer
            .await
            .unwrap_or(pwr_orchestrator::ApprovalDecision::Deny);
        if decision == pwr_orchestrator::ApprovalDecision::AllowForRun
            && let Ok(mut grants) = self.session_grants.lock()
            && !grants.contains(&approval)
        {
            grants.push(approval);
        }
        decision
    }
}

/// The console's reading of a key while a question is open, if it answers it.
fn approval_key(code: KeyCode) -> Option<pwr_orchestrator::ApprovalDecision> {
    use pwr_orchestrator::ApprovalDecision;
    match code {
        KeyCode::Char('y' | 'Y') => Some(ApprovalDecision::AllowOnce),
        KeyCode::Char('a' | 'A') => Some(ApprovalDecision::AllowForRun),
        KeyCode::Char('n' | 'N') | KeyCode::Esc => Some(ApprovalDecision::Deny),
        _ => None,
    }
}

fn push_tui_transcript(state: &mut TuiState, message: impl Into<String>) {
    const TRANSCRIPT_HISTORY_LIMIT: usize = 400;
    state.transcript.push(message.into());
    if state.transcript.len() > TRANSCRIPT_HISTORY_LIMIT {
        state
            .transcript
            .drain(..state.transcript.len() - TRANSCRIPT_HISTORY_LIMIT);
    }
}

fn push_tui_activity(state: &mut TuiState, message: impl Into<String>) {
    const ACTIVITY_HISTORY_LIMIT: usize = 200;
    state.activity.push(message.into());
    if state.activity.len() > ACTIVITY_HISTORY_LIMIT {
        state
            .activity
            .drain(..state.activity.len() - ACTIVITY_HISTORY_LIMIT);
    }
}

/// How far to scroll so the newest line of the conversation is on screen.
///
/// Wrapped lines are counted, not raw ones: a reply of one long paragraph is
/// several rows, and scrolling by rows it does not occupy leaves the answer
/// above the window.
fn transcript_scroll(lines: &[String], area: ratatui::layout::Rect) -> u16 {
    let width = area.width.saturating_sub(2).max(1) as usize;
    let height = area.height.saturating_sub(2) as usize;
    let rows: usize = lines
        .iter()
        .map(|line| line.chars().count().div_ceil(width).max(1))
        .sum();
    u16::try_from(rows.saturating_sub(height)).unwrap_or(u16::MAX)
}

/// Converts the durable audit into a compact, user-visible activity feed.  It
/// deliberately reports plans, model progress, tools and verification rather
/// than exposing private chain-of-thought text.
fn tui_event_summary(event: &pwr_store::EventRecord) -> Option<String> {
    match event.event_type.as_str() {
        "task.plan" => event.payload["steps"]
            .as_array()
            .map(|steps| format!("→ Agent plan: {} step(s)", steps.len())),
        "turn.generated" => Some(format!(
            "• Model turn {} completed ({} response chars)",
            event.payload["turn"].as_u64().unwrap_or(0),
            event.payload["content_chars"].as_u64().unwrap_or(0)
        )),
        "tool.action" => Some(format!(
            "→ {} · {}",
            event.payload["action"]["capability"]
                .as_str()
                .unwrap_or("tool action"),
            event.payload["status"].as_str().unwrap_or("recorded")
        )),
        "verification.result" => Some(if event.payload["verifiable"].as_bool() == Some(false) {
            "! Verification unavailable — completion will be marked unverified".into()
        } else if event.payload["verified"].as_bool() == Some(true) {
            "✓ Deterministic verification passed".into()
        } else {
            "! Deterministic verification did not pass; agent is recovering".into()
        }),
        "task.complete" => Some(if event.payload["verified"].as_bool() == Some(true) {
            "✓ Agent declared task complete and verified".into()
        } else {
            "✓ Agent declared task complete (unverified)".into()
        }),
        "task.failed" => Some(format!(
            "✗ Agent stopped: {}",
            event.payload["reason"].as_str().unwrap_or("unknown reason")
        )),
        _ => None,
    }
}

fn sync_tui_activity(state: &mut TuiState, root: &Path) {
    let Some(run_id) = state.run_id else {
        return;
    };
    let Ok(store) = pwr_store::Store::open(root.join(".pwr/state.sqlite")) else {
        return;
    };
    let Ok(events) = store.events_for_run(run_id) else {
        return;
    };
    for event in events {
        if !state.seen_run_events.insert(event.id) {
            continue;
        }
        if let Some(summary) = tui_event_summary(&event) {
            push_tui_activity(state, summary);
        }
    }
}

fn abbreviated_line(value: &str, limit: usize) -> String {
    let mut text: String = value.chars().take(limit).collect();
    if value.chars().nth(limit).is_some() {
        text.push('…');
    }
    text
}

fn tui_composer_preview(input: &str) -> String {
    if input.is_empty() {
        return "Type a message…".into();
    }
    let lines: Vec<&str> = input.lines().collect();
    let mut preview: Vec<String> = lines
        .iter()
        .take(2)
        .map(|line| abbreviated_line(line, 150))
        .collect();
    if lines.len() > 2 {
        preview.push("…".into());
    }
    preview.join("\n")
}

fn draw_tui(frame: &mut Frame, state: &TuiState, root: &Path, config: &ChatConfig) {
    let area = frame.area();
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            // The conversation takes what is left; the agent's panel is
            // deliberately small. What the operator came here to read is the
            // reply, and the agent's step-by-step is context for it.
            Constraint::Min(5),
            Constraint::Length(6),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(area);
    let ready = readiness_label(config);
    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            " PWR  •  LOCAL AGENT ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(format!(
            " {}  •  {}  •  Context {}",
            config.model.as_deref().unwrap_or("No model"),
            ready,
            config.context_tokens
        )),
        Line::from(format!(" {}", root.display())).style(Style::default().fg(Color::DarkGray)),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    );
    frame.render_widget(header, layout[0]);
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut transcript = state.transcript.clone();
    if state.thinking {
        transcript.push(format!(
            "{} thinking…",
            frames[state.spinner % frames.len()]
        ));
    }
    let conversation = Paragraph::new(transcript.join("\n"))
        .wrap(Wrap { trim: false })
        .scroll((
            // Kept pinned to the newest line: a conversation that scrolls away
            // from what was just said is worse than no scrolling at all.
            transcript_scroll(&transcript, layout[1]),
            0,
        ))
        .block(
            Block::default()
                .title(" Conversation ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue)),
        );
    frame.render_widget(conversation, layout[1]);
    let activity = state.activity.clone();
    let start = activity
        .len()
        .saturating_sub(layout[2].height.saturating_sub(2) as usize);
    let feed = Paragraph::new(activity[start..].join("\n"))
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .title(" Actions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    frame.render_widget(feed, layout[2]);
    let attachment_label = if state.attachments.is_empty() {
        "none".to_string()
    } else {
        format!("{} queued for next task", state.attachments.len())
    };
    let input_lines = state.input.lines().count().max(1);
    let composer_title = match state.input_mode {
        TuiInputMode::Task => format!(
            " Message · {} chars · {} line{} · attachments: {} ",
            state.input.chars().count(),
            input_lines,
            if input_lines == 1 { "" } else { "s" },
            attachment_label
        ),
        TuiInputMode::AttachmentPath => " Attachment path · Enter to queue · Esc to cancel ".into(),
    };
    let composer = Paragraph::new(tui_composer_preview(&state.input)).block(
        Block::default()
            .title(composer_title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta)),
    );
    frame.render_widget(composer, layout[3]);
    let help = Paragraph::new(match state.input_mode {
        _ if state.pending_approval.is_some() => {
            " Waiting for you:  y allow once  •  a allow for this session  •  n or Esc refuse "
        }
        TuiInputMode::Task if state.thinking => {
            " Type to steer it while it works  •  Enter send  •  Esc stop  •  /changes "
        }
        TuiInputMode::Task => {
            " Enter send  •  Esc quit  •  Tab autocomplete  •  /changes  •  /resume  •  /verify  •  /report  •  /sessions  •  /attach  •  /doctor  •  /settings "
        }
        TuiInputMode::AttachmentPath => {
            " Paste or type a file path  •  Tab autocomplete  •  Enter queue  •  Esc cancel "
        }
    })
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help, layout[4]);
    if !state.input.contains('\n') && state.input.chars().count() < 150 {
        frame.set_cursor_position((
            (layout[3].x + 1 + state.input.chars().count() as u16)
                .min(layout[3].right().saturating_sub(2)),
            layout[3].y + 1,
        ));
    }
}

fn complete_attachment_path(input: &mut String, root: &Path) {
    let parsed = parse_attachment_path(input);
    let absolute = if parsed.is_absolute() {
        parsed
    } else {
        root.join(parsed)
    };
    let (directory, prefix) = if input.ends_with('/') {
        (absolute, String::new())
    } else {
        (
            absolute.parent().unwrap_or(root).to_path_buf(),
            absolute
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string(),
        )
    };
    let Ok(entries) = fs::read_dir(&directory) else {
        return;
    };
    let Some(candidate) = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
    else {
        return;
    };
    let display = if let Ok(relative) = candidate.strip_prefix(root) {
        relative.to_path_buf()
    } else {
        candidate
    };
    *input = display.display().to_string();
    if Path::new(input).is_dir() {
        input.push('/');
    }
}

fn complete_tui_input(input: &mut String, root: &Path, mode: TuiInputMode) {
    if mode == TuiInputMode::AttachmentPath {
        complete_attachment_path(input, root);
        return;
    }
    // Tab completion is how these are found. A command that works and cannot
    // be discovered is the same defect as a capability that needs a remembered
    // flag, one level up.
    const COMMANDS: [&str; 12] = [
        "/attach ",
        "/attachments",
        "/changes",
        "/clear-attachments",
        "/diagnose",
        "/doctor",
        "/report",
        "/resume",
        "/session ",
        "/sessions",
        "/settings",
        "/verify",
    ];
    if input.starts_with("/attach ") {
        let mut path = input.trim_start_matches("/attach ").to_string();
        complete_attachment_path(&mut path, root);
        *input = format!("/attach {path}");
    } else if let Some(command) = COMMANDS
        .iter()
        .find(|command| command.starts_with(input.as_str()))
    {
        *input = (*command).into();
    }
}

type ChatTurnResult = Result<(converse::TurnReport, Vec<ChatMessage>), String>;

/// A conversation brought back from the log, ready for the console.
struct ResumedConversation {
    conversation_id: pwr_domain::Id,
    messages: Vec<ChatMessage>,
    checkpoint: pwr_orchestrator::conversation::Checkpoint,
    /// What differs from what the conversation recorded, as the deployment and
    /// the person are told it. `None` when nothing does.
    note: Option<String>,
}

/// How much of a diff the console shows before saying the rest is there.
const CHANGES_SHOWN_CHARS: usize = 8_000;

/// What the conversation has changed, said for a person.
///
/// `diff` is git's view of those paths, when the workspace has one. A file the
/// conversation created and nobody has added to git has no diff, and a
/// workspace that is not a repository has none at all, so the list of files is
/// always said and the diff is added where it exists.
fn changes_summary(paths: &[String], diff: Option<&str>) -> String {
    if paths.is_empty() {
        return "This conversation has not changed any file yet.".to_owned();
    }
    let mut said = format!(
        "Changed by this conversation ({} file(s)): {}",
        paths.len(),
        paths.join(", ")
    );
    match diff.map(str::trim) {
        Some("") => said.push_str(
            "\ngit shows no uncommitted difference for them: they are committed, unchanged \
             since, or new and not yet added to git.",
        ),
        Some(diff) => {
            let cut = (0..=CHANGES_SHOWN_CHARS.min(diff.len()))
                .rev()
                .find(|at| diff.is_char_boundary(*at))
                .unwrap_or(0);
            said.push('\n');
            said.push_str(&diff[..cut]);
            if cut < diff.len() {
                said.push_str(&format!(
                    "\n… {} more characters; `git diff` shows the rest.",
                    diff.len() - cut
                ));
            }
        }
        None => {
            said.push_str("\nThis workspace is not a git repository, so there is no diff to show.")
        }
    }
    said
}

/// The console's `/changes`: the files the conversation changed, and git's
/// diff of them. Read-only, and under the same policy boundary as any tool.
async fn conversation_changes(root: &Path, changed: &BTreeMap<String, String>) -> String {
    let paths: Vec<String> = changed.keys().cloned().collect();
    if paths.is_empty() {
        return changes_summary(&paths, None);
    }
    let policy = pwr_tools::ToolPolicy {
        root: root.to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec!["git".into()],
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(20),
        sandbox: pwr_tools::SandboxPolicy::Preferred,
        approvals: Vec::new(),
    };
    let diff = match pwr_tools::vcs_diff(&policy, &paths).await {
        Ok(result) if result.exit_code == Some(0) => Some(result.stdout),
        _ => None,
    };
    changes_summary(&paths, diff.as_deref())
}

/// Replaces the console's conversation with the latest one in the log, and
/// says what was found.
fn resume_into(
    root: &Path,
    state: &mut TuiState,
    messages: &mut Vec<ChatMessage>,
    conversation_id: &mut pwr_domain::Id,
    continuity: &converse::Continuity,
) {
    match resume_latest_conversation(root) {
        Ok(Some(resumed)) => {
            let turns = resumed.checkpoint.turn;
            *conversation_id = resumed.conversation_id;
            *messages = resumed.messages;
            if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
                *checkpoint = resumed.checkpoint;
            }
            push_tui_transcript(
                state,
                format!(
                    "Continuing the last conversation ({} message(s), {turns} turn(s)).",
                    messages.len()
                ),
            );
            match resumed.note {
                Some(note) => push_tui_transcript(state, format!("PWR: {note}")),
                None => push_tui_activity(state, "✓ the workspace matches what it recorded"),
            }
        }
        Ok(None) => push_tui_transcript(
            state,
            "There is no earlier conversation in this workspace to continue.",
        ),
        Err(error) => push_tui_transcript(state, format!("could not continue: {error}")),
    }
}

/// The latest conversation in `root`'s log, reconciled against the workspace.
fn resume_latest_conversation(root: &Path) -> Result<Option<ResumedConversation>, String> {
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
        .map_err(|error| error.to_string())?;
    let Some(conversation_id) = pwr_orchestrator::conversation::latest(&store)? else {
        return Ok(None);
    };
    Ok(
        pwr_orchestrator::conversation::resume(root, &store, conversation_id)?.map(|resumed| {
            ResumedConversation {
                conversation_id,
                messages: resumed.messages,
                checkpoint: resumed.checkpoint,
                note: resumed.note,
            }
        }),
    )
}

/// What a turn reports about itself while it is still running.
///
/// Sent rather than accumulated. Collected into a list and returned at the end,
/// nothing about a long turn reached the operator until it finished -- so a
/// turn that read the same file forty times, compacted its own context twice
/// and gave up on the specification looked, from the console, exactly like a
/// turn that was thinking. Three runs were spent working out from the audit
/// what a live feed would have said as it happened.
/// Where a turn's steps go as it works. Called on the turn's own task, in
/// order: a front end that queues them instead has to keep that order itself,
/// and the protocol server's first attempt did not -- a permission request
/// reached the client before the action it was asking about.
type StepSink = Box<dyn FnMut(converse::TurnStep)>;

/// The console's own turn, run for a protocol client.
struct ConsoleTurns {
    runtime: RuntimeFactory,
}

#[async_trait::async_trait(?Send)]
impl serve::TurnRunner for ConsoleTurns {
    fn goal_limits(&self, root: &Path) -> Result<serve::GoalLimits, String> {
        // The workspace's own limits; no Gemma pack turns the proposals
        // phase on by itself (it was measured to help Ornith 1.5 9B only).
        Ok(load_chat_config(root)
            .map_err(|error| error.context)?
            .goal_budget)
    }
    fn goal_aids(&self, root: &Path) -> serve::GoalAids {
        load_chat_config(root)
            .map(|config| config.goal_aids)
            .unwrap_or_default()
    }
    fn persist_checkpoint(
        &self,
        root: &Path,
        id: pwr_domain::Id,
        checkpoint: &pwr_orchestrator::conversation::Checkpoint,
    ) -> Result<(), String> {
        let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
            .map_err(|error| error.to_string())?;
        pwr_orchestrator::conversation::record_checkpoint(&store, id, checkpoint)
    }
    async fn open(&self, root: &Path) -> Result<Vec<ChatMessage>, String> {
        let root = self.ready(root).await?;
        // A new chat reads only what is attached to it: the folders of the
        // last one are not this one's.
        if is_chat_home(&root) {
            let mut config = load_chat_config(&root).map_err(|error| error.context)?;
            if !config.reference_roots.is_empty() {
                config.reference_roots.clear();
                save_chat_config(&root, &config).map_err(|error| error.context)?;
            }
        }
        Ok(vec![ChatMessage::text(
            "system",
            chat_system_prompt_for(&root),
        )])
    }

    async fn list(
        &self,
        root: &Path,
    ) -> Result<Vec<pwr_orchestrator::conversation::Listed>, String> {
        match conversation_store(root)? {
            Some(store) => pwr_orchestrator::conversation::list(&store),
            None => Ok(Vec::new()),
        }
    }

    async fn resume(
        &self,
        root: &Path,
        id: pwr_domain::Id,
    ) -> Result<Option<pwr_orchestrator::conversation::Resumed>, String> {
        let root = self.ready(root).await?;
        match conversation_store(&root)? {
            Some(store) => pwr_orchestrator::conversation::resume(&root, &store, id),
            None => Ok(None),
        }
    }

    async fn delete(&self, root: &Path, id: pwr_domain::Id) -> Result<bool, String> {
        match conversation_store(root)? {
            Some(store) => pwr_orchestrator::conversation::delete(&store, id),
            None => Ok(false),
        }
    }

    fn attach(&self, root: &Path, attachment: serve::Attachment) -> Result<String, String> {
        match attachment {
            serve::Attachment::File(path) => attach_chat_file(root, &path),
            serve::Attachment::Embedded { uri, bytes } => attach_chat_bytes(root, &uri, bytes),
        }
        .map(|message| message.content)
        .map_err(|error| error.context)
    }

    fn chat_home(&self) -> Option<PathBuf> {
        chat_home().ok()
    }

    fn store_image(
        &self,
        root: &Path,
        image: &serve::PromptImage,
    ) -> Result<std::path::PathBuf, String> {
        let config = load_chat_config(root).map_err(|error| error.context)?;
        let model = config
            .model
            .ok_or("choose a model before attaching an image")?;
        // The model's own configuration says whether it carries an encoder;
        // only the MLX engine reads images (C.25). A model it cannot find is
        // one it cannot show an image to either.
        if pwr_mlx::MlxConfig::from_env().has_vision_encoder(&model) != Some(true) {
            return Err(format!(
                "{model} cannot read images; choose a model marked \"sees images\""
            ));
        }
        let extension = image.extension().ok_or("unsupported image type")?;
        let digest = {
            use sha2::Digest as _;
            format!("{:x}", sha2::Sha256::digest(&image.bytes))
        };
        let dir = root.join(".pwr/images");
        std::fs::create_dir_all(&dir).map_err(|error| format!("{}: {error}", dir.display()))?;
        let path = dir.join(format!("{digest}.{extension}"));
        if !path.exists() {
            std::fs::write(&path, &image.bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
        path.canonicalize()
            .map_err(|error| format!("{}: {error}", path.display()))
    }

    async fn settings(
        &self,
        root: &Path,
        request: serve::SettingsRequest,
    ) -> Result<serde_json::Value, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", root.display()))?;
        let mut config = load_chat_config(&root).map_err(|error| error.context)?;
        match request {
            serve::SettingsRequest::WikiSummaries { enabled } => {
                if let Some(enabled) = enabled {
                    config.background_summaries = enabled;
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                }
                Ok(serde_json::json!({"enabled":config.background_summaries}))
            }
            serve::SettingsRequest::Models {
                selected,
                context_tokens,
                thinking,
            } => {
                let preparation_requested = selected.is_some() || context_tokens.is_some();
                if let Some(on) = thinking {
                    config.thinking = on;
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                }
                // What the backend has installed, or why it could not say: an
                // unreachable backend is itself what the client needs to show.
                let installed = match self
                    .runtime
                    .backend(Duration::from_secs(config.timeout_secs))
                {
                    // The Gemma line only: other models in the folder, and
                    // Google's assistant drafts, are not offered.
                    Ok(provider) => discover_model_refs(&provider).await.map(|models| {
                        let line = models
                            .into_iter()
                            .filter(|model| pwr_domain::gemma::pack(model).is_some())
                            .collect::<Vec<_>>();
                        (provider.backend_id(), line)
                    }),
                    Err(error) => Err(provider_error(error)),
                };
                if let Some(requested) = context_tokens {
                    if !(2_048..=262_144).contains(&requested) {
                        return Err("contextTokens must be between 2048 and 262144".into());
                    }
                    if config.model.is_none() {
                        return Err("choose a model before setting contextTokens".into());
                    }
                    // Recorded as the person's setting; the computation below
                    // applies it, capped by what the host can hold, and
                    // prepares the backend once.
                    config.context_setting = Some(requested);
                }
                let (backend, installed, unavailable) = match installed {
                    Ok((backend, models)) => (Some(backend), Some(models), None),
                    Err(error) => (None, None, Some(error.context)),
                };
                let selection_changed = if let Some(selected) = selected {
                    let installed = installed.as_ref().ok_or_else(|| {
                        unavailable.clone().unwrap_or_else(|| {
                            "the active backend did not return its model catalog".into()
                        })
                    })?;
                    let changed =
                        select_discovered_model(&self.runtime, &mut config, installed, &selected)
                            .await
                            .map_err(|error| error.context)?;
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                    changed
                } else {
                    hydrate_chat_readiness(&root, &self.runtime, &mut config, false)
                        .await
                        .map_err(|error| error.context)?;
                    false
                };
                // This is deliberately observational: refreshing the app must
                // not hash a 20 GiB GGUF or start a network operation. The
                // downloader remains the only operation that verifies and
                // renames files.
                let download_status = artifact_download_status().map_err(|error| error.context)?;
                let window_before = config.context_tokens;
                let context_decision = match compute_context(&self.runtime, &mut config).await {
                    Ok(decision) => decision.map(|decision| {
                        serde_json::json!({
                            "computed": decision.computed,
                            "requestedTokens": decision.requested_tokens,
                            "grantedTokens": decision.granted_tokens,
                            "bindingCeiling": decision.binding_ceiling,
                            "contextMemoryBudgetBytes": decision.context_memory_budget_bytes,
                            "rationale": decision.line,
                            "setting": config.context_setting,
                        })
                    }),
                    Err(error) if preparation_requested => return Err(error.context),
                    // Inventory must remain available so an unavailable saved
                    // model can be replaced. Report the failure without turning
                    // a requested window into an acknowledged grant.
                    Err(error) => Some(serde_json::json!({
                        "computed": false,
                        "grantedTokens": window_before,
                        "rationale": format!("Context preparation unavailable: {}; retaining the previous observed window", error.context),
                        "setting": config.context_setting,
                    })),
                };
                // What is in force is what the next turn reads: a window
                // computed here and not saved was a window no turn used.
                if context_tokens.is_some() || config.context_tokens != window_before {
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                }
                let line = pwr_domain::gemma::PACKS
                    .iter()
                    .map(|pack| {
                        serde_json::json!({
                            "model": pack.model_ref,
                            "label": pack.label,
                            "tier": pack.tier,
                            "tierMemoryGib": pack.tier.memory_gib(),
                            "weightsBytes": pack.weights_bytes,
                            "installed": installed
                                .as_ref()
                                .is_some_and(|models| models.iter().any(|model| model == pack.model_ref)),
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(serde_json::json!({
                    "backend": backend.map(str::to_owned).or(config.backend.clone()),
                    "installed": installed,
                    // Every pack of the line reads images.
                    "vision": installed.clone().unwrap_or_default(),
                    "backendUnavailable": unavailable,
                    "model": config.model,
                    "selectionChanged": selection_changed,
                    "prepared": chat_is_prepared(&config),
                    "contextTokens": config.context_tokens,
                    "thinking": config.thinking,
                    "thoughtCapTokens": pwr_domain::gemma::THOUGHT_CAP,
                    "contextSource": "core_backend",
                    "contextDecision": context_decision,
                    "downloadStatus": download_status,
                    "line": line,
                }))
            }
            serve::SettingsRequest::Context { compact_at_percent } => {
                if let Some(percent) = compact_at_percent {
                    config.compact_at_percent =
                        (percent != (converse::COMPACT_AT * 100.0) as u8).then_some(percent);
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                }
                Ok(serde_json::json!({
                    "window": config.context_tokens,
                    "model": config.model,
                    "compactAtPercent": config
                        .compact_at_percent
                        .unwrap_or((converse::COMPACT_AT * 100.0) as u8),
                    "compactAtCustom": config.compact_at_percent.is_some(),
                    "compactCeilingTokens": config.compact_ceiling_tokens,
                }))
            }
            serve::SettingsRequest::Approvals { ask_before, mode } => {
                let mut changed = false;
                if let Some(mut kinds) = ask_before {
                    kinds.sort();
                    kinds.dedup();
                    config.ask_before = kinds;
                    changed = true;
                }
                if let Some(mode) = mode {
                    config.permission_mode = Some(mode);
                    changed = true;
                }
                if changed {
                    save_chat_config(&root, &config).map_err(|error| error.context)?;
                }
                // Whether a command this workspace runs is actually confined.
                // `Preferred` runs unconfined where the platform has no
                // sandbox, and until 2026-09-23 nothing said so (backlog R.3).
                let sandboxed = pwr_tools::ToolPolicy {
                    sandbox: sandbox_for(&config),
                    ..pwr_tools::PolicyProfile::Safe.build(root.clone())
                }
                .will_sandbox()
                .unwrap_or(false);
                Ok(serde_json::json!({
                    "mode": config.permission_mode.unwrap_or(PermissionMode::Ask),
                    "sandboxed": sandboxed,
                    "askBefore": config.ask_before,
                    // What is actually asked about now: nothing in auto.
                    "asking": effective_ask_before(&config),
                    "kinds": all_approvals()
                        .into_iter()
                        .map(|kind| serde_json::json!({"kind": kind, "label": approval_label(kind)}))
                        .collect::<Vec<_>>(),
                }))
            }
        }
    }

    async fn download(
        &self,
        _root: &Path,
        request: serve::DownloadRequest,
        mut progress: serve::DownloadProgress,
        stop: Arc<AtomicBool>,
    ) -> Result<serde_json::Value, pwr_models::download::DownloadError> {
        let serve::DownloadRequest::Artifact(artifact) = request;
        download_artifact(artifact, None, &mut progress, &stop).await
    }

    async fn hardware(&self) -> Result<serde_json::Value, String> {
        let active = self.runtime.kind();
        let host = pwr_runtime::host::detect_host(&pwr_runtime::models_root(active)).await;
        let backends = pwr_runtime::backend_status(active).await;
        Ok(serde_json::json!({"host": host, "backends": backends}))
    }

    /// The Gemma line as this Mac sees it: each pack, whether it is here,
    /// how much of the 262,144-token context this machine's memory holds for
    /// it, and the registry entry it downloads from.
    async fn local_models(&self, root: &Path) -> Result<serde_json::Value, String> {
        use pwr_models::catalog::Format;
        let config = root
            .canonicalize()
            .ok()
            .and_then(|root| load_chat_config(&root).ok())
            .unwrap_or_default();
        let models_root = pwr_runtime::models_root(BackendKind::Mlx);
        let host = pwr_runtime::host::detect_host(&models_root).await;
        let memory = host.memory.total_bytes;
        let local = pwr_models::local::list(&models_root, Format::Mlx);
        let found = |model_ref: &str| local.iter().find(|model| model.model_ref == model_ref);
        let registry =
            load_artifact_registry(Path::new(ARTIFACT_FILE)).map_err(|error| error.context)?;
        let packs = pwr_domain::gemma::PACKS
            .iter()
            .map(|pack| {
                let here = found(pack.model_ref);
                let window = memory.map(|memory| pwr_domain::gemma::window(pack, memory, 16));
                serde_json::json!({
                    "model": pack.model_ref,
                    "label": pack.label,
                    "tier": pack.tier,
                    "tierMemoryGib": pack.tier.memory_gib(),
                    "weightsBytes": pack.weights_bytes,
                    "artifact": registry
                        .artifacts
                        .iter()
                        .find(|artifact| matches!(&artifact.source,
                            pwr_domain::ArtifactSource::HuggingFace { repository, .. } if repository == pack.model_ref))
                        .map(|artifact| artifact.id.clone()),
                    "installed": here.is_some_and(|model| !model.partial),
                    "partial": here.is_some_and(|model| model.partial),
                    "bytesOnDisk": here.map_or(0, |model| model.bytes),
                    "inUse": config.model.as_deref() == Some(pack.model_ref),
                    // The context this machine holds for it; the full
                    // context is the line's requirement.
                    "windowTokens": window,
                    "fullContext": window == Some(pwr_domain::gemma::CONTEXT_TOKENS),
                    "neededBytes": pwr_domain::gemma::memory_for(pack, pwr_domain::gemma::CONTEXT_TOKENS, 16),
                    "assistant": {
                        "model": pack.assistant,
                        "installed": found(pack.assistant).is_some_and(|model| !model.partial),
                    },
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "memoryBytes": memory,
            "contextTokens": pwr_domain::gemma::CONTEXT_TOKENS,
            "modelsRoot": models_root,
            "packs": packs,
        }))
    }

    async fn delete_model(
        &self,
        root: &Path,
        format: pwr_models::catalog::Format,
        model_ref: &str,
    ) -> Result<serde_json::Value, String> {
        use pwr_models::catalog::Format;
        let kind = match format {
            Format::Mlx => BackendKind::Mlx,
            Format::Gguf => BackendKind::Llama,
        };
        let config = root
            .canonicalize()
            .ok()
            .and_then(|root| load_chat_config(&root).ok())
            .unwrap_or_default();
        // The model a workspace runs on is not pulled from under it: its
        // next turn would fail with a file that is gone.
        if kind == self.runtime.kind() && config.model.as_deref() == Some(model_ref) {
            return Err(format!(
                "{model_ref} is the model this workspace uses. Choose another model first, then delete it."
            ));
        }
        let deleted =
            pwr_models::local::delete(&pwr_runtime::models_root(kind), format, model_ref)?;
        serde_json::to_value(deleted).map_err(|error| error.to_string())
    }

    fn record_compaction(
        &self,
        root: &Path,
        conversation_id: pwr_domain::Id,
        compaction: &pwr_orchestrator::compaction::Compaction,
        messages: &[ChatMessage],
    ) -> Result<(), String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", root.display()))?;
        let config = load_chat_config(&root).map_err(|error| error.context)?;
        fs::create_dir_all(root.join(".pwr")).map_err(|error| error.to_string())?;
        let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
            .map_err(|error| error.to_string())?;
        pwr_orchestrator::compaction::record(
            &store,
            conversation_id,
            compaction,
            config.model.as_deref().unwrap_or("none"),
            config.context_tokens,
        )?;
        // So a reload resumes from the compacted conversation, as it would
        // after an automatic compaction at the end of a turn.
        pwr_orchestrator::conversation::record_snapshot(&store, conversation_id, messages)
    }

    fn record_rewind(
        &self,
        root: &Path,
        conversation_id: pwr_domain::Id,
        messages: &[ChatMessage],
        detail: &serde_json::Value,
    ) -> Result<(), String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", root.display()))?;
        fs::create_dir_all(root.join(".pwr")).map_err(|error| error.to_string())?;
        let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
            .map_err(|error| error.to_string())?;
        store
            .append(
                Some(conversation_id),
                pwr_orchestrator::conversation::REWOUND_EVENT,
                detail.clone(),
            )
            .map_err(|error| error.to_string())?;
        // So a reload resumes from where the person went back to.
        pwr_orchestrator::conversation::record_snapshot(&store, conversation_id, messages)
    }

    fn last_compaction(
        &self,
        root: &Path,
        conversation_id: pwr_domain::Id,
    ) -> Option<serde_json::Value> {
        let store = conversation_store(root).ok()??;
        pwr_orchestrator::compaction::last_recorded(&store, conversation_id)
            .ok()
            .flatten()
    }

    fn last_generation(
        &self,
        root: &Path,
        conversation_id: pwr_domain::Id,
    ) -> Option<serde_json::Value> {
        let store = conversation_store(root).ok()??;
        converse::last_generation(&store, conversation_id)
            .ok()
            .flatten()
    }

    async fn command(
        &self,
        command: serve::Command,
        context: serve::CommandContext,
    ) -> Result<String, String> {
        let root = context
            .root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", context.root.display()))?;
        let id = context.conversation_id.to_string();
        Ok(match command {
            serve::Command::Changes => conversation_changes(&root, &context.changed_files).await,
            serve::Command::Verify => summarise_verification(
                &verify_in(
                    &root,
                    None,
                    "targeted".into(),
                    session_approvals(&root, &context),
                )
                .await
                .map_err(|error| error.context)?,
            ),
            serve::Command::Report => summarise_session(
                &report_in(&root, id, "json".into()).map_err(|error| error.context)?,
            ),
            serve::Command::Diagnose => {
                summarise_diagnosis(&diagnose_in(&root, id).map_err(|error| error.context)?)
            }
            serve::Command::Doctor => {
                summarise_doctor(&doctor(&self.runtime).await.map_err(|error| error.context)?)
            }
        })
    }

    fn background_summaries(&self, root: &Path) -> bool {
        load_chat_config(root)
            .map(|config| config.background_summaries)
            .unwrap_or(false)
    }
    async fn summarise_cancellable(
        &self,
        root: &Path,
        prompt: String,
        cancel: pwr_provider::Cancel,
    ) -> Result<(String, String), String> {
        self.aside(root, "You write short, factual descriptions of source code for a project wiki. Describe only what the code shows.", prompt, 400, 16_384, None, Some(cancel)).await
    }
    async fn summarise(&self, root: &Path, prompt: String) -> Result<(String, String), String> {
        // Short, factual and without reasoning: a summary is a few sentences,
        // and a model thinking for minutes about one would hold the engine
        // the person's next message is waiting for.
        self.aside(
            root,
            "You write short, factual descriptions of source code for a project wiki. \
             Describe only what the code shows.",
            prompt,
            400,
            16_384,
            None,
            None,
        )
        .await
    }

    async fn author(
        &self,
        root: &Path,
        prompt: String,
        think: bool,
    ) -> Result<pwr_provider::ModelStream, String> {
        let root = self.ready(root).await?;
        let config = load_chat_config(&root).map_err(|error| error.context)?;
        let model = config.model.clone().ok_or("no model is selected")?;
        let selection = self
            .runtime
            .select(model, Duration::from_secs(config.timeout_secs))
            .map_err(|error| error.to_string())?;
        // The sampling a turn of this model would use, resolved the same way.
        // Without it the request carried no temperature, the MLX engine read
        // that as zero, and a second request for a file repeated the first
        // word for word (product-path run, 2026-10-06): the phase keeps the
        // better of several attempts and had one attempt, several times.
        let mut sampling = pwr_domain::gemma::sampling();
        sampling.extend([
            ("think".to_owned(), serde_json::json!(think)),
            // No reasoning cap: the governor reading the stream ends the
            // thinking, judged against the file asked for. A cap of 1,024
            // tokens moved the analysis into the answer instead (W2.9).
            ("reasoning_budget".to_owned(), serde_json::Value::Null),
            ("max_tokens".to_owned(), serde_json::json!(8_192)),
            // Beside the conversation's cache, like a review.
            ("aside".to_owned(), serde_json::json!(true)),
            // A file being corrected repeats most of its own tokens, and is
            // asked to: a penalty on tokens already present works against
            // "copy every other line unchanged". With the 1.5 PWR resolves for
            // Ornith 1.5 from its card's general-use setting, 28-line files
            // came back as 94 and 144 lines and none helped (product path,
            // 2026-10-06); the diagnostics that worked had it at zero.
            ("presence_penalty".to_owned(), serde_json::json!(0.0)),
            ("repetition_penalty".to_owned(), serde_json::json!(1.0)),
        ]);
        let request = ModelRequest {
            deployment: selection.deployment.clone(),
            context_tokens: config.context_tokens.min(32_768),
            tools: None,
            seed: None,
            sampling,
            messages: vec![
                ChatMessage::text(
                    "system",
                    "You write one source file. You are given the project contract, read-only \
                     files, what the project's checks really printed, and the file as it stands \
                     when it exists. Reply with the complete file in one fenced block. The tests \
                     are right. When the file exists, change only what the failing tests and the \
                     contract require and copy every other line unchanged. You cannot call tools; \
                     the tests are run on your file before it is kept.",
                ),
                ChatMessage::text("user", prompt),
            ],
        };
        selection
            .backend
            .chat_cancellable(request, pwr_provider::Cancel::new())
            .await
            .map_err(|error| error.to_string())
    }

    async fn review(&self, root: &Path, mut prompt: String) -> Result<String, String> {
        // What the built site shows, read from a browser, beside the code
        // (`pwr_tools::site`). The look runs in the sandbox whatever the
        // session's mode: it only opens pages this machine serves.
        let looking = pwr_tools::PolicyProfile::Safe.build(root.to_path_buf());
        if let Some(pages) = pwr_tools::site::shown(&looking).await {
            prompt.push_str(&format!(
                "\n\nWhat the built site shows in a browser, page by page -- the text a visitor \
                 reads:\n{pages}\nJudge every rule about what a page shows, says or must not say \
                 against this text as well as against the code. A heading or sentence shown here \
                 that the request excludes -- a framework's placeholder, text in another language \
                 than the one asked for -- is NOT MET wherever in the code it comes from, and so \
                 is something the request asks to be shown that no page shows."
            ));
        }
        self.aside(
            root,
            "You review code someone else wrote against the specification it was written to. \
             You did not write it and have no stake in it being finished. Evaluate every rule \
             requested in the checklist as MET or NOT MET, quoting the rule and citing the \
             relevant code. Report actual behavior, not style or speculative violations; \
             distinguish changes to local copies from mutations of supplied inputs.",
            prompt,
            6_000,
            32_768,
            // Reasoning on, bounded. Measured 2026-09-27 on the c2 workspaces
            // that failed on a README rule: without it the reviewer found the
            // missed rule in one of five, with it in two (and a third real
            // gap), at about 75 s a review; neither flagged anything in the
            // two passing controls.
            Some(4_000),
            None,
        )
        .await
        .map(|(text, _)| text)
    }

    async fn prepare_test_recipe(
        &self,
        context: serve::CommandContext,
    ) -> pwr_orchestrator::test_recipe::Report {
        let root = &context.root;
        let policy = pwr_tools::ToolPolicy {
            root: root.clone(),
            extra_readable: pwr_verify::declared_readable(root)
                .into_iter()
                .chain(pwr_tools::dependency_roots(root))
                .collect(),
            protected: match frozen_paths(root) {
                Ok(paths) => paths,
                Err(_) => {
                    return pwr_orchestrator::test_recipe::Report::absent(
                        "protected paths unavailable",
                    );
                }
            },
            allow_commands: pwr_verify::required_executables(root),
            output_limit: 64 * 1024,
            timeout: Duration::from_secs(30),
            sandbox: sandbox_for(&load_chat_config(root).unwrap_or_default()),
            approvals: session_approvals(root, &context),
        };
        pwr_orchestrator::test_recipe::prepare(&policy).await
    }

    async fn verify_goal(
        &self,
        context: serve::CommandContext,
    ) -> Result<serve::GoalVerification, String> {
        if let Some(before) = context.acceptance_snapshot.as_ref() {
            let changed = pwr_verify::acceptance::changed_contract(
                &context.root,
                before,
                &context.authorized_acceptance_changes,
            )?;
            if !changed.is_empty() {
                return Ok(serve::GoalVerification {
                    summary: format!(
                        "Acceptance contract changed: {}. Human review is required before this evidence can certify a goal.",
                        changed.join(", ")
                    ),
                    contract_changed: changed,
                    ..Default::default()
                });
            }
        }
        let acceptance = pwr_verify::declared_acceptance_checks(&context.root)?;
        let report = verify_in(
            &context.root,
            None,
            "full".into(),
            session_approvals(&context.root, &context),
        )
        .await
        .map_err(|error| error.context)?;
        if let Some(before) = context.acceptance_snapshot.as_ref() {
            let changed = pwr_verify::acceptance::changed_contract(
                &context.root,
                before,
                &context.authorized_acceptance_changes,
            )?;
            if !changed.is_empty() {
                return Ok(serve::GoalVerification {
                    summary: format!(
                        "Acceptance contract changed: {}. Human review is required before this evidence can certify a goal.",
                        changed.join(", ")
                    ),
                    contract_changed: changed,
                    ..Default::default()
                });
            }
        }
        let no_tests: Vec<String> = report
            .get("baseline")
            .and_then(|baseline| baseline.get("checks"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|check| {
                let command = check["command"].as_str().unwrap_or_default();
                let output = format!(
                    "{}\n{}",
                    check["result"]["stdout"].as_str().unwrap_or_default(),
                    check["result"]["stderr"].as_str().unwrap_or_default()
                );
                check["result"]["exit_code"] == 0
                    && (pwr_verify::evidence::ran_zero_tests(command, &output)
                        || pwr_verify::evidence::passed_without_tests(command, &output))
            })
            .map(|check| {
                format!(
                    "{} ran no tests",
                    check["command"].as_str().unwrap_or("cargo test")
                )
            })
            .collect();
        let technical_passed = report["verified"].as_bool().unwrap_or(false) && no_tests.is_empty();
        let acceptance_available = !acceptance.is_empty()
            && context.acceptance_contract_hash.is_some()
            && (context.acceptance_contract_hash == serve::acceptance_contract_hash(&context.root)
                || !context.authorized_acceptance_changes.is_empty());
        let summary = if no_tests.is_empty() {
            summarise_verification(&report)
        } else {
            format!(
                "{}\n{}",
                summarise_verification(&report),
                no_tests.join("; ")
            )
        };
        let mut failing: Vec<String> = report
            .get("baseline")
            .and_then(|baseline| baseline.get("checks"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|check| {
                check
                    .get("result")
                    .and_then(|result| result.get("exit_code"))
                    .and_then(serde_json::Value::as_i64)
                    != Some(0)
            })
            .map(|check| {
                check
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unnamed")
                    .to_owned()
            })
            .collect();
        failing.extend(no_tests);
        // The owner's acceptance checks among them: the goal itself, which a
        // goal is never allowed to leave failing because it failed before.
        let acceptance_commands: Vec<String> = acceptance
            .iter()
            .map(|(executable, args)| {
                std::iter::once(executable.as_str())
                    .chain(args.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let failing_acceptance = failing
            .iter()
            .filter(|command| {
                acceptance_commands.iter().any(|accepted| {
                    accepted == *command
                        || accepted == command.strip_suffix(" ran no tests").unwrap_or(command)
                })
            })
            .cloned()
            .collect();
        let mut failure_fingerprints: Vec<String> = report
            .get("baseline")
            .and_then(|baseline| baseline.get("checks"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|check| check["result"]["exit_code"] != 0)
            .map(|check| {
                format!(
                    "{}:{}",
                    check["command"].as_str().unwrap_or("unknown"),
                    pwr_verify::failure::fingerprint(
                        check["result"]["stdout"].as_str().unwrap_or_default(),
                        check["result"]["stderr"].as_str().unwrap_or_default()
                    )
                    .digest
                )
            })
            .collect();
        failure_fingerprints.sort();
        // What a proposal is judged by and shown (plan W2.9): the failing
        // tests by name, and the failing checks' own output.
        let mut failed_tests = BTreeSet::new();
        let mut failed_tests_by_check = BTreeMap::new();
        let mut evidence = String::new();
        for check in report
            .get("baseline")
            .and_then(|baseline| baseline.get("checks"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|check| check["result"]["exit_code"] != 0)
        {
            let stdout = check["result"]["stdout"].as_str().unwrap_or_default();
            let stderr = check["result"]["stderr"].as_str().unwrap_or_default();
            let named = pwr_verify::failure::fingerprint(stdout, stderr).identifiers;
            failed_tests_by_check.insert(
                check["command"].as_str().unwrap_or("unknown").to_owned(),
                named.clone(),
            );
            failed_tests.extend(named);
            evidence.push_str(&failing_output(stdout, stderr));
        }
        let checks = report
            .get("baseline")
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<pwr_verify::VerificationBaseline>(value).ok()
            })
            .map(|baseline| pwr_verify::evidence::checks(&baseline));
        Ok(serve::GoalVerification {
            checks,
            contract_changed: Vec::new(),
            failure_fingerprints,
            failing_acceptance,
            failing,
            failed_tests,
            failed_tests_by_check,
            evidence,
            passed: technical_passed && acceptance_available,
            technical_passed,
            acceptance_available,
            summary: if technical_passed && !acceptance_available {
                format!(
                    "{summary}\n\nTo have a goal verified, declare a check with `\"kind\": \"acceptance\"` in `.pwr/checks.json` before starting it."
                )
            } else {
                summary
            },
        })
    }

    async fn run(
        &self,
        turn: serve::TurnInput,
    ) -> Result<(converse::TurnReport, Vec<ChatMessage>), String> {
        let root = turn
            .root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", turn.root.display()))?;
        let config = load_chat_config(&root).map_err(|error| error.context)?;
        chat_turn(
            root,
            self.runtime.clone(),
            config,
            turn.conversation_id,
            turn.stop,
            turn.steps,
            turn.messages,
            turn.continuity,
            turn.approvals,
            turn.session_grants,
            turn.goal_mode,
            turn.scripted,
        )
        .await
    }
}

impl ConsoleTurns {
    /// One generation beside the conversation: no tools, reasoning only when
    /// given a budget, on a cache of its own in the engine -- (the text, the
    /// model that wrote it).
    #[allow(clippy::too_many_arguments)]
    async fn aside(
        &self,
        root: &Path,
        system: &str,
        prompt: String,
        max_tokens: u32,
        context_cap: u32,
        reasoning_budget: Option<u32>,
        cancel: Option<pwr_provider::Cancel>,
    ) -> Result<(String, String), String> {
        let root = self.ready(root).await?;
        let config = load_chat_config(&root).map_err(|error| error.context)?;
        let model = config.model.clone().ok_or("no model is selected")?;
        let selection = self
            .runtime
            .select(model.clone(), Duration::from_secs(config.timeout_secs))
            .map_err(|error| error.to_string())?;
        let request = ModelRequest {
            deployment: selection.deployment.clone(),
            context_tokens: config.context_tokens.min(context_cap),
            tools: None,
            seed: None,
            sampling: BTreeMap::from([
                (
                    "think".to_owned(),
                    serde_json::json!(reasoning_budget.is_some()),
                ),
                (
                    "reasoning_budget".to_owned(),
                    serde_json::json!(reasoning_budget),
                ),
                ("max_tokens".to_owned(), serde_json::json!(max_tokens)),
                ("temperature".to_owned(), serde_json::json!(0.3)),
                // On a cache of its own: served on the conversation's, each
                // summary evicted it and the person's next message prefilled
                // the whole conversation again (205 s at 66k tokens, measured
                // 2026-09-26).
                ("aside".to_owned(), serde_json::json!(true)),
            ]),
            messages: vec![
                ChatMessage::text("system", system),
                ChatMessage::text("user", prompt),
            ],
        };
        let stream = selection
            .backend
            .chat_cancellable(request, cancel.unwrap_or_default())
            .await
            .map_err(|error| error.to_string())?;
        let reply = pwr_provider::collect_reply(stream)
            .await
            .map_err(|error| error.to_string())?;
        // A model that reasons anyway writes it inline on some templates.
        let text = match reply.content.rsplit_once("</think>") {
            Some((_, after)) => after.trim().to_owned(),
            None => reply.content.trim().to_owned(),
        };
        Ok((text, model))
    }

    /// `root`, canonical, when its configuration names a model. A supervised
    /// manual conversation does not wait for a capability measurement.
    async fn ready(&self, root: &Path) -> Result<PathBuf, String> {
        let root = root
            .canonicalize()
            .map_err(|error| format!("{}: {error}", root.display()))?;
        let mut config = load_chat_config(&root).map_err(|error| error.context)?;
        hydrate_chat_readiness(&root, &self.runtime, &mut config, false)
            .await
            .map_err(|error| error.context)?;
        if config.model.is_none() {
            return Err(
                "no model is chosen for this workspace; choose one in `pwr chat` Settings".into(),
            );
        }
        if !chat_is_prepared(&config) {
            return Err(
                "the chosen model is unavailable for agent work in this workspace; choose another in `pwr chat` Settings"
                    .into(),
            );
        }
        Ok(root)
    }
}

/// The workspace's log, without creating one where no conversation was ever
/// held: listing a directory should not leave state behind in it.
fn conversation_store(root: &Path) -> Result<Option<pwr_store::Store>, String> {
    let path = root.join(".pwr/state.sqlite");
    if !path.is_file() {
        return Ok(None);
    }
    pwr_store::Store::open(path)
        .map(Some)
        .map_err(|error| error.to_string())
}

/// `pwr serve --stdio`: until the client closes its end.
async fn serve_stdio(runtime: RuntimeFactory) -> i32 {
    let runner = std::rc::Rc::new(ConsoleTurns { runtime });
    let served = tokio::task::LocalSet::new()
        .run_until(serve::serve(
            runner,
            tokio::io::BufReader::new(tokio::io::stdin()),
            tokio::io::stdout(),
        ))
        .await;
    match served {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("pwr serve: {error}");
            1
        }
    }
}

/// How the console shows a step, or nothing for a step it does not show.
fn console_line(step: converse::TurnStep) -> Option<String> {
    Some(match step {
        converse::TurnStep::Acted { capability, detail } => format!("· {capability} · {detail}"),
        converse::TurnStep::Refused(why) => format!("! {why}"),
        converse::TurnStep::Compacted(note) => format!("⤢ context {note}"),
        converse::TurnStep::Steered(text) => format!("↳ read your message: {text}"),
        converse::TurnStep::Note(text) => text,
        // Nothing is saved from the console: the desktop app asks, and a
        // person can add it there or in `~/.pwr/memory.json` themselves.
        converse::TurnStep::MemoryProposed { text, scope } => {
            format!("✎ worth remembering ({scope}), not saved: {text}")
        }
        // The console shows actions as lines, from `Acted` and `Refused`, and
        // the answer once it is whole.
        converse::TurnStep::ToolCall(_)
        | converse::TurnStep::Streaming { .. }
        | converse::TurnStep::Prefill { .. }
        | converse::TurnStep::Usage { .. }
        | converse::TurnStep::Retry { .. }
        | converse::TurnStep::Recovered { .. }
        | converse::TurnStep::Generation(_) => return None,
    })
}

async fn wait_for_chat_turn(
    task: &mut Option<tokio::task::JoinHandle<ChatTurnResult>>,
) -> Result<ChatTurnResult, tokio::task::JoinError> {
    match task {
        Some(task) => task.await,
        None => std::future::pending().await,
    }
}

fn normalise_tui_paste(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn task_with_tui_attachments(task: String, attachments: &[String]) -> String {
    serve::with_attachments(task, attachments)
}

fn queue_tui_attachment(state: &mut TuiState, root: &Path, input: &str) {
    let path = parse_attachment_path(input);
    if path.as_os_str().is_empty() {
        push_tui_activity(state, "✗ Attachment path is empty");
        return;
    }
    match attach_chat_file(root, &path) {
        Ok(message) => {
            state.attachments.push(message.content);
            push_tui_activity(
                state,
                format!(
                    "✓ Attached {} to next task ({} queued)",
                    path.display(),
                    state.attachments.len()
                ),
            );
        }
        Err(error) => push_tui_activity(state, format!("✗ Attachment refused: {}", error.context)),
    }
}

/// What one read, write, patch or search result may hold in a conversation.
///
/// It was lowered to 16 KiB to shorten command output, and bounded all of
/// these with it: measured 2026-09-27 (stack matrix c3, node-fastify-tasks),
/// a 22-26 KB test file was refused three times as over the size limit. A
/// command's output has its own cap, `pwr_tools::COMMAND_STREAM_BYTES`.
const CONVERSATION_OUTPUT_LIMIT: usize = 64 * 1024;

/// Cargo can exit successfully after running zero tests. That establishes a
/// successful build, not that an application was exercised.
#[cfg(test)]
fn cargo_ran_tests(output: &str) -> bool {
    output.lines().any(|line| {
        line.trim()
            .strip_prefix("running ")
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|count| count.parse::<usize>().ok())
            .is_some_and(|count| count > 0)
    })
}

#[cfg(test)]
mod new_project_verification_tests {
    use super::cargo_ran_tests;

    #[test]
    fn an_empty_cargo_suite_is_not_presented_as_behavioral_verification() {
        assert!(!cargo_ran_tests(
            "running 0 tests\n\ntest result: ok. 0 passed"
        ));
        assert!(cargo_ran_tests(
            "running 0 tests\nrunning 2 tests\ntest result: ok"
        ));
    }
}

/// The conversation's share of the shared composer.
///
/// It had none. `context::compose` says of itself that retrieval happens there
/// "so section order, retrieval budget and compaction accounting cannot diverge
/// by caller", and its callers were the evaluator and the scripted run; the
/// conversation opened with a bare system string and retrieved nothing. So the
/// loop where the work now happens saw less of the repository than the loop the
/// benchmarks run, and a retrieval result measured through eval could never
/// have reached a user.
///
/// Replaces the trailing user message with the sections a turn contributes:
/// the passages ranked against what was asked, then the request. Returns the
/// tokens of passages delivered, or `None` when there was no fresh request to
/// rank against.
///
/// Indexing is incremental and cached under `.pwr`, so this is a walk of
/// what changed rather than of the repository -- but the first turn in an
/// unindexed workspace pays for the first index, which is latency the
/// conversation did not have before.
///
/// A failure to index is not a failure of the turn. Passages are a starting
/// point the deployment can rebuild with `search` and `read_file`, which is why
/// the compiler evicts them first under pressure; a turn that cannot retrieve
/// is worse than one that can and better than no turn at all.
/// The W8.3 control's prompt: its neutral instruction and the deployment's
/// declared suffix (plumbing: what the model needs to run, not how PWR works),
/// and nothing retrieved, remembered or ruled.
fn compose_minimal_turn(
    root: &Path,
    context_tokens: u32,
    task_profile: &pwr_orchestrator::TaskProfile,
    messages: &mut [ChatMessage],
) -> Result<(Option<usize>, Option<String>), String> {
    if let Some(system) = messages.first_mut().filter(|first| first.role == "system") {
        let (merged, compiled) = pwr_orchestrator::context::compile(
            vec![
                pwr_orchestrator::context::Section::new(
                    pwr_orchestrator::context::SectionKind::System,
                    converse::minimal_system_prompt(root),
                ),
                pwr_orchestrator::context::Section::new(
                    pwr_orchestrator::context::SectionKind::ModelSuffix,
                    &task_profile.prompt_suffix,
                ),
            ],
            context_tokens,
        );
        if compiled.over_budget_by > 0 {
            return Err(format!(
                "Context full: system instructions exceed budget by {} estimated tokens",
                compiled.over_budget_by
            ));
        }
        if let Some(first) = merged.first() {
            system.content = first.content.clone();
        }
    }
    Ok((None, None))
}

fn compose_chat_turn(
    root: &Path,
    context_tokens: u32,
    task_profile: &pwr_orchestrator::TaskProfile,
    ledger: Option<&str>,
    messages: &mut Vec<ChatMessage>,
) -> Result<(Option<usize>, Option<String>), String> {
    // The deployment's own instructions, merged into the system message the way
    // a run merges them. A deployment told to behave one way on one turn and
    // not told on the next is being given two different agents, which is the
    // reason `ModelSuffix` is a required section rather than an optional one.
    // Refreshed rather than appended, because the model can be changed from
    // Settings mid-conversation and the suffix belongs to the model.
    if let Some(system) = messages.first().filter(|first| first.role == "system") {
        let (merged, compiled) = pwr_orchestrator::context::compile(
            vec![
                pwr_orchestrator::context::Section::new(
                    pwr_orchestrator::context::SectionKind::System,
                    // With the reference folders, as the conversation was
                    // opened: the bare prompt here dropped them from the
                    // first turn on, so a model was never told where the
                    // project's documents were (D.E2E-27).
                    chat_system_prompt_for(root),
                ),
                pwr_orchestrator::context::Section::new(
                    pwr_orchestrator::context::SectionKind::ModelSuffix,
                    &task_profile.prompt_suffix,
                ),
            ],
            context_tokens,
        );
        if compiled.over_budget_by > 0 {
            return Err(format!(
                "Context full: system instructions exceed budget by {} estimated tokens",
                compiled.over_budget_by
            ));
        }
        if let Some(first) = merged.first()
            && first.content != system.content
        {
            messages[0].content = first.content.clone();
        }
    }
    // A message already carrying a purpose has been composed once. Composing it
    // again would rank passages against passages.
    let Some(request) = messages
        .last()
        .filter(|message| message.role == "user" && message.purpose.is_none())
        .map(|message| message.content.clone())
    else {
        return Ok((None, None));
    };
    let (index, _work) = pwr_repo::index_incremental(root, Some(&root.join(".pwr")))
        .map_err(|error| error.to_string())?;
    // What the history has left, not the whole window. A conversation composes
    // a turn into a prompt that already holds everything said before it, and
    // ranking a sixth of the window's worth of passages against a request that
    // has to share the window with twenty turns is how a conversation runs out
    // of room a turn earlier every time. Under real pressure the compiler drops
    // the passages and keeps the request, which is the right way round: the
    // request is what the turn is for, and passages can be searched for again.
    let spent: usize = messages
        .iter()
        .rev()
        .skip(1)
        .map(pwr_orchestrator::context::estimate_message_tokens)
        .sum();
    let room = u32::try_from(
        usize::try_from(context_tokens)
            .unwrap_or(usize::MAX)
            .saturating_sub(spent),
    )
    .unwrap_or(context_tokens);
    // Started per turn, which costs the encoder's load (about a second) on top
    // of the ranking itself: acceptable while it is opt-in and measured, and
    // the section vectors are cached on disk between turns.
    let (mut ranker, fallback) = semantic_ranker_if_requested(root);
    let (composed, mut compiled) =
        pwr_orchestrator::context::compose_turn(pwr_orchestrator::context::TurnComposition {
            root,
            index: &index,
            request: &request,
            context_tokens: room,
            task_profile,
            session_ledger: ledger,
            ranker: ranker
                .as_mut()
                .map(|ranker| ranker as &mut dyn pwr_repo::SectionRanker),
        });
    compiled.retrieval_fallback = ranker
        .as_ref()
        .and_then(|ranker| ranker.fallback_reason.clone())
        .or(fallback);
    drop(ranker);
    if compiled.over_budget_by > 0 && compiled.estimated_tokens > context_tokens as usize * 3 / 4 {
        return Err(format!(
            "Context full: required prompt exceeds input budget by {} estimated tokens",
            compiled.over_budget_by
        ));
    }
    let delivered = compiled
        .sections
        .iter()
        .find(|section| section.kind == pwr_orchestrator::context::SectionKind::RepositoryExcerpts)
        .filter(|section| !section.dropped && section.estimated_tokens > 0)
        .map(|section| section.estimated_tokens);
    // The images the person attached travel with the request, which the
    // composed turn carries last (C.25).
    let images = messages
        .pop()
        .map(|message| message.images)
        .unwrap_or_default();
    let mut composed = composed;
    if let Some(last) = composed
        .iter_mut()
        .rev()
        .find(|message| message.role == "user")
    {
        last.images = images;
    }
    messages.extend(composed);
    Ok((delivered, compiled.retrieval_fallback))
}

/// The console's reading of `doctor`, which answers a question in one line.
///
/// The full report is a page of JSON and the operator asked one thing: is the
/// backend there, and what is it serving. A console that answers with the
/// document rather than the answer has moved the work rather than done it.
fn summarise_doctor(report: &serde_json::Value) -> String {
    let backend = report
        .get("backend")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown backend");
    let state = report
        .get("backend_runtime")
        .or_else(|| report.get("runtime"));
    let status = state
        .and_then(|state| state.get("status"))
        .and_then(serde_json::Value::as_str);
    let reason = state
        .and_then(|state| state.get("reason"))
        .and_then(serde_json::Value::as_str);
    match (status, reason) {
        (Some("unavailable"), Some(reason)) => {
            format!("{backend} is not answering: {reason}")
        }
        _ => {
            let metadata_only = state
                .and_then(|state| state.get("state"))
                .and_then(|state| state.get("status"))
                .and_then(serde_json::Value::as_str)
                == Some("metadata_only");
            if metadata_only {
                return format!(
                    "{backend} is available for model metadata; generation is not wired yet"
                );
            }
            let server_on_demand = state
                .and_then(|state| state.get("state"))
                .and_then(|state| state.get("status"))
                .and_then(serde_json::Value::as_str)
                == Some("server_on_demand");
            if server_on_demand {
                return format!("{backend} is available and starts its server on demand");
            }
            let loaded = state
                .and_then(|state| state.get("loaded_models"))
                .and_then(serde_json::Value::as_array)
                .map(|models| models.len())
                .unwrap_or(0);
            format!("{backend} is answering, with {loaded} model(s) loaded")
        }
    }
}

/// The console's reading of `diagnose`: what the detectors found, or that they
/// found nothing, which is itself worth saying to someone who suspects a loop.
fn summarise_diagnosis(report: &serde_json::Value) -> String {
    let read = report
        .get("events_read")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let findings = report
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if findings.is_empty() {
        return format!(
            "{read} events read, and none of the known pathologies is in them. That is not \
             proof nothing is wrong -- only that nothing this harness has already learned to \
             recognise is."
        );
    }
    let mut said = format!("{read} events read. What the detectors recognise in them:");
    for finding in &findings {
        // `says` exists for exactly this: one sentence a person reads without
        // knowing the schema. The count is what turns a sentence into a
        // measurement, and `detail` is the shape a command-line reader wants
        // and a console reader does not.
        let sentence = finding
            .get("says")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no description");
        let detector = finding
            .get("detector")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unnamed");
        let count = finding
            .get("count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        said.push_str(&format!("\n  · {detector} ×{count}: {sentence}"));
    }
    said
}

/// The console's reading of `verify`: what the repository declares and how it
/// stands right now.
///
/// The turn's own verdict answers a narrower question -- whether a change broke
/// anything -- and cannot be asked when nothing has changed. This one can, and
/// names the checks, because "the checks failed" sends an operator looking for
/// a list the console was holding.
fn summarise_verification(report: &serde_json::Value) -> String {
    let scope = report
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("targeted");
    let checks: Vec<(String, bool)> = report
        .get("baseline")
        .and_then(|baseline| baseline.get("checks"))
        .and_then(serde_json::Value::as_array)
        .map(|checks| {
            checks
                .iter()
                .map(|check| {
                    (
                        check
                            .get("command")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or("unnamed")
                            .to_owned(),
                        check
                            .get("result")
                            .and_then(|result| result.get("exit_code"))
                            .and_then(serde_json::Value::as_i64)
                            == Some(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    if checks.is_empty() {
        return "this workspace declares no checks, so nothing here can confirm a change by \
                running it"
            .to_owned();
    }
    let passing = checks.iter().filter(|(_, passed)| *passed).count();
    let mut said = format!("{passing} of {} {scope} check(s) passing:", checks.len());
    for (command, passed) in &checks {
        said.push_str(&format!(
            "\n  {} {command}",
            if *passed { "·" } else { "✗" }
        ));
    }
    said
}

/// The console's reading of `report`: what this conversation has actually done.
///
/// Counts rather than prose, and the ones an operator acts on: how much of the
/// time went to the model rather than to tools, whether anything was refused,
/// and whether the harness had to name a loop. A conversation that has been
/// working for ten minutes should not have to be exported to answer that.
fn summarise_session(report: &serde_json::Value) -> String {
    let Some(replay) = report.get("replay") else {
        return "nothing has been recorded for this conversation yet".to_owned();
    };
    let number = |key: &str| {
        replay
            .get(key)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };
    let seconds = |key: &str| {
        replay
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0)
    };
    let mut said = format!(
        "{} turn(s), {} action(s) allowed, {} denied, {} failed",
        number("turns"),
        number("actions_allowed"),
        number("actions_denied"),
        number("actions_failed"),
    );
    said.push_str(&format!(
        "\n{:.0}s elapsed, {:.0}s of it generating; {} prompt and {} generated token(s)",
        seconds("elapsed_secs"),
        seconds("generation_secs"),
        number("prompt_tokens"),
        number("generated_tokens"),
    ));
    // Said only when it happened: a line of zeroes every time teaches the
    // reader to skip the line that matters.
    for (count, what) in [
        (number("loops_named"), "repetition(s) named"),
        (
            number("no_progress_named"),
            "window(s) of no progress named",
        ),
        (number("compactions"), "compaction(s)"),
        (number("context_downgrades"), "drop(s) to a lower window"),
    ] {
        if count > 0 {
            said.push_str(&format!("\n{count} {what}"));
        }
    }
    said
}

fn summarise_sessions_list(report: &serde_json::Value) -> String {
    let sessions = report
        .get("sessions")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if sessions.is_empty() {
        return "no saved sessions in this workspace yet".to_owned();
    }
    let mut said = format!("{} saved session(s):", sessions.len());
    for session in &sessions {
        let name = session
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unnamed");
        let runs = session
            .get("runs")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        let task = session
            .get("last_task")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("no recorded task");
        said.push_str(&format!("\n  - {name} - {runs} run(s), last asked: {task}"));
    }
    said
}

fn summarise_named_session(report: &serde_json::Value) -> String {
    let name = report
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unnamed");
    let runs = report
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .map_or(0, Vec::len);
    let branch = |key: &str| {
        report
            .get(key)
            .and_then(|state| state.get("branch"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown")
    };
    let mut said = format!(
        "session {name}: {runs} run(s)\nopened on branch {}; workspace now on branch {}",
        branch("opened_on"),
        branch("workspace_now"),
    );
    if let Some(interrupted) = report.get("interrupted_run")
        && !interrupted.is_null()
    {
        let state = interrupted
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown");
        let actions = interrupted
            .get("actions_spent")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        said.push_str(&format!(
            "\nlast run was interrupted in {state} after {actions} action(s)"
        ));
    }
    let ledger = report
        .get("ledger")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");
    if ledger.trim().is_empty() {
        said.push_str("\nno session ledger has been recorded yet");
    } else {
        let first = ledger.lines().next().unwrap_or("ledger recorded");
        said.push_str(&format!("\nledger: {first}"));
    }
    said
}

/// The terminal console as the session executor's host: a conversation, one
/// turn at a time, with no goals to verify or review.
///
/// The console called the turn directly, so the app and the console ran the
/// same turn under two different callers. Both now go through
/// `executor::execute`, and the console differs only in what it cannot do.
struct ConsoleHost {
    runtime: RuntimeFactory,
    config: ChatConfig,
    /// The conversation as the last turn left it, for the console to keep.
    kept: std::cell::RefCell<Option<Vec<ChatMessage>>>,
}

#[async_trait::async_trait(?Send)]
impl pwr_orchestrator::executor::SessionHost for ConsoleHost {
    async fn run_turn(&self, input: serve::TurnInput) -> ChatTurnResult {
        chat_turn(
            input.root,
            self.runtime.clone(),
            self.config.clone(),
            input.conversation_id,
            input.stop,
            input.steps,
            input.messages,
            input.continuity,
            input.approvals,
            input.session_grants,
            input.goal_mode,
            input.scripted,
        )
        .await
    }

    async fn verify(
        &self,
    ) -> Result<
        (serve::GoalVerification, BTreeMap<String, String>),
        pwr_orchestrator::executor::VerifyError,
    > {
        Err(pwr_orchestrator::executor::VerifyError::Failed(
            "the console runs no goals".into(),
        ))
    }

    async fn review(&self, _: &Path, _: String) -> Result<String, String> {
        Err("the console runs no goals".into())
    }

    fn say(&self, _: &str) {}

    fn keep_messages(&self, messages: &[ChatMessage]) {
        *self.kept.borrow_mut() = Some(messages.to_vec());
    }
}

/// One console exchange, through the executor: the same path the app takes for
/// a conversation.
#[allow(clippy::too_many_arguments)]
async fn console_turn(
    root: PathBuf,
    runtime: RuntimeFactory,
    config: ChatConfig,
    conversation_id: pwr_domain::Id,
    stop: Arc<AtomicBool>,
    steps: StepSink,
    messages: Vec<ChatMessage>,
    continuity: converse::Continuity,
    approvals: Arc<dyn pwr_orchestrator::ApprovalPrompt>,
    session_grants: Arc<std::sync::Mutex<Vec<pwr_tools::Approval>>>,
) -> ChatTurnResult {
    use pwr_orchestrator::executor::{self, Policy, SessionEnd, SessionRequest, SessionResult};
    let host = ConsoleHost {
        runtime,
        config,
        kept: std::cell::RefCell::new(None),
    };
    let SessionResult { end, .. } = executor::execute(
        &host,
        SessionRequest {
            root,
            conversation_id,
            messages,
            stop,
            steps: std::rc::Rc::new(std::cell::RefCell::new(steps)),
            continuity,
            approvals,
            session_grants,
            policy: Policy::Conversation,
            aids: serve::GoalAids::default(),
        },
        serve::GoalLimits::default(),
    )
    .await;
    match end {
        SessionEnd::Reply { report, .. } => Ok((report, host.kept.take().unwrap_or_default())),
        SessionEnd::Error { message, .. } => Err(message),
        SessionEnd::Stopped { .. } | SessionEnd::OutOfBudget { .. } => {
            Err("the console's turn ended in a way only a goal can".into())
        }
    }
}

/// One exchange, run off the console's thread.
///
/// Returns the conversation it produced as well as the outcome, because the
/// turn appends the assistant's reply and any reads it made, and the console
/// has to keep them for the next turn.
/// The part of a failing check's output worth showing to a model asked to
/// make it pass: stack frames and blank lines out, and of a long one the
/// beginning and the end.
///
/// The end alone was kept at first, 3,000 characters of it: on a suite of
/// four test files that is the last file's failures, and the model asked for
/// another file was shown nothing about it (product-path run, 2026-10-06).
fn failing_output(stdout: &str, stderr: &str) -> String {
    const SHOWN: usize = 12_000;
    let mut text = String::new();
    for line in stdout.lines().chain(stderr.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("at ") || trimmed.starts_with("File \"") {
            continue;
        }
        text.push_str(line.trim_end());
        text.push('\n');
    }
    if text.len() <= SHOWN {
        return text;
    }
    let mut head = SHOWN / 2;
    while !text.is_char_boundary(head) {
        head -= 1;
    }
    let mut tail = text.len() - SHOWN / 2;
    while !text.is_char_boundary(tail) {
        tail += 1;
    }
    format!("{}[...]\n{}", &text[..head], &text[tail..])
}

#[allow(clippy::too_many_arguments)]
async fn chat_turn(
    root: PathBuf,
    runtime: RuntimeFactory,
    config: ChatConfig,
    conversation_id: pwr_domain::Id,
    stop: Arc<AtomicBool>,
    mut steps: StepSink,
    mut messages: Vec<ChatMessage>,
    mut continuity: converse::Continuity,
    // Whoever asks the person: the console, or a protocol client.
    approvals: Arc<dyn pwr_orchestrator::ApprovalPrompt>,
    session_grants: Arc<std::sync::Mutex<Vec<pwr_tools::Approval>>>,
    goal_mode: bool,
    // One call PWR decided itself, taken through this turn in the model's
    // place (plan W2.9): everything after the reply is the turn's own.
    scripted: Option<pwr_orchestrator::executor::ScriptedTurn>,
) -> ChatTurnResult {
    {
        let mut checkpoint = continuity
            .checkpoint
            .lock()
            .map_err(|_| "checkpoint lock unavailable")?;
        if !checkpoint.acceptance_initialized {
            checkpoint.acceptance = if checkpoint.turn == 0 {
                pwr_verify::acceptance::snapshot(&root)?
            } else {
                None
            };
            checkpoint.acceptance_initialized = true;
        }
        let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
            .map_err(|error| error.to_string())?;
        pwr_orchestrator::conversation::record_checkpoint(&store, conversation_id, &checkpoint)?;
    }
    let model = config.model.clone().ok_or("no model is selected")?;
    compaction_settings(&config, &mut continuity);
    let selection = runtime
        .select(model.clone(), Duration::from_secs(config.timeout_secs))
        .map_err(|error| error.to_string())?;
    let provider = selection.backend;
    let deployment = selection.deployment;
    // PWR runs the Gemma 4 line: what the model is, how it samples and how it
    // reasons are written once in `pwr_domain::gemma`, never inferred from a
    // card, a profile file or a calibration.
    pwr_domain::gemma::pack(&model).ok_or_else(|| pwr_domain::gemma::not_in_line(&model))?;
    let adapter = pwr_compat::adapter_for(Some("gemma4"), &model);
    let chat_only = is_chat_home(&root);
    let mut task_profile = pwr_orchestrator::TaskProfile::resolve(None, None);
    task_profile.sampling.extend(pwr_domain::gemma::sampling());
    continuity.reasoning_effort = pwr_domain::gemma::effort(config.thinking);
    // A goal set its own limit; a conversation takes the workspace's.
    if continuity.action_limit.is_none() && config.actions_per_turn > 0 {
        continuity.action_limit = Some(config.actions_per_turn);
    }
    continuity.reasoning = pwr_domain::gemma::reasoning();
    continuity.profile_status = Some("gemma_line".into());
    let task_profile = task_profile;
    let mut continuity = continuity;
    continuity.chat_only = chat_only;
    // The W8.3 control: the same deployment, sampling, tools and policy, and
    // none of PWR's composition or checks (`converse::Harness`).
    let minimal = continuity.harness == converse::Harness::Minimal;
    let policy = if chat_only {
        // Chat mode: what the person attached, read-only, and nothing else.
        // No commands, no grants, no dependencies; the catalogue offers only
        // reading, and this policy would refuse the rest anyway.
        pwr_tools::ToolPolicy {
            root: root.clone(),
            extra_readable: reference_roots(&root, &config),
            protected: Vec::new(),
            allow_commands: Vec::new(),
            output_limit: CONVERSATION_OUTPUT_LIMIT,
            timeout: Duration::from_secs(120),
            sandbox: pwr_tools::SandboxPolicy::Preferred,
            approvals: Vec::new(),
        }
    } else {
        pwr_tools::ToolPolicy {
            root: root.clone(),
            // The reference folders the person attached, whatever the task
            // declares readable, and the project's installed dependencies -- the
            // exact versions it builds against, on disk, read-only (C.12).
            extra_readable: reference_roots(&root, &config)
                .into_iter()
                .chain(pwr_verify::declared_readable(&root))
                .chain(pwr_tools::dependency_roots(&root))
                .collect(),
            protected: {
                let checkpoint = continuity
                    .checkpoint
                    .lock()
                    .map_err(|_| "checkpoint lock unavailable")?;
                let authorized = &checkpoint.authorized_acceptance_changes;
                let mut paths = frozen_paths_with_authorization(&root, authorized)
                    .map_err(|error| error.context)?;
                if let Some(snapshot) = &checkpoint.acceptance {
                    paths.extend(
                        snapshot
                            .artifacts
                            .keys()
                            .filter(|path| !authorized.contains(*path))
                            .map(PathBuf::from),
                    );
                }
                paths
            },
            // Derived from what the repository is, exactly as a scripted run
            // derives it. A fixed list decides in advance which languages the
            // conversation can work in: `cargo, git, rg` is the right list for
            // this repository and the wrong one for an Angular project, whose
            // `npm` a scripted run has allowlisted all along.
            allow_commands: pwr_verify::required_executables(&root),
            output_limit: CONVERSATION_OUTPUT_LIMIT,
            timeout: Duration::from_secs(120),
            sandbox: sandbox_for(&config),
            // The grant the console makes for work in this workspace, less what
            // Settings say to ask about, plus what was allowed for this session
            // when asked. The policy still confines writes and records every action.
            approvals: chat_approvals(
                &effective_ask_before(&config),
                &granted_now(&session_grants),
            ),
        }
    };
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite"))
        .map_err(|error| error.to_string())?;
    let checks = if chat_only || minimal {
        Vec::new()
    } else {
        pwr_verify::discover_checks(&root, "targeted").unwrap_or_default()
    };
    // Captured before the turn acts, so what the checks say afterwards is
    // about this turn rather than about whatever the repository was already
    // carrying.
    let before = if checks.is_empty() {
        None
    } else {
        pwr_orchestrator::executor::baseline_until_stopped(&policy, &checks, &stop)
            .await
            .and_then(Result::ok)
    };
    // What this conversation has already established, re-hashed from disk. The
    // conversation is the loop most exposed to compaction loss -- it is the one
    // that discards tool bodies to make room -- and this is the deterministic
    // record of what those bodies said about which files, with the hashes they
    // have now rather than the ones they had when they were read.
    let ledger = if minimal {
        None
    } else {
        pwr_orchestrator::session_ledger(&store, &[conversation_id], &root).ok()
    };
    // Report retrieval degradation to the operator and journal, without
    // polluting a small model's task prompt with harness diagnostics.
    let composition = if chat_only {
        Ok((None, None))
    } else if minimal {
        compose_minimal_turn(&root, config.context_tokens, &task_profile, &mut messages)
    } else {
        compose_chat_turn(
            &root,
            config.context_tokens,
            &task_profile,
            ledger.as_deref(),
            &mut messages,
        )
    };
    match composition {
        Ok((tokens, fallback)) => {
            if let Some(tokens) = tokens {
                steps(converse::TurnStep::Note(format!(
                    "⌕ {tokens} tokens of repository passages ranked against the request"
                )));
            }
            if let Some(reason) = fallback {
                steps(converse::TurnStep::Note(format!(
                    "⌕ Semantic retrieval unavailable; lexical ranking was used: {reason}"
                )));
                store
                    .append(
                        Some(conversation_id),
                        "context.retrieval_fallback",
                        serde_json::json!({"reason":reason,"ranking":"lexical"}),
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        Err(error) if error.starts_with("Context full:") => return Err(error),
        Err(error) => {
            steps(converse::TurnStep::Note(format!(
                "⌕ no repository passages this turn: {error}"
            )));
            store
                .append(
                    Some(conversation_id),
                    "context.retrieval_fallback",
                    serde_json::json!({"reason":error,"ranking":"unavailable"}),
                )
                .map_err(|error| error.to_string())?;
        }
    }
    // The windows calibration measured on this deployment, so a prompt the
    // backend refuses can be retried at one that was measured rather than
    // compacted away. The same filter the console's own preparation uses, not
    // the scripted loop's: preparation refuses to leave the chat on a tier that
    // showed memory pressure, and a turn in trouble falling to exactly that
    // tier would undo the choice this config already made.
    let context_tiers: Vec<u32> = config
        .profile
        .as_ref()
        .and_then(|path| load_calibration(path).ok())
        .map(|calibration| {
            calibration
                .stable_points
                .iter()
                .filter(|point| {
                    !point.memory_pressure_observed && calibration.thresholds.admits(point)
                })
                .map(|point| point.context_tokens)
                .collect()
        })
        .unwrap_or_default();
    let mut groups = config.tool_groups;
    groups.memory &= pwr_orchestrator::personal::Home::from_env()
        .and_then(|home| pwr_orchestrator::personal::load_profile(&home))
        .map(|profile| profile.memory_enabled)
        .unwrap_or(false);
    let catalog = converse::product_tool_catalog(groups, chat_only);
    let catalog = if chat_only {
        catalog
    } else if pwr_mlx::MlxConfig::from_env().has_vision_encoder(&model) == Some(true) {
        converse::with_vision(catalog)
    } else {
        converse::with_page_check(catalog)
    };
    // A client that shows the person's terminal can read it to the model.
    let catalog = if !chat_only && !minimal && approvals.reads_terminal() {
        converse::with_terminal(catalog)
    } else {
        catalog
    };
    let catalog = if continuity.plan.is_some() {
        converse::with_plan(catalog)
    } else {
        catalog
    };
    let catalog = if goal_mode {
        converse::with_goal_verification(catalog)
    } else if minimal {
        converse::minimal_tool_catalog(catalog)
    } else {
        catalog
    };
    let catalog = if let Some(contract) = &continuity.action_contract {
        contract.catalog(catalog)
    } else {
        catalog
    };
    let tools = provider.render_tools(&catalog);
    let is_scripted = scripted.is_some();
    let provider = pwr_orchestrator::executor::Scripted::new(provider, scripted);
    let report = converse::take_turn(
        &provider,
        adapter.as_ref(),
        &deployment,
        &store,
        conversation_id,
        &policy,
        &mut messages,
        config.context_tokens,
        &context_tiers,
        task_profile.sampling.clone(),
        tools,
        &stop,
        &continuity,
        // What Settings say to ask about is put to whoever drives the turn.
        approvals.as_ref(),
        |step| {
            // A closed channel means the front end is gone; the turn is being
            // torn down with it and has nowhere to report.
            steps(step);
        },
    )
    .await?;
    // A turn that changed the workspace has to face the repository's own
    // checks, and the answer carries what they said. Saying "done" without
    // that is the claim this project exists to refuse.
    let mut report = report;
    if let Some(reason) = report.stopped {
        report.answer = match report.answer.trim() {
            "" => reason.said().to_owned(),
            said => format!("{said}\n\n{}", reason.said()),
        };
    }
    if report.edited && !minimal && !is_scripted {
        let after_checks = if chat_only {
            Vec::new()
        } else {
            pwr_verify::discover_checks(&root, "targeted").unwrap_or_default()
        };
        let mut verification_policy = policy.clone();
        // What the person allowed for the session during this turn counts for
        // its checks: a restore needs the network they allowed a moment ago.
        verification_policy.approvals = chat_approvals(
            &effective_ask_before(&config),
            &granted_now(&session_grants),
        );
        for (executable, _) in &after_checks {
            if !verification_policy.allow_commands.contains(executable) {
                verification_policy.allow_commands.push(executable.clone());
            }
        }
        pwr_orchestrator::executor::close_turn_with_stop(
            pwr_orchestrator::executor::ClosingChecks {
                policy: verification_policy,
                checks_before: checks,
                checks_after: after_checks,
                before,
            },
            &mut report,
            &mut messages,
            &mut |step| steps(step),
            &stop,
        )
        .await;
    }
    // The conversation as it now stands, so `pwr chat --continue` can pick
    // it up from here. Not being able to record it is said, not fatal: the
    // turn's work is done and on disk either way.
    if let Err(error) =
        pwr_orchestrator::conversation::record_snapshot(&store, conversation_id, &messages)
    {
        steps(converse::TurnStep::Note(format!(
            "! this turn could not be saved for --continue: {error}"
        )));
    }
    Ok((report, messages))
}

async fn run_tui(
    root: PathBuf,
    config: ChatConfig,
    runtime: RuntimeFactory,
    initial_attachments: Vec<String>,
    resume: bool,
) -> TuiExit {
    tokio::task::LocalSet::new()
        .run_until(run_tui_inner(
            root,
            config,
            runtime,
            initial_attachments,
            resume,
        ))
        .await
}

async fn run_tui_inner(
    root: PathBuf,
    config: ChatConfig,
    runtime: RuntimeFactory,
    initial_attachments: Vec<String>,
    resume: bool,
) -> TuiExit {
    let mut terminal = ratatui::init();
    let mut stdout = io::stdout();
    let _ = execute!(stdout, EnableBracketedPaste);
    let initially_queued = initial_attachments.len();
    let mut state = TuiState {
        input: String::new(),
        attachments: initial_attachments,
        transcript: vec![
            "PWR. Ask about this repository, or ask for a change and I will make it and \
             run its checks."
                .into(),
            if initially_queued == 0 {
                String::new()
            } else {
                format!("{initially_queued} attachment(s) queued for the next task.")
            },
        ],
        activity: vec!["Nothing yet.".into()],
        thinking: false,
        spinner: 0,
        input_mode: TuiInputMode::Task,
        run_id: None,
        seen_run_events: BTreeSet::new(),
        pending_approval: None,
    };
    // Questions a turn puts to the person here, and what they allowed for the
    // rest of the session.
    let (approval_sender, mut approval_receiver) =
        tokio::sync::mpsc::unbounded_channel::<ApprovalRequest>();
    let session_grants: Arc<std::sync::Mutex<Vec<pwr_tools::Approval>>> = Arc::default();
    // The conversation itself, opened with the instructions it runs under.
    let mut messages: Vec<ChatMessage> =
        vec![ChatMessage::text("system", chat_system_prompt_for(&root))];
    // One id for the whole conversation, so `pwr report` reads the thread
    // rather than a scatter of one-action runs -- and so the console can show
    // each action as the audit records it rather than only when the turn ends.
    let mut conversation_id = new_id();
    let continuity = converse::Continuity::default();
    if resume {
        resume_into(
            &root,
            &mut state,
            &mut messages,
            &mut conversation_id,
            &continuity,
        );
    }
    state.run_id = Some(conversation_id);
    let mut chat_task: Option<tokio::task::JoinHandle<ChatTurnResult>> = None;
    // Each step a turn takes, shown as it happens rather than when the turn
    // ends. A long turn is exactly the one an operator needs to watch.
    let (step_sender, mut step_receiver) =
        tokio::sync::mpsc::unbounded_channel::<converse::TurnStep>();
    // The operator's stop button. An interactive agent is bounded by the
    // person watching it, which is why there is no action budget here.
    let mut stop = Arc::new(AtomicBool::new(false));
    let mut ticker = tokio::time::interval(Duration::from_millis(100));
    // `event::read` owns the terminal event stream. Creating a new blocking
    // read on every redraw meant a dropped select branch could consume a key
    // without delivering it to the composer (notably the leading `/` in
    // `/attach`). Keep exactly one reader alive and forward its events.
    let (event_sender, mut event_receiver) = tokio::sync::mpsc::unbounded_channel();
    let event_reader_stop = Arc::new(AtomicBool::new(false));
    let reader_stop = Arc::clone(&event_reader_stop);
    let event_reader = tokio::task::spawn_blocking(move || {
        while !reader_stop.load(Ordering::Acquire) {
            let Ok(ready) = event::poll(Duration::from_millis(50)) else {
                continue;
            };
            if !ready {
                continue;
            }
            let Ok(event) = event::read() else {
                continue;
            };
            if event_sender.send(event).is_err() {
                break;
            }
        }
    });
    let exit = loop {
        let _ = terminal.draw(|frame| draw_tui(frame, &state, &root, &config));
        tokio::select! {
            _ = ticker.tick() => {
                if state.thinking {
                    state.spinner = state.spinner.wrapping_add(1);
                    if state.spinner.is_multiple_of(5) {
                        sync_tui_activity(&mut state, &root);
                    }
                }
            }
            Some(step) = step_receiver.recv() => {
                if let Some(line) = console_line(step) {
                    push_tui_activity(&mut state, line);
                }
            }
            Some(request) = approval_receiver.recv() => {
                push_tui_transcript(
                    &mut state,
                    format!(
                        "PWR asks before it will {}: {} -- y once, a for this session, n to refuse",
                        approval_label(request.approval),
                        request.description
                    ),
                );
                // A question already open is refused rather than lost: a turn
                // asks one at a time, so a second one means the first was
                // abandoned.
                if let Some(earlier) = state.pending_approval.replace(request.reply) {
                    let _ = earlier.send(pwr_orchestrator::ApprovalDecision::Deny);
                }
            }
            outcome = wait_for_chat_turn(&mut chat_task) => {
                state.thinking = false;
                chat_task = None;
                match outcome {
                    Ok(Ok((report, history))) => {
                        messages = history;
                        // The steps have already been shown as they happened.
                        sync_tui_activity(&mut state, &root);
                        let answer = report.answer.trim();
                        if answer.is_empty() {
                            push_tui_transcript(&mut state, format!("PWR: {} action(s) taken.", report.actions));
                        } else {
                            push_tui_transcript(&mut state, format!("PWR: {answer}"));
                        }
                    }
                    Ok(Err(problem)) => push_tui_transcript(&mut state, format!("PWR could not answer: {problem}")),
                    Err(error) => push_tui_transcript(&mut state, format!("PWR stopped unexpectedly: {error}")),
                }
            }
            event = event_receiver.recv() => {
                let Some(event) = event else { break TuiExit::Quit; };
                match event {
                    Event::Paste(text) => {
                        let text = normalise_tui_paste(&text);
                        if !text.is_empty() {
                            let characters = text.chars().count();
                            let lines = text.lines().count().max(1);
                            state.input.push_str(&text);
                            push_tui_activity(&mut state, format!("✓ Pasted {characters} characters across {lines} line(s). Review, then press Enter once to send."));
                        }
                    }
                    Event::Key(key)
                        if key.kind == KeyEventKind::Press && state.pending_approval.is_some() =>
                    {
                        if let Some(decision) = approval_key(key.code)
                            && let Some(reply) = state.pending_approval.take()
                        {
                            let _ = reply.send(decision);
                            push_tui_transcript(
                                &mut state,
                                match decision {
                                    pwr_orchestrator::ApprovalDecision::AllowOnce => "you: allowed, this once",
                                    pwr_orchestrator::ApprovalDecision::AllowForRun => {
                                        "you: allowed for the rest of this session"
                                    }
                                    pwr_orchestrator::ApprovalDecision::Deny => "you: refused",
                                },
                            );
                        }
                    }
                    Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                        KeyCode::Esc if state.input_mode == TuiInputMode::AttachmentPath => {
                            state.input.clear();
                            state.input_mode = TuiInputMode::Task;
                            push_tui_activity(&mut state, "Attachment cancelled");
                        }
                        // Stop comes before everything else Esc does: a turn
                        // in flight is the thing an operator most needs to be
                        // able to end, and quitting the console to do it loses
                        // the conversation.
                        KeyCode::Esc if state.thinking => {
                            stop.store(true, Ordering::Relaxed);
                            push_tui_activity(&mut state, "! Stopping…");
                        }
                        KeyCode::Esc if !state.input.is_empty() => {
                            state.input.clear();
                            push_tui_activity(&mut state, "Draft cleared");
                        }
                        KeyCode::Esc => break TuiExit::Quit,
                        KeyCode::Tab => complete_tui_input(&mut state.input, &root, state.input_mode),
                        KeyCode::Backspace => { state.input.pop(); }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break TuiExit::Quit,
                        KeyCode::Char(ch) => state.input.push(ch),
                        KeyCode::Enter if state.input_mode == TuiInputMode::AttachmentPath => {
                            let path = std::mem::take(&mut state.input);
                            state.input_mode = TuiInputMode::Task;
                            queue_tui_attachment(&mut state, &root, &path);
                        }
                        KeyCode::Enter => {
                            let command = state.input.trim().to_string();
                            if command == "/settings" { break TuiExit::Settings; }
                            if command == "/attach" {
                                state.input.clear();
                                state.input_mode = TuiInputMode::AttachmentPath;
                                push_tui_activity(&mut state, "Enter or paste a file path to attach to the next task.");
                            } else if let Some(path) = command.strip_prefix("/attach ") {
                                state.input.clear();
                                queue_tui_attachment(&mut state, &root, path);
                            } else if command == "/attachments" {
                                state.input.clear();
                                let count = state.attachments.len();
                                push_tui_activity(
                                    &mut state,
                                    format!("{count} attachment(s) queued for the next task"),
                                );
                            } else if command == "/clear-attachments" {
                                state.input.clear();
                                state.attachments.clear();
                                push_tui_activity(&mut state, "Attachments cleared");
                            } else if command == "/doctor" {
                                // A turn that stops on a failing backend tells
                                // the operator to check the backend is still
                                // serving the model. Until this existed there
                                // was no way to do that without leaving the
                                // conversation, which is the console giving an
                                // instruction it does not support.
                                state.input.clear();
                                push_tui_activity(&mut state, "Asking the backend how it is…");
                                let said = match doctor(&runtime).await {
                                    Ok(report) => summarise_doctor(&report),
                                    Err(error) => format!("could not ask: {}", error.context),
                                };
                                push_tui_transcript(&mut state, said);
                            } else if command == "/verify" {
                                // The turn's own verdict says whether the
                                // repository's checks passed after a change.
                                // It could not say which checks those were, or
                                // run them when nothing had changed -- so an
                                // operator who wanted to know where the
                                // repository stood had to leave.
                                state.input.clear();
                                push_tui_activity(&mut state, "Running the repository's own checks…");
                                let said = match verify_in(&root, None, "targeted".into(), granted_now(&session_grants)).await {
                                    Ok(report) => summarise_verification(&report),
                                    Err(error) => format!("could not verify: {}", error.context),
                                };
                                push_tui_transcript(&mut state, said);
                            } else if command == "/report" {
                                state.input.clear();
                                let said = match report_in(&root, conversation_id.to_string(), "json".into()) {
                                    Ok(report) => summarise_session(&report),
                                    Err(error) => format!("nothing recorded yet: {}", error.context),
                                };
                                push_tui_transcript(&mut state, said);
                            } else if command == "/sessions" {
                                state.input.clear();
                                let said = match list_sessions() {
                                    Ok(report) => summarise_sessions_list(&report),
                                    Err(error) => format!("could not list sessions: {}", error.context),
                                };
                                push_tui_transcript(&mut state, said);
                            } else if command == "/session" {
                                state.input.clear();
                                push_tui_transcript(&mut state, "name a session: /session <name>");
                            } else if let Some(name) = command.strip_prefix("/session ") {
                                state.input.clear();
                                let name = name.trim();
                                let said = if name.is_empty() {
                                    "name a session: /session <name>".to_owned()
                                } else {
                                    match show_session(name) {
                                        Ok(report) => summarise_named_session(&report),
                                        Err(error) => format!("could not show session: {}", error.context),
                                    }
                                };
                                push_tui_transcript(&mut state, said);
                            } else if command == "/changes" {
                                state.input.clear();
                                let changed = continuity
                                    .checkpoint
                                    .lock()
                                    .map(|checkpoint| checkpoint.changed_files.clone())
                                    .unwrap_or_default();
                                let said = conversation_changes(&root, &changed).await;
                                push_tui_transcript(&mut state, said);
                            } else if command == "/resume" {
                                state.input.clear();
                                if state.thinking {
                                    push_tui_transcript(&mut state, "• A turn is running; resume after it finishes.");
                                } else {
                                    resume_into(&root, &mut state, &mut messages, &mut conversation_id, &continuity);
                                    state.run_id = Some(conversation_id);
                                }
                            } else if command == "/diagnose" {
                                // The conversation writes `loop.detected` and
                                // `no_progress.detected` into its own events
                                // now. The detectors that read them were only
                                // reachable from the command line, so the
                                // evidence existed and the person looking at
                                // the stuck conversation could not see it.
                                state.input.clear();
                                let said = match diagnose_in(&root, conversation_id.to_string()) {
                                    Ok(report) => summarise_diagnosis(&report),
                                    Err(error) => format!("nothing to diagnose yet: {}", error.context),
                                };
                                push_tui_transcript(&mut state, said);
                            } else if !state.input.trim().is_empty() {
                                if state.thinking {
                                    // Delivered at the turn's next safe point
                                    // rather than refused: redirecting a long
                                    // turn should not require stopping it.
                                    let text = std::mem::take(&mut state.input);
                                    push_tui_transcript(&mut state, format!("you (while it works): {text}"));
                                    push_tui_activity(&mut state, "↳ will be read before the next action");
                                    if let Ok(mut queue) = continuity.steering.lock() {
                                        queue.push(text);
                                    }
                                    continue;
                                }
                                if config.model.is_none() { push_tui_transcript(&mut state, "✗ Choose a model in Settings first"); continue; }
                                if !chat_is_prepared(&config) { push_tui_transcript(&mut state, "✗ Prepare the selected model in Settings first"); continue; }
                                let text = std::mem::take(&mut state.input);
                                let said = task_with_tui_attachments(text, &state.attachments);
                                state.attachments.clear();
                                push_tui_transcript(&mut state, format!("you: {said}"));
                                messages.push(ChatMessage::text("user", said));
                                state.thinking = true;
                                // A fresh flag per turn, so a stop pressed for
                                // the last one does not end the next.
                                stop = Arc::new(AtomicBool::new(false));
                                let runtime = runtime.clone();
                                let config = config.clone();
                                let root = root.clone();
                                let history = messages.clone();
                                let stop = Arc::clone(&stop);
                                let step_sender = step_sender.clone();
                                let steps: StepSink = Box::new(move |step| {
                                    let _ = step_sender.send(step);
                                });
                                let continuity = continuity.clone();
                                let approvals: Arc<dyn pwr_orchestrator::ApprovalPrompt> = Arc::new(ConsoleApproval {
                                    requests: approval_sender.clone(),
                                    session_grants: Arc::clone(&session_grants),
                                });
                                let grants = Arc::clone(&session_grants);
                                chat_task = Some(tokio::task::spawn_local(async move {
                                    console_turn(root, runtime, config, conversation_id, stop, steps, history, continuity, approvals, grants).await
                                }));
                            }
                        }
                        _ => {}
                    }
                    _ => {}
                }
            }
        }
    };
    if let Some(task) = chat_task {
        task.abort();
    }
    event_reader_stop.store(true, Ordering::Release);
    drop(event_receiver);
    let _ = tokio::time::timeout(Duration::from_secs(1), event_reader).await;
    let _ = execute!(stdout, DisableBracketedPaste);
    ratatui::restore();
    exit
}

async fn chat(runtime: RuntimeFactory, args: ChatArgs) -> i32 {
    let mut runtime = runtime;
    let runtime = &mut runtime;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return print(
            false,
            Err::<serde_json::Value, _>(SafeError {
                category: "invalid_input",
                context: "interactive chat requires a terminal; use `pwr run` for scripts".into(),
            }),
        );
    }
    let root = match std::env::current_dir().and_then(|path| path.canonicalize()) {
        Ok(root) => root,
        Err(error) => {
            return print(
                false,
                Err::<serde_json::Value, _>(SafeError {
                    category: "internal",
                    context: error.to_string(),
                }),
            );
        }
    };
    let mut config = match load_chat_config(&root) {
        Ok(config) => config,
        Err(error) => return print(false, Err::<serde_json::Value, _>(error)),
    };
    // A command-line choice is an intentional workspace setting. Recording it
    // makes the first manual GGUF run one command instead of a trip through a
    // legacy setup screen on every later launch.
    config.backend = Some(runtime.kind().id().to_owned());
    if let Some(model) = args.model.as_deref()
        && config.model.as_deref() != Some(model)
    {
        config.model = Some(model.to_owned());
        config.profile = None;
        config.prepared_for_model = None;
        config.prepared_without_calibration = false;
    }
    if let Err(error) = hydrate_chat_readiness(&root, runtime, &mut config, true).await {
        return print(false, Err::<serde_json::Value, _>(error));
    }
    if let Err(error) = save_chat_config(&root, &config) {
        return print(false, Err::<serde_json::Value, _>(error));
    }
    let mut resume = args.resume;
    let mut pending_attachments = Vec::new();
    for attachment in args.attachments {
        match attach_chat_file(&root, &attachment) {
            Ok(message) => pending_attachments.push(message.content),
            Err(error) => eprintln!("attachment skipped: {}", error.context),
        }
    }
    let mut settings_messages = Vec::new();
    match configure_chat_interactively(&root, runtime, &mut config, &mut settings_messages).await {
        Ok(true) => return 0,
        Ok(false) => {}
        Err(error) => return print(false, Err::<serde_json::Value, _>(error)),
    }
    // What the console found in memory before it did anything, so quitting
    // releases what the console loaded and leaves what the person had.
    let resident_at_start = match &config.model {
        Some(model) => resident(runtime, model).await,
        None => None,
    };
    let model_at_start = config.model.clone();
    loop {
        match run_tui(
            root.clone(),
            config.clone(),
            runtime.clone(),
            std::mem::take(&mut pending_attachments),
            std::mem::take(&mut resume),
        )
        .await
        {
            TuiExit::Quit => {
                if let Some(model) = &config.model {
                    // A model chosen in Settings after the console started was
                    // not the person's to begin with.
                    let before = if config.model == model_at_start {
                        resident_at_start
                    } else {
                        Some(false)
                    };
                    release_if_loaded_here(runtime, model, before).await;
                }
                return 0;
            }
            TuiExit::Settings => {
                match configure_chat_interactively(
                    &root,
                    runtime,
                    &mut config,
                    &mut settings_messages,
                )
                .await
                {
                    Ok(true) => return 0,
                    Ok(false) => continue,
                    Err(error) => return print(false, Err::<serde_json::Value, _>(error)),
                }
            }
        }
    }
}

/// Whether `model` is in its backend's memory now. `None` when that cannot be
/// found out, in which case nothing will be released on its behalf.
async fn resident(runtime: &RuntimeFactory, model: &str) -> Option<bool> {
    let selection = runtime
        .select(model.to_owned(), Duration::from_secs(10))
        .ok()?;
    selection
        .backend
        .is_resident(&selection.deployment)
        .await
        .ok()
}

/// Frees `model` if this command put it in memory, and leaves it if it was
/// there already.
///
/// A model the person loaded themselves is theirs to keep. One this command
/// loaded is not: LM Studio holds a model until something unloads it, and
/// after a calibration or a campaign ended the model stayed resident with
/// nothing using it -- reported by the person running the machine. Said on
/// stderr, so a `--json` result on stdout stays one document.
async fn release_if_loaded_here(
    runtime: &RuntimeFactory,
    model: &str,
    resident_before: Option<bool>,
) {
    if resident_before != Some(false) {
        return;
    }
    let Ok(selection) = runtime.select(model.to_owned(), Duration::from_secs(30)) else {
        return;
    };
    if !matches!(
        selection.backend.is_resident(&selection.deployment).await,
        Ok(true)
    ) {
        return;
    }
    match selection.backend.release(&selection.deployment).await {
        Ok(()) => eprintln!("released {model} from memory"),
        Err(error) => eprintln!("could not release {model} from memory: {error}"),
    }
}

async fn dispatch(cli: Cli) -> i32 {
    // Resolved once. Every path below is handed a value that already carries
    // whether leaving this machine was granted, so no later constructor can
    // reach a remote backend without having been given one that says so.
    // Named on the command line, else remembered by this workspace, else MLX.
    // A workspace served by a non-default engine should not have to be told so
    // on every invocation, and a flag should still win when it is given.
    let named = cli.backend.clone().or_else(|| {
        std::env::current_dir()
            .ok()
            .and_then(|root| load_chat_config(&root).ok())
            .and_then(|config| config.backend)
    });
    let from_flag = cli.backend.is_some();
    let requested = named.unwrap_or_else(|| BackendKind::Mlx.id().to_owned());
    let kind = match BackendKind::parse(&requested) {
        Some(kind) => kind,
        // A workspace that remembers an engine this build does not have is
        // moved to the engine rather than refused: the choice it saved no
        // longer exists.
        None if !from_flag => {
            eprintln!(
                "note: this workspace was set to {requested}, which PWR does not have; \
                 running on its own engine (mlx)."
            );
            BackendKind::Mlx
        }
        None => {
            return print::<()>(
                cli.json,
                Err(SafeError {
                    category: "invalid_input",
                    context: format!(
                        "{requested} is not an engine this build has; expected mlx or llama"
                    ),
                }),
            );
        }
    };
    let runtime = RuntimeFactory::local(kind);
    match cli.command.unwrap_or(Command::Chat(ChatArgs::default())) {
        // The console owns its factory, because the backend is one of the
        // things it lets you change.
        Command::Chat(args) => chat(runtime, args).await,
        Command::Serve { stdio: _ } => serve_stdio(runtime).await,
        Command::Doctor => print(cli.json, doctor(&runtime).await),
        Command::Models(m) => match m.command {
            ModelsCommand::DownloadPlan {
                artifact,
                destination_root,
            } => print(cli.json, download_plan(artifact, destination_root)),
            ModelsCommand::Download {
                artifact,
                destination_root,
            } => print(
                cli.json,
                download_artifact(
                    artifact,
                    destination_root,
                    &mut |_| {},
                    &AtomicBool::new(false),
                )
                .await
                .map_err(download_error),
            ),
        },
        Command::Repo(r) => match r.command {
            RepoCommand::Index { path } => print(
                cli.json,
                index_repository(path.unwrap_or_else(|| PathBuf::from("."))),
            ),
            RepoCommand::Rank {
                query,
                path,
                max,
                budget,
                content,
                semantic,
            } => print(
                cli.json,
                rank_passages(
                    path.unwrap_or_else(|| PathBuf::from(".")),
                    &query,
                    max,
                    budget,
                    content,
                    semantic,
                ),
            ),
        },
        Command::Eval(e) => match e.command {
            EvalCommand::Suite {
                file,
                reports,
                strict,
            } => print(cli.json, run_suite(&file, reports.as_deref(), strict)),
        },
    }
}

fn run_suite(
    file: &Path,
    reports: Option<&Path>,
    strict: bool,
) -> Result<pwr_eval::suite::SuiteReport, SafeError> {
    let suite = pwr_eval::suite::load(file).map_err(|error| SafeError {
        category: "invalid_input",
        context: error.to_string(),
    })?;
    let report = pwr_eval::suite::run(&suite, EVAL_HARNESS_REV, reports);
    let regressions: Vec<&str> = report
        .regressions()
        .iter()
        .map(|result| result.id.as_str())
        .collect();
    if strict && !regressions.is_empty() {
        return Err(SafeError {
            category: "task_failed",
            context: format!(
                "{} of {} cases regressed: {}",
                regressions.len(),
                report.cases,
                regressions.join(", ")
            ),
        });
    }
    Ok(report)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let json = cli.json;
    let operation = dispatch(cli);
    tokio::pin!(operation);
    let code = tokio::select! {
        code = &mut operation => code,
        signal = tokio::signal::ctrl_c() => {
            if signal.is_ok() {
                print::<serde_json::Value>(json, Err(SafeError {
                    category: "cancelled",
                    context: "operation cancelled by user".into(),
                }))
            } else {
                operation.await
            }
        }
    };
    std::process::exit(code);
}

/// Where the workspace stands in version control, as far as it can be read.
///
/// Reported rather than assumed: a workspace need not be a git checkout, and a
/// session that says nothing about the branch is honest where one that invents
/// `main` is not. Every field is absent when it cannot be read.
fn version_control_state(root: &Path) -> serde_json::Value {
    let read = |args: &[&str]| -> Option<String> {
        // A workspace's own configuration is not ours to run: `status` would
        // start whatever `core.fsmonitor` names, outside any sandbox.
        let output = std::process::Command::new("git")
            .args(["-c", "core.fsmonitor=false"])
            .args(args)
            .current_dir(root)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    };
    let mut state = serde_json::Map::new();
    if let Some(branch) = read(&["rev-parse", "--abbrev-ref", "HEAD"]) {
        state.insert("branch".into(), branch.into());
    }
    if let Some(head) = read(&["rev-parse", "HEAD"]) {
        state.insert("head".into(), head.into());
    }
    if let Some(status) = read(&["status", "--porcelain"]) {
        state.insert("uncommitted_files".into(), status.lines().count().into());
    }
    serde_json::Value::Object(state)
}

fn open_store() -> Result<(std::path::PathBuf, pwr_store::Store), SafeError> {
    let root = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?;
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite")).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    Ok((root, store))
}

fn list_sessions() -> Result<serde_json::Value, SafeError> {
    let (_, store) = open_store()?;
    let sessions = store.sessions().map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    Ok(serde_json::json!({
        "sessions": sessions
            .iter()
            .map(|s| serde_json::json!({
                "name": s.name,
                "root": s.root,
                "runs": s.runs.len(),
                "last_opened_at": s.last_opened_at,
                "last_task": store
                    .latest_payload(*s.runs.last().expect("a session has a run"), "run.started")
                    .ok()
                    .flatten()
                    .and_then(|p| p["task"].as_str().map(str::to_string)),
            }))
            .collect::<Vec<_>>(),
    }))
}

fn show_session(name: &str) -> Result<serde_json::Value, SafeError> {
    let (root, store) = open_store()?;
    let runs = store.session_runs(name).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    if runs.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: format!("no session named {name} in this workspace"),
        });
    }
    // The same ledger the next run of this session would be given, so what is
    // shown is what the deployment would see rather than a separate summary
    // that could describe it differently.
    let ledger = pwr_orchestrator::session_ledger(&store, &runs, &root).map_err(|e| SafeError {
        category: "internal",
        context: e,
    })?;
    // Where the session started, as recorded at the time, beside where the
    // workspace stands now. A session resumed onto a different branch is a
    // thing the user needs to see before they resume it, not after.
    let opened_at = store
        .events_for_run(runs[0])
        .map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?
        .into_iter()
        .find(|event| event.event_type == "session.opened")
        .map(|event| event.payload["version_control"].clone())
        .unwrap_or(serde_json::Value::Null);
    let now = version_control_state(&root);
    // A run that recorded no terminal event stopped without saying how: a
    // crash, a kill, or a machine going away. Every other exit writes one,
    // including an interruption. Surfacing it here is what makes it a fact a
    // person can act on rather than something buried in the log.
    let interrupted = runs
        .last()
        .and_then(|run_id| store.typed_events_for_run(*run_id).ok())
        .map(|events| pwr_orchestrator::RunState::replay(&events))
        .filter(pwr_orchestrator::RunState::interrupted)
        .map(|state| {
            serde_json::json!({
                "run": runs.last().map(ToString::to_string),
                "state": format!("{:?}", state.state),
                "actions_spent": state.actions_spent,
                "files_changed": state.changed_files.len(),
                "adopted_verifiers": state.adopted_verifiers,
                "plan_steps_total": state.plan.len(),
                "plan_steps_done": state.steps_done.len(),
                "context_tokens": state.context_tokens,
            })
        });
    Ok(serde_json::json!({
        "name": name,
        "runs": runs.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
        "opened_on": opened_at,
        "workspace_now": now,
        "interrupted_run": interrupted,
        "ledger": ledger,
    }))
}

async fn doctor(runtime: &RuntimeFactory) -> Result<serde_json::Value, SafeError> {
    let hardware = probe_hardware().await;
    let provider = runtime
        .backend(Duration::from_secs(4))
        .map_err(provider_error)?;
    let backend_id = provider.backend_id();
    let state = provider.runtime_state().await.map_err(provider_error);
    let state = state
        .map(|state| serde_json::to_value(state).unwrap())
        .unwrap_or_else(|error| serde_json::json!({"status":"unavailable","reason":error.context}));
    // Named by the backend that was actually addressed.
    let report = serde_json::json!({
        "hardware": hardware,
        "backend": backend_id,
        "backend_runtime": state,
        "facts_only": true,
    });
    Ok(report)
}
/// Context tier the boundary probe deliberately under-provisions.
const BOUNDARY_SMALL_CONTEXT: u32 = 512;
/// Tier large enough to hold the boundary prompt, used as the reference count.
const BOUNDARY_REFERENCE_CONTEXT: u32 = 16_384;
/// Repetitions of filler; the prompt must clearly exceed the small tier.
const BOUNDARY_FILLER_REPEATS: usize = 400;

/// Builds a prompt far larger than the small tier, carrying a needle at the
/// front so a truncating deployment can be seen to have dropped it.
fn boundary_prompt() -> String {
    let filler = "The quick brown fox jumps over the lazy dog. ".repeat(BOUNDARY_FILLER_REPEATS);
    format!(
        "REMEMBER THIS CODEWORD: ZEPHYR-8813.\n{filler}\nWhat was the codeword? Answer with the codeword only."
    )
}

/// Names what a deployment did with a prompt that exceeded its configured
/// context, given how many prompt tokens it evaluated with and without the
/// bound.
///
/// Fewer tokens with no error means the context was dropped and nothing said
/// so; that silence is the whole point of measuring this.
fn boundary_behaviour(reference_tokens: u64, bounded_tokens: u64) -> &'static str {
    if bounded_tokens < reference_tokens {
        "truncated_silently"
    } else {
        "limit_not_enforced"
    }
}

/// Observes what a deployment does when a prompt exceeds its configured context.
///
/// This is not one behaviour across deployments. Measured on this host at the
/// same boundary: one accepted the whole prompt and recalled the needle,
/// ignoring the limit; one truncated to a fraction of the prompt and lost the
/// needle with no error at all; one rejected cleanly with a typed error naming
/// the counts. Silent truncation is the case a scheduler must know about,
/// because nothing in the reply says the context was dropped.
/// Observes the boundary and puts the deployment back as it was found.
///
/// The probe works by asking for a context small enough to be exceeded, and on
/// a backend that honours the request that is exactly what it gets. Measured on
/// an 80B whose backend does honour it: the deployment was left loaded at 512
/// tokens, and the edit probe that runs next failed three times out of three
/// with "the number of tokens to keep from the initial prompt is greater than
/// the context length" — a capability reported as unproven because of the
/// probe before it rather than because of the deployment.
///
/// A probe that leaves the machine worse than it found it is a probe that
/// measures itself.
async fn probe_context_boundary(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
) -> Observation {
    // What it was serving before anything asked it to serve less.
    let restore_to = provider
        .prepare_context(deployment, BOUNDARY_REFERENCE_CONTEXT)
        .await
        .ok();
    let observation = probe_context_boundary_inner(provider, deployment).await;
    if let Some(window) = restore_to {
        // Best effort, and its failure is not the boundary's result: the
        // observation above is what was measured either way.
        let _ = provider.prepare_context(deployment, window).await;
    }
    observation
}

async fn probe_context_boundary_inner(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
) -> Observation {
    let prompt = boundary_prompt();
    let ask = async |context_tokens: u32| {
        // The probe has to run at the window it names. On a backend where the
        // window is fixed at load time, generating without asking for it first
        // measures whatever instance happened to be running -- which is how a
        // request for 512 came to be served 4044 prompt tokens and reported as
        // the deployment ignoring its limit.
        let granted = provider
            .prepare_context(deployment, context_tokens)
            .await
            .map_err(|error| error.to_string())?;
        if granted != context_tokens {
            return Err(format!(
                "backend served {granted} context tokens when asked for {context_tokens}"
            ));
        }
        let request = ModelRequest {
            deployment: deployment.clone(),
            context_tokens,
            tools: None,
            seed: None,
            sampling: Default::default(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: prompt.clone(),
                ..Default::default()
            }],
        };
        match provider.chat(request).await {
            Ok(stream) => pwr_provider::collect_reply(stream)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        }
    };
    // The reference establishes how many tokens the prompt really is, as the
    // backend counts them.
    let reference = match ask(BOUNDARY_REFERENCE_CONTEXT).await {
        Ok(reply) => reply,
        Err(reason) => {
            return Observation::Unknown {
                reason: format!("boundary reference request failed: {reason}"),
            };
        }
    };
    let Some(reference_tokens) = reference.metrics.as_ref().and_then(|m| m.prompt_tokens) else {
        return Observation::Unknown {
            reason: "backend reported no prompt token count; boundary is unmeasurable".into(),
        };
    };
    if reference_tokens <= BOUNDARY_SMALL_CONTEXT as u64 {
        return Observation::Unknown {
            reason: "boundary prompt did not exceed the small context tier".into(),
        };
    }
    match ask(BOUNDARY_SMALL_CONTEXT).await {
        Err(reason) => Observation::Observed(serde_json::json!({
            "behaviour": "rejected",
            "requested_context": BOUNDARY_SMALL_CONTEXT,
            "reference_prompt_tokens": reference_tokens,
            "detail": reason,
        })),
        Ok(reply) => {
            let bounded = reply.metrics.as_ref().and_then(|m| m.prompt_tokens);
            let recalled = reply.content.contains("ZEPHYR-8813");
            match bounded {
                None => Observation::Unknown {
                    reason: "bounded request reported no prompt token count".into(),
                },
                Some(bounded) => Observation::Observed(serde_json::json!({
                    "behaviour": boundary_behaviour(reference_tokens, bounded),
                    "requested_context": BOUNDARY_SMALL_CONTEXT,
                    "reference_prompt_tokens": reference_tokens,
                    "bounded_prompt_tokens": bounded,
                    "needle_recalled": recalled,
                })),
            }
        }
    }
}

/// Observes whether a deployment can produce a usable hash-guarded edit.
///
/// This measures the capability, not task skill: given a file and the artifact
/// hash a read returned, does the deployment emit an `apply_replace` call whose
/// path and `expected_hash` the policy actually accepts? The edit is executed
/// against a throwaway workspace, because a well-formed call that the hash
/// guard rejects is not an edit capability. Whether a deployment can *choose*
/// the right edit is an evaluation question, not a discovery one.
async fn probe_edit(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
    trials: u32,
) -> Observation {
    // Proposing an edit is sampled behaviour just as emitting a tool call is, so
    // a single miss is not evidence that a deployment cannot edit.
    let mut successes = Vec::new();
    let mut failures = Vec::new();
    let mut first_try = 0usize;
    let mut after_refusal = 0usize;
    for trial in 1..=trials {
        match probe_edit_trial(provider, deployment).await {
            EditTrial::FirstTry(mut value) => {
                value["trial"] = serde_json::json!(trial);
                value["corrected"] = serde_json::json!(false);
                first_try += 1;
                successes.push(value);
            }
            EditTrial::AfterRefusal { mut value, reason } => {
                value["trial"] = serde_json::json!(trial);
                value["corrected"] = serde_json::json!(true);
                value["first_refusal"] = serde_json::json!(reason);
                after_refusal += 1;
                successes.push(value);
            }
            EditTrial::Failed(reason) => failures.push(format!("trial {trial}: {reason}")),
        }
    }
    if successes.is_empty() {
        return Observation::Unknown {
            reason: format!(
                "no usable edit in {trials} trial(s): {}",
                failures.join("; ")
            ),
        };
    }
    let trials = trials as usize;
    Observation::Observed(serde_json::json!({
        "trials": trials,
        "edits": successes.len(),
        // Right the first time, every time. Unchanged in meaning, so anything
        // that read it before still reads the same thing.
        "reliable": first_try == trials,
        "first_try": first_try,
        // Right within one refusal, every time. This is what the agent loop
        // depends on, and the two differ enough to matter: measured on a 35B,
        // one first-try edit in three and three drivable ones.
        "after_refusal": after_refusal,
        "drivable": successes.len() == trials,
        "evidence": successes,
        "failures": failures,
    }))
}

/// One trial: the first attempt, and -- when the first failed in a way the
/// deployment could act on -- a second with the refusal handed back.
///
/// The probe used to score only the first attempt, and the conversation scores
/// what happens after a refusal, so the two disagreed about the same
/// deployment. Measured: a 35B recorded here as making one usable edit in three
/// went on to complete a thirty-four turn task with fourteen edits, because a
/// refusal that names the missing field is one the deployment corrects. An 80B
/// was recorded as unmeasurable for calling `apply_replace` with another
/// capability's fields -- which is exactly what the refusal now explains.
///
/// Both numbers are kept. `reliable` still means right the first time, so
/// nothing that read it before reads something else now; `drivable` is the
/// measure the agent loop actually depends on.
async fn probe_edit_trial(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
) -> EditTrial {
    match probe_edit_once(provider, deployment, None).await {
        Observation::Observed(value) => EditTrial::FirstTry(value),
        Observation::Unknown { reason } => {
            // A stream that failed or a workspace that could not be made is not
            // the deployment's mistake, and handing it back teaches it nothing.
            if !correctable(&reason) {
                return EditTrial::Failed(reason);
            }
            match probe_edit_once(provider, deployment, Some(&reason)).await {
                Observation::Observed(value) => EditTrial::AfterRefusal { value, reason },
                Observation::Unknown { reason: second } => {
                    EditTrial::Failed(format!("{reason}; after being told, again: {second}"))
                }
            }
        }
    }
}

/// Whether a refusal is one the deployment can do anything about.
fn correctable(reason: &str) -> bool {
    [
        "did not match the declared schema",
        "instead of apply_replace",
        "no tool call at all",
    ]
    .iter()
    .any(|mark| reason.contains(mark))
}

enum EditTrial {
    FirstTry(serde_json::Value),
    AfterRefusal {
        value: serde_json::Value,
        reason: String,
    },
    Failed(String),
}

async fn probe_edit_once(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
    correction: Option<&str>,
) -> Observation {
    let Ok(workspace) = tempfile::tempdir() else {
        return Observation::Unknown {
            reason: "could not create a probe workspace".into(),
        };
    };
    let original = "pub fn value() -> i32 {
    1
}
";
    if std::fs::write(workspace.path().join("probe.rs"), original).is_err() {
        return Observation::Unknown {
            reason: "could not seed the probe workspace".into(),
        };
    }
    let expected_hash = hash_bytes(original);
    let request = ModelRequest {
        deployment: deployment.clone(),
        context_tokens: 4096,
        tools: Some(provider.render_tools(&pwr_orchestrator::action_tool_catalog())),
        seed: None,
        sampling: Default::default(),
        messages: vec![
            ChatMessage {
                role: "system".into(),
                content:
                    "Take one action by calling one of the provided tools; several independent \
                     reads may be asked for in the same turn."
                        .into(),
                ..Default::default()
            },
            ChatMessage {
                role: "user".into(),
                content: format!(
                    "File probe.rs contains:\n{original}\nIts artifact_hash is {expected_hash}. \
                     Call apply_replace on probe.rs so value() returns 2, passing that \
                     hash as expected_hash."
                ),
                ..Default::default()
            },
        ],
    };
    // The second attempt sees why the first was refused, which is what the
    // conversation does and what the probe never did.
    let request = match correction {
        None => request,
        Some(reason) => {
            let mut request = request;
            request.messages.push(ChatMessage::text(
                "tool",
                format!("That call was refused: {reason}. Nothing was done. Send it again."),
            ));
            request
        }
    };
    let policy = pwr_tools::ToolPolicy {
        root: workspace.path().to_path_buf(),
        extra_readable: Vec::new(),
        protected: Vec::new(),
        allow_commands: vec![],
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(10),
        sandbox: pwr_tools::SandboxPolicy::Disabled,
        approvals: Vec::new(),
    };
    // A deployment that reads the file before editing it is being careful,
    // not failing: the agent loop answers the read and lets it go on. Measured
    // 2026-09-19: Qwen3.8-27B read probe.rs first in every trial, after being
    // told to edit it too, and the probe recorded it as unable to edit. Up to
    // two reads are answered as the loop would, and counted.
    let mut request = request;
    let mut reads_before_edit = 0usize;
    let call = loop {
        let reply = match provider.chat(request.clone()).await {
            Ok(stream) => match pwr_provider::collect_reply(stream).await {
                Ok(reply) => reply,
                Err(error) => {
                    return Observation::Unknown {
                        reason: format!("edit probe stream failed: {error}"),
                    };
                }
            },
            Err(error) => {
                return Observation::Unknown {
                    reason: format!("edit probe failed: {error}"),
                };
            }
        };
        // Read through the family's conventions, as a run reads it.
        // Measured 2026-09-29: Qwen2.5-Coder writes its call inside <tools>,
        // which its adapter reads and the raw reply does not show, so the
        // probe recorded "no tool call at all" and kept it out of every suite.
        let canonical = pwr_compat::adapter_for(None, &deployment.model_ref).normalize(&reply);
        let reply = pwr_provider::ModelReply {
            content: canonical.narrative,
            tool_calls: canonical.tool_calls,
            ..reply
        };
        if let Some(call) = reply
            .tool_calls
            .iter()
            .find(|call| call.name == "apply_replace")
            .cloned()
        {
            break call;
        }
        let only_reads = !reply.tool_calls.is_empty()
            && reply.tool_calls.iter().all(|call| call.name == "read_file");
        if only_reads && reads_before_edit < 2 {
            reads_before_edit += 1;
            request.messages.push(ChatMessage {
                role: "assistant".into(),
                content: reply.content.clone(),
                tool_calls: reply.tool_calls.clone(),
                ..Default::default()
            });
            for call in &reply.tool_calls {
                let path = call.arguments["path"].as_str().unwrap_or("probe.rs");
                let outcome = match pwr_tools::read_file(&policy, Path::new(path)) {
                    Ok(read) => serde_json::to_value(read).unwrap_or_default(),
                    Err(error) => serde_json::json!({"denied": error.to_string()}),
                };
                request.messages.push(pwr_orchestrator::tool_result_message(
                    outcome,
                    None,
                    call.id.clone(),
                ));
            }
            continue;
        }
        // Naming what it called instead turns a bare miss into a diagnosis.
        let proposed: Vec<&str> = reply
            .tool_calls
            .iter()
            .map(|call| call.name.as_str())
            .collect();
        return Observation::Unknown {
            reason: if proposed.is_empty() {
                "model proposed no tool call at all".into()
            } else {
                format!("model proposed {proposed:?} instead of apply_replace")
            },
        };
    };
    let call = &call;
    let action = match pwr_orchestrator::action_from_tool_call(call) {
        Ok(action) => action,
        Err(reason) => {
            return Observation::Unknown {
                reason: format!("edit call did not match the declared schema: {reason}"),
            };
        }
    };
    // Any of the edits PWR carries out is an edit. Measured 2026-09-29:
    // Ornith-1.5-9B read probe.rs, then called apply_replace with `find` and
    // `replace` -- a targeted edit, with the right hash -- which decodes to
    // replace_text, and the probe, taking only a whole-file replacement,
    // recorded three trials as unable to edit and kept it out of every suite.
    let (path, applied, form) = match action {
        pwr_tools::ActionProposal::ApplyReplace {
            path,
            expected_hash,
            replacement,
        } => {
            let applied =
                pwr_tools::apply_replace(&policy, Path::new(&path), &expected_hash, &replacement);
            (path, applied, "apply_replace")
        }
        pwr_tools::ActionProposal::ReplaceText {
            path,
            expected_hash,
            find,
            replace,
        } => {
            let applied =
                pwr_tools::replace_text(&policy, Path::new(&path), &expected_hash, &find, &replace);
            (path, applied, "replace_text")
        }
        pwr_tools::ActionProposal::ApplyPatchHunks {
            path,
            expected_hash,
            hunks,
        } => {
            let applied = pwr_tools::apply_patch(&policy, Path::new(&path), &expected_hash, &hunks);
            (path, applied, "apply_patch")
        }
        _ => {
            return Observation::Unknown {
                reason: "edit call did not decode to an edit action".into(),
            };
        }
    };
    // The edit is applied for real, in a throwaway workspace: a call the hash
    // guard refuses is not evidence of an edit capability.
    match applied {
        Ok(result) => Observation::Observed(serde_json::json!({
            "path": path,
            "form": form,
            "reads_before_edit": reads_before_edit,
            "hash_guard_satisfied": true,
            "previous_hash": result.previous_hash,
            "new_hash": result.new_hash,
        })),
        Err(error) => Observation::Unknown {
            reason: format!("proposed edit was refused by policy: {error}"),
        },
    }
}

/// The declared artifact inventory, or an empty one.
///
/// Absent is a decision -- this host manages its own models through the
/// backend -- while present and unreadable is a mistake, and the two must not
/// look the same.
fn load_artifact_registry(path: &Path) -> Result<pwr_domain::ArtifactRegistry, SafeError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error)
            if error.kind() == std::io::ErrorKind::NotFound && path == Path::new(ARTIFACT_FILE) =>
        {
            PACKAGED_ARTIFACT_REGISTRY.as_bytes().to_vec()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(pwr_domain::ArtifactRegistry {
                schema_version: 1,
                artifacts: Vec::new(),
            });
        }
        Err(error) => {
            return Err(SafeError {
                category: "internal",
                context: format!("could not read {}: {error}", path.display()),
            });
        }
    };
    let registry: pwr_domain::ArtifactRegistry =
        serde_json::from_slice(&bytes).map_err(|error| SafeError {
            category: "invalid_input",
            context: format!("{} is present but unreadable: {error}", path.display()),
        })?;
    registry.validate().map_err(|error| SafeError {
        category: "invalid_input",
        context: format!("{}: {error}", path.display()),
    })?;
    Ok(registry)
}

/// Where the declared artifact inventory lives.
const ARTIFACT_FILE: &str = "strategies/artifacts.json";

/// The distributable core still knows the pinned inventory when launched
/// outside a source checkout. A workspace-local registry takes precedence.
const PACKAGED_ARTIFACT_REGISTRY: &str = include_str!("../../../strategies/artifacts.json");

/// Bump when any scoring or execution step changes; reports record it.
const EVAL_HARNESS_REV: &str = concat!("eval-", env!("PWR_HARNESS_REV"));

/// Tool offered by the capability probe. Only a call naming it counts as evidence.
const PROBE_TOOL: &str = "probe_echo";
/// Upper bound on chunks read from one probe. The provider timeout bounds wall
/// clock; this bounds memory against a deployment that never stops emitting.
const PROBE_MAX_CHUNKS: usize = 4096;

#[derive(Debug, Default)]
struct ProbeObservation {
    chunks: usize,
    produced_text: bool,
    tool_call_names: Vec<String>,
    matched_tool_call: Option<ToolCall>,
    matched_chunk_index: Option<usize>,
}

/// Read a probe stream to completion rather than judging it by its first chunk.
///
/// A reasoning deployment opens with `thinking` chunks carrying empty content and
/// emits its tool call near the end of the stream, so a first-chunk verdict
/// reports "no native tool call" for models that in fact make one.
async fn drain_probe(
    stream: Result<pwr_provider::ModelStream, pwr_provider::ProviderError>,
    expect_tool: Option<&str>,
) -> Result<ProbeObservation, String> {
    let mut stream = stream.map_err(|error| format!("probe failed: {error}"))?;
    let mut observed = ProbeObservation::default();
    while let Some(next) = stream.next().await {
        let chunk = match next {
            Ok(chunk) => chunk,
            Err(error) if observed.chunks == 0 => {
                return Err(format!("probe stream error: {error}"));
            }
            // Partial evidence is still evidence: a tool call already seen stands.
            Err(_) => break,
        };
        observed.chunks += 1;
        observed.produced_text |= !chunk.content.is_empty();
        for call in chunk.tool_calls {
            if expect_tool.is_some_and(|name| name == call.name)
                && observed.matched_tool_call.is_none()
            {
                observed.matched_chunk_index = Some(observed.chunks);
                observed.matched_tool_call = Some(call.clone());
            }
            observed.tool_call_names.push(call.name);
        }
        if chunk.done || observed.chunks >= PROBE_MAX_CHUNKS {
            break;
        }
    }
    if observed.chunks == 0 {
        return Err("probe returned no chunk".into());
    }
    Ok(observed)
}

/// Fold repeated tool trials into one observation.
///
/// Emission is sampled behaviour, so the rate is part of the fact: a deployment
/// that answers 1 of 3 must never read the same as one that answers 3 of 3.
/// Zero calls stays `unknown` — absence of a call in n trials is not proof the
/// deployment cannot make one.
fn summarize_tool_trials(
    trials: u32,
    successes: Vec<serde_json::Value>,
    failures: Vec<String>,
) -> Observation {
    if successes.is_empty() {
        return Observation::Unknown {
            reason: format!(
                "no native tool call in {trials} trial(s): {}",
                failures.join("; ")
            ),
        };
    }
    Observation::Observed(serde_json::json!({
        "tool": PROBE_TOOL,
        "trials": trials,
        "calls": successes.len(),
        "reliable": successes.len() == trials as usize,
        "evidence": successes,
        "failures": failures,
    }))
}

/// Prompt used to get a deployment generating long enough to interrupt it.
const CANCEL_PROMPT: &str = "Count slowly from 1 to 500, one number per line. Do not stop early.";
/// Chunks to consume before abandoning the stream. Generation must be genuinely
/// underway, or dropping the stream proves nothing.
const CANCEL_AFTER_CHUNKS: usize = 3;

/// Consume a stream until generation is underway, then abandon it.
///
/// Returns how many chunks were read before the drop. Abandoning the stream is
/// what cancels the request: dropping the response body closes the connection.
async fn abandon_mid_stream(
    stream: Result<pwr_provider::ModelStream, pwr_provider::ProviderError>,
    cancel: &pwr_provider::Cancel,
) -> Result<usize, String> {
    let mut stream = stream.map_err(|error| format!("cancellation probe failed: {error}"))?;
    let mut chunks = 0usize;
    while chunks < CANCEL_AFTER_CHUNKS {
        match stream.next().await {
            Some(Ok(chunk)) => {
                chunks += 1;
                // A deployment that finished before we could interrupt it gives
                // no evidence either way.
                if chunk.done {
                    return Err(format!(
                        "stream completed after {chunks} chunk(s) before it could be interrupted"
                    ));
                }
            }
            Some(Err(error)) => return Err(format!("cancellation probe stream error: {error}")),
            None => {
                return Err(format!(
                    "stream ended after {chunks} chunk(s) before it could be interrupted"
                ));
            }
        }
    }
    // Cancelling and then reading is what shows the abandonment took effect:
    // the stream reports `Cancelled` rather than simply ending, and the
    // underlying connection is dropped, which is what stops the backend.
    cancel.cancel();
    let cancelled = matches!(
        stream.next().await,
        Some(Err(pwr_provider::ProviderError::Cancelled))
    );
    drop(stream);
    if !cancelled {
        return Err(format!(
            "stream did not report cancellation after {chunks} chunk(s)"
        ));
    }
    Ok(chunks)
}

/// Observe whether an in-flight generation can be interrupted without wedging
/// the backend.
///
/// Evidence is threefold: generation was genuinely underway, abandoning the
/// stream returned control promptly, and the backend still served a request
/// afterwards. A backend left unresponsive is a failed cancellation, not a
/// successful one.
async fn probe_cancellation(
    provider: &RuntimeBackend,
    deployment: &DeploymentDescriptor,
) -> Observation {
    let request = ModelRequest {
        deployment: deployment.clone(),
        context_tokens: 512,
        tools: None,
        seed: None,
        sampling: Default::default(),
        messages: vec![ChatMessage {
            role: "user".into(),
            content: CANCEL_PROMPT.into(),
            ..Default::default()
        }],
    };
    let started = std::time::Instant::now();
    let cancel = pwr_provider::Cancel::new();
    let chunks = match abandon_mid_stream(
        provider.chat_cancellable(request, cancel.clone()).await,
        &cancel,
    )
    .await
    {
        Ok(chunks) => chunks,
        Err(reason) => return Observation::Unknown { reason },
    };
    let abandoned_after_ms = started.elapsed().as_millis();
    match provider.runtime_state().await {
        Ok(_) => Observation::Observed(serde_json::json!({
            "chunks_before_cancel": chunks,
            "abandoned_after_ms": abandoned_after_ms,
            "backend_responsive_after": true,
        })),
        Err(error) => Observation::Unknown {
            reason: format!("backend did not answer after cancellation: {error}"),
        },
    }
}

async fn inspect(
    runtime: &RuntimeFactory,
    model: String,
    probe: bool,
    timeout: Duration,
    trials: u32,
) -> Result<serde_json::Value, SafeError> {
    let _runtime_lease = probe
        .then(|| pwr_orchestrator::ModelRuntimeLease::acquire("capability probes", &model))
        .transpose()
        .map_err(|context| SafeError {
            category: "resource_busy",
            context,
        })?;
    let selection = runtime.select(model, timeout).map_err(provider_error)?;
    let provider = selection.backend;
    let deployment = selection.deployment;
    let mut inspection = provider
        .inspect(&deployment)
        .await
        .map_err(provider_error)?;
    if probe {
        let request = ModelRequest {
            deployment: deployment.clone(),
            context_tokens: 512,
            tools: None,
            seed: None,
            sampling: Default::default(),
            // Longer than a few tokens: a short answer arrives in a single
            // chunk -- PWR's MLX engine holds back the last 16 characters so
            // a call marker is never half-shown -- and Qwen2.5-Coder, answering
            // "OK", was recorded as unable to stream by an engine that streams.
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "Reply with the numbers from 1 to 20, separated by spaces.".into(),
                ..Default::default()
            }],
        };
        match drain_probe(provider.chat(request).await, None).await {
            Ok(observed) => {
                inspection.definition.capabilities.insert(
                    "chat".into(),
                    Observation::Observed(serde_json::json!({
                        "chunks": observed.chunks,
                        "produced_text": observed.produced_text,
                    })),
                );
                // Streaming is only demonstrated by more than one incremental chunk;
                // a single terminal chunk is a non-streaming reply.
                if observed.chunks > 1 {
                    inspection.definition.capabilities.insert(
                        "streaming".into(),
                        Observation::Observed(serde_json::json!({"chunks": observed.chunks})),
                    );
                } else {
                    inspection.definition.capabilities.insert(
                        "streaming".into(),
                        Observation::Unknown {
                            reason: "deployment returned a single chunk; incremental delivery not demonstrated".into(),
                        },
                    );
                }
            }
            Err(reason) => {
                for key in ["chat", "streaming"] {
                    inspection.definition.capabilities.insert(
                        key.into(),
                        Observation::Unknown {
                            reason: reason.clone(),
                        },
                    );
                }
            }
        }
        let tool_schema = serde_json::json!([{"type":"function","function":{"name":PROBE_TOOL,"description":"Return a probe value without side effects.","parameters":{"type":"object","properties":{"value":{"type":"string"}},"required":["value"]}}}]);
        // A single sample records a coin flip as a fact: some deployments emit a
        // native call only intermittently. Repeat the trial and report the rate.
        let mut successes: Vec<serde_json::Value> = Vec::new();
        let mut failures: Vec<String> = Vec::new();
        for trial in 1..=trials {
            let tool_request = ModelRequest {
                deployment: deployment.clone(),
                context_tokens: 512,
                tools: Some(tool_schema.clone()),
                seed: None,
                sampling: Default::default(),
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: "Call the probe_echo tool with value 'ok'. Do not answer in prose."
                        .into(),
                    ..Default::default()
                }],
            };
            match drain_probe(provider.chat(tool_request).await, Some(PROBE_TOOL)).await {
                // A native call is only credited when the deployment named the
                // tool this probe actually offered.
                Ok(observed) => match observed.matched_tool_call {
                    Some(call) => successes.push(serde_json::json!({
                        "trial": trial,
                        "arguments": call.arguments,
                        "chunk_index": observed.matched_chunk_index,
                        "chunks": observed.chunks,
                    })),
                    None if observed.tool_call_names.is_empty() => {
                        failures.push(format!("trial {trial}: no native tool call"));
                    }
                    None => failures.push(format!(
                        "trial {trial}: called {:?} instead of the offered {PROBE_TOOL} tool",
                        observed.tool_call_names
                    )),
                },
                Err(reason) => failures.push(format!("trial {trial}: {reason}")),
            }
        }
        inspection.definition.capabilities.insert(
            "structured_tools".into(),
            summarize_tool_trials(trials, successes, failures),
        );
        inspection.definition.capabilities.insert(
            "cancellation".into(),
            probe_cancellation(&provider, &deployment).await,
        );
        // Recorded before the boundary probe, because it is what makes the
        // boundary measurable. A backend that fixes the context window
        // elsewhere cannot be asked to run at a smaller one, so nothing can
        // provoke its truncation behaviour -- and an evidence gate reading
        // only "context_boundary: unknown" cannot tell that from a probe that
        // was never run.
        inspection.definition.capabilities.insert(
            "context_window_control".into(),
            Observation::Observed(serde_json::json!(
                provider.capabilities().context_window_control
            )),
        );
        inspection.definition.capabilities.insert(
            "context_boundary".into(),
            probe_context_boundary(&provider, &deployment).await,
        );
        inspection.definition.capabilities.insert(
            "edit".into(),
            probe_edit(&provider, &deployment, trials).await,
        );
        // No capability is left as a declared placeholder: every entry above is
        // an observation or an explained unknown.
    }
    let root = std::env::current_dir()
        .map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?
        .canonicalize()
        .map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?;
    let bytes = serde_json::to_vec_pretty(&inspection).expect("serializable");
    let artifact_name = format!("{}.json", inspection.definition.id);
    let artifact = root.join(".pwr/models").join(&artifact_name);
    write_immutable_artifact(&artifact, &bytes)?;
    if let Some(global) = global_model_evidence_dir() {
        write_immutable_artifact(&global.join(artifact_name), &bytes)?;
    }
    // The window this host would run the model at, and what bounds it: the
    // catalogue's arithmetic (backlog B.3), computed from the model's config
    // and this machine's memory without loading anything.
    let window = match provider.model_facts(&deployment).await {
        Ok(facts) => {
            use pwr_orchestrator::window;
            let shape = window::ModelShape::from_facts(&facts);
            let host = probe_hardware()
                .await
                .total_memory_bytes
                .map(window::HostBudget::with_default_reserve);
            match window::decide(&shape, host.as_ref(), None, None) {
                Ok(decision) => serde_json::json!({
                    "decision": decision,
                    "shape": shape,
                    "host": host,
                    "facts_source": facts.source,
                }),
                Err(error) => serde_json::json!({"error": error.to_string(), "shape": shape}),
            }
        }
        Err(error) => serde_json::json!({"error": error.to_string()}),
    };
    Ok(serde_json::json!({
        "inspection": inspection,
        "artifact": artifact,
        "probed": probe,
        "window": window,
    }))
}

fn load_calibration(path: &Path) -> Result<pwr_domain::CalibrationProfile, SafeError> {
    let bytes = std::fs::read(path).map_err(|e| SafeError {
        category: "invalid_input",
        context: e.to_string(),
    })?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| SafeError {
        category: "invalid_input",
        context: format!("calibration artifact is not JSON: {e}"),
    })?;
    let profile = value
        .pointer("/run/profile")
        .or_else(|| value.pointer("/profile"))
        .unwrap_or(&value);
    let profile: pwr_domain::CalibrationProfile =
        serde_json::from_value(profile.clone()).map_err(|e| SafeError {
            category: "invalid_input",
            context: format!("invalid calibration profile: {e}"),
        })?;
    // A refused run persists no usable profile; say so rather than failing on a
    // missing field.
    if value.pointer("/run/outcome").and_then(|o| o.as_str()) == Some("refused") {
        return Err(SafeError {
            category: "invalid_input",
            context: "this artifact records a refused calibration; it authorises no execution"
                .into(),
        });
    }
    pwr_domain::check_schema_version(profile.schema_version, "calibration profile").map_err(
        |e| SafeError {
            category: "invalid_input",
            context: e.to_string(),
        },
    )?;
    profile.validate().map_err(|e| SafeError {
        category: "invalid_input",
        context: format!("calibration profile is not valid: {e}"),
    })?;
    Ok(profile)
}

/// The semantic section ranker, when the person asked for it with
/// `PWR_SEMANTIC_RETRIEVAL=1` (backlog C.22). Opt-in until a run with a
/// small model at a small window says what it is worth; when the encoder
/// cannot start, retrieval is lexical and stderr says why, once.
fn semantic_ranker_if_requested(
    root: &Path,
) -> (Option<semantic::EmbeddingRanker>, Option<String>) {
    if std::env::var("PWR_SEMANTIC_RETRIEVAL").ok().as_deref() != Some("1") {
        return (None, None);
    }
    match semantic::EmbeddingRanker::open(root) {
        Ok(ranker) => (Some(ranker), None),
        Err(error) => {
            eprintln!("semantic retrieval requested but unavailable, ranking lexically: {error}");
            (None, Some(error))
        }
    }
}

/// What retrieval would deliver for one request, as evidence rather than as
/// prose: each passage's path, lines, hash, rationale and estimated tokens.
fn rank_passages(
    path: PathBuf,
    query: &str,
    max: Option<usize>,
    budget: Option<usize>,
    content: bool,
    semantic: bool,
) -> Result<serde_json::Value, SafeError> {
    let (index, _) =
        pwr_repo::index_incremental(&path, Some(&path.join(".pwr"))).map_err(|error| {
            SafeError {
                category: "invalid_input",
                context: error.to_string(),
            }
        })?;
    let max = max.unwrap_or(pwr_orchestrator::context::RETRIEVAL_MAX_EXCERPTS);
    // The share a turn gives passages, at the window a conversation opens with
    // on this host's default model.
    const WINDOW_FOR_THE_SHARE: usize = 262_144;
    let budget =
        budget.unwrap_or(WINDOW_FOR_THE_SHARE / pwr_orchestrator::context::RETRIEVAL_TOKEN_SHARE);
    let started = std::time::Instant::now();
    let (mut ranker, semantic_note) = if semantic {
        match semantic::EmbeddingRanker::open(&path) {
            Ok(ranker) => {
                let note = format!("fused with {}", ranker.model());
                (Some(ranker), note)
            }
            Err(error) => (None, format!("lexical only: {error}")),
        }
    } else {
        (None, "lexical".to_owned())
    };
    let excerpts = pwr_repo::retrieve_with(
        &path,
        &index,
        query,
        max,
        budget,
        ranker
            .as_mut()
            .map(|ranker| ranker as &mut dyn pwr_repo::SectionRanker),
    )
    .map_err(|error| SafeError {
        category: "internal",
        context: error.to_string(),
    })?;
    let embedded = ranker.as_ref().map_or(0, |ranker| ranker.embedded);
    let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    let tokens: usize = excerpts
        .iter()
        .map(|excerpt| pwr_orchestrator::context::estimate_tokens(&excerpt.content))
        .sum();
    Ok(serde_json::json!({
        "query": query,
        "ranking": semantic_note,
        "sections_embedded_now": embedded,
        "elapsed_ms": elapsed_ms,
        "max_excerpts": max,
        "token_budget": budget,
        "estimated_tokens": tokens,
        "passages": excerpts.iter().map(|excerpt| {
            let mut value = serde_json::json!({
                "path": excerpt.path,
                "first_line": excerpt.first_line,
                "last_line": excerpt.last_line,
                "content_hash": excerpt.content_hash,
                "rationale": excerpt.rationale,
                "estimated_tokens": pwr_orchestrator::context::estimate_tokens(&excerpt.content),
            });
            if content && let Some(object) = value.as_object_mut() {
                object.insert("content".into(), serde_json::json!(excerpt.content));
            }
            value
        }).collect::<Vec<_>>(),
    }))
}

fn index_repository(path: PathBuf) -> Result<serde_json::Value, SafeError> {
    let (index, index_work) = pwr_repo::index_incremental(&path, Some(&path.join(".pwr")))
        .map_err(|e| SafeError {
            category: "invalid_input",
            context: e.to_string(),
        })?;
    let artifact =
        pwr_repo::persist(&index, &PathBuf::from(&index.root).join(".pwr")).map_err(|e| {
            SafeError {
                category: "internal",
                context: e.to_string(),
            }
        })?;
    // How much had to be read. A claim that indexing is incremental is worth
    // nothing beside the number that shows it was.
    Ok(serde_json::json!({
        "index": index,
        "artifact": artifact,
        "stale": false,
        "work": index_work,
    }))
}

/// Renders a run's audit as prose a person can read without a JSON viewer.
///
/// The trail is already complete in the event log; what was missing is a shape
/// that answers "what happened in this run?" without the reader assembling it.
/// Nothing here is computed -- every line is an event that was recorded.
/// What every report built from a run's trail says about itself.
///
/// R1's exit: a replay report cannot be presented as resumed execution. A
/// report folds what was recorded; it runs nothing, re-verifies nothing, and a
/// trail that ends mid-action is a run that stopped, not one that is continuing.
/// Continuing is `pwr chat --continue` or `eval run --resume`, which record
/// that they resumed.
const REPLAY_IS_NOT_EXECUTION: &str = "A reconstruction of what this run recorded. \
     Nothing was executed or re-verified to produce it, and it is not a resumed run.";

fn report_markdown(run_id: uuid::Uuid, events: &[pwr_store::EventRecord]) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "# Run {run_id}\n");
    let _ = writeln!(out, "> {REPLAY_IS_NOT_EXECUTION}\n");
    if let (Some(first), Some(last)) = (events.first(), events.last()) {
        let _ = writeln!(
            out,
            "{} events, {} to {}.\n",
            events.len(),
            first.at.to_rfc3339(),
            last.at.to_rfc3339()
        );
    }

    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for event in events {
        *counts.entry(event.event_type.as_str()).or_default() += 1;
    }
    let _ = writeln!(out, "## What the run did\n");
    let _ = writeln!(out, "| Event | Count |");
    let _ = writeln!(out, "|---|---:|");
    for (event_type, count) in &counts {
        let _ = writeln!(out, "| `{event_type}` | {count} |");
    }

    let _ = writeln!(out, "\n## Sequence\n");
    for event in events {
        // One line per event: the fields that say what happened, and the
        // payload left out. A report that inlines every payload is the JSON
        // with extra characters.
        let detail = match event.event_type.as_str() {
            // The capability's value, not the key that holds it. Reading the
            // first key of the action object printed the literal word
            // "capability" on every line, so a sequence of eleven actions
            // named none of them.
            "tool.action" => format!(
                "{} — {}",
                event.payload["action"]["capability"]
                    .as_str()
                    .unwrap_or("action"),
                event.payload["status"].as_str().unwrap_or("?")
            ),
            // A checkpoint records the state it moved to and why. `from`/`to`
            // were never fields of it, so every transition rendered as
            // "Null → Null" -- seven lines of a twenty-seven line report
            // saying nothing.
            "task.transition" => match (
                event.payload["state"].as_str(),
                event.payload["detail"].as_str(),
            ) {
                (Some(state), Some(detail)) if !detail.is_empty() => {
                    format!("→ {state}: {detail}")
                }
                (Some(state), _) => format!("→ {state}"),
                _ => String::new(),
            },
            "verification.result" => format!(
                "verified={} verifiable={}",
                event.payload["verified"], event.payload["verifiable"]
            ),
            _ => event.payload["reason"]
                .as_str()
                .or_else(|| event.payload["step"].as_str())
                .unwrap_or("")
                .to_string(),
        };
        let _ = writeln!(
            out,
            "- `{}` {} {}",
            event.at.to_rfc3339(),
            event.event_type,
            detail
        );
    }
    let _ = writeln!(
        out,
        "\nEvery line above is an entry in the hash chain, not a summary of one. Whether that chain still holds is reported beside this document rather than asserted inside it."
    );
    out
}

fn diagnose_in(root: &Path, id: String) -> Result<serde_json::Value, SafeError> {
    let run_id = uuid::Uuid::parse_str(&id).map_err(|_| SafeError {
        category: "invalid_input",
        context: "id must be a UUID".into(),
    })?;
    let root = root.to_path_buf();
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite")).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    let events = store.events_for_run(run_id).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    if events.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: "no events found for run".into(),
        });
    }
    let exported: Vec<pwr_observe::ExportedEvent> = events
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            Some(pwr_observe::ExportedEvent {
                run_id: run_id.to_string(),
                sequence: index + 1,
                at: record.at,
                event_type: record.event_type.clone(),
                event_hash: record.event_hash.clone(),
                event: pwr_domain::RunEvent::from_stored(&record.event_type, &record.payload)?,
            })
        })
        .collect();
    let findings = pwr_observe::diagnose::diagnose(&exported);
    Ok(serde_json::json!({
        "run_id": run_id,
        "events_read": exported.len(),
        // Said rather than left to be inferred from a short list: a build that
        // cannot decode some of a run's events is diagnosing part of it.
        "events_unknown_to_this_build": events.len() - exported.len(),
        "findings": findings,
        "clean": findings.is_empty(),
    }))
}

fn report_in(root: &Path, id: String, format: String) -> Result<serde_json::Value, SafeError> {
    if !["json", "md", "jsonl"].contains(&format.as_str()) {
        return Err(SafeError {
            category: "invalid_input",
            context: "report format must be json, md or jsonl".into(),
        });
    }
    let run_id = uuid::Uuid::parse_str(&id).map_err(|_| SafeError {
        category: "invalid_input",
        context: "id must be a UUID".into(),
    })?;
    let root = root.to_path_buf();
    let store = pwr_store::Store::open(root.join(".pwr/state.sqlite")).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    let events = store.events_for_run(run_id).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    if events.is_empty() {
        return Err(SafeError {
            category: "invalid_input",
            context: "no events found for run".into(),
        });
    }
    if format == "jsonl" {
        // The trail as a stream of records rather than a document: appendable,
        // tailable, greppable, and readable by something that does not know
        // this schema.
        let exported: Vec<pwr_observe::ExportedEvent> = events
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                Some(pwr_observe::ExportedEvent {
                    run_id: run_id.to_string(),
                    sequence: index + 1,
                    at: record.at,
                    event_type: record.event_type.clone(),
                    event_hash: record.event_hash.clone(),
                    event: pwr_domain::RunEvent::from_stored(&record.event_type, &record.payload)?,
                })
            })
            .collect();
        let mut out = Vec::new();
        pwr_observe::export_jsonl(&exported, &mut out).map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?;
        let chain = store.verify_run_chain(run_id).map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?;
        return Ok(serde_json::json!({
            "run_id": run_id,
            "format": "jsonl",
            // Said plainly: a record this build does not know is left out of
            // the export rather than guessed at, and the count says how many.
            "events_exported": exported.len(),
            "events_unknown_to_this_build": events.len() - exported.len(),
            "chain": chain,
            "chain_intact": chain.intact(),
            "replay": pwr_observe::replay(&exported),
            "replay_is": REPLAY_IS_NOT_EXECUTION,
            "jsonl": String::from_utf8_lossy(&out),
        }));
    }
    // A trail nobody checks is a trail that can be edited. The API only ever
    // appended, and SQLite permits UPDATE and DELETE regardless.
    let chain = store.verify_run_chain(run_id).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    if format == "md" {
        return Ok(serde_json::json!({
            "run_id": run_id,
            "format": "md",
            "chain": chain,
            "chain_intact": chain.intact(),
            "replay_is": REPLAY_IS_NOT_EXECUTION,
            "markdown": report_markdown(run_id, &events),
        }));
    }
    Ok(serde_json::json!({
        "run_id": run_id,
        "chain": chain,
        "chain_intact": chain.intact(),
        "events": events,
    }))
}

async fn verify_in(
    root: &Path,
    run_id: Option<String>,
    scope: String,
    approvals: Vec<pwr_tools::Approval>,
) -> Result<serde_json::Value, SafeError> {
    let root = root.to_path_buf();
    let checks = pwr_verify::discover_checks(&root, &scope).map_err(|e| SafeError {
        category: "invalid_input",
        context: e,
    })?;
    let policy = pwr_tools::ToolPolicy {
        root: root.clone(),
        extra_readable: pwr_verify::declared_readable(&root)
            .into_iter()
            .chain(pwr_tools::dependency_roots(&root))
            .collect(),
        protected: frozen_paths(&root)?,
        allow_commands: pwr_verify::required_executables(&root),
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(120),
        // The workspace's mode: a check runs where the model's commands do.
        sandbox: sandbox_for(&load_chat_config(&root).unwrap_or_default()),
        // What the caller was granted: nothing from the command line, the
        // session's grants from a conversation. Measured 2026-09-26: a goal's
        // checks ran with none, so `dotnet test` failed its restore (NU1301)
        // and a smoke test its Docker socket, while the model -- allowed both
        // for the session -- had run them green; the goal was refused three
        // times and ended "blocked" on work that was done.
        approvals,
    };
    let state_dir = root.join(".pwr");
    std::fs::create_dir_all(&state_dir).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    let store = pwr_store::Store::open(state_dir.join("state.sqlite")).map_err(|e| SafeError {
        category: "internal",
        context: e.to_string(),
    })?;
    let parsed = run_id
        .as_deref()
        .map(|id| {
            uuid::Uuid::parse_str(id).map_err(|_| SafeError {
                category: "invalid_input",
                context: "run_id must be a UUID".into(),
            })
        })
        .transpose()?;
    let previous = parsed
        .and_then(|id| {
            store
                .latest_payload(id, "verification.baseline")
                .ok()
                .flatten()
        })
        .and_then(|payload| {
            serde_json::from_value::<pwr_verify::VerificationBaseline>(payload).ok()
        });
    let baseline = pwr_verify::baseline(&policy, &checks)
        .await
        .map_err(|e| SafeError {
            category: match e {
                pwr_tools::ToolError::Denied(_) => "policy_denied",
                pwr_tools::ToolError::Timeout | pwr_tools::ToolError::CommandTimedOut(_) => {
                    "verification_timeout"
                }
                pwr_tools::ToolError::Io(_) => "verification_io",
            },
            context: e.to_string(),
        })?;
    store
        .append(
            parsed,
            "verification.baseline",
            serde_json::to_value(&baseline).expect("serializable"),
        )
        .map_err(|e| SafeError {
            category: "internal",
            context: e.to_string(),
        })?;
    let check_evidence = pwr_verify::evidence::checks(&baseline);
    let passed = matches!(check_evidence, pwr_domain::ChecksOutcome::Passed);
    let comparison = previous
        .as_ref()
        .map(|prior| pwr_verify::compare(prior, &baseline));
    let verified = passed
        && comparison
            .as_ref()
            .is_none_or(|result| result.regression_free);
    Ok(
        serde_json::json!({"verified":verified,"checks_passed":passed,"check_evidence":check_evidence,"scope":scope,"workspace_root":root,"baseline":baseline,"comparison":comparison,"note":"verified requires nonempty passing checks, no zero-test result and no baseline regressions when a prior baseline exists"}),
    )
}
fn provider_error(e: pwr_provider::ProviderError) -> SafeError {
    let category = match e {
        pwr_provider::ProviderError::Unavailable { .. }
        | pwr_provider::ProviderError::Timeout { .. } => "provider_unavailable",
        pwr_provider::ProviderError::ContextLimit { .. }
        | pwr_provider::ProviderError::PromptTooLarge { .. } => "provider_context_limit",
        pwr_provider::ProviderError::Protocol { .. } => "provider_protocol",
        pwr_provider::ProviderError::Cancelled => "provider_cancelled",
        pwr_provider::ProviderError::Truncated { .. } => "provider_truncated",
        pwr_provider::ProviderError::ModelOutput { .. } => "model_output",
        pwr_provider::ProviderError::ReasoningUnfinished { .. } => "reasoning_unfinished",
        pwr_provider::ProviderError::Looping { .. } => "model_looping",
    };
    SafeError {
        category,
        context: e.to_string(),
    }
}

fn download_plan(
    artifact_id: String,
    destination_root: Option<PathBuf>,
) -> Result<serde_json::Value, SafeError> {
    let plan = artifact_download_plan(artifact_id, destination_root)?;
    let verified_files = plan.files.iter().all(|file| {
        file.expected_bytes.is_some() && (file.blake3.is_some() || file.sha256.is_some())
    });
    Ok(serde_json::json!({
        "plan": plan,
        "executes_network": false,
        "verification": {
            "revision": "pinned",
            "files": if verified_files { "declared" } else { "incomplete" }
        }
    }))
}

/// The local, untrusted-on-its-own state of every declared downloadable
/// artifact. A final file is only "present" here, never "verified": computing
/// its hash every time an app refreshes would make the status surface itself
/// expensive. `models download` remains the verifying transition.
fn artifact_download_status() -> Result<Vec<serde_json::Value>, SafeError> {
    let registry = load_artifact_registry(Path::new(ARTIFACT_FILE))?;
    registry
        .artifacts
        .iter()
        .filter_map(|artifact| match &artifact.source {
            pwr_domain::ArtifactSource::HuggingFace { .. } => Some(artifact.id.clone()),
            _ => None,
        })
        .map(|artifact_id| {
            let plan = artifact_download_plan(artifact_id.clone(), None)?;
            let files = plan
                .files
                .iter()
                .map(|file| {
                    let destination = PathBuf::from(&file.destination);
                    let part = pwr_models::download::part_path(&destination);
                    let destination_bytes =
                        destination.metadata().ok().map(|metadata| metadata.len());
                    let part_bytes = part.metadata().ok().map(|metadata| metadata.len());
                    let state = if destination_bytes.is_some() {
                        "present_unverified"
                    } else if part_bytes.is_some() {
                        "partial"
                    } else {
                        "missing"
                    };
                    serde_json::json!({
                        "file": file.file,
                        "destination": file.destination,
                        "expectedBytes": file.expected_bytes,
                        "state": state,
                        "bytesOnDisk": destination_bytes.or(part_bytes).unwrap_or(0),
                    })
                })
                .collect::<Vec<_>>();
            Ok(serde_json::json!({
                "artifactId": artifact_id,
                "repository": plan.repository,
                "revision": plan.revision,
                "destinationRoot": plan.destination_root,
                "files": files,
            }))
        })
        .collect()
}

async fn download_artifact(
    artifact_id: String,
    destination_root: Option<PathBuf>,
    progress: &mut dyn FnMut(&pwr_models::download::Progress),
    stop: &AtomicBool,
) -> Result<serde_json::Value, pwr_models::download::DownloadError> {
    use pwr_models::download::{DownloadError, FailureKind};
    let plan =
        artifact_download_plan(artifact_id, destination_root).map_err(|error| DownloadError {
            kind: FailureKind::Unverifiable,
            message: error.context,
        })?;
    for file in &plan.files {
        if file.expected_bytes.is_none() || (file.blake3.is_none() && file.sha256.is_none()) {
            return Err(DownloadError {
                kind: FailureKind::Unverifiable,
                message: format!(
                    "{} cannot be downloaded safely: {} lacks bytes or a hash in {}",
                    plan.artifact_id, file.file, ARTIFACT_FILE
                ),
            });
        }
    }
    // The registry's plan, handed to the downloader the app's Model Manager
    // uses too: one implementation of verification, resume and refusal.
    let shared = pwr_models::download::Plan {
        id: plan.artifact_id.clone(),
        destination_root: PathBuf::from(&plan.destination_root),
        files: plan
            .files
            .iter()
            .map(|file| pwr_models::download::PlannedFile {
                file: file.file.clone(),
                url: file.url.clone(),
                destination: PathBuf::from(&file.destination),
                expected_bytes: file.expected_bytes.unwrap_or_default(),
                blake3: file.blake3.clone(),
                sha256: file.sha256.clone(),
                git_sha1: None,
            })
            .collect(),
        revision: None,
    };
    let preflight = download_preflight(&shared).await?;
    let client = reqwest::Client::new();
    let token = std::env::var("HF_TOKEN").ok();
    let outcomes =
        pwr_models::download::download(&client, &shared, token.as_deref(), progress, stop).await?;
    Ok(serde_json::json!({
        "artifact_id": plan.artifact_id,
        "repository": plan.repository,
        "revision": plan.revision,
        "destination_root": plan.destination_root,
        "disk_preflight": preflight,
        "files": outcomes,
    }))
}

/// The disk check before a download, with the free space where it will land.
async fn download_preflight(
    plan: &pwr_models::download::Plan,
) -> Result<pwr_models::download::Preflight, pwr_models::download::DownloadError> {
    let plan = plan.clone();
    tokio::task::spawn_blocking(move || {
        let available = pwr_models::download::available_disk_bytes(&plan.destination_root)?;
        pwr_models::download::preflight(&plan, available)
    })
    .await
    .map_err(|error| pwr_models::download::DownloadError {
        kind: pwr_models::download::FailureKind::Io,
        message: format!("download disk check stopped unexpectedly: {error}"),
    })?
}

fn download_error(error: pwr_models::download::DownloadError) -> SafeError {
    SafeError {
        category: error.category(),
        context: error.message,
    }
}

fn artifact_download_plan(
    artifact_id: String,
    destination_root: Option<PathBuf>,
) -> Result<pwr_domain::ArtifactDownloadPlan, SafeError> {
    let registry = load_artifact_registry(Path::new(ARTIFACT_FILE))?;
    let artifact = registry
        .artifacts
        .iter()
        .find(|artifact| artifact.id == artifact_id)
        .ok_or_else(|| SafeError {
            category: "invalid_input",
            context: format!("{artifact_id} is not in {ARTIFACT_FILE}"),
        })?;
    let pwr_domain::ArtifactSource::HuggingFace { .. } = artifact.source else {
        return Err(SafeError {
            category: "invalid_input",
            context: format!(
                "{} is {:?}, not a HuggingFace artifact",
                artifact.id, artifact.source
            ),
        });
    };
    let destination_root = destination_root.unwrap_or_else(default_artifact_destination_root);
    let hub_base =
        std::env::var("PWR_HF_BASE_URL").unwrap_or_else(|_| "https://huggingface.co".into());
    artifact
        .download_plan_from_base(&destination_root, &hub_base)
        .map_err(|error| SafeError {
            category: "invalid_input",
            context: error.to_string(),
        })?
        .ok_or_else(|| SafeError {
            category: "invalid_input",
            context: format!("{} is not a HuggingFace artifact", artifact.id),
        })
}

fn default_artifact_destination_root() -> PathBuf {
    std::env::var_os("PWR_MODEL_ARTIFACTS")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".pwr/artifacts")))
        .unwrap_or_else(|| PathBuf::from(".pwr/artifacts"))
}

/// The host's hardware facts, read by the shared cross-platform profiler.
///
/// This used to be a macOS-only probe living in the CLI, which meant that on
/// any other host every fact was `unknown` and admission had nothing to work
/// with. It now delegates to `pwr-runtime`, which the orchestrator and any
/// future frontend can call without going through the command line.
async fn probe_hardware() -> HardwareProfile {
    pwr_runtime::hardware::probe_hardware().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwr_domain::ModelChunk;
    use pwr_domain::now;
    use pwr_provider::ProviderError;

    #[test]
    fn a_long_failing_output_keeps_its_beginning_and_its_end_and_drops_stack_frames() {
        let short = failing_output(
            "✖ a weekend (1ms)\n\n    at Test.run (node:internal)\n",
            "  File \"x.py\", line 3\nAssertionError: 1 != 0\n",
        );
        assert_eq!(short, "✖ a weekend (1ms)\nAssertionError: 1 != 0\n");
        let long: String = (0..2_000).map(|n| format!("line {n} è\n")).collect();
        let shown = failing_output(&long, "");
        assert!(shown.starts_with("line 0 è\n") && shown.ends_with("line 1999 è\n"));
        assert!(shown.contains("[...]") && shown.len() < 12_100);
    }

    // ------------------------------------------ what the checks establish

    fn check(command: &str, exit_code: Option<i32>) -> pwr_verify::CheckRecord {
        pwr_verify::CheckRecord {
            command: command.to_owned(),
            result: pwr_tools::ToolResult {
                exit_code,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 1,
                redacted: false,
                artifact_hash: "hash".into(),
                stdout_truncated: false,
                stderr_truncated: false,
                sandboxed: true,
                failing_files: None,
                ran_in: None,
            },
        }
    }

    fn baseline(checks: Vec<pwr_verify::CheckRecord>) -> pwr_verify::VerificationBaseline {
        pwr_verify::VerificationBaseline {
            id: new_id(),
            captured_at: now(),
            checks,
            environment_hash: "env".into(),
        }
    }

    /// The conversation said "the repository's own checks passed after the
    /// change" whenever no check that passed before was failing now. A
    /// repository whose suite was already red therefore got told its checks
    /// passed, by the harness that exists to refuse that claim.
    #[test]
    fn a_check_that_was_already_failing_is_not_a_check_that_passed() {
        let before = baseline(vec![
            check("cargo test", Some(101)),
            check("cargo fmt", Some(0)),
        ]);
        let after = baseline(vec![
            check("cargo test", Some(101)),
            check("cargo fmt", Some(0)),
        ]);

        // The condition the old message turned on is still true, and on its own
        // it establishes only that nothing was broken.
        assert!(pwr_verify::compare(&before, &after).new_failures.is_empty());

        let said = check_verdict(&before, &after).said();
        assert!(!said.contains("checks passed"), "{said}");
        assert!(said.contains("cargo test"), "{said}");
        assert!(said.contains("not verified"), "{said}");
    }

    /// The claim is still available, and available when it is true. A fix that
    /// made the honest sentence unreachable would be its own defect.
    #[test]
    fn every_check_green_is_said_plainly() {
        let before = baseline(vec![check("cargo test", Some(101))]);
        let after = baseline(vec![check("cargo test", Some(0))]);
        assert_eq!(
            check_verdict(&before, &after).said(),
            "the repository's own checks passed after the change"
        );
    }

    /// A regression is named, and named before the baseline is described: a
    /// reader who broke something needs that first.
    #[test]
    fn a_new_failure_is_named() {
        let before = baseline(vec![
            check("cargo test", Some(0)),
            check("cargo clippy", Some(1)),
        ]);
        let after = baseline(vec![
            check("cargo test", Some(1)),
            check("cargo clippy", Some(1)),
        ]);
        let said = check_verdict(&before, &after).said();
        assert!(
            said.starts_with("the repository's own checks did not pass"),
            "{said}"
        );
        assert!(said.contains("cargo test"), "{said}");
    }

    // ------------------------------------------- the evaluator's two modes

    /// `/changes` always names the files, adds git's diff where there is one,
    /// and says why there is none where there is not.
    #[test]
    fn the_changes_view_names_the_files_and_shows_what_git_can() {
        assert_eq!(
            changes_summary(&[], None),
            "This conversation has not changed any file yet."
        );
        let paths = vec!["src/a.rs".to_string()];
        let with_diff = changes_summary(
            &paths,
            Some("diff --git a/src/a.rs b/src/a.rs\n-one\n+two\n"),
        );
        assert!(
            with_diff.contains("src/a.rs") && with_diff.contains("+two"),
            "{with_diff}"
        );
        assert!(changes_summary(&paths, Some("  ")).contains("not yet added to git"));
        assert!(changes_summary(&paths, None).contains("not a git repository"));
        let long = "x".repeat(CHANGES_SHOWN_CHARS + 50);
        let cut = changes_summary(&paths, Some(&long));
        assert!(
            cut.contains("50 more characters"),
            "{}",
            &cut[cut.len() - 80..]
        );
    }

    /// `--continue` restores the last complete turn, tells the deployment what
    /// changed while the conversation was away, and records that it did.
    #[test]
    fn continuing_restores_the_conversation_and_says_what_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".pwr")).unwrap();
        std::fs::write(dir.path().join("a.rs"), "someone else's version\n").unwrap();
        let id = new_id();
        {
            let store = pwr_store::Store::open(dir.path().join(".pwr/state.sqlite")).unwrap();
            pwr_orchestrator::conversation::record_snapshot(
                &store,
                id,
                &[ChatMessage::text("user", "fix a.rs")],
            )
            .unwrap();
            let mut changed = BTreeMap::new();
            changed.insert(
                "a.rs".to_string(),
                hash_bytes("the conversation's version\n"),
            );
            pwr_orchestrator::conversation::record_checkpoint(
                &store,
                id,
                &pwr_orchestrator::conversation::Checkpoint {
                    turn: 1,
                    changed_files: changed,
                    ..Default::default()
                },
            )
            .unwrap();
        }
        let resumed = resume_latest_conversation(dir.path())
            .unwrap()
            .expect("a conversation to continue");
        assert_eq!(resumed.conversation_id, id);
        assert_eq!(resumed.checkpoint.turn, 1);
        let note = resumed.note.expect("a change to report");
        assert!(note.contains("a.rs"), "{note}");
        assert_eq!(
            resumed.messages.len(),
            2,
            "the note did not reach the deployment"
        );
        assert_eq!(resumed.messages[1].content, note);
        let store = pwr_store::Store::open(dir.path().join(".pwr/state.sqlite")).unwrap();
        assert!(
            store
                .events_for_run(id)
                .unwrap()
                .iter()
                .any(|event| event.event_type == "conversation.resumed")
        );
    }

    #[test]
    fn there_is_nothing_to_continue_in_a_workspace_without_a_conversation() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".pwr")).unwrap();
        assert!(resume_latest_conversation(dir.path()).unwrap().is_none());
    }

    // ------------------------------------------ what the console says back

    /// A console that answers a one-line question with a page of JSON has moved
    /// the work rather than done it.
    #[test]
    fn the_doctor_summary_answers_the_question_that_was_asked() {
        let down = serde_json::json!({
            "backend": "mlx",
            "backend_runtime": {"status": "unavailable", "reason": "connection refused"},
        });
        let said = summarise_doctor(&down);
        assert!(said.contains("not answering"), "{said}");
        assert!(said.contains("connection refused"), "{said}");

        let up = serde_json::json!({
            "backend": "mlx",
            "backend_runtime": {"loaded_models": ["a", "b"], "state": {}},
        });
        let said = summarise_doctor(&up);
        assert!(said.contains("mlx"), "{said}");
        assert!(said.contains("2 model"), "{said}");

        let metadata_only = serde_json::json!({
            "backend": "llama",
            "backend_runtime": {"loaded_models": [], "state": {"status": "metadata_only"}},
        });
        let said = summarise_doctor(&metadata_only);
        assert!(said.contains("model metadata"), "{said}");
        assert!(said.contains("generation is not wired"), "{said}");

        let server_on_demand = serde_json::json!({
            "backend": "llama",
            "backend_runtime": {"loaded_models": [], "state": {"status": "server_on_demand"}},
        });
        let said = summarise_doctor(&server_on_demand);
        assert!(said.contains("server on demand"), "{said}");
    }

    /// Finding nothing is a result, and saying so plainly matters more than the
    /// finding does: the person typing `/diagnose` already suspects a loop.
    #[test]
    fn a_clean_diagnosis_says_what_it_does_and_does_not_establish() {
        let clean = serde_json::json!({"events_read": 41, "findings": [], "clean": true});
        let said = summarise_diagnosis(&clean);
        assert!(said.contains("41 events"), "{said}");
        assert!(said.contains("not proof nothing is wrong"), "{said}");
    }

    /// The detectors carry a sentence written to be read without the schema.
    /// Using it rather than reformatting `detail` is the difference between a
    /// console and a JSON viewer.
    #[test]
    fn a_diagnosis_uses_the_sentence_the_detector_wrote() {
        let found = serde_json::json!({
            "events_read": 12,
            "findings": [{
                "detector": "repeated_refusal",
                "count": 3,
                "says": "the same edit was refused three times",
                "detail": {"action": "replace_text:code.rs"},
            }],
        });
        let said = summarise_diagnosis(&found);
        assert!(said.contains("repeated_refusal"), "{said}");
        assert!(said.contains("×3"), "{said}");
        assert!(
            said.contains("the same edit was refused three times"),
            "{said}"
        );
        // The structured detail belongs to the command line, not to a
        // conversation: a console that pastes JSON into the transcript has
        // handed the reader the schema to interpret.
        assert!(!said.contains("replace_text:code.rs"), "{said}");
    }

    /// "The checks failed" sends an operator looking for a list the console was
    /// holding. Naming them costs four lines and saves the question.
    #[test]
    fn the_verification_summary_names_the_checks() {
        let report = serde_json::json!({
            "scope": "targeted",
            "baseline": {"checks": [
                {"command": "cargo fmt", "result": {"exit_code": 0}},
                {"command": "cargo test", "result": {"exit_code": 101}},
            ]},
        });
        let said = summarise_verification(&report);
        assert!(said.contains("1 of 2"), "{said}");
        assert!(said.contains("cargo test"), "{said}");
        assert!(said.contains("cargo fmt"), "{said}");
    }

    /// A workspace with no checks is not a workspace that failed them, and the
    /// sentence has to be the one the turn's own verdict already uses.
    #[test]
    fn a_workspace_with_no_checks_is_told_apart_from_a_failing_one() {
        let said = summarise_verification(&serde_json::json!({"baseline": {"checks": []}}));
        assert!(said.contains("declares no checks"), "{said}");
        assert!(!said.contains("passing"), "{said}");
    }

    #[test]
    fn the_minimal_control_is_given_a_neutral_instruction_and_its_models_suffix() {
        let root = tempfile::tempdir().unwrap();
        let mut profile = pwr_orchestrator::TaskProfile::resolve(None, None);
        profile.prompt_suffix = "Write tool calls in the model's own format.".into();
        let mut messages = vec![
            ChatMessage::text("system", "PWR's instructions and repository rules"),
            ChatMessage::text("user", "Do it."),
        ];
        compose_minimal_turn(root.path(), 8192, &profile, &mut messages).unwrap();
        assert!(messages[0].content.starts_with("You are a coding agent"));
        assert!(
            messages[0]
                .content
                .contains("Write tool calls in the model's own format.")
        );
        assert!(!messages[0].content.contains("PWR"));
        assert_eq!(messages.len(), 2, "something was retrieved for the control");
        assert_eq!(messages[1].content, "Do it.");
    }

    #[tokio::test]
    async fn verification_without_checks_cannot_claim_a_pass() {
        let root = tempfile::tempdir().unwrap();
        let report = verify_in(root.path(), None, "full".into(), vec![])
            .await
            .unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(report["baseline"]["checks"], serde_json::json!([]));
        assert_eq!(report["checks_passed"], false, "{report}");
        assert_eq!(report["verified"], false, "{report}");
    }

    #[tokio::test]
    async fn verification_that_ran_zero_tests_cannot_claim_a_pass() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        std::fs::write(root.path().join(".pwr/checks.json"), serde_json::json!({"checks":[{"executable":"sh","args":["-c","echo 'no tests ran' # pytest"]}]}).to_string()).unwrap();
        let report = verify_in(root.path(), None, "full".into(), vec![])
            .await
            .unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(report["baseline"]["checks"][0]["result"]["exit_code"], 0);
        assert_eq!(report["checks_passed"], false, "{report}");
        assert_eq!(report["verified"], false, "{report}");
    }

    /// A line of zeroes every time teaches the reader to skip the line that
    /// matters, so the counts that did not happen are not printed.
    #[test]
    fn a_session_summary_reports_only_what_happened() {
        let quiet = serde_json::json!({"replay": {
            "turns": 3, "actions_allowed": 7, "actions_denied": 0, "actions_failed": 0,
            "elapsed_secs": 42.0, "generation_secs": 30.0,
            "prompt_tokens": 900, "generated_tokens": 300,
            "loops_named": 0, "no_progress_named": 0, "compactions": 0, "context_downgrades": 0,
        }});
        let said = summarise_session(&quiet);
        assert!(said.contains("3 turn(s)"), "{said}");
        assert!(said.contains("42s elapsed"), "{said}");
        assert!(!said.contains("compaction"), "{said}");

        let stuck = serde_json::json!({"replay": {
            "turns": 9, "actions_allowed": 20, "actions_denied": 4, "actions_failed": 1,
            "elapsed_secs": 300.0, "generation_secs": 240.0,
            "prompt_tokens": 9000, "generated_tokens": 3000,
            "loops_named": 2, "no_progress_named": 1, "compactions": 3, "context_downgrades": 1,
        }});
        let said = summarise_session(&stuck);
        assert!(said.contains("2 repetition(s) named"), "{said}");
        assert!(said.contains("1 window(s) of no progress named"), "{said}");
        assert!(said.contains("1 drop(s) to a lower window"), "{said}");
    }

    /// A report says what it is, so a reconstruction cannot be read as a run
    /// that carried on.
    #[test]
    fn a_report_says_it_is_a_reconstruction_not_an_execution() {
        let markdown = report_markdown(uuid::Uuid::nil(), &[]);
        assert!(markdown.contains(REPLAY_IS_NOT_EXECUTION), "{markdown}");
        assert!(REPLAY_IS_NOT_EXECUTION.contains("not a resumed run"));
    }

    #[tokio::test]
    async fn a_zero_test_acceptance_check_cannot_be_exempted_as_a_baseline_failure() {
        use serve::TurnRunner;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        std::fs::write(root.path().join(".pwr/checks.json"), serde_json::json!({"checks":[{"executable":"sh","args":["-c","echo 'no tests ran' # pytest"],"kind":"acceptance"}]}).to_string()).unwrap();
        let snapshot = pwr_verify::acceptance::snapshot(root.path())
            .unwrap()
            .unwrap();
        let context = serve::CommandContext {
            root: root.path().to_owned(),
            conversation_id: pwr_domain::new_id(),
            acceptance_contract_hash: Some(snapshot.digest()),
            acceptance_snapshot: Some(snapshot),
            authorized_acceptance_changes: Default::default(),
            changed_files: Default::default(),
            session_grants: vec![],
        };
        let runner = ConsoleTurns {
            runtime: RuntimeFactory::local(BackendKind::Mlx),
        };
        let verdict = runner.verify_goal(context).await.unwrap();
        assert!(verdict.acceptance_available);
        assert!(!verdict.passed);
        assert_eq!(verdict.failing.len(), 1);
        assert_eq!(
            verdict.failing_acceptance, verdict.failing,
            "zero-test acceptance was eligible for baseline exemption"
        );
    }

    #[tokio::test]
    async fn modifying_acceptance_artifacts_cannot_certify_a_goal_even_with_full_access() {
        use serve::TurnRunner;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        std::fs::create_dir(root.path().join("tests")).unwrap();
        std::fs::write(root.path().join(".pwr/checks.json"), r#"{"checks":[{"executable":"cargo","args":["test"],"kind":"acceptance"}],"acceptance":{"artifacts":["tests/*.rs"]}}"#).unwrap();
        let evidence = root.path().join("tests/acceptance.rs");
        std::fs::write(&evidence, "assert!(false)").unwrap();
        let snapshot = pwr_verify::acceptance::snapshot(root.path())
            .unwrap()
            .unwrap();
        let context = serve::CommandContext {
            root: root.path().to_owned(),
            conversation_id: pwr_domain::new_id(),
            acceptance_contract_hash: Some(snapshot.digest()),
            acceptance_snapshot: Some(snapshot),
            authorized_acceptance_changes: Default::default(),
            changed_files: Default::default(),
            session_grants: vec![pwr_tools::Approval::OutsideSandbox],
        };
        assert!(
            frozen_paths(root.path())
                .map_err(|error| error.context)
                .unwrap()
                .contains(&PathBuf::from("tests/acceptance.rs"))
        );
        std::fs::write(evidence, "assert!(true)").unwrap();
        let runner = ConsoleTurns {
            runtime: RuntimeFactory::local(BackendKind::Mlx),
        };
        let verdict = runner.verify_goal(context).await.unwrap();
        assert!(!verdict.passed);
        assert_eq!(verdict.contract_changed, ["tests/acceptance.rs"]);
    }

    #[tokio::test]
    async fn a_check_that_rewrites_its_own_evidence_cannot_certify_a_goal() {
        use serve::TurnRunner;
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join(".pwr")).unwrap();
        std::fs::create_dir(root.path().join("tests")).unwrap();
        std::fs::write(root.path().join("tests/evidence.txt"), "original").unwrap();
        let declaration = serde_json::json!({
            "checks": [{"executable":"/usr/bin/python3", "args":["-c",
                "from pathlib import Path; Path('tests/evidence.txt').write_text('weakened')"], "kind":"acceptance"}],
            "acceptance":{"artifacts":["tests/evidence.txt"]}
        });
        std::fs::write(
            root.path().join(".pwr/checks.json"),
            declaration.to_string(),
        )
        .unwrap();
        let config = ChatConfig {
            permission_mode: Some(PermissionMode::Full),
            ..Default::default()
        };
        save_chat_config(root.path(), &config)
            .map_err(|error| error.context)
            .unwrap();
        let snapshot = pwr_verify::acceptance::snapshot(root.path())
            .unwrap()
            .unwrap();
        let context = serve::CommandContext {
            root: root.path().to_owned(),
            conversation_id: pwr_domain::new_id(),
            acceptance_contract_hash: Some(snapshot.digest()),
            acceptance_snapshot: Some(snapshot),
            authorized_acceptance_changes: Default::default(),
            changed_files: Default::default(),
            session_grants: vec![pwr_tools::Approval::OutsideSandbox],
        };
        let runner = ConsoleTurns {
            runtime: RuntimeFactory::local(BackendKind::Mlx),
        };
        let verdict = runner.verify_goal(context).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(root.path().join("tests/evidence.txt")).unwrap(),
            "weakened"
        );
        assert!(!verdict.passed);
        assert_eq!(verdict.contract_changed, ["tests/evidence.txt"]);
    }

    #[test]
    fn verification_marks_follow_the_typed_verdict() {
        assert_eq!(CheckVerdict::Green.mark(), "✓");
        assert_eq!(
            CheckVerdict::BaselinePreserved {
                still_failing: vec!["cargo test".into()]
            }
            .mark(),
            "–"
        );
        assert_eq!(
            CheckVerdict::NewFailures(vec!["cargo test".into()]).mark(),
            "✗"
        );
    }

    /// A goal's checks restore packages and start containers with what the
    /// person allowed for the session, and with nothing more.
    #[test]
    fn a_conversations_checks_run_with_the_sessions_grants() {
        use pwr_tools::Approval;
        let root = tempfile::tempdir().unwrap();
        let context = |grants: Vec<Approval>| serve::CommandContext {
            root: root.path().to_path_buf(),
            conversation_id: pwr_domain::new_id(),
            acceptance_contract_hash: None,
            acceptance_snapshot: None,
            authorized_acceptance_changes: Default::default(),
            changed_files: Default::default(),
            session_grants: grants,
        };
        let none = session_approvals(root.path(), &context(Vec::new()));
        assert!(!none.contains(&Approval::NetworkAccess), "{none:?}");
        assert!(!none.contains(&Approval::ContainerEngine), "{none:?}");
        let allowed = session_approvals(
            root.path(),
            &context(vec![Approval::NetworkAccess, Approval::ContainerEngine]),
        );
        assert!(allowed.contains(&Approval::NetworkAccess), "{allowed:?}");
        assert!(allowed.contains(&Approval::ContainerEngine), "{allowed:?}");
        // And nothing Settings withhold that the session did not grant.
        assert!(!allowed.contains(&Approval::Publish), "{allowed:?}");
    }

    /// Settings decide what the conversation asks about; everything else is
    /// granted, and "for this session" lifts a question for later turns.
    #[test]
    fn chat_approvals_follow_settings_and_session_grants() {
        use pwr_tools::Approval;
        let default = ChatConfig::default();
        assert_eq!(default.ask_before, asked_before_by_default());
        let granted = chat_approvals(&effective_ask_before(&default), &[]);
        // Ask mode, the default, asks before what reaches outside the
        // workspace or cannot be taken back (decided 2026-09-23).
        for asked in [
            Approval::Publish,
            Approval::HistoryRewrite,
            Approval::DependencyChange,
            Approval::NetworkAccess,
            Approval::ToolchainInstall,
            Approval::ContainerEngine,
        ] {
            assert!(
                !granted.contains(&asked),
                "{asked:?} granted without asking"
            );
        }
        assert!(granted.contains(&Approval::LocalService));

        // Standard grants all but what cannot be taken back or leaves the
        // sandbox; Full grants everything, and lifts the sandbox.
        let standard = ChatConfig {
            permission_mode: Some(PermissionMode::Auto),
            ..ChatConfig::default()
        };
        let granted = chat_approvals(&effective_ask_before(&standard), &[]);
        for asked in STANDARD_ASKED {
            assert!(!granted.contains(&asked), "{asked:?}");
        }
        assert!(granted.contains(&Approval::NetworkAccess));
        assert!(granted.contains(&Approval::ToolchainInstall));
        assert_eq!(sandbox_for(&standard), pwr_tools::SandboxPolicy::Preferred);
        let full = ChatConfig {
            permission_mode: Some(PermissionMode::Full),
            ..ChatConfig::default()
        };
        assert_eq!(chat_approvals(&effective_ask_before(&full), &[]), {
            let mut all = all_approvals();
            all.sort();
            all
        });
        assert_eq!(sandbox_for(&full), pwr_tools::SandboxPolicy::FullAccess);
        // Protected asks before leaving the sandbox whatever a saved list says.
        let protected = ChatConfig {
            ask_before: vec![Approval::Publish],
            ..ChatConfig::default()
        };
        assert!(effective_ask_before(&protected).contains(&Approval::OutsideSandbox));

        let after_session_grant = chat_approvals(&default.ask_before, &[Approval::Publish]);
        assert!(after_session_grant.contains(&Approval::Publish));
        assert!(!after_session_grant.contains(&Approval::HistoryRewrite));

        assert_eq!(chat_approvals(&[], &[]), {
            let mut all = all_approvals();
            all.sort();
            all
        });
        // A config written before the setting existed asks about the defaults.
        let old: ChatConfig = serde_json::from_value(serde_json::json!({
            "model": null, "profile": null, "prepared_for_model": null,
            "context_tokens": 8192, "timeout_secs": 60, "plan": false
        }))
        .unwrap();
        assert_eq!(old.ask_before, asked_before_by_default());
    }

    /// A configuration saved before the permission modes, holding the old
    /// default nobody chose, is read as Ask with the new default; one holding
    /// a list someone did choose keeps it.
    #[test]
    fn configurations_from_before_the_modes_are_migrated_to_ask() {
        use pwr_tools::Approval;
        let workspace = tempfile::tempdir().unwrap();
        let path = chat_config_path(workspace.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut saved = serde_json::to_value(ChatConfig::default()).unwrap();
        saved.as_object_mut().unwrap().remove("permission_mode");
        saved["ask_before"] = serde_json::json!(["history_rewrite", "publish"]);
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let read =
            load_chat_config(workspace.path()).unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(read.permission_mode, Some(PermissionMode::Ask));
        assert!(read.ask_before.contains(&Approval::DependencyChange));

        saved["ask_before"] = serde_json::json!(["publish"]);
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let chosen =
            load_chat_config(workspace.path()).unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(chosen.ask_before, vec![Approval::Publish]);
    }

    /// A question put to the console reaches it, and its answer reaches the
    /// turn; "for this session" is remembered, and a console that is gone
    /// refuses.
    #[test]
    fn a_console_approval_crosses_to_the_console_and_back() {
        use pwr_orchestrator::{ApprovalDecision, ApprovalPrompt};
        use pwr_tools::Approval;
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<ApprovalRequest>();
        let prompt = ConsoleApproval {
            requests: sender,
            session_grants: Arc::default(),
        };
        let decision = runtime.block_on(async {
            let console = async {
                let request = receiver.recv().await.unwrap();
                assert!(request.description.contains("git push"));
                request.reply.send(ApprovalDecision::AllowForRun).unwrap();
            };
            let (decision, ()) = tokio::join!(
                prompt.ask(Approval::Publish, "run `git push origin main`"),
                console
            );
            decision
        });
        assert_eq!(decision, ApprovalDecision::AllowForRun);
        assert_eq!(
            *prompt.session_grants.lock().unwrap(),
            vec![Approval::Publish]
        );

        drop(receiver);
        let refused = runtime.block_on(prompt.ask(Approval::HistoryRewrite, "rebase"));
        assert_eq!(refused, ApprovalDecision::Deny);

        assert_eq!(
            approval_key(KeyCode::Char('y')),
            Some(ApprovalDecision::AllowOnce)
        );
        assert_eq!(
            approval_key(KeyCode::Char('a')),
            Some(ApprovalDecision::AllowForRun)
        );
        assert_eq!(approval_key(KeyCode::Esc), Some(ApprovalDecision::Deny));
        assert_eq!(approval_key(KeyCode::Char('x')), None);
    }

    /// Session navigation is an admitted conversation capability now, so the
    /// console says enough to choose one without falling back to JSON.
    #[test]
    fn a_sessions_summary_names_saved_sessions() {
        let empty = serde_json::json!({"sessions": []});
        let said = summarise_sessions_list(&empty);
        assert!(said.contains("no saved sessions"), "{said}");

        let list = serde_json::json!({"sessions": [
            {"name": "refactor", "runs": 2, "last_task": "rename the helper"},
            {"name": "plain", "runs": 1, "last_task": null}
        ]});
        let said = summarise_sessions_list(&list);
        assert!(said.contains("2 saved session"), "{said}");
        assert!(said.contains("refactor"), "{said}");
        assert!(said.contains("rename the helper"), "{said}");
        assert!(said.contains("plain"), "{said}");
        assert!(!said.contains("\"sessions\""), "{said}");
    }

    /// Showing one session is a conversation answer: branch drift,
    /// interruption and ledger presence are the facts a person acts on.
    #[test]
    fn a_named_session_summary_reports_branch_drift_and_ledger() {
        let shown = serde_json::json!({
            "name": "refactor",
            "runs": ["r1", "r2"],
            "opened_on": {"branch": "main"},
            "workspace_now": {"branch": "experiment"},
            "interrupted_run": {"state": "Act", "actions_spent": 4},
            "ledger": "Session ledger\n- changed code.py"
        });
        let said = summarise_named_session(&shown);
        assert!(said.contains("session refactor"), "{said}");
        assert!(said.contains("2 run"), "{said}");
        assert!(said.contains("main"), "{said}");
        assert!(said.contains("experiment"), "{said}");
        assert!(said.contains("interrupted"), "{said}");
        assert!(said.contains("Session ledger"), "{said}");
        assert!(!said.contains("\"ledger\""), "{said}");
    }

    /// A command that produced no exit code did not run to a verdict, and an
    /// absent verdict is not a passing one.
    ///
    /// A guard rather than evidence for this change: `pwr_verify::compare`
    /// already classifies an absent exit code as a new failure, because
    /// `same_failure` requires the previous run to have had one. So this
    /// fixture passes on the behaviour being replaced too. It is here because
    /// the two pieces of reasoning are in different crates and the property
    /// belongs to neither alone.
    #[test]
    fn a_check_with_no_exit_code_does_not_count_as_passing() {
        let before = baseline(vec![check("cargo test", None)]);
        let after = baseline(vec![check("cargo test", None)]);
        let said = check_verdict(&before, &after).said();
        assert!(!said.contains("checks passed"), "{said}");
    }

    /// One failing check reads as one, because a sentence in the plural over a
    /// single command is the kind of detail that makes a reader doubt the rest.
    #[test]
    fn the_count_agrees_with_itself() {
        let one = baseline(vec![check("cargo test", Some(1))]);
        let said = check_verdict(&one, &one).said();
        assert!(said.contains("1 still does"), "{said}");
        let two = baseline(vec![check("a", Some(1)), check("b", Some(1))]);
        let said = check_verdict(&two, &two).said();
        assert!(said.contains("2 still do"), "{said}");
    }

    fn thinking(text: &str) -> ModelChunk {
        ModelChunk {
            thinking: Some(text.into()),
            ..Default::default()
        }
    }
    fn call(name: &str) -> ModelChunk {
        ModelChunk {
            tool_calls: vec![ToolCall {
                name: name.into(),
                arguments: serde_json::json!({"value": "ok"}),
                id: None,
            }],
            ..Default::default()
        }
    }
    fn stream(
        chunks: Vec<Result<ModelChunk, ProviderError>>,
    ) -> Result<pwr_provider::ModelStream, ProviderError> {
        Ok(Box::pin(futures_util::stream::iter(chunks)))
    }

    /// The defect this probe had: a reasoning deployment opens with thinking
    /// chunks and emits its tool call near the end, so reading only the first
    /// chunk reported "no native tool call" for a model that made one.
    #[tokio::test]
    async fn tool_call_after_leading_thinking_chunks_is_observed() {
        let observed = drain_probe(
            stream(vec![
                Ok(thinking("The")),
                Ok(thinking(" user")),
                Ok(thinking(" wants")),
                Ok(call(PROBE_TOOL)),
                Ok(ModelChunk {
                    done: true,
                    ..Default::default()
                }),
            ]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap();
        assert_eq!(observed.matched_chunk_index, Some(4));
        assert_eq!(observed.matched_tool_call.unwrap().arguments["value"], "ok");
    }

    #[tokio::test]
    async fn prose_only_stream_does_not_claim_tool_support() {
        let observed = drain_probe(
            stream(vec![
                Ok(ModelChunk {
                    content: "I cannot".into(),
                    ..Default::default()
                }),
                Ok(ModelChunk {
                    content: " do that".into(),
                    done: true,
                    ..Default::default()
                }),
            ]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap();
        assert!(observed.matched_tool_call.is_none());
        assert!(observed.tool_call_names.is_empty());
        assert!(observed.produced_text);
    }

    /// A JSON array in prose is not a native call. The typed channel is the
    /// only evidence; content is never re-parsed to infer one.
    #[tokio::test]
    async fn json_shaped_prose_is_not_mistaken_for_a_tool_call() {
        let observed = drain_probe(
            stream(vec![Ok(ModelChunk {
                content: r#"[{"name":"probe_echo"}]"#.into(),
                done: true,
                ..Default::default()
            })]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap();
        assert!(observed.matched_tool_call.is_none());
    }

    #[tokio::test]
    async fn call_to_an_unoffered_tool_is_not_credited() {
        let observed = drain_probe(
            stream(vec![
                Ok(call("some_other_tool")),
                Ok(ModelChunk {
                    done: true,
                    ..Default::default()
                }),
            ]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap();
        assert!(observed.matched_tool_call.is_none());
        assert_eq!(observed.tool_call_names, vec!["some_other_tool"]);
    }

    /// Evidence already gathered survives a stream that breaks afterwards.
    #[tokio::test]
    async fn tool_call_survives_a_later_stream_error() {
        let observed = drain_probe(
            stream(vec![
                Ok(thinking("hm")),
                Ok(call(PROBE_TOOL)),
                Err(ProviderError::Protocol {
                    safe_context: "truncated".into(),
                }),
            ]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap();
        assert!(observed.matched_tool_call.is_some());
    }

    #[test]
    fn an_exit_code_says_which_kind_of_failure_it_was() {
        assert_eq!(exit_code("task_failed"), 1);
        assert_eq!(exit_code("invalid_input"), 2);
        assert_eq!(exit_code("missing_evidence"), 2);
        assert_eq!(exit_code("policy_denied"), 3);
        assert_eq!(exit_code("provider_unavailable"), 4);
        assert_eq!(exit_code("resource_busy"), 4);
        assert_eq!(exit_code("internal"), 5);
        // An unrecognised category is internal rather than success.
        assert_eq!(exit_code("something_new"), 5);
    }

    #[tokio::test]
    async fn immediate_failure_is_reported_rather_than_denied() {
        let error = drain_probe(
            stream(vec![Err(ProviderError::Timeout {
                safe_context: "local Ollama request".into(),
            })]),
            Some(PROBE_TOOL),
        )
        .await
        .unwrap_err();
        // A timeout must never read as "the model lacks tool support".
        assert!(error.contains("timed out"));
    }

    fn trial(n: u32) -> serde_json::Value {
        serde_json::json!({"trial": n, "arguments": {"value": "ok"}})
    }

    #[tokio::test]
    async fn abandons_a_stream_once_generation_is_underway() {
        // More chunks available than we intend to read: the drop is what ends it.
        let chunks: Vec<_> = (0..50)
            .map(|_| {
                Ok(ModelChunk {
                    content: "1\n".into(),
                    ..Default::default()
                })
            })
            .collect();
        let cancel = pwr_provider::Cancel::new();
        let read = abandon_mid_stream(
            Ok(pwr_provider::cancellable(
                cancel.clone(),
                stream(chunks).unwrap(),
            )),
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(read, CANCEL_AFTER_CHUNKS);
    }

    /// A generation that finishes on its own was never interrupted, so it is no
    /// evidence that the deployment can be cancelled.
    #[tokio::test]
    async fn a_stream_that_completes_first_is_not_cancellation_evidence() {
        let cancel = pwr_provider::Cancel::new();
        let reason = abandon_mid_stream(
            stream(vec![
                Ok(ModelChunk {
                    content: "1".into(),
                    ..Default::default()
                }),
                Ok(ModelChunk {
                    content: "2".into(),
                    done: true,
                    ..Default::default()
                }),
            ]),
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(reason.contains("before it could be interrupted"));
    }

    #[tokio::test]
    async fn a_stream_shorter_than_the_interrupt_point_is_unknown() {
        let cancel = pwr_provider::Cancel::new();
        let reason = abandon_mid_stream(
            stream(vec![Ok(ModelChunk {
                content: "1".into(),
                ..Default::default()
            })]),
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(reason.contains("ended after 1 chunk"));
    }

    #[tokio::test]
    async fn cancellation_probe_surfaces_a_failed_start() {
        let cancel = pwr_provider::Cancel::new();
        let reason = abandon_mid_stream(
            Err(ProviderError::Unavailable {
                safe_context: "local Ollama request".into(),
            }),
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(reason.contains("cancellation probe failed"));
    }

    /// Measured on this host at the same boundary: one deployment accepted the
    /// whole prompt, one evaluated 258 tokens of 4095 and lost the needle with
    /// no error, one returned HTTP 400.
    #[test]
    fn fewer_evaluated_tokens_without_an_error_is_silent_truncation() {
        assert_eq!(boundary_behaviour(4095, 258), "truncated_silently");
        assert_eq!(boundary_behaviour(4044, 4044), "limit_not_enforced");
        // A backend that evaluated more than the reference did not truncate.
        assert_eq!(boundary_behaviour(4000, 4100), "limit_not_enforced");
    }

    #[test]
    fn the_boundary_prompt_carries_a_needle_and_exceeds_the_small_tier() {
        let prompt = boundary_prompt();
        assert!(prompt.starts_with("REMEMBER THIS CODEWORD: ZEPHYR-8813."));
        // Far more characters than the small tier could hold in tokens, so the
        // probe cannot silently degrade into a within-budget request.
        assert!(prompt.len() > BOUNDARY_SMALL_CONTEXT as usize * 8);
    }

    #[test]
    fn unanimous_trials_are_reported_as_reliable() {
        let observed = summarize_tool_trials(3, vec![trial(1), trial(2), trial(3)], vec![]);
        let Observation::Observed(value) = observed else {
            panic!("expected an observation");
        };
        assert_eq!(value["calls"], 3);
        assert_eq!(value["reliable"], true);
    }

    /// Nemotron emits a native call only on some runs. The capability is real,
    /// but a caller must be able to see it is not dependable.
    #[test]
    fn intermittent_trials_are_observed_but_not_reliable() {
        let observed = summarize_tool_trials(
            3,
            vec![trial(2)],
            vec![
                "trial 1: no native tool call".into(),
                "trial 3: no native tool call".into(),
            ],
        );
        let Observation::Observed(value) = observed else {
            panic!("expected an observation");
        };
        assert_eq!(value["calls"], 1);
        assert_eq!(value["trials"], 3);
        assert_eq!(value["reliable"], false);
        // The failing trials stay in the record; a rate is not evidence unless
        // the misses are visible too.
        assert_eq!(value["failures"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn zero_calls_is_unknown_and_names_every_trial() {
        let observed = summarize_tool_trials(
            2,
            vec![],
            vec![
                "trial 1: timed out".into(),
                "trial 2: no native tool call".into(),
            ],
        );
        let Observation::Unknown { reason } = observed else {
            panic!("absence of a call is not proof of absent support");
        };
        assert!(reason.contains("trial 1: timed out"));
        assert!(reason.contains("trial 2"));
    }

    #[tokio::test]
    async fn empty_stream_is_unknown_not_absent() {
        assert!(drain_probe(stream(vec![]), Some(PROBE_TOOL)).await.is_err());
    }

    #[tokio::test]
    async fn single_chunk_reply_does_not_demonstrate_streaming() {
        let observed = drain_probe(
            stream(vec![Ok(ModelChunk {
                content: "OK".into(),
                done: true,
                ..Default::default()
            })]),
            None,
        )
        .await
        .unwrap();
        assert_eq!(observed.chunks, 1);
    }

    #[tokio::test]
    async fn drain_stops_at_the_chunk_bound() {
        let endless: Vec<_> = (0..PROBE_MAX_CHUNKS + 500)
            .map(|_| Ok(thinking("x")))
            .collect();
        let observed = drain_probe(stream(endless), Some(PROBE_TOOL))
            .await
            .unwrap();
        assert_eq!(observed.chunks, PROBE_MAX_CHUNKS);
    }

    /// A configuration written by an earlier build is missing whatever has
    /// been added since. Refusing it stopped the console from opening at all,
    /// which leaves no way to fix a setting that lives inside the console.
    #[test]
    fn a_configuration_from_an_earlier_build_still_opens_the_console() {
        let partial = serde_json::json!({"backend": "lmstudio"}).to_string();
        let config: ChatConfig =
            serde_json::from_str(&partial).expect("a partial configuration must load");
        assert_eq!(config.backend.as_deref(), Some("lmstudio"));
        assert_eq!(config.context_tokens, CHAT_CONTEXT_DEFAULT);
        // And the safe side of every choice it did not make.
        assert!(config.require_probe);
        assert!(config.model.is_none());
    }

    /// Selecting a model opens a supervised manual path; a prior inspection
    /// can upgrade the label, but no capability probe is a gate.
    #[test]
    fn readiness_does_not_make_a_probe_a_manual_run_prerequisite() {
        let selected = |prepared: bool| ChatConfig {
            model: Some("m".into()),
            prepared_for_model: prepared.then(|| "m".into()),
            ..ChatConfig::default()
        };
        assert!(chat_is_prepared(&selected(false)));
        assert_eq!(readiness_label(&selected(false)), "READY · UNMEASURED");
        assert_eq!(readiness_label(&selected(true)), "READY");
        assert_eq!(
            readiness_label(&ChatConfig::default()),
            "CHOOSE MODEL",
            "a model that was never chosen cannot be ready"
        );
    }

    /// A configuration from the probe-gated console remains readable, but its
    /// old preference cannot prevent the selected model being used manually.
    #[test]
    fn legacy_probe_preference_does_not_withdraw_manual_readiness() {
        let config = ChatConfig {
            model: Some("m".into()),
            prepared_for_model: None,
            require_probe: true,
            ..ChatConfig::default()
        };
        assert!(chat_is_prepared(&config));
    }

    /// Profiles still bind to the deployment that produced them, but manual
    /// readiness comes from selecting an available model and its computed
    /// window, not from a calibration or a probe.
    #[test]
    fn readiness_does_not_wait_on_a_calibration_that_cannot_be_taken() {
        let mut config = ChatConfig {
            model: Some("qwen/qwen3.5-9b@4bit".into()),
            profile: None,
            prepared_for_model: Some("qwen/qwen3.5-9b@4bit".into()),
            prepared_without_calibration: false,
            ..ChatConfig::default()
        };
        assert!(chat_is_prepared(&config));
        config.prepared_without_calibration = true;
        assert!(chat_is_prepared(&config));
        // A changed model remains selectable; its old profile is cleared by
        // the interactive selector before a turn is allowed to use it.
        config.model = Some("qwen/qwen3.5-9b@q4_k_m".into());
        assert!(chat_is_prepared(&config));
    }

    #[test]
    fn an_embedded_resource_is_attached_like_a_file_and_named_by_its_uri() {
        let workspace = tempfile::tempdir().unwrap();
        let attached = attach_chat_bytes(
            workspace.path(),
            "zed:///buffer/3",
            b"fn unsaved() {}".to_vec(),
        )
        .unwrap_or_else(|error| panic!("{}", error.context));
        assert!(
            attached
                .content
                .starts_with("[Read-only attachment: zed:///buffer/3 |")
        );
        assert!(attached.content.ends_with("\nfn unsaved() {}"));
        let snapshot = workspace
            .path()
            .join(".pwr/chat-attachments")
            .join(format!("{}.txt", hash_bytes(b"fn unsaved() {}")));
        assert_eq!(fs::read_to_string(snapshot).unwrap(), "fn unsaved() {}");
        let Err(refused) = attach_chat_bytes(workspace.path(), "file:///blob", vec![0xff, 0xfe])
        else {
            panic!("bytes that are not text were attached");
        };
        assert!(
            refused.context.contains("file:///blob is not UTF-8"),
            "{}",
            refused.context
        );
    }

    /// A project's folder attached from a site in its subfolder becomes a
    /// read-only reference the turn's policy can read, not a truncated paste.
    #[test]
    fn an_attached_parent_folder_becomes_a_readable_reference() {
        let project = tempfile::tempdir().unwrap();
        let project_root = project.path().canonicalize().unwrap();
        fs::create_dir_all(project_root.join("docs")).unwrap();
        fs::create_dir_all(project_root.join("site")).unwrap();
        fs::write(project_root.join("docs/guide.md"), "the guide").unwrap();
        let site = project_root.join("site");

        let message = attach_chat_file(&site, &project_root)
            .unwrap_or_else(|error| panic!("{}", error.context));
        assert!(
            message.content.contains("../docs/guide.md"),
            "{}",
            message.content
        );
        let config = load_chat_config(&site).unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(config.reference_roots, vec![PathBuf::from("..")]);
        // Attaching it again declares nothing new.
        attach_chat_file(&site, &project_root).unwrap_or_else(|error| panic!("{}", error.context));
        let config = load_chat_config(&site).unwrap_or_else(|error| panic!("{}", error.context));
        assert_eq!(config.reference_roots.len(), 1);

        let policy = pwr_tools::ToolPolicy {
            extra_readable: reference_roots(&site, &config),
            ..pwr_tools::PolicyProfile::Safe.build(site.clone())
        };
        let read = pwr_tools::read_file(&policy, Path::new("../docs/guide.md")).unwrap();
        assert!(read.content.contains("the guide"));
        assert!(chat_system_prompt_for(&site).contains("../docs/guide.md"));

        // Composing a turn keeps them in the system message.
        let mut messages = vec![
            ChatMessage::text("system", chat_system_prompt_for(&site)),
            ChatMessage::text("user", "what does the guide say?"),
        ];
        let _ = compose_chat_turn(
            &site,
            32_768,
            &pwr_orchestrator::TaskProfile::resolve(None, None),
            None,
            &mut messages,
        );
        assert!(
            messages[0].content.contains("../docs/guide.md"),
            "{}",
            messages[0].content
        );
    }

    #[test]
    fn structured_history_and_reasoning_leave_no_room_for_retrieval() {
        for reasoning in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(
                root.path().join("a.txt"),
                "retrieval_sentinel important file content",
            )
            .unwrap();
            let profile = pwr_orchestrator::TaskProfile::resolve(None, None);
            let mut baseline = vec![
                ChatMessage::text("system", "work"),
                ChatMessage::text("user", "find retrieval_sentinel"),
            ];
            assert!(
                compose_chat_turn(root.path(), 16384, &profile, None, &mut baseline)
                    .unwrap()
                    .0
                    .is_some()
            );
            let mut generated = ChatMessage::text("assistant", "");
            if reasoning {
                generated.reasoning = Some("reasoning ".repeat(15000));
            } else {
                generated.tool_calls.push(pwr_domain::ToolCall {
                    name: "write_file".into(),
                    id: Some("large".into()),
                    arguments: serde_json::json!({"path":"a.txt","content":"text ".repeat(30000)}),
                });
            }
            let mut messages = vec![
                ChatMessage::text("system", "work"),
                generated,
                ChatMessage::text("tool", "saved"),
                ChatMessage::text("user", "find retrieval_sentinel"),
            ];
            let result = compose_chat_turn(
                root.path(),
                16384,
                &pwr_orchestrator::TaskProfile::resolve(None, None),
                None,
                &mut messages,
            );
            assert_eq!(result.unwrap().0, None);
            assert!(
                messages
                    .last()
                    .unwrap()
                    .content
                    .contains("find retrieval_sentinel")
            );
        }
    }

    /// Composing the turn replaced the person's message with the composed
    /// one and lost its images: the model answered that it saw no image
    /// (2026-09-23, the first end-to-end check of C.25).
    #[test]
    fn composing_a_turn_keeps_the_attached_images() {
        let workspace = tempfile::tempdir().unwrap();
        let mut request = ChatMessage::text("user", "what does this screenshot say?");
        request.images = vec![PathBuf::from("/w/.pwr/images/ab.png")];
        let mut messages = vec![ChatMessage::text("system", "be brief"), request];
        compose_chat_turn(
            workspace.path(),
            32_768,
            &pwr_orchestrator::TaskProfile::resolve(None, None),
            None,
            &mut messages,
        )
        .unwrap();
        let last = messages.last().unwrap();
        assert_eq!(last.role, "user");
        assert!(last.content.contains("what does this screenshot say?"));
        assert_eq!(last.images, [PathBuf::from("/w/.pwr/images/ab.png")]);
    }

    /// Measured 2026-09-26 (stack matrix c1, cs-docker): every goal turn's
    /// own note -- "The checks pass. Before finishing..." -- was composed as a
    /// new request, so a fresh ledger and passages ranked against that note
    /// were added to the history on each of the goal's turns.
    #[test]
    fn a_goal_s_own_note_is_not_composed_as_a_request() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(
            workspace.path().join("README.md"),
            "# Notes\n\nThe checks pass here.\n",
        )
        .unwrap();
        let mut note = ChatMessage::text("user", "The checks pass. Before finishing, review.");
        note.purpose = Some(pwr_domain::MessagePurpose::GoalGuidance);
        let mut messages = vec![ChatMessage::text("system", "be brief"), note.clone()];
        let composed = compose_chat_turn(
            workspace.path(),
            32_768,
            &pwr_orchestrator::TaskProfile::resolve(None, None),
            Some("Ledger of 1 earlier run(s) of this session"),
            &mut messages,
        )
        .unwrap();
        assert_eq!(composed, (None, None));
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[1], note);
    }

    /// Reading the person's terminal is never granted by a mode, Full access
    /// included; only their "Allow for this session" grants it.
    #[test]
    fn reading_the_terminal_is_granted_only_by_the_person() {
        let full = ChatConfig {
            permission_mode: Some(PermissionMode::Full),
            ..ChatConfig::default()
        };
        assert!(
            !chat_approvals(&effective_ask_before(&full), &[])
                .contains(&pwr_tools::Approval::TerminalRead)
        );
        assert!(
            chat_approvals(
                &effective_ask_before(&ChatConfig::default()),
                &[pwr_tools::Approval::TerminalRead]
            )
            .contains(&pwr_tools::Approval::TerminalRead)
        );
    }

    /// The owner's Libra configuration of 2026-10-02, as saved: a window the
    /// app computed (262,144) and no threshold chosen. A default ceiling
    /// compacted that conversation at 34,039 tokens, 13 % of the window,
    /// while the app said 75 %.
    #[test]
    fn a_computed_window_compacts_at_three_quarters_of_it() {
        let saved: ChatConfig = serde_json::from_str(
            r#"{"background_summaries": false, "backend": null,
                "model": "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
                "reasoning_effort": "medium", "actions_per_turn": 100,
                "acknowledged_provisional": [], "profile": null,
                "prepared_for_model": "mlx-community/NVIDIA-Nemotron-3.5-Lightning-30B-A3B-4bit",
                "require_probe": true, "prepared_without_calibration": true,
                "context_tokens": 262144, "timeout_secs": 900, "plan": false,
                "permission_mode": "full"}"#,
        )
        .unwrap();
        let mut continuity = converse::Continuity::default();
        compaction_settings(&saved, &mut continuity);
        assert_eq!(continuity.compaction_room(saved.context_tokens), 196_608);
        let ceiling = ChatConfig {
            compact_ceiling_tokens: Some(65_536),
            ..saved
        };
        compaction_settings(&ceiling, &mut continuity);
        assert_eq!(continuity.compaction_room(ceiling.context_tokens), 65_536);
    }

    #[test]
    fn folder_attachments_snapshot_text_without_following_build_or_git_directories() {
        let workspace = tempfile::tempdir().unwrap();
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("brief.md"), "Build the public site.").unwrap();
        std::fs::create_dir_all(folder.path().join("node_modules/package")).unwrap();
        std::fs::write(
            folder.path().join("node_modules/package/index.js"),
            "this dependency must not be included",
        )
        .unwrap();
        std::fs::create_dir_all(folder.path().join(".git")).unwrap();
        std::fs::write(folder.path().join(".git/config"), "private metadata").unwrap();

        // Outside the workspace: declared as a reference and listed, not
        // pasted -- and still nothing from dependencies or Git.
        let attachment = attach_chat_file(workspace.path(), folder.path())
            .unwrap_or_else(|error| panic!("{}", error.context));
        assert!(attachment.content.contains("brief.md"));
        assert!(!attachment.content.contains("Build the public site."));
        assert!(!attachment.content.contains("node_modules"));
        assert!(!attachment.content.contains(".git"));

        // Inside it: a bounded snapshot, as before.
        let inner = workspace.path().join("brief");
        std::fs::create_dir_all(inner.join("node_modules/package")).unwrap();
        std::fs::create_dir_all(inner.join(".git")).unwrap();
        std::fs::write(inner.join("brief.md"), "Build the public site.").unwrap();
        std::fs::write(
            inner.join("node_modules/package/index.js"),
            "this dependency must not be included",
        )
        .unwrap();
        std::fs::write(inner.join(".git/config"), "private metadata").unwrap();
        let snapshot = attach_chat_file(workspace.path(), &inner)
            .unwrap_or_else(|error| panic!("{}", error.context));
        assert!(snapshot.content.contains("Build the public site."));
        assert!(!snapshot.content.contains("dependency must not be included"));
        assert!(!snapshot.content.contains("private metadata"));
    }

    #[test]
    fn tui_paste_preserves_multiline_special_character_prompts() {
        let pasted = "Create a portfolio — dark mode.\r\nUse \"special\" chars & symbols.\r\n";
        assert_eq!(
            normalise_tui_paste(pasted),
            "Create a portfolio — dark mode.\nUse \"special\" chars & symbols.\n"
        );
    }

    #[test]
    fn tab_completion_finds_an_attachment_path_with_spaces() {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("project brief.pdf"), "plain text").unwrap();
        let mut input = "project br".to_string();
        complete_attachment_path(&mut input, workspace.path());
        assert_eq!(input, "project brief.pdf");
    }

    #[test]
    fn a_queued_attachment_is_sent_with_only_the_next_composed_task() {
        let task = task_with_tui_attachments(
            "Create the portfolio".into(),
            &["[Read-only attachment: cv.pdf]\nCV contents".into()],
        );
        assert!(task.starts_with("Create the portfolio"));
        assert!(task.contains("--- Attachments for this task only ---"));
        assert!(task.ends_with("CV contents"));
    }
}

#[cfg(test)]
mod protection_tests {
    use super::*;

    /// Absent protects nothing; a well-formed file protects what it names;
    /// anything else stops the work rather than protecting nothing in silence.
    #[test]
    fn a_protection_file_that_cannot_be_read_stops_the_work() {
        let workspace = tempfile::tempdir().unwrap();
        let root = workspace.path();
        assert!(
            frozen_paths(root)
                .ok()
                .is_some_and(|paths| paths.is_empty()),
            "absent means nothing"
        );

        fs::create_dir_all(root.join(".pwr")).unwrap();
        fs::write(
            root.join(".pwr/protected.json"),
            r#"{"protected": ["test/orders.test.js"]}"#,
        )
        .unwrap();
        assert_eq!(
            frozen_paths(root).ok(),
            Some(vec![PathBuf::from("test/orders.test.js")])
        );

        // The shape three experiment runs used on 2026-09-23.
        fs::write(
            root.join(".pwr/protected.json"),
            r#"{"paths": ["test/orders.test.js"]}"#,
        )
        .unwrap();
        let Err(error) = frozen_paths(root) else {
            panic!("an unknown key protected nothing, silently");
        };
        assert!(error.context.contains("\"protected\""), "{}", error.context);

        fs::write(root.join(".pwr/protected.json"), "{not json").unwrap();
        assert!(frozen_paths(root).is_err());
    }
}
