//! The session executor: one place that decides what a turn, a goal and the
//! checks around them add up to.
//!
//! The conversation turn, Goal mode and the checks that close a turn used to be
//! sequenced in the command-line crate -- `main.rs` for the turn and its
//! checks, `serve.rs` for the goal loop -- so a new interface or a benchmark
//! had to rebuild what "complete" means by hand, and the loops that were
//! measured were not the ones that shipped (technical review of 2026-09-30,
//! §2, §3.1). The executor owns the sequencing, the limits, the acceptance
//! decisions and the shape of the result; a [`SessionHost`] supplies what only
//! the front end has: the session, the notification channel, the model turn
//! itself and the repository checks.
//!
//! What it returns is a [`SessionEnd`], never a JSON reply: how an ending is
//! shown -- an ACP response, a console line, a campaign record -- is the
//! caller's business, and its meaning is not.

use crate::ApprovalPrompt;
use crate::converse::{self, TurnReport, TurnStep};
use pwr_domain::ChatMessage;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Where a turn's steps go: shared, because a goal runs several turns under
/// one prompt and a step sink is the client's.
pub type SharedStepSink = Rc<RefCell<Box<dyn FnMut(TurnStep)>>>;

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
    /// What the person allowed for the rest of the session, as it stands:
    /// shared rather than copied, so a grant made during the turn reaches the
    /// checks that close it.
    pub session_grants: Arc<Mutex<Vec<pwr_tools::Approval>>>,
    /// Goal mode keeps a single task moving across ordinary turn checkpoints.
    /// The runner still owns completion evidence; the server owns the bounded
    /// continuation policy and the operator's stop control.
    pub goal_mode: bool,
    /// One action PWR decided itself, taken through the turn as if the model
    /// had called it. The proposals phase (W2.9) applies and restores a file
    /// this way, so that the policy, the person's approval and the record are
    /// the turn's own and there is no second way to write to a workspace.
    pub scripted: Option<ScriptedTurn>,
}

/// The one call a scripted turn makes, and what it says once it has.
#[derive(Debug, Clone)]
pub struct ScriptedTurn {
    pub call: pwr_domain::ToolCall,
    pub said: String,
}

/// A provider that answers once with a call PWR decided, then with nothing
/// more to do. The proposals phase applies and restores a file through it, so
/// the call meets the policy, the person and the record exactly as a model's
/// own would: there is one way to write to a workspace.
pub struct Scripted<P> {
    inner: P,
    turn: Mutex<Option<ScriptedTurn>>,
}

impl<P> Scripted<P> {
    /// `inner` answers as itself when there is nothing scripted.
    pub fn new(inner: P, turn: Option<ScriptedTurn>) -> Self {
        Self {
            inner,
            turn: Mutex::new(turn),
        }
    }
}

#[async_trait::async_trait]
impl<P: pwr_provider::ModelProvider> pwr_provider::ModelProvider for Scripted<P> {
    async fn inspect(
        &self,
        deployment: &pwr_domain::DeploymentDescriptor,
    ) -> Result<pwr_domain::ModelInspection, pwr_provider::ProviderError> {
        self.inner.inspect(deployment).await
    }
    async fn runtime_state(&self) -> Result<pwr_domain::BackendState, pwr_provider::ProviderError> {
        self.inner.runtime_state().await
    }
    async fn prepare_context(
        &self,
        deployment: &pwr_domain::DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, pwr_provider::ProviderError> {
        // Nothing is generated: no model needs loading or resizing for it.
        let scripted = self.turn.lock().is_ok_and(|turn| turn.is_some());
        if scripted {
            return Ok(context_tokens);
        }
        self.inner.prepare_context(deployment, context_tokens).await
    }
    async fn chat(
        &self,
        request: pwr_domain::ModelRequest,
    ) -> Result<pwr_provider::ModelStream, pwr_provider::ProviderError> {
        // `None` from the start is an ordinary turn.
        let scripted = match self.turn.lock() {
            Ok(mut turn) => turn.as_mut().map(|turn| {
                let call = (!turn.call.name.is_empty()).then(|| turn.call.clone());
                turn.call.name.clear();
                (call, turn.said.clone())
            }),
            Err(_) => None,
        };
        let Some((call, said)) = scripted else {
            return self.inner.chat(request).await;
        };
        let chunk = pwr_domain::ModelChunk {
            content: if call.is_some() { String::new() } else { said },
            tool_calls: call.into_iter().collect(),
            done: true,
            ..Default::default()
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
    /// The model's own way of abandoning a reply, when nothing is scripted.
    ///
    /// Every turn's provider is wrapped in this type, and the default
    /// `chat_cancellable` only drops the stream: without this the MLX engine's
    /// own cancellation -- which tells the sidecar to stop, during prefill too
    /// (W5.1) -- was bypassed for every ordinary turn from the commit that
    /// introduced the wrapper until this one.
    async fn chat_cancellable(
        &self,
        request: pwr_domain::ModelRequest,
        cancel: pwr_provider::Cancel,
    ) -> Result<pwr_provider::ModelStream, pwr_provider::ProviderError> {
        let scripted = self.turn.lock().is_ok_and(|turn| turn.is_some());
        if scripted {
            return self.chat(request).await;
        }
        self.inner.chat_cancellable(request, cancel).await
    }
}

/// Evidence the core gathered after a model declared a goal complete.
#[derive(Clone, Default)]
pub struct GoalVerification {
    pub checks: Option<pwr_domain::ChecksOutcome>,
    pub contract_changed: Vec<String>,
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
    /// Stable identities of the failures, rather than just check commands.
    pub failure_fingerprints: Vec<String>,
    /// Those of `failing` the workspace declares as acceptance checks.
    pub failing_acceptance: Vec<String>,
    /// The failing tests by name, where the checks' output names them
    /// ([`pwr_verify::failure`]): what a proposal is judged by.
    pub failed_tests: std::collections::BTreeSet<String>,
    /// What the failing checks printed, shortened: what a proposal is shown.
    pub evidence: String,
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
pub fn already_failing_note(failing: &[String]) -> String {
    format!(
        "Before this goal started, the core ran the repository's checks and these were already failing: {}. \
         Repair failures covered by the engineer's request; acceptance checks must still pass. \
         Do not repair unrelated pre-existing failures or rebuild their configuration or dependencies \
         unless the engineer asks. Report any unrelated failures left unchanged; their baseline status \
         does not mean the requested work is complete.",
        failing.join(", ")
    )
}

pub const GOAL_MAX_ACTIONS: usize = 208;
/// Completions the checks may refuse before a goal stops, however the failing
/// checks change from one to the next. The same-failure limit below catches a
/// wall hit three times; this catches a goal that alternates between two.
pub const GOAL_MAX_REFUSED_COMPLETIONS: usize = 6;
/// The most wall-clock time one goal may take. A goal runs unattended between
/// check-ins, and nothing else bounded how long: a slow model with slow checks
/// spends the hour before the action count says anything.
pub const GOAL_MAX_WALL: std::time::Duration = std::time::Duration::from_secs(60 * 60);
/// How long a turn the deadline asked to stop has to return its history. A
/// turn watches Stop every 50 ms around generation, permissions and commands
/// (a killed command, a cancelled stream); this is for the case where it does
/// not, so the deadline still holds.
pub const GOAL_STOP_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// What bounds a goal, whichever branch of its loop it is going round.
///
/// The action limit used to be tested only when a turn did not end in a
/// completion. A goal whose completions were all refused, with a failing set
/// that alternated so the same-failure count never reached its limit, never
/// took that branch and had no limit at all (technical review of 2026-09-30,
/// verified the same day). Every limit is now tested before each turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GoalLimits {
    pub actions: usize,
    pub refused_completions: usize,
    pub verification_runs: usize,
    pub review_rounds: usize,
    /// Files PWR may ask the model for, one at a time, before the first turn
    /// (W2.9). Absent, the workspace has not said: the front end takes the
    /// model's profile, which is off unless it was measured to help that
    /// model. Zero is off whatever the profile says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposals: Option<usize>,
    /// Model work the goal may do, in generated tokens, a prompt token read
    /// counting an eighth ([`converse::WORK_PER_TOKEN`]). Absent, the default,
    /// is no such limit: the wall clock bounds the goal as before. Set, it
    /// bounds the goal the same on a fast machine and a slow one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work: Option<u64>,
    #[serde(with = "goal_seconds")]
    pub wall: std::time::Duration,
}

impl Default for GoalLimits {
    fn default() -> Self {
        Self {
            actions: GOAL_MAX_ACTIONS,
            refused_completions: GOAL_MAX_REFUSED_COMPLETIONS,
            verification_runs: 9, // baseline + six refusals + passing checks before/after review
            review_rounds: 1,
            proposals: None,
            work: None,
            wall: GOAL_MAX_WALL,
        }
    }
}

/// A limit a goal reached, and how much of it was spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalLimitReached {
    Actions {
        spent: usize,
        allowed: usize,
    },
    RefusedCompletions {
        spent: usize,
        allowed: usize,
    },
    Time {
        spent_secs: u64,
        allowed_secs: u64,
    },
    VerificationRuns {
        spent: usize,
        allowed: usize,
    },
    ReviewRounds {
        spent: usize,
        allowed: usize,
    },
    /// Generated-token equivalents of model work.
    Work {
        spent: u64,
        allowed: u64,
    },
}

impl GoalLimits {
    /// The first limit `spent` has reached, in the order a person would want it
    /// named: time, then refused completions, then actions.
    pub fn reached(
        &self,
        actions: usize,
        refused_completions: usize,
        elapsed: std::time::Duration,
    ) -> Option<GoalLimitReached> {
        if elapsed >= self.wall {
            return Some(GoalLimitReached::Time {
                spent_secs: elapsed.as_secs(),
                allowed_secs: self.wall.as_secs(),
            });
        }
        if refused_completions >= self.refused_completions {
            return Some(GoalLimitReached::RefusedCompletions {
                spent: refused_completions,
                allowed: self.refused_completions,
            });
        }
        if actions >= self.actions {
            return Some(GoalLimitReached::Actions {
                spent: actions,
                allowed: self.actions,
            });
        }
        None
    }
}

impl GoalLimitReached {
    /// What the person is told, and what a client reads in `_meta.pwr.budget`.
    pub fn said(self) -> String {
        match self {
            Self::VerificationRuns { spent, .. } => format!(
                "Goal mode paused after {spent} verification runs. Review the current changes before continuing."
            ),
            Self::ReviewRounds { spent, .. } => format!(
                "Goal mode paused after {spent} review rounds. Review the current changes before continuing."
            ),
            Self::Actions { spent, .. } => format!(
                "Goal mode paused after {spent} actions without verified completion. Review the current changes, then continue deliberately if the objective still needs work."
            ),
            Self::RefusedCompletions { spent, .. } => format!(
                "Goal mode paused: the work was declared complete {spent} times and verification refused it each time, Review the checks' output, then continue deliberately."
            ),
            Self::Work { spent, .. } => format!(
                "Goal mode paused after about {spent} tokens of model work without verified completion. Review the current changes, then continue deliberately if the objective still needs work."
            ),
            Self::Time { spent_secs, .. } => format!(
                "Goal mode paused after {} minutes without verified completion. Review the current changes, then continue deliberately if the objective still needs work.",
                spent_secs / 60
            ),
        }
    }

    pub fn meta(self) -> Value {
        match self {
            Self::VerificationRuns { spent, allowed } => {
                json!({"limit": "verification_runs", "spent": spent, "allowed": allowed})
            }
            Self::ReviewRounds { spent, allowed } => {
                json!({"limit": "review_rounds", "spent": spent, "allowed": allowed})
            }
            Self::Actions { spent, allowed } => {
                json!({"limit": "actions", "spent": spent, "allowed": allowed})
            }
            Self::RefusedCompletions { spent, allowed } => {
                json!({"limit": "refused_completions", "spent": spent, "allowed": allowed})
            }
            Self::Work { spent, allowed } => {
                json!({"limit": "work", "spent": spent, "allowed": allowed})
            }
            Self::Time {
                spent_secs,
                allowed_secs,
            } => {
                json!({"limit": "time", "spentSeconds": spent_secs, "allowedSeconds": allowed_secs})
            }
        }
    }
}
// Configuration uses whole seconds rather than serde's Duration object.
mod goal_seconds {
    pub fn serialize<S: serde::Serializer>(
        value: &std::time::Duration,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.as_secs())
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<std::time::Duration, D::Error> {
        <u64 as serde::Deserialize>::deserialize(deserializer).map(std::time::Duration::from_secs)
    }
}

pub struct GoalBudget {
    pub limits: GoalLimits,
    pub started: tokio::time::Instant,
    pub actions: usize,
    pub refused: usize,
    pub verifications: usize,
    pub reviews: usize,
    pub proposals: usize,
    /// The goal's work meter, shared with its turns.
    pub work: Arc<std::sync::atomic::AtomicU64>,
}
impl GoalBudget {
    pub fn new(limits: GoalLimits) -> Self {
        Self {
            limits,
            started: tokio::time::Instant::now(),
            actions: 0,
            refused: 0,
            verifications: 0,
            reviews: 0,
            proposals: 0,
            work: Arc::default(),
        }
    }
    pub fn reached(&self) -> Option<GoalLimitReached> {
        self.limits
            .reached(self.actions, self.refused, self.started.elapsed())
            .or_else(|| {
                let allowed = self.limits.work?;
                let spent = self.work_done();
                (spent >= allowed).then_some(GoalLimitReached::Work { spent, allowed })
            })
    }
    /// Model work done so far, in generated-token equivalents.
    pub fn work_done(&self) -> u64 {
        self.work.load(Ordering::Relaxed) / converse::WORK_PER_TOKEN
    }
    /// What is left of the work allowance, when there is one.
    pub fn work_left(&self) -> Option<u64> {
        self.limits
            .work
            .map(|allowed| allowed.saturating_sub(self.work_done()))
    }
    pub fn time_limit(&self) -> GoalLimitReached {
        GoalLimitReached::Time {
            spent_secs: self.started.elapsed().as_secs(),
            allowed_secs: self.limits.wall.as_secs(),
        }
    }
    pub fn remaining(&self) -> std::time::Duration {
        self.limits.wall.saturating_sub(self.started.elapsed())
    }
    pub fn snapshot(&self) -> Value {
        json!({"limits": self.limits, "spent": {"actions": self.actions, "refused_completions": self.refused, "verification_runs": self.verifications, "review_rounds": self.reviews, "proposals": self.proposals, "work": self.work_done(), "wall_seconds": self.started.elapsed().as_secs()}})
    }
}

/// Check-ins in a row that took no action before a goal pauses as stalled.
/// Measured 2026-09-26: a model that could not run its toolchain answered in
/// prose, and the goal re-prompted it 57 times in 23 minutes, the action count
/// standing still at 20 -- far below [`GOAL_MAX_ACTIONS`], so nothing stopped it.
pub const GOAL_IDLE_LIMIT: usize = 3;
/// Completions in a row that verification refuses with the same failing checks
/// before a goal stops as blocked: the same wall three times is the
/// environment or the task, not a step the next attempt will take.
pub const GOAL_SAME_FAILURE_LIMIT: usize = 3;
/// Actions between two checkpoint notices; a notice per check-in made the
/// conversation a column of them while a run was stuck.
pub const GOAL_NOTICE_EVERY: usize = 10;

/// What a goal is asked once, the first time its checks pass: to hold the
/// work against the request before calling it done.
///
/// Measured on the stack matrix, 2026-09-26: three of four failed tasks had
/// every declared check green and broke a rule the request stated plainly and
/// no visible test covered -- "blank lines are ignored", "copies above N are
/// deleted". A model makes the checks pass and stops; the checks are rarely
/// the whole request.
pub const GOAL_REVIEW: &str = "The checks pass. Before finishing, hold the work against what was \
    asked, because checks rarely cover every rule: re-read the request and any specification \
    it points to (a README, a spec file), go through each rule it states, and for each one \
    find where the code does it. A rule no check exercises is only known to work once it has \
    run: try it -- a short script, or a test file of your own that gives the same result \
    on any machine (not on this one's time zone, locale or paths) -- rather than trusting a \
    reading of the code. Where the request names something -- an image, a version, a \
    library, a file and where it goes -- the work must use exactly that; something else that \
    behaves the same is not what was asked. Fix any rule that is missing or wrong, run the \
    checks again, then finish. If a named thing could not be used, say so and why instead of \
    counting a substitute as done. If every rule is met, finish and say so.";

/// What a reviewer that has not seen the conversation reads: the person's
/// requests, the README, and the source this session changed -- tests left
/// out, each file and the whole bounded. `None` when there is no source.
///
/// Why a second reader. Stack matrix c2 (2026-09-27): three of the first ten
/// tasks failed on a rule the README states plainly -- copies above N deleted,
/// fields separated by any whitespace, `opts[:name]` registering the process --
/// each after a review round in which the model listed that very rule as
/// verified. The model that wrote the code reads it the way it meant it.
pub fn review_prompt(
    root: &Path,
    requests: &str,
    changed: &BTreeMap<String, String>,
) -> Option<String> {
    const FILE_CHARS: usize = 20_000;
    const CODE_CHARS: usize = 60_000;
    const SPEC_CHARS: usize = 16_000;
    let mut code = String::new();
    let mut left_out = 0usize;
    for path in changed.keys().filter(|path| reviewable(path)) {
        let Ok(text) = std::fs::read_to_string(root.join(path)) else {
            continue;
        };
        let shown: String = text.chars().take(FILE_CHARS).collect();
        if code.len() + shown.len() > CODE_CHARS {
            left_out += 1;
            continue;
        }
        code.push_str(&format!("--- {path} ---\n{shown}\n"));
        if shown.len() < text.len() {
            code.push_str("(the rest of this file is not shown)\n");
        }
    }
    if code.is_empty() {
        return None;
    }
    if left_out > 0 {
        code.push_str(&format!("({left_out} more changed file(s) not shown)\n"));
    }
    let spec = std::fs::read_to_string(root.join("README.md"))
        .map(|text| text.chars().take(SPEC_CHARS).collect::<String>())
        .unwrap_or_else(|_| "(there is no README.md)".into());
    Some(format!(
        "The request:\n{requests}\n\nThe specification (README.md):\n{spec}\n\n\
         The code as it is now:\n{code}\n\
         Go through the specification and the request rule by rule -- every option, error case, \
         input form and edge case they state -- and write one line per rule:\n\
         - <the rule> -- MET: <file and function that does it>\n\
         or\n\
         - <the rule> -- NOT MET: <what the code does instead>\n\
         Judge each rule against the code as it is written, reading the lines that would do it, \
         not against what the code seems meant to do. Write only the list."
    ))
}

/// Source a reviewer should read: not tests, not anything under a hidden
/// directory (PWR's state, toolchains, scratch).
pub fn reviewable(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').collect();
    let (name, directories) = segments.split_last().unwrap_or((&"", &[]));
    let name = name.to_ascii_lowercase();
    !directories.iter().any(|segment| {
        segment.starts_with('.')
            || matches!(
                *segment,
                "test" | "tests" | "spec" | "__tests__" | "node_modules"
            )
    }) && !name.contains("_test.")
        && !name.contains(".test.")
        && !name.contains(".spec.")
        && !name.starts_with("test_")
        && !name.ends_with(".lock")
        && name != "package-lock.json"
}

/// The person's own requests in this conversation, latest last, bounded.
pub fn person_requests(messages: &[ChatMessage]) -> String {
    let requests: Vec<String> = messages
        .iter()
        .filter(|message| {
            message.role == "user"
                && matches!(
                    message.purpose,
                    None | Some(pwr_domain::MessagePurpose::Task)
                )
        })
        .map(|message| {
            let text = message.content.as_str();
            text.split_once("\n\nGoal mode is enabled.")
                .map_or(text, |(request, _)| request)
                .trim()
                .to_owned()
        })
        .filter(|text| !text.is_empty())
        .collect();
    let joined = requests.join("\n---\n");
    let skip = joined.chars().count().saturating_sub(4_000);
    joined.chars().skip(skip).collect()
}

/// The review round's message, with the rules the reviewer marked NOT MET.
///
/// A checklist, not a question: asked instead to list what the code does not
/// do "or answer NO DISCREPANCIES", the same model answered that in three
/// seconds for all seven c2 workspaces probed, five of which had failed on a
/// rule their README states. Made to mark every rule, it found them.
pub fn review_guidance(findings: Option<&str>) -> String {
    let unmet: Vec<&str> = findings
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| line.contains("NOT MET"))
        .take(12)
        .collect();
    match unmet.as_slice() {
        [] => GOAL_REVIEW.to_owned(),
        lines => {
            let found: String = lines.join("\n").chars().take(4_000).collect();
            format!(
                "{GOAL_REVIEW}\n\nA reviewer who has not seen this conversation read the \
                 specification against the code as it is now and marked these rules not met:\n\n\
                 {found}\n\n\
                 It can be wrong. For each point, read the rule and the code: fix what is really \
                 missing or wrong, and try it; leave what is not."
            )
        }
    }
}

/// A message goal mode writes between its own turns, marked as the
/// harness's so it is not composed, replayed or summarised as a request.
pub fn goal_guidance(text: impl Into<String>) -> ChatMessage {
    let mut message = ChatMessage::text("user", text);
    message.purpose = Some(pwr_domain::MessagePurpose::GoalGuidance);
    message
}

/// Help a goal is given beyond being run. Each is measured on its own and off
/// unless the workspace asks for it (`goal_aids` in `.pwr/chat-config.json`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GoalAids {
    /// Tell the goal, in its first request, which files its failing
    /// acceptance tests use ([`crate::proposals::pointers`]; plan W2.10).
    pub pointers: bool,
    /// Offer a goal's turns the ten tools of [`converse::CORE_TOOLS`] instead
    /// of the whole catalogue (plan W2.11). Read by the front end, which
    /// builds the catalogue.
    pub core_tools: bool,
    /// Ask the proposals phase for edit blocks instead of the whole file
    /// when the file exists ([`crate::proposals::Transport`]; plan W2.12).
    pub block_edits: bool,
    /// Let the model keep a plan with `update_plan` and show it the plan
    /// before every reply ([`crate::board`]; plan W2.14).
    pub plan: bool,
    /// Reason at the person's level on the first step and after anything
    /// failed, and one level lower on the step after an action that worked
    /// ([`converse::paced_effort`]; plan W2.15).
    pub paced_reasoning: bool,
}

