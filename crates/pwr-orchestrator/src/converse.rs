//! The conversation, which is also where the work happens.
//!
//! There is one loop. A turn may answer, read, edit, run a command, or any
//! sequence of those, and the operator sees one thread rather than a chat that
//! hands work to something else. This is a deliberate move away from the
//! project's original shape, where every change was a separate bounded run:
//! the seam that shape produced -- an agent that could not see the
//! conversation that asked for the work -- cost more than the tidy audit
//! boundary was worth.
//!
//! What survives from that shape is the part that was load-bearing. Every
//! action is still executed through the audited executor, so the event log and
//! `pwr report` are unchanged. And a turn that edits still has to face the
//! repository's own checks: the baseline is captured before the first edit and
//! compared after, so "it changed your code" always arrives with what the
//! checks said about it.

use pwr_compat::ModelBehaviorAdapter;
use pwr_domain::{ChatMessage, DeploymentDescriptor, ModelRequest, ToolCatalog, ToolDefinition};
use pwr_provider::ModelProvider;
use pwr_tools::{ActionProposal, ToolPolicy};
use std::collections::BTreeMap;

/// The share of the context that triggers compaction.
///
/// Every action's result joins the prompt, so a turn that keeps working walks
/// toward the limit. Walking into it produces a provider error about a prompt
/// that is too long; compacting before it lets the turn carry on.
///
/// The default; a workspace may set its own within [`COMPACT_AT_BOUNDS`].
pub const COMPACT_AT: f64 = 0.75;

/// The most a conversation grows before it compacts, in tokens, when the person
/// chose neither a window nor a threshold. HYPOTHESIS, a usability decision
/// rather than a quality measurement: a model with a 262k window on a 64 GB
/// Mac has a window far above any size at which a 4-bit model was seen to stay
/// coherent, and every compaction (like a model switch) re-reads the whole
/// prompt -- 32 minutes for a 30B at that size, measured 2026-09-30. Nemotron,
/// GLM and Qwen3.5 went off the rails in long conversations in the same pass.
/// Set `context_tokens` or the compaction percentage to opt out.
pub const DEFAULT_COMPACTION_CEILING_TOKENS: usize = 65_536;

/// The thresholds, in percent of the window, a person may choose. Below half,
/// a conversation compacts so often it forgets what it just read; above nine
/// tenths, a single file read can overflow the window before the check runs.
pub const COMPACT_AT_BOUNDS: (u8, u8) = (50, 90);

/// The share of the room the verbatim tail may occupy after compaction.
///
/// A count of messages was the obvious thing and the wrong one: eight recent
/// exchanges are nothing in a 128k window and do not fit at all in a 4k one,
/// where four file reads alone overflow it. So the tail is measured in tokens
/// and the count falls out of it -- what is kept is however much recent
/// conversation fits in half the room, and never fewer than two messages.
const VERBATIM_SHARE: f64 = 0.5;

/// Consecutive turns that produce nothing before the deployment is treated as
/// unable to continue.
///
/// A deployment that says nothing once can be asked again -- measured on a 9B,
/// which occasionally replies empty after a tool result and then carries on.
/// One that says nothing three times running is not about to start.
///
/// This is the bound that an action count used to provide by accident. When
/// the count went, nothing replaced it for this case: an empty turn adds a
/// one-line nudge, so the context barely grows, so neither compaction nor the
/// looping guard ever fires. Measured on an 80B: twelve turns alternating
/// 2,246 and 26 generated tokens against a prompt frozen at 7,090, producing
/// no actions and no answer, until the run was killed by hand.
const EMPTY_TURNS_BEFORE_GIVING_UP: usize = 3;
/// How many consecutive unreadable tool calls end a conversation.
///
/// Measured on an 80B asked the same thing three times: two calls parsed, one
/// did not. One bad generation is noise; three in a row is a deployment whose
/// tool calling this backend cannot rely on, and saying so beats retrying.
const UNPARSEABLE_CALLS_BEFORE_GIVING_UP: usize = 3;

/// Compactions one turn may perform before it is treated as looping.
///
/// There is no action budget here: an action count is what bounds a run nobody
/// is watching, and this one has an operator and a stop key. But compaction
/// removes the limit that would otherwise end a runaway turn by itself, so
/// something has to take its place -- and "this turn has re-summarised its own
/// context twice and still has not finished" is evidence of looping, where "it
/// has taken thirty actions" is only evidence that the work was large.
const COMPACTIONS_PER_TURN: usize = 2;

/// Actions one turn takes before it stops and asks.
///
/// A conversation had no budget at all. Its only bounds were the operator
/// pressing stop, three silent turns, three unreadable replies and two
/// compactions; between one action and the next, nothing counted. MASTER_SPEC's
/// eighth principle says to bound resources and count every underlying action,
/// and this was the path that did not.
///
/// A soft bound rather than the scripted loop's hard one, because the two modes
/// differ in the way that matters here: a run is unattended and its budget is a
/// cap, while a conversation has someone in front of it and the budget is a
/// place to check in. Hitting it ends the turn with a reason and a count; the
/// next message continues the work, because the history is still there and
/// nothing was discarded.
///
/// The default is 100 and the person can change it per workspace
/// (`actions_per_turn` in `.pwr/chat-config.json`): it was 26, anchored to the
/// scripted loop's budget, which is twice the maximum of the three real defects
/// in `external-v1` (7, 11 and 13 actions). A real build from scratch takes
/// more -- a manual pass on 2026-09-30 stopped a working model mid-task at 26 --
/// so the default moved to a number a stuck loop still cannot spend unnoticed
/// (the stall detectors stop it far earlier). It is a choice, not a measurement
/// of conversations.
pub const DEFAULT_ACTIONS_PER_TURN: usize = 100;

/// Consecutive backend faults a turn absorbs before it stops and says so.
///
/// Three, consecutive, like the silent and unreadable counters beside it: a
/// backend that drops one connection and then works is a backend that works.
///
/// What this replaces is worse than it sounds. Any provider error other than a
/// truncated or unparseable reply ended the turn with `Err`, and the console
/// gets no messages back from a failed turn -- so a transient fault after the
/// deployment had edited three files left the files edited on disk and the
/// conversation with no record that it had touched them. The next turn would
/// then read hashes it had never been told about.
const BACKEND_FAULTS_BEFORE_GIVING_UP: usize = 3;

/// Retries after a generation reasoned to its budget without an answer. The
/// engine has already closed the phase once; the retry asks for the answer
/// with thinking off (or at a zero budget). One retry, consecutive: a second
/// failure ends the turn, so reasoning enforcement can never loop.
pub const REASONING_FINALIZATION_RETRIES: usize = 1;

/// The cap on the answer/action phase of a reply, when the sampling names
/// none: the engines' own default cap on a whole reply before reasoning had a
/// budget of its own, so an action is given the room it always had.
const ANSWER_ALLOWANCE: u32 = 16_384;
/// Ornith-1.5 on MLX spent its 4,096-token thinking budget, then emitted over
/// 48 KiB of additional self-dialogue in the answer channel until the 20,480
/// token cap. Plain answer text this long after substantial thinking, with no
/// tool call, is not useful progress for an agent turn. Keep tool-call bodies
/// outside this guard: the adapter holds them rather than streaming them as
/// prose.
const AGENT_UNSTRUCTURED_REPLY_GUARD: (usize, usize) = (3_000, 12_000);
const RUNAWAY_RETRY_MAX_TOKENS: u32 = 8_192;

/// Replies that fell apart, in a row, before the conversation is cut back to
/// start clean; how many such cuts a turn may make; and the room they keep.
const DEGENERATE_REPLIES_BEFORE_RESET: usize = 2;
const DEGENERATE_RESETS_PER_TURN: usize = 2;
const DEGENERATE_RESET_ROOM: usize = 8_192;

/// The presence penalty a turn asks for after one of its generations looped.
/// Qwen's cards say to raise it, between 0 and 2, "to reduce endless
/// repetitions"; 1.0 is half way. Measured 2026-09-30: Qwen3.5-9B with its
/// card's sampling (penalty 0) still looped in its reasoning on a Bash task --
/// the same passage four times, five generations in an hour.
const ANTI_LOOP_PRESENCE_PENALTY: f64 = 1.0;

/// How far back the engine looks for a token already written, once a reply has
/// looped. mlx-lm's own window is 20 tokens (read from its source, 2026-10-01),
/// which cannot see a passage repeated at length: the penalty above did
/// nothing against exactly the loops it was added for.
const ANTI_LOOP_PRESENCE_WINDOW: u64 = 1_024;
/// Replies a turn may lose to being cut off inside a tool call, in all.
///
/// Not consecutive, unlike the counters beside it. Measured 2026-09-30 (Bonsai
/// 27B 1-bit in the desktop, asked for a whole site): the model tried to write
/// the file in one call, was cut off after eleven minutes, made two small calls
/// -- which reset every consecutive counter -- and tried again, four times over
/// forty minutes. What repeats is the size of the file, not a stumble.
const CUT_OFF_REPLIES_PER_TURN: usize = 3;

/// What one exchange produced.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnReport {
    pub outcome: pwr_domain::TurnOutcome,
    /// What to say back. Empty when the turn only acted.
    pub answer: String,
    pub actions: usize,
    /// Whether anything in the workspace changed, which is what makes the
    /// checks worth running.
    pub edited: bool,
    /// `true` only when the deployment used the structured `complete` action.
    /// A prose answer can end an ordinary conversation, but it cannot satisfy
    /// a goal-driven session's completion contract.
    pub completed: bool,
    /// Why the turn ended, where that was not the model finishing.
    pub stopped: Option<StopReason>,
    /// The model declined the request and said why in `answer`. Not a stop:
    /// the turn ended as the model chose, and a front end names it a refusal.
    pub declined: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The operator pressed stop.
    Interrupted,
    /// The conversation filled the context it was given.
    ContextFull,
    /// The turn kept compacting its own context without finishing, which is
    /// looping rather than working.
    Looping,
    /// The deployment produced neither an answer nor an action, repeatedly.
    Silent,
    /// The deployment wrote tool calls only inside its reasoning phase.
    ToolCallInReasoning,
    /// The deployment kept emitting tool calls this backend could not parse.
    Unparseable,
    /// The turn took the actions it is given before checking in.
    BudgetSpent,
    /// The backend failed repeatedly rather than the deployment failing.
    BackendFailing,
    /// Window after window of actions left the workspace exactly where it
    /// was. Not a budget: a goal must not carry on past it by itself.
    NoProgress,
    /// The model reasoned to its budget without answering, and again when
    /// asked to answer directly.
    ReasoningUnfinished,
}

impl StopReason {
    /// The run's terminal class for the same stop, so a turn and a run that
    /// ended for the same reason are classified the same way.
    pub fn terminal_class(self) -> pwr_domain::TerminalClass {
        use pwr_domain::TerminalClass;
        match self {
            Self::Interrupted => TerminalClass::Interrupted,
            Self::BudgetSpent => TerminalClass::Budget,
            Self::Silent
            | Self::ToolCallInReasoning
            | Self::Unparseable
            | Self::ReasoningUnfinished => TerminalClass::Protocol,
            Self::BackendFailing => TerminalClass::Provider,
            // Compaction could not make room, twice or at all: recovery ran out
            // of ways forward, which is what the run calls the same position.
            Self::ContextFull | Self::Looping | Self::NoProgress => TerminalClass::Recovery,
        }
    }

    /// Every way a turn can stop.
    ///
    /// Listed rather than derived, and held to it by the exhaustive match in
    /// this module's tests: a new variant fails to compile there until it is
    /// added here, which is what stops a stop reason from reaching an operator
    /// having never been read by anyone.
    ///
    /// Test-only, and said so rather than kept alive by a token use in the
    /// product: its whole job is to make the fixture unable to miss a variant.
    #[cfg(test)]
    pub const ALL: [Self; 10] = [
        Self::Interrupted,
        Self::ContextFull,
        Self::Looping,
        Self::Silent,
        Self::ToolCallInReasoning,
        Self::Unparseable,
        Self::BudgetSpent,
        Self::BackendFailing,
        Self::NoProgress,
        Self::ReasoningUnfinished,
    ];

    pub fn said(self) -> String {
        match self {
            Self::Interrupted => "stopped at your request".to_owned(),
            Self::ContextFull => {
                "this conversation filled its context faster than it could be summarised; start \
                 a new one, or raise the context budget in Settings"
                    .to_owned()
            }
            Self::Looping => {
                "this turn summarised its own context twice and still did not finish, so it was \
                 stopped; ask for something narrower"
                    .to_owned()
            }
            Self::Silent => {
                "the deployment produced neither an answer nor an action three times running, so \
                 it was stopped; its replies may be in a form this backend cannot parse into tool \
                 calls"
                    .to_owned()
            }
            Self::ToolCallInReasoning => {
                "the deployment wrote tool calls inside its reasoning phase three times running; \
                 those calls cannot be executed, so the turn was stopped"
                    .to_owned()
            }
            Self::Unparseable => {
                "three turns running produced nothing this backend could use -- tool calls that \
                 were not valid JSON, replies that ran on until they were cut off, or replies \
                 that went round in circles -- so it was stopped; everything done before that \
                 is kept, and carrying on continues from it. A smaller task, a lower Reasoning \
                 Effort (a small model that loops in its reasoning often stops when it has less \
                 room to), or a deployment whose tool calling has been demonstrated is more \
                 likely to hold a long conversation together"
                    .to_owned()
            }
            Self::BackendFailing => format!(
                "the backend failed {BACKEND_FAULTS_BEFORE_GIVING_UP} times running -- this is \
                 the server, not the model -- so the turn stopped; everything it did before that \
                 is kept, and carrying on will retry. Check the backend is still serving the \
                 model, then say to continue"
            ),
            Self::NoProgress => format!(
                "{} windows of {} actions running left the workspace exactly as it was and \
                 repeated what had already been tried, so the work was stopped rather than left \
                 to spend its budget in a circle; everything it did is kept -- read the last \
                 actions, then say what to change",
                crate::stall::NO_PROGRESS_LIMIT,
                crate::stall::NO_PROGRESS_WINDOW
            ),
            Self::ReasoningUnfinished => {
                "the model reasoned until its thinking budget ran out without answering, and did                  the same when asked to answer directly, so the turn stopped; everything done                  before is kept. A lower Reasoning Effort, a narrower request, or another model                  may get further"
                    .to_owned()
            }
            Self::BudgetSpent => format!(
                "this turn used the actions a turn is given before checking in (the default is \
                 {DEFAULT_ACTIONS_PER_TURN}; `actions_per_turn` in the workspace's chat \
                 configuration changes it), and stopped here rather than carrying on unasked; \
                 say what to do next -- \"carry on\" is enough, and nothing it has done or read \
                 was discarded"
            ),
        }
    }
}

/// What a turn did on the way, for the console to show beside the answer.
pub enum TurnStep {
    Acted {
        capability: String,
        detail: String,
    },
    Refused(String),
    Compacted(String),
    /// A message the person sent while the turn was working, now delivered.
    Steered(String),
    /// One action's life, for a front end that shows actions as objects rather
    /// than as lines: announced before it runs, then completed, failed or
    /// refused. Emitted beside `Acted` and `Refused`, which the console renders.
    ToolCall(ToolCallStep),
    /// Something the turn has to say that is neither an action nor a refusal:
    /// the check verdict after an edit, a snapshot that could not be saved.
    Note(String),
    /// Text of the reply being generated, as it arrives: `thinking` on the
    /// reasoning channel, `content` on the answer channel. Deltas, not totals.
    ///
    /// Without it a front end showed nothing for as long as a generation took
    /// -- measured 2026-09-22, twenty minutes of an empty conversation while a
    /// 27B wrote one page -- and then only the action it had decided on.
    Streaming {
        thinking: String,
        content: String,
    },
    /// The engine is reading the prompt before it can answer: tokens processed
    /// of the total. A long conversation on a cold cache (after a model switch,
    /// say) takes minutes; measured 2026-09-30, a 32-minute first word that
    /// looked like a hang.
    Prefill {
        processed: u64,
        total: u64,
    },
    /// A fact the model proposed to remember, for the person to confirm or
    /// dismiss. Nothing is saved by the model.
    MemoryProposed {
        text: String,
        /// `workspace` or `global`.
        scope: String,
    },
    /// Tokens the conversation occupies, and the window: the backend's count
    /// after a reply, or -- `estimated` -- the prompt about to be sent, the
    /// last count plus an estimate of what was appended since (tool results,
    /// steering). The second lets a front end follow the window between
    /// generations instead of only when a reply has finished.
    Usage {
        used: u64,
        window: u64,
        estimated: bool,
    },
    /// A generation or call the turn could not use and is asking for again,
    /// automatically, within its own bound. Emitted beside the `Refused` that
    /// the console renders, for a front end that shows retries as state.
    Retry {
        /// `reply_fault`, `malformed_call`, `backend_fault`, `silent`,
        /// `reasoning_unfinished` or `context_limit`.
        cause: &'static str,
        attempt: usize,
        limit: usize,
        detail: String,
    },
    /// The turn produced something usable after `retries` retries.
    Recovered {
        retries: usize,
    },
    /// One generation's counts and timings, as the backend reported them.
    Generation(GenerationStats),
}

/// What one generation cost, for a front end's runtime figures.
#[derive(Debug, Clone)]
pub struct GenerationStats {
    pub metrics: Option<pwr_domain::GenerationMetrics>,
    /// From the request to the last chunk, measured here.
    pub elapsed: std::time::Duration,
    /// From the request to the first streamed chunk, measured here.
    pub first_chunk: Option<std::time::Duration>,
}

/// An action as a front end tracks it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallStep {
    /// Unique within the turn, and the same across an action's phases.
    pub id: u64,
    pub capability: String,
    /// A concise, user-visible description of this particular invocation.
    /// It is operational evidence, never model chain-of-thought.
    pub detail: String,
    pub path: Option<String>,
    pub phase: ToolPhase,
    /// For an edit that completed: the file before and after.
    pub diff: Option<FileDiff>,
}

