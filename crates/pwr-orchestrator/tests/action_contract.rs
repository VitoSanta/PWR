//! Real conversation executor, durable receipts and filesystem/process effects.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{
    DenyWithoutAsking,
    action_contract::{ActionContract, ExactCommand, unix_ms},
    converse,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::{SandboxPolicy, ToolPolicy};
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

struct Model {
    calls: Mutex<Vec<Vec<ToolCall>>>,
    asked: AtomicUsize,
    delay: bool,
}
#[async_trait]
impl ModelProvider for Model {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        if self.delay {
            tokio::time::sleep(Duration::from_secs(10)).await;
        }
        let mut calls = self.calls.lock().unwrap();
        let mut tools = if calls.is_empty() {
            vec![]
        } else {
            calls.remove(0)
        };
        let content = if tools
            .first()
            .is_some_and(|call| call.name == "__raw_fixture")
        {
            let content = tools[0].arguments["text"].as_str().unwrap().to_owned();
            tools.clear();
            content
        } else if tools.is_empty() {
            "done".into()
        } else {
            String::new()
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(ModelChunk {
            content,
            tool_calls: tools,
            done: true,
            ..Default::default()
        })])))
    }
}
fn call(name: &str, args: Value) -> ToolCall {
    ToolCall {
        name: name.into(),
        arguments: args,
        id: None,
    }
}
fn files() -> ActionContract {
    ActionContract {
        turn: 1,
        expires_unix_ms: unix_ms() + 30_000,
        read_paths: vec!["allowed.py".into()],
        replace_paths: vec!["allowed.py".into()],
        command: None,
        max_actions: 6,
    }
}
fn command() -> ActionContract {
    ActionContract {
        read_paths: vec![],
        replace_paths: vec![],
        command: Some(ExactCommand {
            executable: "/usr/bin/touch".into(),
            cwd: ".".into(),
            args: vec!["ran".into()],
        }),
        max_actions: 1,
        ..files()
    }
}
fn root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("allowed.py"), "old").unwrap();
    root
}
async fn run(
    root: &std::path::Path,
    contract: Option<ActionContract>,
    calls: Vec<Vec<ToolCall>>,
    stopped: bool,
    delay: bool,
) -> (converse::TurnReport, Vec<pwr_store::EventRecord>, usize) {
    let model = Model {
        calls: Mutex::new(calls),
        asked: AtomicUsize::new(0),
        delay,
    };
    let policy = ToolPolicy {
        root: root.into(),
        extra_readable: vec![],
        protected: vec![],
        allow_commands: vec!["/usr/bin/touch".into(), "/bin/sh".into()],
        output_limit: 4096,
        timeout: Duration::from_secs(2),
        sandbox: SandboxPolicy::Disabled,
        approvals: vec![],
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let id = pwr_domain::new_id();
    let continuity = converse::Continuity {
        action_contract: contract,
        ..Default::default()
    };
    continuity.person_turn.store(1, Ordering::Relaxed);
    let mut messages = vec![
        ChatMessage::text("system", "act"),
        ChatMessage::text("user", "do the phase"),
    ];
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
        &model,
        &pwr_compat::QwenFamilyAdapter,
        &deployment,
        &store,
        id,
        &policy,
        &mut messages,
        16384,
        &[],
        Default::default(),
        json!([]),
        &AtomicBool::new(stopped),
        &continuity,
        &DenyWithoutAsking,
        |_| {},
    )
    .await
    .unwrap();
    (
        report,
        store.events_for_run(id).unwrap(),
        model.asked.load(Ordering::Relaxed),
    )
}
fn receipts(events: &[pwr_store::EventRecord]) -> usize {
    events
        .iter()
        .filter(|e| e.event_type == "tool.action")
        .count()
}