/// How a request is run: one turn, or turns repeated until the work is
/// verified, paused or out of budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Conversation,
    Goal,
    /// The W8.3 control: one turn under the goal's action and time budgets,
    /// with PWR's harness off and nothing verified (`converse::Harness`).
    Minimal,
}

/// A prompt to run, and what it runs with.
pub struct SessionRequest {
    pub root: PathBuf,
    pub conversation_id: pwr_domain::Id,
    pub messages: Vec<ChatMessage>,
    pub stop: Arc<AtomicBool>,
    pub steps: SharedStepSink,
    pub continuity: converse::Continuity,
    pub approvals: Arc<dyn ApprovalPrompt>,
    pub session_grants: Arc<Mutex<Vec<pwr_tools::Approval>>>,
    pub policy: Policy,
    pub aids: GoalAids,
}

/// Why the host could not verify.
#[derive(Debug)]
pub enum VerifyError {
    /// The session the request belongs to is gone.
    NoSuchSession,
    Failed(String),
}

/// What the executor needs from the front end that drives it.
#[async_trait::async_trait(?Send)]
pub trait SessionHost {
    /// One model turn -- composition, generation and tools -- with whatever the
    /// front end closes it with.
    async fn run_turn(&self, input: TurnInput) -> Result<(TurnReport, Vec<ChatMessage>), String>;
    /// The repository's full verification of the session as it stands, and the
    /// files the session changed, as they were when it ran.
    async fn verify(&self) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError>;
    /// A second reading of the work against its specification.
    async fn review(&self, root: &Path, prompt: String) -> Result<String, String>;
    /// A plain reply to `prompt` as it is generated, with no tools: what a
    /// proposed file is read from (W2.9). A front end without one has no
    /// proposals phase.
    async fn author(
        &self,
        _root: &Path,
        _prompt: String,
        _think: bool,
    ) -> Result<pwr_provider::ModelStream, String> {
        Err("this front end cannot ask for a proposal".into())
    }
    /// Says something to the person, between turns.
    fn say(&self, text: &str);
    /// The conversation as it now stands, for the session to keep.
    fn keep_messages(&self, messages: &[ChatMessage]);
}

/// How a request ended.
pub enum SessionEnd {
    /// The work ended in an answer, from one turn or after the goal's checks.
    Reply {
        report: TurnReport,
        total_actions: usize,
        goal: bool,
        verification: Option<GoalVerification>,
    },
    /// A goal that cannot finish by itself: `terminal` says why, `text` says it
    /// to the person.
    Stopped {
        report: TurnReport,
        total_actions: usize,
        terminal: &'static str,
        text: String,
    },
    /// A goal that reached one of its limits. The work stays where it is.
    OutOfBudget {
        total_actions: usize,
        reached: GoalLimitReached,
    },
    /// The request could not be run; `code` is JSON-RPC's.
    Error { code: i64, message: String },
}

/// The ending and what was spent getting there.
pub struct SessionResult {
    pub end: SessionEnd,
    pub budget: GoalBudget,
}

/// Runs a request to its end.
pub async fn execute<H: SessionHost + ?Sized>(
    host: &H,
    request: SessionRequest,
    limits: GoalLimits,
) -> SessionResult {
    let mut budget = GoalBudget::new(limits);
    // One meter for the goal: the turns count on the request's own.
    budget.work = Arc::clone(&request.continuity.work);
    budget.work.store(0, Ordering::Relaxed);
    let end = drive(host, request, &mut budget).await;
    SessionResult { end, budget }
}

/// What a completed goal's verification says about the turn's evidence.
pub fn apply_verification(outcome: &mut pwr_domain::TurnOutcome, verification: &GoalVerification) {
    outcome.checks = verification.checks.clone().unwrap_or_else(|| {
        if verification.technical_passed {
            pwr_domain::ChecksOutcome::Passed
        } else if !verification.failure_fingerprints.is_empty() {
            pwr_domain::ChecksOutcome::Failed {
                fingerprints: verification.failure_fingerprints.clone(),
            }
        } else {
            outcome.checks.clone()
        }
    });
    outcome.acceptance = if !verification.contract_changed.is_empty() {
        pwr_domain::AcceptanceOutcome::ContractChanged {
            what: verification.contract_changed.clone(),
        }
    } else if verification.passed {
        pwr_domain::AcceptanceOutcome::Accepted
    } else if verification.acceptance_available {
        pwr_domain::AcceptanceOutcome::Failed
    } else {
        pwr_domain::AcceptanceOutcome::NotDeclared
    };
    if !verification.contract_changed.is_empty() {
        outcome.terminal = pwr_domain::TurnTerminal::Blocked;
    }
}

/// The host's plain generation, as the proposals module asks for it.
struct HostAuthor<'a, H: SessionHost + ?Sized> {
    host: &'a H,
    root: &'a Path,
}

#[async_trait::async_trait(?Send)]
impl<H: SessionHost + ?Sized> crate::proposals::Author for HostAuthor<'_, H> {
    async fn write(
        &self,
        prompt: String,
        think: bool,
    ) -> Result<pwr_provider::ModelStream, String> {
        self.host.author(self.root, prompt, think).await
    }
}

