//! The llama.cpp/GGUF engine.
//!
//! The backend reads GGUF metadata for the computed window, starts a managed
//! `llama-server` process on demand, and converts the OpenAI-compatible stream
//! into PWR's provider stream.

use async_trait::async_trait;
use chrono::Utc;
use futures_util::{StreamExt, stream};
use pwr_domain::{
    BackendState, DeploymentDescriptor, GenerationMetrics, ModelChunk, ModelDefinition,
    ModelInspection, ModelRequest, Observation, Provenance, ToolCall, hash_bytes, new_id,
};
use pwr_provider::{
    BackendCapabilities, DiscoveredModel, InferenceBackend, ModelFacts, ModelProvider, ModelStream,
    ProviderError,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, VecDeque};
use std::io::{Cursor, Read};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::{Child, Command};
use tokio::time::{sleep, timeout};

pub const DEFAULT_MAX_TOKENS: u64 = 16_384;
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const SERVER_READY_INTERVAL: Duration = Duration::from_millis(100);
const SERVER_VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const INSPECTION_ARRAY_SAMPLE_ITEMS: usize = 32;

#[derive(Debug, Clone)]
pub struct LlamaConfig {
    pub models_root: PathBuf,
    pub server: PathBuf,
    pub host: String,
    pub port: u16,
}

impl LlamaConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        LlamaConfig {
            models_root: std::env::var_os("PWR_LLAMA_MODELS")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".lmstudio/models")),
            server: std::env::var_os("PWR_LLAMA_SERVER")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("llama-server")),
            host: std::env::var("PWR_LLAMA_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("PWR_LLAMA_PORT")
                .ok()
                .and_then(|port| port.parse().ok())
                .unwrap_or(0),
        }
    }

    pub fn model_file(&self, model_ref: &str) -> Result<PathBuf, ProviderError> {
        let expanded = match model_ref.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(model_ref)),
            None => PathBuf::from(model_ref),
        };
        let candidate = if expanded.is_absolute() {
            expanded
        } else {
            self.models_root.join(expanded)
        };
        if candidate.is_file()
            && candidate
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            Ok(candidate)
        } else {
            Err(ProviderError::Protocol {
                safe_context: format!(
                    "{model_ref} is not a GGUF model file under {}",
                    self.models_root.display()
                ),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LlamaServerPlan {
    pub server: PathBuf,
    pub model: PathBuf,
    pub host: String,
    pub port: u16,
    pub context_tokens: u32,
}

impl LlamaServerPlan {
    pub fn args(&self) -> Vec<String> {
        vec![
            "--model".into(),
            self.model.display().to_string(),
            "--host".into(),
            self.host.clone(),
            "--port".into(),
            self.port.to_string(),
            "--ctx-size".into(),
            self.context_tokens.to_string(),
        ]
    }

    pub fn endpoint(&self) -> String {
        format!("http://{}:{}/v1/chat/completions", self.host, self.port)
    }
}

pub fn chat_completions_body(request: &ModelRequest) -> serde_json::Value {
    let sampling = &request.sampling;
    let number = |name: &str| {
        sampling
            .get(name)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    let mut body = serde_json::json!({
        "model": request.deployment.model_ref,
        "messages": chat_messages(&request.messages),
        "tools": request.tools,
        "stream": true,
        "max_tokens": sampling
            .get("max_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_MAX_TOKENS),
        "temperature": number("temperature"),
        "top_p": number("top_p"),
        "top_k": number("top_k"),
        "seed": request.seed,
    });
    if request.tools.is_some() {
        // llama-server derives a grammar from the offered native tools. Mark
        // the choice required so an agent turn cannot fall back to prose and
        // leave the harness guessing whether a malformed call was intended.
        body["tool_choice"] = serde_json::Value::String("required".into());
    }
    body
}

pub async fn post_chat_stream(
    client: &reqwest::Client,
    endpoint: &str,
    request: ModelRequest,
) -> Result<ModelStream, ProviderError> {
    let response = client
        .post(endpoint)
        .json(&chat_completions_body(&request))
        .send()
        .await
        .map_err(|error| ProviderError::Unavailable {
            safe_context: format!("llama.cpp chat request failed: {error}"),
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(ProviderError::Protocol {
            safe_context: format!("llama.cpp chat request returned HTTP {status}"),
        });
    }
    let bytes = response.bytes_stream();
    let state = (bytes, LlamaSseDecoder::default(), VecDeque::new());
    Ok(Box::pin(stream::try_unfold(
        state,
        |(mut bytes, mut decoder, mut queue)| async move {
            loop {
                if let Some(chunk) = queue.pop_front() {
                    return Ok(Some((chunk, (bytes, decoder, queue))));
                }
                let Some(next) = bytes.next().await else {
                    return Ok(None);
                };
                let bytes = next.map_err(|error| ProviderError::Unavailable {
                    safe_context: format!("llama.cpp stream read failed: {error}"),
                })?;
                let text =
                    std::str::from_utf8(&bytes).map_err(|error| ProviderError::Protocol {
                        safe_context: format!("llama.cpp stream was not UTF-8: {error}"),
                    })?;
                queue.extend(decoder.push(text)?);
            }
        },
    )))
}

fn keep_server_alive(stream: ModelStream, server: LlamaServer) -> ModelStream {
    Box::pin(stream::try_unfold(
        (stream, Some(server)),
        |(mut stream, server)| async move {
            let Some(next) = stream.next().await else {
                drop(server);
                return Ok(None);
            };
            Ok(Some((next?, (stream, server))))
        },
    ))
}

#[derive(Debug)]
pub struct LlamaServer {
    plan: LlamaServerPlan,
    child: Child,
}

impl LlamaServer {
    pub async fn start(
        plan: LlamaServerPlan,
        client: &reqwest::Client,
    ) -> Result<Self, ProviderError> {
        let mut child = Command::new(&plan.server)
            .args(plan.args())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|error| ProviderError::Unavailable {
                safe_context: format!(
                    "cannot start llama.cpp server {}: {error}",
                    plan.server.display()
                ),
            })?;
        if let Err(error) = wait_until_ready(client, &plan, &mut child).await {
            let _ = child.kill().await;
            return Err(error);
        }
        Ok(Self { plan, child })
    }

    pub fn endpoint(&self) -> String {
        self.plan.endpoint()
    }
}

impl Drop for LlamaServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

async fn wait_until_ready(
    client: &reqwest::Client,
    plan: &LlamaServerPlan,
    child: &mut Child,
) -> Result<(), ProviderError> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| ProviderError::Unavailable {
                safe_context: format!("cannot poll llama.cpp server: {error}"),
            })?
        {
            return Err(ProviderError::Unavailable {
                safe_context: format!("llama.cpp server exited before becoming ready: {status}"),
            });
        }
        if server_ready(client, plan).await {
            return Ok(());
        }
        if started.elapsed() >= SERVER_READY_TIMEOUT {
            return Err(ProviderError::Timeout {
                safe_context: format!(
                    "llama.cpp server did not become ready on {}:{} within {} seconds",
                    plan.host,
                    plan.port,
                    SERVER_READY_TIMEOUT.as_secs()
                ),
            });
        }
        sleep(SERVER_READY_INTERVAL).await;
    }
}

async fn server_ready(client: &reqwest::Client, plan: &LlamaServerPlan) -> bool {
    for path in ["/health", "/v1/models"] {
        let url = format!("http://{}:{}{}", plan.host, plan.port, path);
        if client
            .get(url)
            .send()
            .await
            .is_ok_and(|response| response.status().is_success())
        {
            return true;
        }
    }
    false
}

async fn read_server_version(server: &Path) -> Result<Option<String>, ProviderError> {
    let output = timeout(
        SERVER_VERSION_TIMEOUT,
        Command::new(server).arg("--version").output(),
    )
    .await
    .map_err(|_| ProviderError::Timeout {
        safe_context: format!(
            "llama.cpp server version command exceeded {} seconds",
            SERVER_VERSION_TIMEOUT.as_secs()
        ),
    })?
    .map_err(|error| ProviderError::Unavailable {
        safe_context: format!(
            "cannot run llama.cpp server version command {}: {error}",
            server.display()
        ),
    })?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr)
    } else {
        String::from_utf8_lossy(&output.stdout)
    };
    Ok(text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_owned()))
}