#[tokio::test]
async fn permitted_reads_and_replace_have_real_effects_and_receipts() {
    let root = root();
    let edit = call(
        "replace_text",
        json!({"path":"allowed.py","expected_hash":pwr_domain::hash_bytes("old"),"find":"old","replace":"new"}),
    );
    let (_, events, _) = run(
        root.path(),
        Some(files()),
        vec![
            vec![call("read_file", json!({"path":"allowed.py"}))],
            vec![edit],
        ],
        false,
        false,
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("allowed.py")).unwrap(),
        "new"
    );
    assert_eq!(receipts(&events), 2);
}
#[tokio::test]
async fn unoffered_and_alternative_capabilities_never_execute_and_stop_at_two() {
    for proposal in [
        call("search", json!({"query":"old"})),
        call("write_file", json!({"path":"new_test.py","content":"bad"})),
        call(
            "apply_replace",
            json!({"path":"allowed.py","expected_hash":pwr_domain::hash_bytes("old"),"replacement":"bad"}),
        ),
        call("plan_task", json!({"steps":[]})),
        call("read_file", json!({"path":"../outside"})),
    ] {
        let root = root();
        let (report, events, asked) = run(
            root.path(),
            Some(files()),
            vec![vec![
                proposal.clone(),
                proposal,
                call("read_file", json!({"path":"allowed.py"})),
            ]],
            false,
            false,
        )
        .await;
        assert_eq!(report.stopped, Some(converse::StopReason::ContractBlocked));
        assert_eq!(asked, 1);
        assert_eq!(receipts(&events), 0);
        assert_eq!(
            std::fs::read_to_string(root.path().join("allowed.py")).unwrap(),
            "old"
        );
        assert!(!root.path().join("new_test.py").exists());
        assert_eq!(
            events
                .iter()
                .filter(|e| e.event_type == "contract.proposal")
                .count(),
            2
        );
    }
}
#[tokio::test]
async fn command_exactly_once_closes_before_second_call_in_same_reply() {
    let root = root();
    let cmd = call(
        "run_command",
        json!({"executable":"/usr/bin/touch","cwd":".","args":["ran"]}),
    );
    let (report, events, asked) = run(
        root.path(),
        Some(command()),
        vec![vec![
            cmd,
            call(
                "run_command",
                json!({"executable":"/usr/bin/touch","cwd":".","args":["second"]}),
            ),
        ]],
        false,
        false,
    )
    .await;
    assert!(root.path().join("ran").exists());
    assert!(!root.path().join("second").exists());
    assert_eq!(receipts(&events), 1);
    assert_eq!(asked, 1);
    assert_eq!(report.actions, 1);
    assert!(
        events
            .iter()
            .any(|e| e.event_type == "contract.command_receipt")
    );
}
#[tokio::test]
async fn command_alias_stdin_inline_argv_and_cwd_are_denied_without_process_effects() {
    let canonical = json!({"executable":"/usr/bin/touch","cwd":".","args":["ran"]});
    for (key, value) in [
        ("executable", json!("touch")),
        ("stdin", json!("None")),
        ("args", json!(["ran", "extra"])),
        ("cwd", json!("..")),
        ("executable", json!("/bin/sh")),
        ("outside_sandbox", json!(true)),
    ] {
        let root = root();
        let mut args = canonical.clone();
        args[key] = value;
        let proposal = call("run_command", args);
        let (_, events, _) = run(
            root.path(),
            Some(command()),
            vec![vec![proposal.clone(), proposal]],
            false,
            false,
        )
        .await;
        assert_eq!(receipts(&events), 0);
        assert!(!root.path().join("ran").exists());
    }
}
#[test]
fn invalid_stale_contract_and_symlink_escape_fail_before_inference() {
    let root = root();
    let mut spec = files();
    spec.turn = 2;
    assert!(spec.validate(root.path(), 1).is_err());
    spec.turn = 1;
    spec.expires_unix_ms = 0;
    assert!(spec.validate(root.path(), 1).is_err());
    let mut spec = files();
    spec.max_actions = 0;
    assert!(spec.validate(root.path(), 1).is_err());
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), "secret").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("link")).unwrap();
    let mut spec = files();
    spec.read_paths = vec!["link".into()];
    spec.replace_paths = vec![];
    assert!(spec.validate(root.path(), 1).is_err());
    assert!(serde_json::from_value::<ActionContract>(json!({"turn":1,"unexpected":true})).is_err());
}
#[tokio::test]
async fn a_symlink_changed_after_validation_is_denied_before_read() {
    let root = root();
    let spec = files();
    spec.validate(root.path(), 1).unwrap();
    std::fs::remove_file(root.path().join("allowed.py")).unwrap();
    std::os::unix::fs::symlink("/etc/hosts", root.path().join("allowed.py")).unwrap();
    let proposal = call("read_file", json!({"path":"allowed.py"}));
    assert!(spec.check(root.path(), 1, 0, &proposal).is_err());
}
#[tokio::test]
async fn cancelled_turn_runs_nothing_and_expiry_interrupts_generation() {
    let root = root();
    let (_, events, asked) = run(root.path(), Some(files()), vec![], true, false).await;
    assert_eq!(asked, 0);
    assert_eq!(receipts(&events), 0);
    let mut spec = files();
    spec.expires_unix_ms = unix_ms() + 100;
    let (report, events, asked) = run(root.path(), Some(spec), vec![], false, true).await;
    assert_eq!(report.stopped, Some(converse::StopReason::ContractBlocked));
    assert_eq!(asked, 1);
    assert_eq!(receipts(&events), 0);
}
#[tokio::test]
async fn absent_contract_preserves_ordinary_write_capability() {
    let root = root();
    let (_, events, _) = run(
        root.path(),
        None,
        vec![vec![call(
            "write_file",
            json!({"path":"new_test.py","content":"normal"}),
        )]],
        false,
        false,
    )
    .await;
    assert_eq!(
        std::fs::read_to_string(root.path().join("new_test.py")).unwrap(),
        "normal"
    );
    assert_eq!(receipts(&events), 1);
    assert!(!events.iter().any(|e| e.event_type.starts_with("contract.")));
}
#[test]
fn schema_only_exposes_phase_capabilities_and_counter_cannot_reuse_command() {
    let spec = files();
    assert_eq!(
        spec.catalog(converse::chat_tool_catalog())
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        vec!["read_file", "replace_text"]
    );
    let root = root();
    assert!(
        command()
            .check(
                root.path(),
                1,
                1,
                &call(
                    "run_command",
                    json!({"executable":"/usr/bin/touch","cwd":".","args":["ran"]})
                )
            )
            .is_err()
    );
}