/// The least and the most one proposal may take, in seconds, whatever share
/// of the goal's time falls to it.
const PROPOSAL_SHARE: (f64, f64) = (30.0, 240.0);
/// The same bounds in tokens, for a goal bounded by work: what a 9B model
/// generates in a second at full power on the machine these were set on.
const PROPOSAL_TOKENS_PER_SECOND: f64 = 40.0;
/// Proposals in a row with nothing kept before the phase gives way to the
/// ordinary goal: this many, or two for each file if that is more.
///
/// It was two passes over the files, which with one file is two proposals:
/// on a repository to be written from nothing the phase gave up with three
/// of its five still allowed (product path, 2026-10-06), where the same model
/// had needed two or three attempts at that file in the diagnostics.
const PROPOSAL_PATIENCE: usize = 4;
/// How long the checks may run on a proposed file: this many times what they
/// took at the baseline, within these bounds. A proposal can loop for ever --
/// the first one measured on the product path did (2026-10-06), and its
/// verification took 4 min 40 s of a ten-minute goal.
const PROPOSAL_CHECKS: (u32, std::time::Duration, std::time::Duration) = (
    10,
    std::time::Duration::from_secs(20),
    std::time::Duration::from_secs(300),
);

async fn drive<H: SessionHost + ?Sized>(
    host: &H,
    request: SessionRequest,
    budget: &mut GoalBudget,
) -> SessionEnd {
    let SessionRequest {
        root,
        conversation_id,
        mut messages,
        stop,
        steps,
        mut continuity,
        approvals,
        session_grants,
        policy,
        aids,
    } = request;
    // One plan for the goal, across its turns; a conversation keeps none.
    continuity.plan = (aids.plan && policy == Policy::Goal).then(Arc::default);
    continuity.paced_reasoning = aids.paced_reasoning && policy == Policy::Goal;
    let goal_mode = policy == Policy::Goal;
    let minimal = policy == Policy::Minimal;
    // The control spends the same budget a goal would, so the two arms of a
    // comparison stop at the same limits.
    let budgeted = goal_mode || minimal;
    let mut last_report: Option<TurnReport> = None;
    // Stop interrupts checks/review here. A turn handles Stop cooperatively so
    // it can return its transcript; dropping that future would lose its history,
    // so the turn has its own deadline handling below. Provider guards cancel
    // abandoned opening futures and streams.
    macro_rules! bounded {
        ($operation:expr) => {{
            if let Some(reached) = budget.reached() {
                return SessionEnd::OutOfBudget {
                    total_actions: budget.actions,
                    reached,
                };
            }
            let operation = tokio::select! {
                biased;
                () = converse::pressed(&stop) => {
                    let mut report = last_report.clone().unwrap_or(TurnReport {
                        outcome: Default::default(), answer: String::new(), actions: 0,
                        edited: false, completed: false, stopped: None, declined: false,
                    });
                    report.completed = false;
                    report.stopped = Some(converse::StopReason::Interrupted);
                    report.outcome.terminal = pwr_domain::TurnTerminal::Interrupted;
                    report.outcome.checks = pwr_domain::ChecksOutcome::CouldNotRun { why: "Goal operation interrupted by the operator".into() };
                    return SessionEnd::Reply { report, total_actions: budget.actions, goal: goal_mode, verification: None };
                }
                result = tokio::time::timeout(budget.remaining(), $operation) => result,
            };
            match operation {
                Ok(value) if budget.started.elapsed() < budget.limits.wall => value,
                _ => {
                    stop.store(true, Ordering::Relaxed);
                    return SessionEnd::OutOfBudget {
                        total_actions: budget.actions,
                        reached: budget.time_limit(),
                    };
                }
            }
        }};
    }
    macro_rules! verify {
        () => {{
            if let Some(reached) = budget.reached() {
                return SessionEnd::OutOfBudget {
                    total_actions: budget.actions,
                    reached,
                };
            }
            if budget.verifications >= budget.limits.verification_runs {
                return SessionEnd::OutOfBudget {
                    total_actions: budget.actions,
                    reached: GoalLimitReached::VerificationRuns {
                        spent: budget.verifications,
                        allowed: budget.limits.verification_runs,
                    },
                };
            }
            budget.verifications += 1;
            bounded!(host.verify())
        }};
    }
    let mut total_actions = 0usize;
    // Each turn numbers its calls from one, and a goal runs several turns
    // under one prompt: without an offset the second turn's first call
    // reused `turnN-call1`, and a client merged two different actions.
    let highest_call = Rc::new(std::cell::Cell::new(0u64));
    // The checks already failing when the goal starts, so the goal is
    // neither sent to repair them nor held open by them.
    let mut already_failing: Vec<String> = Vec::new();
    let baseline_started = tokio::time::Instant::now();
    let baseline = if goal_mode {
        verify!().ok().map(|(baseline, _)| baseline)
    } else {
        None
    };
    let baseline_took = baseline_started.elapsed();
    if let Some(baseline) = baseline
        .as_ref()
        .filter(|baseline| !baseline.failing.is_empty())
    {
        already_failing = baseline.failing.clone();
        if let Some(last) = messages.last_mut().filter(|last| last.role == "user") {
            last.content
                .push_str(&format!("\n\n{}", already_failing_note(&already_failing)));
        }
    }
    if aids.pointers
        && let Some(baseline) = baseline
            .as_ref()
            .filter(|baseline| !baseline.failing_acceptance.is_empty())
        && let Some(note) = crate::proposals::pointers(
            &crate::proposals::survey(&root, &baseline.evidence),
            |path| root.join(path).exists(),
        )
        && let Some(last) = messages.last_mut().filter(|last| last.role == "user")
    {
        last.content.push_str(&format!("\n\n{note}"));
    }
    let mut goal_edited = false;
    // One action of PWR's own, through a turn (see `TurnInput::scripted`).
    macro_rules! scripted {
        ($call:expr, $said:expr) => {{
            let base = highest_call.get();
            let mut turn_continuity = continuity.clone();
            if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
                checkpoint.actions = 0;
            }
            turn_continuity.action_limit =
                Some(budget.limits.actions.saturating_sub(total_actions));
            turn_continuity.work_limit = budget
                .limits
                .work
                .map(|work| work * converse::WORK_PER_TOKEN);
            bounded!(host.run_turn(TurnInput {
                root: root.clone(),
                conversation_id,
                messages: messages.clone(),
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
                continuity: turn_continuity,
                approvals: Arc::clone(&approvals),
                session_grants: Arc::clone(&session_grants),
                goal_mode,
                scripted: Some(ScriptedTurn {
                    call: $call,
                    said: $said,
                }),
            }))
        }};
    }
    // Verified proposals (W2.9): while an acceptance check fails, ask the
    // model for one file at a time, with no tools, and keep a file only when
    // fewer of the owner's tests fail with it. The ordinary goal follows with
    // what is left of the budget and decides the ending as it always has.
    let proposals_allowed = budget.limits.proposals.unwrap_or(0);
    if proposals_allowed > 0
        && let Some(baseline) = baseline
            .as_ref()
            .filter(|baseline| !baseline.failing_acceptance.is_empty())
    {
        use crate::proposals;
        let mut standing = proposals::Standing {
            failing: true,
            failures: baseline.failed_tests.clone(),
        };
        let mut evidence = baseline.evidence.clone();
        let contract = proposals::contract(&root);
        let mut refused: BTreeMap<String, (String, String, Vec<String>)> = BTreeMap::new();
        let mut kept: Vec<String> = Vec::new();
        let mut restored = 0usize;
        let mut refused_in_a_row = 0usize;
        let mut note: Option<String> = None;
        let author = HostAuthor { host, root: &root };
        'phase: while standing.failing && budget.proposals < proposals_allowed {
            let targets = proposals::survey(&root, &evidence);
            if targets.is_empty() {
                break;
            }
            let patience = PROPOSAL_PATIENCE.max(2 * targets.len());
            for target in targets {
                if !standing.failing || budget.proposals >= proposals_allowed {
                    break;
                }
                if refused_in_a_row >= patience {
                    break 'phase;
                }
                let left = (proposals_allowed - budget.proposals) as f64;
                // Until something says otherwise, this one is not kept.
                refused_in_a_row += 1;
                budget.proposals += 1;
                let path = target.path.clone();
                let on_disk = std::fs::read(root.join(&path)).ok();
                let current = on_disk
                    .as_deref()
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                    .unwrap_or_default();
                let was = refused
                    .get(&path)
                    .map(|(file, evidence, broke)| proposals::Refused {
                        file,
                        evidence,
                        broke,
                    });
                let transport = if aids.block_edits {
                    proposals::Transport::Blocks
                } else {
                    proposals::Transport::Whole
                };
                let brief = proposals::brief_for(
                    transport,
                    &path,
                    &contract,
                    &target.context,
                    &current,
                    &proposals::shown(&evidence, &target),
                    was.as_ref(),
                );
                // Half of what the goal has left is the ordinary goal's. In
                // tokens when the goal is bounded by work, so that a proposal
                // is the same proposal on a slower machine; in seconds when
                // it is bounded by the clock alone.
                let by_work = budget.limits.work.is_some();
                let share = match budget.work_left() {
                    Some(tokens) => (tokens as f64 / 2.0 / left).clamp(
                        PROPOSAL_SHARE.0 * PROPOSAL_TOKENS_PER_SECOND,
                        PROPOSAL_SHARE.1 * PROPOSAL_TOKENS_PER_SECOND,
                    ),
                    None => (budget.remaining().as_secs_f64() / 2.0 / left)
                        .clamp(PROPOSAL_SHARE.0, PROPOSAL_SHARE.1),
                };
                let shares = proposals::Shares {
                    thinking: share / 2.0,
                    reply: share,
                    // A file written from nothing is longer than a correction.
                    answer: if on_disk.is_some() {
                        share / 2.0
                    } else {
                        share
                    },
                };
                let proposal = match bounded!(proposals::propose(
                    &author,
                    &path,
                    &current,
                    brief,
                    shares,
                    transport,
                    proposals::Meter {
                        work: &budget.work,
                        by_work,
                    },
                    &stop
                )) {
                    Ok(proposal) => proposal,
                    Err(problem) => {
                        note = Some(format!("Proposals stopped: {problem}."));
                        break 'phase;
                    }
                };
                let Some(file) = proposal.file.filter(|file| *file != current) else {
                    continue;
                };
                let write = match &on_disk {
                    Some(bytes) => pwr_domain::ToolCall {
                        name: "apply_replace".into(),
                        arguments: json!({"path": path, "expected_hash": pwr_domain::hash_bytes(bytes), "replacement": file}),
                        id: None,
                    },
                    None => pwr_domain::ToolCall {
                        name: "write_file".into(),
                        arguments: json!({"path": path, "content": file}),
                        id: None,
                    },
                };
                let (report, next) = match scripted!(
                    write,
                    format!("Proposed {path}; checking it against the owner's tests.")
                ) {
                    Ok(turn) => turn,
                    Err(problem) => {
                        note = Some(format!("Proposals stopped: {problem}."));
                        break 'phase;
                    }
                };
                total_actions = total_actions.saturating_add(report.actions);
                budget.actions = total_actions;
                messages = next;
                host.keep_messages(&messages);
                let (declined, stopped, edited) =
                    (report.declined, report.stopped.is_some(), report.edited);
                last_report = Some(report);
                if declined || stopped {
                    note = Some("Proposals stopped: an edit was not allowed.".into());
                    break 'phase;
                }
                if !edited {
                    continue;
                }
                goal_edited = true;
                let allowed =
                    (baseline_took * PROPOSAL_CHECKS.0).clamp(PROPOSAL_CHECKS.1, PROPOSAL_CHECKS.2);
                // No verdict -- the checks did not finish, or could not run --
                // is not a pass: the file goes back like any other refusal.
                let after = match bounded!(tokio::time::timeout(allowed, host.verify())) {
                    Ok(Ok((after, _))) => Some(after),
                    _ => None,
                };
                let now = after.as_ref().map(|after| proposals::Standing {
                    failing: !after.failing.is_empty(),
                    failures: after.failed_tests.clone(),
                });
                if let (Some(after), Some(now)) = (after.as_ref(), now.as_ref())
                    && proposals::improves(&standing, now)
                {
                    host.say(&format!(
                        "Kept {path}: {} of the owner's tests still fail.\n\n",
                        now.failures.len()
                    ));
                    kept.push(path.clone());
                    refused_in_a_row = 0;
                    refused.remove(&path);
                    standing = now.clone();
                    evidence = after.evidence.clone();
                    continue;
                }
                let undo = match &on_disk {
                    Some(_) => pwr_domain::ToolCall {
                        name: "apply_replace".into(),
                        arguments: json!({"path": path, "expected_hash": pwr_domain::hash_bytes(file.as_bytes()), "replacement": current}),
                        id: None,
                    },
                    None => pwr_domain::ToolCall {
                        name: "delete_path".into(),
                        // Bound to the file just written, like the overwrite.
                        arguments: json!({"path": path, "expected_hash": pwr_domain::hash_bytes(file.as_bytes())}),
                        id: None,
                    },
                };
                let said =
                    format!("{path} did not reduce the failing tests; restored what was there.");
                // What a scripted turn left, taken into the goal's own state.
                macro_rules! taken {
                    ($turn:expr) => {{
                        match $turn {
                            Ok((report, next)) => {
                                total_actions = total_actions.saturating_add(report.actions);
                                budget.actions = total_actions;
                                messages = next;
                                host.keep_messages(&messages);
                                let edited = report.edited;
                                last_report = Some(report);
                                edited
                            }
                            Err(_) => false,
                        }
                    }};
                }
                let mut put_back = taken!(scripted!(undo, said.clone()));
                if !put_back && on_disk.is_some() {
                    // An overwrite that drops more than half of a file is
                    // refused (the shrink guard), and a proposal twice the
                    // size of the original makes the way back exactly that.
                    // The guard's own advice: delete it by its hash, then
                    // write. Seen on the product path, 2026-10-06: a 137-line
                    // invoice.ts stayed where a 58-line one had been.
                    let removed = pwr_domain::ToolCall {
                        name: "delete_path".into(),
                        arguments: json!({"path": path, "expected_hash": pwr_domain::hash_bytes(file.as_bytes())}),
                        id: None,
                    };
                    if taken!(scripted!(removed, said.clone())) {
                        let rewritten = pwr_domain::ToolCall {
                            name: "write_file".into(),
                            arguments: json!({"path": path, "content": current}),
                            id: None,
                        };
                        put_back = taken!(scripted!(rewritten, said));
                    }
                }
                if !put_back {
                    // Said plainly: the workspace may now hold a file its
                    // tests did not approve, or lack one it had, and the goal
                    // that follows must know.
                    note = Some(format!(
                        "Proposals stopped: {path} did not reduce the failing tests and could not be restored; check it before anything else."
                    ));
                    break 'phase;
                }
                restored += 1;
                let broke = now
                    .as_ref()
                    .map(|now| {
                        now.failures
                            .difference(&standing.failures)
                            .cloned()
                            .collect()
                    })
                    .unwrap_or_default();
                let said = after.map_or_else(
                    || {
                        format!(
                            "The checks did not finish within {} s with this file: something in it may never return.",
                            allowed.as_secs()
                        )
                    },
                    |after| proposals::shown(&after.evidence, &target),
                );
                refused.insert(path, (file, said, broke));
            }
        }
        if budget.proposals > 0 {
            let mut text = format!(
                "Before this turn PWR asked for files one at a time and checked each against the owner's tests: {} kept ({}), {restored} restored because the failing tests did not shrink.",
                kept.len(),
                if kept.is_empty() {
                    "none".to_owned()
                } else {
                    kept.join(", ")
                },
            );
            text.push_str(&if standing.failing {
                format!(
                    " {} tests still fail. Continue with the request from the files as they are now.",
                    standing.failures.len()
                )
            } else {
                " The checks now pass. Hold the work against the request for anything its tests do not cover, then complete.".to_owned()
            });
            if let Some(note) = note {
                text.push_str(&format!(" {note}"));
            }
            messages.push(goal_guidance(text));
            host.keep_messages(&messages);
        }
    }
    let mut idle_rounds = 0usize;
    let mut same_failure: (Vec<String>, usize) = (Vec::new(), 0);
    let mut noticed_at: Option<usize> = None;
    // The verification that passed before the review round, kept so a
    // review that changes nothing ends on it without re-running checks.
    let mut reviewed: Option<GoalVerification> = None;
    let mut review_done = false;
    loop {
        // Before the turn, on every way round: the limits are not one
        // branch's business.
        if budgeted && let Some(reached) = budget.reached() {
            return SessionEnd::OutOfBudget {
                total_actions,
                reached,
            };
        }
        let base = highest_call.get();
        let mut turn_continuity = continuity.clone();
        if minimal {
            turn_continuity.harness = converse::Harness::Minimal;
        }
        if budgeted {
            if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
                checkpoint.actions = 0;
            }
            turn_continuity.action_limit =
                Some(budget.limits.actions.saturating_sub(total_actions));
            turn_continuity.work_limit = budget
                .limits
                .work
                .map(|work| work * converse::WORK_PER_TOKEN);
        }
        let turn = host.run_turn(TurnInput {
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
            continuity: turn_continuity,
            approvals: Arc::clone(&approvals),
            // Shared, not copied: what the person allowed for the session
            // during one turn of a goal holds for the next. Copied once per
            // prompt, it did not -- measured 2026-09-26, Docker allowed for
            // the session and asked about again on the goal's next turn.
            session_grants: Arc::clone(&session_grants),
            goal_mode,
            scripted: None,
        });
        let outcome = if budgeted {
            // The loop checked the limits just before this turn.
            let mut turn = std::pin::pin!(turn);
            let finished = tokio::time::timeout(budget.remaining(), &mut turn).await;
            match finished {
                Ok(value) if budget.started.elapsed() < budget.limits.wall => value,
                // The deadline passed with the turn still working. Dropped, the
                // turn took its history with it: its edits stayed on disk and
                // out of the conversation, which continued as if they had not
                // happened. Asked to stop, it returns that history.
                finished => {
                    stop.store(true, Ordering::Relaxed);
                    let returned = match finished {
                        Ok(value) => Some(value),
                        Err(_) => tokio::time::timeout(GOAL_STOP_GRACE, &mut turn).await.ok(),
                    };
                    if let Some(Ok((report, next_messages))) = returned {
                        budget.actions = budget.actions.saturating_add(report.actions);
                        host.keep_messages(&next_messages);
                    } else if let Ok(checkpoint) = continuity.checkpoint.lock() {
                        budget.actions = budget.actions.saturating_add(checkpoint.actions);
                    }
                    return SessionEnd::OutOfBudget {
                        total_actions: budget.actions,
                        reached: budget.time_limit(),
                    };
                }
            }
        } else {
            turn.await
        };
        let (report, next_messages) = match outcome {
            Ok(value) => value,
            Err(problem) => {
                return SessionEnd::Error {
                    code: -32000,
                    message: problem,
                };
            }
        };
        last_report = Some(report.clone());
        total_actions = total_actions.saturating_add(report.actions);
        budget.actions = total_actions;
        messages = next_messages;
        host.keep_messages(&messages);

        if !goal_mode
            || report.declined
            || report.stopped.is_some()
                && !matches!(report.stopped, Some(converse::StopReason::BudgetSpent))
        {
            return SessionEnd::Reply {
                report,
                total_actions,
                goal: goal_mode,
                verification: None,
            };
        }
        idle_rounds = if report.actions == 0 && !report.completed {
            idle_rounds + 1
        } else {
            0
        };

        // A verification from before a change says nothing about after it.
        goal_edited |= report.edited;
        if report.edited {
            reviewed = None;
        }
        // After the review the checks have passed and nothing has changed
        // since: a reply that changes nothing and takes no action is the
        // model's closing statement, said in words. Seen 2026-10-07 (gpt-oss
        // 20B, an Angular site built and building): it answered the review
        // "Complete." three times, was told three times not to stop with
        // prose, and a finished goal ended "paused: the last 3 check-ins took
        // no action".
        let closed_in_words =
            reviewed.is_some() && !report.edited && report.actions == 0 && report.stopped.is_none();
        if report.completed || closed_in_words {
            if let Some(verification) = reviewed.take() {
                if let Some(note) = not_verified_note(&verification) {
                    host.say(&note);
                }
                return SessionEnd::Reply {
                    report,
                    total_actions,
                    goal: true,
                    verification: Some(verification),
                };
            }
            let verified = verify!();
            let (verification, changed) = match verified {
                Ok(pair) => pair,
                Err(VerifyError::NoSuchSession) => {
                    return SessionEnd::Error {
                        code: -32602,
                        message: "no such session".into(),
                    };
                }
                Err(VerifyError::Failed(problem)) => {
                    return SessionEnd::Error {
                        code: -32000,
                        message: format!("goal verification could not run: {problem}"),
                    };
                }
            };
            // Every branch below either ends the goal or sends the model back.
            // The review reads the specification against the code, rule by
            // rule, and it is what catches a rule no check covers. It used to
            // run only when an acceptance check had passed -- which a person
            // who has declared none never gets, nearly everyone. Measured
            // 2026-09-30: Qwen3.6-35B ended two dev tasks "technical checks
            // passed, not verified" with an explicit README rule unmet
            // (blank lines ignored; `--keep 1` deleting older copies), no
            // review having run.
            let reviewable = verification.passed
                || (verification.technical_passed
                    && !verification.acceptance_available
                    && verification.contract_changed.is_empty());
            if reviewable && goal_edited && !review_done {
                if budget.reviews >= budget.limits.review_rounds {
                    return SessionEnd::OutOfBudget {
                        total_actions,
                        reached: GoalLimitReached::ReviewRounds {
                            spent: budget.reviews,
                            allowed: budget.limits.review_rounds,
                        },
                    };
                }
                budget.reviews += 1;
                review_done = true;
                reviewed = Some(verification);
                let prompt = review_prompt(&root, &person_requests(&messages), &changed);
                // The second reading takes about a minute and streams
                // nothing: said, so a person does not take it for a hang.
                host.say(if prompt.is_some() {
                    "The checks pass. Reviewing the work against the request before \
                     finishing: first the specification is read against the code \
                     once more, rule by rule (about a minute)."
                } else {
                    "The checks pass. Reviewing the work against the request before finishing."
                });
                let findings = match prompt {
                    Some(prompt) => bounded!(host.review(&root, prompt)).ok(),
                    None => None,
                };
                messages.push(goal_guidance(review_guidance(findings.as_deref())));
            } else if !verification.contract_changed.is_empty() || verification.passed {
                return SessionEnd::Reply {
                    report,
                    total_actions,
                    goal: true,
                    verification: Some(verification),
                };
            } else if !verification.technical_passed
                && !verification.failing.is_empty()
                && verification.failing_acceptance.is_empty()
                && verification
                    .failing
                    .iter()
                    .all(|check| already_failing.contains(check))
            {
                // A check broken before the goal is not the goal's to fix --
                // unless it is an acceptance check, which *is* the goal:
                // ending "left alone" on one reported a failed task as done.
                host.say(&format!(
                    "The checks that fail were already failing before this goal started, and were left alone: {}.\n{}",
                    verification.failing.join(", "),
                    verification.summary
                ));
                return SessionEnd::Reply {
                    report,
                    total_actions,
                    goal: true,
                    verification: Some(verification),
                };
            } else if let Some(note) = not_verified_note(&verification) {
                host.say(&note);
                return SessionEnd::Reply {
                    report,
                    total_actions,
                    goal: true,
                    verification: Some(verification),
                };
            } else {
                budget.refused += 1;
                let fingerprints = if verification.failure_fingerprints.is_empty() {
                    verification.failing.clone()
                } else {
                    verification.failure_fingerprints.clone()
                };
                if same_failure.0 == fingerprints {
                    same_failure.1 += 1;
                } else {
                    same_failure = (fingerprints, 1);
                }
                if same_failure.1 >= GOAL_SAME_FAILURE_LIMIT {
                    return SessionEnd::Stopped {
                        report,
                        total_actions,
                        terminal: "blocked",
                        text: format!(
                            "Goal mode stopped: the work was declared complete {} times and verification refused it the same way each time ({}). Something the code cannot change is probably in the way -- a missing tool, a permission, the environment; the checks' output says which.\n{}",
                            same_failure.1,
                            verification.failing.join(", "),
                            verification.summary
                        ),
                    };
                }
                host.say(&format!(
                    "Goal verification did not pass:\n{}\n\nContinuing from the current workspace.",
                    verification.summary
                ));
                messages.push(goal_guidance(format!(
                    "The goal is not complete: full repository verification failed. Fix the remaining issue. Evidence:\n{}",
                    verification.summary
                )));
            }
        } else {
            if idle_rounds >= GOAL_IDLE_LIMIT {
                return SessionEnd::Stopped {
                    report,
                    total_actions,
                    terminal: "stalled",
                    text: format!(
                        "Goal mode paused: the last {idle_rounds} check-ins took no action. Read the last replies for what is in the way, then continue deliberately."
                    ),
                };
            }
            if noticed_at.is_none_or(|at| total_actions >= at + GOAL_NOTICE_EVERY) {
                noticed_at = Some(total_actions);
                host.say(&format!(
                    "Checkpoint after {total_actions} action(s). Continuing toward the goal; use Stop to interrupt."
                ));
            }
            messages.push(goal_guidance(
                "Continue the same goal from the saved workspace state. Do not stop with prose: either make the next necessary change, investigate an unmet requirement, or call complete only when the full objective is ready for verification.",
            ));
        }
    }
}

