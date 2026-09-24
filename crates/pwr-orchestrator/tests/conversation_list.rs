//! The saved conversations a front end lists and resumes, read from a real log.

use pwr_domain::ChatMessage;
use pwr_orchestrator::conversation;
use pwr_store::Store;

fn snapshot(store: &Store, id: pwr_domain::Id, request: &str) {
    let messages = [
        ChatMessage::text("system", "prompt"),
        ChatMessage::text("user", request),
        ChatMessage::text("assistant", "done"),
    ];
    conversation::record_snapshot(store, id, &messages).unwrap();
}

#[test]
fn conversations_are_listed_most_recently_active_first_with_their_first_request() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.sqlite")).unwrap();
    let older = pwr_domain::new_id();
    let newer = pwr_domain::new_id();
    let never_answered = pwr_domain::new_id();
    snapshot(&store, older, "fix the parser\nand its tests");
    snapshot(&store, newer, &"a".repeat(200));
    store
        .append(
            Some(never_answered),
            conversation::STEERED_EVENT,
            serde_json::json!({}),
        )
        .unwrap();
    // Activity after its snapshot makes the older conversation the latest.
    snapshot(&store, older, "fix the parser\nand its tests");

    let listed = conversation::list(&store).unwrap();
    let ids: Vec<_> = listed.iter().map(|listed| listed.id).collect();
    assert_eq!(
        ids,
        [older, newer],
        "a conversation with no turn is not listed"
    );
    assert_eq!(listed[0].title.as_deref(), Some("fix the parser"));
    assert_eq!(listed[0].messages, 3);
    let long = listed[1].title.as_deref().unwrap();
    assert_eq!(long.chars().count(), 80);
    assert!(long.ends_with('…'));
    assert!(!listed[0].updated_at.is_empty());
}

#[test]
fn resuming_records_what_the_conversation_was_told() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("state.sqlite")).unwrap();
    let id = pwr_domain::new_id();
    snapshot(&store, id, "edit code.rs");
    std::fs::write(dir.path().join("code.rs"), "changed by hand\n").unwrap();
    let checkpoint = conversation::Checkpoint {
        turn: 1,
        changed_files: [(
            "code.rs".to_string(),
            pwr_domain::hash_bytes("left by the conversation\n"),
        )]
        .into(),
        ..Default::default()
    };
    conversation::record_checkpoint(&store, id, &checkpoint).unwrap();

    let resumed = conversation::resume(dir.path(), &store, id)
        .unwrap()
        .unwrap();
    let note = resumed.note.clone().expect("a hand edit is reported");
    assert!(note.contains("code.rs"), "{note}");
    assert_eq!(resumed.messages.len(), 4);
    assert_eq!(resumed.messages[3].role, "tool");
    assert_eq!(resumed.checkpoint, checkpoint);
    let recorded = store
        .latest_payload(id, conversation::RESUMED_EVENT)
        .unwrap()
        .expect("the resume is in the log");
    assert_eq!(recorded["changed_since"], serde_json::json!(["code.rs"]));

    assert!(
        conversation::resume(dir.path(), &store, pwr_domain::new_id())
            .unwrap()
            .is_none()
    );
}
