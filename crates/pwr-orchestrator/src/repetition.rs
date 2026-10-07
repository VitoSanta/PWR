//! A deployment proposing the same refused action again.
//!
//! This is not a deployment short of budget. It is a deployment that is not
//! reading the refusal, and more actions buy more repeats. Saying so plainly is
//! the only thing that has not already been tried.
//!
//! Extracted from the scripted loop because the conversation did not have it.
//! The loop where the work now happens could propose the same stale-hash edit
//! until its turn ran out, with nothing naming the repetition and nothing in the
//! audit to find afterwards -- the pathology detectors in `pwr-observe` read
//! `loop.detected`, and a conversation never emitted one. The run's behaviour is
//! unchanged: same threshold, same event, same words.

use crate::ActionProposal;

/// Proposals of the same action that will be tolerated before the repetition
/// itself is named.
///
/// Three, because two can be a deployment correcting one argument and getting
/// it wrong again, which is work rather than a loop.
pub const REPEATED_REFUSAL_LIMIT: usize = 3;

/// What makes two proposals the same proposal.
///
/// Deliberately not the whole action: the same command on different input, the
/// same pattern searched literally and as a regular expression, the same file
/// read from different lines are different questions. A fingerprint that cannot
/// tell them apart reports a deployment that is narrowing its search as one
/// that is repeating itself.
pub fn action_fingerprint(action: &ActionProposal) -> String {
    crate::action_fingerprint(action)
}

/// Refused proposals since the last one that was not refused.
///
/// A refusal followed by a success is recovery rather than a loop, so progress
/// clears the streak. Repetition is counted per fingerprint rather than in
/// total: a deployment that is refused three different ways is failing at three
/// things, and telling it that it repeated itself would be false.
#[derive(Debug, Default)]
pub struct RefusalStreak {
    refused: Vec<String>,
}

impl RefusalStreak {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether another attempt at this proposal would buy anything.
    ///
    /// Clears the streak when it answers yes, so the deployment gets the whole
    /// allowance again after being told. Telling it once and then refusing
    /// everything it sends would end a turn that might still recover.
    pub fn would_only_repeat(&mut self, fingerprint: &str) -> bool {
        if self
            .refused
            .iter()
            .filter(|seen| *seen == fingerprint)
            .count()
            < REPEATED_REFUSAL_LIMIT
        {
            return false;
        }
        self.refused.clear();
        true
    }

    /// Records what became of a proposal that ran.
    ///
    /// Read from the outcome rather than from the proposal: an action that was
    /// refused contributes, and one that succeeded clears what came before it.
    pub fn observe(&mut self, fingerprint: &str, outcome: &serde_json::Value) {
        if outcome.get("denied").is_some() {
            self.refused.push(fingerprint.to_owned());
        } else {
            self.refused.clear();
        }
    }

    /// Records a refusal that never reached an outcome, such as a proposal the
    /// loop declined to run at all.
    pub fn refused(&mut self, fingerprint: &str) {
        self.refused.push(fingerprint.to_owned());
    }
}

/// What the deployment is told instead of the action being run again.
///
/// Names the way out rather than only the problem: a refusal that says stop and
/// nothing else costs a turn to work around, and the two ways out of a repeated
/// refusal are nearly always re-reading for a current hash or admitting the work
/// is finished.
pub fn repetition_notice() -> String {
    format!(
        "{{\"stop\":\"You have proposed this same action {REPEATED_REFUSAL_LIMIT} \
         times and it has been refused every time. It will not succeed on \
         another attempt. Read the refusal, then do something different: \
         re-read the file to get its current hash, or call complete if the \
         work is done.\"}}"
    )
}

/// Identical results of the same action that will be tolerated before the
/// echo is named, and how far back they are looked for.
pub const ECHO_LIMIT: usize = 3;
pub const ECHO_WINDOW: usize = 10;

/// The same action giving the same result, again and again.
///
/// `RefusalStreak` sees a deployment repeating what is refused; this sees one
/// repeating what *succeeds* and changes nothing. Measured 2026-09-26 (stack
/// matrix, C++ task): the same download answered 404 and the same delete
/// cleared its error page, twelve times in a row -- every action allowed,
/// every workspace change undone by the next, so neither the refusal streak
/// nor the no-progress detector (which reads the workspace) saw a loop.
///
/// Results are compared with their numbers masked, so a duration or a
/// timestamp does not make the same failure look new.
#[derive(Debug, Default)]
pub struct Echoes {
    recent: std::collections::VecDeque<(String, u64, bool)>,
}

