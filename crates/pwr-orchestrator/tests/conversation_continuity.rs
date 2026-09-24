//! A conversation that outlives the console holding it.
//!
//! Quitting the console lost every message while the edits it had made stayed
//! on disk. These fixtures hold what `pwr chat --continue` depends on: the
//! last complete turn comes back as it was, and what happened since -- work an
//! interrupted turn did after that point, files edited by someone else, a write
//! that was announced and never confirmed -- is found and said.

use pwr_domain::ChatMessage;
use pwr_orchestrator::conversation::{
    Checkpoint, Intent, reconcile, record_checkpoint, record_intent, record_receipt,
    record_snapshot, record_steering, restore,
};
use pwr_store::Store;
use std::collections::BTreeMap;

fn hash(text: &str) -> String {
    pwr_domain::hash_bytes(text.as_bytes())
}

#[test]
fn a_conversation_that_never_finished_a_turn_has_nothing_to_continue() {
    let store = Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    record_checkpoint(&store, id, &Checkpoint::default()).unwrap();
    assert_eq!(restore(&store, id).unwrap(), None);
}

/// The last complete turn is what comes back, and the steering revision with it.
#[test]
fn the_last_complete_turn_comes_back_as_it_was() {
    let store = Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let first = vec![ChatMessage::text("user", "one")];
    let second = vec![
        ChatMessage::text("user", "one"),
        ChatMessage::text("assistant", "done one"),
        ChatMessage::text("user", "two"),
    ];
    record_snapshot(&store, id, &first).unwrap();
    record_steering(&store, id, 1, "not that file", 3).unwrap();
    record_snapshot(&store, id, &second).unwrap();
    let restored = restore(&store, id).unwrap().expect("a conversation");
    assert_eq!(restored.messages, second);
    assert_eq!(restored.revision, 1);
    assert!(restored.unreceipted.is_empty());
}

/// An interrupted turn's work after the last snapshot is not in the messages,
/// and a resumed conversation is told about it rather than left to rediscover
/// it -- or to redo it.
#[test]
fn work_after_the_last_snapshot_is_named() {
    let store = Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    record_snapshot(&store, id, &[ChatMessage::text("user", "fix it")]).unwrap();
    store
        .append_event(
            Some(id),
            &pwr_domain::RunEvent::ToolAction {
                action: serde_json::json!({"capability": "write_file", "path": "src/a.rs"}),
                status: pwr_domain::ToolActionStatus::Allowed,
                outcome_class: "allowed_success".into(),
                outcome: None,
                denial: None,
                failure: None,
                failure_category: None,
            },
        )
        .unwrap();
    let restored = restore(&store, id).unwrap().unwrap();
    assert_eq!(
        restored.after_snapshot,
        vec!["write_file src/a.rs (allowed)"]
    );
    let dir = tempfile::tempdir().unwrap();
    let note = reconcile(dir.path(), &restored)
        .note()
        .expect("something to say");
    assert!(note.contains("write_file src/a.rs"), "{note}");
}

/// A file edited by someone else after the conversation wrote it, a file
/// deleted, and a write that was announced and never confirmed are three
/// different facts, and each is said.
#[test]
fn what_changed_while_the_conversation_was_away_is_found() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("kept.rs"), "as left").unwrap();
    std::fs::write(dir.path().join("edited.rs"), "someone else's edit").unwrap();
    let store = Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    record_snapshot(&store, id, &[ChatMessage::text("user", "go")]).unwrap();
    let mut changed = BTreeMap::new();
    changed.insert("kept.rs".to_string(), hash("as left"));
    changed.insert("edited.rs".to_string(), hash("what the conversation wrote"));
    changed.insert("gone.rs".to_string(), hash("anything"));
    // A move is recorded by what it did, not by content, and is not compared.
    changed.insert("moved".to_string(), "renamed:1".to_string());
    record_checkpoint(
        &store,
        id,
        &Checkpoint {
            turn: 2,
            actions: 4,
            changed_files: changed,
            revision: 0,
            next_intent: 3,
        },
    )
    .unwrap();
    let confirmed = Intent {
        sequence: 1,
        capability: "write_file".into(),
        path: Some("kept.rs".into()),
    };
    let unconfirmed = Intent {
        sequence: 2,
        capability: "apply_replace".into(),
        path: Some("edited.rs".into()),
    };
    record_intent(&store, id, &confirmed).unwrap();
    record_receipt(&store, id, 1).unwrap();
    record_intent(&store, id, &unconfirmed).unwrap();

    let restored = restore(&store, id).unwrap().unwrap();
    let found = reconcile(dir.path(), &restored);
    assert_eq!(found.changed_since, vec!["edited.rs"]);
    assert_eq!(found.deleted_since, vec!["gone.rs"]);
    assert_eq!(found.uncertain, vec![unconfirmed]);
    let note = found.note().unwrap();
    assert!(note.contains("re-read before editing: edited.rs"), "{note}");
    assert!(note.contains("gone.rs"), "{note}");
    assert!(note.contains("apply_replace edited.rs"), "{note}");
    assert!(
        !note.contains("kept.rs"),
        "an unchanged file was reported: {note}"
    );
}

#[test]
fn a_workspace_that_matches_its_record_has_nothing_to_say() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "as left").unwrap();
    let store = Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    record_snapshot(&store, id, &[ChatMessage::text("user", "go")]).unwrap();
    let mut changed = BTreeMap::new();
    changed.insert("a.rs".to_string(), hash("as left"));
    record_checkpoint(
        &store,
        id,
        &Checkpoint {
            changed_files: changed,
            ..Default::default()
        },
    )
    .unwrap();
    let restored = restore(&store, id).unwrap().unwrap();
    assert_eq!(reconcile(dir.path(), &restored).note(), None);
}

/// `--continue` picks the conversation that most recently completed a turn.
#[test]
fn the_latest_conversation_is_the_one_that_last_finished_a_turn() {
    let store = Store::open(":memory:").unwrap();
    let older = pwr_domain::new_id();
    let newer = pwr_domain::new_id();
    record_snapshot(&store, older, &[ChatMessage::text("user", "a")]).unwrap();
    record_snapshot(&store, newer, &[ChatMessage::text("user", "b")]).unwrap();
    record_checkpoint(&store, older, &Checkpoint::default()).unwrap();
    assert_eq!(
        pwr_orchestrator::conversation::latest(&store).unwrap(),
        Some(newer)
    );
}