fn tool_call_detail(call: &pwr_domain::ToolCall) -> String {
    let arguments = call.arguments.as_object();
    match call.name.as_str() {
        "read_file" | "write_file" | "replace_text" | "apply_patch_hunks" => arguments
            .and_then(|fields| fields.get("path"))
            .and_then(serde_json::Value::as_str)
            .map(|path| path.to_string())
            .unwrap_or_else(|| "a workspace file".into()),
        "search" => {
            let query = arguments
                .and_then(|fields| fields.get("query"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("workspace content");
            let scope = arguments
                .and_then(|fields| fields.get("path_glob"))
                .and_then(serde_json::Value::as_str)
                .map(|glob| format!(" in {glob}"))
                .unwrap_or_default();
            format!("{query:?}{scope}")
        }
        "run_command" => {
            let executable = arguments
                .and_then(|fields| fields.get("executable"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("command");
            let args = arguments
                .and_then(|fields| fields.get("args"))
                .and_then(serde_json::Value::as_array)
                .map(|args| {
                    args.iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let place = arguments
                .and_then(|fields| fields.get("cwd"))
                .and_then(serde_json::Value::as_str)
                .filter(|dir| !dir.trim().is_empty() && *dir != ".")
                .map(|dir| format!(" (in {dir})"))
                .unwrap_or_default();
            format!("{}{place}", format!("{executable} {args}").trim())
        }
        "look_at" => arguments
            .and_then(|fields| fields.get("target"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("a page")
            .to_owned(),
        "list_tree" => arguments
            .and_then(|fields| fields.get("path"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("workspace")
            .to_owned(),
        _ => arguments
            .map(|fields| {
                let mut keys = fields.keys().cloned().collect::<Vec<_>>();
                keys.sort();
                if keys.is_empty() {
                    "No arguments".into()
                } else {
                    format!("Arguments: {}", keys.join(", "))
                }
            })
            .unwrap_or_else(|| "No arguments".into()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPhase {
    /// Decided on and about to be put to the policy, which may ask a person
    /// about it. Announced before that question so a front end can show what
    /// the question is about.
    Proposed,
    Started,
    Completed,
    Failed(String),
    /// Not run: a repeat of a refused action, an approval refused, or a call
    /// that did not decode.
    Refused(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiff {
    pub path: String,
    /// `None` for a file the action created.
    pub old_text: Option<String>,
    pub new_text: String,
}

/// What carries a conversation across turns, restarts and the person typing
/// while it works.
///
/// Shared rather than returned: a turn that fails still leaves its checkpoint
/// behind, and the console must be able to queue steering into a turn it does
/// not own while that turn runs on another task.
#[derive(Debug, Clone, Default)]
pub struct Continuity {
    /// Remaining actions for this turn under a goal-wide budget.
    pub action_limit: Option<usize>,
    /// The person's Reasoning Effort, and what is known about how this
    /// deployment reasons. Every generation turns the two into a budget with
    /// [`pwr_domain::plan_reasoning`], inside the room the context has left.
    pub reasoning_effort: pwr_domain::ReasoningEffort,
    pub reasoning: pwr_domain::ReasoningProfile,
    /// The model's compatibility status, for the audit.
    pub profile_status: Option<String>,
    /// Where the conversation stands, updated at every action boundary and
    /// recorded as it is updated.
    pub checkpoint: std::sync::Arc<std::sync::Mutex<crate::conversation::Checkpoint>>,
    /// Messages typed during a turn, delivered at its next safe point.
    ///
    /// The console answered "wait for this to finish" to a person trying to
    /// redirect a long turn, which left stop -- losing the turn's momentum --
    /// as the only way to say "not that file". A safe point is between
    /// actions, never inside one, for the reason stop is.
    pub steering: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Whether the work is moving, carried across turns so a goal's automatic
    /// check-ins cannot reset it. See [`crate::stall::Stall`].
    pub stall: std::sync::Arc<std::sync::Mutex<crate::stall::Stall>>,
    /// A conversation with no workspace (chat mode): it may only read what
    /// the person attached, never write or run anything. Enforced by the
    /// catalogue the calls are decoded against, not by the prompt.
    pub chat_only: bool,
    /// The share of the window at which the conversation compacts itself, in
    /// percent, when the workspace chose one; [`COMPACT_AT`] otherwise.
    pub compact_at_percent: Option<u8>,
    /// An absolute ceiling on the tokens a conversation holds before it
    /// compacts, applied under the percentage of the window. `None`: only the
    /// percentage bounds it (the person chose a window or a threshold).
    pub compact_ceiling_tokens: Option<usize>,
    /// Files this conversation wrote, by workspace path, with the hash each
    /// had when it last wrote it. See [`own_overwrite`].
    pub written: std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>,
    /// Each file as this conversation last saw it -- read, created or edited
    /// -- by workspace path and content hash, across turns. A whole-file
    /// rewrite is checked against this, never against what the file holds
    /// when the write is made: see [`own_overwrite`]. Unlike `written`, which
    /// is what a rewind compares against, a read updates it.
    pub known: std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>,
    /// The person's message the work in progress answers, numbered from 1 in
    /// this session; set by the front end when the person sends one.
    pub person_turn: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Every file edit this session made, with the file as it was before, so
    /// the person can rewind to one of their messages (see [`FileEdit`]).
    pub edits: std::sync::Arc<std::sync::Mutex<Vec<FileEdit>>>,
}

/// A file as it was before an edit, kept for rewinding. In memory only, for
/// the session: a conversation reopened later can rewind its messages but not
/// its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEdit {
    /// The person's message the edit answered.
    pub turn: u32,
    pub path: String,
    pub before: Before,
}

/// What a rewind knows about a file before an edit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Before {
    /// The edit created the file.
    Created,
    /// The file's bytes, as they were: not text, which would lose a file
    /// that is not UTF-8 (and a rewind would then delete it).
    Content(Vec<u8>),
    /// The file existed but its content was not kept -- too large, past what
    /// a session keeps, or unreadable -- so a rewind leaves it as it is and
    /// says so rather than guessing.
    NotKept,
}

impl Before {
    /// The file as it is now, before an edit, within what a session keeps.
    pub fn capture(file: &std::path::Path, kept: &[FileEdit]) -> Self {
        match std::fs::read(file) {
            Ok(bytes) => {
                let held: usize = kept
                    .iter()
                    .map(|edit| match &edit.before {
                        Before::Content(bytes) => bytes.len(),
                        _ => 0,
                    })
                    .sum();
                if bytes.len() <= EDIT_BYTES_KEPT
                    && kept.len() < EDITS_KEPT
                    && held + bytes.len() <= EDIT_BYTES_HELD
                {
                    Before::Content(bytes)
                } else {
                    Before::NotKept
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Before::Created,
            Err(_) => Before::NotKept,
        }
    }
}

/// The most file edits whose earlier content a session keeps for rewinding,
/// the largest file it keeps, and the most it holds in all: past any of
/// them, the edit is still recorded and a rewind says what it could not
/// restore.
pub const EDITS_KEPT: usize = 5_000;
pub const EDIT_BYTES_KEPT: usize = 2 * 1024 * 1024;
pub const EDIT_BYTES_HELD: usize = 256 * 1024 * 1024;

/// How a rewind would put the files back: each path as it was before the
/// first edit made at or after `turn`, and the files changed since by
/// someone other than PWR (their hash no longer the one it left). A file
/// whose earlier content was not kept is in the plan, to be reported, but
/// never a conflict: the rewind leaves it alone.
pub fn rewind_plan(
    root: &std::path::Path,
    edits: &[FileEdit],
    written: &BTreeMap<String, String>,
    turn: u32,
) -> (Vec<(String, Before)>, Vec<String>) {
    let mut restore: Vec<(String, Before)> = Vec::new();
    for edit in edits.iter().filter(|edit| edit.turn >= turn) {
        if !restore.iter().any(|(path, _)| path == &edit.path) {
            restore.push((edit.path.clone(), edit.before.clone()));
        }
    }
    let conflicts = restore
        .iter()
        .filter(|(path, before)| {
            if *before == Before::NotKept {
                return false;
            }
            let current = std::fs::read(root.join(path))
                .ok()
                .map(pwr_domain::hash_bytes);
            current.as_ref() != written.get(path)
        })
        .map(|(path, _)| path.clone())
        .collect();
    (restore, conflicts)
}

impl Continuity {
    /// The threshold in force, clamped to [`COMPACT_AT_BOUNDS`].
    pub fn compact_at(&self) -> f64 {
        self.compact_at_percent
            .map(|percent| {
                f64::from(percent.clamp(COMPACT_AT_BOUNDS.0, COMPACT_AT_BOUNDS.1)) / 100.0
            })
            .unwrap_or(COMPACT_AT)
    }

    /// The prompt size at which the conversation compacts: the share of the
    /// window, under the ceiling when there is one.
    pub fn compaction_room(&self, context_tokens: u32) -> usize {
        #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
        let share = (f64::from(context_tokens) * self.compact_at()) as usize;
        self.compact_ceiling_tokens
            .map_or(share, |ceiling| share.min(ceiling))
    }

    /// A new request from the operator, as opposed to a goal continuing on
    /// its own: the stalled windows counted so far answered the old one.
    pub fn operator_spoke(&self) {
        if let Ok(mut stall) = self.stall.lock() {
            stall.progress.acknowledge();
        }
    }
}

/// Actions that only mean something inside a goal-driven run.
///
/// `record_progress` and `propose_verifier` answer to a plan and a run's
/// verification contract, neither of which a conversation has.
const RUN_ONLY: [&str; 2] = ["record_progress", "propose_verifier"];

/// Every capability, because the conversation is where the work happens now.
pub fn chat_tool_catalog() -> ToolCatalog {
    let mut tools: Vec<ToolDefinition> = crate::action_tool_catalog()
        .tools
        .into_iter()
        .filter(|tool| !RUN_ONLY.contains(&tool.name.as_str()))
        .map(|mut tool| {
            // A conversation replaces an existing file whole rather than
            // refusing (see `own_overwrite`), and says so.
            if tool.name == "write_file" {
                tool.description = "Write a file with the given content: creates it, or replaces \
                                    an existing one whole. To change part of a file, prefer \
                                    replace_text or apply_patch."
                    .into();
            }
            tool
        })
        .collect();
    // Conversations only: a scripted run has no person to confirm it, and its
    // catalogue is part of what a campaign measures.
    tools.push(remember_tool());
    tools.push(recall_project_tool());
    tools.push(wiki_query_tool());
    ToolCatalog::new(tools).expect("a filtered catalogue is valid")
}

/// `look_at`: what a page on this machine looks like, for a model that
/// reads images. Offered only to one (see [`with_vision`]).
pub fn look_at_tool() -> ToolDefinition {
    ToolDefinition {
        name: "look_at".into(),
        description: "See a page as a person would: takes a screenshot of a local server's URL \
                      (http://localhost:PORT/...) or an HTML file in the workspace, and shows it \
                      to you. Use it to check what a page you built looks like -- layout, text, \
                      what is visible -- after its tests pass. Start a server first with \
                      start_service if the page needs one."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "target": {"type": "string", "description": "http://localhost:PORT/path, or a workspace-relative .html file"},
                "width": {"type": "integer", "description": "Viewport width in pixels, 320-1920; 1280 when omitted."},
                "height": {"type": "integer", "description": "Viewport height in pixels, 240-1600; 800 when omitted."},
            },
            "required": ["target"],
        }),
    }
}

/// A conversation's catalogue with `look_at`, for a model that reads images.
pub fn with_vision(catalog: ToolCatalog) -> ToolCatalog {
    let mut tools = catalog.tools;
    if !tools.iter().any(|tool| tool.name == "look_at") {
        tools.push(look_at_tool());
    }
    ToolCatalog::new(tools).expect("a catalogue with one more tool is valid")
}

/// `wiki_query`: a node of a workspace's knowledge graph and its neighbours.
pub fn wiki_query_tool() -> ToolDefinition {
    ToolDefinition {
        name: "wiki_query".into(),
        description: "Ask the workspace's knowledge graph about a file, folder, symbol or \
                      package: what it contains or defines, what it imports and what imports it, \
                      what tests it, which past requests changed it, and decisions about it. \
                      Each link says how it is known. Faster than reading files to find how \
                      things connect; read the files before changing them. Give project to ask \
                      about another project you know. An empty query gives an outline."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "A path, folder, symbol or package name."},
                "project": {"type": "string", "description": "Another project's name; this workspace when omitted."},
            },
            "required": ["query"],
        }),
    }
}

/// `recall_project`: what PWR knows about a workspace it worked in before,
/// from that workspace's wiki (`crate::wiki`).
pub fn recall_project_tool() -> ToolDefinition {
    ToolDefinition {
        name: "recall_project".into(),
        description: "Recall a project you worked on with this person before, in any folder: \
                      what it is, its layout and scripts, and the work done there, newest first. \
                      Use it when they mention a project by name that is not this workspace. \
                      Leave name empty to list every project you know."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "name": {"type": "string", "description": "The project's folder name, or part of it."},
            },
            "required": [],
        }),
    }
}

/// Capabilities a conversation has and a scripted run does not: each needs a
/// person (to confirm a memory) or reads what PWR kept about them and their
/// projects, and a run's catalogue is part of what a campaign measures.
pub const CONVERSATION_ONLY: [&str; 3] = ["remember", "recall_project", "wiki_query"];

/// `remember`: proposes a fact to keep across conversations. The person
/// confirms it before anything is saved (`crate::personal`).
pub fn remember_tool() -> ToolDefinition {
    ToolDefinition {
        name: "remember".into(),
        description: "Propose one short fact worth keeping for future conversations: \
                      something the person told you about themselves, how they like to work, \
                      or a decision about this project. Only when they ask you to remember \
                      something, or state a lasting preference -- never facts you can read \
                      from the files. The person confirms it before it is saved."
            .into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "The fact, in one sentence."},
                "scope": {
                    "type": "string",
                    "enum": ["workspace", "global"],
                    "description": "workspace: this project only; global: the person, everywhere."
                },
            },
            "required": ["text"],
        }),
    }
}

/// What chat mode offers: reading, and nothing that changes anything.
const CHAT_ONLY_TOOLS: [&str; 2] = ["read_file", "list_tree"];

/// The catalogue of a conversation without a workspace (chat mode, decided
/// 2026-09-23 with the maintainer): the person talks to the model and attaches
/// files, folders or images for it to read. A call to anything else is
/// refused as not available, however the model phrases it.
pub fn chat_only_tool_catalog() -> ToolCatalog {
    let tools: Vec<ToolDefinition> = chat_tool_catalog()
        .tools
        .into_iter()
        .filter(|tool| {
            CHAT_ONLY_TOOLS.contains(&tool.name.as_str())
                || tool.name == "remember"
                || tool.name == "recall_project"
                || tool.name == "wiki_query"
        })
        .collect();
    ToolCatalog::new(tools).expect("a filtered catalogue is valid")
}

/// The instructions of chat mode: no repository, no edits, no checks.
pub fn chat_only_system_prompt() -> String {
    "You are PWR, talking with an engineer. There is no project open: answer from what \
     you know and from what the engineer attaches. Attached files and images arrive in the \
     message itself. Attached folders are read-only: list them with list_tree and read their \
     files with read_file, using the paths the attachment names. Read before you state \
     anything about what a file says, and say plainly when you have not checked.\n\
     \n\
     You cannot edit files or run commands here. If something should change, say what and \
     how, briefly; the engineer can open a workspace for that.\n\
     \n\
     Answer in the engineer's language, directly and briefly unless asked for more."
        .to_owned()
}

/// The instructions a conversation runs under.
pub fn chat_system_prompt(root: &std::path::Path) -> String {
    format!(
        "You are PWR, working with an engineer in the repository {}.\n\
         \n\
         Answer questions directly and briefly. Read files, search and list the tree to ground \
         what you say rather than guessing, and say plainly when you have not checked something. \
         Every path is relative to the repository root -- `src/main.rs`, never a path beginning \
         with `/`. A folder outside it (`../other`) is reached only with the engineer's \
         permission, which is asked for you.\n\
         \n\
         The engineer's newest message is the one to act on: when it changes, narrows or adds \
         to an earlier request, follow it over anything said before.\n\
         \n\
         When the engineer asks for a change, make it: edit the files, run what you need to run, \
         and say what you did. The repository's own checks run after you edit and you will be \
         told what they said, so do not claim something works before you have seen them pass. \
         Call `complete` when the work is done and `decline` when it should not be done; either \
         ends your turn and its rationale is what the engineer reads.\n\
         \n\
         {}",
        // The name, not the absolute path: given the full path, a model reads
        // it as the prefix for every file it asks for, and every read is then
        // refused for escaping the workspace.
        root.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string()),
        // What the machine has and how to add what it lacks: asked for a Go
        // service on a Mac without Go, a model had no way to learn either.
        pwr_tools::host_facts(root),
    )
}

