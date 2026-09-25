//! The embedded MLX engine: a model PWR runs itself, not one it asks a
//! server for.
//!
//! Roadmap step 4. The case for it is control (backlog A.17): a model like
//! Qwen3.6 switches its reasoning in its chat template, only a caller that
//! renders that template controls it, and LM Studio renders it for PWR and
//! silently ignores the request to switch it off. Runaway reasoning then
//! confounded every measurement taken through LM Studio.
//!
//! The generation itself runs in a Python sidecar over `mlx-lm`
//! (`sidecar/pwr_mlx.py`), because `mlx-lm` is the reference implementation
//! of the architectures and reimplementing them over MLX's C API would trail
//! every new model. This crate owns the sidecar's lifecycle and speaks its
//! JSON-lines protocol. The sidecar returns calls as the model wrote them; this
//! crate reads them with PWR's own family adapter before handing the reply
//! on, so a call arrives structured, as it does from LM Studio, whose server
//! parses them. Everything downstream -- the capability probe, both loops --
//! then sees the same shape from every backend, and the call's text does not
//! also land in the conversation as prose.
//!
//! Configuration, all optional:
//! - `PWR_MLX_PYTHON`: the interpreter with `mlx-lm` installed (default
//!   `python3`);
//! - `PWR_MLX_SIDECAR`: the sidecar script (default: the one in this crate);
//! - `PWR_MLX_MODELS`: where a model named by a relative reference is looked
//!   for (default `~/.pwr/models`, where the Model Manager downloads them).

pub mod embed;

use async_trait::async_trait;
use chrono::Utc;
use futures_util::stream;
use pwr_domain::{
    BackendState, ChatMessage, DeploymentDescriptor, GenerationMetrics, ModelChunk,
    ModelDefinition, ModelInspection, ModelRequest, Observation, Provenance, new_id,
};
use pwr_provider::{
    BackendCapabilities, DiscoveredModel, InferenceBackend, ModelFacts, ModelProvider, ModelStream,
    ProviderError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

/// The completion cap when a request names none. A cap is what turns a
/// runaway generation into a bounded, reported fault rather than a turn that
/// never ends.
pub const DEFAULT_MAX_TOKENS: u64 = 16_384;

/// Sampling fields the MLX wire protocol and sidecar actually apply. Values
/// from other generation-config fields are never presented as effective.
pub const MLX_SAMPLING_FIELDS: [&str; 6] = [
    "temperature",
    "top_p",
    "top_k",
    "min_p",
    "presence_penalty",
    "repetition_penalty",
];

/// Resolve a request against the artifact's structured generation settings.
/// Explicit request values (including a packaged profile) win. The private
/// provenance entry is consumed by the audit; `chat_body` never sends it as a
/// sampler option.
pub fn resolve_generation_sampling(
    sampling: &mut BTreeMap<String, serde_json::Value>,
    generation_config: Option<&serde_json::Value>,
) -> Result<(), ProviderError> {
    // Older packaged profiles call this repeat_penalty. Normalize it before
    // resolution so the audit and the sidecar describe the same parameter.
    if let Some(legacy) = sampling.remove("repeat_penalty") {
        if let Some(current) = sampling.get("repetition_penalty") {
            if current != &legacy {
                return Err(ProviderError::Protocol {
                    safe_context: "conflicting repetition penalty values".into(),
                });
            }
        } else {
            sampling.insert("repetition_penalty".into(), legacy);
        }
    }
    let mut sources = sampling
        .get("_pwr_sampling_sources")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();
    if let Some(legacy) = sources.remove("repeat_penalty") {
        sources.entry("repetition_penalty").or_insert(legacy);
    }
    for name in MLX_SAMPLING_FIELDS {
        if let Some(value) = sampling.get(name) {
            validate_sampling(name, value)?;
            sources
                .entry(name)
                .or_insert_with(|| serde_json::json!("request"));
            continue;
        }
        // A generation config with sampling disabled asks for greedy decoding
        // even when it carries dormant temperature/top-p values. An explicit
        // PWR profile still wins over that artifact instruction.
        if name == "temperature"
            && generation_config.and_then(|config| config.get("do_sample"))
                == Some(&serde_json::Value::Bool(false))
        {
            sampling.insert(name.into(), serde_json::json!(0.0));
            sources.insert(name.into(), serde_json::json!("artifact_do_sample_false"));
        } else if let Some(value) = generation_config
            .and_then(|config| config.get(name))
            .filter(|value| !value.is_null())
        {
            validate_sampling(name, value)?;
            sampling.insert(name.into(), value.clone());
            sources.insert(name.into(), serde_json::json!("artifact_generation_config"));
        } else if !matches!(name, "presence_penalty" | "repetition_penalty") {
            let fallback = if name == "top_k" {
                serde_json::json!(0)
            } else {
                serde_json::json!(0.0)
            };
            sampling.insert(name.into(), fallback);
            sources.insert(name.into(), serde_json::json!("mlx_sidecar_default"));
        }
    }
    sampling.insert(
        "_pwr_sampling_sources".into(),
        serde_json::Value::Object(sources),
    );
    Ok(())
}

pub fn validate_sampling(name: &str, value: &serde_json::Value) -> Result<(), ProviderError> {
    let valid = match name {
        "temperature" => value.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0),
        "top_p" | "min_p" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && (0.0..=1.0).contains(&n)),
        "top_k" => value.as_u64().is_some_and(|n| n <= i32::MAX as u64),
        "presence_penalty" => value.as_f64().is_some_and(f64::is_finite),
        "repetition_penalty" => value.as_f64().is_some_and(|n| n.is_finite() && n > 0.0),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ProviderError::Protocol {
            safe_context: format!("invalid MLX sampling value for {name}"),
        })
    }
}

fn generation_config(dir: &Path) -> Result<Option<serde_json::Value>, ProviderError> {
    let path = dir.join("generation_config.json");
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ProviderError::Protocol {
                safe_context: format!("cannot read generation_config.json: {error}"),
            });
        }
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::Protocol {
            safe_context: "generation_config.json is not valid JSON".into(),
        })?;
    if !value.is_object() {
        return Err(ProviderError::Protocol {
            safe_context: "generation_config.json must be an object".into(),
        });
    }
    Ok(Some(value))
}

/// Where the sidecar and its models are.
#[derive(Debug, Clone)]
pub struct MlxConfig {
    pub python: PathBuf,
    pub sidecar: PathBuf,
    pub models_root: PathBuf,
}

impl MlxConfig {
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        MlxConfig {
            // Without an override, the engine the desktop app installed is
            // preferred over the system `python3`, which has no MLX: a `pwr`
            // run from a shell otherwise saw the sidecar exit on its first
            // request and every capability probe came back `unknown`.
            python: std::env::var_os("PWR_MLX_PYTHON")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    let installed = home
                        .join("Library/Application Support/ai.pwr.desktop/engine/venv/bin/python");
                    if installed.is_file() {
                        installed
                    } else {
                        PathBuf::from("python3")
                    }
                }),
            sidecar: std::env::var_os("PWR_MLX_SIDECAR")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("sidecar/pwr_mlx.py")
                }),
            models_root: std::env::var_os("PWR_MLX_MODELS")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".pwr/models")),
        }
    }

    /// The model directory a reference names: a path, or a directory below
    /// the models root such as `lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit`.
    /// Only a directory holding a `config.json` is a model.
    pub fn model_dir(&self, model_ref: &str) -> Result<PathBuf, ProviderError> {
        let expanded = match model_ref.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|| PathBuf::from(model_ref)),
            None => PathBuf::from(model_ref),
        };
        // A relative reference names a folder below the models root and
        // cannot climb out of it; a path is named as a path.
        if !expanded.is_absolute()
            && expanded
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        {
            return Err(ProviderError::Protocol {
                safe_context: format!("{model_ref} leaves the models folder"),
            });
        }
        let candidate = if expanded.is_absolute() {
            expanded
        } else {
            self.models_root.join(expanded)
        };
        if candidate.join("config.json").is_file() {
            Ok(candidate)
        } else {
            Err(ProviderError::Protocol {
                safe_context: format!(
                    "{model_ref} is not an MLX model directory: no config.json at {}",
                    candidate.display()
                ),
            })
        }
    }
}

impl MlxConfig {
    /// Whether a model carries a vision encoder, read from its own
    /// `config.json` rather than from a list (backlog C.25).
    ///
    /// Checked on the maintainer's host 2026-09-23: Qwen3.6-35B-A3B and
    /// Qwen3.8-27B declare a `vision_config`; Qwen3-14B, GLM-4.7-Flash,
    /// Seed-OSS, Nemotron 3.5 and gpt-oss do not. `None` when the model
    /// cannot be found or read, which is not the same answer as `false`.
    ///
    /// The sidecar loads such a model through mlx-vlm when its interpreter has
    /// it (`setup-mlx.sh` installs it), and then accepts images; with only
    /// mlx-lm it serves the same model as text and refuses an image.
    pub fn has_vision_encoder(&self, model_ref: &str) -> Option<bool> {
        let dir = self.model_dir(model_ref).ok()?;
        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("config.json")).ok()?).ok()?;
        let declared = config
            .get("vision_config")
            .is_some_and(|value| !value.is_null());
        let vision_type = config
            .get("model_type")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|kind| kind.contains("_vl") || kind.ends_with("vl"));
        Some(declared || vision_type)
    }
}