/// What a goal says when the technical checks pass and there is nothing to
/// verify the goal against.
fn not_verified_note(verification: &GoalVerification) -> Option<String> {
    if !verification.acceptance_available
        && verification.failing.is_empty()
        && matches!(
            verification.checks,
            Some(pwr_domain::ChecksOutcome::Unavailable { .. })
        )
    {
        return Some(format!(
            "Independent verification unavailable; the goal is not verified.\n{}",
            verification.summary
        ));
    }
    (verification.technical_passed && !verification.acceptance_available).then(|| {
        format!(
            "Technical checks passed, but the goal is not verified because this workspace has no declared acceptance check.\n{}",
            verification.summary
        )
    })
}

/// What the checks establish after an edit, as three different things.
///
/// They were one thing, and the one thing was wrong. The conversation ran
/// `pwr_verify::compare` and said "the repository's own checks passed after
/// the change" whenever `new_failures` was empty. `new_failures` is the set of
/// checks that fail now and did not fail before: a check that was already red
/// at the baseline and is still red is, correctly, not in it. So a repository
/// with a failing suite got told its checks passed, by the harness whose stated
/// purpose is to refuse exactly that claim.
///
/// The distinction is the one [MASTER_SPEC](../../../MASTER_SPEC.md) draws
/// between `checks_passed` and `baseline_preserved`, and it is not pedantry:
/// the first says the work is verified and the second says only that the work
/// broke nothing that was working. An engineer acts differently on each.
pub enum CheckVerdict {
    /// Every check the workspace declares passes now.
    Green,
    /// Nothing that passed before fails now, and something is still red.
    ///
    /// Named with the commands, because "some were already failing" invites the
    /// reader to assume they are the ones they already knew about.
    BaselinePreserved { still_failing: Vec<String> },
    /// Something that passed before fails now.
    NewFailures(Vec<String>),
}