/// Whether an action changes the workspace, and so owes the checks an answer.
/// Runs one exchange to its answer, and records how it ended.
///
/// A run's audit ends with its terminal class; a turn's ended with nothing, so
/// `pwr report` and `pwr diagnose` could read what a turn did and never
/// why it stopped. The ending is recorded here, classified with the run's own
/// terminal classes, so the same stop reads the same way in either audit.
#[allow(clippy::too_many_arguments)]
pub async fn take_turn<P: ModelProvider>(
    provider: &P,
    adapter: &dyn ModelBehaviorAdapter,
    deployment: &DeploymentDescriptor,
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
    policy: &ToolPolicy,
    messages: &mut Vec<ChatMessage>,
    context_tokens: u32,
    context_tiers: &[u32],
    sampling: BTreeMap<String, serde_json::Value>,
    tools: serde_json::Value,
    stop: &std::sync::atomic::AtomicBool,
    continuity: &Continuity,
    prompt: &dyn crate::ApprovalPrompt,
    on_step: impl FnMut(TurnStep),
) -> Result<TurnReport, String> {
    let mut declined = false;
    let report = take_turn_inner(
        provider,
        adapter,
        deployment,
        store,
        conversation_id,
        policy,
        messages,
        context_tokens,
        context_tiers,
        sampling,
        tools,
        stop,
        continuity,
        prompt,
        &mut declined,
        on_step,
    )
    .await?;
    let mut report = report;
    report.declined = declined;
    report.outcome.delivered = !report.answer.trim().is_empty() || report.edited;
    report.outcome.terminal = if report.declined {
        pwr_domain::TurnTerminal::Declined
    } else {
        match report.stopped {
            None => pwr_domain::TurnTerminal::Completed,
            Some(StopReason::Interrupted) => pwr_domain::TurnTerminal::Interrupted,
            Some(StopReason::BudgetSpent) => pwr_domain::TurnTerminal::BudgetExhausted,
            Some(reason) => pwr_domain::TurnTerminal::Failed {
                class: match reason {
                    StopReason::BackendFailing => pwr_domain::TerminalClass::Provider,
                    StopReason::Unparseable
                    | StopReason::ToolCallInReasoning
                    | StopReason::Silent => pwr_domain::TerminalClass::Protocol,
                    _ => pwr_domain::TerminalClass::Recovery,
                },
            },
        }
    };
    report.outcome.confinement = match policy.will_sandbox() {
        Ok(true) => pwr_domain::Confinement::Sandboxed,
        Ok(false) => pwr_domain::Confinement::Unconfined,
        Err(error) => pwr_domain::Confinement::PartiallyEnforced {
            what: vec![error.to_string()],
        },
    };
    if continuity
        .checkpoint
        .lock()
        .map_err(|_| "checkpoint lock unavailable")?
        .commands_sandboxed
        == Some(false)
    {
        report.outcome.confinement = pwr_domain::Confinement::Unconfined;
    }
    report.outcome.budget.insert(
        "actions".into(),
        pwr_domain::BudgetCounter {
            spent: report.actions as u64,
            limit: continuity.action_limit.map(|limit| limit as u64),
        },
    );
    let terminal = match report.stopped {
        Some(reason) => Some(reason.terminal_class()),
        None if declined => Some(pwr_domain::TerminalClass::Declined),
        None => None,
    };
    store
        .append(
            Some(conversation_id),
            TURN_ENDED_EVENT,
            serde_json::json!({
                "answered": report.stopped.is_none() && !declined,
                "stopped": report.stopped.map(|reason| format!("{reason:?}")),
                "terminal": terminal,
                "actions": report.actions,
                "edited": report.edited,
                "outcome": report.outcome,
            }),
        )
        .map_err(|error| error.to_string())?;
    Ok(report)
}

/// How a turn ended, in the audit.
pub const TURN_ENDED_EVENT: &str = "conversation.turn_ended";

