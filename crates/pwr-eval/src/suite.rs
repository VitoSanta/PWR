//! Regression suites: the fast half of the evaluation regime (areas A1-A4).
//!
//! A campaign answers whether PWR is capable; a suite answers whether a
//! harness change broke or fixed something, the same day, in seconds to
//! minutes. Every case comes from a recorded failure (or a recorded success
//! worth keeping), names where it came from, and states the *right* outcome.
//! Where PWR does not yet produce it, the case says so under `known_gap`,
//! so the suite measures the gap without failing on it and reports the day
//! the gap closes.
//!
//! One format for every area. A case carries its `type`:
//!
//! - `replay` feeds a model's recorded raw reply through the reading a run
//!   applies (area A1). Needs no model.
//! - `task` names a task in a corpus file. The tasks are run the ordinary way
//!   (`pwr eval run <corpus> --out-dir D`) and the suite is scored from what
//!   that wrote (`pwr eval suite <file> --reports D`), so running and
//!   judging stay separate and this module never starts a model.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Area {
    A1,
    A2,
    A3,
    A4,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suite {
    pub schema_version: u32,
    pub id: String,
    pub area: Area,
    pub description: String,
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Case {
    Replay(ReplayCase),
    Task(TaskCase),
}

impl Case {
    pub fn id(&self) -> &str {
        match self {
            Case::Replay(case) => &case.id,
            Case::Task(case) => &case.id,
        }
    }

    fn why(&self) -> &str {
        match self {
            Case::Replay(case) => &case.why,
            Case::Task(case) => &case.why,
        }
    }

    fn known_gap(&self) -> Option<&str> {
        match self {
            Case::Replay(case) => case.known_gap.as_deref(),
            Case::Task(case) => case.known_gap.as_deref(),
        }
    }
}

/// A model's reply as it was generated, and what reading it should produce.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCase {
    pub id: String,
    /// Where the reply was recorded: experiment, trace, turn.
    pub origin: String,
    /// The family the adapter is chosen by (a model's `model_type`), if known.
    #[serde(default)]
    pub family: Option<String>,
    pub model_ref: String,
    /// The answer channel, verbatim.
    pub content: String,
    #[serde(default)]
    pub thinking: String,
    pub expect: Expect,
    /// Why this outcome is the right one.
    pub why: String,
    /// Present when PWR does not yet produce `expect`: what it does instead
    /// and why that is a gap rather than the intended behaviour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_gap: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expect {
    /// The turn decodes to these actions, in order. Each lists the capability
    /// and the fields that matter; fields not listed are not compared.
    Actions { actions: Vec<ExpectedAction> },
    /// The turn is refused as malformed, with this kind.
    Malformed { malformed: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpectedAction {
    pub capability: String,
    #[serde(default)]
    pub fields: serde_json::Map<String, serde_json::Value>,
}

/// A task run by a model, and what its run should show.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCase {
    pub id: String,
    /// The corpus file holding the task, relative to the repository root.
    pub corpus: String,
    /// The task's id in that corpus.
    pub task: String,
    /// The recorded failure it is modelled on.
    pub origin: String,
    pub expect: TaskExpect,
    pub why: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub known_gap: Option<String>,
}

/// What a task's run should show. Every expectation present must hold, and
/// always: nothing changed out of scope, nothing claimed falsely.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskExpect {
    /// Visible and hidden verifiers both pass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved: Option<bool>,
    /// A repository question's answer contains what the task requires, and
    /// names none of its forbidden (invented) symbols.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered: Option<bool>,
}

/// What a run did to the files, read from its trace: area A3's measures.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EditMeasures {
    /// Edit actions proposed: replace_text, apply_patch, apply_replace,
    /// write_file.
    pub attempted: usize,
    /// Of those, the ones that changed a file.
    pub applied: usize,
    /// Refused because the text to find was not in the file: the escape and
    /// whitespace failures.
    pub find_missed: usize,
    /// Refused by the guard against replacing most of a file.
    pub shrink_refused: usize,
    /// Refused as stale: the file had changed since its hash was read.
    pub stale: usize,
    pub restores: usize,
    /// Edit attempts before the first that applied; 0 is applied first time.
    pub attempts_before_first_applied: Option<usize>,
}