impl CheckVerdict {
    pub fn mark(&self) -> &'static str {
        match self {
            Self::Green => "✓",
            Self::BaselinePreserved { .. } => "–",
            Self::NewFailures(_) => "✗",
        }
    }
    pub fn said(&self) -> String {
        match self {
            Self::Green => "the repository's own checks passed after the change".to_owned(),
            Self::BaselinePreserved { still_failing } => format!(
                "no check that passed before the change fails now, and {} still \
                 {} as {} before the change: {}. The change is not verified by \
                 {}; it is only not the cause of {} failing",
                still_failing.len(),
                if still_failing.len() == 1 {
                    "does"
                } else {
                    "do"
                },
                if still_failing.len() == 1 {
                    "it did"
                } else {
                    "they did"
                },
                still_failing.join(", "),
                if still_failing.len() == 1 {
                    "it"
                } else {
                    "them"
                },
                if still_failing.len() == 1 {
                    "it"
                } else {
                    "them"
                },
            ),
            Self::NewFailures(commands) => format!(
                "the repository's own checks did not pass: {}",
                commands.join(", ")
            ),
        }
    }
}

/// Reads the two baselines for what they actually establish.
///
/// A check with no exit code at all counts as failing: the command did not run
/// to a verdict, and an absent verdict is not a passing one.
pub fn check_verdict(
    before: &pwr_verify::VerificationBaseline,
    after: &pwr_verify::VerificationBaseline,
) -> CheckVerdict {
    let comparison = pwr_verify::compare(before, after);
    if !comparison.new_failures.is_empty() {
        return CheckVerdict::NewFailures(comparison.new_failures);
    }
    let still_failing: Vec<String> = after
        .checks
        .iter()
        .filter(|check| check.result.exit_code != Some(0))
        .map(|check| check.command.clone())
        .collect();
    if still_failing.is_empty() {
        CheckVerdict::Green
    } else {
        CheckVerdict::BaselinePreserved { still_failing }
    }
}

/// What the front end prepares for the checks that close a turn: the policy
/// they run under -- with the allowlist of the checks' own programs and what
/// the person allowed for the session, so a restore has the network they
/// allowed a moment ago -- the checks as discovered before and after the turn,
/// and the baseline taken before the turn acted.
pub struct ClosingChecks {
    pub policy: pwr_tools::ToolPolicy,
    pub checks_before: Vec<(String, Vec<String>)>,
    pub checks_after: Vec<(String, Vec<String>)>,
    pub before: Option<pwr_verify::VerificationBaseline>,
}

/// Runs repository checks until the operator stops; None records interruption.
pub async fn baseline_until_stopped(
    policy: &pwr_tools::ToolPolicy,
    checks: &[(String, Vec<String>)],
    stop: &AtomicBool,
) -> Option<Result<pwr_verify::VerificationBaseline, pwr_tools::ToolError>> {
    tokio::select! {
        biased;
        () = converse::pressed(stop) => None,
        result = pwr_verify::baseline(policy, checks) => Some(result),
    }
}

/// Closing verification with the same operator Stop as the conversation.
pub async fn close_turn_with_stop(
    checks: ClosingChecks,
    report: &mut TurnReport,
    messages: &mut Vec<ChatMessage>,
    steps: &mut dyn FnMut(TurnStep),
    stop: &AtomicBool,
) {
    tokio::select! {
        biased;
        () = converse::pressed(stop) => {},
        () = close_turn(checks, report, messages, steps) => return,
    }
    report.completed = false;
    report.stopped = Some(converse::StopReason::Interrupted);
    report.outcome.terminal = pwr_domain::TurnTerminal::Interrupted;
    report.outcome.checks = pwr_domain::ChecksOutcome::CouldNotRun {
        why: "verification interrupted by the operator".into(),
    };
    let verdict = "Independent verification interrupted at your request; changes are kept and remain unverified.";
    report.answer = format!("{}\n\n{verdict}", report.answer.trim());
    steps(TurnStep::Note(format!("– {verdict}")));
    messages.push(ChatMessage {
        purpose: Some(pwr_domain::MessagePurpose::VerificationFeedback),
        ..ChatMessage::text("user", verdict)
    });
}