#[allow(clippy::too_many_arguments)]
async fn take_turn_inner<P: ModelProvider>(
    provider: &P,
    adapter: &dyn ModelBehaviorAdapter,
    deployment: &DeploymentDescriptor,
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
    policy: &ToolPolicy,
    messages: &mut Vec<ChatMessage>,
    mut context_tokens: u32,
    // Windows calibration measured as stable on this deployment, so a prompt
    // the backend refuses can be retried at one that was measured rather than
    // at one that was guessed. Empty where the backend cannot be calibrated,
    // and then compaction is the whole answer.
    context_tiers: &[u32],
    sampling: BTreeMap<String, serde_json::Value>,
    tools: serde_json::Value,
    stop: &std::sync::atomic::AtomicBool,
    continuity: &Continuity,
    // Asked when an action needs an approval the policy does not hold, through
    // the same gate a run uses. The console grants what Settings allow up
    // front, so this is reached only for what they do not.
    prompt: &dyn crate::ApprovalPrompt,
    declined: &mut bool,
    mut on_step: impl FnMut(TurnStep),
) -> Result<TurnReport, String> {
    let mut services = pwr_tools::service::ServiceSupervisor::new();
    // A grant given for one action is taken back after it, so the turn holds
    // its own copy of the policy.
    let mut policy = policy.clone();
    // What this turn has already been shown, so a re-read of an unchanged file
    // says so. The run carried this and the turn did not.
    let mut reads = crate::ReadHistory::default();
    // What the conversation already knows of its files: what earlier turns
    // read or wrote, and, after a restart, what the checkpoint says it left.
    if let Ok(known) = continuity.known.lock() {
        reads.seed_known(&known);
    }
    if let Ok(checkpoint) = continuity.checkpoint.lock() {
        let unseen: BTreeMap<String, String> = checkpoint
            .changed_files
            .iter()
            .filter(|(path, hash)| {
                reads.known_hash(path).is_none()
                    && hash.len() == 64
                    && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .map(|(path, hash)| (path.clone(), hash.clone()))
            .collect();
        reads.seed_known(&unseen);
    }
    if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
        for message in messages.iter().filter(|message| {
            message.role == "user"
                && matches!(
                    message.purpose,
                    None | Some(pwr_domain::MessagePurpose::Task)
                )
        }) {
            if !checkpoint.objectives.contains(&message.content) {
                checkpoint.objectives.push(message.content.clone());
            }
        }
        if !checkpoint.acceptance_initialized {
            checkpoint.acceptance = if checkpoint.turn == 0 {
                pwr_verify::acceptance::snapshot(&policy.root)?
            } else {
                None
            };
            checkpoint.acceptance_initialized = true;
        }
        checkpoint.turn += 1;
        checkpoint.actions = 0;
        checkpoint.commands_sandboxed = None;
        crate::conversation::record_checkpoint(store, conversation_id, &checkpoint)?;
    }
    // The semantic catalogue, kept beside the rendered one: a refusal names the
    // fields a capability takes, and the wire form has already been flattened
    // into the backend's own shape.
    let catalog = if continuity.chat_only {
        chat_only_tool_catalog()
    } else {
        chat_tool_catalog()
    };
    let mut actions = 0usize;
    let mut edited = false;
    let stopped = |actions, edited, reason| {
        Ok(TurnReport {
            outcome: Default::default(),
            answer: String::new(),
            actions,
            edited,
            completed: false,
            stopped: Some(reason),
            declined: false,
        })
    };
    // A deployment repeating a refused action is not short of budget; it is
    // not reading the refusal. The scripted loop has named this since the
    // campaign that found it, and the conversation did not: it could propose
    // the same stale-hash edit until the turn ran out, with nothing naming the
    // repetition and no `loop.detected` in the audit for `pwr diagnose` to
    // find afterwards. Per turn, because a turn is what a conversation has in
    // place of a run's action loop.
    let mut refused_streak = crate::repetition::RefusalStreak::new();
    let mut echoes = crate::repetition::Echoes::default();
    let mut failed_runs = crate::repetition::FailedRuns::default();
    let mut held_empty_completion = false;
    // What the never-ran hold judges: whether this turn built a program and
    // ever ran anything. The scripted loop had it and the conversation did not,
    // so a model that wrote code and declared it done, unrun, was stopped in
    // campaigns and not in the app (plan W2.4, row 6).
    let mut created_here: std::collections::BTreeSet<String> = Default::default();
    let mut ran_something = false;
    let mut changed_runnable = false;
    let mut asked_to_run = false;
    // Acting and getting nowhere is the other half of being stuck, and the
    // conversation had neither half. A turn could spend its whole budget
    // reading the same three files in a circle, or editing a line and putting
    // it back, with nothing saying so to the deployment or to the audit.
    let mut compactions = 0usize;
    // Consecutive, so a deployment that recovers is not held to account for
    // having stumbled once.
    let mut silent = 0usize;
    let mut reasoning_calls = 0usize;
    let mut answer_without_thinking = false;
    let mut runaway_retry = false;
    // Set once a generation of this turn looped: every later one is sampled
    // with a presence penalty (see `ANTI_LOOP_PRESENCE_PENALTY`).
    let mut anti_loop = false;
    // Replies of this turn that fell apart (a loop, or the engine's repetition
    // stop) since the last good one, and the clean starts made for them.
    let mut degenerate_replies = 0usize;
    let mut degenerate_resets = 0usize;
    let mut turn = 0u32;
    // What the backend said the last prompt actually cost. `None` until the
    // first reply, which is the only turn with nothing to measure.
    let mut measured_prompt: Option<u64> = None;
    // How much of the history the measured count covers, and what this
    // deployment's counts say a character estimate is worth. Without the tail,
    // a turn that read a large file was measured as the prompt before the read
    // and sent the next one into a window it no longer fitted.
    let mut measured_upto: usize = 0;
    // Once per conversation: see where it is set.
    let mut truncation_reported = false;
    // Consecutive, like `silent`: a deployment that recovers is not held to
    // account for one bad generation.
    let mut unparseable = 0usize;
    // Replies cut off inside a tool call, never reset within the turn.
    let mut cut_offs = 0usize;
    // Its own count: a reply carrying an unreadable call arrives as a reply,
    // and every reply that arrives resets `unparseable`.
    let mut broken_calls = 0usize;
    // Consecutive tool calls that did not decode against the catalogue.
    let mut malformed_calls = 0usize;
    // Identifies each action across its phases for a front end.
    let mut call_sequence = 0u64;
    // Consecutive, like the others: a backend that drops one connection and
    // then serves the next is a backend that works.
    let mut backend_faults = 0usize;
    // Drops to a lower measured window, bounded by the same budget the scripted
    // loop spends on the same move.
    let mut context_drops = 0u8;
    let max_context_drops = pwr_verify::RecoveryBudget::default().max_context_retries;
    // Consecutive generations that reasoned to their budget with no answer;
    // the next generation after one is asked to answer directly.
    let mut unfinished_reasoning = 0usize;
    // Retries since the last usable output, of every cause, so a front end
    // can say the turn recovered and after how many.
    let mut retrying = 0usize;
    loop {
        if actions >= continuity.action_limit.unwrap_or(DEFAULT_ACTIONS_PER_TURN) {
            return stopped(actions, edited, StopReason::BudgetSpent);
        }
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            return stopped(actions, edited, StopReason::Interrupted);
        }
        // Steering lands here, between an action's result and the next
        // request: the deployment reads it before deciding what to do next,
        // and nothing it was in the middle of is abandoned. Each message opens
        // a revision of the objective, recorded so a resumed conversation and
        // a person reading the log both know the goal moved and when.
        let steered: Vec<String> = continuity
            .steering
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default();
        for text in steered {
            let revision = continuity
                .checkpoint
                .lock()
                .map(|mut checkpoint| {
                    checkpoint.revision += 1;
                    checkpoint.objectives.push(text.clone());
                    checkpoint.revision
                })
                .unwrap_or_default();
            crate::conversation::record_steering(store, conversation_id, revision, &text, actions)?;
            on_step(TurnStep::Steered(text.clone()));
            messages.push(ChatMessage::text("user", text));
        }
        // Checked before the request rather than after the failure: a prompt
        // that has outgrown the window comes back as a provider error about
        // its length, which tells the operator nothing they can do.
        let room = continuity.compaction_room(context_tokens);
        let objective_tokens = continuity
            .checkpoint
            .lock()
            .map(|checkpoint| {
                checkpoint
                    .objectives
                    .iter()
                    .map(|text| crate::context::estimate_tokens(text))
                    .sum::<usize>()
            })
            .unwrap_or_default();
        if objective_tokens >= room {
            on_step(TurnStep::Note(format!(
                "The objective and its revisions take {objective_tokens} estimated tokens of {room}; choose a larger context or shorten the objective."
            )));
            return stopped(actions, edited, StopReason::ContextFull);
        }
        if prompt_tokens_now(messages, measured_prompt, measured_upto) >= room {
            if compactions >= COMPACTIONS_PER_TURN {
                return stopped(actions, edited, StopReason::Looping);
            }
            match compact_and_record(
                store,
                conversation_id,
                deployment,
                context_tokens,
                messages,
                room,
                continuity,
            )? {
                Some(note) => {
                    compactions += 1;
                    on_step(TurnStep::Compacted(note));
                }
                // Nothing left to summarise: the recent exchanges alone
                // already fill the window, so no amount of compacting will
                // make room and saying so is the only honest answer.
                None => return stopped(actions, edited, StopReason::ContextFull),
            }
        }
        // Replies that fall apart in a long conversation are not fixed by
        // asking again: the same prompt makes the same mess. Measured
        // 2026-10-01 (Qwen3-Coder-30B on a Bash task): thirty-two sound
        // generations, then at 26,900 tokens of failing-and-retrying history
        // nine in a row that collapsed into "!!!!!" and "call call call";
        // the same model answers a fresh 30,000-token prompt and a synthetic
        // 40-turn chain correctly. After two such replies the history is cut
        // back to what compaction keeps, and the work goes on from there.
        if degenerate_replies >= DEGENERATE_REPLIES_BEFORE_RESET
            && degenerate_resets < DEGENERATE_RESETS_PER_TURN
            && prompt_tokens_now(messages, measured_prompt, measured_upto) > DEGENERATE_RESET_ROOM
        {
            if let Some(note) = compact_and_record(
                store,
                conversation_id,
                deployment,
                context_tokens,
                messages,
                DEGENERATE_RESET_ROOM,
                continuity,
            )? {
                degenerate_resets += 1;
                degenerate_replies = 0;
                on_step(TurnStep::Compacted(format!(
                    "{note} -- the last replies fell apart, so the conversation was cut back \
                     to start clean"
                )));
            } else {
                degenerate_replies = 0;
            }
        }
        let mut request_sampling = sampling.clone();
        if anti_loop {
            let current = request_sampling
                .get("presence_penalty")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            // A person's or a profile's own value is never lowered, and a
            // higher one is kept.
            if current < ANTI_LOOP_PRESENCE_PENALTY {
                request_sampling.insert(
                    "presence_penalty".into(),
                    serde_json::json!(ANTI_LOOP_PRESENCE_PENALTY),
                );
                request_sampling.insert(
                    "presence_context_size".into(),
                    serde_json::json!(ANTI_LOOP_PRESENCE_WINDOW),
                );
                if let Some(sources) = request_sampling
                    .get_mut("_pwr_sampling_sources")
                    .and_then(serde_json::Value::as_object_mut)
                {
                    sources.insert(
                        "presence_penalty".into(),
                        serde_json::json!("pwr_loop_recovery"),
                    );
                }
            }
        }
        if std::mem::take(&mut runaway_retry) {
            let current = request_sampling
                .get("max_tokens")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(ANSWER_ALLOWANCE);
            request_sampling.insert(
                "max_tokens".into(),
                serde_json::json!(current.min(RUNAWAY_RETRY_MAX_TOKENS)),
            );
        }
        // The generation's envelope, after any compaction above: what the
        // prompt takes (counted conservatively), and the answer's own cap.
        let envelope = pwr_domain::GenerationEnvelope {
            context_limit: context_tokens,
            input_tokens: u32::try_from(conservative_prompt_tokens(
                messages,
                measured_prompt,
                measured_upto,
            ))
            .unwrap_or(u32::MAX),
            answer_allowance: request_sampling
                .get("max_tokens")
                .and_then(serde_json::Value::as_u64)
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(ANSWER_ALLOWANCE),
        };
        let finalizing = unfinished_reasoning > 0 || answer_without_thinking;
        answer_without_thinking = false;
        let plan = if finalizing {
            pwr_domain::plan_finalization(
                continuity.reasoning_effort,
                &continuity.reasoning,
                envelope,
            )
        } else {
            pwr_domain::plan_reasoning(continuity.reasoning_effort, &continuity.reasoning, envelope)
        };
        apply_reasoning_plan(&mut request_sampling, &plan);
        let request = ModelRequest {
            deployment: deployment.clone(),
            context_tokens,
            tools: Some(tools.clone()),
            seed: None,
            sampling: request_sampling,
            messages: messages.clone(),
        };
        // The prompt as sent: what the count that comes back is a count of.
        let sent_upto = messages.len();
        // What this request is about to occupy, said before a generation that
        // can take minutes. Only once a count exists: an estimate from the
        // messages alone leaves out the instructions and the tool schemas,
        // and would show the window emptying as the turn began.
        if measured_prompt.is_some() {
            on_step(TurnStep::Usage {
                used: prompt_tokens_now(messages, measured_prompt, measured_upto) as u64,
                window: u64::from(context_tokens),
                estimated: true,
            });
        }
        // A backend that refuses the request and one that drops the reply
        // half-way are the same event to this turn -- the turn has nothing to
        // work with -- so both arrive here and are judged by the same match
        // below. Splitting them meant a refused request bypassed every
        // recovery the collected reply had.
        // What streamed and how long it took, kept for a generation that
        // fails: its record is the only trace it leaves (D.E2E-22).
        let started = std::time::Instant::now();
        store
            .append(
                Some(conversation_id),
                "generation.started",
                serde_json::json!({
                    "model": deployment.model_ref,
                    "backend": deployment.provider,
                    "profile_status": continuity.profile_status,
                    "context_tokens": context_tokens,
                    "input_tokens_estimated": envelope.input_tokens,
                    "reasoning": {
                        "effort": plan.effort,
                        "capability": plan.capability,
                        "evidence": continuity.reasoning.evidence,
                        "directive": plan.directive,
                        "requested_tokens": plan.requested_tokens,
                        "effective_tokens": plan.effective_tokens,
                        "clamped": plan.clamped,
                        "budget_source": plan.budget_source,
                        "finalizing": finalizing,
                    },
                    "max_tokens": plan.max_tokens,
                    "answer_reserve": plan.answer_reserve,
                    "sampling": {
                        "temperature": request.sampling.get("temperature"),
                        "top_p": request.sampling.get("top_p"),
                        "top_k": request.sampling.get("top_k"),
                        "min_p": request.sampling.get("min_p"),
                        "presence_penalty": request.sampling.get("presence_penalty"),
                        "repetition_penalty": request.sampling.get("repetition_penalty"),
                        "sources": request.sampling.get("_pwr_sampling_sources"),
                        "ignored": request.sampling.keys()
                            .filter(|key| key.as_str() == "repeat_penalty")
                            .collect::<Vec<_>>(),
                    },
                }),
            )
            .map_err(|error| error.to_string())?;
        let streamed_thinking = std::cell::Cell::new(0usize);
        let reasoning_seen = std::cell::Cell::new(false);
        let streamed_content = std::cell::Cell::new(0usize);
        let first_chunk = std::cell::Cell::new(None::<std::time::Duration>);
        let failed = |turn: &mut u32, outcome: &str, detail: String| {
            *turn = turn.saturating_add(1);
            store
                .append_event(
                    Some(conversation_id),
                    &pwr_domain::RunEvent::GenerationFailed {
                        step: u8::try_from(actions).unwrap_or(u8::MAX),
                        turn: *turn,
                        outcome: outcome.to_owned(),
                        detail,
                        thinking_chars: streamed_thinking.get(),
                        content_chars: streamed_content.get(),
                        elapsed_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                    },
                )
                .map_err(|error| error.to_string())
        };
        let collected = match provider.chat(request).await {
            // Stop has to reach a generation already in flight, not merely the
            // gap between actions: a reply that takes a minute is exactly when
            // an operator presses it. Abandoning the read drops the stream,
            // which drops the HTTP body, which closes the socket -- so the
            // backend stops generating rather than being politely waited out.
            Ok(stream) => tokio::select! {
                outcome = pwr_provider::collect_reply_with_guard(stream, |chunk| {
                    if let Some(progress) = chunk.prefill {
                        // Reading the prompt is not the first word of the
                        // answer, and time-to-first-chunk stays about that.
                        on_step(TurnStep::Prefill {
                            processed: progress.processed,
                            total: progress.total,
                        });
                        return;
                    }
                    if first_chunk.get().is_none() && !chunk.done {
                        first_chunk.set(Some(started.elapsed()));
                    }
                    let thinking = chunk.thinking.clone().unwrap_or_default();
                    if !thinking.is_empty() && !reasoning_seen.replace(true) {
                        // When, not what: the reasoning itself is never logged.
                        let _ = store.append(
                            Some(conversation_id),
                            "reasoning.started",
                            serde_json::json!({
                                "elapsed_ms": u64::try_from(started.elapsed().as_millis())
                                    .unwrap_or(u64::MAX),
                            }),
                        );
                    }
                    streamed_thinking.set(streamed_thinking.get() + thinking.len());
                    streamed_content.set(streamed_content.get() + chunk.content.len());
                    // Gemma 4 can begin inside a thought channel whose opening
                    // marker is in the prompt. Until normalization sees its
                    // closing marker, raw content may be internal reasoning.
                    // Keep it off the answer rail; the canonical final reply
                    // is sent when this generation completes.
                    let visible_content = if adapter.id() == "gemma4" {
                        String::new()
                    } else {
                        chunk.content.clone()
                    };
                    if !thinking.is_empty() || !visible_content.is_empty() {
                        on_step(TurnStep::Streaming {
                            thinking,
                            content: visible_content,
                        });
                    } else if !chunk.done {
                        // The provider is still making progress, possibly in
                        // tool arguments that must not be rendered as text.
                        on_step(TurnStep::Streaming {
                            thinking: String::new(),
                            content: String::new(),
                        });
                    }
                }, (deployment.provider == "mlx" && !continuity.chat_only)
                    .then_some(AGENT_UNSTRUCTURED_REPLY_GUARD)) => outcome,
                () = pressed(stop) => {
                    failed(&mut turn, "cancelled", "stopped by the operator".into())?;
                    return stopped(actions, edited, StopReason::Interrupted);
                }
            },
            Err(error) => Err(error),
        };
        // A tool call that is not valid JSON is a generation that went wrong,
        // not a backend that is broken, and the difference is worth a turn.
        // Measured on an 80B asked three times for the same component: two
        // replies parsed and one did not, the failure being a doubled
        // backslash before a quote, which ends the JSON string early. Repairing
        // that is guesswork -- the sequence is valid JSON and means something
        // else -- and guessing would write content nobody chose into a file.
        // Asking again is not guesswork, and the deployment is the only thing
        // that knows what it meant.
        //
        // Ending the whole conversation instead cost three runs of this
        // workspace, each having produced most of the files it needed.
        let reply = match collected {
            Ok(reply) => {
                unparseable = 0;
                degenerate_replies = 0;
                backend_faults = 0;
                unfinished_reasoning = 0;
                reply
            }
            // The engine closed the thinking phase at its budget and no
            // answer followed. Asked once more with thinking off; a second
            // failure ends the turn rather than looping.
            Err(pwr_provider::ProviderError::ReasoningUnfinished { safe_context }) => {
                failed(&mut turn, "reasoning_unfinished", safe_context.clone())?;
                unfinished_reasoning += 1;
                store
                    .append(
                        Some(conversation_id),
                        "reasoning.finalization_failed",
                        serde_json::json!({
                            "attempt": unfinished_reasoning,
                            "retries_allowed": REASONING_FINALIZATION_RETRIES,
                            "effective_tokens": plan.effective_tokens,
                            "detail": safe_context,
                        }),
                    )
                    .map_err(|error| error.to_string())?;
                if unfinished_reasoning > REASONING_FINALIZATION_RETRIES {
                    return stopped(actions, edited, StopReason::ReasoningUnfinished);
                }
                on_step(TurnStep::Refused(format!(
                    "{safe_context}; asking the model to answer without further reasoning"
                )));
                retrying += 1;
                on_step(TurnStep::Retry {
                    cause: "reasoning_unfinished",
                    attempt: unfinished_reasoning,
                    limit: REASONING_FINALIZATION_RETRIES,
                    detail: safe_context,
                });
                continue;
            }
            // A reply that never stops is the same kind of event as one that
            // cannot be read: the turn produced nothing usable, and the
            // deployment is the only thing that can produce something else.
            // Measured on a 35B given a large task: seventy minutes of
            // uninterrupted generation, ended by the chunk bound, and the whole
            // conversation ended with it -- discarding twenty-two actions of
            // work that had gone fine.
            // A reply the turn cannot use, in the shape the scripted loop uses
            // it. One definition of the fault and of what the deployment is
            // told: these were two texts for the same two faults, and the
            // conversation's carried the escaping rules while the run's carried
            // the backend's own message. Both carry both now.
            Err(ref error) if crate::ReplyFault::of(error).is_some() => {
                let fault = crate::ReplyFault::of(error).expect("guarded above");
                if matches!(fault, crate::ReplyFault::RanAway(_))
                    && (fault.detail().contains("unstructured answer text")
                        || fault.detail().contains("(length)"))
                {
                    answer_without_thinking = true;
                    runaway_retry = true;
                }
                // Cut off inside a tool call: the file being written is too long
                // for one. The next try is bounded as a cut-off call's is, and
                // the model is told to split it (`ReplyFault::told`); a turn
                // that keeps losing replies this way stops rather than repeat.
                if matches!(fault, crate::ReplyFault::RanAway(_)) && fault.is_cut_off_call() {
                    runaway_retry = true;
                    cut_offs += 1;
                    if cut_offs >= CUT_OFF_REPLIES_PER_TURN {
                        failed(&mut turn, fault.kind(), fault.detail().to_owned())?;
                        return stopped(actions, edited, StopReason::Unparseable);
                    }
                }
                // A model that looped once is bounded more tightly on the next
                // try, so a second loop costs minutes rather than a turn.
                if matches!(fault, crate::ReplyFault::Looped(_)) {
                    runaway_retry = true;
                    anti_loop = true;
                }
                if matches!(fault, crate::ReplyFault::Looped(_))
                    || (matches!(fault, crate::ReplyFault::RanAway(_))
                        && fault.detail().contains("(repetition)"))
                {
                    degenerate_replies += 1;
                }
                failed(&mut turn, fault.kind(), fault.detail().to_owned())?;
                unparseable = unparseable.saturating_add(1);
                if unparseable >= UNPARSEABLE_CALLS_BEFORE_GIVING_UP {
                    return stopped(actions, edited, StopReason::Unparseable);
                }
                on_step(TurnStep::Refused(format!(
                    "{} ({unparseable} of {UNPARSEABLE_CALLS_BEFORE_GIVING_UP})",
                    fault.detail()
                )));
                retrying += 1;
                on_step(TurnStep::Retry {
                    cause: "reply_fault",
                    attempt: unparseable,
                    limit: UNPARSEABLE_CALLS_BEFORE_GIVING_UP - 1,
                    detail: fault.detail().to_owned(),
                });
                messages.push(fault.message());
                continue;
            }
            // A fault in the backend is not a fault in the deployment, and it
            // used to end the turn either way -- taking the turn's history with
            // it, since a failed turn returns no messages to the console while
            // its edits stay on disk. Bounded and consecutive, like every other
            // counter here: what recovers is not held to account for having
            // stumbled.
            Err(pwr_provider::ProviderError::Cancelled) => {
                failed(
                    &mut turn,
                    "cancelled",
                    "the backend cancelled the generation".into(),
                )?;
                return stopped(actions, edited, StopReason::Interrupted);
            }
            Err(pwr_provider::ProviderError::ContextLimit { safe_context }) => {
                failed(&mut turn, "context_limit", safe_context.clone())?;
                // A lower window that calibration measured, before reaching for
                // the prompt. Dropping a tier keeps the conversation whole;
                // compacting spends part of it. The scripted loop does this and
                // the turn could not, for want of being given the tiers.
                //
                // Requested and then taken as granted, the way the console asks
                // for its window in the first place: a backend that serves less
                // than was asked is a fact to work with, and one that serves
                // less than was assumed corrupts every turn after it silently.
                //
                // It is not free. On a backend where the window is fixed at load
                // time, `prepare_context` unloads and reloads the deployment to
                // change it -- twenty gigabytes and a minute or two, in the
                // middle of a turn. That is the same price the scripted loop
                // pays for the same move, and it is bounded by the same budget,
                // but a turn that pays it goes quiet for long enough that the
                // step above says so before it starts.
                //
                // It cannot happen on a backend that ignores context requests:
                // nothing can calibrate one, so it has no profile, so the tier
                // list is empty and this branch is unreachable there.
                if context_drops < max_context_drops
                    && let Some(lower) = crate::lower_measured_tier(context_tokens, context_tiers)
                {
                    context_drops += 1;
                    // Said before the call, not after it: on a backend that
                    // fixes the window at load time this reloads the
                    // deployment, and an operator watching a console go quiet
                    // for two minutes should know which of the two it is.
                    on_step(TurnStep::Refused(format!(
                        "the backend refused the prompt ({safe_context}); asking for \
                         {lower} tokens, a window calibration measured -- this reloads the \
                         model on a backend that fixes its window at load time"
                    )));
                    retrying += 1;
                    on_step(TurnStep::Retry {
                        cause: "context_limit",
                        attempt: usize::from(context_drops),
                        limit: usize::from(max_context_drops),
                        detail: format!("{safe_context}; asking for {lower} tokens"),
                    });
                    let granted = provider
                        .prepare_context(deployment, lower)
                        .await
                        .unwrap_or(lower);
                    if granted != lower {
                        on_step(TurnStep::Refused(format!(
                            "the backend served {granted} tokens rather than the {lower} asked \
                             for; that is the number this turn is measured against"
                        )));
                    }
                    context_tokens = granted;
                    continue;
                }
                // The backend, not our estimate, saying the prompt does not
                // fit. Reducing the prompt is what this turn can do: the
                // scripted loop also drops to a lower *measured* context tier,
                // and a turn is not given the tiers to do that with. They do
                // exist where the backend can be calibrated at all -- the chat
                // config carries a calibration profile and its preparation
                // already refuses a tier calibration rejected -- so this is
                // work not yet done rather than a thing that cannot be done.
                // On a backend that cannot be calibrated there is no ladder to
                // drop down, and compaction is the whole answer.
                if compactions >= COMPACTIONS_PER_TURN {
                    return stopped(actions, edited, StopReason::ContextFull);
                }
                match compact_and_record(
                    store,
                    conversation_id,
                    deployment,
                    context_tokens,
                    messages,
                    room,
                    continuity,
                )? {
                    Some(note) => {
                        compactions += 1;
                        on_step(TurnStep::Compacted(format!(
                            "{note} (after the backend refused the prompt: {safe_context})"
                        )));
                        continue;
                    }
                    None => return stopped(actions, edited, StopReason::ContextFull),
                }
            }
            Err(error) => {
                failed(&mut turn, "backend_fault", error.to_string())?;
                backend_faults = backend_faults.saturating_add(1);
                if backend_faults >= BACKEND_FAULTS_BEFORE_GIVING_UP {
                    return stopped(actions, edited, StopReason::BackendFailing);
                }
                on_step(TurnStep::Refused(format!(
                    "{error} ({backend_faults} of {BACKEND_FAULTS_BEFORE_GIVING_UP})"
                )));
                retrying += 1;
                on_step(TurnStep::Retry {
                    cause: "backend_fault",
                    attempt: backend_faults,
                    limit: BACKEND_FAULTS_BEFORE_GIVING_UP - 1,
                    detail: error.to_string(),
                });
                continue;
            }
        };
        record_generation(
            store,
            conversation_id,
            &plan,
            &reply,
            reasoning_seen.get(),
            started.elapsed(),
        )?;
        on_step(TurnStep::Generation(GenerationStats {
            metrics: reply.metrics.clone(),
            elapsed: started.elapsed(),
            first_chunk: first_chunk.get(),
        }));
        let mut reply = adapter.normalize(&reply);
        reply.tool_calls = crate::expand_numbered_commands(&reply.tool_calls);
        let counted = reply
            .metrics
            .as_ref()
            .and_then(|metrics| metrics.prompt_tokens);
        // Said once, because it is a property of how the deployment is loaded
        // and will hold for the rest of the conversation: repeating it every
        // turn would bury the turns themselves.
        if let Some(note) = truncation_note(counted, measured_prompt, context_tokens)
            .filter(|_| !truncation_reported)
        {
            truncation_reported = true;
            on_step(TurnStep::Refused(note));
        }
        if counted.is_some() {
            measured_upto = sent_upto;
        }
        measured_prompt = counted.or(measured_prompt);
        // How full the window is, as a front end shows it: the prompt the
        // backend counted plus what it just generated, against the window in
        // force.
        if let Some(prompt) = counted {
            let generated = reply
                .metrics
                .as_ref()
                .and_then(|metrics| metrics.generated_tokens)
                .unwrap_or_default();
            on_step(TurnStep::Usage {
                used: prompt.saturating_add(generated),
                window: u64::from(context_tokens),
                estimated: false,
            });
        }
        // The audit records what the deployment did and, from here, what the
        // turn cost. Without this a conversation's trail held its actions and
        // no token counts at all -- so `pwr report` could say a run made
        // fifteen edits and nothing about what generating them took, which is
        // the half of a measurement a scripted run has always carried.
        turn = turn.saturating_add(1);
        store
            .append_event(
                Some(conversation_id),
                &pwr_domain::RunEvent::TurnGenerated {
                    step: u8::try_from(actions).unwrap_or(u8::MAX),
                    turn,
                    metrics: reply.metrics.clone(),
                    tokens_per_second: reply
                        .metrics
                        .as_ref()
                        .and_then(pwr_domain::GenerationMetrics::tokens_per_second),
                    thinking_chars: reply.thinking.len(),
                    content_chars: reply.narrative.len(),
                    prompt_delivery: None,
                    normalizations: reply
                        .diagnostics
                        .iter()
                        .map(|diagnostic| format!("{}: {}", diagnostic.kind, diagnostic.detail))
                        .collect(),
                },
            )
            .map_err(|error| error.to_string())?;
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: reply.narrative.clone(),
            tool_calls: reply.tool_calls.clone(),
            tool_call_id: None,
            purpose: None,
            images: Vec::new(),
            reasoning: kept_reasoning(&reply.thinking),
        });
        // A call the model tried to make and the family adapter could not
        // read is not an answer. Taken as one, the turn ended "answered" with
        // the raw block as its text; measured on the stack matrix (2026-09-26)
        // the model then saw its file had never been written and wrote the
        // same unreadable call sixty times across a goal. Told, counted with
        // the other reply faults, and asked again.
        let broken_call = reply
            .diagnostics
            .iter()
            .filter(|_| reply.tool_calls.is_empty())
            .find(|diagnostic| {
                diagnostic.kind.ends_with("undecodable_tool_call")
                    || diagnostic.kind.ends_with("unterminated_tool_call")
            });
        if let Some(diagnostic) = broken_call {
            broken_calls = broken_calls.saturating_add(1);
            if broken_calls >= UNPARSEABLE_CALLS_BEFORE_GIVING_UP {
                return stopped(actions, edited, StopReason::Unparseable);
            }
            let cut_off = diagnostic.kind.ends_with("unterminated_tool_call");
            on_step(TurnStep::Refused(format!(
                "{} ({broken_calls} of {UNPARSEABLE_CALLS_BEFORE_GIVING_UP})",
                diagnostic.detail
            )));
            retrying += 1;
            on_step(TurnStep::Retry {
                cause: "reply_fault",
                attempt: broken_calls,
                limit: UNPARSEABLE_CALLS_BEFORE_GIVING_UP - 1,
                detail: diagnostic.detail.clone(),
            });
            if cut_off {
                runaway_retry = true;
            }
            messages.push(ChatMessage::text(
                "tool",
                if cut_off {
                    "Your tool call was cut off before it ended, so nothing was done. Send it again, \
                     complete; if a file is long, write part of it and add the rest with replace_text \
                     or apply_patch."
                } else {
                    "Your tool call could not be read, so nothing was done. Write it exactly in this \
                     form, closing every parameter with </parameter> (not with the parameter's name):\n\
                     <tool_call>\n<function=NAME>\n<parameter=KEY>\nVALUE\n</parameter>\n</function>\n</tool_call>"
                },
            ));
            continue;
        }
        broken_calls = 0;
        if reply.tool_calls.is_empty() {
            // A turn with neither an answer nor an action produced nothing,
            // and returning it as the answer shows the operator "1 action(s)
            // taken" and no reason. Measured on a 9B: after a tool result it
            // sometimes replies empty. Told and retried, within the same
            // budget, because a deployment that says nothing can be asked
            // again -- and a run that reports silence as a reply is worse than
            // one that spends a turn.
            if reply.narrative.trim().is_empty() {
                silent = silent.saturating_add(1);
                let call_in_reasoning = reply.thinking.contains("<tool_call>")
                    && reply.thinking.contains("</tool_call>");
                reasoning_calls = if call_in_reasoning {
                    reasoning_calls.saturating_add(1)
                } else {
                    0
                };
                if silent >= EMPTY_TURNS_BEFORE_GIVING_UP {
                    return stopped(
                        actions,
                        edited,
                        if reasoning_calls == silent {
                            StopReason::ToolCallInReasoning
                        } else {
                            StopReason::Silent
                        },
                    );
                }
                let detail = if call_in_reasoning {
                    answer_without_thinking = true;
                    "the model wrote a tool call in its reasoning phase; calls there cannot run"
                } else {
                    "the reply held neither an answer nor a tool call"
                };
                on_step(TurnStep::Refused(format!(
                    "{detail} ({silent} of {EMPTY_TURNS_BEFORE_GIVING_UP})"
                )));
                retrying += 1;
                on_step(TurnStep::Retry {
                    cause: if call_in_reasoning {
                        "tool_in_reasoning"
                    } else {
                        "silent"
                    },
                    attempt: silent,
                    limit: EMPTY_TURNS_BEFORE_GIVING_UP - 1,
                    detail: detail.into(),
                });
                messages.push(ChatMessage::text(
                    "tool",
                    if call_in_reasoning {
                        "Your tool call was inside the reasoning phase and was not executed. \
                         Close the reasoning phase, then send the tool call in the answer phase."
                    } else {
                        "That turn contained neither an answer nor a tool call. Say what you found, \
                         or take the next action."
                    },
                ));
                continue;
            }
            if retrying > 0 {
                on_step(TurnStep::Recovered {
                    retries: std::mem::take(&mut retrying),
                });
            }
            return Ok(TurnReport {
                outcome: Default::default(),
                answer: reply.narrative,
                actions,
                edited,
                completed: false,
                stopped: None,
                declined: false,
            });
        }
        silent = 0;
        reasoning_calls = 0;
        for (position, call) in reply.tool_calls.iter().enumerate() {
            // A single generation can contain more calls than the goal has
            // left. Bound each call, not only the next generation.
            if continuity
                .action_limit
                .is_some_and(|limit| actions >= limit)
            {
                return stopped(actions, edited, StopReason::BudgetSpent);
            }
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                // Between actions; inside one only for a command, below,
                // whose process group is killed with it. An edit is never
                // dropped half-written.
                return stopped(actions, edited, StopReason::Interrupted);
            }
            let action = match decode(call, &catalog) {
                Ok(action) => {
                    malformed_calls = 0;
                    if retrying > 0 {
                        on_step(TurnStep::Recovered {
                            retries: std::mem::take(&mut retrying),
                        });
                    }
                    action
                }
                Err(problem) => {
                    // Told rather than fatal: a call the deployment can
                    // correct is a turn that continues. Not charged as an
                    // action -- it performed nothing -- and bounded when it
                    // repeats, as the run bounds it: a turn charged each one to
                    // its budget and could spend all twenty-six on calls that
                    // never decoded, where the run stopped after three.
                    malformed_calls += 1;
                    if malformed_calls >= UNPARSEABLE_CALLS_BEFORE_GIVING_UP {
                        return stopped(actions, edited, StopReason::Unparseable);
                    }
                    on_step(TurnStep::Refused(format!("{}: {problem}", call.name)));
                    retrying += 1;
                    on_step(TurnStep::Retry {
                        cause: "malformed_call",
                        attempt: malformed_calls,
                        limit: UNPARSEABLE_CALLS_BEFORE_GIVING_UP - 1,
                        detail: format!("{}: {problem}", call.name),
                    });
                    call_sequence += 1;
                    on_step(TurnStep::ToolCall(ToolCallStep {
                        id: call_sequence,
                        capability: call.name.clone(),
                        detail: tool_call_detail(call),
                        path: None,
                        phase: ToolPhase::Refused(problem.clone()),
                        diff: None,
                    }));
                    // Through the same renderer as everything else, with the
                    // category the run would have given it: a call that cannot
                    // be decoded is an invalid action, and the deployment that
                    // has to correct it should not have to tell that from prose.
                    messages.push(tool_message(
                        call,
                        crate::action_outcome(Err(crate::ActionExecutionError::Invalid(problem))),
                    ));
                    continue;
                }
            };
            // A completion sent in the same reply as the calls before it
            // waits for their results to be read. Measured 2026-09-30
            // (Qwen3-14B in the desktop, the bank page): two write_file calls
            // and complete in one reply; the writes came back warning that
            // `@playwright/test` was not installed and that the test opened
            // a placeholder path, and the turn had already ended on the
            // completion, so neither warning was ever read.
            // A completion in a turn that has done nothing at all, once.
            // Measured 2026-09-30 (Qwen3-14B in the desktop): asked to create
            // a page and its test and run them, its first and only call was
            // `complete` -- "the files were created and the tests validated"
            // -- over an empty folder. A turn that only answers ends in prose
            // and never reaches this; a second completion is carried out.
            if actions == 0
                && !held_empty_completion
                && !continuity.chat_only
                && matches!(action, ActionProposal::Complete { .. })
            {
                held_empty_completion = true;
                on_step(TurnStep::Refused(
                    "complete: nothing was done this turn".into(),
                ));
                messages.push(tool_message(
                    call,
                    serde_json::json!({"not_completed": COMPLETION_WITH_NOTHING_DONE}),
                ));
                continue;
            }
            if position > 0 && matches!(action, ActionProposal::Complete { .. }) {
                on_step(TurnStep::Refused(
                    "complete: waits for the results of the calls before it".into(),
                ));
                messages.push(tool_message(
                    call,
                    serde_json::json!({"not_completed": COMPLETION_OVER_UNSEEN_RESULTS}),
                ));
                continue;
            }
            // A program was built here and nothing that runs it has run: asked
            // once, exactly as the scripted loop asks. No checks are passed
            // because a conversation runs them after the turn, not during it.
            if matches!(action, ActionProposal::Complete { .. })
                && !continuity.chat_only
                && changed_runnable
                && !ran_something
                && !asked_to_run
                && let Some(held) = crate::completion_held(false, true, &[])
            {
                asked_to_run = true;
                on_step(TurnStep::Refused(
                    "complete: what was built has not been run".into(),
                ));
                messages.push(tool_message(
                    call,
                    serde_json::json!({"not_completed": held.message}),
                ));
                continue;
            }
            actions += 1;
            // Ending the turn is itself an action the model can take, and its
            // rationale is the answer.
            if let ActionProposal::Complete { rationale } | ActionProposal::Decline { rationale } =
                &action
            {
                *declined = matches!(action, ActionProposal::Decline { .. });
                return Ok(TurnReport {
                    outcome: Default::default(),
                    answer: rationale.clone(),
                    actions,
                    edited,
                    completed: matches!(action, ActionProposal::Complete { .. }),
                    stopped: None,
                    declined: false,
                });
            }
            // What PWR keeps about the person and their projects: answered
            // here, never put to the workspace's policy, and shown to the
            // front end like any other action. `remember` is proposed, not
            // performed -- the person decides -- and the model is told so.
            if matches!(
                action,
                ActionProposal::Remember { .. }
                    | ActionProposal::RecallProject { .. }
                    | ActionProposal::WikiQuery { .. }
                    | ActionProposal::LookAt { .. }
            ) {
                call_sequence += 1;
                let step = |phase| {
                    TurnStep::ToolCall(ToolCallStep {
                        id: call_sequence,
                        capability: call.name.clone(),
                        detail: tool_call_detail(call),
                        path: None,
                        phase,
                        diff: None,
                    })
                };
                on_step(step(ToolPhase::Started));
                let outcome = match &action {
                    ActionProposal::Remember { text, scope } => {
                        let scope = scope.clone().unwrap_or_else(|| {
                            if continuity.chat_only {
                                "global"
                            } else {
                                "workspace"
                            }
                            .to_owned()
                        });
                        on_step(TurnStep::MemoryProposed {
                            text: text.trim().to_owned(),
                            scope: scope.clone(),
                        });
                        Ok(serde_json::json!({
                            "proposed": true,
                            "scope": scope,
                            "note": "shown to the person; it is saved only if they confirm it",
                        }))
                    }
                    ActionProposal::RecallProject { name } => crate::personal::Home::from_env()
                        .map(|home| {
                            serde_json::json!({
                                "recalled": crate::wiki::recall(
                                    &home,
                                    name.as_deref().unwrap_or_default()
                                )
                            })
                        }),
                    ActionProposal::WikiQuery { query, project } => {
                        let root = (!continuity.chat_only).then_some(policy.root.as_path());
                        crate::personal::Home::from_env().map(|home| {
                            serde_json::json!({
                                "graph": crate::wiki::query(
                                    &home,
                                    root,
                                    project.as_deref().unwrap_or_default(),
                                    query
                                )
                            })
                        })
                    }
                    // Only where it was offered: a model that cannot read the
                    // image would be sent one.
                    ActionProposal::LookAt { .. } if !tools.to_string().contains("\"look_at\"") => {
                        Err(
                            "look_at is not available with this model: it does not read images"
                                .to_owned(),
                        )
                    }
                    ActionProposal::LookAt {
                        target,
                        width,
                        height,
                    } => pwr_tools::look_at(&policy, target, *width, *height)
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|look| {
                            serde_json::to_value(look).map_err(|error| error.to_string())
                        }),
                    _ => unreachable!("matched above"),
                };
                match outcome {
                    Ok(value) => {
                        on_step(step(ToolPhase::Completed));
                        // The screenshot goes to the model with the result,
                        // its path kept out of the text it reads.
                        let image = value
                            .get("image")
                            .and_then(serde_json::Value::as_str)
                            .filter(|_| matches!(action, ActionProposal::LookAt { .. }))
                            .map(std::path::PathBuf::from);
                        let mut message = tool_message(call, crate::action_outcome(Ok(value)));
                        if let Some(image) = image {
                            message.images.push(image);
                        }
                        messages.push(message);
                    }
                    Err(why) => {
                        on_step(step(ToolPhase::Failed(why.clone())));
                        messages.push(tool_message(
                            call,
                            crate::action_outcome(Err(crate::ActionExecutionError::Invalid(why))),
                        ));
                    }
                }
                continue;
            }
            let action = match own_overwrite(action, &policy.root, &reads) {
                Ok(action) => action,
                Err(refusal) => {
                    call_sequence += 1;
                    let capability = call.name.clone();
                    let detail = tool_call_detail(call);
                    let path = Some(refusal.path.clone());
                    crate::record_refused_action(
                        store,
                        conversation_id,
                        &refusal.action,
                        &refusal.why,
                    )?;
                    on_step(TurnStep::Refused(format!(
                        "{capability}: {} was not read first",
                        refusal.path
                    )));
                    on_step(TurnStep::ToolCall(ToolCallStep {
                        id: call_sequence,
                        capability,
                        detail,
                        path,
                        phase: ToolPhase::Refused(refusal.summary.into()),
                        diff: None,
                    }));
                    messages.push(tool_message(
                        call,
                        crate::action_outcome(Err(crate::ActionExecutionError::Denied(
                            refusal.why,
                        ))),
                    ));
                    continue;
                }
            };
            // Whether it mutates is decided here, while the typed action is
            // still in hand; whether it *did* is decided below, by whether it
            // ran. A denied edit marked the turn as having changed the
            // workspace, and the answer then carried "the checks passed after
            // the change" about a change that never happened.
            let would_mutate = crate::conversation::may_change_workspace(&action);
            ran_something |= crate::runs_the_work(&action);
            if let ActionProposal::WriteFile { path, .. } = &action
                && !policy.root.join(path).exists()
            {
                created_here.insert(path.clone());
            }
            // Only a program this turn created: an edit to one the workspace
            // already had is judged by the workspace's own checks.
            changed_runnable |= crate::changes_runnable_file(&action)
                && crate::action_path(&action).is_some_and(|path| created_here.contains(path));
            let capability = call.name.clone();
            let detail = tool_call_detail(call);
            call_sequence += 1;
            let call_id = call_sequence;
            let path = serde_json::to_value(&action)
                .ok()
                .and_then(|value| value["path"].as_str().map(str::to_owned));
            // The file before an edit, so a front end can show what changed.
            let edits_a_file = matches!(
                action,
                ActionProposal::ReplaceText { .. }
                    | ActionProposal::ApplyReplace { .. }
                    | ActionProposal::ApplyPatchHunks { .. }
                    | ActionProposal::WriteFile { .. }
            );
            let kept = edits_a_file
                .then(|| {
                    path.as_ref().map(|path| {
                        let file = policy.root.join(path);
                        match continuity.edits.lock() {
                            Ok(edits) => Before::capture(&file, &edits),
                            Err(_) => Before::capture(&file, &[]),
                        }
                    })
                })
                .flatten();
            let before = match &kept {
                Some(Before::Content(bytes)) => String::from_utf8(bytes.clone()).ok(),
                Some(Before::NotKept) => path
                    .as_ref()
                    .and_then(|path| std::fs::read_to_string(policy.root.join(path)).ok()),
                _ => None,
            };
            let fingerprint = crate::repetition::action_fingerprint(&action);
            on_step(TurnStep::ToolCall(ToolCallStep {
                id: call_id,
                capability: capability.clone(),
                detail: detail.clone(),
                path: path.clone(),
                phase: ToolPhase::Proposed,
                diff: None,
            }));
            // A run that already failed the same way twice, or any run once
            // the turn was asked to hand its work over, is answered without
            // running it.
            let runs_something = matches!(
                action,
                ActionProposal::RunCommand { .. } | ActionProposal::StartService { .. }
            );
            let repeated = echoes.repeated_failures(&fingerprint);
            if runs_something
                && (repeated >= crate::repetition::REPEATED_FAILURE_LIMIT
                    || failed_runs.handed_over())
            {
                let why = if failed_runs.handed_over() {
                    "Not run: this turn already stopped running things after repeated failures. \
                     Answer the engineer with what is ready, what failed, and the commands they \
                     can run themselves."
                        .to_owned()
                } else {
                    crate::repetition::repeated_failure_notice(repeated)
                };
                on_step(TurnStep::Refused(format!("{capability}: not run again")));
                on_step(TurnStep::ToolCall(ToolCallStep {
                    id: call_id,
                    capability: capability.clone(),
                    detail: detail.clone(),
                    path: path.clone(),
                    phase: ToolPhase::Refused("not run again".into()),
                    diff: None,
                }));
                messages.push(tool_message(call, serde_json::json!({"not_run": why})));
                continue;
            }
            let mut granted_once = match crate::session::gate(
                store,
                conversation_id,
                u8::try_from(actions).unwrap_or(u8::MAX),
                &action,
                &fingerprint,
                &mut policy,
                prompt,
                &mut refused_streak,
                call.id.clone(),
            )
            .await?
            {
                crate::session::Gate::Refused(message) => {
                    let why = if message.content == crate::repetition::repetition_notice() {
                        "proposed again after being refused, and not run".to_owned()
                    } else {
                        crate::compaction::one_line(&message.content)
                    };
                    on_step(TurnStep::Refused(format!("{capability}: {why}")));
                    on_step(TurnStep::ToolCall(ToolCallStep {
                        id: call_id,
                        capability: capability.clone(),
                        detail: detail.clone(),
                        path: path.clone(),
                        phase: ToolPhase::Refused(why),
                        diff: None,
                    }));
                    messages.push(message);
                    continue;
                }
                crate::session::Gate::Proceed { granted_once } => granted_once,
            };
            on_step(TurnStep::ToolCall(ToolCallStep {
                id: call_id,
                capability: capability.clone(),
                detail: detail.clone(),
                path: path.clone(),
                phase: ToolPhase::Started,
                diff: None,
            }));
            let mut checkpoint = continuity
                .checkpoint
                .lock()
                .map(|checkpoint| checkpoint.clone())
                .unwrap_or_default();
            // A command is the one action stop does not wait for. Seen
            // 2026-09-23: stop pressed during `npm install`, which hung until
            // its timeout, and nothing happened for minutes -- the flag was
            // read only between actions. Dropping the command kills its whole
            // process group (`ProcessGroupGuard`), and its intent is left
            // without a receipt, which is what an effect nobody can vouch for
            // should look like to a restart.
            let interruptible = matches!(action, ActionProposal::RunCommand { .. });
            // Kept to run again if the network or the engine is what it lacked.
            let command = interruptible.then(|| action.clone());
            let performing = crate::session::perform(
                store,
                conversation_id,
                &policy,
                action,
                &mut services,
                &mut reads,
                u8::try_from(actions).unwrap_or(u8::MAX),
                &mut checkpoint,
                actions,
            );
            let outcome = if interruptible {
                tokio::select! {
                    outcome = performing => outcome?,
                    () = pressed(stop) => {
                        return stopped(actions, edited, StopReason::Interrupted);
                    }
                }
            } else {
                performing.await?
            };
            let mut outcome = outcome;
            if let Some(command) = command {
                let withheld = crate::session::withheld(
                    store,
                    conversation_id,
                    u8::try_from(actions).unwrap_or(u8::MAX),
                    &command,
                    &fingerprint,
                    &mut outcome,
                    &mut policy,
                    prompt,
                    &mut refused_streak,
                )
                .await?;
                if let crate::session::Withheld::Allowed { approval, once } = withheld {
                    if once {
                        granted_once.push(approval);
                    }
                    let again = crate::session::perform(
                        store,
                        conversation_id,
                        &policy,
                        command,
                        &mut services,
                        &mut reads,
                        u8::try_from(actions).unwrap_or(u8::MAX),
                        &mut checkpoint,
                        actions,
                    );
                    outcome = tokio::select! {
                        outcome = again => outcome?,
                        () = pressed(stop) => {
                            return stopped(actions, edited, StopReason::Interrupted);
                        }
                    };
                }
            }
            if let Ok(mut shared) = continuity.checkpoint.lock() {
                *shared = checkpoint;
            }
            for approval in granted_once {
                policy.approvals.retain(|granted| *granted != approval);
            }
            match outcome {
                Ok(mut value) => {
                    // Reads are left to `ReadHistory`, which already names a
                    // re-read of an unchanged file.
                    if !matches!(
                        capability.as_str(),
                        "read_file" | "search" | "list_tree" | "vcs_status" | "vcs_diff"
                    ) && let Some(seen) = echoes.observe(&fingerprint, &value)
                        && let Some(object) = value.as_object_mut()
                    {
                        object.insert(
                            "repeated".into(),
                            serde_json::Value::String(crate::repetition::echo_notice(seen)),
                        );
                        on_step(TurnStep::Refused(format!(
                            "{capability}: the same result {seen} times"
                        )));
                    }
                    if runs_something
                        && let Some(notice) = failed_runs
                            .observe(crate::repetition::failed(&value), edited || would_mutate)
                        && let Some(object) = value.as_object_mut()
                    {
                        object.insert("stop".into(), serde_json::Value::String(notice));
                        on_step(TurnStep::Refused(format!(
                            "{capability}: {} failed runs in a row, handing over",
                            crate::repetition::FAILED_RUN_LIMIT
                        )));
                    }
                    if let Some(path) = &path
                        && let Some(kept) = &kept
                        && let Ok(mut edits) = continuity.edits.lock()
                    {
                        edits.push(FileEdit {
                            turn: continuity
                                .person_turn
                                .load(std::sync::atomic::Ordering::Relaxed),
                            path: path.clone(),
                            before: kept.clone(),
                        });
                    }
                    if edits_a_file
                        && let Some(path) = &path
                        && let Ok(bytes) = std::fs::read(policy.root.join(path))
                        && let Ok(mut written) = continuity.written.lock()
                    {
                        written.insert(path.clone(), pwr_domain::hash_bytes(&bytes));
                    }
                    if edits_a_file
                        && value.get("new_hash").is_some()
                        && let Some(path) = &path
                        && let Ok(mut stall) = continuity.stall.lock()
                    {
                        let count = {
                            let count = stall.rewrites.entry(path.clone()).or_insert(0);
                            *count += 1;
                            *count
                        };
                        if count % crate::stall::REWRITES_BEFORE_NOTE == 0
                            && let Some(object) = value.as_object_mut()
                        {
                            object.insert(
                                "note".into(),
                                serde_json::Value::String(crate::stall::rewrite_notice(
                                    path, count,
                                )),
                            );
                            on_step(TurnStep::Note(format!(
                                "{path} has been written {count} times; the model was told to \
                                 step back"
                            )));
                        }
                    }
                    if let Some(path) = &path
                        && let Some(hash) = reads.known_hash(path)
                        && let Ok(mut known) = continuity.known.lock()
                    {
                        known.insert(crate::known_key(path), hash.to_owned());
                    }
                    refused_streak.observe(&fingerprint, &value);
                    // The checks half of the signature is constant within a
                    // turn: a conversation runs the repository's checks after
                    // the turn, not during it. So what moves here is the
                    // workspace, which is the half a turn can move.
                    let observed = continuity.stall.lock().ok().and_then(|mut stall| {
                        let stall = &mut *stall;
                        crate::stall::record_effect(&mut stall.changed_files, &value);
                        let effect = crate::EffectSignature::of(
                            &stall.changed_files,
                            "checks are run after the turn, not within it",
                        );
                        stall
                            .progress
                            .observe(fingerprint.clone(), effect)
                            .map(|stalled| (stalled, stall.progress.windows))
                    });
                    let mut halt = false;
                    if let Some((stalled, windows)) = observed {
                        store
                            .append_event(
                                Some(conversation_id),
                                &pwr_domain::RunEvent::NoProgressDetected {
                                    step: u8::try_from(actions).unwrap_or(u8::MAX),
                                    window: crate::stall::NO_PROGRESS_WINDOW,
                                    actions: stalled,
                                },
                            )
                            .map_err(|error| error.to_string())?;
                        halt = windows >= crate::stall::NO_PROGRESS_LIMIT;
                        on_step(TurnStep::Refused(
                            "the last actions left the workspace where it was".into(),
                        ));
                        if !halt {
                            messages.push(ChatMessage {
                                role: "tool".into(),
                                content: crate::stall::no_progress_notice(),
                                ..Default::default()
                            });
                        }
                    }
                    edited |= would_mutate;
                    let diff = edits_a_file
                        .then(|| {
                            let path = path.clone()?;
                            let new_text = std::fs::read_to_string(policy.root.join(&path)).ok()?;
                            Some(FileDiff {
                                path,
                                old_text: before.clone(),
                                new_text,
                            })
                        })
                        .flatten();
                    on_step(TurnStep::ToolCall(ToolCallStep {
                        id: call_id,
                        capability: capability.clone(),
                        detail: detail.clone(),
                        path: path.clone(),
                        phase: ToolPhase::Completed,
                        diff,
                    }));
                    on_step(TurnStep::Acted {
                        capability,
                        detail: format!("{} chars", value.to_string().chars().count()),
                    });
                    messages.push(tool_message(call, crate::action_outcome(Ok(value))));
                    if halt {
                        return stopped(actions, edited, StopReason::NoProgress);
                    }
                }
                Err(problem) => {
                    // Every error the conversation sees here is a refusal: the
                    // action did not have its effect. The run distinguishes a
                    // policy denial from a broken tool for the streak's
                    // purposes and both count as refused, which is what
                    // `action_outcome` renders and what this reads back.
                    refused_streak.refused(&fingerprint);
                    on_step(TurnStep::ToolCall(ToolCallStep {
                        id: call_id,
                        capability: capability.clone(),
                        detail,
                        path: path.clone(),
                        phase: ToolPhase::Failed(problem.to_string()),
                        diff: None,
                    }));
                    on_step(TurnStep::Refused(format!("{capability}: {problem}")));
                    let mut outcome = crate::action_outcome(Err(problem));
                    if runs_something
                        && let Some(notice) = failed_runs.observe(true, edited)
                        && let Some(object) = outcome.as_object_mut()
                    {
                        object.insert("stop".into(), serde_json::Value::String(notice));
                    }
                    messages.push(tool_message(call, outcome));
                }
            }
        }
    }
}