struct Sidecar {
    child: Child,
    /// Shared, so a cancel can be written while a reply holds the sidecar.
    stdin: Arc<Mutex<ChildStdin>>,
    lines: Lines<BufReader<ChildStdout>>,
    loaded: Option<PathBuf>,
    next_id: u64,
    /// A request whose reply was not read to its end, because its stream was
    /// dropped. Its remaining lines are drained before the next request.
    pending: Option<u64>,
}

impl Sidecar {
    async fn send(&mut self, request: serde_json::Value) -> Result<(), ProviderError> {
        write_line(&self.stdin, request).await
    }

    async fn event(&mut self) -> Result<serde_json::Value, ProviderError> {
        let line = self
            .lines
            .next_line()
            .await
            .map_err(|error| unavailable(format!("the MLX sidecar's output failed: {error}")))?
            .ok_or_else(|| unavailable("the MLX sidecar exited".into()))?;
        serde_json::from_str(&line).map_err(|_| ProviderError::Protocol {
            safe_context: "the MLX sidecar wrote a line that is not JSON".into(),
        })
    }

    async fn drain(&mut self) -> Result<(), ProviderError> {
        let Some(pending) = self.pending.take() else {
            return Ok(());
        };
        loop {
            let event = self.event().await?;
            let finished = matches!(event["event"].as_str(), Some("done" | "error"));
            if finished && event["id"].as_u64() == Some(pending) {
                return Ok(());
            }
        }
    }

    async fn request(&mut self, mut body: serde_json::Value) -> Result<u64, ProviderError> {
        self.drain().await?;
        self.next_id += 1;
        let id = self.next_id;
        body["id"] = serde_json::json!(id);
        self.send(body).await?;
        Ok(id)
    }
}

async fn write_line(
    stdin: &Arc<Mutex<ChildStdin>>,
    request: serde_json::Value,
) -> Result<(), ProviderError> {
    let mut line = request.to_string();
    line.push('\n');
    let mut stdin = stdin.lock().await;
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|error| unavailable(format!("the MLX sidecar stopped reading: {error}")))?;
    stdin
        .flush()
        .await
        .map_err(|error| unavailable(format!("the MLX sidecar stopped reading: {error}")))
}

fn unavailable(safe_context: String) -> ProviderError {
    ProviderError::Unavailable { safe_context }
}

/// PWR's own MLX backend.
pub struct MlxProvider {
    config: MlxConfig,
    sidecar: Arc<Mutex<Option<Sidecar>>>,
    version: Arc<tokio::sync::OnceCell<Option<String>>>,
}

/// The one sidecar this process runs, whichever provider value is asking.
///
/// PWR builds a fresh backend value wherever it needs one; for an HTTP
/// client that costs nothing, but here each would start its own sidecar and
/// load its own twenty-gigabyte copy of the model. So the sidecar belongs to
/// the process, not to the value. It never outlives PWR: it reads its
/// requests from PWR's pipe, and exits when that pipe closes.
fn shared_sidecar() -> Arc<Mutex<Option<Sidecar>>> {
    static SIDECAR: std::sync::OnceLock<Arc<Mutex<Option<Sidecar>>>> = std::sync::OnceLock::new();
    SIDECAR.get_or_init(|| Arc::new(Mutex::new(None))).clone()
}

impl MlxProvider {
    pub fn new(config: MlxConfig) -> Self {
        MlxProvider {
            config,
            sidecar: shared_sidecar(),
            version: Arc::new(tokio::sync::OnceCell::new()),
        }
    }

