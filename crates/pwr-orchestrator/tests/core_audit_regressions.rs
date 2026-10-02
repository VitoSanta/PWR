use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{DenyWithoutAsking, conversation, converse};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::{PolicyProfile, ToolPolicy};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
struct Fake {
    replies: Mutex<VecDeque<Vec<ToolCall>>>,
}
#[async_trait]
impl ModelProvider for Fake {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let calls = self.replies.lock().unwrap().pop_front().unwrap_or_default();
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            tool_calls: calls,
            done: true,
            ..Default::default()
        })])))
    }
}
fn call(name: &str, id: &str, args: serde_json::Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments: args,
        id: Some(id.into()),
    }
}
async fn turn(
    fake: &Fake,
    store: &pwr_store::Store,
    id: pwr_domain::Id,
    policy: &ToolPolicy,
    messages: &mut Vec<ChatMessage>,
    stop: &AtomicBool,
    continuity: &converse::Continuity,
) -> converse::TurnReport {
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    converse::take_turn(
        fake,
        &pwr_compat::GenericAdapter,
        &deployment,
        store,
        id,
        policy,
        messages,
        16384,
        &[],
        Default::default(),
        serde_json::json!([]),
        stop,
        continuity,
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap()
}
struct AllowNetwork {
    asked: std::sync::atomic::AtomicUsize,
}
#[async_trait]
impl pwr_orchestrator::ApprovalPrompt for AllowNetwork {
    async fn ask(&self, a: pwr_tools::Approval, _: &str) -> pwr_orchestrator::ApprovalDecision {
        if a == pwr_tools::Approval::NetworkAccess {
            self.asked.fetch_add(1, Ordering::Relaxed);
            pwr_orchestrator::ApprovalDecision::AllowOnce
        } else {
            pwr_orchestrator::ApprovalDecision::Deny
        }
    }
}
#[tokio::test]
async fn core_audit_effects_and_protocol_survive_stop_and_budget() {
    let fixture = tempfile::tempdir().unwrap();
    let base = fixture.path().to_owned();
    std::fs::create_dir_all(&base).unwrap();
    // Actual conversation engine: one available action, two reads in one response.
    let batch = base.join("batch");
    std::fs::create_dir_all(&batch).unwrap();
    std::fs::write(batch.join("a.txt"), "a").unwrap();
    std::fs::write(batch.join("b.txt"), "b").unwrap();
    let policy = PolicyProfile::Safe.build(batch);
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([vec![
            call("read_file", "c1", serde_json::json!({"path":"a.txt"})),
            call("read_file", "c2", serde_json::json!({"path":"b.txt"})),
        ]])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "read both"),
    ];
    let continuity = converse::Continuity {
        action_limit: Some(1),
        ..Default::default()
    };
    let stop = AtomicBool::new(false);
    let report = turn(
        &fake,
        &store,
        id,
        &policy,
        &mut messages,
        &stop,
        &continuity,
    )
    .await;
    assert_eq!(
        messages
            .iter()
            .filter(|m| m.role == "tool" && m.tool_call_id.is_some())
            .count(),
        2
    );
    println!(
        "partial_batch: stopped={:?} declared_calls={} tool_results={} transcript={}",
        report.stopped,
        messages.iter().map(|m| m.tool_calls.len()).sum::<usize>(),
        messages
            .iter()
            .filter(|m| m.role == "tool" && m.tool_call_id.is_some())
            .count(),
        serde_json::to_string(&messages).unwrap()
    );
    // Actual action future: write first, then block. Stop drops it after the marker exists.
    let root = base.join("stop");
    std::fs::create_dir_all(&root).unwrap();
    let mut policy = PolicyProfile::Safe.build(root.clone());
    policy.allow_commands = vec!["sh".into()];
    policy.timeout = Duration::from_secs(3);
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([
            vec![call(
                "run_command",
                "r1",
                serde_json::json!({"executable":"sh","args":["-c","printf changed > marker; sleep 2"]}),
            )],
            vec![call(
                "write_file",
                "w1",
                serde_json::json!({"path":"next.txt","content":"next"}),
            )],
        ])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "work"),
    ];
    conversation::record_snapshot(&store, id, &messages).unwrap();
    let continuity = converse::Continuity {
        action_limit: Some(1),
        ..Default::default()
    };
    let stop = Arc::new(AtomicBool::new(false));
    let watched = root.join("marker");
    let token = stop.clone();
    let watcher = tokio::spawn(async move {
        for _ in 0..200 {
            if watched.exists() {
                token.store(true, Ordering::Relaxed);
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        token.store(true, Ordering::Relaxed);
    });
    let first = turn(
        &fake,
        &store,
        id,
        &policy,
        &mut messages,
        &stop,
        &continuity,
    )
    .await;
    watcher.await.unwrap();
    let uncertain1 = conversation::restore(&store, id)
        .unwrap()
        .unwrap()
        .unreceipted;
    assert!(first.edited);
    println!(
        "stopped_effect: marker_exists={} edited={} stopped={:?} shared_next_intent={} uncertain={:?}",
        root.join("marker").exists(),
        first.edited,
        first.stopped,
        continuity.checkpoint.lock().unwrap().next_intent,
        uncertain1
    );
    stop.store(false, Ordering::Relaxed);
    messages.push(ChatMessage::text("user", "continue"));
    let second = turn(
        &fake,
        &store,
        id,
        &policy,
        &mut messages,
        &stop,
        &continuity,
    )
    .await;
    let uncertain2 = conversation::restore(&store, id)
        .unwrap()
        .unwrap()
        .unreceipted;
    let events = store.events_for_run(id).unwrap();
    let sequences: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == conversation::INTENT_EVENT)
        .map(|e| e.payload["sequence"].clone())
        .collect();
    assert_eq!(sequences, vec![serde_json::json!(1), serde_json::json!(2)]);
    assert_eq!(uncertain2.len(), 1);
    println!(
        "stop_then_mutation: next_file_exists={} edited={} intent_sequences={:?} uncertain_after={:?}",
        root.join("next.txt").exists(),
        second.edited,
        sequences,
        uncertain2
    );
    // Timeout also follows a command which already changed the disk.
    let root = base.join("timeout");
    std::fs::create_dir_all(&root).unwrap();
    let mut policy = PolicyProfile::Safe.build(root.clone());
    policy.allow_commands = vec!["sh".into()];
    policy.timeout = Duration::from_millis(150);
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([vec![call(
            "run_command",
            "t1",
            serde_json::json!({"executable":"sh","args":["-c","printf changed > marker; sleep 2"]}),
        )]])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "work"),
    ];
    let continuity = converse::Continuity {
        action_limit: Some(1),
        ..Default::default()
    };
    let stop = AtomicBool::new(false);
    let report = turn(
        &fake,
        &store,
        id,
        &policy,
        &mut messages,
        &stop,
        &continuity,
    )
    .await;
    assert!(report.edited);
    println!(
        "timeout_effect: marker_exists={} edited={} actions={} transcript={}",
        root.join("marker").exists(),
        report.edited,
        report.actions,
        serde_json::to_string(&messages).unwrap()
    );

    let root = base.join("retry");
    std::fs::create_dir_all(&root).unwrap();
    let mut policy = PolicyProfile::Safe.build(root.clone());
    policy.allow_commands = vec!["sh".into()];
    policy.timeout = Duration::from_secs(2);
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([vec![call(
            "run_command",
            "n1",
            serde_json::json!({"executable":"sh","args":["-c","printf x >> marker; printf NU1301 >&2; exit 1"]}),
        )]])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "work"),
    ];
    let continuity = converse::Continuity {
        action_limit: Some(1),
        ..Default::default()
    };
    let stop = AtomicBool::new(false);
    let prompt = AllowNetwork {
        asked: std::sync::atomic::AtomicUsize::new(0),
    };
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let report = converse::take_turn(
        &fake,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        id,
        &policy,
        &mut messages,
        16384,
        &[],
        Default::default(),
        serde_json::json!([]),
        &stop,
        &continuity,
        &prompt,
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read_to_string(root.join("marker")).unwrap(), "x");
    println!(
        "permission_retry: marker={:?} network_questions={} proposal_actions={} (network denial simulated, no network contacted)",
        std::fs::read_to_string(root.join("marker")).unwrap(),
        prompt.asked.load(Ordering::Relaxed),
        report.actions
    );
}