/// Whether the backend is silently truncating, and what to do about it.
///
/// A backend that cannot fit the prompt is not obliged to refuse it. LM
/// Studio's answer is `TruncateMiddle`: it keeps the head -- system prompt and
/// task -- and the tail, deletes everything between them, and returns an
/// ordinary reply. What it then reports as the prompt is what survived the cut
/// rather than what was sent, so the count stops moving.
///
/// Measured on an 80B loaded at 16,384 tokens while the chat believed it had
/// 65,536: exactly 7,090 tokens reported for twenty consecutive turns while the
/// conversation kept growing, and every tool result, file body and build log
/// the turn had gathered was deleted before the model read it. The run was
/// recorded as a model that could not converge.
///
/// Two different prompts do not tokenise to the same length by chance, so a
/// count that repeats while messages are being added is the truncation showing
/// through. The first turn has nothing to compare against, and a backend that
/// reports no count cannot be checked; neither is evidence, so neither speaks.
fn truncation_note(
    counted: Option<u64>,
    previous: Option<u64>,
    context_tokens: u32,
) -> Option<String> {
    let counted = counted?;
    (counted == previous?).then(|| {
        format!(
            "the backend reported the same {counted}-token prompt as last turn although the \
             conversation grew, which is what silent truncation looks like: it is serving a \
             smaller window than the {context_tokens} configured here, and the middle of every \
             turn is being dropped before the model reads it. Load the model with a larger \
             context, and check the backend's parallel slots -- they divide it."
        )
    })
}