    /// Sends one chat request and streams its reply. Also returns what a
    /// cancel needs: the request id, the sidecar's input, and a signal that
    /// resolves when the stream ends or is dropped.
    async fn reply(
        &self,
        mut request: ModelRequest,
    ) -> Result<
        (
            ModelStream,
            u64,
            Arc<Mutex<ChildStdin>>,
            tokio::sync::oneshot::Receiver<()>,
        ),
        ProviderError,
    > {
        let dir = self.config.model_dir(&request.deployment.model_ref)?;
        resolve_generation_sampling(&mut request.sampling, generation_config(&dir)?.as_ref())?;
        let family = Self::read_config(&dir)
            .and_then(|config| config["model_type"].as_str().map(str::to_owned));
        let adapter: Arc<dyn pwr_compat::ModelBehaviorAdapter> = Arc::from(
            pwr_compat::adapter_for(family.as_deref(), &request.deployment.model_ref),
        );
        let mut guard = self.sidecar.clone().lock_owned().await;
        if guard
            .as_ref()
            .is_some_and(|sidecar| sidecar.pending.is_some())
        {
            match tokio::time::timeout(
                Self::ENGINE_SILENCE,
                guard.as_mut().expect("pending sidecar").drain(),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    *guard = None;
                    return Err(error);
                }
                Err(_) => {
                    *guard = None;
                    return Err(unavailable(
                        "the previous MLX reply did not finish draining; the engine was restarted"
                            .into(),
                    ));
                }
            }
        }
        self.ensure_loaded(&mut guard, &dir).await?;
        let requested = tokio::time::timeout(
            Self::ENGINE_SILENCE,
            guard.as_mut().expect("loaded").request(chat_body(&request)),
        )
        .await;
        let id = match requested {
            Ok(Ok(id)) => id,
            Ok(Err(error)) => return Err(error),
            Err(_) => {
                *guard = None;
                return Err(unavailable(
                    "the previous MLX reply did not finish draining; the engine was restarted"
                        .into(),
                ));
            }
        };
        let sidecar = guard.as_mut().expect("loaded");
        sidecar.pending = Some(id);
        let stdin = sidecar.stdin.clone();
        let (ended_tx, ended_rx) = tokio::sync::oneshot::channel::<()>();
        let live = Live {
            answer: String::new(),
            sent_thinking: 0,
            sent_text: 0,
        };
        // The lock travels with the stream, so a second request waits for this
        // reply rather than interleaving with it. A stream dropped early leaves
        // `pending` set, and the next request drains the rest first; the
        // sender dropped with it is what ends a cancel watcher.
        let replies = stream::unfold(
            Some((
                guard,
                live,
                ended_tx,
                std::time::Instant::now(),
                std::time::Instant::now(),
            )),
            move |state| {
                let adapter = adapter.clone();
                async move {
                    let (mut guard, mut live, ended, mut last_event, mut last_chunk) = state?;
                    loop {
                        let sidecar = guard.as_mut().expect("loaded");
                        // Every stretch of a reply produces an event: prefill
                        // progress, then a delta per token. Silence this long is
                        // an engine stuck, not a slow one -- measured 2026-09-22,
                        // a sidecar blocked in `mlx::core::eval` for thirty
                        // minutes while the turn waited without a bound, because
                        // the reply had not produced its first chunk.
                        let remaining = Self::ENGINE_SILENCE.saturating_sub(last_event.elapsed());
                        if remaining.is_zero() {
                            *guard = None;
                            return Some((
                                Err(unavailable(
                                    "the MLX engine stopped reporting progress and was restarted"
                                        .into(),
                                )),
                                None,
                            ));
                        }
                        let wait = Self::PROGRESS_INTERVAL
                            .saturating_sub(last_chunk.elapsed())
                            .min(remaining);
                        let event = match tokio::time::timeout(wait, sidecar.event()).await {
                            Ok(Ok(event)) => event,
                            Ok(Err(error)) => return Some((Err(error), None)),
                            Err(_) => {
                                // A content-free chunk keeps the reply collector
                                // informed without exposing partial tool arguments.
                                last_chunk = std::time::Instant::now();
                                return Some((
                                    Ok(ModelChunk::default()),
                                    Some((guard, live, ended, last_event, last_chunk)),
                                ));
                            }
                        };
                        last_event = std::time::Instant::now();
                        if event["id"].as_u64() != Some(id) {
                            continue;
                        }
                        match step_of(&event, &live.answer, adapter.as_ref()) {
                            Step::Yield(chunk) => {
                                return Some((
                                    Ok(chunk),
                                    Some((
                                        guard,
                                        live,
                                        ended,
                                        last_event,
                                        std::time::Instant::now(),
                                    )),
                                ));
                            }
                            Step::Answer(text) => {
                                live.answer.push_str(&text);
                                if let Some(chunk) = live.advance() {
                                    return Some((
                                        Ok(chunk),
                                        Some((
                                            guard,
                                            live,
                                            ended,
                                            last_event,
                                            std::time::Instant::now(),
                                        )),
                                    ));
                                }
                            }
                            Step::Finish(item) => {
                                sidecar.pending = None;
                                return Some((item.map(|chunk| live.finish(chunk)), None));
                            }
                            Step::Skip => {}
                        }
                        if last_chunk.elapsed() >= Self::PROGRESS_INTERVAL {
                            last_chunk = std::time::Instant::now();
                            return Some((
                                Ok(ModelChunk::default()),
                                Some((guard, live, ended, last_event, last_chunk)),
                            ));
                        }
                    }
                }
            },
        );
        Ok((Box::pin(replies), id, stdin, ended_rx))
    }

    /// How long a reply in progress may go without a single event from the engine.
    /// Prefill reports progress per step (seconds each, at the widest windows
    /// measured) and generation a delta per token, so this is generous.
    const ENGINE_SILENCE: std::time::Duration = std::time::Duration::from_secs(300);
    const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

    /// Starts the sidecar if it is not running.
    fn ensure_started(&self, slot: &mut Option<Sidecar>) -> Result<(), ProviderError> {
        if slot.is_none() {
            let mut child = tokio::process::Command::new(&self.config.python)
                .arg(&self.config.sidecar)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|error| {
                    unavailable(format!(
                        "could not start the MLX sidecar with {}: {error}; set PWR_MLX_PYTHON \
                         to an interpreter with mlx-lm installed",
                        self.config.python.display()
                    ))
                })?;
            let stdin = Arc::new(Mutex::new(child.stdin.take().expect("piped stdin")));
            let stdout = child.stdout.take().expect("piped stdout");
            *slot = Some(Sidecar {
                child,
                stdin,
                lines: BufReader::new(stdout).lines(),
                loaded: None,
                next_id: 0,
                pending: None,
            });
        }
        Ok(())
    }

    /// What a prefill step of this model materialises in attention scores, as
    /// the engine answers it for the model's head dimension.
    async fn prefill_scores_bytes(&self, config: &serde_json::Value) -> Option<u64> {
        let text = config.get("text_config").unwrap_or(config);
        // Latent attention attends over the latent: its width is the head.
        let head_dim = text["kv_lora_rank"]
            .as_u64()
            .or_else(|| text["head_dim"].as_u64())
            .or_else(|| {
                Some(text["hidden_size"].as_u64()? / text["num_attention_heads"].as_u64()?)
            })?;
        let mut slot = self.sidecar.lock().await;
        self.ensure_started(&mut slot).ok()?;
        let sidecar = slot.as_mut()?;
        let id = sidecar
            .request(serde_json::json!({"op": "attention", "head_dim": head_dim}))
            .await
            .ok()?;
        loop {
            let event = sidecar.event().await.ok()?;
            if event["id"].as_u64() != Some(id) {
                continue;
            }
            return event["scores_bytes"].as_u64();
        }
    }

    /// Starts the sidecar if it is not running and loads `dir` if it is not the
    /// model loaded. One model at a time: loading another replaces it.
    async fn ensure_loaded(
        &self,
        slot: &mut Option<Sidecar>,
        dir: &Path,
    ) -> Result<(), ProviderError> {
        self.ensure_started(slot)?;
        let sidecar = slot.as_mut().expect("just started");
        if sidecar.loaded.as_deref() == Some(dir) {
            return Ok(());
        }
        let id = sidecar
            .request(serde_json::json!({"op": "load", "path": dir}))
            .await?;
        loop {
            let event = sidecar.event().await?;
            if event["id"].as_u64() != Some(id) {
                continue;
            }
            match event["event"].as_str() {
                Some("loaded") => {
                    sidecar.loaded = Some(dir.to_path_buf());
                    return Ok(());
                }
                _ => {
                    return Err(ProviderError::Protocol {
                        safe_context: format!(
                            "the MLX sidecar could not load {}: {}",
                            dir.display(),
                            event["message"].as_str().unwrap_or("no reason given")
                        ),
                    });
                }
            }
        }
    }

    fn read_config(dir: &Path) -> Option<serde_json::Value> {
        serde_json::from_slice(&std::fs::read(dir.join("config.json")).ok()?).ok()
    }

    fn weights_bytes(dir: &Path) -> Option<u64> {
        let total: u64 = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "safetensors")
            })
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| metadata.len())
            .sum();
        (total > 0).then_some(total)
    }

    fn model_weights_complete(dir: &Path) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return false;
        };
        let files: Vec<_> = entries.flatten().collect();
        if files.iter().any(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "part")
        }) {
            return false;
        }
        let index = dir.join("model.safetensors.index.json");
        if index.exists() {
            let Ok(bytes) = std::fs::read(index) else {
                return false;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                return false;
            };
            let Some(weight_map) = value
                .get("weight_map")
                .and_then(serde_json::Value::as_object)
            else {
                return false;
            };
            let required: BTreeSet<&str> = weight_map
                .values()
                .filter_map(serde_json::Value::as_str)
                .collect();
            if required.is_empty() {
                return false;
            }
            return required.into_iter().all(|name| {
                let path = Path::new(name);
                path.components().count() == 1
                    && path.file_name().and_then(|part| part.to_str()) == Some(name)
                    && path
                        .extension()
                        .is_some_and(|extension| extension == "safetensors")
                    && std::fs::metadata(dir.join(path))
                        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
            });
        }

        let mut weights = false;
        let mut shards: BTreeMap<(String, usize), BTreeSet<usize>> = BTreeMap::new();
        for entry in files {
            let path = entry.path();
            if path
                .extension()
                .is_none_or(|extension| extension != "safetensors")
            {
                continue;
            }
            if !entry
                .metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
            {
                return false;
            }
            weights = true;
            if let Some((name, number, total)) = safetensors_shard(&path) {
                shards.entry((name, total)).or_default().insert(number);
            }
        }
        weights
            && shards
                .iter()
                .all(|((_, total), found)| found.len() == *total)
    }
}

fn safetensors_shard(path: &Path) -> Option<(String, usize, usize)> {
    let stem = path.file_name()?.to_str()?.strip_suffix(".safetensors")?;
    let (prefix, total) = stem.rsplit_once("-of-")?;
    let (name, number) = prefix.rsplit_once('-')?;
    if name.is_empty()
        || number.len() != 5
        || total.len() != 5
        || !number.bytes().all(|byte| byte.is_ascii_digit())
        || !total.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let number: usize = number.parse().ok()?;
    let total: usize = total.parse().ok()?;
    (number > 0 && total > 0 && number <= total).then(|| (name.to_owned(), number, total))
}