impl Echoes {
    /// Failures before a real file change cannot predict its next check.
    pub fn inputs_changed(&mut self) {
        self.recent.retain(|(_, _, failed)| !failed);
    }
    /// Records a result; returns how many times it has now been seen, when
    /// that is the limit or more.
    pub fn observe(&mut self, fingerprint: &str, outcome: &serde_json::Value) -> Option<usize> {
        let digest = outcome_digest(outcome);
        self.recent
            .push_back((fingerprint.to_owned(), digest, failed(outcome)));
        if self.recent.len() > ECHO_WINDOW {
            self.recent.pop_front();
        }
        let seen = self
            .recent
            .iter()
            .filter(|(seen, result, _)| seen == fingerprint && *result == digest)
            .count();
        (seen >= ECHO_LIMIT).then_some(seen)
    }

    /// How many times this action has just failed with the same result as
    /// its last run, counting that run.
    pub fn repeated_failures(&self, fingerprint: &str) -> usize {
        let mut runs = self
            .recent
            .iter()
            .rev()
            .filter(|(seen, _, _)| seen == fingerprint);
        let Some((_, last, true)) = runs.next() else {
            return 0;
        };
        1 + runs
            .take_while(|(_, result, failure)| *failure && result == last)
            .count()
    }
}

/// Failures of the same action, with the same result, after which it is not
/// run again.
pub const REPEATED_FAILURE_LIMIT: usize = 2;

/// Whether a result is a failure: a command that exited non-zero, or a tool
/// that reports one.
pub fn failed(outcome: &serde_json::Value) -> bool {
    outcome
        .get("exit_code")
        .is_some_and(|code| code.as_i64() != Some(0))
        || outcome
            .get("failure")
            .is_some_and(|failure| !failure.is_null())
}

/// Said instead of running an action that failed the same way twice.
pub fn repeated_failure_notice(seen: usize) -> String {
    format!(
        "Not run: this exact command has failed {seen} times in a row with the same result, so \
         running it again would fail again. Change what it depends on or run something \
         different -- or, if you cannot see what is wrong, stop here and tell the engineer what \
         is ready and what did not work."
    )
}

/// Consecutive failed runs after which a turn that has produced its files
/// stops trying to run them and hands them over.
///
/// Measured 2026-09-29 (Qwen3-14B in the desktop): a bank page and its
/// Playwright test were written in seven minutes and right; the next forty
/// minutes were fourteen failed runs of that test -- a wrong `file://` path,
/// browsers to download, a server started four ways -- and the person
/// stopped a turn whose work had been done since the start.
pub const FAILED_RUN_LIMIT: usize = 5;

/// Failed runs in a row within a turn, and whether the hand-over was asked.
#[derive(Debug, Default)]
pub struct FailedRuns {
    streak: usize,
    handed_over: bool,
}

impl FailedRuns {
    /// A successful file change starts another repair/check cycle.
    pub fn inputs_changed(&mut self) {
        self.streak = 0;
        self.handed_over = false;
    }
    /// Records a run; returns the hand-over notice the first time the
    /// streak reaches the limit in a turn that has changed files.
    pub fn observe(&mut self, failed: bool, edited: bool) -> Option<String> {
        self.streak = if failed { self.streak + 1 } else { 0 };
        (edited && !self.handed_over && self.streak >= FAILED_RUN_LIMIT).then(|| {
            self.handed_over = true;
            format!(
                "Stop running things now: the last {FAILED_RUN_LIMIT} runs all failed without a successful file change. Any possible \
                 filesystem effects are kept. Inspect what exists and answer the engineer: say what was produced, what \
                 you tried to run and why it failed, and the exact commands they can run \
                 themselves. Further commands this turn will not be run."
            )
        })
    }

    /// Whether this turn has already been asked to hand over.
    pub fn handed_over(&self) -> bool {
        self.handed_over
    }
}

