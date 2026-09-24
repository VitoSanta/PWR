// Included inside the existing test module to reuse its provider fixtures.
use std::path::Path;
struct RecordingProvider {
    replies: std::sync::Mutex<std::collections::VecDeque<ModelChunk>>,
    requests: std::sync::Mutex<Vec<ModelRequest>>,
}
#[async_trait]
impl ModelProvider for RecordingProvider {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> { unreachable!() }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> { unreachable!() }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front().ok_or(ProviderError::Protocol {safe_context:"fixture exhausted".into()})?;
        Ok(Box::pin(stream::iter([Ok(reply)])))
    }
}
fn regression_reply(content: String, tool_calls: Vec<pwr_domain::ToolCall>) -> ModelChunk {
    ModelChunk { content, thinking:None, tool_calls, metrics:None, done:true }
}
fn regression_policy(root: &Path) -> ToolPolicy {
    ToolPolicy { root:root.to_path_buf(), extra_readable:vec![], protected:vec![], allow_commands:vec!["sh".into()],
        output_limit:64*1024, timeout:Duration::from_secs(5), sandbox:pwr_tools::SandboxPolicy::Disabled, approvals:vec![] }
}
fn regression_request() -> ModelRequest {
    ModelRequest { deployment:deployment(), context_tokens:4096, tools:None, seed:None, sampling:Default::default(),
        messages:vec![pwr_domain::ChatMessage::text("user", "repair the project")] }
}

#[tokio::test]
async fn regression_diagnostics_survive_real_loop_compaction_and_clear_after_repair() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("marker"), "pass").unwrap();
    fs::write(root.path().join("notes"), "lots of unrelated context\n".repeat(3000)).unwrap();
    fs::write(root.path().join("check.sh"), "if test \"$(cat marker)\" = pass; then exit 0; fi\necho 'src/app/app.ts:7:3: error: missing component' >&2\nexit 1").unwrap();
    let action = |from: &str, to: &str| serde_json::json!({"capability":"apply_replace", "path":"marker", "expected_hash":pwr_domain::hash_bytes(from), "replacement":to}).to_string();
    let mut replies = vec![regression_reply(action("pass", "fail"), vec![])];
    for _ in 0..10 { replies.push(regression_reply(r#"{"capability":"read_file","path":"notes"}"#.into(), vec![])); }
    replies.push(regression_reply(action("fail", "pass"), vec![]));
    replies.push(regression_reply(r#"{"capability":"read_file","path":"notes"}"#.into(), vec![]));
    replies.push(regression_reply(r#"{"capability":"complete","rationale":"fixed"}"#.into(), vec![]));
    let provider = RecordingProvider { replies:std::sync::Mutex::new(replies.into()), requests:Default::default() };
    let store = Store::open(":memory:").unwrap();
    let id = new_id();
    let result = run_action_loop(&store, &provider, id, regression_request(), &regression_policy(root.path()), &[("sh".into(),vec!["check.sh".into()])], 20).await.unwrap();
    assert!(result.verified);
    let events = store.events_for_run(id).unwrap();
    assert!(events.iter().any(|e| e.event_type=="context.compacted"));
    let requests = provider.requests.lock().unwrap();
    // The model's request immediately before repair still has the useful error,
    // even after ten reads and actual production-loop compactions.
    let before_repair = &requests[11];
    assert!(before_repair.messages.iter().any(|m| m.content.contains("missing component") && m.content.contains("src/app/app.ts")));
    let mut after_repair = requests.last().unwrap().clone();
    compact_history(&store,id,&mut after_repair,14,&crate::plan::Plan::default(),&[],2048,crate::evidence::ContextPolicy::Current,std::path::Path::new(".")).unwrap();
    assert!(!after_repair.messages.iter().any(|m|m.content.contains("missing component")));
    let latest = events.iter().rev().find(|e| e.event_type=="verification.diagnostics").unwrap();
    assert_eq!(latest.payload["passing"], true);
    assert_eq!(latest.payload["diagnostics"]["failing_checks"], serde_json::json!([]));
    assert!(pwr_domain::RunEvent::from_stored(&latest.event_type,&latest.payload).is_some());
}

#[tokio::test]
async fn regression_real_read_batches_bound_first_later_and_deferred_results() {
    for (count, budget, large_index) in [(1,3,0),(2,4,1),(6,8,99),(7,10,0),(7,2,99)] {
        let root = tempfile::tempdir().unwrap();
        for i in 0..count { fs::write(root.path().join(format!("f{i}")), if i==large_index {"\"\\\n💡".repeat(9000)} else {"small".into()}).unwrap(); }
        let calls = (0..count).map(|i|pwr_domain::ToolCall {name:"read_file".into(),arguments:serde_json::json!({"path":format!("f{i}")}),id:None}).collect();
        let provider = RecordingProvider {replies:std::sync::Mutex::new(vec![regression_reply(String::new(),calls),regression_reply(r#"{"capability":"decline","rationale":"fixture done"}"#.into(),vec![])].into()),requests:Default::default()};
        let store = Store::open(":memory:").unwrap(); let id=new_id();
        let mut request = regression_request();
        request.context_tokens = 32768;
        let _ = run_action_loop(&store,&provider,id,request,&regression_policy(root.path()),&[],budget).await;
        let events=store.events_for_run(id).unwrap();
        let batch=events.iter().find(|e|e.event_type=="context.read_batch").unwrap();
        assert!(batch.payload["returned_bytes"].as_u64().unwrap()<=batch.payload["byte_limit"].as_u64().unwrap());
        let reads=events.iter().filter(|e|e.event_type=="tool.action" && e.payload["action"]["capability"]=="read_file").count();
        assert_eq!(reads,batch.payload["performed"].as_u64().unwrap() as usize);
        assert!(reads<=usize::from(budget) && reads<=MAX_READS_PER_TURN);
        if count==6 {assert_eq!(reads,6);}
        if count==7 && budget==10 {assert_eq!(reads,6);}
        if budget==2 {assert_eq!(reads,2);}
        if let Some(next)=provider.requests.lock().unwrap().get(1) {
            let message=next.messages.iter().rev().find_map(|m|crate::tool_result_json(&m.content).filter(|v|v.get("result").is_some())).unwrap();
            assert!(json_bytes(&message["result"])<=batch.payload["byte_limit"].as_u64().unwrap() as usize);
            assert_eq!(message["status"]["actions_remaining"],serde_json::json!(usize::from(budget)-reads));
            if count==1 { assert!(message["result"]["truncated"].as_bool().unwrap()); assert!(message["result"]["expected_hash"].is_string()); }
        }
    }
}