/// How a run checked its work, read from its trace: area A4's measures.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VerifyMeasures {
    /// Commands the run itself ran that are test runs (unittest, pytest).
    pub checks_run: usize,
    /// Completions the run declared.
    pub completes: usize,
    /// Of those, the ones the harness's own verification refused.
    pub completes_refused: usize,
}

/// How a run found its way, read from its outcome: area A2's measures.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NavigateMeasures {
    pub reads: usize,
    pub searches: usize,
    /// Reads of a file already shown and unchanged since.
    pub rereads_unchanged: u64,
    pub peak_prompt_tokens: u64,
    pub prompt_tokens: u64,
}

/// What reading a reply, or scoring a task's run, produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observed {
    Actions {
        actions: Vec<serde_json::Value>,
    },
    Malformed {
        malformed: String,
        problem: String,
    },
    Task {
        resolved: bool,
        visible: bool,
        hidden: bool,
        declared: bool,
        false_completion: bool,
        out_of_scope: Vec<String>,
        turns: u64,
        minutes: f64,
        edits: EditMeasures,
        verification: VerifyMeasures,
        navigation: NavigateMeasures,
        /// A repository question's answer matched what it requires.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        answer_matched: Option<bool>,
        /// It named a symbol the task lists as invented.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        forbidden_mentioned: Option<bool>,
        /// Test files the run changed, allowed or not.
        tests_changed: Vec<String>,
        end: Option<String>,
    },
    /// No report for this task was found under the reports directory given,
    /// or none was given.
    NotRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The expected outcome, as expected.
    Holds,
    /// Not the expected outcome, and no gap was declared: a regression.
    Fails,
    /// Not the expected outcome, and the case says so.
    KnownGap,
    /// A declared gap now produces the expected outcome: promote the case.
    GapClosed,
    /// A task case with no run to score.
    NotRun,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub id: String,
    pub verdict: Verdict,
    pub observed: Observed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteReport {
    pub suite: String,
    pub area: Area,
    pub harness_rev: String,
    pub cases: usize,
    pub verdicts: BTreeMap<String, usize>,
    /// Replay cases: the share that decoded to actions (area A1's headline).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decoded_rate: Option<f64>,
    /// Replay cases: refusals by malformed kind.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub malformed_by_kind: BTreeMap<String, usize>,
    /// Task cases that ran: edit measures summed over them (area A3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edits: Option<EditMeasures>,
    /// Task cases that ran: verification measures summed over them (area A4).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerifyMeasures>,
    /// Task cases that ran: completions declared while a verifier failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub false_completions: Option<usize>,
    /// Cases that regressed: not the expected outcome, no gap declared.
    pub regressions: Vec<String>,
    /// Task cases that ran: how many applied their first edit attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_first_time: Option<String>,
    pub results: Vec<CaseResult>,
}

