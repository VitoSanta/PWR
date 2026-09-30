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
         They are not part of this goal. Do not repair them -- not their configuration, not their dependencies -- \
         unless the engineer asks; mention them in your answer instead. They do not block completion.",
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
            wall: GOAL_MAX_WALL,
        }
    }
}

/// A limit a goal reached, and how much of it was spent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalLimitReached {
    Actions { spent: usize, allowed: usize },
    RefusedCompletions { spent: usize, allowed: usize },
    Time { spent_secs: u64, allowed_secs: u64 },
    VerificationRuns { spent: usize, allowed: usize },
    ReviewRounds { spent: usize, allowed: usize },
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
        }
    }
    pub fn reached(&self) -> Option<GoalLimitReached> {
        self.limits
            .reached(self.actions, self.refused, self.started.elapsed())
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
        json!({"limits": self.limits, "spent": {"actions": self.actions, "refused_completions": self.refused, "verification_runs": self.verifications, "review_rounds": self.reviews, "wall_seconds": self.started.elapsed().as_secs()}})
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

/// How a request is run: one turn, or turns repeated until the work is
/// verified, paused or out of budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    Conversation,
    Goal,
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
        continuity,
        approvals,
        session_grants,
        policy,
    } = request;
    let goal_mode = policy == Policy::Goal;
    // A deadline covers the operation in progress, not just loop boundaries.
    // Set the shared cancellation flag before dropping its future so the
    // managed inference worker also sees cancellation.
    macro_rules! bounded {
        ($operation:expr) => {
            bounded!($operation, false)
        };
        ($operation:expr, $running_turn:expr) => {{
            if let Some(reached) = budget.reached() {
                return SessionEnd::OutOfBudget {
                    total_actions: budget.actions,
                    reached,
                };
            }
            match tokio::time::timeout(budget.remaining(), $operation).await {
                Ok(value) if budget.started.elapsed() < budget.limits.wall => value,
                _ => {
                    stop.store(true, Ordering::Relaxed);
                    if $running_turn && let Ok(checkpoint) = continuity.checkpoint.lock() {
                        budget.actions = budget.actions.saturating_add(checkpoint.actions);
                    }
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
    if goal_mode
        && let Ok((baseline, _)) = verify!()
        && !baseline.failing.is_empty()
    {
        already_failing = baseline.failing;
        if let Some(last) = messages.last_mut().filter(|last| last.role == "user") {
            last.content
                .push_str(&format!("\n\n{}", already_failing_note(&already_failing)));
        }
    }
    let mut idle_rounds = 0usize;
    let mut same_failure: (Vec<String>, usize) = (Vec::new(), 0);
    let mut noticed_at: Option<usize> = None;
    // The verification that passed before the review round, kept so a
    // review that changes nothing ends on it without re-running checks.
    let mut reviewed: Option<GoalVerification> = None;
    let mut review_done = false;
    let mut goal_edited = false;
    loop {
        // Before the turn, on every way round: the limits are not one
        // branch's business.
        if goal_mode && let Some(reached) = budget.reached() {
            return SessionEnd::OutOfBudget {
                total_actions,
                reached,
            };
        }
        let base = highest_call.get();
        let mut turn_continuity = continuity.clone();
        if goal_mode {
            if let Ok(mut checkpoint) = continuity.checkpoint.lock() {
                checkpoint.actions = 0;
            }
            turn_continuity.action_limit =
                Some(budget.limits.actions.saturating_sub(total_actions));
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
        });
        let outcome = if goal_mode {
            bounded!(turn, true)
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
        if report.completed {
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
}