struct PromptCapacity;
#[async_trait]
impl ModelProvider for PromptCapacity {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        Err(ProviderError::PromptTooLarge {
            safe_context: "actual schema and template exceed the window".into(),
        })
    }
    async fn prepare_context(
        &self,
        _: &DeploymentDescriptor,
        _: u32,
    ) -> Result<u32, ProviderError> {
        panic!("An oversized input must not cause a smaller context reload")
    }
}

#[tokio::test]
async fn actual_prompt_overflow_does_not_reload_at_a_smaller_tier() {
    let root = tempfile::tempdir().unwrap();
    let store = pwr_store::Store::open(":memory:").unwrap();
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "work"),
    ];
    let report = converse::take_turn(
        &PromptCapacity,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        32768,
        &[16384, 32768],
        Default::default(),
        serde_json::json!([]),
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    assert_eq!(report.stopped, Some(converse::StopReason::ContextFull));
}

#[tokio::test]
async fn failed_write_completion_recovers_inside_workspace() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("directory")).unwrap();
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([
            vec![call(
                "write_file",
                "bad",
                serde_json::json!({"path":"directory","content":"bad"}),
            )],
            vec![call(
                "complete",
                "premature",
                serde_json::json!({"rationale":"I will provide the files."}),
            )],
            vec![call(
                "write_file",
                "fixed",
                serde_json::json!({"path":"README.md","content":"delivered"}),
            )],
            vec![call(
                "complete",
                "done",
                serde_json::json!({"rationale":"Created README.md."}),
            )],
        ])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "create files"),
    ];
    let report = turn(
        &fake,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        &AtomicBool::new(false),
        &converse::Continuity::default(),
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("README.md")).unwrap(),
        "delivered"
    );
    assert!(report.outcome.delivered);
}