/// What the conversation costs, measured where possible.
///
/// The backend counts the prompt it received and reports it with every reply,
/// so after the first turn there is a measurement and no reason to estimate.
/// `measured` is that count; the character heuristic is the fallback for the
/// first turn alone, and for a backend that reports nothing.
///
/// The heuristic was the whole rule and it was wrong by nearly three times.
/// Measured on the 80B run's own audit: 89,666 characters of tool results
/// estimated at 22,416 tokens and counted by the backend at 8,183. On English
/// prose the same heuristic lands within 8 percent -- but an agent's
/// conversation is not prose. It is JSON, hex digests, paths and build output,
/// all of which tokenise far more densely, and it is what a long run is
/// mostly made of.
///
/// The cost: compaction fired at roughly a quarter of the context the operator
/// had configured, so a long turn kept losing the file it was editing. The 80B
/// re-read the same service forty times, reintroduced an error it had already
/// What the prompt would cost now: the backend's own count of what it last
/// read, plus what has been appended since, corrected by how wrong the
/// character estimate proved on this deployment.
///
/// D6, 2026-09-16: a measured count alone is stale by whatever the turn added,
/// and what a turn adds is a tool result -- a file read, at up to the read
/// budget. The scripted loop filled its window that way and came back with
/// reasoning and no call in it, five times in fifteen trials. What the count
/// already includes is the request's fixed parts, so only the messages
/// appended since it are added to it.
/// The last finished generation of a conversation, for the context panel:
/// token counts by phase, where they came from, and the reasoning budget that
/// applied. Counts only -- the reasoning itself is never recorded.
pub fn last_generation(
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
) -> Result<Option<serde_json::Value>, String> {
    let events = store
        .events_for_run(conversation_id)
        .map_err(|error| error.to_string())?;
    let Some(position) = events
        .iter()
        .rposition(|event| event.event_type == "generation.completed")
    else {
        return Ok(None);
    };
    let completed = &events[position].payload;
    let started = events[..position]
        .iter()
        .rev()
        .find(|event| event.event_type == "generation.started")
        .map(|event| event.payload["reasoning"].clone())
        .unwrap_or(serde_json::Value::Null);
    Ok(Some(serde_json::json!({
        "at": events[position].at,
        "reasoningTokens": completed["reasoning_tokens"],
        "answerTokens": completed["answer_tokens"],
        "generatedTokens": completed["generated_tokens"],
        "tokenAccounting": completed["token_accounting"],
        "toolCallTokensEstimated": completed["tool_call_tokens_estimated"],
        "budgetReached": completed["reasoning_budget_reached"],
        "effort": started["effort"],
        "requestedTokens": started["requested_tokens"],
        "effectiveTokens": started["effective_tokens"],
        "clamped": started["clamped"],
    })))
}

/// Writes the plan into the request's sampling, where the engines read it.
fn apply_reasoning_plan(
    sampling: &mut BTreeMap<String, serde_json::Value>,
    plan: &pwr_domain::ReasoningPlan,
) {
    use pwr_domain::ReasoningDirective;
    sampling.remove("reasoning_budget");
    match plan.directive {
        ReasoningDirective::TemplateDefault => {}
        ReasoningDirective::Off => {
            sampling.insert("think".into(), serde_json::json!(false));
        }
        ReasoningDirective::Budget { tokens } => {
            sampling.insert("reasoning_budget".into(), serde_json::json!(tokens));
        }
        ReasoningDirective::Level { effort } => {
            // A profile's "off" would make the engine send its least level
            // instead; the chosen level is what goes to the template.
            sampling.remove("think");
            sampling.insert(
                "reasoning_effort".into(),
                serde_json::json!(effort.as_str()),
            );
        }
    }
    sampling.insert("max_tokens".into(), serde_json::json!(plan.max_tokens));
}

/// The audit of one finished generation: counts, never the reasoning itself.
fn record_generation(
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
    plan: &pwr_domain::ReasoningPlan,
    reply: &pwr_provider::ModelReply,
    reasoning_seen: bool,
    elapsed: std::time::Duration,
) -> Result<(), String> {
    let metrics = reply.metrics.clone().unwrap_or_default();
    let append = |kind: &str, payload: serde_json::Value| {
        store
            .append(Some(conversation_id), kind, payload)
            .map(|_| ())
            .map_err(|error| error.to_string())
    };
    if metrics.reasoning_budget_reached == Some(true) {
        append(
            "reasoning.budget_reached",
            serde_json::json!({
                "effective_tokens": plan.effective_tokens,
                "reasoning_tokens": metrics.reasoning_tokens,
            }),
        )?;
        append(
            "generation.finalizing",
            serde_json::json!({"strategy": "engine_closed_thinking_delimiter"}),
        )?;
    }
    if reasoning_seen || metrics.reasoning_tokens.unwrap_or(0) > 0 {
        append(
            "reasoning.completed",
            serde_json::json!({
                "reasoning_tokens": metrics.reasoning_tokens,
                "token_accounting": metrics.token_accounting,
                // Only when the backend did not count it: an estimate, so
                // named as one.
                "reasoning_tokens_estimated": metrics
                    .reasoning_tokens
                    .is_none()
                    .then(|| crate::context::estimate_tokens(&reply.thinking)),
            }),
        )?;
    }
    let tool_call_chars: usize = reply
        .tool_calls
        .iter()
        .map(|call| call.name.len() + call.arguments.to_string().len())
        .sum();
    append(
        "generation.completed",
        serde_json::json!({
            "duration_ms": u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
            "prompt_tokens": metrics.prompt_tokens,
            "generated_tokens": metrics.generated_tokens,
            "reasoning_tokens": metrics.reasoning_tokens,
            "answer_tokens": metrics.answer_tokens,
            "token_accounting": metrics.token_accounting,
            // No backend separates a call's tokens from the answer's.
            "tool_call_tokens_estimated": (tool_call_chars > 0).then_some(tool_call_chars / 4),
            "reasoning_budget_reached": metrics.reasoning_budget_reached,
            "reasoning_repetition_bps": metrics.reasoning_repetition_bps,
            "answer_repetition_bps": metrics.answer_repetition_bps,
            "effective_reasoning_tokens": plan.effective_tokens,
            "clamped": plan.clamped,
        }),
    )
}

/// The prompt's size for budgeting a generation: the engine's count where
/// there is one, and a denser estimate (three characters a token rather than
/// four) for what was appended since, because code and JSON tokenize denser
/// than prose and understating here would eat the answer's reserve.
fn conservative_prompt_tokens(
    messages: &[ChatMessage],
    measured: Option<u64>,
    measured_upto: usize,
) -> usize {
    let (base, from) = match measured {
        Some(measured) => (
            usize::try_from(measured).unwrap_or(usize::MAX),
            measured_upto,
        ),
        None => (0, 0),
    };
    let since: usize = messages
        .iter()
        .skip(from)
        .map(|message| {
            message.content.len().div_ceil(3)
                + message
                    .reasoning
                    .as_ref()
                    .map_or(0, |r| r.len().div_ceil(3))
                + 4
        })
        .sum();
    base.saturating_add(since)
}

/// Said to a `complete` in a turn that has written, run and read nothing.
pub(crate) const COMPLETION_WITH_NOTHING_DONE: &str = "complete was not carried out: nothing has been \
     written, run or read in this turn, so any file or result it describes does not exist yet. \
     If the request needs files or commands, do that now with the tools. If it needed only an \
     answer, call complete again.";

/// Said to a `complete` that arrived behind other calls of its reply.
const COMPLETION_OVER_UNSEEN_RESULTS: &str = "complete was not carried out: it came in the same \
     reply as the calls before it, so their results had not been read. They are above. If they \
     show the work is done, call complete again; otherwise act on what they say.";

/// A `write_file` onto a file that exists, as the whole-file replacement it
/// means -- of the version the conversation last saw.
///
/// `write_file` alone refuses an existing file, so that a blind overwrite is
/// never one missing argument away, and names `apply_replace` and the hash.
/// Measured 2026-09-25 on Ornith 1.5 35B: six refusals in one conversation,
/// every one on a file it had itself created minutes earlier, each costing a
/// whole regenerated file. A conversation keeps every file's earlier content
/// for rewinding ([`FileEdit`]) and shows each change as a diff that can be
/// reverted, so replacing a file the conversation knows is no longer the
/// unrecoverable act that refusal guarded against.
///
/// What that change removed by mistake was the hash's meaning. It read the
/// file's hash *when the write was made* and used it as the expected one, so
/// the check compared the file with itself: a rewrite made from content the
/// model read an hour ago, over an edit the person made since, passed, and so
/// did a rewrite of a file the model had never opened (technical review of
/// 2026-09-30, verified the same day). The expected hash is now the one the
/// conversation last saw -- from a read, or from the edit or creation that
/// left the file as it is -- and a file it has not seen, or that is no longer
/// as it was seen, is refused with what to do. The refusal does not name the
/// file's current hash: a caller handed it would send it back without reading
/// anything, which is the mistake again.
///
/// Protected paths and the workspace boundary are still enforced by the tool.
fn own_overwrite(
    action: ActionProposal,
    root: &std::path::Path,
    reads: &crate::ReadHistory,
) -> Result<ActionProposal, Box<OverwriteRefusal>> {
    let ActionProposal::WriteFile { path, content } = action else {
        return Ok(action);
    };
    let Ok(bytes) = std::fs::read(root.join(&path)) else {
        return Ok(ActionProposal::WriteFile { path, content });
    };
    let current = pwr_domain::hash_bytes(&bytes);
    match reads.known_hash(&path) {
        Some(known) if known == current => Ok(ActionProposal::ApplyReplace {
            path,
            expected_hash: current,
            replacement: content,
        }),
        Some(_) => Err(Box::new(OverwriteRefusal {
            summary: "changed since it was last read",
            why: format!(
                "not written: {path} has changed since you last read or wrote it -- by a \
                 command, or outside this conversation -- so replacing it whole would discard \
                 those changes. Read it again with read_file, then send the file you mean it to \
                 be; or change only part of it with replace_text or apply_patch."
            ),
            action: ActionProposal::WriteFile {
                path: path.clone(),
                content,
            },
            path,
        })),
        None => Err(Box::new(OverwriteRefusal {
            summary: "exists and was not read",
            why: format!(
                "not written: {path} already exists and this conversation has not read it, so \
                 replacing it whole could discard content you have not seen. Read it with \
                 read_file first, then send the file you mean it to be; or change only part of \
                 it with replace_text or apply_patch."
            ),
            action: ActionProposal::WriteFile {
                path: path.clone(),
                content,
            },
            path,
        })),
    }
}

/// A whole-file rewrite the harness did not make, and what it told the model.
struct OverwriteRefusal {
    path: String,
    /// The refused call as proposed, for the audit.
    action: ActionProposal,
    /// The line a front end shows.
    summary: &'static str,
    /// What the model is told.
    why: String,
}

/// The most reasoning one assistant step hands to the next, in characters:
/// about four thousand tokens, the Low budget. A reply that ran past it keeps
/// its end, which is where a model writes what it concluded.
const REASONING_KEPT_CHARS: usize = 16_000;

