//! A conversation that outlives the console holding it.
//!
//! The console kept the conversation in memory. Quitting, a crash, or a closed
//! terminal lost every message while the edits it had made stayed on disk --
//! the next conversation in that workspace started blind to work it was
//! standing on. Claude Code's shape, which this project adopted, has
//! `--continue` for exactly that.
//!
//! What is kept, and why each part:
//!
//! - A snapshot of the messages when a turn ends. It is the conversation as
//!   the deployment last saw it whole, compaction included, so a resumed
//!   conversation is the same prompt and not a reconstruction of one.
//! - A checkpoint at every action boundary: which files the conversation had
//!   changed and to what content. Messages produced mid-turn are not
//!   snapshotted -- a turn's partial history is not a prompt anyone saw -- but
//!   its effects are, because effects are what a resume has to reconcile.
//! - An intent before every action that changes the workspace. A write that
//!   was announced and never receipted is the one case a restart cannot
//!   repeat blindly: it may or may not have happened, and the file says which.
//! - Every steering message, with the objective revision it opened.
//!
//! Resuming reconciles those records against the workspace as it is and tells
//! both the deployment and the person what differs, rather than assuming
//! nothing happened while the conversation was away.

use pwr_domain::ChatMessage;
use pwr_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const SNAPSHOT_EVENT: &str = "conversation.snapshot";
pub const CHECKPOINT_EVENT: &str = "conversation.checkpoint";
pub const INTENT_EVENT: &str = "action.intent";
pub const STEERED_EVENT: &str = "conversation.steered";
pub const RESUMED_EVENT: &str = "conversation.resumed";
pub const DELETED_EVENT: &str = "conversation.deleted";

/// Where a conversation stood at an action boundary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Turns started in this conversation, the current one included.
    pub turn: u32,
    /// Actions taken in the current turn.
    pub actions: usize,
    /// Every file the conversation has changed, with the content hash it left.
    pub changed_files: BTreeMap<String, String>,
    /// How many times the objective has been revised by steering.
    pub revision: u32,
    /// The sequence the next announced action will carry.
    #[serde(default)]
    pub next_intent: u64,
}

/// An action announced before it ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Intent {
    /// Sequence within the conversation, matched by the receipt.
    pub sequence: u64,
    pub capability: String,
    pub path: Option<String>,
}

/// Whether an action can change the workspace, and so is announced before it
/// runs and receipted after.
///
/// One definition for both loops. The conversation announced its effects and
/// the scripted run did not, so a run interrupted mid-edit left nothing to tell
/// a finished write from an uncertain one -- the prerequisite R5 names for
/// giving both loops one continuation semantics.
pub fn may_change_workspace(action: &pwr_tools::ActionProposal) -> bool {
    use pwr_tools::ActionProposal;
    matches!(
        action,
        ActionProposal::ReplaceText { .. }
            | ActionProposal::ApplyReplace { .. }
            | ActionProposal::ApplyPatchHunks { .. }
            | ActionProposal::WriteFile { .. }
            | ActionProposal::MakeDirectory { .. }
            | ActionProposal::DeletePath { .. }
            | ActionProposal::MovePath { .. }
            | ActionProposal::RestoreFile { .. }
            | ActionProposal::RunCommand { .. }
            | ActionProposal::ExtractDocument { .. }
    )
}

/// The announcement for an action, carrying its capability and, where it has
/// one, its path.
pub fn intent_for(action: &pwr_tools::ActionProposal, sequence: u64) -> Intent {
    let value = serde_json::to_value(action).unwrap_or_default();
    Intent {
        sequence,
        capability: value["capability"].as_str().unwrap_or_default().to_owned(),
        path: value["path"].as_str().map(str::to_owned),
    }
}