/// Changes to one file, each followed by a failed run, after which the model
/// is shown the lines the failure names as they stand on disk.
pub const STILL_FAILING_LIMIT: usize = 3;
/// Lines of the workspace quoted with that notice.
const QUOTED_LINES: usize = 3;

/// One file changed again and again with a run failing after each change.
///
/// [`Echoes`] and [`FailedRuns`] both start over at a successful file change,
/// since a change is what a repair looks like. Seen 2026-10-07 (Nemotron 30B,
/// a Python CLI from nothing): the model wrote `x ifoggi_str else None` for
/// `x if oggi_str else None`, the tests failed on that line, and it rewrote
/// the whole file some twenty times with the same line in it -- every write
/// allowed, every failure "new" -- reasoning about a space it could not see
/// it was not writing, until the engine ran out of memory. What it never did
/// was look at the line as the file has it, or write it another way.
#[derive(Debug, Default)]
pub struct StillFailing {
    /// The file changed since the last run, if one was.
    changed: Option<String>,
    /// The file the streak is about, and how many of its changes a failed
    /// run has followed.
    file: Option<String>,
    survived: usize,
    /// The runs that failed in this streak: one of them passing ends it,
    /// where an `ls` that works says nothing about the failure.
    failing: Vec<String>,
}

impl StillFailing {
    /// A file was changed.
    pub fn changed(&mut self, path: &str) {
        self.changed = Some(path.to_owned());
    }

    /// Records a run; returns the file and the count when another change to
    /// it has just been followed by a failure, at the limit and at each
    /// multiple of it.
    pub fn observe(&mut self, fingerprint: &str, failed: bool) -> Option<(String, usize)> {
        if !failed {
            if self.failing.iter().any(|run| run == fingerprint) {
                *self = Self::default();
            }
            return None;
        }
        if !self.failing.iter().any(|run| run == fingerprint) {
            self.failing.push(fingerprint.to_owned());
        }
        let changed = self.changed.take()?;
        if self.file.as_deref() == Some(changed.as_str()) {
            self.survived += 1;
        } else {
            self.file = Some(changed.clone());
            self.survived = 1;
        }
        (self.survived >= STILL_FAILING_LIMIT && self.survived.is_multiple_of(STILL_FAILING_LIMIT))
            .then_some((changed, self.survived))
    }
}

/// The places a failure names -- `File "a.py", line 12`, `src/a.rs:12:5`,
/// `a.ts(12,5)` -- that are files of the workspace, with that line as the
/// file has it now. Whatever the language: a path the output writes beside a
/// line number.
pub fn named_lines(root: &std::path::Path, output: &str) -> Vec<String> {
    let mut found: Vec<(String, usize)> = Vec::new();
    let mut note = |path: &str, line: &str| {
        let path = path.trim_matches(|c: char| "\"'`()[]<>,".contains(c));
        let path = path.strip_prefix("./").unwrap_or(path);
        let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(line) = digits.parse::<usize>()
            && line > 0
            && !path.is_empty()
            && !found.iter().any(|(seen, at)| seen == path && *at == line)
        {
            found.push((path.to_owned(), line));
        }
    };
    for text in output.lines() {
        // Python: `File "path", line N`.
        if let Some((before, after)) = text.split_once("\", line ")
            && let Some((_, path)) = before.rsplit_once("File \"")
        {
            note(path, after);
        }
        for word in text.split_whitespace() {
            // `path:N` and `path:N:M`.
            let mut parts = word.split(':');
            if let (Some(path), Some(line)) = (parts.next(), parts.next())
                && path.contains('.')
            {
                note(path, line);
            }
            // `path(N,M)`, as tsc and MSBuild write it.
            if let Some((path, rest)) = word.split_once('(')
                && path.contains('.')
            {
                note(path, rest);
            }
        }
    }
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    found
        .into_iter()
        .filter_map(|(path, line)| {
            let file = std::path::Path::new(&path);
            let file = if file.is_absolute() {
                file.to_path_buf()
            } else {
                root.join(file)
            };
            let inside = file
                .canonicalize()
                .is_ok_and(|real| real.starts_with(&canonical));
            if !inside {
                return None;
            }
            let text = std::fs::read_to_string(&file).ok()?;
            let written = text.lines().nth(line - 1)?;
            let shown = file
                .strip_prefix(root)
                .or_else(|_| file.strip_prefix(&canonical))
                .unwrap_or(&file)
                .display()
                .to_string();
            let written: String = written.chars().take(300).collect();
            Some(format!("{shown}:{line}: {written}"))
        })
        .take(QUOTED_LINES)
        .collect()
}