#[tokio::test]
async fn failed_write_and_listing_do_not_prove_delivery() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("directory")).unwrap();
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([
            vec![call(
                "write_file",
                "bad",
                serde_json::json!({"path":"directory","content":"bad"}),
            )],
            vec![call("list_tree", "list", serde_json::json!({"path":"."}))],
            vec![call(
                "complete",
                "end1",
                serde_json::json!({"rationale":"I will provide the files."}),
            )],
            vec![call(
                "complete",
                "end2",
                serde_json::json!({"rationale":"Cannot create the files."}),
            )],
        ])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "create files"),
    ];
    let report = turn(
        &fake,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        &AtomicBool::new(false),
        &converse::Continuity::default(),
    )
    .await;
    assert!(!report.outcome.delivered);
}

#[tokio::test]
async fn command_failure_is_visible_and_is_not_delivery() {
    let root = tempfile::tempdir().unwrap();
    let mut policy = PolicyProfile::Safe.build(root.path().to_owned());
    policy.allow_commands.push("sh".into());
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([
            vec![call(
                "run_command",
                "failure",
                serde_json::json!({"executable":"sh","args":["-c","printf 'AssertionError: billing total\n'; printf 'npm notice test\n' >&2; exit 1"]}),
            )],
            vec![call(
                "complete",
                "end",
                serde_json::json!({"rationale":"The command failed."}),
            )],
        ])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "fake".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "work"),
    ];
    let mut failed = false;
    let mut failure_detail = String::new();
    let report = converse::take_turn(
        &fake,
        &pwr_compat::GenericAdapter,
        &deployment,
        &store,
        pwr_domain::new_id(),
        &policy,
        &mut messages,
        16384,
        &[],
        Default::default(),
        serde_json::json!([]),
        &AtomicBool::new(false),
        &converse::Continuity::default(),
        &DenyWithoutAsking,
        |step| {
            if let converse::TurnStep::ToolCall(call) = step
                && let converse::ToolPhase::Failed(why) = call.phase
            {
                failed = true;
                failure_detail = why;
            }
        },
    )
    .await
    .unwrap();
    assert!(
        failed,
        "nonzero exit must be visible as failed in the live trace"
    );
    assert!(
        failure_detail.contains("AssertionError: billing total"),
        "stdout diagnostic was hidden: {failure_detail}"
    );
    assert!(
        failure_detail.contains("npm notice test"),
        "stderr was lost: {failure_detail}"
    );
    assert!(
        report.edited,
        "possible command effects still require checks"
    );
    assert!(!report.outcome.delivered);
}

