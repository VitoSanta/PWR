//! What survives a compaction, whichever trigger asked for it.
//!
//! Manual ("Compact now") and automatic compaction are one function with two
//! tail budgets. These tests pin what that function must never lose: the
//! instructions, the task, the files changed, the paths worked on, the errors
//! still open and what the checks said -- and that it only ever touches the
//! message list, never the workspace or its indexes.

use pwr_domain::{ChatMessage, MessagePurpose, ToolCall};
use pwr_orchestrator::compaction::{
    self, COMPACTED_EVENT, Carry, RECORD_HEADER, Trigger, composition, estimated_tokens,
};
use std::collections::BTreeMap;

fn said(role: &str, text: &str) -> ChatMessage {
    ChatMessage::text(role, text)
}

fn call(name: &str, arguments: serde_json::Value) -> ChatMessage {
    ChatMessage {
        role: "assistant".into(),
        tool_calls: vec![ToolCall {
            name: name.into(),
            arguments,
            id: None,
        }],
        ..Default::default()
    }
}

fn result(outcome: serde_json::Value) -> ChatMessage {
    pwr_orchestrator::tool_result_message(outcome, None, None)
}

/// A conversation with everything compaction has to keep somewhere in its
/// older half, and a recent tail that should survive verbatim.
fn working_session() -> Vec<ChatMessage> {
    let bulk = "fn body() {}\n".repeat(400);
    vec![
        said(
            "system",
            "You are PWR. Harness rule: paths are relative.",
        ),
        said(
            "user",
            "Add a --verbose flag to the CLI and keep the tests green.",
        ),
        call("read_file", serde_json::json!({"path": "src/cli.rs"})),
        result(serde_json::json!({"content": bulk})),
        call("search", serde_json::json!({"query": "fn parse_args"})),
        result(serde_json::json!({"matches": bulk})),
        call(
            "replace_text",
            serde_json::json!({"path": "src/cli.rs", "old": "a", "new": "b"}),
        ),
        result(serde_json::json!({"written": "src/cli.rs"})),
        call(
            "run_command",
            serde_json::json!({"executable": "cargo", "args": ["test"]}),
        ),
        result(
            serde_json::json!({"exit_code": 101, "stderr": "error[E0425]: cannot find value `verbose`\n"}),
        ),
        call(
            "run_command",
            serde_json::json!({"executable": "cargo", "args": ["fmt"]}),
        ),
        result(serde_json::json!({"exit_code": 1, "stderr": "fmt failed"})),
        call(
            "run_command",
            serde_json::json!({"executable": "cargo", "args": ["fmt"]}),
        ),
        result(serde_json::json!({"exit_code": 0, "stdout": ""})),
        said(
            "assistant",
            "Decided: the flag lives in Args, not in a global. The test still fails on `verbose`.",
        ),
        said(
            "tool",
            "The workspace checks were run after your edits: 1 check failing (cargo test).",
        ),
        said("user", "Also document the flag in README.md."),
        said("assistant", "Will do after the test passes."),
    ]
}

fn carry() -> Carry {
    Carry {
        changed_files: BTreeMap::from([("src/cli.rs".to_owned(), "a1b2c3d4e5f6a7b8".to_owned())]),
    }
}

fn record_of(messages: &[ChatMessage]) -> &str {
    let record = messages
        .iter()
        .find(|message| message.purpose == Some(MessagePurpose::CompactedMemory))
        .expect("no record was left in place of the folded history");
    assert!(record.content.starts_with(RECORD_HEADER));
    &record.content
}