/// Faces a turn that changed the workspace with the repository's own checks,
/// and puts what they said into the answer, the model's next prompt and the
/// turn's typed outcome.
///
/// Saying "done" without that is the claim this project exists to refuse. The
/// mark, the wording and the evidence all come from the same reading of the
/// baselines, so a failure is never shown with a tick (plan W2.2).
pub async fn close_turn(
    checks: ClosingChecks,
    report: &mut TurnReport,
    messages: &mut Vec<ChatMessage>,
    steps: &mut dyn FnMut(TurnStep),
) {
    let ClosingChecks {
        policy: verification_policy,
        checks_before: checks,
        checks_after: after_checks,
        before,
    } = checks;
    let mut evidence = pwr_domain::ChecksOutcome::Unavailable {
        why: "this workspace declares no automated checks".into(),
    };
    let mut baseline_evidence = pwr_domain::BaselineOutcome::NoBaseline;
    let (mark, verdict) = match (&before, after_checks.is_empty(), checks == after_checks) {
        (_, true, _) => (
            "–",
            "Independent verification unavailable: this workspace declares no automated checks"
                .to_owned(),
        ),
        (Some(before), false, true) => {
            match pwr_verify::baseline(&verification_policy, &after_checks).await {
                Ok(after) => {
                    if after.checks.iter().any(|check| !check.result.sandboxed) {
                        report.outcome.confinement = pwr_domain::Confinement::Unconfined;
                    }
                    evidence = pwr_verify::evidence::checks(&after);
                    baseline_evidence = pwr_verify::evidence::baseline(Some(before), &after);
                    let verdict = check_verdict(before, &after);
                    if matches!(verdict, CheckVerdict::Green)
                        && matches!(evidence, pwr_domain::ChecksOutcome::RanZeroTests)
                    {
                        ("–", "the checks exited successfully but ran zero tests; behavior remains unverified".to_owned())
                    } else {
                        (verdict.mark(), verdict.said())
                    }
                }
                Err(error) => {
                    evidence = pwr_domain::ChecksOutcome::CouldNotRun {
                        why: error.to_string(),
                    };
                    ("!", format!("the checks could not be run: {error}"))
                }
            }
        }
        _ => match pwr_verify::baseline(&verification_policy, &after_checks).await {
            Ok(after) => {
                if after.checks.iter().any(|check| !check.result.sandboxed) {
                    report.outcome.confinement = pwr_domain::Confinement::Unconfined;
                }
                evidence = pwr_verify::evidence::checks(&after);
                let failing: Vec<_> = after
                    .checks
                    .iter()
                    .filter(|check| check.result.exit_code != Some(0))
                    .map(|check| check.command.as_str())
                    .collect();
                if failing.is_empty() {
                    if matches!(evidence, pwr_domain::ChecksOutcome::RanZeroTests) {
                        ("–", "the new project checks exited successfully but ran zero tests; its behavior has not been verified".to_owned())
                    } else {
                        ("✓", "the newly discovered project checks passed after the edit; no prior baseline exists for them".to_owned())
                    }
                } else {
                    (
                        "✗",
                        format!(
                            "the newly discovered project checks failed: {}; no prior baseline exists for them",
                            failing.join(", ")
                        ),
                    )
                }
            }
            Err(error) => {
                evidence = pwr_domain::ChecksOutcome::CouldNotRun {
                    why: error.to_string(),
                };
                (
                    "!",
                    format!("the newly discovered checks could not be run: {error}"),
                )
            }
        },
    };
    report.outcome.checks = evidence;
    report.outcome.baseline = baseline_evidence;
    steps(converse::TurnStep::Note(format!("{mark} {verdict}")));
    report.answer = format!("{}\n\n{verdict}", report.answer.trim());
    messages.push(ChatMessage {
        role: "user".into(),
        content: format!("Harness verification feedback after your edits: {verdict}"),
        purpose: Some(pwr_domain::MessagePurpose::VerificationFeedback),
        ..Default::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A broken check may be the very behaviour the engineer requested.
    /// The observed React/ledger prompts incorrectly excluded every baseline
    /// failure before asking the model to repair the same behaviour.
    #[test]
    fn baseline_note_preserves_requested_repairs_and_acceptance() {
        let note = already_failing_note(&["npm test --silent".into()]);
        assert!(note.contains("already failing: npm test --silent"));
        assert!(
            note.contains("failures covered by the engineer's request"),
            "{note}"
        );
        assert!(note.contains("acceptance checks must still pass"), "{note}");
        assert!(!note.contains("They are not part of this goal"), "{note}");
    }

    /// The first passing verification asks once for a review against the
    /// request; a review that changes nothing ends the goal on it.
    #[test]
    fn the_reviewer_reads_source_not_tests_or_pwr_state() {
        for path in [
            "lib/stock.ex",
            "bin/rotate",
            "Dockerfile",
            ".dockerignore",
            "src/app/app.ts",
        ] {
            assert!(reviewable(path), "{path}");
        }
        for path in [
            "test/cron_extra_test.dart",
            "tests/test_rotate.py",
            "src/app/todo-list.spec.ts",
            "src/app/store.test.ts",
            "test_hidden.py",
            ".pwr-scratch/check.sh",
            ".toolchains/elixir/bin/mix",
            "package-lock.json",
            "Cargo.lock",
        ] {
            assert!(!reviewable(path), "{path}");
        }
    }

    #[test]
    fn the_reviewer_gets_the_request_the_readme_and_the_changed_source() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(
            root.path().join("README.md"),
            "`opts[:name]` registers the process",
        )
        .unwrap();
        std::fs::create_dir_all(root.path().join("lib")).unwrap();
        std::fs::create_dir_all(root.path().join("test")).unwrap();
        std::fs::write(root.path().join("lib/stock.ex"), "def start_link(opts)").unwrap();
        std::fs::write(root.path().join("test/extra_test.exs"), "assert true").unwrap();
        let changed = BTreeMap::from([
            ("lib/stock.ex".to_owned(), "h1".to_owned()),
            ("test/extra_test.exs".to_owned(), "h2".to_owned()),
        ]);
        let prompt = review_prompt(root.path(), "Implement Stock", &changed).unwrap();
        assert!(prompt.contains("Implement Stock"));
        assert!(prompt.contains("`opts[:name]` registers the process"));
        assert!(prompt.contains("--- lib/stock.ex ---\ndef start_link(opts)"));
        assert!(!prompt.contains("extra_test"));
        let only_tests = BTreeMap::from([("test/extra_test.exs".to_owned(), "h2".to_owned())]);
        assert!(review_prompt(root.path(), "Implement Stock", &only_tests).is_none());
    }

    #[test]
    fn the_person_s_requests_are_theirs_without_the_goal_s_instructions() {
        let mut guidance = ChatMessage::text("user", "The checks pass. Before finishing...");
        guidance.purpose = Some(pwr_domain::MessagePurpose::GoalGuidance);
        let messages = vec![
            ChatMessage::text("system", "you are PWR"),
            ChatMessage::text(
                "user",
                "Implement the cron parser\n\nGoal mode is enabled. Keep working.",
            ),
            ChatMessage::text("assistant", "done"),
            guidance,
            ChatMessage::text("user", "Also accept tabs"),
        ];
        assert_eq!(
            person_requests(&messages),
            "Implement the cron parser\n---\nAlso accept tabs"
        );
    }

    #[test]
    fn what_the_reviewer_found_is_added_to_the_review_and_nothing_else_is() {
        assert_eq!(review_guidance(None), GOAL_REVIEW);
        assert_eq!(review_guidance(Some("  NO DISCREPANCIES\n")), GOAL_REVIEW);
        assert_eq!(
            review_guidance(Some("- ids are never reused -- MET: lib/stock.ex next_id")),
            GOAL_REVIEW
        );
        let found = review_guidance(Some(
            "- ids are never reused -- MET: lib/stock.ex next_id\n\
             - `opts[:name]` registers the process -- NOT MET: start_link ignores it",
        ));
        assert!(found.starts_with(GOAL_REVIEW));
        assert!(found.contains("NOT MET: start_link ignores it"));
        assert!(!found.contains("next_id"));
        assert!(found.contains("It can be wrong"));
    }

    // ------------------------------------------------ the executor on its own

    use std::sync::atomic::AtomicUsize;

    /// A host with no front end behind it: turns and verifications are played
    /// from lists, and what it was told to say is kept.
    struct Fake {
        turns: Mutex<Vec<TurnReport>>,
        verifications: Mutex<Vec<Result<GoalVerification, VerifyError>>>,
        said: Mutex<Vec<String>>,
        ran: AtomicUsize,
    }

    impl Fake {
        fn new(
            turns: Vec<TurnReport>,
            verifications: Vec<Result<GoalVerification, VerifyError>>,
        ) -> Self {
            Self {
                turns: Mutex::new(turns),
                verifications: Mutex::new(verifications),
                said: Mutex::default(),
                ran: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl SessionHost for Fake {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            self.ran.fetch_add(1, Ordering::Relaxed);
            let mut turns = self.turns.lock().unwrap();
            // The last one repeats once the others are spent.
            let report = if turns.len() > 1 {
                turns.remove(0)
            } else {
                turns[0].clone()
            };
            let mut messages = input.messages;
            messages.push(ChatMessage::text("assistant", report.answer.clone()));
            Ok((report, messages))
        }

        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            let mut all = self.verifications.lock().unwrap();
            let next = if all.len() > 1 {
                all.remove(0)
            } else {
                match &all[0] {
                    Ok(verification) => Ok(verification.clone()),
                    Err(VerifyError::NoSuchSession) => Err(VerifyError::NoSuchSession),
                    Err(VerifyError::Failed(why)) => Err(VerifyError::Failed(why.clone())),
                }
            };
            next.map(|verification| (verification, BTreeMap::new()))
        }

        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            Ok(String::new())
        }

        fn say(&self, text: &str) {
            self.said.lock().unwrap().push(text.to_owned());
        }

        fn keep_messages(&self, _: &[ChatMessage]) {}
    }

    fn report(actions: usize, completed: bool) -> TurnReport {
        TurnReport {
            outcome: Default::default(),
            answer: if completed { "done" } else { "working" }.into(),
            actions,
            edited: false,
            completed,
            stopped: None,
            declined: false,
        }
    }

    fn refused(failing: &str) -> GoalVerification {
        GoalVerification {
            failing: vec![failing.into()],
            failure_fingerprints: vec![failing.into()],
            summary: format!("0 of 1 checks passing: {failing}"),
            ..Default::default()
        }
    }

    fn request(policy: Policy) -> SessionRequest {
        SessionRequest {
            root: PathBuf::from("."),
            conversation_id: pwr_domain::new_id(),
            messages: vec![ChatMessage::text("user", "make the tests pass")],
            stop: Arc::new(AtomicBool::new(false)),
            steps: Rc::new(RefCell::new(Box::new(|_| {}))),
            continuity: converse::Continuity::default(),
            approvals: Arc::new(crate::DenyWithoutAsking),
            session_grants: Arc::default(),
            policy,
            aids: GoalAids::default(),
        }
    }

    fn run<H: SessionHost>(host: &H, policy: Policy, limits: GoalLimits) -> SessionResult {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .unwrap()
            .block_on(execute(host, request(policy), limits))
    }

    /// A conversation is one turn: no verification, no second turn, whatever
    /// the turn says.
    #[test]
    fn a_conversation_ends_after_one_turn_and_verifies_nothing() {
        let host = Fake::new(
            vec![report(2, false)],
            vec![Err(VerifyError::NoSuchSession)],
        );
        let result = run(&host, Policy::Conversation, GoalLimits::default());
        assert!(matches!(
            result.end,
            SessionEnd::Reply {
                goal: false,
                verification: None,
                total_actions: 2,
                ..
            }
        ));
        assert_eq!(host.ran.load(Ordering::Relaxed), 1);
        assert_eq!(result.budget.verifications, 0);
    }

    /// The defect of 2026-09-30: completions refused with a failing set that
    /// alternates never reach the same-failure limit, and the action limit was
    /// tested on one branch only. The executor stops them at the refused
    /// completions limit, with no front end involved.
    #[test]
    fn a_goal_whose_completions_are_refused_in_turn_stops_at_the_limit() {
        // The first verification is the goal's baseline, taken before any turn.
        let alternating: Vec<_> = std::iter::once(Ok(GoalVerification::default()))
            .chain((0..12).map(|n| Ok(refused(if n % 2 == 0 { "suite a" } else { "suite b" }))))
            .collect();
        let host = Fake::new(vec![report(1, true)], alternating);
        let limits = GoalLimits {
            verification_runs: 50,
            ..GoalLimits::default()
        };
        let result = run(&host, Policy::Goal, limits);
        match result.end {
            SessionEnd::OutOfBudget { reached, .. } => assert!(
                matches!(
                    reached,
                    GoalLimitReached::RefusedCompletions { spent: 6, .. }
                ),
                "{reached:?}"
            ),
            _ => panic!("the goal was not stopped by its budget"),
        }
        // Six completions were refused, and no seventh turn was started.
        assert_eq!(host.ran.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn the_same_refusal_three_times_is_a_wall_and_says_so() {
        let host = Fake::new(
            vec![report(1, true)],
            vec![Ok(GoalVerification::default()), Ok(refused("suite a"))],
        );
        let result = run(&host, Policy::Goal, GoalLimits::default());
        match result.end {
            SessionEnd::Stopped { terminal, text, .. } => {
                assert_eq!(terminal, "blocked");
                assert!(text.contains("the same way each time"), "{text}");
            }
            _ => panic!("the goal did not stop as blocked"),
        }
        assert_eq!(host.ran.load(Ordering::Relaxed), 3);
    }

    /// Passing checks with a declared acceptance check end the goal verified.
    #[test]
    fn a_goal_ends_verified_only_when_acceptance_passed() {
        let passed = GoalVerification {
            passed: true,
            technical_passed: true,
            acceptance_available: true,
            summary: "1 of 1 checks passing".into(),
            ..Default::default()
        };
        let host = Fake::new(
            vec![report(1, true)],
            vec![Ok(GoalVerification::default()), Ok(passed.clone())],
        );
        let SessionEnd::Reply { verification, .. } =
            run(&host, Policy::Goal, GoalLimits::default()).end
        else {
            panic!("a passing goal did not reply");
        };
        let verification = verification.expect("the goal carries its verification");
        let mut outcome = pwr_domain::TurnOutcome::default();
        apply_verification(&mut outcome, &verification);
        assert!(outcome.verified());

        // Technical checks alone, with no acceptance check declared, are not it.
        let technical = GoalVerification {
            passed: false,
            technical_passed: true,
            acceptance_available: false,
            ..passed
        };
        let mut outcome = pwr_domain::TurnOutcome::default();
        apply_verification(&mut outcome, &technical);
        assert!(!outcome.verified());
        assert!(matches!(
            outcome.acceptance,
            pwr_domain::AcceptanceOutcome::NotDeclared
        ));
    }

    /// With no acceptance check declared the goal is never "verified", but the
    /// work is still read against the request once before it is handed over.
    #[test]
    fn a_goal_with_technical_checks_only_is_still_reviewed_once_and_ends_not_verified() {
        let technical = GoalVerification {
            technical_passed: true,
            acceptance_available: false,
            summary: "1 of 1 checks passing".into(),
            ..Default::default()
        };
        let mut edited = report(2, true);
        edited.edited = true;
        let host = Fake::new(
            vec![edited, report(1, true)],
            vec![Ok(GoalVerification::default()), Ok(technical)],
        );
        let SessionEnd::Reply { verification, .. } =
            run(&host, Policy::Goal, GoalLimits::default()).end
        else {
            panic!("the goal did not reply");
        };
        assert!(!verification.expect("carries it").passed);
        let said = host.said.lock().unwrap().join("\n");
        assert!(
            said.contains("Reviewing the work against the request"),
            "{said}"
        );
        assert!(
            said.contains("not verified because this workspace has no declared"),
            "{said}"
        );
        assert_eq!(
            host.ran.load(Ordering::Relaxed),
            2,
            "one turn after the review"
        );
    }

    #[test]
    fn after_the_review_an_answer_in_words_that_changes_nothing_closes_the_goal() {
        let passing = GoalVerification {
            technical_passed: true,
            acceptance_available: false,
            summary: "1 of 1 checks passing".into(),
            ..Default::default()
        };
        let mut edited = report(2, true);
        edited.edited = true;
        // The reply to the review: no call, no action, "Complete." in prose.
        let host = Fake::new(
            vec![edited, report(0, false)],
            vec![Ok(GoalVerification::default()), Ok(passing)],
        );
        let SessionEnd::Reply { verification, .. } =
            run(&host, Policy::Goal, GoalLimits::default()).end
        else {
            panic!("the goal did not reply");
        };
        assert!(verification.expect("carries it").technical_passed);
        assert_eq!(host.ran.load(Ordering::Relaxed), 2, "not asked again");
    }

    #[test]
    fn a_session_that_is_gone_is_the_hosts_answer_to_verification() {
        let host = Fake::new(vec![report(1, true)], vec![Err(VerifyError::NoSuchSession)]);
        let result = run(&host, Policy::Goal, GoalLimits::default());
        // The baseline could not run, which a goal survives; the completion's
        // verification could not either, which it does not.
        assert!(matches!(result.end, SessionEnd::Error { code: -32602, .. }));
    }

    // ------------------------------------------ the checks that close a turn

    fn closing_policy(root: &Path) -> pwr_tools::ToolPolicy {
        pwr_tools::ToolPolicy {
            root: root.to_path_buf(),
            extra_readable: Vec::new(),
            protected: Vec::new(),
            allow_commands: vec!["sh".into()],
            output_limit: 4096,
            timeout: std::time::Duration::from_secs(10),
            sandbox: pwr_tools::SandboxPolicy::Disabled,
            approvals: Vec::new(),
        }
    }

    #[tokio::test]
    async fn stop_interrupts_baseline_and_closing_checks_without_claiming_completion() {
        for closing in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let policy = closing_policy(dir.path());
            let checks = vec![(
                "sh".into(),
                vec!["-c".into(), "touch started; sleep 30".into()],
            )];
            let stop = Arc::new(AtomicBool::new(false));
            let signal = stop.clone();
            let started = dir.path().join("started");
            let stopping = tokio::spawn(async move {
                while !started.exists() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                signal.store(true, Ordering::Relaxed);
            });
            let mut report = report(2, true);
            report.edited = true;
            let mut messages = vec![];
            let mut notes = vec![];
            tokio::time::timeout(Duration::from_secs(2), async {
                if closing {
                    close_turn_with_stop(
                        ClosingChecks {
                            policy,
                            checks_before: checks.clone(),
                            checks_after: checks,
                            before: None,
                        },
                        &mut report,
                        &mut messages,
                        &mut |step| {
                            if let TurnStep::Note(note) = step {
                                notes.push(note);
                            }
                        },
                        &stop,
                    )
                    .await;
                    assert!(report.edited);
                    assert_eq!(report.actions, 2);
                    assert!(!report.completed);
                    assert_eq!(report.stopped, Some(converse::StopReason::Interrupted));
                    assert!(!report.outcome.verified());
                    assert!(report.answer.contains("verification interrupted"));
                    assert!(notes.iter().all(|note| !note.starts_with('✓')));
                } else {
                    assert!(
                        baseline_until_stopped(&policy, &checks, &stop)
                            .await
                            .is_none()
                    );
                }
            })
            .await
            .expect("Stop waited for a repository check to finish");
            stopping.await.unwrap();
        }
    }

    struct StopInGoalPhase {
        phase: &'static str,
        stop: Arc<AtomicBool>,
        verifies: std::cell::Cell<usize>,
    }
    #[async_trait::async_trait(?Send)]
    impl SessionHost for StopInGoalPhase {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            if self.phase == "turn" {
                std::fs::write(input.root.join("lib.rs"), "edited before Stop").unwrap();
                {
                    let mut checkpoint = input.continuity.checkpoint.lock().unwrap();
                    checkpoint.actions = 3;
                    checkpoint
                        .changed_files
                        .insert("lib.rs".into(), "edited-hash".into());
                }
                self.stop.store(true, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(50)).await;
                let mut stopped = report(3, false);
                stopped.edited = true;
                stopped.stopped = Some(converse::StopReason::Interrupted);
                return Ok((stopped, input.messages));
            }
            let mut done = report(2, true);
            done.edited = true;
            Ok((done, input.messages))
        }
        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            let n = self.verifies.get();
            self.verifies.set(n + 1);
            if (self.phase == "baseline" && n == 0) || (self.phase == "verification" && n == 1) {
                self.stop.store(true, Ordering::Relaxed);
                std::future::pending::<()>().await;
            }
            Ok((
                GoalVerification {
                    passed: true,
                    technical_passed: true,
                    acceptance_available: true,
                    ..Default::default()
                },
                BTreeMap::from([("lib.rs".into(), "hash".into())]),
            ))
        }
        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            assert_eq!(self.phase, "review");
            self.stop.store(true, Ordering::Relaxed);
            std::future::pending().await
        }
        fn say(&self, _: &str) {}
        fn keep_messages(&self, _: &[ChatMessage]) {}
    }
    #[tokio::test]
    async fn stop_interrupts_goal_baseline_verification_and_review() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("lib.rs"), "pub fn value() -> u8 { 1 }").unwrap();
        for phase in ["baseline", "verification", "review", "turn"] {
            let mut input = request(Policy::Goal);
            input.root = root.path().to_owned();
            let host = StopInGoalPhase {
                phase,
                stop: input.stop.clone(),
                verifies: Default::default(),
            };
            let result = tokio::time::timeout(
                Duration::from_secs(1),
                execute(&host, input, GoalLimits::default()),
            )
            .await
            .expect("Stop did not interrupt a Goal phase");
            match result.end {
                SessionEnd::Reply {
                    report,
                    total_actions,
                    verification,
                    ..
                } => {
                    assert_eq!(report.stopped, Some(converse::StopReason::Interrupted));
                    assert!(!report.completed);
                    assert!(verification.is_none());
                    assert_eq!(
                        total_actions,
                        match phase {
                            "baseline" => 0,
                            "turn" => 3,
                            _ => 2,
                        }
                    );
                    if phase == "turn" {
                        assert_eq!(
                            std::fs::read_to_string(root.path().join("lib.rs")).unwrap(),
                            "edited before Stop"
                        );
                    }
                    if phase != "baseline" {
                        assert!(report.edited);
                    }
                }
                _ => panic!("operator Stop was classified as a Goal budget or failure"),
            }
        }
    }

    struct TranscriptStop {
        stop: Arc<AtomicBool>,
        kept: Mutex<Vec<ChatMessage>>,
    }
    #[async_trait::async_trait(?Send)]
    impl SessionHost for TranscriptStop {
        async fn run_turn(
            &self,
            mut input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            input.messages.push(ChatMessage::text(
                "tool",
                "edit receipt retained after Stop",
            ));
            self.stop.store(true, Ordering::Relaxed);
            tokio::time::sleep(Duration::from_millis(50)).await;
            let mut stopped = report(1, false);
            stopped.edited = true;
            stopped.stopped = Some(converse::StopReason::Interrupted);
            Ok((stopped, input.messages))
        }
        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            Ok((Default::default(), Default::default()))
        }
        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            unreachable!()
        }
        fn say(&self, _: &str) {}
        fn keep_messages(&self, messages: &[ChatMessage]) {
            *self.kept.lock().unwrap() = messages.to_vec();
        }
    }
    #[tokio::test]
    async fn goal_stop_retains_the_interrupted_turn_transcript() {
        let input = request(Policy::Goal);
        let host = TranscriptStop {
            stop: input.stop.clone(),
            kept: Default::default(),
        };
        let result = execute(&host, input, GoalLimits::default()).await;
        assert!(matches!(result.end, SessionEnd::Reply { .. }));
        assert!(
            host.kept
                .lock()
                .unwrap()
                .iter()
                .any(|message| message.content == "edit receipt retained after Stop"),
            "outer Stop dropped the turn before its transcript was saved"
        );
    }

    /// A turn still working when the goal's time runs out: it edits, then
    /// works until asked to stop -- or, `ignores_stop`, never returns.
    struct OutlastsTheWall {
        ignores_stop: bool,
        kept: Mutex<Vec<ChatMessage>>,
    }
    #[async_trait::async_trait(?Send)]
    impl SessionHost for OutlastsTheWall {
        async fn run_turn(
            &self,
            mut input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            input.messages.push(ChatMessage::text(
                "tool",
                "edit receipt from the turn the deadline ended",
            ));
            if let Ok(mut checkpoint) = input.continuity.checkpoint.lock() {
                checkpoint.actions = 2;
            }
            loop {
                if !self.ignores_stop && input.stop.load(Ordering::Relaxed) {
                    let mut stopped = report(2, false);
                    stopped.edited = true;
                    stopped.stopped = Some(converse::StopReason::Interrupted);
                    return Ok((stopped, input.messages));
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            Ok((Default::default(), Default::default()))
        }
        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            unreachable!()
        }
        fn say(&self, _: &str) {}
        fn keep_messages(&self, messages: &[ChatMessage]) {
            *self.kept.lock().unwrap() = messages.to_vec();
        }
    }
    /// The goal's deadline used to drop the turn in progress, and with it the
    /// turn's history: its edits stayed on disk and out of the conversation,
    /// so the next prompt continued as if they had not happened.
    #[tokio::test(start_paused = true)]
    async fn the_goal_deadline_keeps_the_transcript_of_the_turn_it_ends() {
        let host = OutlastsTheWall {
            ignores_stop: false,
            kept: Default::default(),
        };
        let limits = GoalLimits {
            wall: Duration::from_secs(60),
            ..GoalLimits::default()
        };
        let result = execute(&host, request(Policy::Goal), limits).await;
        match result.end {
            SessionEnd::OutOfBudget {
                total_actions,
                reached,
            } => {
                assert!(
                    matches!(reached, GoalLimitReached::Time { .. }),
                    "{reached:?}"
                );
                assert_eq!(total_actions, 2);
            }
            _ => panic!("the deadline did not end the goal"),
        }
        assert!(
            host.kept
                .lock()
                .unwrap()
                .iter()
                .any(|message| message.content == "edit receipt from the turn the deadline ended"),
            "the deadline dropped the turn before its transcript was kept"
        );
    }
    /// A turn that does not stop when asked is abandoned after a bounded
    /// grace, so the deadline still holds.
    #[tokio::test(start_paused = true)]
    async fn a_turn_that_ignores_the_deadline_is_abandoned_after_a_grace() {
        let host = OutlastsTheWall {
            ignores_stop: true,
            kept: Default::default(),
        };
        let limits = GoalLimits {
            wall: Duration::from_secs(60),
            ..GoalLimits::default()
        };
        let started = tokio::time::Instant::now();
        let result = execute(&host, request(Policy::Goal), limits).await;
        assert!(matches!(
            result.end,
            SessionEnd::OutOfBudget {
                total_actions: 2,
                ..
            }
        ));
        assert!(
            started.elapsed() <= Duration::from_secs(60) + GOAL_STOP_GRACE,
            "{:?}",
            started.elapsed()
        );
    }

    /// What the W8.3 control's turn is given, and what it is spared.
    struct Control {
        seen: Mutex<Vec<(converse::Harness, Option<usize>, bool)>>,
        verified: std::cell::Cell<usize>,
    }
    #[async_trait::async_trait(?Send)]
    impl SessionHost for Control {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            self.seen.lock().unwrap().push((
                input.continuity.harness,
                input.continuity.action_limit,
                input.goal_mode,
            ));
            let mut done = report(4, true);
            done.edited = true;
            Ok((done, input.messages))
        }
        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            self.verified.set(self.verified.get() + 1);
            Ok((Default::default(), Default::default()))
        }
        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            unreachable!("the control is never reviewed")
        }
        fn say(&self, _: &str) {}
        fn keep_messages(&self, _: &[ChatMessage]) {}
    }
    #[test]
    fn the_minimal_control_runs_one_budgeted_turn_and_verifies_nothing() {
        let host = Control {
            seen: Mutex::default(),
            verified: std::cell::Cell::new(0),
        };
        let limits = GoalLimits {
            actions: 50,
            ..GoalLimits::default()
        };
        let result = run(&host, Policy::Minimal, limits);
        assert_eq!(
            host.seen.lock().unwrap().as_slice(),
            &[(converse::Harness::Minimal, Some(50), false)]
        );
        assert_eq!(host.verified.get(), 0, "the control was verified");
        match result.end {
            SessionEnd::Reply {
                report,
                total_actions,
                goal,
                verification,
            } => {
                assert!(report.completed);
                assert_eq!(total_actions, 4);
                assert!(!goal);
                assert!(verification.is_none());
            }
            _ => panic!("the control did not end on its turn"),
        }
        assert_eq!(result.budget.actions, 4);
    }

    /// The one check the fixtures declare: green until a `broken` file exists.
    fn the_check() -> (String, Vec<String>) {
        ("sh".into(), vec!["-c".into(), "test ! -e broken".into()])
    }

    /// Closes a turn that edited a file, the way the front end does: the check
    /// run before the turn (`was_red` says whether the repository was already
    /// failing it), the same check run after (`now_red`).
    fn close(
        has_check: bool,
        was_red: bool,
        now_red: bool,
    ) -> (TurnReport, Vec<ChatMessage>, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let policy = closing_policy(dir.path());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let checks: Vec<_> = has_check.then(the_check).into_iter().collect();
        if was_red {
            std::fs::write(dir.path().join("broken"), "").unwrap();
        }
        let before = if checks.is_empty() {
            None
        } else {
            runtime
                .block_on(pwr_verify::baseline(&policy, &checks))
                .ok()
        };
        // The turn's edit, as far as the check can tell.
        match (was_red, now_red) {
            (false, true) => std::fs::write(dir.path().join("broken"), "").unwrap(),
            (true, false) => std::fs::remove_file(dir.path().join("broken")).unwrap(),
            _ => {}
        }
        let mut report = report(2, false);
        report.edited = true;
        let mut messages = vec![ChatMessage::text("user", "change it")];
        let mut noted = Vec::new();
        runtime.block_on(close_turn(
            ClosingChecks {
                policy,
                checks_before: checks.clone(),
                checks_after: checks,
                before,
            },
            &mut report,
            &mut messages,
            &mut |step| {
                if let TurnStep::Note(text) = step {
                    noted.push(text);
                }
            },
        ));
        (report, messages, noted)
    }

    #[test]
    fn a_turn_that_edited_in_a_workspace_with_no_checks_says_so_and_claims_nothing() {
        let (report, messages, noted) = close(false, false, false);
        assert!(matches!(
            report.outcome.checks,
            pwr_domain::ChecksOutcome::Unavailable { .. }
        ));
        assert!(!report.outcome.verified());
        assert!(noted[0].starts_with('–'), "{noted:?}");
        assert!(
            report
                .answer
                .contains("Independent verification unavailable")
        );
        // The model is told, in a message that is the harness's, not a tool's.
        let feedback = messages.last().unwrap();
        assert_eq!(feedback.role, "user");
        assert_eq!(
            feedback.purpose,
            Some(pwr_domain::MessagePurpose::VerificationFeedback)
        );
    }

    #[test]
    fn passing_checks_are_ticked_and_recorded_as_passed() {
        let (report, _, noted) = close(true, false, false);
        assert!(matches!(
            report.outcome.checks,
            pwr_domain::ChecksOutcome::Passed
        ));
        assert!(noted[0].starts_with('✓'), "{noted:?}");
        // Checks passing is not acceptance.
        assert!(!report.outcome.verified());
    }

    /// The defect of 2026-09-30: the note was prefixed with a tick whatever the
    /// verdict said. A check that fails is never shown with one.
    #[test]
    fn a_failing_check_is_never_shown_with_a_tick() {
        let (report, _, noted) = close(true, false, true);
        assert!(matches!(
            report.outcome.checks,
            pwr_domain::ChecksOutcome::Failed { .. }
        ));
        assert!(!noted[0].starts_with('✓'), "{noted:?}");
        assert!(noted[0].starts_with('✗'), "{noted:?}");
        assert!(report.answer.contains("did not pass"), "{}", report.answer);
    }

    #[test]
    fn a_check_that_was_already_failing_is_not_called_a_pass() {
        let (report, _, noted) = close(true, true, true);
        assert!(noted[0].starts_with('–'), "{noted:?}");
        assert!(
            report.answer.contains("is not verified"),
            "{}",
            report.answer
        );
    }

    // ------------------------------------------------ verified proposals (W2.9)

    /// A workspace on disk with one source file and its test, a model that
    /// answers a proposal with the next scripted file, and checks that fail
    /// by what the source file says: `BROKEN` three tests, anything else two,
    /// `HALF` one, `FIXED` none.
    struct Proposing {
        root: tempfile::TempDir,
        files: Mutex<Vec<String>>,
        asked: Mutex<Vec<(String, bool)>>,
        calls: Mutex<Vec<String>>,
        said: Mutex<Vec<String>>,
        kept: Mutex<Vec<ChatMessage>>,
        turns: AtomicUsize,
        refuse_edits: bool,
        /// Refuse an overwrite that drops more than half of the file, as
        /// `pwr_tools::apply_replace` does.
        shrink_guard: bool,
    }

    const SOURCE: &str = "src/dates.ts";

    impl Proposing {
        fn new(original: Option<&str>, files: &[&str]) -> Self {
            let root = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(root.path().join("src")).unwrap();
            std::fs::create_dir_all(root.path().join("test")).unwrap();
            std::fs::write(
                root.path().join("test/dates.test.ts"),
                "import { dueDate } from '../src/dates.ts';\n",
            )
            .unwrap();
            if let Some(original) = original {
                std::fs::write(root.path().join(SOURCE), original).unwrap();
            }
            Self {
                root,
                files: Mutex::new(files.iter().map(|file| (*file).to_owned()).collect()),
                asked: Mutex::default(),
                calls: Mutex::default(),
                said: Mutex::default(),
                kept: Mutex::default(),
                turns: AtomicUsize::new(0),
                refuse_edits: false,
                shrink_guard: false,
            }
        }

        fn source(&self) -> Option<String> {
            std::fs::read_to_string(self.root.path().join(SOURCE)).ok()
        }

        fn standing(&self) -> GoalVerification {
            let source = self.source().unwrap_or_default();
            let failed: &[&str] = if source.contains("FIXED") {
                &[]
            } else if source.contains("HALF") {
                &["a weekend"]
            } else if source.contains("BROKEN") {
                &["a weekend", "a weekday", "a holiday"]
            } else {
                &["a weekend", "a weekday"]
            };
            let failing: Vec<String> = if failed.is_empty() {
                Vec::new()
            } else {
                vec!["node --test".into()]
            };
            GoalVerification {
                passed: failing.is_empty(),
                technical_passed: failing.is_empty(),
                acceptance_available: true,
                failing_acceptance: failing.clone(),
                failing,
                failed_tests: failed.iter().map(|name| (*name).to_owned()).collect(),
                evidence: "test at test/dates.test.ts:11:1".into(),
                ..Default::default()
            }
        }

        fn run(&self, proposals: usize) -> SessionResult {
            let limits = GoalLimits {
                proposals: Some(proposals),
                ..Default::default()
            };
            let mut request = request(Policy::Goal);
            request.root = self.root.path().to_path_buf();
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                // A hung verification is waited out on the clock, not for real.
                .start_paused(true)
                .build()
                .unwrap()
                .block_on(execute(self, request, limits))
        }
    }

    fn body(text: &str) -> String {
        // Long enough to be a file and not a quote of part of one.
        format!(
            "export function dueDate(): string {{\n  return '{text}';\n}}\n{}",
            "// rule\n".repeat(40)
        )
    }

    #[async_trait::async_trait(?Send)]
    impl SessionHost for Proposing {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            let mut messages = input.messages;
            let Some(scripted) = input.scripted else {
                self.turns.fetch_add(1, Ordering::Relaxed);
                messages.push(ChatMessage::text("assistant", "done"));
                return Ok((report(0, true), messages));
            };
            let arguments = &scripted.call.arguments;
            let path = self.root.path().join(arguments["path"].as_str().unwrap());
            self.calls.lock().unwrap().push(scripted.call.name.clone());
            let mut done = report(1, false);
            if self.refuse_edits {
                done.declined = true;
                return Ok((done, messages));
            }
            match scripted.call.name.as_str() {
                "write_file" => {
                    assert!(!path.exists(), "write_file never overwrites");
                    std::fs::write(&path, arguments["content"].as_str().unwrap()).unwrap();
                }
                "apply_replace" => {
                    assert_eq!(
                        arguments["expected_hash"].as_str().unwrap(),
                        pwr_domain::hash_bytes(std::fs::read(&path).unwrap()),
                        "an overwrite is bound to the version it replaces"
                    );
                    let replacement = arguments["replacement"].as_str().unwrap();
                    let old = std::fs::read_to_string(&path).unwrap();
                    if self.shrink_guard && replacement.lines().count() * 2 < old.lines().count() {
                        messages.push(ChatMessage::text("assistant", "refused"));
                        return Ok((done, messages));
                    }
                    std::fs::write(&path, replacement).unwrap();
                }
                "delete_path" => std::fs::remove_file(&path).unwrap(),
                other => panic!("unexpected scripted call {other}"),
            }
            done.edited = true;
            done.answer = scripted.said.clone();
            messages.push(ChatMessage::text("assistant", scripted.said));
            Ok((done, messages))
        }

        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            if self.source().is_some_and(|source| source.contains("HANGS")) {
                // A file whose tests never return.
                tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
            }
            Ok((self.standing(), BTreeMap::new()))
        }

        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            Ok(String::new())
        }

        async fn author(
            &self,
            _: &Path,
            prompt: String,
            think: bool,
        ) -> Result<pwr_provider::ModelStream, String> {
            self.asked.lock().unwrap().push((prompt, think));
            let mut files = self.files.lock().unwrap();
            if files.is_empty() {
                return Err("no model".into());
            }
            let chunk = |content: String, done: bool| {
                Ok(pwr_domain::ModelChunk {
                    content,
                    done,
                    ..Default::default()
                })
            };
            let reply = format!("```typescript\n{}```\n", files.remove(0));
            Ok(Box::pin(futures_util::stream::iter([
                chunk(reply, false),
                chunk(String::new(), true),
            ])))
        }

        fn say(&self, text: &str) {
            self.said.lock().unwrap().push(text.to_owned());
        }

        fn keep_messages(&self, messages: &[ChatMessage]) {
            *self.kept.lock().unwrap() = messages.to_vec();
        }
    }

    #[test]
    fn a_goal_is_told_where_its_failing_checks_point_only_when_asked() {
        let run = |pointers: bool| {
            let host = Proposing::new(Some(&body("original")), &[]);
            let mut request = request(Policy::Goal);
            request.root = host.root.path().to_path_buf();
            request.aids = GoalAids {
                pointers,
                ..Default::default()
            };
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .start_paused(true)
                .build()
                .unwrap()
                .block_on(execute(&host, request, GoalLimits::default()));
            assert!(
                host.asked.lock().unwrap().is_empty(),
                "no proposals were asked for"
            );
            host.kept.lock().unwrap()[0].content.clone()
        };
        let told = run(true);
        assert!(told.starts_with("make the tests pass"));
        assert!(told.contains("Where the failing checks point."));
        assert!(told.contains("- src/dates.ts, used by test/dates.test.ts"));
        assert!(!run(false).contains("Where the failing checks point."));
    }

    #[test]
    fn proposals_are_off_unless_the_workspace_asks_for_them() {
        let host = Proposing::new(Some(&body("original")), &[&body("FIXED")]);
        let result = host.run(0);
        assert!(host.asked.lock().unwrap().is_empty());
        assert_eq!(host.source().unwrap(), body("original"));
        assert_eq!(result.budget.proposals, 0);
    }

    #[test]
    fn a_proposal_that_shrinks_the_failing_tests_is_kept_and_the_goal_goes_on_from_it() {
        let host = Proposing::new(Some(&body("original")), &[&body("HALF"), &body("FIXED")]);
        let result = host.run(4);
        assert_eq!(host.source().unwrap(), body("FIXED"));
        assert_eq!(
            *host.calls.lock().unwrap(),
            ["apply_replace", "apply_replace"]
        );
        assert_eq!(
            result.budget.proposals, 2,
            "it stops asking once the checks pass"
        );
        let said = host.said.lock().unwrap().join("");
        assert!(said.contains("Kept src/dates.ts: 1 of the owner's tests still fail"));
        // The ordinary goal follows, told what was done, and ends it verified.
        assert!(host.turns.load(Ordering::Relaxed) >= 1);
        let kept = host.kept.lock().unwrap();
        let guidance = kept
            .iter()
            .find(|message| message.purpose == Some(pwr_domain::MessagePurpose::GoalGuidance))
            .expect("the model is told what the phase did");
        assert!(
            guidance
                .content
                .contains("2 kept (src/dates.ts, src/dates.ts), 0 restored")
        );
        assert!(guidance.content.contains("The checks now pass."));
        assert!(matches!(
            result.end,
            SessionEnd::Reply { verification: Some(ref verification), .. } if verification.passed
        ));
        // The second request showed the checks' output on the file as it stood.
        let asked = host.asked.lock().unwrap();
        assert!(asked[1].0.contains("CURRENT src/dates.ts") && asked[1].0.contains("HALF"));
    }

    #[test]
    fn a_proposal_that_does_not_help_is_restored_and_named_to_the_next_request() {
        // One test more fails with it, then one that changes nothing.
        let host = Proposing::new(
            Some(&body("original")),
            &[&body("BROKEN"), &body("other"), &body("FIXED")],
        );
        let result = host.run(2);
        assert_eq!(
            host.source().unwrap(),
            body("original"),
            "both were put back"
        );
        assert_eq!(
            *host.calls.lock().unwrap(),
            [
                "apply_replace",
                "apply_replace",
                "apply_replace",
                "apply_replace"
            ]
        );
        assert_eq!(result.budget.proposals, 2, "the allowance is what ends it");
        let asked = host.asked.lock().unwrap();
        assert!(asked[1].0.contains("A PREVIOUS src/dates.ts WAS REFUSED"));
        assert!(asked[1].0.contains("It broke: a holiday"));
        let kept = host.kept.lock().unwrap();
        assert!(kept.iter().any(
            |message| message.content.contains("0 kept (none), 2 restored")
                && message.content.contains("2 tests still fail")
        ));
    }

    #[test]
    fn a_proposal_whose_checks_never_finish_is_restored_and_the_phase_goes_on() {
        let host = Proposing::new(Some(&body("original")), &[&body("HANGS"), &body("FIXED")]);
        let result = host.run(4);
        // Put back, said to the next request, and the next proposal is kept.
        assert_eq!(host.source().unwrap(), body("FIXED"));
        assert_eq!(
            *host.calls.lock().unwrap(),
            ["apply_replace", "apply_replace", "apply_replace"]
        );
        let asked = host.asked.lock().unwrap();
        assert!(
            asked[1]
                .0
                .contains("The checks did not finish within 20 s with this file")
        );
        assert!(result.budget.started.elapsed() < std::time::Duration::from_secs(120));
    }

    #[test]
    fn a_refused_proposal_much_longer_than_the_file_is_still_put_back() {
        let long = format!("{}{}", body("BROKEN"), "// more\n".repeat(200));
        let mut host = Proposing::new(Some(&body("original")), &[&long]);
        host.shrink_guard = true;
        host.run(1);
        assert_eq!(host.source().unwrap(), body("original"));
        assert_eq!(
            *host.calls.lock().unwrap(),
            [
                "apply_replace",
                "apply_replace",
                "delete_path",
                "write_file"
            ]
        );
        assert!(
            !host
                .kept
                .lock()
                .unwrap()
                .iter()
                .any(|message| message.content.contains("could not be restored"))
        );
    }

    #[test]
    fn a_file_created_by_a_refused_proposal_is_deleted_again() {
        let host = Proposing::new(None, &[&body("BROKEN")]);
        host.run(1);
        assert_eq!(host.source(), None);
        assert_eq!(*host.calls.lock().unwrap(), ["write_file", "delete_path"]);
    }

    #[test]
    fn an_edit_the_person_does_not_allow_ends_the_phase_and_not_the_goal() {
        let mut host = Proposing::new(Some(&body("original")), &[&body("FIXED"), &body("FIXED")]);
        host.refuse_edits = true;
        let result = host.run(4);
        assert_eq!(host.source().unwrap(), body("original"));
        assert_eq!(host.calls.lock().unwrap().len(), 1, "it does not ask again");
        assert_eq!(result.budget.proposals, 1);
        assert!(
            host.turns.load(Ordering::Relaxed) >= 1,
            "the ordinary goal still runs"
        );
        assert!(host.kept.lock().unwrap().iter().any(|message| {
            message
                .content
                .contains("Proposals stopped: an edit was not allowed.")
        }));
    }

    #[test]
    fn a_model_that_cannot_be_asked_leaves_the_goal_as_it_was() {
        let host = Proposing::new(Some(&body("original")), &[]);
        let result = host.run(3);
        assert!(host.calls.lock().unwrap().is_empty());
        assert_eq!(result.budget.proposals, 1);
        assert!(host.turns.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn one_file_is_asked_for_more_than_twice_before_the_phase_gives_way() {
        // A repository to write from nothing has one target: two refusals in
        // a row used to end the phase with most of its allowance unspent.
        let host = Proposing::new(
            None,
            &[
                &body("BROKEN"),
                &body("BROKEN two"),
                &body("BROKEN three"),
                &body("FIXED"),
            ],
        );
        let result = host.run(6);
        assert_eq!(host.source().unwrap(), body("FIXED"));
        assert_eq!(result.budget.proposals, 4);
    }

    #[test]
    fn a_phase_that_keeps_nothing_stops_at_its_patience_not_at_its_allowance() {
        let files: Vec<String> = (0..9).map(|n| body(&format!("BROKEN {n}"))).collect();
        let files: Vec<&str> = files.iter().map(String::as_str).collect();
        let host = Proposing::new(Some(&body("original")), &files);
        let result = host.run(9);
        assert_eq!(result.budget.proposals, PROPOSAL_PATIENCE);
        assert_eq!(host.source().unwrap(), body("original"));
    }

    #[test]
    fn an_unset_allowance_is_absent_from_the_configuration_and_zero_is_kept() {
        let limits: GoalLimits = serde_json::from_str("{}").unwrap();
        assert_eq!(limits.proposals, None);
        assert!(
            !serde_json::to_string(&limits)
                .unwrap()
                .contains("proposals")
        );
        let off: GoalLimits = serde_json::from_str(r#"{"proposals":0}"#).unwrap();
        assert_eq!(off.proposals, Some(0));
        assert!(
            serde_json::to_string(&off)
                .unwrap()
                .contains(r#""proposals":0"#)
        );
    }

    // ------------------------------------------------ a goal bounded by work

    /// A model that does `0` tokens of work a turn and never says it is done.
    struct Spends(u64);

    #[async_trait::async_trait(?Send)]
    impl SessionHost for Spends {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            // Asked to stop where the meter stands, as the turn itself checks.
            if input
                .continuity
                .work_limit
                .is_some_and(|limit| input.continuity.work.load(Ordering::Relaxed) >= limit)
            {
                panic!("a turn was started with no work left");
            }
            input
                .continuity
                .work
                .fetch_add(self.0 * converse::WORK_PER_TOKEN, Ordering::Relaxed);
            Ok((report(1, false), input.messages))
        }

        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            Ok((GoalVerification::default(), BTreeMap::new()))
        }

        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            Ok(String::new())
        }

        fn say(&self, _: &str) {}

        fn keep_messages(&self, _: &[ChatMessage]) {}
    }

    #[test]
    fn a_goal_with_a_work_allowance_ends_when_the_model_has_done_that_much() {
        let limits = GoalLimits {
            work: Some(2_500),
            ..Default::default()
        };
        let result = run(&Spends(1_000), Policy::Goal, limits);
        assert!(matches!(
            result.end,
            SessionEnd::OutOfBudget {
                reached: GoalLimitReached::Work {
                    spent: 3_000,
                    allowed: 2_500
                },
                ..
            }
        ));
        assert_eq!(result.budget.snapshot()["spent"]["work"], 3_000);
        assert_eq!(
            GoalLimitReached::Work {
                spent: 3_000,
                allowed: 2_500
            }
            .meta()["limit"],
            "work"
        );
    }

    #[test]
    fn without_a_work_allowance_work_is_counted_and_bounds_nothing() {
        let limits = GoalLimits {
            actions: 3,
            ..Default::default()
        };
        let result = run(&Spends(1_000_000), Policy::Goal, limits);
        assert!(matches!(
            result.end,
            SessionEnd::OutOfBudget {
                reached: GoalLimitReached::Actions { .. },
                ..
            }
        ));
        assert_eq!(result.budget.work_done(), 3_000_000);
        // Absent from a saved configuration, so an older build reads it.
        assert!(
            !serde_json::to_string(&GoalLimits::default())
                .unwrap()
                .contains("work")
        );
    }

    /// Says whether the turn it was given keeps a plan, then is done.
    struct SeesPlan(Mutex<Vec<bool>>);

    #[async_trait::async_trait(?Send)]
    impl SessionHost for SeesPlan {
        async fn run_turn(
            &self,
            input: TurnInput,
        ) -> Result<(TurnReport, Vec<ChatMessage>), String> {
            self.0.lock().unwrap().push(input.continuity.plan.is_some());
            Ok((report(0, true), input.messages))
        }

        async fn verify(
            &self,
        ) -> Result<(GoalVerification, BTreeMap<String, String>), VerifyError> {
            Ok((GoalVerification::default(), BTreeMap::new()))
        }

        async fn review(&self, _: &Path, _: String) -> Result<String, String> {
            Ok(String::new())
        }

        fn say(&self, _: &str) {}

        fn keep_messages(&self, _: &[ChatMessage]) {}
    }

    #[test]
    fn a_goal_keeps_a_plan_only_when_the_workspace_asks_and_a_conversation_never() {
        let saw = |policy: Policy, plan: bool| {
            let host = SeesPlan(Mutex::default());
            let mut request = request(policy);
            request.aids = GoalAids {
                plan,
                ..Default::default()
            };
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .unwrap()
                .block_on(execute(&host, request, GoalLimits::default()));
            host.0.into_inner().unwrap()
        };
        assert!(saw(Policy::Goal, true).iter().all(|kept| *kept));
        assert!(saw(Policy::Goal, false).iter().all(|kept| !*kept));
        assert_eq!(saw(Policy::Conversation, true), [false]);
    }
}