/// Said with a failure that outlived several changes to one file.
pub fn still_failing_notice(file: &str, changes: usize, lines: &[String]) -> String {
    let quoted = if lines.is_empty() {
        "Read the lines this failure names with read_file before changing anything else.".to_owned()
    } else {
        format!(
            "These are the lines this failure names, exactly as the file has them now:\n{}\n\
             Compare them character by character with what you meant to write.",
            lines.join("\n")
        )
    };
    format!(
        "`{file}` has been changed {changes} times in a row and a run has failed after each \
         change: the changes are not reaching what fails. {quoted} Do not rewrite the file \
         again. Change only the failing line with replace_text; if a line keeps coming out \
         different from what you intend, write it another way -- a different construct, \
         shorter lines, other names."
    )
}

/// A result without what differs between two runs of the same failure: the
/// hash of its output and how long it took.
///
/// Measured 2026-09-29 (Qwen3-14B in the desktop, a bank page): the same
/// `npx start --port 8080` failed four times with the same npm error and was
/// never named, because npm writes a log named by the time, the output hash
/// changed with it, and every failure looked new.
fn comparable(outcome: &serde_json::Value) -> serde_json::Value {
    match outcome {
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .iter()
                .filter(|(name, _)| !name.ends_with("_hash") && !name.ends_with("_ms"))
                .map(|(name, value)| (name.clone(), comparable(value)))
                .collect(),
        ),
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(comparable).collect())
        }
        other => other.clone(),
    }
}