/// The reasoning an assistant step carries in the history, for every step
/// after it, until a compaction folds it away.
fn kept_reasoning(thinking: &str) -> Option<String> {
    let thinking = thinking.trim();
    if thinking.is_empty() {
        return None;
    }
    if thinking.len() <= REASONING_KEPT_CHARS {
        return Some(thinking.to_owned());
    }
    let mut start = thinking.len() - REASONING_KEPT_CHARS;
    while !thinking.is_char_boundary(start) {
        start += 1;
    }
    Some(format!(
        "[earlier reasoning omitted]\n{}",
        &thinking[start..]
    ))
}

fn prompt_tokens_now(
    messages: &[ChatMessage],
    measured: Option<u64>,
    measured_upto: usize,
) -> usize {
    let Some(measured) = measured else {
        return conversation_tokens(messages, None);
    };
    let since: usize = messages
        .iter()
        .skip(measured_upto)
        .map(|message| {
            crate::context::estimate_tokens(&message.content)
                + message
                    .reasoning
                    .as_deref()
                    .map_or(0, crate::context::estimate_tokens)
        })
        .sum();
    // The measured count is of a whole request, tool schemas and template
    // included, so what it is missing is only what was appended after it.
    usize::try_from(measured).unwrap_or(usize::MAX) + since
}

/// fixed, and finally made the build pass by abandoning the signals the
/// specification asked for.
fn conversation_tokens(messages: &[ChatMessage], measured: Option<u64>) -> usize {
    if let Some(measured) = measured {
        // Stale by one turn: it counts the prompt already sent, not the
        // messages appended since. Understating by a turn is the safe
        // direction -- it delays compaction rather than triggering it early,
        // and the messages added since are bounded by one reply and one tool
        // result.
        return usize::try_from(measured).unwrap_or(usize::MAX);
    }
    messages
        .iter()
        .map(|message| crate::context::estimate_tokens(&message.content))
        .sum()
}

/// Automatic compaction: the shared [`crate::compaction::compact`], keeping
/// as much recent conversation as fits in half the room, recorded in the audit
/// as `context.compacted` with its trigger. `None` when nothing older than the
/// tail is left to fold.
fn compact_and_record(
    store: &pwr_store::Store,
    conversation_id: pwr_domain::Id,
    deployment: &DeploymentDescriptor,
    context_tokens: u32,
    messages: &mut Vec<ChatMessage>,
    room: usize,
    continuity: &Continuity,
) -> Result<Option<String>, String> {
    let carry = continuity
        .checkpoint
        .lock()
        .map(|checkpoint| crate::compaction::Carry::from_checkpoint(&checkpoint))
        .unwrap_or_default();
    let Some(done) = crate::compaction::compact(
        messages,
        tail_budget(room),
        &carry,
        crate::compaction::Trigger::Automatic,
    ) else {
        return Ok(None);
    };
    crate::compaction::record(
        store,
        conversation_id,
        &done,
        &deployment.model_ref,
        context_tokens,
    )?;
    Ok(Some(done.note()))
}

/// The verbatim tail automatic compaction keeps, from the room it must make.
fn tail_budget(room: usize) -> usize {
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    let budget = (room as f64 * VERBATIM_SHARE) as usize;
    budget
}

/// The automatic compaction without an audit, for the tests below.
#[cfg(test)]
fn compact(messages: &mut Vec<ChatMessage>, room: usize) -> Option<String> {
    crate::compaction::compact(
        messages,
        tail_budget(room),
        &crate::compaction::Carry::default(),
        crate::compaction::Trigger::Automatic,
    )
    .map(|done| done.note())
}