#[tokio::test]
async fn repaired_inputs_allow_checks_after_repeated_failures() {
    for varying_diagnostics in [false, true] {
        for changed in [false, true] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("unchanged.txt"), "stable").unwrap();
            let mut policy = PolicyProfile::Safe.build(root.path().to_owned());
            policy.allow_commands.push("sh".into());
            let attempts = if varying_diagnostics { 6 } else { 3 };
            let last = char::from(b'a' + attempts - 1);
            let diagnostic = if varying_diagnostics {
                "ls attempt-*"
            } else {
                "printf 'still failing\\n'"
            };
            let script = format!("{diagnostic}; test -f attempt-{last}.txt");
            let mut replies = VecDeque::new();
            for n in 0..attempts {
                let letter = char::from(b'a' + n);
                let edit_path = if changed {
                    format!("attempt-{letter}.txt")
                } else {
                    "unchanged.txt".into()
                };
                replies.push_back(vec![call(
                    "write_file",
                    &format!("edit-{letter}"),
                    serde_json::json!({"path":edit_path, "content":"repair"}),
                )]);
                replies.push_back(vec![call(
                    "run_command",
                    &format!("check-{letter}"),
                    serde_json::json!({"executable":"sh", "args":["-c",script]}),
                )]);
            }
            let fake = Fake {
                replies: Mutex::new(replies),
            };
            let store = pwr_store::Store::open(":memory:").unwrap();
            let mut messages = vec![
                ChatMessage::text("system", "s"),
                ChatMessage::text("user", "repair and check"),
            ];
            turn(
                &fake,
                &store,
                pwr_domain::new_id(),
                &policy,
                &mut messages,
                &AtomicBool::new(false),
                &converse::Continuity {
                    action_limit: Some(usize::from(attempts) * 2),
                    ..Default::default()
                },
            )
            .await;
            let outcomes: Vec<_> = messages
                .iter()
                .filter(|m| m.role == "tool")
                .filter_map(|m| {
                    serde_json::from_str::<serde_json::Value>(
                        m.content.lines().next().unwrap_or(""),
                    )
                    .ok()
                })
                .filter_map(|v| v.get("result").cloned())
                .filter(|v| v.get("exit_code").is_some())
                .collect();
            if changed {
                assert_eq!(
                    outcomes.len(),
                    usize::from(attempts),
                    "a real file repair must permit another check: {messages:?}"
                );
                assert_eq!(outcomes.last().unwrap()["exit_code"], 0);
            } else {
                assert_eq!(
                    outcomes.len(),
                    2,
                    "refused edits must not renew failed command attempts"
                );
                assert!(outcomes.iter().all(|v| v["exit_code"] != 0));
            }
        }
    }
}