fn outcome_digest(outcome: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut text = comparable(outcome).to_string();
    // Any run of digits is the same run of digits.
    let mut masked = String::with_capacity(text.len());
    let mut in_number = false;
    for c in text.drain(..) {
        if c.is_ascii_digit() {
            if !in_number {
                masked.push('#');
            }
            in_number = true;
        } else {
            in_number = false;
            masked.push(c);
        }
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    masked.hash(&mut hasher);
    hasher.finish()
}

/// What the deployment is told alongside the result that echoed.
pub fn echo_notice(seen: usize) -> String {
    format!(
        "You have now run this exact action {seen} times in your last {ECHO_WINDOW}, and it \
         gave the same result every time. Another identical try will not change it: change \
         something first -- the file, the command, the address -- or read what this result \
         says and act on that."
    )
}

#[cfg(test)]
mod echo_tests {
    use super::*;

    #[test]
    fn the_same_result_three_times_is_named() {
        let mut echoes = Echoes::default();
        let failed = serde_json::json!({"status": 404, "note": "nothing saved", "duration_ms": 12});
        assert_eq!(echoes.observe("fetch_url:x", &failed), None);
        echoes.observe("delete_path:y", &serde_json::json!({"deleted": true}));
        assert_eq!(
            echoes.observe(
                "fetch_url:x",
                &serde_json::json!({"status": 404, "note": "nothing saved", "duration_ms": 97})
            ),
            None
        );
        assert_eq!(echoes.observe("fetch_url:x", &failed), Some(3));
    }

    #[test]
    fn a_different_result_or_action_is_not_an_echo() {
        let mut echoes = Echoes::default();
        for n in 0..5 {
            assert_eq!(echoes.observe("run:tests", &serde_json::json!({"stdout": format!("{} failed", ["a", "b", "c", "d", "e"][n])})), None);
        }
        for n in 0..5 {
            assert_eq!(
                echoes.observe(&format!("run:{n}"), &serde_json::json!({"exit_code": 0})),
                None
            );
        }
    }

    #[test]
    fn the_same_failure_is_the_same_whatever_its_log_is_called() {
        let mut echoes = Echoes::default();
        let run = |log: &str, hash: &str| {
            serde_json::json!({"exit_code": 1, "artifact_hash": hash, "duration_ms": 400,
                "stderr": format!("npm error could not determine executable to run\nnpm error A complete log of this run can be found in: /Users/x/.npm/_logs/{log}-debug-0.log")})
        };
        echoes.observe("npx start", &run("2026-09-29T17_12_29_583Z", "ce55e0"));
        assert_eq!(echoes.repeated_failures("npx start"), 1);
        echoes.observe("read x", &serde_json::json!({"content": "x"}));
        echoes.observe("npx start", &run("2026-09-29T17_14_12_138Z", "3d8f6c"));
        assert_eq!(echoes.repeated_failures("npx start"), 2);
        // A success in between, or another failure, starts it over.
        echoes.observe("npx start", &serde_json::json!({"exit_code": 0}));
        assert_eq!(echoes.repeated_failures("npx start"), 0);
    }

    #[test]
    fn a_turn_that_wrote_its_files_hands_over_after_five_failed_runs_once() {
        let mut runs = FailedRuns::default();
        for _ in 0..4 {
            assert_eq!(runs.observe(true, true), None);
        }
        runs.observe(false, true);
        for _ in 0..4 {
            assert_eq!(runs.observe(true, true), None);
        }
        assert!(runs.observe(true, true).is_some());
        assert!(runs.handed_over());
        assert_eq!(runs.observe(true, true), None);
        // Nothing written, nothing to hand over: the failures are the answer.
        let mut reading = FailedRuns::default();
        for _ in 0..8 {
            assert_eq!(reading.observe(true, false), None);
        }
    }

    #[test]
    fn old_results_fall_out_of_the_window() {
        let mut echoes = Echoes::default();
        let same = serde_json::json!({"exit_code": 1});
        echoes.observe("a", &same);
        echoes.observe("a", &same);
        for n in 0..ECHO_WINDOW {
            echoes.observe(&format!("other{n}"), &same);
        }
        assert_eq!(echoes.observe("a", &same), None);
    }
    #[test]
    fn a_file_changed_again_and_again_with_a_failure_after_each_is_named() {
        let mut still = StillFailing::default();
        // A failure with no change before it is not counted.
        assert_eq!(still.observe("test", true), None);
        for _ in 0..STILL_FAILING_LIMIT - 1 {
            still.changed("a.py");
            assert_eq!(still.observe("test", true), None);
        }
        still.changed("a.py");
        assert_eq!(
            still.observe("test", true),
            Some(("a.py".to_owned(), STILL_FAILING_LIMIT))
        );
        // Not said again at every run, and a run that passes starts over.
        still.changed("a.py");
        assert_eq!(still.observe("test", true), None);
        // A different command passing says nothing about this failure.
        assert_eq!(still.observe("ls", false), None);
        still.changed("a.py");
        still.changed("a.py");
        assert_eq!(still.observe("test", true), None);
        still.changed("a.py");
        assert_eq!(
            still.observe("test", true),
            Some(("a.py".to_owned(), 2 * STILL_FAILING_LIMIT))
        );
        assert_eq!(still.observe("test", false), None);
        still.changed("a.py");
        assert_eq!(still.observe("test", true), None);
        // Another file is another streak.
        let mut other = StillFailing::default();
        for file in ["a.py", "a.py", "b.py"] {
            other.changed(file);
            assert_eq!(other.observe("test", true), None);
        }
    }

    #[test]
    fn the_lines_a_failure_names_are_quoted_as_the_file_has_them() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("src/a.py"),
            "import os\nx = y ifz else None\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("b.ts"),
            "const a = 1;\nconst b: string = a;\n",
        )
        .unwrap();
        let output = format!(
            "  File \"{}\", line 2\n    x = y ifz else None\nSyntaxError: invalid syntax\n\
             b.ts(2,7): error TS2322\nerror at ./src/a.py:1:3 and /etc/hosts:1 and gone.py:4\n",
            root.path().join("src/a.py").display()
        );
        assert_eq!(
            named_lines(root.path(), &output),
            vec![
                "src/a.py:2: x = y ifz else None",
                "b.ts:2: const b: string = a;",
                "src/a.py:1: import os",
            ]
        );
        let notice = still_failing_notice("src/a.py", 3, &named_lines(root.path(), &output));
        assert!(
            notice.contains("src/a.py:2: x = y ifz else None"),
            "{notice}"
        );
        assert!(notice.contains("Do not rewrite the file again"), "{notice}");
        assert!(still_failing_notice("a", 3, &[]).contains("read_file"));
    }
}