fn chat_messages(messages: &[pwr_domain::ChatMessage]) -> Vec<serde_json::Value> {
    let mut answering = false;
    messages
        .iter()
        .map(|message| {
            let mut value = chat_message(message);
            match message.role.as_str() {
                "assistant" => answering = !message.tool_calls.is_empty(),
                "tool" if answering => {}
                "tool" => {
                    value["role"] = serde_json::json!("user");
                    value["content"] = serde_json::json!(format!("[PWR] {}", message.content));
                    if let Some(object) = value.as_object_mut() {
                        object.remove("tool_call_id");
                    }
                }
                _ => answering = false,
            }
            value
        })
        .collect()
}

fn chat_message(message: &pwr_domain::ChatMessage) -> serde_json::Value {
    let mut value = serde_json::json!({"role": message.role, "content": message.content});
    let object = value.as_object_mut().expect("literal object");
    if !message.tool_calls.is_empty() {
        object.insert(
            "tool_calls".into(),
            serde_json::Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "id": call.id,
                            "type": "function",
                            "function": {
                                "name": call.name,
                                "arguments": call.arguments.to_string()
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }
    if let Some(id) = &message.tool_call_id {
        object.insert("tool_call_id".into(), serde_json::json!(id));
    }
    value
}

#[derive(Debug, Default)]
pub struct LlamaStreamDecoder {
    tool_calls: BTreeMap<u64, PartialToolCall>,
}

impl LlamaStreamDecoder {
    pub fn decode_data(&mut self, data: &str) -> Result<Option<ModelChunk>, ProviderError> {
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            return Ok(None);
        }
        let event: Value = serde_json::from_str(data).map_err(|error| ProviderError::Protocol {
            safe_context: format!("llama.cpp stream event was not JSON: {error}"),
        })?;
        self.decode_event(&event)
    }

    fn decode_event(&mut self, event: &Value) -> Result<Option<ModelChunk>, ProviderError> {
        let Some(choice) = event["choices"]
            .as_array()
            .and_then(|choices| choices.first())
        else {
            return Ok(None);
        };
        let delta = &choice["delta"];
        self.capture_tool_calls(delta)?;
        let content = delta["content"].as_str().unwrap_or_default().to_owned();
        let thinking = delta
            .get("reasoning_content")
            .or_else(|| delta.get("reasoning"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        let finish = choice["finish_reason"].as_str();
        if let Some(reason) = finish {
            if reason != "stop" && reason != "tool_calls" {
                return Err(ProviderError::Truncated {
                    safe_context: format!(
                        "llama.cpp stopped the reply with finish_reason {reason}"
                    ),
                });
            }
            return Ok(Some(ModelChunk {
                content,
                thinking,
                tool_calls: self.finish_tool_calls()?,
                metrics: metrics(event),
                done: true,
            }));
        }
        if content.is_empty() && thinking.as_ref().is_none_or(String::is_empty) {
            return Ok(None);
        }
        Ok(Some(ModelChunk {
            content,
            thinking,
            tool_calls: Vec::new(),
            metrics: None,
            done: false,
        }))
    }

    fn capture_tool_calls(&mut self, delta: &Value) -> Result<(), ProviderError> {
        let Some(calls) = delta["tool_calls"].as_array() else {
            return Ok(());
        };
        for (position, call) in calls.iter().enumerate() {
            let index = call["index"].as_u64().unwrap_or(position as u64);
            let partial = self.tool_calls.entry(index).or_default();
            if let Some(id) = call["id"].as_str() {
                partial.id = Some(id.to_owned());
            }
            if let Some(name) = call["function"]["name"].as_str() {
                partial.name = Some(name.to_owned());
            }
            if let Some(arguments) = call["function"]["arguments"].as_str() {
                partial.arguments.push_str(arguments);
            }
        }
        Ok(())
    }

    fn finish_tool_calls(&mut self) -> Result<Vec<ToolCall>, ProviderError> {
        let partials = std::mem::take(&mut self.tool_calls);
        partials
            .into_values()
            .map(|partial| partial.finish())
            .collect()
    }
}

#[derive(Debug, Default)]
struct PartialToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

impl PartialToolCall {
    fn finish(self) -> Result<ToolCall, ProviderError> {
        let Some(name) = self.name else {
            return Err(ProviderError::ModelOutput {
                safe_context: "llama.cpp streamed a tool call without a function name".into(),
            });
        };
        let arguments =
            serde_json::from_str(&self.arguments).map_err(|error| ProviderError::ModelOutput {
                safe_context: format!("llama.cpp streamed invalid tool arguments: {error}"),
            })?;
        Ok(ToolCall {
            name,
            arguments,
            id: self.id,
        })
    }
}

fn metrics(event: &Value) -> Option<GenerationMetrics> {
    let usage = event.get("usage")?;
    // The server counts with the model's own tokenizer. It separates
    // reasoning tokens only where its build reports them in the OpenAI shape;
    // otherwise the split is left unknown rather than set to zero.
    let generated = usage["completion_tokens"].as_u64();
    let reasoning = usage["completion_tokens_details"]["reasoning_tokens"].as_u64();
    Some(GenerationMetrics {
        prompt_tokens: usage["prompt_tokens"].as_u64(),
        generated_tokens: generated,
        reasoning_tokens: reasoning,
        answer_tokens: generated
            .zip(reasoning)
            .map(|(total, reasoning)| total.saturating_sub(reasoning)),
        token_accounting: generated.map(|_| pwr_domain::TokenAccounting::ServerUsage),
        ..GenerationMetrics::default()
    })
}

fn reserve_port(host: &str) -> Result<u16, ProviderError> {
    let listener = TcpListener::bind((host, 0)).map_err(|error| ProviderError::Unavailable {
        safe_context: format!("cannot reserve a loopback port for llama.cpp: {error}"),
    })?;
    listener
        .local_addr()
        .map(|address| address.port())
        .map_err(|error| ProviderError::Unavailable {
            safe_context: format!("cannot read reserved llama.cpp port: {error}"),
        })
}

#[derive(Debug, Default)]
pub struct LlamaSseDecoder {
    pending: String,
    event_data: Vec<String>,
    stream: LlamaStreamDecoder,
}

impl LlamaSseDecoder {
    pub fn push(&mut self, text: &str) -> Result<Vec<ModelChunk>, ProviderError> {
        self.pending.push_str(text);
        let mut chunks = Vec::new();
        while let Some(newline) = self.pending.find('\n') {
            let mut line = self.pending[..newline].to_owned();
            if line.ends_with('\r') {
                line.pop();
            }
            self.pending.drain(..=newline);
            if let Some(chunk) = self.consume_line(&line)? {
                chunks.push(chunk);
            }
        }
        Ok(chunks)
    }

    fn consume_line(&mut self, line: &str) -> Result<Option<ModelChunk>, ProviderError> {
        if line.is_empty() {
            let data = self.event_data.join("\n");
            self.event_data.clear();
            return self.stream.decode_data(&data);
        }
        if let Some(data) = line.strip_prefix("data:") {
            self.event_data.push(data.trim_start().to_owned());
        }
        Ok(None)
    }
}

pub struct LlamaProvider {
    config: LlamaConfig,
}

impl LlamaProvider {
    pub fn new(config: LlamaConfig) -> Self {
        Self { config }
    }

    pub fn server_plan(
        &self,
        deployment: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<LlamaServerPlan, ProviderError> {
        let port = if self.config.port == 0 {
            reserve_port(&self.config.host)?
        } else {
            self.config.port
        };
        Ok(LlamaServerPlan {
            server: self.config.server.clone(),
            model: self.config.model_file(&deployment.model_ref)?,
            host: self.config.host.clone(),
            port,
            context_tokens,
        })
    }

    fn read_metadata(path: &Path) -> Result<Value, ProviderError> {
        let mut file = std::fs::File::open(path).map_err(|error| ProviderError::Protocol {
            safe_context: format!("cannot read {}: {error}", path.display()),
        })?;
        let mut header = [0_u8; 24];
        file.read_exact(&mut header)
            .map_err(|error| ProviderError::Protocol {
                safe_context: format!("cannot read GGUF header from {}: {error}", path.display()),
            })?;
        let mut cursor = Cursor::new(header);
        let mut magic = [0_u8; 4];
        cursor.read_exact(&mut magic).expect("in memory");
        if &magic != b"GGUF" {
            return Err(ProviderError::Protocol {
                safe_context: format!("{} is not a GGUF file", path.display()),
            });
        }
        let _version = read_u32(&mut cursor)?;
        let _tensor_count = read_u64(&mut cursor)?;
        let metadata_count = read_u64(&mut cursor)?;
        let mut metadata = Map::new();
        for _ in 0..metadata_count {
            let key = read_string(&mut file)?;
            let value_type = read_u32(&mut file)?;
            let value = read_value(&mut file, value_type)?;
            metadata.insert(key, value);
        }
        Ok(Value::Object(metadata))
    }

    fn inspect_file(
        &self,
        path: &Path,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelInspection, ProviderError> {
        let metadata = Self::read_metadata(path)?;
        let metadata_bytes =
            serde_json::to_vec(&metadata).map_err(|error| ProviderError::Protocol {
                safe_context: format!(
                    "GGUF metadata from {} is not serialisable: {error}",
                    path.display()
                ),
            })?;
        let size = path.metadata().ok().map(|metadata| metadata.len());
        let digest = format!(
            "gguf-metadata:{}",
            hash_bytes(
                [
                    metadata_bytes.as_slice(),
                    &size.unwrap_or_default().to_le_bytes(),
                ]
                .concat()
            )
        );
        let template = metadata
            .get("tokenizer.chat_template")
            .and_then(Value::as_str);
        let chat_template_identity = template.map(|template| hash_bytes(template.as_bytes()));
        // llama-server separates reasoning into `reasoning_content` but takes
        // no per-request budget, and PWR does not pass template variables
        // to it: whatever the template supports, PWR can only observe.
        let reasoning = template
            .map(pwr_domain::TemplateReasoning::of)
            .unwrap_or_default();
        let reasoning_capability = match reasoning.capability(false) {
            pwr_domain::ReasoningCapability::TemplateControlled => {
                pwr_domain::ReasoningCapability::ObservableOnly
            }
            other => other,
        };
        let mut capabilities = BTreeMap::new();
        capabilities.insert(
            "capability_probe".into(),
            Observation::Unknown {
                reason: "capability probe not run; use `pwr models inspect MODEL --probe` to measure this deployment".into(),
            },
        );
        Ok(ModelInspection {
            definition: ModelDefinition {
                schema_version: 1,
                id: new_id(),
                digest: digest.clone(),
                family: metadata
                    .get("general.architecture")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                quantization: metadata
                    .get("general.file_type")
                    .and_then(Value::as_u64)
                    .map(|file_type| format!("gguf-file-type-{file_type}")),
                capabilities,
                metadata: serde_json::json!({
                    "path": path,
                    "weights_bytes": size,
                    "gguf": inspection_metadata(&metadata),
                    "chat_template_fingerprint": chat_template_identity,
                    "has_chat_template": template.is_some(),
                    "reasoning_template": reasoning,
                    "reasoning_capability": reasoning_capability,
                    "format": "gguf",
                }),
                provenance: Provenance {
                    source: format!("gguf:{}", path.display()),
                    observed_at: Utc::now(),
                    content_hash: digest,
                },
            },
            deployment: deployment.clone(),
        })
    }
}

#[async_trait]
impl ModelProvider for LlamaProvider {
    async fn inspect(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelInspection, ProviderError> {
        let path = self.config.model_file(&deployment.model_ref)?;
        self.inspect_file(&path, deployment)
    }

    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        Ok(BackendState {
            observed_at: Utc::now(),
            loaded_models: Vec::new(),
            state: serde_json::json!({
                "engine": "llama.cpp",
                "server": self.config.server,
                "host": self.config.host,
                "port": self.config.port,
                "status": "server_on_demand"
            }),
        })
    }

    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        let plan = self.server_plan(&request.deployment, request.context_tokens)?;
        let client = reqwest::Client::new();
        let server = LlamaServer::start(plan, &client).await?;
        let stream = post_chat_stream(&client, &server.endpoint(), request).await?;
        Ok(keep_server_alive(stream, server))
    }

    async fn prepare_context(
        &self,
        _deployment: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        Ok(context_tokens)
    }
}

#[async_trait]
impl InferenceBackend for LlamaProvider {
    fn backend_id(&self) -> &'static str {
        "llama"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_discovery: true,
            model_lifecycle: true,
            streaming: true,
            cancellation: true,
            native_tools: true,
            constrained_tool_calls: true,
            generation_metrics: true,
            context_window_control: true,
        }
    }

    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError> {
        let mut found = Vec::new();
        for path in gguf_files(&self.config.models_root) {
            let Ok(relative) = path.strip_prefix(&self.config.models_root) else {
                continue;
            };
            let metadata = Self::read_metadata(&path).ok();
            let context_limit = metadata.as_ref().and_then(context_limit);
            found.push(DiscoveredModel {
                model_ref: relative.display().to_string(),
                digest: None,
                context_limit,
                size_bytes: path.metadata().ok().map(|metadata| metadata.len()),
                accelerator_size_bytes: None,
            });
        }
        found.sort_by(|a, b| a.model_ref.cmp(&b.model_ref));
        Ok(found)
    }

    async fn model_facts(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelFacts, ProviderError> {
        let path = self.config.model_file(&deployment.model_ref)?;
        let metadata = Self::read_metadata(&path)?;
        Ok(ModelFacts {
            hf_config: None,
            gguf_metadata: Some(metadata),
            trained_max: None,
            weights_bytes: path.metadata().ok().map(|metadata| metadata.len()),
            // llama.cpp can choose flash/fused attention at runtime; until the
            // server path reports it, window arithmetic uses the conservative
            // transient rule.
            prefill_scores_bytes: None,
            source: format!("gguf:{}", path.display()),
        })
    }

    async fn backend_version(&self) -> Result<Option<String>, ProviderError> {
        read_server_version(&self.config.server).await
    }
}

fn gguf_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(gguf_files(&path));
        } else if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
        {
            out.push(path);
        }
    }
    out
}