/// A message in the shape Qwen-style chat templates read.
///
/// The arguments of a call stay an object, not an encoded string: the template
/// writes each one as its own `<parameter=...>` block by iterating the object,
/// and a string would render as one parameter holding the whole call.
pub fn template_message(message: &ChatMessage) -> serde_json::Value {
    let mut value = serde_json::json!({"role": message.role, "content": message.content});
    let object = value.as_object_mut().expect("literal object");
    if !message.images.is_empty() {
        // The images first, then the text, as the sidecar takes them.
        let mut parts: Vec<serde_json::Value> = message
            .images
            .iter()
            .map(|path| serde_json::json!({"type": "image", "path": path}))
            .collect();
        parts.push(serde_json::json!({"type": "text", "text": message.content}));
        object.insert("content".into(), serde_json::Value::Array(parts));
    }
    // The template decides whether to render it: Qwen 3.x's shows an
    // assistant step's reasoning back to the model (with `preserve_thinking`,
    // on every turn), which is how it was trained on multi-step tool use. The
    // harness keeps it on the step it belongs to for the whole conversation:
    // dropping it when the person wrote again changed the history part way,
    // and a prompt cache that cannot be cut back (Qwen 3.5/3.6) then prefilled
    // the whole conversation for every message.
    if message.role == "assistant"
        && let Some(reasoning) = &message.reasoning
    {
        object.insert("reasoning_content".into(), serde_json::json!(reasoning));
    }
    if !message.tool_calls.is_empty() {
        object.insert(
            "tool_calls".into(),
            serde_json::Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|call| {
                        serde_json::json!({
                            "type": "function",
                            "function": {"name": call.name, "arguments": call.arguments}
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

/// The conversation as chat templates take it.
///
/// PWR also uses the `tool` role for its own notes -- the checks' first
/// verdict, a refusal -- which answer no call. Qwen's template tolerated them;
/// gpt-oss's refuses the whole request ("Message has tool role, but there was
/// no previous assistant message with a tool call!", suite A4, 2026-09-19).
/// A tool message that does not follow an assistant turn with calls is sent
/// as a user message marked as PWR's.
fn template_messages(messages: &[ChatMessage]) -> Vec<serde_json::Value> {
    let mut answering = false;
    messages
        .iter()
        .map(|message| {
            let mut value = template_message(message);
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

/// The chat template an MLX model ships, as text, never rendered here:
/// `chat_template.jinja` (newer exports, and what the tokenizer prefers when
/// present), `chat_template.json`, or `tokenizer_config.json`'s
/// `chat_template` -- a string, or a list of named templates, all of which are
/// kept because the tokenizer picks among them by request (`tool_use`).
pub fn chat_template_text(dir: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(dir.join("chat_template.jinja")) {
        return Some(text);
    }
    let from_json = |value: &serde_json::Value| match value {
        serde_json::Value::String(text) => Some(text.clone()),
        serde_json::Value::Array(named) => Some(
            named
                .iter()
                .filter_map(|entry| entry["template"].as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    };
    for (file, key) in [
        ("chat_template.json", "chat_template"),
        ("tokenizer_config.json", "chat_template"),
    ] {
        let Ok(bytes) = std::fs::read(dir.join(file)) else {
            continue;
        };
        if let Some(text) = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|value| value.get(key).and_then(from_json))
        {
            return Some(text);
        }
    }
    None
}

/// Names and sizes of the weight files, hashed: changes when a weight file is
/// replaced by one of another size, which the config and index alone need not
/// show. Cheap -- no weight byte is read.
fn weights_fingerprint(dir: &Path) -> Option<String> {
    let mut files: Vec<(String, u64)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "safetensors")
        })
        .filter_map(|entry| {
            Some((
                entry.file_name().to_string_lossy().into_owned(),
                entry.metadata().ok()?.len(),
            ))
        })
        .collect();
    if files.is_empty() {
        return None;
    }
    files.sort();
    let listing: String = files
        .iter()
        .map(|(name, size)| format!("{name}:{size}\n"))
        .collect();
    Some(pwr_domain::hash_bytes(listing.as_bytes()))
}

/// The sidecar request for one model request.
///
/// `think` is the reasoning switch a model profile declares
/// (`ReasoningControl::Think`); absent, the template's own default applies.
pub fn chat_body(request: &ModelRequest) -> serde_json::Value {
    let sampling = &request.sampling;
    let number = |name: &str| {
        sampling
            .get(name)
            .cloned()
            .unwrap_or(serde_json::Value::Null)
    };
    serde_json::json!({
        "op": "chat",
        "messages": template_messages(&request.messages),
        "tools": request.tools,
        "thinking": sampling.get("think").cloned().unwrap_or(serde_json::Value::Null),
        "reasoning_budget": number("reasoning_budget"),
        "reasoning_effort": number("reasoning_effort"),
        "max_tokens": sampling
            .get("max_tokens")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(DEFAULT_MAX_TOKENS),
        "temperature": number("temperature"),
        "top_p": number("top_p"),
        "top_k": number("top_k"),
        "min_p": number("min_p"),
        "presence_penalty": number("presence_penalty"),
        "repetition_penalty": number("repetition_penalty"),
        "seed": request.seed,
    })
}

/// What the reply stream does with one sidecar event.
/// Where the part of an answer that can be shown ends: the first place a
/// call may begin. Everything from there on is held until the reply ends and
/// the family adapter has taken the calls out.
const CALL_MARKERS: [&str; 5] = [
    "<tool_call>",
    "<seed:tool_call>",
    "<function=",
    "<think>",
    "<seed:think>",
];

/// Characters held back at the end of what is shown, so a marker arriving
/// split across tokens is never half-shown and then taken back. Longer than
/// every marker, harmony's included.
const HOLD_BACK: usize = 16;

/// What of an answer so far can be shown: (reasoning, answer text).
///
/// For most families, the text before the first call marker. For harmony
/// (gpt-oss), the bodies of its `analysis` messages as reasoning and of its
/// `final` or unaddressed messages as text, up to the first message addressed
/// to a tool. Streamed so a reply is seen as it is written: the capability
/// probes of 2026-09-19 found the answer held whole, one chunk per reply, which
/// is no streaming at all and leaves nothing to cancel.
/// Which part of `visible`'s view may still grow at its end, and so is the
/// only one whose tail is held back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Open {
    Thinking,
    Text,
    Neither,
}

fn visible(answer: &str) -> (String, String, Open) {
    const CHANNEL: &str = "<|channel|>";
    if !answer.trim_start().starts_with(CHANNEL) {
        // A reply that opens with its own think block (Seed-OSS with its
        // budget unset writes `<seed:think>` itself): the block is reasoning,
        // and the text after it is the answer.
        let lead = answer.trim_start();
        for (open_tag, close_tag) in [("<think>", "</think>"), ("<seed:think>", "</seed:think>")] {
            let Some(body) = lead.strip_prefix(open_tag) else {
                continue;
            };
            let Some(close) = body.find(close_tag) else {
                return (body.to_owned(), String::new(), Open::Thinking);
            };
            let rest = &body[close + close_tag.len()..];
            let (_, text, open) = visible(rest);
            return (body[..close].to_owned(), text, open);
        }
        let cut = CALL_MARKERS
            .iter()
            .filter_map(|marker| answer.find(marker))
            .min();
        let open = if cut.is_some() {
            Open::Neither
        } else {
            Open::Text
        };
        let cut = cut.unwrap_or(answer.len());
        return (String::new(), answer[..cut].to_owned(), open);
    }
    let (mut thinking, mut text) = (String::new(), String::new());
    let mut open = Open::Neither;
    for segment in answer.split(CHANNEL).skip(1) {
        let Some((header, body)) = segment.split_once("<|message|>") else {
            break;
        };
        if header.contains("to=") {
            break;
        }
        let end = ["<|end|>", "<|call|>", "<|return|>", "<|start|>"]
            .iter()
            .filter_map(|marker| body.find(marker))
            .min();
        let body = end.map_or(body, |end| &body[..end]);
        let analysis = header.trim_start().starts_with("analysis");
        open = match (end, analysis) {
            (Some(_), _) => Open::Neither,
            (None, true) => Open::Thinking,
            (None, false) => Open::Text,
        };
        let target = if analysis { &mut thinking } else { &mut text };
        if !target.is_empty() {
            target.push('\n');
        }
        target.push_str(body);
    }
    (thinking, text, open)
}

/// `text[sent..]`, less the held-back tail, on a character boundary.
fn fresh(text: &str, sent: usize, hold: usize) -> &str {
    let mut end = text.len().saturating_sub(hold);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    if end <= sent {
        return "";
    }
    &text[sent..end]
}

/// What a reply stream has gathered and already handed on.
struct Live {
    answer: String,
    sent_thinking: usize,
    sent_text: usize,
}

impl Live {
    /// The next chunk of shown text, if the answer grew into any.
    fn advance(&mut self) -> Option<ModelChunk> {
        let (thinking, text, open) = visible(&self.answer);
        let hold = |part: Open| if open == part { HOLD_BACK } else { 0 };
        let new_thinking = fresh(&thinking, self.sent_thinking, hold(Open::Thinking)).to_owned();
        let new_text = fresh(&text, self.sent_text, hold(Open::Text)).to_owned();
        if new_thinking.is_empty() && new_text.is_empty() {
            return None;
        }
        self.sent_thinking += new_thinking.len();
        self.sent_text += new_text.len();
        Some(ModelChunk {
            content: new_text,
            thinking: (!new_thinking.is_empty()).then_some(new_thinking),
            tool_calls: Vec::new(),
            metrics: None,
            done: false,
        })
    }

    /// The terminal chunk less what was already shown.
    fn finish(&self, mut chunk: ModelChunk) -> ModelChunk {
        let (thinking, text, _) = visible(&self.answer);
        // Compared without leading whitespace: the adapter's reading of the
        // whole reply trims it, and what was shown live need not be. Seen
        // 2026-09-23 (Qwen3-14B, "Ciao"): the model wrote its own `<think>`
        // block, the live text began "\n\nCiao! How can I a", the adapter's
        // began "Ciao!", no prefix matched, and the rest of the answer was
        // dropped -- the reply ended mid-word.
        let shown = text[..self.sent_text.min(text.len())].trim_start();
        let whole = chunk.content.trim_start();
        chunk.content = whole
            .strip_prefix(shown)
            .or_else(|| whole.strip_prefix(shown.trim_end()))
            .map(str::to_owned)
            .unwrap_or_else(|| {
                if shown.is_empty() {
                    chunk.content.clone()
                } else {
                    String::new()
                }
            });
        if self.sent_thinking > 0 {
            let rest = &thinking[self.sent_thinking.min(thinking.len())..];
            chunk.thinking = (!rest.is_empty()).then(|| rest.to_owned());
        }
        chunk
    }
}

enum Step {
    /// Hand this item on and keep reading.
    Yield(ModelChunk),
    /// Answer text: kept, not handed on, until the calls in it are read.
    Answer(String),
    /// The last item of the reply.
    Finish(Result<ModelChunk, ProviderError>),
    /// Nothing for the caller.
    Skip,
}

/// One sidecar event, given the answer text gathered so far and the family
/// adapter that reads calls out of it.
///
/// Reasoning is streamed as it comes, and so is the answer up to where a call
/// may begin (`visible`). The rest is held until the reply ends, because the
/// calls are written inside it and have to be taken out before any of it is
/// shown -- otherwise a call would reach the caller twice, once as the
/// structured call and once as the text it was written in.
fn step_of(
    event: &serde_json::Value,
    answer: &str,
    adapter: &dyn pwr_compat::ModelBehaviorAdapter,
) -> Step {
    match event["event"].as_str() {
        Some("delta") => {
            let text = event["text"].as_str().unwrap_or_default().to_owned();
            match event["channel"].as_str() {
                Some("reasoning") => Step::Yield(ModelChunk {
                    content: String::new(),
                    thinking: Some(text),
                    tool_calls: Vec::new(),
                    metrics: None,
                    done: false,
                }),
                _ => Step::Answer(text),
            }
        }
        Some("done") => match terminal_of(event) {
            Ok(mut chunk) => {
                let canonical = adapter.normalize(&pwr_provider::ModelReply {
                    content: answer.to_owned(),
                    thinking: String::new(),
                    tool_calls: Vec::new(),
                    chunks: 0,
                    metrics: None,
                });
                if canonical.diagnostics.iter().any(|diagnostic| {
                    diagnostic.kind == "qwen_unterminated_tool_call"
                        || diagnostic.kind == "glm_unterminated_tool_call"
                        || diagnostic.kind == "harmony_unterminated_tool_call"
                }) {
                    return Step::Finish(Err(ProviderError::Truncated {
                        safe_context:
                            "the model stopped inside an unfinished tool call; no call was executed"
                                .into(),
                    }));
                }
                chunk.content = canonical.narrative;
                chunk.tool_calls = canonical.tool_calls;
                if !canonical.thinking.is_empty() {
                    chunk.thinking = Some(canonical.thinking);
                }
                Step::Finish(Ok(chunk))
            }
            Err(error) => Step::Finish(Err(error)),
        },
        Some("error") => Step::Finish(Err(ProviderError::Protocol {
            safe_context: format!(
                "the MLX engine failed: {}",
                event["message"].as_str().unwrap_or("no reason given")
            ),
        })),
        _ => Step::Skip,
    }
}

/// The terminal chunk of a finished reply, or the fault a stopped one is.
fn terminal_of(event: &serde_json::Value) -> Result<ModelChunk, ProviderError> {
    let finish = event["finish_reason"].as_str().unwrap_or("stop");
    if finish == "reasoning_unfinished" {
        return Err(ProviderError::ReasoningUnfinished {
            safe_context: format!(
                "the MLX engine closed the thinking phase at its budget ({} tokens) and no \
                 answer followed",
                event["usage"]["reasoning_tokens"].as_u64().unwrap_or(0)
            ),
        });
    }
    if finish != "stop" {
        // The same fault LM Studio's truncated replies raise, so the
        // loop records a runaway reply the way it always has.
        let repetition = if finish == "repetition" {
            format!(
                "; repeated-window ratios: reasoning={}bp, answer={}bp",
                event["repetition_signals"]["reasoning"]["ratio_bps"]
                    .as_u64()
                    .unwrap_or(0),
                event["repetition_signals"]["answer"]["ratio_bps"]
                    .as_u64()
                    .unwrap_or(0),
            )
        } else {
            String::new()
        };
        return Err(ProviderError::Truncated {
            safe_context: format!(
                "the MLX engine stopped the reply ({finish}) after {} tokens{repetition}",
                event["usage"]["completion_tokens"].as_u64().unwrap_or(0)
            ),
        });
    }
    let usage = &event["usage"];
    let timings = &event["timings"];
    let nanos = |key: &str| timings[key].as_f64().map(|secs| (secs * 1e9) as u64);
    Ok(ModelChunk {
        content: String::new(),
        thinking: None,
        tool_calls: Vec::new(),
        metrics: Some(GenerationMetrics {
            // Counted by the engine with the loaded model's tokenizer, one
            // per generated token; null when the template gave it no
            // delimiters to see the phase by.
            reasoning_tokens: usage["reasoning_tokens"].as_u64(),
            answer_tokens: usage["answer_tokens"].as_u64(),
            token_accounting: Some(pwr_domain::TokenAccounting::EngineTokenizer),
            reasoning_budget_reached: event["budget_forced"].as_bool(),
            reasoning_repetition_bps: event["repetition_signals"]["reasoning"]["ratio_bps"]
                .as_u64()
                .and_then(|value| u16::try_from(value).ok()),
            answer_repetition_bps: event["repetition_signals"]["answer"]["ratio_bps"]
                .as_u64()
                .and_then(|value| u16::try_from(value).ok()),
            prompt_tokens: usage["prompt_tokens"].as_u64(),
            generated_tokens: usage["completion_tokens"].as_u64(),
            total_duration_ns: nanos("prefill_secs")
                .zip(nanos("generation_secs"))
                .map(|(a, b)| a + b),
            load_duration_ns: Some(0),
            prompt_eval_duration_ns: nanos("prefill_secs"),
            generation_duration_ns: nanos("generation_secs"),
        }),
        done: true,
    })
}

#[async_trait]
impl ModelProvider for MlxProvider {
    async fn inspect(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelInspection, ProviderError> {
        let dir = self.config.model_dir(&deployment.model_ref)?;
        if !MlxProvider::model_weights_complete(&dir) {
            return Err(ProviderError::Protocol {
                safe_context: format!(
                    "{} does not have all required safetensors weights",
                    deployment.model_ref
                ),
            });
        }
        let config_bytes =
            std::fs::read(dir.join("config.json")).map_err(|error| ProviderError::Protocol {
                safe_context: format!("cannot read {}: {error}", dir.display()),
            })?;
        let config: serde_json::Value =
            serde_json::from_slice(&config_bytes).map_err(|_| ProviderError::Protocol {
                safe_context: format!("{} has a config.json that is not JSON", dir.display()),
            })?;
        let generation_config = generation_config(&dir)?;
        // The artifact's identity includes the structured generation settings
        // now that they affect runtime behavior. Changing them invalidates
        // digest-scoped calibration/profile evidence.
        let index = std::fs::read(dir.join("model.safetensors.index.json")).unwrap_or_default();
        let generation_bytes = generation_config
            .as_ref()
            .map(serde_json::to_vec)
            .transpose()
            .map_err(|_| ProviderError::Protocol {
                safe_context: "cannot encode generation_config.json".into(),
            })?
            .unwrap_or_default();
        let digest = format!(
            "mlx:{}",
            pwr_domain::hash_bytes(
                [
                    config_bytes.as_slice(),
                    index.as_slice(),
                    generation_bytes.as_slice()
                ]
                .concat()
            )
        );
        let quantization = config["quantization"]["bits"]
            .as_u64()
            .map(|bits| format!("{bits}bit"));
        // Identities of what shapes the model's behaviour besides its
        // weights, for evidence provenance. Hashes of files read as bytes;
        // nothing in them is executed.
        let tokenizer_identity = ["tokenizer.json", "tokenizer_config.json"]
            .iter()
            .filter_map(|file| std::fs::read(dir.join(file)).ok())
            .reduce(|mut all, bytes| {
                all.extend(bytes);
                all
            })
            .map(pwr_domain::hash_bytes);
        let template = chat_template_text(&dir);
        let template_identity = template
            .as_deref()
            .map(|text| pwr_domain::hash_bytes(text.as_bytes()));
        let reasoning = template
            .as_deref()
            .map(pwr_domain::TemplateReasoning::of)
            .unwrap_or_default();
        let revision = std::fs::read_to_string(dir.join(".pwr-revision"))
            .ok()
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        let mut capabilities = BTreeMap::new();
        capabilities.insert(
            "capability_probe".into(),
            Observation::Unknown {
                reason: "an MLX model declares no capabilities; the active probe establishes them"
                    .into(),
            },
        );
        Ok(ModelInspection {
            definition: ModelDefinition {
                schema_version: 1,
                id: new_id(),
                digest: digest.clone(),
                family: config["model_type"].as_str().map(str::to_owned),
                quantization,
                capabilities,
                metadata: serde_json::json!({
                    "path": dir,
                    "model_type": config["model_type"],
                    "max_position_embeddings": config
                        .get("text_config")
                        .unwrap_or(&config)["max_position_embeddings"],
                    "weights_bytes": Self::weights_bytes(&dir),
                    "weights_fingerprint": weights_fingerprint(&dir),
                    "tokenizer_fingerprint": tokenizer_identity,
                    "chat_template_fingerprint": template_identity,
                    "has_chat_template": template.is_some(),
                    "reasoning_template": reasoning,
                    "reasoning_capability": reasoning.capability(true),
                    "generation_config": generation_config,
                    "revision": revision,
                    "format": "mlx",
                }),
                provenance: Provenance {
                    source: format!("mlx:{}", dir.display()),
                    observed_at: Utc::now(),
                    content_hash: digest,
                },
            },
            deployment: deployment.clone(),
        })
    }

    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        let slot = self.sidecar.lock().await;
        let loaded = slot
            .as_ref()
            .and_then(|sidecar| sidecar.loaded.as_ref())
            .map(|dir| dir.display().to_string());
        Ok(BackendState {
            observed_at: Utc::now(),
            loaded_models: loaded.iter().cloned().collect(),
            state: serde_json::json!({"engine": "mlx", "loaded": loaded}),
        })
    }

    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        Ok(self.reply(request).await?.0)
    }

    /// Stops the generation itself, not only the reading of it: the sidecar is
    /// told to cancel the request, and ends it within a token.
    async fn chat_cancellable(
        &self,
        request: ModelRequest,
        cancel: pwr_provider::Cancel,
    ) -> Result<ModelStream, ProviderError> {
        let (stream, id, stdin, ended) = self.reply(request).await?;
        let watch = cancel.clone();
        tokio::spawn(async move {
            tokio::select! {
                () = watch.cancelled() => {
                    let _ = write_line(&stdin, serde_json::json!({"op": "cancel", "target": id})).await;
                }
                _ = ended => {}
            }
        });
        Ok(pwr_provider::cancellable(cancel, stream))
    }

    /// The window is a property of each request here, not of a load: MLX grows
    /// its cache as the prompt grows. What bounds it is memory, which the
    /// computed window already accounts for, so the window asked for is the
    /// window in force.
    async fn prepare_context(
        &self,
        _deployment: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        Ok(context_tokens)
    }
}

#[async_trait]
impl InferenceBackend for MlxProvider {
    fn backend_id(&self) -> &'static str {
        "mlx"
    }

    /// `mlx-lm <v>; mlx <v>; sidecar <hash>`: the libraries that generate,
    /// and the engine script that drives them (its budget and finalization
    /// logic are part of what a calibration measured). Asked of the
    /// interpreter once per process, with a timeout; `None` when it cannot
    /// answer, which evidence provenance records as unknown.
    async fn backend_version(&self) -> Result<Option<String>, ProviderError> {
        let python = self.config.python.clone();
        let sidecar = self.config.sidecar.clone();
        let version = self
            .version
            .get_or_init(|| async move {
                let libraries = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    tokio::process::Command::new(&python)
                        .args([
                            "-c",
                            "import mlx.core as mx, mlx_lm; print(mlx_lm.__version__, mx.__version__)",
                        ])
                        .stdin(std::process::Stdio::null())
                        .output(),
                )
                .await
                .ok()?
                .ok()
                .filter(|output| output.status.success())?;
                let text = String::from_utf8_lossy(&libraries.stdout);
                let mut parts = text.split_whitespace();
                let (lm, core) = (parts.next()?, parts.next()?);
                let script = std::fs::read(&sidecar)
                    .map(|bytes| pwr_domain::hash_bytes(&bytes))
                    .unwrap_or_else(|_| "unknown".into());
                let script = script.trim_start_matches("sha256:");
                Some(format!(
                    "mlx-lm {lm}; mlx {core}; sidecar {}",
                    &script[..script.len().min(12)]
                ))
            })
            .await;
        Ok(version.clone())
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_discovery: true,
            model_lifecycle: true,
            streaming: true,
            // A cancel is sent to the sidecar, which ends the generation
            // within a token (`chat_cancellable`); a stream merely dropped is
            // still drained before the next request.
            cancellation: true,
            // Read from the answer by the family adapter before they are handed
            // on, so they arrive structured, as a native parser's would.
            native_tools: true,
            constrained_tool_calls: false,
            generation_metrics: true,
            context_window_control: true,
        }
    }

    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError> {
        let mut found = Vec::new();
        let Ok(publishers) = std::fs::read_dir(&self.config.models_root) else {
            return Ok(found);
        };
        for publisher in publishers.flatten() {
            let Ok(models) = std::fs::read_dir(publisher.path()) else {
                continue;
            };
            for model in models.flatten() {
                let dir = model.path();
                let Some(config) = Self::read_config(&dir) else {
                    continue;
                };
                if !MlxProvider::model_weights_complete(&dir) {
                    continue;
                }
                // Only MLX-format artifacts: a GGUF directory has no config.
                let Ok(relative) = dir.strip_prefix(&self.config.models_root) else {
                    continue;
                };
                let text = config.get("text_config").unwrap_or(&config);
                found.push(DiscoveredModel {
                    model_ref: relative.display().to_string(),
                    digest: None,
                    context_limit: text["max_position_embeddings"]
                        .as_u64()
                        .and_then(|limit| u32::try_from(limit).ok()),
                    size_bytes: Self::weights_bytes(&dir),
                    accelerator_size_bytes: None,
                });
            }
        }
        found.sort_by(|a, b| a.model_ref.cmp(&b.model_ref));
        Ok(found)
    }

    async fn model_facts(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelFacts, ProviderError> {
        let dir = self.config.model_dir(&deployment.model_ref)?;
        let config = Self::read_config(&dir);
        let trained_max = config.as_ref().and_then(|config| {
            let text = config.get("text_config").unwrap_or(config);
            u32::try_from(text["max_position_embeddings"].as_u64()?).ok()
        });
        let prefill_scores_bytes = match &config {
            Some(config) => self.prefill_scores_bytes(config).await,
            None => None,
        };
        Ok(ModelFacts {
            prefill_scores_bytes,
            hf_config: config,
            gguf_metadata: None,
            trained_max,
            weights_bytes: Self::weights_bytes(&dir),
            source: format!("mlx:{}", dir.join("config.json").display()),
        })
    }

    async fn load_model(&self, model_ref: &str) -> Result<(), ProviderError> {
        let dir = self.config.model_dir(model_ref)?;
        if !MlxProvider::model_weights_complete(&dir) {
            return Err(ProviderError::Protocol {
                safe_context: format!("{model_ref} does not have all required safetensors weights"),
            });
        }
        let mut slot = self.sidecar.lock().await;
        self.ensure_loaded(&mut slot, &dir).await
    }

    async fn unload_model(&self, _model_ref: &str) -> Result<(), ProviderError> {
        self.stop().await;
        Ok(())
    }

    async fn release(&self, _deployment: &DeploymentDescriptor) -> Result<(), ProviderError> {
        self.stop().await;
        Ok(())
    }
}

impl MlxProvider {
    /// Ends the sidecar, and with it the model's memory. Politely first, so it
    /// can exit on its own; killed if it does not.
    async fn stop(&self) {
        let mut slot = self.sidecar.lock().await;
        if let Some(mut sidecar) = slot.take() {
            let _ = sidecar.send(serde_json::json!({"op": "shutdown"})).await;
            let exited =
                tokio::time::timeout(std::time::Duration::from_secs(5), sidecar.child.wait()).await;
            if exited.is_err() {
                let _ = sidecar.child.kill().await;
            }
        }
    }
}

#[cfg(test)]
mod tests {

    fn stream_of(pieces: &[&str]) -> (String, String, Live) {
        let mut live = Live {
            answer: String::new(),
            sent_thinking: 0,
            sent_text: 0,
        };
        let (mut thinking, mut text) = (String::new(), String::new());
        for piece in pieces {
            live.answer.push_str(piece);
            if let Some(chunk) = live.advance() {
                text.push_str(&chunk.content);
                thinking.push_str(chunk.thinking.as_deref().unwrap_or_default());
            }
        }
        (thinking, text, live)
    }

    /// Qwen3-14B writes its own think block; the text after it starts with a
    /// blank line the adapter trims (2026-09-23: the answer ended mid-word).
    #[test]
    fn an_answer_after_an_inline_think_block_arrives_whole() {
        let pieces = [
            "<think>\nThe user said hello.\n</think>",
            "\n\nCiao! How can I a",
            "ssist you today?",
        ];
        let (_, shown, live) = stream_of(&pieces);
        let finished = live.finish(ModelChunk {
            content: "Ciao! How can I assist you today?".into(),
            ..Default::default()
        });
        assert_eq!(
            format!("{shown}{}", finished.content).trim(),
            "Ciao! How can I assist you today?"
        );
    }

    #[test]
    fn answer_text_streams_and_a_call_is_never_shown_as_text() {
        let pieces = [
            "I will read ",
            "the file first, then fix it.",
            "\n\n<tool",
            "_call>\n<function=read_file>\n",
            "<parameter=path>\na.py\n</parameter>\n</function>\n</tool_call>",
        ];
        let (_, shown, live) = stream_of(&pieces);
        assert!(shown.starts_with("I will read the file"), "{shown:?}");
        assert!(!shown.contains('<'), "part of a call was shown: {shown:?}");
        // The terminal chunk adds only what was not shown, and no call text.
        let finished = live.finish(ModelChunk {
            content: "I will read the file first, then fix it.".into(),
            ..Default::default()
        });
        assert!(
            !finished.content.contains("I will read"),
            "{:?}",
            finished.content
        );
        assert_eq!(
            format!("{shown}{}", finished.content).trim(),
            "I will read the file first, then fix it."
        );
    }

    #[test]
    fn a_plain_answer_is_shown_whole_once() {
        let (_, shown, live) =
            stream_of(&["The answer ", "is 42, and nothing ", "else is needed."]);
        let finished = live.finish(ModelChunk {
            content: "The answer is 42, and nothing else is needed.".into(),
            ..Default::default()
        });
        assert_eq!(
            format!("{shown}{}", finished.content),
            "The answer is 42, and nothing else is needed."
        );
    }

    #[test]
    fn harmony_reasoning_and_answer_stream_on_their_own_channels() {
        let pieces = [
            "<|channel|>analysis<|message|>The user wants ",
            "a count; easy.<|end|>",
            "<|start|>assistant<|channel|>final<|message|>1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12",
        ];
        let (thinking, shown, _) = stream_of(&pieces);
        assert_eq!(thinking, "The user wants a count; easy.");
        assert!(shown.starts_with("1\n2\n3"), "{shown:?}");
        assert!(!shown.contains("<|"), "{shown:?}");
    }

    #[test]
    fn a_leading_think_block_streams_as_reasoning() {
        let (thinking, shown, _) = stream_of(&[
            "<seed:think>The user wants a count.",
            "</seed:think>1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n12\n",
        ]);
        assert_eq!(thinking, "The user wants a count.");
        assert!(shown.starts_with("1\n2\n"), "{shown:?}");
    }

    #[test]
    fn a_harness_note_in_the_tool_role_reaches_the_template_as_a_user_message() {
        let messages = vec![
            ChatMessage::text("user", "Fix it."),
            ChatMessage::text("tool", "The checks fail."),
            ChatMessage {
                role: "assistant".into(),
                tool_calls: vec![pwr_domain::ToolCall {
                    name: "read_file".into(),
                    arguments: serde_json::json!({"path": "a.py"}),
                    id: Some("c1".into()),
                }],
                ..Default::default()
            },
            ChatMessage {
                role: "tool".into(),
                content: "contents".into(),
                tool_call_id: Some("c1".into()),
                ..Default::default()
            },
        ];
        let rendered = template_messages(&messages);
        assert_eq!(rendered[1]["role"], "user");
        assert_eq!(rendered[1]["content"], "[PWR] The checks fail.");
        assert_eq!(rendered[3]["role"], "tool");
    }

    #[test]
    fn a_harmony_call_is_held() {
        let (_, shown, _) = stream_of(&[
            "<|channel|>analysis<|message|>Read it.<|end|><|start|>assistant",
            "<|channel|>commentary to=functions.read_file <|constrain|>json<|message|>{\"path\":\"a.py\"}",
        ]);
        assert!(shown.is_empty(), "{shown:?}");
    }

    use super::*;
    use pwr_domain::ToolCall;

    #[test]
    fn a_call_keeps_its_arguments_as_an_object_for_the_template() {
        let message = ChatMessage {
            tool_calls: vec![ToolCall {
                name: "read_file".into(),
                arguments: serde_json::json!({"path": "src/lib.rs"}),
                id: Some("call_1".into()),
            }],
            ..ChatMessage::text("assistant", "")
        };
        let wire = template_message(&message);
        assert_eq!(
            wire["tool_calls"][0]["function"]["arguments"]["path"],
            "src/lib.rs"
        );
    }

    #[test]
    fn an_assistant_steps_reasoning_reaches_the_template_and_nothing_else_carries_it() {
        let mut step = ChatMessage::text("assistant", "Running the tests.");
        step.reasoning = Some("The check character is U; the tables are settled.".into());
        let wire = template_message(&step);
        assert_eq!(
            wire["reasoning_content"],
            "The check character is U; the tables are settled."
        );
        // Not stored: a message written to disk and read back has none.
        let stored: ChatMessage =
            serde_json::from_str(&serde_json::to_string(&step).unwrap()).unwrap();
        assert_eq!(stored.reasoning, None);
        let mut user = ChatMessage::text("user", "hi");
        user.reasoning = Some("never sent for a user message".into());
        assert!(template_message(&user).get("reasoning_content").is_none());
        assert!(
            template_message(&ChatMessage::text("assistant", "ok"))
                .get("reasoning_content")
                .is_none()
        );
    }

    #[test]
    fn an_attached_image_goes_before_the_text_and_a_plain_message_stays_a_string() {
        let mut message = ChatMessage::text("user", "what does this show?");
        message.images = vec!["/w/.pwr/images/ab.png".into()];
        let wire = template_message(&message);
        assert_eq!(
            wire["content"],
            serde_json::json!([
                {"type": "image", "path": "/w/.pwr/images/ab.png"},
                {"type": "text", "text": "what does this show?"}
            ])
        );
        let plain = template_message(&ChatMessage::text("user", "hi"));
        assert_eq!(plain["content"], "hi");
    }

    #[test]
    fn the_reasoning_switch_and_the_cap_reach_the_sidecar() {
        let request: ModelRequest = serde_json::from_value(serde_json::json!({
            "deployment": {
                "schema_version": 1, "id": new_id(), "provider": "mlx", "endpoint": "",
                "model_ref": "m", "backend_options": {}, "auth_ref": null
            },
            "messages": [{"role": "user", "content": "hi"}],
            "context_tokens": 4096,
            "tools": null,
            "seed": 3,
            "sampling": {"think": false, "temperature": 0.6}
        }))
        .unwrap();
        let body = chat_body(&request);
        assert_eq!(body["thinking"], false);
        assert_eq!(body["temperature"], 0.6);
        assert_eq!(body["max_tokens"], DEFAULT_MAX_TOKENS);
        assert_eq!(body["seed"], 3);
        assert_eq!(body["messages"][0]["content"], "hi");
    }

    #[test]
    fn artifact_sampling_fills_only_missing_supported_values() {
        let mut sampling = BTreeMap::from([
            ("temperature".into(), serde_json::json!(0.6)),
            ("min_p".into(), serde_json::json!(0.1)),
        ]);
        resolve_generation_sampling(
            &mut sampling,
            Some(&serde_json::json!({
                "temperature": 1.0, "top_p": 0.95, "top_k": 20,
                "min_p": 0.2, "do_sample": true
            })),
        )
        .unwrap();
        assert_eq!(sampling["temperature"], 0.6);
        assert_eq!(sampling["top_p"], 0.95);
        assert_eq!(sampling["top_k"], 20);
        assert_eq!(sampling["min_p"], 0.1);
        assert_eq!(sampling["_pwr_sampling_sources"]["temperature"], "request");
        assert_eq!(
            sampling["_pwr_sampling_sources"]["top_p"],
            "artifact_generation_config"
        );
        assert_eq!(sampling["_pwr_sampling_sources"]["min_p"], "request");
    }

    #[test]
    fn missing_or_invalid_artifact_sampling_is_explicit() {
        let mut sampling = BTreeMap::new();
        resolve_generation_sampling(&mut sampling, None).unwrap();
        assert_eq!(sampling["temperature"], 0.0);
        assert_eq!(sampling["top_p"], 0.0);
        assert_eq!(sampling["top_k"], 0);
        assert_eq!(sampling["min_p"], 0.0);
        assert!(!sampling.contains_key("presence_penalty"));
        assert!(!sampling.contains_key("repetition_penalty"));
        assert_eq!(
            sampling["_pwr_sampling_sources"]["top_k"],
            "mlx_sidecar_default"
        );

        let mut invalid = BTreeMap::new();
        assert!(
            resolve_generation_sampling(&mut invalid, Some(&serde_json::json!({"top_p": 1.5})),)
                .is_err()
        );

        let mut greedy = BTreeMap::new();
        resolve_generation_sampling(
            &mut greedy,
            Some(&serde_json::json!({"do_sample": false, "temperature": 1.0})),
        )
        .unwrap();
        assert_eq!(greedy["temperature"], 0.0);
        assert_eq!(
            greedy["_pwr_sampling_sources"]["temperature"],
            "artifact_do_sample_false"
        );
    }

    #[test]
    fn legacy_repetition_penalty_is_normalized_and_conflicts_fail() {
        let mut sampling = BTreeMap::from([
            ("repeat_penalty".into(), serde_json::json!(1.05)),
            (
                "_pwr_sampling_sources".into(),
                serde_json::json!({"repeat_penalty": {"kind": "declared_profile"}}),
            ),
        ]);
        resolve_generation_sampling(&mut sampling, None).unwrap();
        assert_eq!(sampling["repetition_penalty"], 1.05);
        assert!(!sampling.contains_key("repeat_penalty"));
        assert_eq!(
            sampling["_pwr_sampling_sources"]["repetition_penalty"]["kind"],
            "declared_profile"
        );

        let mut conflict = BTreeMap::from([
            ("repeat_penalty".into(), serde_json::json!(1.05)),
            ("repetition_penalty".into(), serde_json::json!(1.1)),
        ]);
        assert!(resolve_generation_sampling(&mut conflict, None).is_err());
    }

    #[test]
    fn malformed_generation_config_is_a_visible_error() {
        let model = tempfile::tempdir().unwrap();
        assert!(generation_config(model.path()).unwrap().is_none());
        std::fs::write(model.path().join("generation_config.json"), "{bad").unwrap();
        assert!(generation_config(model.path()).is_err());
    }

    #[test]
    fn a_failed_forced_close_is_its_own_fault_not_a_truncation() {
        let event = serde_json::json!({
            "event": "done", "finish_reason": "reasoning_unfinished", "budget_forced": true,
            "usage": {"completion_tokens": 900, "reasoning_tokens": 700}, "timings": {}
        });
        assert!(matches!(
            step_of(&event, "", &pwr_compat::QwenFamilyAdapter),
            Step::Finish(Err(ProviderError::ReasoningUnfinished { .. }))
        ));
    }

    #[test]
    fn an_eos_inside_a_tool_call_is_not_a_successful_generation() {
        let event = serde_json::json!({
            "event": "done", "finish_reason": "stop", "usage": {}, "timings": {}
        });
        assert!(matches!(
            step_of(
                &event,
                "<tool_call>{\"name\":\"read_file\",\"arguments\":{\"path\":\"a",
                &pwr_compat::QwenFamilyAdapter,
            ),
            Step::Finish(Err(ProviderError::Truncated { .. }))
        ));
        assert!(matches!(
            step_of(
                &event,
                "<tool_call>read_file<arg_key>path</arg_key><arg_value>a.txt</arg_value>",
                &pwr_compat::GlmFamilyAdapter,
            ),
            Step::Finish(Err(ProviderError::Truncated { .. }))
        ));
        assert!(matches!(
            step_of(
                &event,
                "<|channel|>commentary to=functions.read_file<|message|>{\"path\":\"a.txt\"}",
                &pwr_compat::HarmonyAdapter,
            ),
            Step::Finish(Err(ProviderError::Truncated { .. }))
        ));
    }

    #[test]
    fn reasoning_and_answer_tokens_are_the_engines_own_counts() {
        let event = serde_json::json!({
            "event": "done", "finish_reason": "stop", "budget_forced": true,
            "usage": {"prompt_tokens": 50, "completion_tokens": 900,
                      "reasoning_tokens": 700, "answer_tokens": 200},
            "timings": {}
        });
        let metrics = terminal_of(&event).unwrap().metrics.unwrap();
        assert_eq!(metrics.reasoning_tokens, Some(700));
        assert_eq!(metrics.answer_tokens, Some(200));
        assert_eq!(metrics.reasoning_budget_reached, Some(true));
        assert_eq!(
            metrics.token_accounting,
            Some(pwr_domain::TokenAccounting::EngineTokenizer)
        );
        // A template the engine could not see reasoning in reports none,
        // rather than zero.
        let untracked = serde_json::json!({
            "event": "done", "finish_reason": "stop",
            "usage": {"completion_tokens": 900, "reasoning_tokens": null, "answer_tokens": null},
            "timings": {}
        });
        let metrics = terminal_of(&untracked).unwrap().metrics.unwrap();
        assert_eq!(metrics.reasoning_tokens, None);
    }

    #[test]
    fn the_budget_and_the_level_reach_the_sidecar() {
        let request: ModelRequest = serde_json::from_value(serde_json::json!({
            "deployment": {
                "schema_version": 1, "id": new_id(), "provider": "mlx", "endpoint": "",
                "model_ref": "m", "backend_options": {}, "auth_ref": null
            },
            "messages": [{"role": "user", "content": "hi"}],
            "context_tokens": 4096, "tools": null, "seed": null,
            "sampling": {"reasoning_budget": 3000, "reasoning_effort": "high", "max_tokens": 19000}
        }))
        .unwrap();
        let body = chat_body(&request);
        assert_eq!(body["reasoning_budget"], 3000);
        assert_eq!(body["reasoning_effort"], "high");
        assert_eq!(body["max_tokens"], 19000);
    }

    #[test]
    fn the_chat_template_is_read_from_where_the_model_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(chat_template_text(dir.path()), None);
        std::fs::write(
            dir.path().join("tokenizer_config.json"),
            r#"{"chat_template": [{"name": "default", "template": "A"}, {"name": "tool_use", "template": "<think></think>"}]}"#,
        )
        .unwrap();
        let text = chat_template_text(dir.path()).unwrap();
        assert!(text.contains('A') && text.contains("<think>"));
        std::fs::write(dir.path().join("chat_template.jinja"), "jinja wins").unwrap();
        assert_eq!(
            chat_template_text(dir.path()).as_deref(),
            Some("jinja wins")
        );
    }

    #[test]
    fn a_relative_reference_cannot_leave_the_models_folder() {
        let root = tempfile::tempdir().unwrap();
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("config.json"), "{}").unwrap();
        let config = MlxConfig {
            python: "python3".into(),
            sidecar: "s.py".into(),
            models_root: root.path().join("models"),
        };
        assert!(config.model_dir("../outside").is_err());
        assert!(config.model_dir(&outside.display().to_string()).is_ok());
    }

    #[test]
    fn a_stopped_reply_is_the_same_fault_as_a_truncated_one() {
        let event = serde_json::json!({
            "event": "done", "finish_reason": "repetition",
            "usage": {"completion_tokens": 4864}, "timings": {}
        });
        assert!(matches!(
            step_of(&event, "", &pwr_compat::QwenFamilyAdapter),
            Step::Finish(Err(ProviderError::Truncated { .. }))
        ));
    }

    #[test]
    fn reasoning_streams_and_the_answer_waits() {
        let adapter = pwr_compat::QwenFamilyAdapter;
        let reasoning = serde_json::json!({"event": "delta", "channel": "reasoning", "text": "hm"});
        assert!(matches!(
            step_of(&reasoning, "", &adapter),
            Step::Yield(ModelChunk { thinking: Some(ref text), .. }) if text == "hm"
        ));
        let answer = serde_json::json!({"event": "delta", "channel": "content", "text": "ok"});
        assert!(matches!(step_of(&answer, "", &adapter), Step::Answer(ref text) if text == "ok"));
    }

    #[test]
    fn a_call_written_in_the_answer_arrives_structured_and_not_as_text() {
        // The capability probe of 2026-09-18 saw no call at all: it reads the
        // stream as a backend hands it over, and the call was still text.
        let written = "Reading it.\n<tool_call>\n<function=probe_echo>\n<parameter=value>\nok\n</parameter>\n</function>\n</tool_call>";
        let done = serde_json::json!({
            "event": "done", "finish_reason": "stop",
            "usage": {"prompt_tokens": 10, "completion_tokens": 20},
            "timings": {"prefill_secs": 0.1, "generation_secs": 0.2}
        });
        let Step::Finish(Ok(chunk)) = step_of(&done, written, &pwr_compat::QwenFamilyAdapter)
        else {
            panic!("a finished reply");
        };
        assert!(chunk.done);
        assert_eq!(chunk.tool_calls.len(), 1);
        assert_eq!(chunk.tool_calls[0].name, "probe_echo");
        assert_eq!(chunk.tool_calls[0].arguments["value"], "ok");
        assert_eq!(chunk.content, "Reading it.");
        assert_eq!(chunk.metrics.unwrap().generated_tokens, Some(20));
    }

    #[test]
    fn a_model_is_a_directory_with_a_config() {
        let root = tempfile::tempdir().unwrap();
        let model = root.path().join("pub/model");
        std::fs::create_dir_all(&model).unwrap();
        let config = MlxConfig {
            python: "python3".into(),
            sidecar: "x".into(),
            models_root: root.path().into(),
        };
        assert!(config.model_dir("pub/model").is_err());
        std::fs::write(model.join("config.json"), "{}").unwrap();
        assert_eq!(config.model_dir("pub/model").unwrap(), model);
    }

    #[test]
    fn discovery_and_loading_reject_an_interrupted_sharded_model() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("publisher/model");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.json"), "{}").unwrap();
        for number in 1..=4 {
            std::fs::write(
                dir.join(format!("model-{number:05}-of-00008.safetensors")),
                [1_u8],
            )
            .unwrap();
        }
        std::fs::write(dir.join("model-00005-of-00008.safetensors.part"), [1_u8]).unwrap();
        assert!(!MlxProvider::model_weights_complete(&dir));

        std::fs::remove_file(dir.join("model-00005-of-00008.safetensors.part")).unwrap();
        assert!(!MlxProvider::model_weights_complete(&dir));
        for number in 5..=8 {
            std::fs::write(
                dir.join(format!("model-{number:05}-of-00008.safetensors")),
                [1_u8],
            )
            .unwrap();
        }
        assert!(MlxProvider::model_weights_complete(&dir));
    }
}