pub fn record_snapshot(
    store: &Store,
    conversation_id: pwr_domain::Id,
    messages: &[ChatMessage],
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            SNAPSHOT_EVENT,
            serde_json::json!({"messages": messages}),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn record_checkpoint(
    store: &Store,
    conversation_id: pwr_domain::Id,
    checkpoint: &Checkpoint,
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            CHECKPOINT_EVENT,
            serde_json::to_value(checkpoint).map_err(|e| e.to_string())?,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn record_intent(
    store: &Store,
    conversation_id: pwr_domain::Id,
    intent: &Intent,
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            INTENT_EVENT,
            serde_json::to_value(intent).map_err(|e| e.to_string())?,
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Records a receipt for an intent: the action ran, whatever its outcome.
pub fn record_receipt(
    store: &Store,
    conversation_id: pwr_domain::Id,
    sequence: u64,
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            "action.receipt",
            serde_json::json!({"sequence": sequence}),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub fn record_steering(
    store: &Store,
    conversation_id: pwr_domain::Id,
    revision: u32,
    text: &str,
    during_action: usize,
) -> Result<(), String> {
    store
        .append(
            Some(conversation_id),
            STEERED_EVENT,
            serde_json::json!({
                "revision": revision,
                "text": text,
                "after_actions": during_action,
            }),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// A conversation as the log can give it back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Restored {
    /// The messages of the last complete turn.
    pub messages: Vec<ChatMessage>,
    /// The last action boundary recorded, if any turn got that far.
    pub checkpoint: Option<Checkpoint>,
    /// Workspace-changing actions announced and never receipted.
    pub unreceipted: Vec<Intent>,
    /// What the conversation did after its last snapshot, in the order the
    /// audit recorded it -- the part of an interrupted turn the restored
    /// messages do not show.
    pub after_snapshot: Vec<String>,
    /// The highest objective revision steering reached.
    pub revision: u32,
}

/// Rebuilds a conversation from its events. `None` if it never completed a
/// turn, since there is then no conversation to continue.
pub fn restore(
    store: &Store,
    conversation_id: pwr_domain::Id,
) -> Result<Option<Restored>, String> {
    let events = store
        .events_for_run(conversation_id)
        .map_err(|e| e.to_string())?;
    if events.iter().any(|event| event.event_type == DELETED_EVENT) {
        return Ok(None);
    }
    let Some(last_snapshot) = events
        .iter()
        .rposition(|event| event.event_type == SNAPSHOT_EVENT)
    else {
        return Ok(None);
    };
    let messages: Vec<ChatMessage> =
        serde_json::from_value(events[last_snapshot].payload["messages"].clone())
            .map_err(|e| format!("unreadable conversation snapshot: {e}"))?;
    let checkpoint = events
        .iter()
        .rev()
        .find(|event| event.event_type == CHECKPOINT_EVENT)
        .and_then(|event| serde_json::from_value::<Checkpoint>(event.payload.clone()).ok());
    let mut intents: BTreeMap<u64, Intent> = BTreeMap::new();
    let mut revision = 0;
    for event in &events {
        match event.event_type.as_str() {
            INTENT_EVENT => {
                if let Ok(intent) = serde_json::from_value::<Intent>(event.payload.clone()) {
                    intents.insert(intent.sequence, intent);
                }
            }
            "action.receipt" => {
                if let Some(sequence) = event.payload["sequence"].as_u64() {
                    intents.remove(&sequence);
                }
            }
            STEERED_EVENT => {
                revision = revision.max(event.payload["revision"].as_u64().unwrap_or(0) as u32);
            }
            _ => {}
        }
    }
    let after_snapshot = events[last_snapshot + 1..]
        .iter()
        .filter(|event| event.event_type == "tool.action")
        .map(|event| {
            let action = &event.payload["action"];
            let capability = action["capability"].as_str().unwrap_or("action");
            let target = action["path"]
                .as_str()
                .or_else(|| action["executable"].as_str())
                .unwrap_or_default();
            let status = event.payload["status"].as_str().unwrap_or_default();
            format!("{capability} {target} ({status})")
                .trim()
                .to_string()
        })
        .collect();
    Ok(Some(Restored {
        messages,
        checkpoint,
        unreceipted: intents.into_values().collect(),
        after_snapshot,
        revision,
    }))
}

/// What differs between what a conversation recorded and the workspace now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Files the conversation changed whose content is no longer what it left:
    /// someone edited them since, or they are gone.
    pub changed_since: Vec<String>,
    pub deleted_since: Vec<String>,
    /// Actions that may or may not have taken effect.
    pub uncertain: Vec<Intent>,
    /// Work done after the last snapshot, which the messages do not show.
    pub unseen_work: Vec<String>,
}

impl Reconciliation {
    pub fn is_clean(&self) -> bool {
        self.changed_since.is_empty()
            && self.deleted_since.is_empty()
            && self.uncertain.is_empty()
            && self.unseen_work.is_empty()
    }

    /// Said to the deployment, and shown to the person, when a conversation
    /// resumes. `None` when there is nothing to say.
    pub fn note(&self) -> Option<String> {
        if self.is_clean() {
            return None;
        }
        let mut lines = vec![
            "This conversation was resumed after it stopped. Check the workspace against what \
             you remember before relying on it."
                .to_string(),
        ];
        if !self.unseen_work.is_empty() {
            lines.push(format!(
                "Actions taken after the last complete turn, which the history above does not \
                 show: {}.",
                self.unseen_work.join("; ")
            ));
        }
        if !self.changed_since.is_empty() {
            lines.push(format!(
                "Changed outside this conversation since it last wrote them -- re-read before \
                 editing: {}.",
                self.changed_since.join(", ")
            ));
        }
        if !self.deleted_since.is_empty() {
            lines.push(format!(
                "Deleted since this conversation wrote them: {}.",
                self.deleted_since.join(", ")
            ));
        }
        if !self.uncertain.is_empty() {
            lines.push(format!(
                "Started and never confirmed, so they may or may not have taken effect -- look \
                 before repeating them: {}.",
                self.uncertain
                    .iter()
                    .map(|intent| match &intent.path {
                        Some(path) => format!("{} {path}", intent.capability),
                        None => intent.capability.clone(),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        Some(lines.join("\n"))
    }
}

/// Compares a restored conversation's records with the workspace at `root`.
pub fn reconcile(root: &Path, restored: &Restored) -> Reconciliation {
    let mut reconciliation = Reconciliation {
        uncertain: restored.unreceipted.clone(),
        unseen_work: restored.after_snapshot.clone(),
        ..Default::default()
    };
    if let Some(checkpoint) = &restored.checkpoint {
        for (path, left) in &checkpoint.changed_files {
            // A move or a directory is recorded by what it did, not by a
            // content hash, and there is no content to compare it with.
            let is_content_hash = left.len() == 64 && left.chars().all(|c| c.is_ascii_hexdigit());
            if !is_content_hash {
                continue;
            }
            match std::fs::read(root.join(path)) {
                Ok(bytes) => {
                    if &pwr_domain::hash_bytes(&bytes) != left {
                        reconciliation.changed_since.push(path.clone());
                    }
                }
                Err(_) => reconciliation.deleted_since.push(path.clone()),
            }
        }
    }
    reconciliation
}

/// A conversation as a list of them shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: pwr_domain::Id,
    /// When anything was last recorded for it.
    pub updated_at: String,
    /// The first request made in it, shortened to a line.
    pub title: Option<String>,
    /// Messages in its last snapshot.
    pub messages: usize,
}

/// The conversations in this store that completed a turn, most recently active
/// first.
pub fn list(store: &Store) -> Result<Vec<Listed>, String> {
    let runs = store
        .runs_with_event(SNAPSHOT_EVENT)
        .map_err(|e| e.to_string())?;
    let mut listed = Vec::new();
    for (id, updated_at) in runs {
        if store
            .latest_payload(id, DELETED_EVENT)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            continue;
        }
        let messages: Vec<ChatMessage> = store
            .latest_payload(id, SNAPSHOT_EVENT)
            .map_err(|e| e.to_string())?
            .and_then(|snapshot| serde_json::from_value(snapshot["messages"].clone()).ok())
            .unwrap_or_default();
        let title = messages
            .iter()
            .find(|message| {
                message.role == "user"
                    && !message.content.trim().is_empty()
                    && !written_by_the_harness(&message.content)
            })
            .map(|message| title_of(&message.content));
        listed.push(Listed {
            id,
            updated_at,
            title,
            messages: messages.len(),
        });
    }
    Ok(listed)
}

/// Context the harness adds under the user role -- the ledger, the workspace
/// topology, framework guidance, ranked passages. A conversation named after
/// one of them was listed as "Ledger of 1 earlier run(s) of this …", every
/// conversation alike (seen in the desktop app, 2026-09-22).
const HARNESS_PREFIXES: [&str; 5] = [
    "Ledger of ",
    "Workspace topology",
    "Framework guidance loaded",
    "Repository passages ranked",
    "Continue the same goal",
];

fn written_by_the_harness(content: &str) -> bool {
    let start = content.trim_start();
    HARNESS_PREFIXES
        .iter()
        .any(|prefix| start.starts_with(prefix))
}

fn title_of(request: &str) -> String {
    const LONGEST: usize = 80;
    let line = request.trim().lines().next().unwrap_or_default().trim();
    if line.chars().count() <= LONGEST {
        return line.to_string();
    }
    let mut title: String = line.chars().take(LONGEST - 1).collect();
    title.push('…');
    title
}

#[cfg(test)]
mod title_tests {
    #[test]
    fn a_title_is_the_person_s_request_not_the_harness_context() {
        for injected in [
            "Ledger of 1 earlier run(s) of this session, taken from the recorded audit.",
            "Workspace topology (deterministic preflight, not lexical retrieval):",
            "Repository passages ranked against this task.",
        ] {
            assert!(super::written_by_the_harness(injected), "{injected}");
        }
        assert!(!super::written_by_the_harness(
            "Crea il sito web ufficiale di PWR"
        ));
    }
}

/// A conversation restored and reconciled, ready to continue.
#[derive(Debug, Clone, PartialEq)]
pub struct Resumed {
    /// The restored messages, with the reconciliation note appended as a tool
    /// message when there is one, so the deployment reads it before anything.
    pub messages: Vec<ChatMessage>,
    pub checkpoint: Checkpoint,
    /// What differs from what the conversation recorded. `None` when nothing
    /// does.
    pub note: Option<String>,
}

/// Restores `conversation_id` and reconciles it against the workspace at
/// `root`, recording what it was told. `None` if it never completed a turn.
///
/// The reconciliation is recorded as its own event, so the log says what the
/// conversation was told on resuming and not only that it resumed. Every front
/// end resumes through here: the console's `/resume` and `--continue`, and a
/// protocol client's `session/load` and `session/resume`.
pub fn resume(
    root: &Path,
    store: &Store,
    conversation_id: pwr_domain::Id,
) -> Result<Option<Resumed>, String> {
    let Some(restored) = restore(store, conversation_id)? else {
        return Ok(None);
    };
    let reconciliation = reconcile(root, &restored);
    let note = reconciliation.note();
    store
        .append(
            Some(conversation_id),
            RESUMED_EVENT,
            serde_json::json!({
                "messages": restored.messages.len(),
                "changed_since": reconciliation.changed_since,
                "deleted_since": reconciliation.deleted_since,
                "uncertain": reconciliation.uncertain,
                "unseen_work": reconciliation.unseen_work,
            }),
        )
        .map_err(|error| error.to_string())?;
    let mut messages = restored.messages;
    if let Some(note) = &note {
        messages.push(ChatMessage::text("tool", note.clone()));
    }
    Ok(Some(Resumed {
        messages,
        checkpoint: restored.checkpoint.unwrap_or_default(),
        note,
    }))
}

/// The most recent conversation in this store that completed a turn.
pub fn latest(store: &Store) -> Result<Option<pwr_domain::Id>, String> {
    // The last snapshot, as before deletion existed; a checkpoint written later
    // on another conversation does not make that one "latest".
    let last = store
        .latest_run_with_event(SNAPSHOT_EVENT)
        .map_err(|e| e.to_string())?;
    match last {
        Some(id)
            if store
                .latest_payload(id, DELETED_EVENT)
                .map_err(|e| e.to_string())?
                .is_none() =>
        {
            Ok(Some(id))
        }
        // It was deleted: the most recently active one still listed.
        _ => Ok(list(store)?.first().map(|session| session.id)),
    }
}

/// Removes a saved conversation from history without breaking the event log.
pub fn delete(store: &Store, conversation_id: pwr_domain::Id) -> Result<bool, String> {
    if store
        .latest_payload(conversation_id, SNAPSHOT_EVENT)
        .map_err(|e| e.to_string())?
        .is_none()
        || store
            .latest_payload(conversation_id, DELETED_EVENT)
            .map_err(|e| e.to_string())?
            .is_some()
    {
        return Ok(false);
    }
    store
        .append(Some(conversation_id), DELETED_EVENT, serde_json::json!({}))
        .map_err(|e| e.to_string())?;
    Ok(true)
}
