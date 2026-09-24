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