fn context_limit(metadata: &Value) -> Option<u32> {
    let arch = metadata.get("general.architecture")?.as_str()?;
    metadata
        .get(format!("{arch}.context_length"))?
        .as_u64()
        .and_then(|limit| u32::try_from(limit).ok())
}

fn inspection_metadata(metadata: &Value) -> Value {
    compact_inspection_value(metadata)
}

fn compact_inspection_value(value: &Value) -> Value {
    match value {
        Value::Array(values) if values.len() > INSPECTION_ARRAY_SAMPLE_ITEMS => {
            serde_json::json!({
                "items_total": values.len(),
                "sample": values
                    .iter()
                    .take(INSPECTION_ARRAY_SAMPLE_ITEMS)
                    .map(compact_inspection_value)
                    .collect::<Vec<_>>(),
                "truncated": true,
            })
        }
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(compact_inspection_value)
                .collect::<Vec<_>>(),
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), compact_inspection_value(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32, ProviderError> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(u32::from_le_bytes(bytes))
}

fn read_u64<R: Read>(reader: &mut R) -> Result<u64, ProviderError> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(u64::from_le_bytes(bytes))
}

fn read_i64<R: Read>(reader: &mut R) -> Result<i64, ProviderError> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(i64::from_le_bytes(bytes))
}