#[test]
fn compaction_preserves_what_the_work_depends_on() {
    let mut messages = working_session();
    let system = messages[0].clone();
    let done = compaction::compact(&mut messages, 10, &carry(), Trigger::Manual)
        .expect("there was history to fold");
    assert!(done.tokens_after < done.tokens_before);

    // The harness rules, verbatim and first.
    assert_eq!(messages[0], system);
    let record = record_of(&messages);
    // The task, verbatim.
    assert!(
        record.contains(
            "The first request: Add a --verbose flag to the CLI and keep the tests green."
        ),
        "{record}"
    );
    // Files changed, with the hash the checkpoint recorded.
    assert!(record.contains("src/cli.rs (a1b2c3d4e5f6)"), "{record}");
    // Paths worked on.
    assert!(
        record.contains("- paths you worked with: src/cli.rs"),
        "{record}"
    );
    // Decisions and where the model left off.
    assert!(
        record.contains("Decided: the flag lives in Args"),
        "{record}"
    );
    // The error still open, with its evidence -- and not the one a later
    // success of the same command resolved.
    assert!(
        record.contains("- still unresolved: run_command cargo test: exit 101 -- error[E0425]"),
        "{record}"
    );
    assert!(
        !record.contains("run_command cargo fmt: exit 1"),
        "{record}"
    );
    // What the checks said last.
    assert!(
        record.contains("- the checks last said: 1 check failing (cargo test)."),
        "{record}"
    );
    // The bulk is gone.
    assert!(!record.contains("fn body() {}"), "{record}");
    // The newest request survives verbatim in the tail.
    assert!(
        messages
            .iter()
            .any(|message| message.content == "Also document the flag in README.md."),
        "the newest request was folded"
    );
    assert_eq!(done.preserved.unresolved.len(), 1);
    assert_eq!(done.preserved.changed_files.len(), 1);
}

#[test]
fn manual_and_automatic_compaction_are_the_same_function() {
    let mut manual = working_session();
    let mut automatic = working_session();
    let by_hand = compaction::compact(&mut manual, 60, &carry(), Trigger::Manual).unwrap();
    let by_threshold =
        compaction::compact(&mut automatic, 60, &carry(), Trigger::Automatic).unwrap();
    assert_eq!(manual, automatic, "the triggers produced different prompts");
    assert_eq!(by_hand.preserved, by_threshold.preserved);
    assert_eq!(by_hand.trigger, Trigger::Manual);
    assert_eq!(by_threshold.trigger, Trigger::Automatic);
}

#[test]
fn a_second_compaction_keeps_what_the_first_preserved() {
    let mut messages = working_session();
    compaction::compact(&mut messages, 60, &carry(), Trigger::Automatic).unwrap();
    // More work after the first compaction, then another.
    messages.push(call("read_file", serde_json::json!({"path": "README.md"})));
    messages.push(result(
        serde_json::json!({"content": "# tool\n".repeat(300)}),
    ));
    messages.push(said("assistant", "README read."));
    messages.push(said("user", "carry on"));
    messages.push(said("assistant", "On it."));
    compaction::compact(&mut messages, 10, &carry(), Trigger::Manual).expect("folded again");

    let records = messages
        .iter()
        .filter(|message| message.purpose == Some(MessagePurpose::CompactedMemory))
        .count();
    assert_eq!(records, 1, "records nested instead of merging");
    let record = record_of(&messages);
    assert!(
        record.contains("The first request: Add a --verbose flag"),
        "the task was lost on the second compaction: {record}"
    );
    assert!(
        record.contains("run_command cargo test: exit 101"),
        "{record}"
    );
    assert!(record.contains("README.md"), "{record}");
    assert!(record.contains("the checks last said"), "{record}");
    // A record is not a request the person made.
    assert!(
        !record.contains("you were asked: Earlier in this conversation"),
        "{record}"
    );
}

#[test]
fn a_compacted_prompt_stays_well_formed() {
    for tail in [0usize, 10, 60, 400, 4_000] {
        let mut messages = working_session();
        if compaction::compact(&mut messages, tail, &Carry::default(), Trigger::Manual).is_none() {
            continue;
        }
        assert_eq!(messages[0].role, "system");
        assert!(messages.iter().any(|message| message.role == "user"));
        assert_ne!(
            messages[2].role, "tool",
            "the kept window opens on an orphaned result"
        );
    }
}

