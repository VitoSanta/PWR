//! A deployment that is acting and getting nowhere.
//!
//! Distinct from [`crate::repetition`], which is about one proposal sent again
//! after being refused. This is about a window of actions that all ran, all
//! succeeded, and left the workspace and the checks exactly where they were --
//! an edit and its revert, the same files read in a circle, a search narrowed
//! and then widened back.
//!
//! Extracted from the scripted loop for the same reason repetition was: the
//! conversation did not have it. A turn could spend its whole budget reading
//! the same three files and nothing would say so, to the deployment or to the
//! audit.
//!
//! Two conditions are required before a window counts, and the second is the
//! one that matters: the effect signature must end where it began, **and**
//! nothing in the window may be novel. Reading six files the session has never
//! read is investigation, and calling that non-progress would interrupt exactly
//! the behaviour a hard task needs.

pub use crate::EffectSignature;
pub use crate::run_state::ProgressTracker;

use std::collections::BTreeMap;

/// Actions judged together.
pub const NO_PROGRESS_WINDOW: usize = 6;

/// Consecutive stalled windows before the work is stopped. The reasoning is
/// on the scripted loop's use of it; a conversation is held to the same bound.
pub const NO_PROGRESS_LIMIT: usize = 3;

/// What a conversation knows about its own progress, kept across turns.
///
/// Per turn, it was forgotten at every check-in. Measured on 2026-09-21: a
/// goal-mode session building an Angular site ran 169 actions, the last
/// hundred alternating the same two no-op commands, and `no_progress.detected`
/// fired five times without ever stopping it -- each goal checkpoint started a
/// fresh tracker, so every window of the new turn was "novel" again and the
/// count never reached the limit.
#[derive(Debug, Default)]
pub struct Stall {
    pub progress: ProgressTracker,
    /// Files the conversation changed, by the hash it left them with.
    pub changed_files: BTreeMap<String, String>,
}

/// Records what an action did to the workspace, for the signature.
///
/// Read from the outcome rather than from the proposal, so an edit that was
/// refused or failed contributes nothing. A delete or a move moves the
/// workspace without giving any file a new hash, which is why the second arm
/// exists: without it, a deployment deleting files in a circle would look
/// like one making progress.
pub fn record_effect(changed: &mut BTreeMap<String, String>, outcome: &serde_json::Value) {
    let Some(path) = outcome.get("path").and_then(|path| path.as_str()) else {
        return;
    };
    if let Some(hash) = outcome.get("new_hash").and_then(|hash| hash.as_str()) {
        changed.insert(path.to_string(), hash.to_string());
    } else if let Some(entries) = outcome.get("entries").and_then(|n| n.as_u64()) {
        changed.insert(
            path.to_string(),
            format!(
                "{}:{entries}",
                outcome
                    .get("from")
                    .and_then(|from| from.as_str())
                    .unwrap_or("changed")
            ),
        );
    }
}

/// What the deployment is told when a window made no progress.
///
/// States the fact and does not decide what to do about it. Deciding would be
/// the harness taking over the task, and the three things worth doing next are
/// named rather than left to be inferred.
pub fn no_progress_notice() -> String {
    serde_json::json!({
        "no_progress": format!(
            "The last {NO_PROGRESS_WINDOW} actions left the workspace and the checks exactly as they were, and none of them was something you had not already tried. Change approach: edit a file, run a check, or call complete if the work is done."
        )
    })
    .to_string()
}