fn read_f32<R: Read>(reader: &mut R) -> Result<f32, ProviderError> {
    let mut bytes = [0_u8; 4];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(f32::from_le_bytes(bytes))
}

fn read_f64<R: Read>(reader: &mut R) -> Result<f64, ProviderError> {
    let mut bytes = [0_u8; 8];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    Ok(f64::from_le_bytes(bytes))
}

fn read_string<R: Read>(reader: &mut R) -> Result<String, ProviderError> {
    let len = read_u64(reader)?;
    let len = usize::try_from(len).map_err(|_| ProviderError::Protocol {
        safe_context: "GGUF string length does not fit this platform".into(),
    })?;
    let mut bytes = vec![0_u8; len];
    reader.read_exact(&mut bytes).map_err(read_error)?;
    String::from_utf8(bytes).map_err(|_| ProviderError::Protocol {
        safe_context: "GGUF metadata contains a non-UTF-8 string".into(),
    })
}

fn read_value<R: Read>(reader: &mut R, value_type: u32) -> Result<Value, ProviderError> {
    match value_type {
        0 => read_int(reader, 1, false),
        1 => read_int(reader, 1, true),
        2 => read_int(reader, 2, false),
        3 => read_int(reader, 2, true),
        4 => Ok(Value::from(read_u32(reader)?)),
        5 => read_int(reader, 4, true),
        6 => Ok(Value::from(read_f32(reader)?)),
        7 => {
            let mut byte = [0_u8; 1];
            reader.read_exact(&mut byte).map_err(read_error)?;
            Ok(Value::Bool(byte[0] != 0))
        }
        8 => Ok(Value::String(read_string(reader)?)),
        9 => read_array(reader),
        10 => Ok(Value::from(read_u64(reader)?)),
        11 => Ok(Value::from(read_i64(reader)?)),
        12 => Ok(Value::from(read_f64(reader)?)),
        other => Err(ProviderError::Protocol {
            safe_context: format!("unsupported GGUF metadata type {other}"),
        }),
    }
}