#[test]
fn nothing_to_fold_is_reported_rather_than_faked() {
    let mut messages = vec![said("system", "s"), said("user", "hello")];
    assert!(compaction::compact(&mut messages, 0, &Carry::default(), Trigger::Manual).is_none());
    // A record alone is not folded again into an identical record.
    let mut messages = working_session();
    compaction::compact(&mut messages, 0, &Carry::default(), Trigger::Manual).unwrap();
    let after_first = messages.clone();
    let again = compaction::compact(&mut messages, 0, &Carry::default(), Trigger::Manual);
    if again.is_none() {
        assert_eq!(messages, after_first);
    }
}

#[test]
fn composition_accounts_for_every_estimated_token() {
    let mut messages = working_session();
    messages.insert(
        2,
        ChatMessage {
            role: "user".into(),
            content: "Files this session changed: src/cli.rs".into(),
            purpose: Some(MessagePurpose::SessionLedger),
            ..Default::default()
        },
    );
    let parts = composition(&messages);
    assert_eq!(parts.total(), estimated_tokens(&messages));
    assert!(parts.system > 0);
    assert!(parts.conversation > 0);
    assert!(parts.task_state > 0);
    // File reads and searches are repository content; command output is not.
    assert!(parts.repository > parts.tool_results, "{parts:?}");
    assert!(parts.tool_results > 0);
    assert_eq!(parts.compacted_memory, 0);

    compaction::compact(&mut messages, 60, &carry(), Trigger::Manual).unwrap();
    let after = composition(&messages);
    assert!(after.compacted_memory > 0);
    assert_eq!(after.system, parts.system, "the system prompt changed size");
    assert!(after.total() < parts.total());
}

#[test]
fn a_compaction_is_audited_with_its_trigger_and_leaves_the_workspace_alone() {
    let workspace = tempfile::tempdir().unwrap();
    let index = workspace.path().join(".pwr/index/symbols.json");
    std::fs::create_dir_all(index.parent().unwrap()).unwrap();
    std::fs::write(&index, "{\"symbols\": [\"parse_args\"]}").unwrap();
    std::fs::write(workspace.path().join("main.rs"), "fn main() {}\n").unwrap();
    let store = pwr_store::Store::open(workspace.path().join(".pwr/state.sqlite")).unwrap();
    let conversation = pwr_domain::new_id();

    let mut messages = working_session();
    let done = compaction::compact(&mut messages, 60, &carry(), Trigger::Manual).unwrap();
    compaction::record(&store, conversation, &done, "qwen3-4b", 32_768).unwrap();

    let events = store.events_for_run(conversation).unwrap();
    let event = events
        .iter()
        .find(|event| event.event_type == COMPACTED_EVENT)
        .expect("no context.compacted event");
    assert_eq!(event.payload["trigger"], "manual");
    assert_eq!(event.payload["model"], "qwen3-4b");
    assert_eq!(event.payload["session_id"], conversation.to_string());
    assert_eq!(event.payload["estimated_tokens_before"], done.tokens_before);
    assert_eq!(event.payload["estimated_tokens_after"], done.tokens_after);
    assert!(
        event.payload["estimate_basis"]
            .as_str()
            .unwrap()
            .contains("not a provider count")
    );

    let last = compaction::last_recorded(&store, conversation)
        .unwrap()
        .unwrap();
    assert_eq!(last["trigger"], "manual");
    assert_eq!(last["tokensAfter"], done.tokens_after);

    // The repository and its index are exactly as they were.
    assert_eq!(
        std::fs::read_to_string(&index).unwrap(),
        "{\"symbols\": [\"parse_args\"]}"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.path().join("main.rs")).unwrap(),
        "fn main() {}\n"
    );
}