impl SuiteReport {
    /// A suite passes when nothing regressed. Known gaps are measured, not
    /// failed on; a closed gap is good news that still needs the file updated.
    pub fn regressions(&self) -> Vec<&CaseResult> {
        self.results
            .iter()
            .filter(|result| result.verdict == Verdict::Fails)
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SuiteError {
    #[error("suite file: {0}")]
    Io(#[from] std::io::Error),
    #[error("suite file does not parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("suite is unsound: {0}")]
    Unsound(String),
}

pub fn load(path: &Path) -> Result<Suite, SuiteError> {
    let suite: Suite = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let mut seen = std::collections::BTreeSet::new();
    for case in &suite.cases {
        if !seen.insert(case.id().to_owned()) {
            return Err(SuiteError::Unsound(format!(
                "case id {} repeats",
                case.id()
            )));
        }
        if case.why().trim().is_empty() {
            return Err(SuiteError::Unsound(format!(
                "case {} does not say why its outcome is the right one",
                case.id()
            )));
        }
    }
    Ok(suite)
}

fn observe_replay(case: &ReplayCase) -> Observed {
    match pwr_orchestrator::decode_reply(
        case.family.as_deref(),
        &case.model_ref,
        &case.content,
        &case.thinking,
    ) {
        Ok(actions) => Observed::Actions {
            actions: actions
                .iter()
                .map(|action| serde_json::to_value(action).unwrap_or_default())
                .collect(),
        },
        Err(malformed) => Observed::Malformed {
            malformed: malformed.kind.to_owned(),
            problem: malformed.problem,
        },
    }
}

fn replay_matches(expect: &Expect, observed: &Observed) -> bool {
    match (expect, observed) {
        (Expect::Actions { actions: wanted }, Observed::Actions { actions: got }) => {
            wanted.len() == got.len()
                && wanted.iter().zip(got).all(|(want, action)| {
                    action.get("capability").and_then(serde_json::Value::as_str)
                        == Some(want.capability.as_str())
                        && want
                            .fields
                            .iter()
                            .all(|(key, value)| action.get(key) == Some(value))
                })
        }
        (Expect::Malformed { malformed: wanted }, Observed::Malformed { malformed: got, .. }) => {
            wanted == got
        }
        _ => false,
    }
}

const EDIT_CAPABILITIES: [&str; 4] = ["replace_text", "apply_patch", "apply_replace", "write_file"];

/// Edit measures from a run's event trace.
pub fn edit_measures(trace: &str) -> EditMeasures {
    let mut measures = EditMeasures::default();
    for line in trace.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event["event_type"] != "tool.action" {
            continue;
        }
        let payload = &event["payload"];
        let capability = payload["action"]["capability"].as_str().unwrap_or_default();
        if capability == "restore_file" && payload["outcome"].is_object() {
            measures.restores += 1;
        }
        if !EDIT_CAPABILITIES.contains(&capability) {
            continue;
        }
        measures.attempted += 1;
        if payload["outcome"].is_object() {
            measures.applied += 1;
            if measures.attempts_before_first_applied.is_none() {
                measures.attempts_before_first_applied = Some(measures.attempted - 1);
            }
            continue;
        }
        let refusal = format!(
            "{} {}",
            payload["denial"].as_str().unwrap_or_default(),
            payload["failure"].as_str().unwrap_or_default()
        );
        if refusal.contains("does not appear in the file") {
            measures.find_missed += 1;
        } else if refusal.contains("replaces the whole file") {
            measures.shrink_refused += 1;
        } else if refusal.contains("stale file hash") {
            measures.stale += 1;
        }
    }
    measures
}

/// Verification measures from a run's event trace.
pub fn verify_measures(trace: &str) -> VerifyMeasures {
    let mut measures = VerifyMeasures::default();
    for line in trace.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let payload = &event["payload"];
        match event["event_type"].as_str() {
            Some("tool.action") => {
                let action = &payload["action"];
                match action["capability"].as_str() {
                    Some("complete") => measures.completes += 1,
                    Some("run_command") => {
                        let argv = format!("{} {}", action["executable"], action["args"]);
                        if argv.contains("unittest") || argv.contains("pytest") {
                            measures.checks_run += 1;
                        }
                    }
                    _ => {}
                }
            }
            Some("task.transition") if payload["detail"] == "deterministic verification failed" => {
                measures.completes_refused += 1;
            }
            _ => {}
        }
    }
    measures
}

fn navigate_measures(trace: &str, outcome: &serde_json::Value) -> NavigateMeasures {
    let mut measures = NavigateMeasures {
        rereads_unchanged: outcome["mechanism"]["rereads_unchanged"]
            .as_u64()
            .unwrap_or(0),
        peak_prompt_tokens: outcome["peak_prompt_tokens"].as_u64().unwrap_or(0),
        prompt_tokens: outcome["prompt_tokens"].as_u64().unwrap_or(0),
        ..NavigateMeasures::default()
    };
    for line in trace.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event["event_type"] != "tool.action" {
            continue;
        }
        match event["payload"]["action"]["capability"].as_str() {
            Some("read_file") => measures.reads += 1,
            Some("search" | "find_definition") => measures.searches += 1,
            _ => {}
        }
    }
    measures
}

fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    !path.contains("__pycache__")
        && (path.starts_with("tests/")
            || path.contains("/tests/")
            || name.starts_with("test_")
            || name.ends_with("_test.py"))
}

/// The latest recorded outcome of `task` under `reports`, with its trace.
fn find_outcome(reports: &Path, task: &str) -> Option<(serde_json::Value, String)> {
    let mut found: Option<(String, serde_json::Value)> = None;
    for campaign in std::fs::read_dir(reports).ok()?.flatten() {
        if !campaign
            .file_name()
            .to_string_lossy()
            .starts_with("trials-")
        {
            continue;
        }
        let Ok(trials) = std::fs::read_dir(campaign.path()) else {
            continue;
        };
        for trial in trials.flatten() {
            if !trial
                .file_name()
                .to_string_lossy()
                .ends_with("-outcome.json")
            {
                continue;
            }
            let Some(record) = std::fs::read_to_string(trial.path())
                .ok()
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            else {
                continue;
            };
            if record["outcome"]["task_id"] != task {
                continue;
            }
            let completed = record["completed_at"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            if found.as_ref().is_none_or(|(latest, _)| completed > *latest) {
                found = Some((completed, record["outcome"].clone()));
            }
        }
    }
    let (_, outcome) = found?;
    let trace = outcome["event_trace"]
        .as_str()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default();
    Some((outcome, trace))
}

fn observe_task(case: &TaskCase, reports: Option<&Path>) -> Observed {
    let Some((outcome, trace)) = reports.and_then(|dir| find_outcome(dir, &case.task)) else {
        return Observed::NotRun;
    };
    let flag = |key: &str| outcome[key].as_bool().unwrap_or(false);
    let visible = flag("visible_verifier_passed_after");
    let hidden = flag("hidden_verifier_passed");
    let declared = flag("declared_complete");
    // A repository question is answered, not repaired: its checks may be red
    // on purpose ("find the defect, do not fix it"), so what a completion
    // claims is the answer.
    let answer_matched = outcome["answer_matched"].as_bool();
    let forbidden_mentioned = outcome["forbidden_symbol_mentioned"].as_bool();
    let claim_holds = match answer_matched {
        Some(matched) => matched && !forbidden_mentioned.unwrap_or(false),
        None => visible && hidden,
    };
    Observed::Task {
        resolved: visible && hidden,
        visible,
        hidden,
        declared,
        false_completion: declared && !claim_holds,
        out_of_scope: outcome["out_of_scope_changes"]
            .as_array()
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(|path| path.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        turns: outcome["turns"].as_u64().unwrap_or(0),
        minutes: (outcome["duration_secs"].as_f64().unwrap_or(0.0) / 6.0).round() / 10.0,
        edits: edit_measures(&trace),
        verification: verify_measures(&trace),
        navigation: navigate_measures(&trace, &outcome),
        answer_matched,
        forbidden_mentioned,
        tests_changed: outcome["changed_files"]
            .as_array()
            .map(|paths| {
                paths
                    .iter()
                    .filter_map(serde_json::Value::as_str)
                    .filter(|path| is_test_file(path))
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
        end: outcome["error"].as_str().map(str::to_owned),
    }
}

fn task_holds(expect: &TaskExpect, observed: &Observed) -> bool {
    match observed {
        Observed::Task {
            resolved,
            false_completion,
            out_of_scope,
            answer_matched,
            forbidden_mentioned,
            ..
        } => {
            let answered = answer_matched.unwrap_or(false) && !forbidden_mentioned.unwrap_or(false);
            expect.resolved.is_none_or(|want| want == *resolved)
                && expect.answered.is_none_or(|want| want == answered)
                && !false_completion
                && out_of_scope.is_empty()
        }
        _ => false,
    }
}

/// Scores a suite. `reports` is where its task cases were run; replay cases
/// need nothing.
pub fn run(suite: &Suite, harness_rev: &str, reports: Option<&Path>) -> SuiteReport {
    let mut results = Vec::new();
    let mut verdicts: BTreeMap<String, usize> = BTreeMap::new();
    let mut malformed_by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let (mut replays, mut decoded) = (0usize, 0usize);
    let mut edits: Option<EditMeasures> = None;
    let mut verification: Option<VerifyMeasures> = None;
    let mut false_completions = 0usize;
    let (mut ran, mut first_time) = (0usize, 0usize);
    for case in &suite.cases {
        let (observed, holds) = match case {
            Case::Replay(replay) => {
                replays += 1;
                let observed = observe_replay(replay);
                match &observed {
                    Observed::Actions { .. } => decoded += 1,
                    Observed::Malformed { malformed, .. } => {
                        *malformed_by_kind.entry(malformed.clone()).or_default() += 1;
                    }
                    _ => {}
                }
                let holds = replay_matches(&replay.expect, &observed);
                (observed, holds)
            }
            Case::Task(task) => {
                let observed = observe_task(task, reports);
                if let Observed::Task {
                    edits: measured,
                    verification: checked,
                    false_completion,
                    ..
                } = &observed
                {
                    ran += 1;
                    false_completions += usize::from(*false_completion);
                    let checks = verification.get_or_insert_with(VerifyMeasures::default);
                    checks.checks_run += checked.checks_run;
                    checks.completes += checked.completes;
                    checks.completes_refused += checked.completes_refused;
                    if measured.attempts_before_first_applied == Some(0) {
                        first_time += 1;
                    }
                    let total = edits.get_or_insert_with(EditMeasures::default);
                    total.attempted += measured.attempted;
                    total.applied += measured.applied;
                    total.find_missed += measured.find_missed;
                    total.shrink_refused += measured.shrink_refused;
                    total.stale += measured.stale;
                    total.restores += measured.restores;
                }
                let holds = task_holds(&task.expect, &observed);
                (observed, holds)
            }
        };
        let verdict = if observed == Observed::NotRun {
            Verdict::NotRun
        } else {
            match (holds, case.known_gap().is_some()) {
                (true, false) => Verdict::Holds,
                (true, true) => Verdict::GapClosed,
                (false, true) => Verdict::KnownGap,
                (false, false) => Verdict::Fails,
            }
        };
        let name = serde_json::to_value(verdict)
            .ok()
            .and_then(|value| value.as_str().map(str::to_owned))
            .unwrap_or_default();
        *verdicts.entry(name).or_default() += 1;
        results.push(CaseResult {
            id: case.id().to_owned(),
            verdict,
            observed,
        });
    }
    let regressions = results
        .iter()
        .filter(|result: &&CaseResult| result.verdict == Verdict::Fails)
        .map(|result| result.id.clone())
        .collect();
    SuiteReport {
        regressions,
        suite: suite.id.clone(),
        area: suite.area,
        harness_rev: harness_rev.to_owned(),
        cases: suite.cases.len(),
        verdicts,
        decoded_rate: (replays > 0).then(|| decoded as f64 / replays as f64),
        malformed_by_kind,
        edits,
        verification,
        false_completions: (ran > 0).then_some(false_completions),
        applied_first_time: (ran > 0).then(|| format!("{first_time} of {ran} runs")),
        results,
    }
}