fn read_int<R: Read>(reader: &mut R, bytes: usize, signed: bool) -> Result<Value, ProviderError> {
    let mut raw = [0_u8; 8];
    reader.read_exact(&mut raw[..bytes]).map_err(read_error)?;
    if signed {
        let sign = raw[bytes - 1] & 0x80 != 0;
        if sign {
            for byte in &mut raw[bytes..] {
                *byte = 0xff;
            }
        }
        Ok(Value::from(i64::from_le_bytes(raw)))
    } else {
        Ok(Value::from(u64::from_le_bytes(raw)))
    }
}

fn read_array<R: Read>(reader: &mut R) -> Result<Value, ProviderError> {
    let value_type = read_u32(reader)?;
    let len = read_u64(reader)?;
    let len = usize::try_from(len).map_err(|_| ProviderError::Protocol {
        safe_context: "GGUF array length does not fit this platform".into(),
    })?;
    let mut values = Vec::with_capacity(len);
    for _ in 0..len {
        values.push(read_value(reader, value_type)?);
    }
    Ok(Value::Array(values))
}

fn read_error(error: std::io::Error) -> ProviderError {
    ProviderError::Protocol {
        safe_context: format!("could not read GGUF metadata: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwr_domain::{ChatMessage, ToolCall};
    use pwr_provider::{Cancel, InferenceBackend, collect_reply};
    use std::io::Write;

    #[test]
    fn reads_gguf_metadata_without_tensor_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny.gguf");
        write_tiny_gguf(&path);
        let metadata = LlamaProvider::read_metadata(&path).unwrap();
        assert_eq!(metadata["general.architecture"], "qwen35moe");
        assert_eq!(metadata["qwen35moe.context_length"], 262144);
        assert_eq!(
            metadata["qwen35moe.attention.head_count_kv"],
            serde_json::json!([2, 0, 2])
        );
    }

    #[test]
    fn inspection_metadata_summarizes_large_gguf_arrays() {
        let tokenizer_tokens = (0..100)
            .map(|index| Value::String(format!("token-{index}")))
            .collect::<Vec<_>>();
        let metadata = serde_json::json!({
            "general.architecture": "qwen35moe",
            "qwen35moe.context_length": 262144,
            "tokenizer.ggml.tokens": tokenizer_tokens,
        });

        let compact = inspection_metadata(&metadata);

        assert_eq!(compact["general.architecture"], "qwen35moe");
        assert_eq!(compact["qwen35moe.context_length"], 262144);
        assert_eq!(compact["tokenizer.ggml.tokens"]["items_total"], 100);
        assert_eq!(compact["tokenizer.ggml.tokens"]["truncated"], true);
        assert_eq!(
            compact["tokenizer.ggml.tokens"]["sample"]
                .as_array()
                .unwrap()
                .len(),
            INSPECTION_ARRAY_SAMPLE_ITEMS
        );
    }

    #[tokio::test]
    async fn discovers_gguf_files_and_exposes_facts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("publisher/model/tiny.gguf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_tiny_gguf(&path);
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: PathBuf::from("llama-server"),
            host: "127.0.0.1".into(),
            port: 0,
        });
        assert!(provider.capabilities().native_tools);
        assert!(provider.capabilities().constrained_tool_calls);
        let models = provider.discover_models().await.unwrap();
        assert_eq!(models[0].model_ref, "publisher/model/tiny.gguf");
        assert_eq!(models[0].context_limit, Some(262144));
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let facts = provider.model_facts(&deployment).await.unwrap();
        assert_eq!(
            facts.gguf_metadata.unwrap()["qwen35moe.attention.head_count"],
            16
        );
    }

    #[tokio::test]
    async fn inspection_does_not_claim_generation_is_unimplemented_before_a_probe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tiny.gguf");
        write_tiny_gguf(&path);
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: PathBuf::from("llama-server"),
            host: "127.0.0.1".into(),
            port: 0,
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };

        let inspection = provider.inspect(&deployment).await.unwrap();
        let Observation::Unknown { reason } =
            inspection.definition.capabilities["capability_probe"].clone()
        else {
            panic!("an unprobed deployment must remain unknown");
        };
        assert!(reason.contains("not run"), "{reason}");
        assert!(!reason.contains("not implemented"), "{reason}");
    }

    #[tokio::test]
    async fn plans_the_llama_server_process_without_starting_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("publisher/model/tiny.gguf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_tiny_gguf(&path);
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: PathBuf::from("/opt/llama.cpp/llama-server"),
            host: "127.0.0.1".into(),
            port: 49152,
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let plan = provider.server_plan(&deployment, 131_072).unwrap();
        assert_eq!(plan.server, PathBuf::from("/opt/llama.cpp/llama-server"));
        assert_eq!(plan.model, path);
        assert_eq!(
            plan.args()[0..2],
            ["--model".to_string(), path.display().to_string()]
        );
        assert_eq!(
            plan.endpoint(),
            "http://127.0.0.1:49152/v1/chat/completions"
        );
        assert!(plan.args().contains(&"--ctx-size".to_string()));
        assert!(plan.args().contains(&"131072".to_string()));
    }

    #[tokio::test]
    async fn reserves_a_concrete_loopback_port_when_the_config_asks_for_zero() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("publisher/model/tiny.gguf");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        write_tiny_gguf(&path);
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: PathBuf::from("llama-server"),
            host: "127.0.0.1".into(),
            port: 0,
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let plan = provider.server_plan(&deployment, 8192).unwrap();
        assert_ne!(plan.port, 0);
        assert!(plan.endpoint().starts_with("http://127.0.0.1:"));
        assert!(plan.args().contains(&plan.port.to_string()));
    }

    #[test]
    fn renders_chat_completions_body_for_llama_server() {
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let mut assistant = ChatMessage::text("assistant", "");
        assistant.tool_calls.push(ToolCall {
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "src/lib.rs"}),
            id: Some("call_1".into()),
        });
        let mut tool = ChatMessage::text("tool", "file contents");
        tool.tool_call_id = Some("call_1".into());
        let mut sampling = BTreeMap::new();
        sampling.insert("temperature".into(), serde_json::json!(0.2));
        sampling.insert("max_tokens".into(), serde_json::json!(1024));
        let body = chat_completions_body(&ModelRequest {
            deployment,
            messages: vec![
                ChatMessage::text("system", "stay concise"),
                ChatMessage::text("tool", "orphan harness note"),
                assistant,
                tool,
            ],
            context_tokens: 8192,
            tools: Some(serde_json::json!([{"type": "function"}])),
            seed: Some(7),
            sampling,
        });

        assert_eq!(body["model"], "publisher/model/tiny.gguf");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 1024);
        assert_eq!(body["temperature"], 0.2);
        assert_eq!(body["seed"], 7);
        assert_eq!(
            body["messages"][2]["tool_calls"][0]["function"]["arguments"],
            "{\"path\":\"src/lib.rs\"}"
        );
        assert_eq!(body["messages"][3]["role"], "tool");
        assert_eq!(body["messages"][3]["tool_call_id"], "call_1");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(
            body["messages"][1]["content"],
            "[PWR] orphan harness note"
        );
        assert!(body["messages"][1]["tool_call_id"].is_null());
        assert_eq!(body["tool_choice"], "required");
    }

    #[test]
    fn does_not_require_a_tool_when_none_were_offered() {
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let body = chat_completions_body(&ModelRequest {
            deployment,
            messages: vec![ChatMessage::text("user", "hello")],
            context_tokens: 8192,
            tools: None,
            seed: None,
            sampling: BTreeMap::new(),
        });
        assert!(body["tool_choice"].is_null());
    }

    #[test]
    fn decodes_openai_style_stream_text_and_terminal_usage() {
        let mut decoder = LlamaStreamDecoder::default();
        let first = decoder
            .decode_data(r#"{"choices":[{"delta":{"content":"hel"},"finish_reason":null}]}"#)
            .unwrap()
            .unwrap();
        assert_eq!(first.content, "hel");
        assert!(!first.done);
        let done = decoder
            .decode_data(
                r#"{"choices":[{"delta":{"content":"lo"},"finish_reason":"stop"}],
                   "usage":{"prompt_tokens":10,"completion_tokens":2}}"#,
            )
            .unwrap()
            .unwrap();
        assert_eq!(done.content, "lo");
        assert!(done.done);
        assert_eq!(done.metrics.unwrap().prompt_tokens, Some(10));
        assert!(decoder.decode_data("[DONE]").unwrap().is_none());
    }

    #[test]
    fn assembles_streamed_tool_calls_only_on_the_terminal_chunk() {
        let mut decoder = LlamaStreamDecoder::default();
        assert!(
            decoder
                .decode_data(
                    r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1",
                       "type":"function","function":{"name":"read_file","arguments":"{\"pa"}}]},
                       "finish_reason":null}]}"#,
                )
                .unwrap()
                .is_none()
        );
        assert!(
            decoder
                .decode_data(
                    r#"{"choices":[{"delta":{"tool_calls":[{"index":0,
                       "function":{"arguments":"th\":\"src/lib.rs\"}"}}]},
                       "finish_reason":null}]}"#,
                )
                .unwrap()
                .is_none()
        );
        let done = decoder
            .decode_data(r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#)
            .unwrap()
            .unwrap();
        assert!(done.done);
        assert_eq!(done.tool_calls.len(), 1);
        assert_eq!(done.tool_calls[0].name, "read_file");
        assert_eq!(done.tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(done.tool_calls[0].arguments["path"], "src/lib.rs");
    }

    #[test]
    fn refuses_non_terminal_finish_reasons_and_bad_tool_json() {
        let mut decoder = LlamaStreamDecoder::default();
        let truncated = decoder
            .decode_data(r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#)
            .unwrap_err();
        assert!(matches!(truncated, ProviderError::Truncated { .. }));

        let mut decoder = LlamaStreamDecoder::default();
        decoder
            .decode_data(
                r#"{"choices":[{"delta":{"tool_calls":[{"index":0,
                   "function":{"name":"read_file","arguments":"not-json"}}]},
                   "finish_reason":null}]}"#,
            )
            .unwrap();
        let failed = decoder
            .decode_data(r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#)
            .unwrap_err();
        assert!(matches!(failed, ProviderError::ModelOutput { .. }));
    }

    #[test]
    fn decodes_sse_chunks_split_across_transport_boundaries() {
        let mut decoder = LlamaSseDecoder::default();
        assert!(
            decoder
                .push("data: {\"choices\":[{\"delta\":{\"content\":\"he")
                .unwrap()
                .is_empty()
        );
        let chunks = decoder
            .push("y\"},\"finish_reason\":null}]}\r\n\r\n: keepalive\r\n")
            .unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "hey");
        assert!(!chunks[0].done);

        let done = decoder
            .push("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n")
            .unwrap();
        assert_eq!(done.len(), 1);
        assert!(done[0].done);
        assert!(decoder.push("data: [DONE]\n\n").unwrap().is_empty());
    }

    #[tokio::test]
    async fn posts_chat_requests_and_streams_sse_responses() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!(
            "http://{}/v1/chat/completions",
            listener.local_addr().unwrap()
        );
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let read = std::io::Read::read(&mut stream, &mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..read]);
            assert!(request.contains("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("\"stream\":true"));
            let body = concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"ok\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],",
                "\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1}}\n\n",
                "data: [DONE]\n\n"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let stream = post_chat_stream(
            &reqwest::Client::new(),
            &endpoint,
            ModelRequest {
                deployment,
                messages: vec![ChatMessage::text("user", "hi")],
                context_tokens: 8192,
                tools: None,
                seed: None,
                sampling: BTreeMap::new(),
            },
        )
        .await
        .unwrap();
        let reply = collect_reply(stream).await.unwrap();
        assert_eq!(reply.content, "ok");
        assert_eq!(reply.metrics.unwrap().generated_tokens, Some(1));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn starts_a_server_process_and_waits_for_readiness() {
        let script = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            script.path(),
            r#"#!/usr/bin/env python3
import http.server
import sys

host = sys.argv[sys.argv.index("--host") + 1]
port = int(sys.argv[sys.argv.index("--port") + 1])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
    def log_message(self, *_):
        pass

http.server.HTTPServer((host, port), Handler).serve_forever()
"#,
        )
        .unwrap();
        make_executable(script.path());
        let port = reserve_port("127.0.0.1").unwrap();
        let server = LlamaServer::start(
            LlamaServerPlan {
                server: script.path().to_path_buf(),
                model: PathBuf::from("model.gguf"),
                host: "127.0.0.1".into(),
                port,
                context_tokens: 8192,
            },
            &reqwest::Client::new(),
        )
        .await
        .unwrap();
        assert_eq!(
            server.endpoint(),
            format!("http://127.0.0.1:{port}/v1/chat/completions")
        );
    }

    #[tokio::test]
    async fn reports_a_server_that_exits_before_readiness() {
        let script = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            script.path(),
            "#!/usr/bin/env python3\nimport sys\nsys.exit(7)\n",
        )
        .unwrap();
        make_executable(script.path());
        let failed = LlamaServer::start(
            LlamaServerPlan {
                server: script.path().to_path_buf(),
                model: PathBuf::from("model.gguf"),
                host: "127.0.0.1".into(),
                port: reserve_port("127.0.0.1").unwrap(),
                context_tokens: 8192,
            },
            &reqwest::Client::new(),
        )
        .await
        .unwrap_err();
        assert!(matches!(failed, ProviderError::Unavailable { .. }));
    }

    #[tokio::test]
    async fn provider_chat_starts_the_server_and_streams_the_reply() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("publisher/model/tiny.gguf");
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        write_tiny_gguf(&model);
        let script = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            script.path(),
            r#"#!/usr/bin/env python3
