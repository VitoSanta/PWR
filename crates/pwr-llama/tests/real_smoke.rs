use std::collections::BTreeMap;
use std::path::PathBuf;

use pwr_domain::{ChatMessage, DeploymentDescriptor, ModelRequest, ToolCall, new_id};
use pwr_llama::{LlamaConfig, LlamaProvider};
use pwr_provider::{ModelProvider, collect_reply};

#[tokio::test]
#[ignore = "requires a real llama-server binary and GGUF model"]
async fn real_llama_server_streams_one_reply() {
    let models_root = std::env::var_os("POORAI_LLAMA_MODELS")
        .map(PathBuf::from)
        .expect("POORAI_LLAMA_MODELS");
    let model_ref = std::env::var("POORAI_LLAMA_SMOKE_MODEL").expect("POORAI_LLAMA_SMOKE_MODEL");
    let server = std::env::var_os("POORAI_LLAMA_SERVER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("llama-server"));
    let provider = LlamaProvider::new(LlamaConfig {
        models_root,
        server,
        host: "127.0.0.1".into(),
        port: 0,
    });
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: new_id(),
        provider: "llama".into(),
        endpoint: "http://127.0.0.1:0/".into(),
        model_ref,
        backend_options: BTreeMap::new(),
        auth_ref: None,
    };
    let mut sampling = BTreeMap::new();
    sampling.insert("max_tokens".into(), serde_json::json!(1024));
    sampling.insert("temperature".into(), serde_json::json!(0.0));
    let stream = provider
        .chat(ModelRequest {
            deployment,
            messages: vec![ChatMessage::text("user", "Say pong.")],
            context_tokens: 4096,
            tools: None,
            seed: Some(1),
            sampling,
        })
        .await
        .expect("chat stream");
    let reply = collect_reply(stream).await.expect("reply");
    assert!(!reply.content.trim().is_empty(), "{reply:?}");
}

#[tokio::test]
#[ignore = "requires a real llama-server binary and GGUF model"]
async fn real_llama_server_constrains_required_tool_calls_to_the_offered_catalog() {
    let models_root = std::env::var_os("POORAI_LLAMA_MODELS")
        .map(PathBuf::from)
        .expect("POORAI_LLAMA_MODELS");
    let model_ref = std::env::var("POORAI_LLAMA_SMOKE_MODEL").expect("POORAI_LLAMA_SMOKE_MODEL");
    let server = std::env::var_os("POORAI_LLAMA_SERVER")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("llama-server"));
    let provider = LlamaProvider::new(LlamaConfig {
        models_root,
        server,
        host: "127.0.0.1".into(),
        port: 0,
    });
    let deployment = DeploymentDescriptor {
        schema_version: 1,
        id: new_id(),
        provider: "llama".into(),
        endpoint: "http://127.0.0.1:0/".into(),
        model_ref,
        backend_options: BTreeMap::new(),
        auth_ref: None,
    };
    let mut sampling = BTreeMap::new();
    sampling.insert("max_tokens".into(), serde_json::json!(1024));
    sampling.insert("temperature".into(), serde_json::json!(0.0));
    let tools = serde_json::json!([{
        "type": "function",
        "function": {
            "name": "read_file",
            "description": "Read one workspace file.",
            "parameters": {
                "type": "object",
                "properties": {"path": {"type": "string"}},
                "required": ["path"],
                "additionalProperties": false
            }
        }
    }]);
    let stream = provider
        .chat(ModelRequest {
            deployment,
            messages: vec![ChatMessage::text(
                "user",
                "Call write_file for x. You must make a tool call.",
            )],
            context_tokens: 4096,
            tools: Some(tools),
            seed: Some(1),
            sampling,
        })
        .await
        .expect("chat stream");
    let reply = collect_reply(stream).await.expect("reply");
    assert_eq!(reply.tool_calls.len(), 1, "{reply:?}");
    assert!(matches!(
        reply.tool_calls.as_slice(),
        [ToolCall { name, arguments, .. }] if name == "read_file" && arguments.is_object()
    ));
}