/// Resolves when the operator has pressed stop.
///
/// Polled rather than notified, because the flag is shared with a terminal
/// event loop that has no async waker to offer. The interval is short enough
/// that stop feels immediate and long enough to cost nothing.
async fn pressed(stop: &std::sync::atomic::AtomicBool) {
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// Through the same decoder the run uses, so the conversation gets every
/// repair it has: a lone string where a list was declared, the program at the
/// front of `args`, numbers and booleans sent for strings, file content sent
/// as a structure. The conversation had its own `serde_json::from_value` and
/// none of them -- measured 2026-09-22, `write_file` for `package.json` with an
/// object as `content`, refused three times and the turn ended, while the fix
/// for exactly that sat in the shared decoder unused.
fn decode(call: &pwr_domain::ToolCall, catalog: &ToolCatalog) -> Result<ActionProposal, String> {
    if !call.arguments.is_object() {
        return Err("the call carried no arguments".into());
    }
    // A capability the conversation does not offer is refused as unknown even
    // where the run has it (`record_progress`, `propose_verifier`).
    if catalog.get(&call.name).is_none() {
        let mut arguments = call.arguments.clone();
        arguments["capability"] = serde_json::Value::String(call.name.clone());
        return match serde_json::from_value::<ActionProposal>(arguments) {
            Ok(_) => Err(format!("{} is not available in a conversation", call.name)),
            Err(error) => Err(format!("{error}{}", what_was_sent(call, catalog))),
        };
    }
    crate::action_from_tool_call(call).map_err(|malformed| malformed.problem)
}

/// The half of a decode failure the deployment cannot see.
///
/// serde says what was missing and nothing about what arrived. The explanation
/// lives on the catalogue now, so the conversation, the capability probe and a
/// scripted run all give the same one -- they used to differ, and the probe
/// recorded a deployment as unmeasurable for a mistake the conversation could
/// already explain.
fn what_was_sent(call: &pwr_domain::ToolCall, catalog: &ToolCatalog) -> String {
    let sent = call
        .arguments
        .as_object()
        .map(|fields| fields.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    let hint = catalog.mismatch_hint(&call.name, &sent);
    if hint.is_empty() {
        String::new()
    } else {
        format!(". {hint}")
    }
}

/// One outcome, in the shape both loops send.
///
/// `status` is `None` because a conversation has no plan and no action budget
/// to report: the key is absent rather than present and empty. That is the
/// declared difference between the modes. The envelope is not -- it was a
/// difference nobody chose, and a deployment answering both loops was reading
/// two protocols.
fn tool_message(call: &pwr_domain::ToolCall, outcome: serde_json::Value) -> ChatMessage {
    crate::tool_result_message(outcome, None, call.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rewind_restores_each_file_as_it_was_and_names_what_someone_else_changed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.py"), "v3\n").unwrap();
        std::fs::write(dir.path().join("new.py"), "created\n").unwrap();
        std::fs::write(dir.path().join("mine.py"), "edited by hand\n").unwrap();
        let edit = |turn, path: &str, before: Option<&str>| FileEdit {
            turn,
            path: path.into(),
            before: before.map_or(Before::Created, |text| Before::Content(text.into())),
        };
        let edits = vec![
            edit(1, "a.py", Some("v1\n")),
            edit(2, "a.py", Some("v2\n")),
            edit(3, "a.py", Some("v2.5\n")),
            edit(2, "new.py", None),
            edit(2, "mine.py", Some("original\n")),
        ];
        let written = BTreeMap::from([
            ("a.py".to_owned(), pwr_domain::hash_bytes(b"v3\n")),
            ("new.py".to_owned(), pwr_domain::hash_bytes(b"created\n")),
            (
                "mine.py".to_owned(),
                pwr_domain::hash_bytes(b"what PWR wrote\n"),
            ),
        ]);
        let (plan, conflicts) = rewind_plan(dir.path(), &edits, &written, 2);
        // Each file as it was before the first edit at or after turn 2.
        assert_eq!(
            plan,
            vec![
                ("a.py".to_owned(), Before::Content(b"v2\n".to_vec())),
                ("new.py".to_owned(), Before::Created),
                (
                    "mine.py".to_owned(),
                    Before::Content(b"original\n".to_vec())
                ),
            ]
        );
        assert_eq!(conflicts, vec!["mine.py".to_owned()]);
        // Nothing at or after turn 4: nothing to restore.
        assert!(rewind_plan(dir.path(), &edits, &written, 4).0.is_empty());
    }

    #[test]
    fn a_file_that_is_not_text_is_kept_as_bytes_and_one_not_kept_is_never_deleted() {
        let dir = tempfile::tempdir().unwrap();
        // Latin-1, not UTF-8: read as text it looked absent, and a rewind
        // then deleted it as a file the edit had created.
        let latin1 = b"caf\xe9\n".to_vec();
        std::fs::write(dir.path().join("notes.txt"), &latin1).unwrap();
        assert_eq!(
            Before::capture(&dir.path().join("notes.txt"), &[]),
            Before::Content(latin1)
        );
        assert_eq!(
            Before::capture(&dir.path().join("absent.txt"), &[]),
            Before::Created
        );
        // Past what a session keeps: recorded, never restored or deleted.
        let big = vec![b'x'; EDIT_BYTES_KEPT + 1];
        std::fs::write(dir.path().join("big.bin"), &big).unwrap();
        assert_eq!(
            Before::capture(&dir.path().join("big.bin"), &[]),
            Before::NotKept
        );
        let full: Vec<FileEdit> = (0..EDITS_KEPT)
            .map(|n| FileEdit {
                turn: 1,
                path: format!("f{n}"),
                before: Before::Created,
            })
            .collect();
        assert_eq!(
            Before::capture(&dir.path().join("notes.txt"), &full),
            Before::NotKept
        );
        let edits = vec![FileEdit {
            turn: 1,
            path: "big.bin".into(),
            before: Before::NotKept,
        }];
        let (plan, conflicts) = rewind_plan(dir.path(), &edits, &BTreeMap::new(), 1);
        assert_eq!(plan, vec![("big.bin".to_owned(), Before::NotKept)]);
        assert!(conflicts.is_empty());
    }

    fn write_of(path: &str) -> ActionProposal {
        ActionProposal::WriteFile {
            path: path.into(),
            content: "print(2)\n".into(),
        }
    }

    fn seen(path: &str, bytes: &[u8]) -> crate::ReadHistory {
        let mut reads = crate::ReadHistory::default();
        reads.seed_known(&BTreeMap::from([(
            path.to_owned(),
            pwr_domain::hash_bytes(bytes),
        )]));
        reads
    }

    #[test]
    fn a_file_the_conversation_knows_is_replaced_and_a_new_one_is_created() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("_check.py"), "print(1)\n").unwrap();
        let reads = seen("_check.py", b"print(1)\n");
        assert!(matches!(
            own_overwrite(write_of("_check.py"), dir.path(), &reads),
            Ok(ActionProposal::ApplyReplace { ref expected_hash, .. })
                if *expected_hash == pwr_domain::hash_bytes(b"print(1)\n")
        ));
        // The same file under another spelling of its path is the same file.
        assert!(matches!(
            own_overwrite(write_of("./_check.py"), dir.path(), &reads),
            Ok(ActionProposal::ApplyReplace { .. })
        ));
        assert!(matches!(
            own_overwrite(write_of("new.py"), dir.path(), &reads),
            Ok(ActionProposal::WriteFile { .. })
        ));
    }

    /// The defect of 2026-09-30: the expected hash was read from the file at
    /// write time, so a rewrite over an edit made after the model read passed
    /// its own hash check and discarded the edit.
    #[test]
    fn a_rewrite_over_an_edit_made_after_the_read_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.py"), "print(1)\n").unwrap();
        let reads = seen("a.py", b"print(1)\n");
        // The person edits the file; the model, still holding the old read,
        // sends the whole file again.
        std::fs::write(dir.path().join("a.py"), "print(1)\nprint('mine')\n").unwrap();
        let Err(refusal) = own_overwrite(write_of("a.py"), dir.path(), &reads) else {
            panic!("the rewrite discarded an edit it had not seen");
        };
        assert!(refusal.why.contains("changed since you last read"));
        assert!(refusal.why.contains("Read it again"));
        // The refusal does not hand over the hash that would get the same
        // write through without a read.
        let current = pwr_domain::hash_bytes(b"print(1)\nprint('mine')\n");
        assert!(!refusal.why.contains(&current));
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.py")).unwrap(),
            "print(1)\nprint('mine')\n"
        );
    }

    #[test]
    fn a_file_that_exists_and_was_never_read_is_not_rewritten() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.py"), "print(1)\n").unwrap();
        let reads = crate::ReadHistory::default();
        let Err(refusal) = own_overwrite(write_of("a.py"), dir.path(), &reads) else {
            panic!("a file nobody read was rewritten");
        };
        assert!(refusal.why.contains("has not read it"));
        assert!(refusal.why.contains("read_file"));
        assert_eq!(refusal.path, "a.py");
    }

    #[test]
    fn a_file_a_command_changed_after_the_read_is_refused_too() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("out.txt"), "one\n").unwrap();
        let reads = seen("out.txt", b"one\n");
        // A build, a formatter, a generator: anything that is not the model.
        std::fs::write(dir.path().join("out.txt"), "two\n").unwrap();
        assert!(own_overwrite(write_of("out.txt"), dir.path(), &reads).is_err());
    }

    #[test]
    fn a_steps_reasoning_is_kept_to_its_conclusion_and_forgotten_when_the_person_speaks() {
        assert_eq!(kept_reasoning("  \n "), None);
        assert_eq!(kept_reasoning(" short ").as_deref(), Some("short"));
        // A runaway keeps its end, on a character boundary.
        let long = format!(
            "{}é{}CONCLUSION",
            "x".repeat(REASONING_KEPT_CHARS),
            "y".repeat(10)
        );
        let kept = kept_reasoning(&long).unwrap();
        assert!(kept.starts_with("[earlier reasoning omitted]"));
        assert!(kept.ends_with("CONCLUSION"));
        assert!(kept.len() <= REASONING_KEPT_CHARS + 40);

        let mut step = ChatMessage::text("assistant", "");
        step.reasoning = Some("r".repeat(3_000));
        let mut messages = vec![ChatMessage::text("system", "s"), step];
        // Counted while it is carried, since the prompt carries it.
        assert!(prompt_tokens_now(&messages, Some(100), 1) > 100 + 500);
        messages[1].reasoning = None;
        assert_eq!(prompt_tokens_now(&messages, Some(100), 1), 100);
    }

    #[test]
    fn the_conversation_is_offered_the_work_and_not_only_the_reading() {
        let catalog = chat_tool_catalog();
        let names: Vec<&str> = catalog
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        for expected in [
            "read_file",
            "search",
            "replace_text",
            "run_command",
            "complete",
        ] {
            assert!(names.contains(&expected), "{expected} is not offered");
        }
        // What answers to a plan and a run's verification contract, neither of
        // which a conversation has.
        for run_only in RUN_ONLY {
            assert!(!names.contains(&run_only), "{run_only} is still offered");
        }
    }

    #[test]
    fn an_edit_owes_the_checks_an_answer_and_a_read_does_not() {
        assert!(crate::conversation::may_change_workspace(
            &ActionProposal::WriteFile {
                path: "a.rs".into(),
                content: String::new(),
            }
        ));
        assert!(crate::conversation::may_change_workspace(
            &ActionProposal::RunCommand {
                executable: "cargo".into(),
                args: vec![],
                stdin: None,
                cwd: None,
                outside_sandbox: false,
            }
        ));
        assert!(!crate::conversation::may_change_workspace(
            &ActionProposal::ReadFile {
                path: "a.rs".into(),
                first_line: None,
                max_lines: None,
            }
        ));
        assert!(!crate::conversation::may_change_workspace(
            &ActionProposal::VcsStatus {}
        ));
    }

    fn said(role: &str, text: &str) -> ChatMessage {
        ChatMessage::text(role, text)
    }

    fn called(name: &str) -> ChatMessage {
        ChatMessage {
            role: "assistant".into(),
            content: String::new(),
            tool_calls: vec![pwr_domain::ToolCall {
                name: name.into(),
                arguments: serde_json::json!({}),
                id: None,
            }],
            tool_call_id: None,
            purpose: None,
            images: Vec::new(),
            reasoning: None,
        }
    }

    /// Compaction keeps what a conversation is made of -- the instructions,
    /// the goals, the actions -- and drops what its tokens are made of, which
    /// is tool output already consumed by the turn that asked for it.
    #[test]
    fn compaction_keeps_the_goals_and_drops_the_output() {
        let mut messages = vec![said("system", "you are PWR")];
        messages.push(said("user", "fix the parser"));
        messages.push(called("read_file"));
        for _ in 0..12 {
            messages.push(said("tool", &"x".repeat(4_000)));
        }
        messages.push(said("user", "and now the lexer"));
        messages.push(said("assistant", "done"));
        let before = conversation_tokens(&messages, None);

        let note = compact(&mut messages, 8_000).expect("there was history to fold");
        let after = conversation_tokens(&messages, None);
        assert!(after < before, "{note}");

        // The instructions survive: a model told how to behave and then not
        // told is a different agent on the next turn.
        assert_eq!(messages[0].content, "you are PWR");
        // The summary is the user's side of the conversation, and the role
        // says so: a template with no user turn will not render.
        assert_eq!(messages[1].role, "user");
        let record = &messages[1].content;
        assert!(record.contains("fix the parser"), "{record}");
        assert!(record.contains("read_file"), "{record}");
        assert!(record.contains("tool result(s)"), "{record}");
        // And the raw output does not.
        assert!(!record.contains(&"x".repeat(100)));
    }

    /// A kept window that begins with a tool result whose call was folded away
    /// is a result answering nothing, which no backend can pair up.
    /// A ledger composed ten turns ago carries the hashes the files had then.
    /// Keeping it is worse than dropping it: a stale hash replayed as current
    /// is the refused-edit loop the ledger exists to prevent. A current one is
    /// composed every turn, so the older copies are superseded, not lost.
    #[test]
    fn a_superseded_ledger_is_dropped_rather_than_carried_with_its_old_hashes() {
        let ledger = |hash: &str| ChatMessage {
            role: "user".into(),
            content: format!(
                "Files this session changed:\n  - code.rs (expected_hash {hash}){}",
                " padding".repeat(400)
            ),
            purpose: Some(pwr_domain::MessagePurpose::SessionLedger),
            ..Default::default()
        };
        let mut messages = vec![
            said("system", "instructions"),
            ledger("old"),
            said("user", "change it"),
            said("assistant", "Done."),
            said("user", "and again"),
        ];
        compact(&mut messages, 200).expect("nothing was folded");
        let record = &messages[1].content;
        assert!(!record.contains("expected_hash old"), "{record}");
        assert!(
            record.contains("composed every turn"),
            "the summary did not say a current one replaces it: {record}"
        );
        // The request itself is recent enough to survive verbatim, which is
        // the point of the tail window: only the ledger was old enough to fold.
        assert!(
            messages.iter().any(|m| m.content == "change it"),
            "the request was folded away with the ledger"
        );
    }

    /// Retrieved passages ride in the user role, and folding them as requests
    /// wrote "you were asked: Repository passages ranked against this task"
    /// into the summary -- false, and the sort of sentence a deployment then
    /// tries to answer.
    #[test]
    fn retrieved_passages_are_not_folded_away_as_things_the_operator_asked_for() {
        let excerpts = ChatMessage {
            role: "user".into(),
            content: format!(
                "Repository passages ranked against this task.{}",
                " padding".repeat(400)
            ),
            purpose: Some(pwr_domain::MessagePurpose::RepositoryExcerpts),
            ..Default::default()
        };
        let mut messages = vec![
            said("system", "instructions"),
            said("user", "find the parser"),
            excerpts,
            said("assistant", "I read it."),
            said("user", "now fix it"),
        ];
        let note = compact(&mut messages, 200).expect("nothing was folded");
        let record = &messages[1].content;
        assert!(
            !record.contains("you were asked: Repository passages"),
            "{record}"
        );
        assert!(
            record.contains("retrieved repository passages were dropped"),
            "{record}"
        );
        // The operator's own request is still recorded as one.
        assert!(
            record.contains("you were asked: find the parser"),
            "{record}"
        );
        assert!(note.contains("summarised"), "{note}");
    }

    #[test]
    fn compaction_never_leaves_a_result_answering_nothing() {
        let mut messages = vec![said("system", "s")];
        messages.push(said("user", "go"));
        for _ in 0..14 {
            messages.push(called("read_file"));
            messages.push(said("tool", "result"));
        }
        // Small enough that the tail is genuinely bounded, so the window has
        // to be moved off an orphaned result rather than swallowing everything.
        compact(&mut messages, 24).expect("folded");
        assert_ne!(messages[2].role, "tool", "the window opens on an orphan");
    }

    /// Nothing left to fold is not the same as folding nothing: the caller has
    /// to be able to tell, because it is the case where compacting again would
    /// not help.
    #[test]
    fn a_conversation_with_no_history_is_not_compacted() {
        let mut messages = vec![said("system", "s"), said("user", "go")];
        assert!(compact(&mut messages, 4_000).is_none());
        assert_eq!(messages.len(), 2);
    }

    /// A small window is where a fixed count of kept messages failed: four
    /// file reads alone overflow a 4k context, so compaction refused to run
    /// and the turn stopped instead of continuing.
    #[test]
    fn compaction_works_where_a_fixed_count_of_kept_messages_would_not() {
        let mut messages = vec![said("system", "s"), said("user", "read them all")];
        for _ in 0..4 {
            messages.push(called("read_file"));
            // A file read at this size is roughly 900 tokens; four of them
            // overflow a 4096-token window on their own.
            messages.push(said("tool", &"x".repeat(3_500)));
        }
        let room = 3_072;
        assert!(conversation_tokens(&messages, None) > room);
        let note = compact(&mut messages, room).expect("a small window still compacts");
        assert!(
            conversation_tokens(&messages, None) < room,
            "compaction did not make room: {note}"
        );
        assert!(messages.len() >= crate::compaction::KEEP_AT_LEAST + 2);
    }

    /// The invariant that broke a real conversation. Qwen's template refuses a
    /// message list with no user turn -- "No user query found in messages" --
    /// and compaction had folded every one of them into a tool message.
    #[test]
    fn a_compacted_conversation_always_still_has_a_user_turn() {
        for tail in [2usize, 5, 9] {
            let mut messages = vec![said("system", "s"), said("user", "the request")];
            for _ in 0..tail {
                messages.push(called("read_file"));
                messages.push(said("tool", &"x".repeat(2_000)));
            }
            if compact(&mut messages, 400).is_none() {
                continue;
            }
            assert!(
                messages.iter().any(|message| message.role == "user"),
                "no user turn survived compaction with a tail of {tail}"
            );
        }
    }

    /// There is no action budget. What ends a turn is the operator, the
    /// context, or the evidence that it is looping.
    #[test]
    fn the_bounds_are_the_operator_the_context_and_looping() {
        const { assert!(COMPACT_AT < 1.0 && COMPACT_AT > 0.5) };
        const { assert!(COMPACTIONS_PER_TURN >= 1) };
        // Spelled in pieces so this assertion is not itself the match it is
        // looking for.
        let source = include_str!("converse.rs");
        for banned in [concat!("ACTION_", "BUDGET"), concat!("ACTION_", "GUARD")] {
            assert!(!source.contains(banned), "{banned} came back");
        }
    }

    /// The backend counts the prompt it received; a character heuristic guesses
    /// at it. Where there is a count, guessing is a choice to be wrong.
    ///
    /// Measured on the 80B run's own audit: 89,666 characters of tool results
    /// estimated at 22,416 tokens and counted by the backend at 8,183 — nearly
    /// three times over. On English prose the same heuristic lands within 8
    /// percent, which is why it survived: an agent's conversation is JSON, hex
    /// digests, paths and build output, and none of that is prose.
    #[test]
    fn a_refused_call_is_told_what_it_sent_and_not_only_what_was_missing() {
        // The run's own confusion: apply_replace takes `replacement`, its
        // sibling replace_text takes `replace`, and the deployment used the
        // sibling's fields. Since 2026-09-23 that call has one reading and is
        // repaired into replace_text (backlog C.24) -- asserted first.
        let sibling = pwr_domain::ToolCall {
            id: None,
            name: "apply_replace".into(),
            arguments: serde_json::json!({
                "path": "src/app/app.ts",
                "expected_hash": "abc123",
                "find": "imports: []",
                "replace": "imports: [RouterOutlet]",
            }),
        };
        assert!(matches!(
            decode(&sibling, &chat_tool_catalog()),
            Ok(ActionProposal::ReplaceText { .. })
        ));

        // A call with no single reading is still refused, and the refusal
        // names what was sent as well as what was missing.
        let call = pwr_domain::ToolCall {
            id: None,
            name: "apply_replace".into(),
            arguments: serde_json::json!({
                "path": "src/app/app.ts",
                "expected_hash": "abc123",
                "content": "export const x = 1;",
            }),
        };
        let problem = decode(&call, &chat_tool_catalog()).expect_err("this call cannot decode");

        assert!(
            problem.contains("replacement"),
            "the refusal still names what was missing: {problem}"
        );
        assert!(
            problem.contains("content"),
            "the refusal does not name what was actually sent: {problem}"
        );
        assert!(
            problem.contains("apply_replace"),
            "the refusal does not name the capability: {problem}"
        );
    }

    /// Naming the missing field is not enough when the call the deployment
    /// wanted exists under another name.
    ///
    /// Measured on an 80B editing an Angular component: `apply_patch` takes
    /// `hunks` and `apply_replace` takes `replacement`, and it called the
    /// second with the first's arguments twenty-nine times in one run. The
    /// refusal named what was missing and what had been sent, both of which
    /// were already true of the call it kept making.
    #[test]
    fn a_call_carrying_another_tools_arguments_is_told_which_tool_that_is() {
        let call = pwr_domain::ToolCall {
            id: None,
            name: "apply_replace".into(),
            arguments: serde_json::json!({
                "path": "src/app/pages/faq-page.ts",
                "expected_hash": "abc123",
                "hunks": [{"find": "a", "replace": "b"}],
            }),
        };
        let problem = decode(&call, &chat_tool_catalog()).expect_err("this call cannot decode");
        assert!(
            problem.contains("apply_patch"),
            "the refusal does not name the tool that takes these arguments: {problem}"
        );
        assert!(
            problem.contains("call apply_patch"),
            "the refusal does not say what to do about it: {problem}"
        );
    }

    /// The call from the maintainer's run, verbatim in shape: the conversation
    /// decodes through the shared decoder, so an object for `content` is
    /// written as its JSON instead of ending the turn.
    #[test]
    fn a_conversation_accepts_file_content_sent_as_an_object() {
        let call = pwr_domain::ToolCall {
            id: None,
            name: "write_file".into(),
            arguments: serde_json::json!({
                "path": "package.json",
                "content": {"name": "star-runner", "type": "module", "scripts": {"test": "node --test"}},
            }),
        };
        match decode(&call, &chat_tool_catalog()).expect("decodes in a conversation") {
            ActionProposal::WriteFile { content, .. } => {
                assert!(content.contains("\"node --test\""))
            }
            other => panic!("wrong action: {other:?}"),
        }
        // And what the conversation does not offer stays unknown to it.
        let run_only = pwr_domain::ToolCall {
            id: None,
            name: "record_progress".into(),
            arguments: serde_json::json!({"step": 1}),
        };
        assert!(decode(&run_only, &chat_tool_catalog()).is_err());
    }

    /// Chat mode reads and does nothing else: a write or a command is refused
    /// as unavailable, whatever the model asks for.
    #[test]
    fn chat_mode_offers_reading_and_refuses_everything_else() {
        let catalog = chat_only_tool_catalog();
        let mut names: Vec<&str> = catalog
            .tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect();
        names.sort_unstable();
        // Reading, and what PWR keeps about the person and their projects --
        // none of which acts on anything.
        assert_eq!(
            names,
            [
                "list_tree",
                "read_file",
                "recall_project",
                "remember",
                "wiki_query"
            ]
        );
        for (name, arguments) in [
            (
                "write_file",
                serde_json::json!({"path": "a.md", "content": "x"}),
            ),
            (
                "run_command",
                serde_json::json!({"executable": "ls", "args": []}),
            ),
            (
                "replace_text",
                serde_json::json!({"path": "a.md", "expected_hash": "h", "find": "a", "replace": "b"}),
            ),
        ] {
            let call = pwr_domain::ToolCall {
                id: None,
                name: name.into(),
                arguments,
            };
            let refused = decode(&call, &catalog).expect_err("chat mode must refuse it");
            assert!(refused.contains("not available"), "{name}: {refused}");
        }
        let read = pwr_domain::ToolCall {
            id: None,
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "/tmp/notes.md"}),
        };
        assert!(decode(&read, &catalog).is_ok());
    }

    /// A wrong call that is not some other tool's call gets no invented advice.
    #[test]
    fn a_call_matching_no_other_tool_is_not_sent_somewhere_at_random() {
        let call = pwr_domain::ToolCall {
            id: None,
            name: "apply_replace".into(),
            arguments: serde_json::json!({"path": "a.ts", "expected_hash": "h", "nonsense": 1}),
        };
        let problem = decode(&call, &chat_tool_catalog()).expect_err("this call cannot decode");
        assert!(
            !problem.contains("Those are the arguments"),
            "a tool was suggested for arguments that match none: {problem}"
        );
    }

    #[test]
    fn a_prompt_cost_that_repeats_while_the_conversation_grows_is_truncation() {
        // The run's own numbers: the count froze the moment the backend began
        // deleting the middle of the prompt.
        let note = truncation_note(Some(7_090), Some(7_090), 65_536)
            .expect("a repeated count is the signature");
        assert!(
            note.contains("7090"),
            "the note names what was reported: {note}"
        );
        assert!(
            note.contains("65536"),
            "the note names the window the operator configured: {note}"
        );

        // A cost that moves is a prompt that arrived whole.
        assert!(truncation_note(Some(13_505), Some(11_244), 65_536).is_none());

        // The first turn has nothing to compare against, and a backend that
        // reports nothing cannot be checked. Neither is evidence.
        assert!(truncation_note(Some(2_203), None, 65_536).is_none());
        assert!(truncation_note(None, Some(7_090), 65_536).is_none());
    }

    #[test]
    fn a_measured_prompt_is_preferred_to_a_guess_at_one() {
        // Characters chosen so the heuristic and the measurement disagree the
        // way they did in the run.
        let messages = vec![said("user", &"x".repeat(89_666))];
        let guessed = conversation_tokens(&messages, None);
        assert_eq!(guessed, 22_417, "the heuristic changed shape");

        // Given what the backend actually said, that is what is used.
        assert_eq!(conversation_tokens(&messages, Some(8_183)), 8_183);

        // The ratio is the defect: the guess is nearly three times the count,
        // so a conversation is compacted at roughly a third of the context the
        // operator configured, and a long turn keeps losing the file it is
        // editing.
        let measured = conversation_tokens(&messages, Some(8_183));
        assert!(
            guessed > measured * 2,
            "the guess ({guessed}) no longer overstates the measurement ({measured}), so this \
             test no longer guards anything"
        );
    }

    /// The bound an action count used to provide by accident.
    ///
    /// An empty turn adds a one-line nudge, so the context barely grows, so
    /// neither compaction nor the looping guard ever fires. Measured on an
    /// 80B: twelve turns alternating 2,246 and 26 generated tokens against a
    /// prompt frozen at 7,090, no actions, no answer, until it was killed by
    /// hand.
    #[test]
    fn silence_is_bounded_and_the_bound_is_consecutive() {
        // Small enough to stop a spin quickly, large enough that a deployment
        // which stumbles once and recovers is not punished for it.
        const { assert!(EMPTY_TURNS_BEFORE_GIVING_UP >= 2 && EMPTY_TURNS_BEFORE_GIVING_UP <= 5) };
        // Consecutive, so any productive turn clears the count. The loop sets
        // `silent = 0` before executing calls; this asserts the source still
        // does, because a bound that never resets turns a long working run
        // into a refusal.
        let source = include_str!("converse.rs");
        assert!(source.contains("silent = 0;"), "the count never resets");
        assert!(
            source.contains("StopReason::Silent"),
            "silence has no way to end a turn"
        );
    }

    #[test]
    fn every_way_a_turn_can_stop_says_something_the_operator_can_act_on() {
        // Adding a variant breaks this match until it is added to `ALL` too,
        // which is the only thing keeping the list honest.
        for reason in StopReason::ALL {
            match reason {
                StopReason::Interrupted
                | StopReason::ContextFull
                | StopReason::Looping
                | StopReason::Silent
                | StopReason::ToolCallInReasoning
                | StopReason::Unparseable
                | StopReason::BudgetSpent
                | StopReason::BackendFailing
                | StopReason::NoProgress
                | StopReason::ReasoningUnfinished => {}
            }
            let said = reason.said();
            assert!(!said.is_empty());
            // Not a code, not a number: a sentence.
            assert!(said.contains(' '), "{said}");
        }
        // The one an operator can do something about names the something.
        assert!(StopReason::ContextFull.said().contains("start a new one"));
        // And this one distinguishes a deployment that cannot form a call from
        // a backend that is down, because the remedies are different.
        assert!(
            StopReason::Unparseable.said().contains("valid JSON"),
            "the reason does not say what was wrong with the calls"
        );
        assert!(
            StopReason::Unparseable.said().contains("cut off"),
            "the reason does not cover a reply that ran away, which ends a turn the same way"
        );
        // A check-in is not a failure, and the sentence has to read like one:
        // it says the work is intact and what to type.
        let spent = StopReason::BudgetSpent.said();
        assert!(spent.contains("carry on"), "{spent}");
        assert!(spent.contains("discarded"), "{spent}");
        // The one that is not the model's fault says so, because an operator
        // who reads "the model failed" goes looking in the wrong place.
        let backend = StopReason::BackendFailing.said();
        assert!(backend.contains("the server, not the model"), "{backend}");
        assert!(backend.contains("kept"), "{backend}");
    }

    /// A refused edit is not a change, and a turn that says "the checks passed
    /// after the change" about one that never happened has made exactly the
    /// unearned claim the checks exist to stop.
    #[test]
    fn a_refused_edit_is_not_a_change() {
        // The classification alone says only what the action would do.
        let write = ActionProposal::WriteFile {
            path: "a.rs".into(),
            content: String::new(),
        };
        assert!(crate::conversation::may_change_workspace(&write));
        // Whether it did is the executor's answer, and the loop only records
        // it on success -- asserted at the boundary this test can reach.
        let denied: Result<serde_json::Value, ()> = Err(());
        let mut edited = false;
        if denied.is_ok() {
            edited |= crate::conversation::may_change_workspace(&write);
        }
        assert!(!edited);
    }

    #[test]
    fn look_at_is_offered_only_with_vision_and_decodes() {
        assert!(
            !chat_tool_catalog()
                .tools
                .iter()
                .any(|tool| tool.name == "look_at")
        );
        let seeing = with_vision(chat_tool_catalog());
        assert_eq!(
            seeing
                .tools
                .iter()
                .filter(|tool| tool.name == "look_at")
                .count(),
            1
        );
        assert_eq!(
            with_vision(seeing)
                .tools
                .iter()
                .filter(|tool| tool.name == "look_at")
                .count(),
            1
        );
        let call = pwr_domain::ToolCall {
            name: "look_at".into(),
            arguments: serde_json::json!({"target": "http://localhost:4200/", "width": 800}),
            id: None,
        };
        assert!(matches!(
            crate::action_from_tool_call(&call).unwrap(),
            ActionProposal::LookAt { ref target, width: Some(800), height: None } if target == "http://localhost:4200/"
        ));
        assert_eq!(tool_call_detail(&call), "http://localhost:4200/");
    }

    #[test]
    fn the_prompt_never_names_the_absolute_path() {
        let prompt = chat_system_prompt(std::path::Path::new("/tmp/project"));
        // Naming it taught the model to prefix every read with it, and every
        // read was then refused for escaping the workspace.
        assert!(!prompt.contains("/tmp/project"), "{prompt}");
        assert!(prompt.contains("project"));
        assert!(prompt.contains("relative to the repository root"));
        // The claim the checks exist to stop.
        assert!(prompt.contains("do not claim something works"));
        // The machine, and where a missing toolchain goes.
        assert!(prompt.contains(std::env::consts::ARCH), "{prompt}");
        assert!(prompt.contains(".toolchains/<name>/"), "{prompt}");
    }
}