import http.server
import sys

host = sys.argv[sys.argv.index("--host") + 1]
port = int(sys.argv[sys.argv.index("--port") + 1])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
    def do_POST(self):
        body = (
            'data: {"choices":[{"delta":{"content":"done"},"finish_reason":null}]}\n\n'
            'data: {"choices":[{"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":4,"completion_tokens":1}}\n\n'
            'data: [DONE]\n\n'
        )
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body.encode("utf-8"))
    def log_message(self, *_):
        pass

http.server.HTTPServer((host, port), Handler).serve_forever()
"#,
        )
        .unwrap();
        make_executable(script.path());
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: script.path().to_path_buf(),
            host: "127.0.0.1".into(),
            port: 0,
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let stream = provider
            .chat(ModelRequest {
                deployment,
                messages: vec![ChatMessage::text("user", "hi")],
                context_tokens: 8192,
                tools: None,
                seed: None,
                sampling: BTreeMap::new(),
            })
            .await
            .unwrap();
        let reply = collect_reply(stream).await.unwrap();
        assert_eq!(reply.content, "done");
        assert_eq!(reply.metrics.unwrap().prompt_tokens, Some(4));
    }

    #[tokio::test]
    async fn reads_the_llama_server_version() {
        let script = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            script.path(),
            "#!/usr/bin/env python3\nimport sys\nprint('llama-server 42.0')\n",
        )
        .unwrap();
        make_executable(script.path());
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: tempfile::tempdir().unwrap().path().to_path_buf(),
            server: script.path().to_path_buf(),
            host: "127.0.0.1".into(),
            port: 0,
        });
        assert_eq!(
            provider.backend_version().await.unwrap().as_deref(),
            Some("llama-server 42.0")
        );
    }

    #[tokio::test]
    async fn cancellable_chat_stops_an_in_flight_server_stream() {
        let dir = tempfile::tempdir().unwrap();
        let model = dir.path().join("publisher/model/tiny.gguf");
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        write_tiny_gguf(&model);
        let script = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            script.path(),
            r#"#!/usr/bin/env python3
import http.server
import sys
import time

host = sys.argv[sys.argv.index("--host") + 1]
port = int(sys.argv[sys.argv.index("--port") + 1])

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
    def do_POST(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.end_headers()
        self.wfile.write(b'data: {"choices":[{"delta":{"content":"partial"},"finish_reason":null}]}\n\n')
        self.wfile.flush()
        time.sleep(30)
    def log_message(self, *_):
        pass

http.server.HTTPServer((host, port), Handler).serve_forever()
"#,
        )
        .unwrap();
        make_executable(script.path());
        let provider = LlamaProvider::new(LlamaConfig {
            models_root: dir.path().to_path_buf(),
            server: script.path().to_path_buf(),
            host: "127.0.0.1".into(),
            port: 0,
        });
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "llama".into(),
            endpoint: "http://127.0.0.1:0/".into(),
            model_ref: "publisher/model/tiny.gguf".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let cancel = Cancel::new();
        let stream = provider
            .chat_cancellable(
                ModelRequest {
                    deployment,
                    messages: vec![ChatMessage::text("user", "hi")],
                    context_tokens: 8192,
                    tools: None,
                    seed: None,
                    sampling: BTreeMap::new(),
                },
                cancel.clone(),
            )
            .await
            .unwrap();
        let reply = tokio::spawn(async move { collect_reply(stream).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel.cancel();
        let failed = reply.await.unwrap().unwrap_err();
        assert!(matches!(failed, ProviderError::Cancelled));
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).unwrap();
    }

    #[cfg(not(unix))]
    fn make_executable(_path: &Path) {}

    fn write_tiny_gguf(path: &Path) {
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(b"GGUF").unwrap();
        file.write_all(&3_u32.to_le_bytes()).unwrap();
        file.write_all(&0_u64.to_le_bytes()).unwrap();
        file.write_all(&7_u64.to_le_bytes()).unwrap();
        kv_string(&mut file, "general.architecture", "qwen35moe");
        kv_u32(&mut file, "qwen35moe.block_count", 3);
        kv_u32(&mut file, "qwen35moe.context_length", 262144);
        kv_u32(&mut file, "qwen35moe.attention.head_count", 16);
        kv_array_u32(&mut file, "qwen35moe.attention.head_count_kv", &[2, 0, 2]);
        kv_u32(&mut file, "qwen35moe.attention.key_length", 256);
        kv_u32(&mut file, "qwen35moe.attention.value_length", 256);
    }

    fn key(file: &mut std::fs::File, name: &str, kind: u32) {
        file.write_all(&(name.len() as u64).to_le_bytes()).unwrap();
        file.write_all(name.as_bytes()).unwrap();
        file.write_all(&kind.to_le_bytes()).unwrap();
    }

    fn kv_string(file: &mut std::fs::File, name: &str, value: &str) {
        key(file, name, 8);
        file.write_all(&(value.len() as u64).to_le_bytes()).unwrap();
        file.write_all(value.as_bytes()).unwrap();
    }

    fn kv_u32(file: &mut std::fs::File, name: &str, value: u32) {
        key(file, name, 4);
        file.write_all(&value.to_le_bytes()).unwrap();
    }

    fn kv_array_u32(file: &mut std::fs::File, name: &str, values: &[u32]) {
        key(file, name, 9);
        file.write_all(&4_u32.to_le_bytes()).unwrap();
        file.write_all(&(values.len() as u64).to_le_bytes())
            .unwrap();
        for value in values {
            file.write_all(&value.to_le_bytes()).unwrap();
        }
    }
}