#[tokio::test]
async fn expiry_kills_command_before_its_later_effect_and_keeps_intent() {
    let root = root();
    let mut spec = command();
    spec.expires_unix_ms = unix_ms() + 150;
    spec.command = Some(ExactCommand {
        executable: "/bin/sh".into(),
        cwd: ".".into(),
        args: vec!["-c".into(), "sleep 1; touch too_late".into()],
    });
    let proposal = call(
        "run_command",
        json!({"executable":"/bin/sh","cwd":".","args":["-c","sleep 1; touch too_late"]}),
    );
    let (report, events, _) =
        run(root.path(), Some(spec), vec![vec![proposal]], false, false).await;
    assert_eq!(report.stopped, Some(converse::StopReason::ContractBlocked));
    assert!(!root.path().join("too_late").exists());
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "action.intent")
    );
    assert!(
        !events
            .iter()
            .any(|event| event.event_type == "contract.command_receipt")
    );
}

#[tokio::test]
async fn malformed_closing_uses_the_same_two_attempt_limit_as_a_denied_edit() {
    let root = root();
    let invalid = call(
        "replace_text",
        json!({"path":"allowed.py","find":"old","replace":"bad"}),
    );
    let raw = call(
        "__raw_fixture",
        json!({"text":"<tool_call>\n<function=complete>\n</parameter>\n</function>\n</tool_call>"}),
    );
    let (report, events, asked) = run(
        root.path(),
        Some(files()),
        vec![vec![invalid], vec![raw.clone()], vec![raw]],
        false,
        false,
    )
    .await;
    assert_eq!(report.stopped, Some(converse::StopReason::ContractBlocked));
    assert_eq!(asked, 2);
    assert_eq!(receipts(&events), 0);
    assert!(
        events
            .iter()
            .any(|event| event.event_type == "contract.reply_rejected")
    );
    assert_eq!(
        std::fs::read_to_string(root.path().join("allowed.py")).unwrap(),
        "old"
    );
}

#[tokio::test]
async fn packed_numbered_commands_are_not_repaired_into_an_allowed_command() {
    let root = root();
    let proposal = call(
        "run_command",
        json!({"executable":"/usr/bin/touch","cwd":".","args":["1. /usr/bin/touch ran\n2. /usr/bin/touch second"]}),
    );
    let (report, events, _) = run(
        root.path(),
        Some(command()),
        vec![vec![proposal.clone(), proposal]],
        false,
        false,
    )
    .await;
    assert_eq!(report.stopped, Some(converse::StopReason::ContractBlocked));
    assert_eq!(receipts(&events), 0);
    assert!(!root.path().join("ran").exists());
    assert!(!root.path().join("second").exists());
}
