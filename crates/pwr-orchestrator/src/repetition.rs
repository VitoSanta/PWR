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
    recent: std::collections::VecDeque<(String, u64)>,
}

impl Echoes {
    /// Records a result; returns how many times it has now been seen, when
    /// that is the limit or more.
    pub fn observe(&mut self, fingerprint: &str, outcome: &serde_json::Value) -> Option<usize> {
        let digest = outcome_digest(outcome);
        self.recent.push_back((fingerprint.to_owned(), digest));
        if self.recent.len() > ECHO_WINDOW {
            self.recent.pop_front();
        }
        let seen = self
            .recent
            .iter()
            .filter(|(seen, result)| seen == fingerprint && *result == digest)
            .count();
        (seen >= ECHO_LIMIT).then_some(seen)
    }
}

fn outcome_digest(outcome: &serde_json::Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut text = outcome.to_string();
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
}