#[tokio::test]
async fn path_repairs_renew_checks_but_directory_noops_do_not() {
    for kind in [
        "delete",
        "move",
        "mkdir",
        "restore",
        "mkdir-noop",
        "restore-noop",
    ] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("source.txt"), "good").unwrap();
        if kind == "mkdir-noop" {
            std::fs::create_dir(root.path().join("created")).unwrap();
        }
        let mut policy = PolicyProfile::Safe.build(root.path().to_owned());
        policy.allow_commands.push("sh".into());
        let mut replies = VecDeque::new();
        if kind.starts_with("restore") {
            replies.push_back(vec![call(
                "read_file",
                "original",
                serde_json::json!({"path":"source.txt"}),
            )]);
        }
        if kind == "restore" {
            replies.push_back(vec![call("replace_text", "break", serde_json::json!({"path":"source.txt", "expected_hash":pwr_domain::hash_bytes(b"good"), "find":"good", "replace":"bad"}))]);
        }
        let (script, repair) = match kind {
            "delete" => (
                "test ! -f source.txt",
                call(
                    "delete_path",
                    "repair",
                    serde_json::json!({"path":"source.txt", "expected_hash":pwr_domain::hash_bytes(b"good")}),
                ),
            ),
            "move" => (
                "test -f moved.txt",
                call(
                    "move_path",
                    "repair",
                    serde_json::json!({"from":"source.txt", "to":"moved.txt"}),
                ),
            ),
            "mkdir" => (
                "test -d created",
                call(
                    "make_directory",
                    "repair",
                    serde_json::json!({"path":"created"}),
                ),
            ),
            "restore" => (
                "grep -q good source.txt",
                call(
                    "restore_file",
                    "repair",
                    serde_json::json!({"path":"source.txt"}),
                ),
            ),
            "mkdir-noop" => (
                "exit 1",
                call(
                    "make_directory",
                    "repair",
                    serde_json::json!({"path":"created"}),
                ),
            ),
            _ => (
                "exit 1",
                call(
                    "restore_file",
                    "repair",
                    serde_json::json!({"path":"source.txt"}),
                ),
            ),
        };
        let check = |id: &str| {
            call(
                "run_command",
                id,
                serde_json::json!({"executable":"sh", "args":["-c",script]}),
            )
        };
        replies.extend([
            vec![check("check1")],
            vec![check("check2")],
            vec![repair],
            vec![check("check3")],
        ]);
        let fake = Fake {
            replies: Mutex::new(replies),
        };
        let store = pwr_store::Store::open(":memory:").unwrap();
        let mut messages = vec![
            ChatMessage::text("system", "s"),
            ChatMessage::text("user", "repair"),
        ];
        turn(
            &fake,
            &store,
            pwr_domain::new_id(),
            &policy,
            &mut messages,
            &AtomicBool::new(false),
            &converse::Continuity::default(),
        )
        .await;
        let codes: Vec<_> = messages
            .iter()
            .filter(|m| m.role == "tool")
            .filter_map(|m| pwr_orchestrator::tool_result_json(&m.content))
            .filter_map(|v| v["result"]["exit_code"].as_i64())
            .collect();
        if kind.ends_with("noop") {
            assert_eq!(codes, [1, 1], "{kind}: {messages:?}");
        } else {
            assert_eq!(codes, [1, 1, 0], "{kind}: {messages:?}");
        }
    }
}

#[tokio::test]
async fn completion_rationale_without_artifacts_is_not_delivery() {
    let root = tempfile::tempdir().unwrap();
    let fake = Fake {
        replies: Mutex::new(VecDeque::from([
            vec![call(
                "complete",
                "first",
                serde_json::json!({"rationale":"Created all files."}),
            )],
            vec![call(
                "complete",
                "second",
                serde_json::json!({"rationale":"Created all files."}),
            )],
        ])),
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let mut messages = vec![
        ChatMessage::text("system", "s"),
        ChatMessage::text("user", "create files"),
    ];
    let report = turn(
        &fake,
        &store,
        pwr_domain::new_id(),
        &PolicyProfile::Safe.build(root.path().to_owned()),
        &mut messages,
        &AtomicBool::new(false),
        &converse::Continuity::default(),
    )
    .await;
    assert!(
        report.completed,
        "the model may terminate, but its rationale is only a claim"
    );
    assert!(!report.outcome.delivered);
}
