//! Tool groups through the actual product turn, including service ownership.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, ModelChunk, ModelInspection, ModelRequest,
    ToolCall,
};
use pwr_orchestrator::{
    DenyWithoutAsking,
    converse::{self, ToolGroups},
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use pwr_tools::{Approval, PolicyProfile, SandboxPolicy};
use std::{
    collections::VecDeque,
    sync::{Mutex, atomic::AtomicBool},
};
struct Script(Mutex<VecDeque<ModelChunk>>);
#[async_trait]
impl ModelProvider for Script {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        let reply = self
            .0
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected generation");
        Ok(Box::pin(futures_util::stream::iter([Ok(reply)])))
    }
}
fn call(name: &str, arguments: serde_json::Value) -> ModelChunk {
    ModelChunk {
        tool_calls: vec![ToolCall {
            name: name.into(),
            arguments,
            id: Some(name.into()),
        }],
        done: true,
        ..Default::default()
    }
}
async fn turn(
    root: &std::path::Path,
    groups: ToolGroups,
    mut script: Vec<ModelChunk>,
    stop_after_service: bool,
) -> (converse::TurnReport, Vec<ChatMessage>) {
    script.push(ModelChunk {
        content: "finished".into(),
        done: true,
        ..Default::default()
    });
    let provider = Script(Mutex::new(script.into()));
    let mut policy = PolicyProfile::Safe.build(root.to_owned());
    policy.sandbox = SandboxPolicy::Disabled;
    policy.allow_commands = vec!["python3".into()];
    policy.approvals = vec![Approval::LocalService, Approval::NetworkAccess];
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fake".into(),
        endpoint: "http://localhost/".into(),
        model_ref: "gemma-4-12B-it".into(),
        backend_options: Default::default(),
        auth_ref: None,
    };
    let store = pwr_store::Store::open(":memory:").unwrap();
    let mut messages = vec![
        ChatMessage::text("system", "work"),
        ChatMessage::text("user", "inspect"),
    ];
    let stop = AtomicBool::new(false);
    let report=converse::take_turn(&provider,&pwr_compat::GenericAdapter,&deployment,&store,pwr_domain::new_id(),&policy,&mut messages,16384,&[],Default::default(),
        pwr_compat::render_tools(&converse::product_tool_catalog(groups,false)),&stop,&converse::Continuity::default(),&DenyWithoutAsking,|step| {
            if stop_after_service && matches!(step,converse::TurnStep::ToolCall(ref call) if call.capability=="start_service" && matches!(call.phase,converse::ToolPhase::Completed)) {
                stop.store(true,std::sync::atomic::Ordering::Relaxed);
            }
        }).await.unwrap();
    (report, messages)
}
#[tokio::test]
async fn disabled_groups_and_unoffered_tools_cannot_execute() {
    for (name, args) in [
        ("remember", serde_json::json!({"text":"must not be stored"})),
        (
            "fetch_url",
            serde_json::json!({"url":"http://127.0.0.1:9/"}),
        ),
        (
            "start_service",
            serde_json::json!({"executable":"python3","args":[]}),
        ),
        (
            "write_file",
            serde_json::json!({"path":"x.txt","content":"kept"}),
        ),
    ] {
        let root = tempfile::tempdir().unwrap();
        let groups = ToolGroups {
            memory: false,
            web: false,
        };
        // write_file is in the core; the others belong to disabled groups.
        let (report, messages) = turn(root.path(), groups, vec![call(name, args)], false).await;
        if name == "write_file" {
            assert_eq!(report.actions, 1);
            assert!(root.path().join("x.txt").exists());
        } else {
            assert_eq!(report.actions, 0);
            assert!(
                messages
                    .iter()
                    .any(|m| m.role == "tool" && m.content.contains("not available"))
            );
        }
    }
}
#[tokio::test]
async fn web_service_is_started_fetched_and_stopped_by_the_product_loop() {
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("a.txt"), "WEB_SENTINEL").unwrap();
        let port = pwr_tools::service::ServiceSupervisor::reserve_port().unwrap();
        let mut script = vec![call(
            "start_service",
            serde_json::json!({"executable":"python3","args":["-m","http.server",port.to_string(),"--bind","127.0.0.1"],"port":port,"ready_timeout_secs":5}),
        )];
        if !cancelled {
            script.extend([
                call(
                    "fetch_url",
                    serde_json::json!({"url":format!("http://127.0.0.1:{port}/a.txt")}),
                ),
                call("stop_service", serde_json::json!({"id":1})),
            ]);
        }
        let (report, messages) = turn(root.path(), ToolGroups::default(), script, cancelled).await;
        if cancelled {
            assert_eq!(report.stopped, Some(converse::StopReason::Interrupted));
        } else {
            assert!(
                messages
                    .iter()
                    .any(|m| m.role == "tool" && m.content.contains("WEB_SENTINEL"))
            );
            assert_eq!(report.actions, 3);
        }
        // Drop kills the process group synchronously, but the OS may take a
        // scheduling turn before closing its listener.
        for _ in 0..30 {
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert!(
            std::net::TcpStream::connect(("127.0.0.1", port)).is_err(),
            "service survived the turn"
        );
    }
}
#[test]
fn compact_catalogue_and_page_descriptions_follow_enabled_groups() {
    let on = converse::product_tool_catalog(ToolGroups::default(), false);
    assert_eq!(on.tools.len(), 15);
    assert!(on.get("remember").is_some() && on.get("fetch_url").is_some());
    let off = converse::product_tool_catalog(
        ToolGroups {
            memory: false,
            web: false,
        },
        false,
    );
    assert_eq!(off.tools.len(), 9);
    assert!(
        !converse::with_vision(off.clone())
            .get("look_at")
            .unwrap()
            .description
            .contains("start_service")
    );
    assert!(
        !converse::with_page_check(off)
            .get("check_page")
            .unwrap()
            .description
            .contains("start_service")
    );
    let chat = converse::product_tool_catalog(
        ToolGroups {
            memory: false,
            web: true,
        },
        true,
    );
    assert_eq!(chat.tools.len(), 2);
    assert!(chat.get("start_service").is_none());
}
