//! Runtime backend selection shared by CLI commands and future frontends.
//!
//! This crate is a composition boundary: it binds a local backend adapter to a
//! provider-neutral contract and produces an immutable deployment descriptor.
//! It contains no task, prompt, tool, verification or CLI policy.

pub mod hardware;
pub mod host;

use async_trait::async_trait;
use pwr_domain::{BackendState, DeploymentDescriptor, ModelInspection, ModelRequest, new_id};
use pwr_llama::{LlamaConfig, LlamaProvider};
use pwr_mlx::{MlxConfig, MlxProvider};
use pwr_provider::{
    BackendCapabilities, DiscoveredModel, InferenceBackend, ModelProvider, ModelStream,
    ProviderError,
};
use std::collections::BTreeMap;
use std::time::Duration;

/// The inference engines this build can run.
///
/// One today: PWR's own MLX engine. The HTTP backends (Ollama, LM Studio)
/// were removed on 2026-09-19 at the maintainer's request, once PWR ran
/// models itself and matched Bionic on A.1's tasks through its own engine; the
/// last revision with them is tagged `last-with-http-backends`. The kind stays
/// an enum because llama.cpp, for GGUF and Windows, joins it next (roadmap
/// step 8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackendKind {
    /// PWR's own engine: MLX in a sidecar it starts itself.
    Mlx,
    /// The llama.cpp/GGUF engine. Metadata inspection is wired first;
    /// generation follows behind the same provider contract.
    Llama,
}

impl BackendKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Mlx => "mlx",
            Self::Llama => "llama",
        }
    }

    /// The address recorded in a deployment. The engine is a child process on
    /// a pipe and listens nowhere; a loopback placeholder keeps the recorded
    /// field meaning "local".
    pub fn default_endpoint(self) -> &'static str {
        match self {
            Self::Mlx => "http://127.0.0.1:0/",
            Self::Llama => "http://127.0.0.1:0/",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mlx" => Some(Self::Mlx),
            "llama" | "llama.cpp" | "gguf" => Some(Self::Llama),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct RuntimeFactory {
    kind: BackendKind,
}

impl RuntimeFactory {
    pub fn local(kind: BackendKind) -> Self {
        Self { kind }
    }

    pub fn kind(&self) -> BackendKind {
        self.kind
    }

    pub fn backend(&self, _timeout: Duration) -> Result<RuntimeBackend, ProviderError> {
        Ok(match self.kind {
            BackendKind::Mlx => RuntimeBackend::Mlx(MlxProvider::new(MlxConfig::from_env())),
            BackendKind::Llama => {
                RuntimeBackend::Llama(LlamaProvider::new(LlamaConfig::from_env()))
            }
        })
    }

    pub fn select(
        &self,
        model_ref: impl Into<String>,
        timeout: Duration,
    ) -> Result<ResolvedBackendSelection, ProviderError> {
        let backend = self.backend(timeout)?;
        let deployment = DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: backend.backend_id().into(),
            endpoint: self.kind.default_endpoint().into(),
            model_ref: model_ref.into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        Ok(ResolvedBackendSelection {
            backend,
            deployment,
        })
    }
}

pub struct ResolvedBackendSelection {
    pub backend: RuntimeBackend,
    pub deployment: DeploymentDescriptor,
}

/// Current runtime backends. New variants must delegate the stable provider
/// contracts; application command handlers never match this enum.
pub enum RuntimeBackend {
    Mlx(MlxProvider),
    Llama(LlamaProvider),
}

impl RuntimeBackend {
    /// Renders canonical tools for the selected backend. Command handlers pass
    /// semantic definitions; they do not choose a backend wire format.
    pub fn render_tools(&self, catalog: &pwr_domain::ToolCatalog) -> serde_json::Value {
        match self {
            // The chat template renders them; it reads the OpenAI shape.
            Self::Mlx(_) => pwr_compat::render_tools(catalog),
            Self::Llama(_) => pwr_compat::render_tools(catalog),
        }
    }
}

#[async_trait]
impl ModelProvider for RuntimeBackend {
    async fn inspect(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<ModelInspection, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.inspect(deployment).await,
            Self::Llama(provider) => provider.inspect(deployment).await,
        }
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.runtime_state().await,
            Self::Llama(provider) => provider.runtime_state().await,
        }
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.chat(request).await,
            Self::Llama(provider) => provider.chat(request).await,
        }
    }
    async fn prepare_context(
        &self,
        deployment: &DeploymentDescriptor,
        context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.prepare_context(deployment, context_tokens).await,
            Self::Llama(provider) => provider.prepare_context(deployment, context_tokens).await,
        }
    }
}

#[async_trait]
impl InferenceBackend for RuntimeBackend {
    fn backend_id(&self) -> &'static str {
        match self {
            Self::Mlx(provider) => provider.backend_id(),
            Self::Llama(provider) => provider.backend_id(),
        }
    }
    fn capabilities(&self) -> BackendCapabilities {
        match self {
            Self::Mlx(provider) => provider.capabilities(),
            Self::Llama(provider) => provider.capabilities(),
        }
    }
    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.discover_models().await,
            Self::Llama(provider) => provider.discover_models().await,
        }
    }
    async fn load_model(&self, model_ref: &str) -> Result<(), ProviderError> {
        match self {
            Self::Mlx(provider) => provider.load_model(model_ref).await,
            Self::Llama(provider) => provider.load_model(model_ref).await,
        }
    }
    async fn unload_model(&self, model_ref: &str) -> Result<(), ProviderError> {
        match self {
            Self::Mlx(provider) => provider.unload_model(model_ref).await,
            Self::Llama(provider) => provider.unload_model(model_ref).await,
        }
    }
    async fn backend_version(&self) -> Result<Option<String>, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.backend_version().await,
            Self::Llama(provider) => provider.backend_version().await,
        }
    }
    async fn is_resident(&self, deployment: &DeploymentDescriptor) -> Result<bool, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.is_resident(deployment).await,
            Self::Llama(provider) => provider.is_resident(deployment).await,
        }
    }
    async fn release(&self, deployment: &DeploymentDescriptor) -> Result<(), ProviderError> {
        match self {
            Self::Mlx(provider) => provider.release(deployment).await,
            Self::Llama(provider) => provider.release(deployment).await,
        }
    }

    async fn model_facts(
        &self,
        deployment: &DeploymentDescriptor,
    ) -> Result<pwr_provider::ModelFacts, ProviderError> {
        match self {
            Self::Mlx(provider) => provider.model_facts(deployment).await,
            Self::Llama(provider) => provider.model_facts(deployment).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_resolves_the_engine_and_a_deployment_without_starting_it() {
        let selection = RuntimeFactory::local(BackendKind::Mlx)
            .select(
                "lmstudio-community/Qwen3.6-35B-A3B-MLX-4bit",
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(selection.backend.backend_id(), "mlx");
        assert_eq!(selection.deployment.provider, "mlx");
        assert_eq!(selection.deployment.endpoint, "http://127.0.0.1:0/");
    }

    #[test]
    fn backends_are_parsed_explicitly() {
        assert_eq!(BackendKind::parse("MLX"), Some(BackendKind::Mlx));
        assert_eq!(BackendKind::parse("llama.cpp"), Some(BackendKind::Llama));
        assert_eq!(BackendKind::parse("gguf"), Some(BackendKind::Llama));
        assert_eq!(BackendKind::parse("ollama"), None);
        assert_eq!(BackendKind::parse("lmstudio"), None);
    }

    #[test]
    fn tools_render_in_the_shape_chat_templates_read() {
        let catalog = pwr_domain::ToolCatalog::new(vec![pwr_domain::ToolDefinition {
            name: "return_status".into(),
            description: "Report the outcome.".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        }])
        .unwrap();
        let backend = RuntimeFactory::local(BackendKind::Mlx)
            .backend(Duration::from_secs(1))
            .unwrap();
        let rendered = backend.render_tools(&catalog);
        assert_eq!(rendered[0]["function"]["name"], "return_status");
        assert!(rendered[0]["function"]["parameters"].is_object());
    }
}

/// Where an engine looks for models, and so where the Model Manager puts the
/// ones it downloads: `PWR_MLX_MODELS` / `PWR_LLAMA_MODELS`, by default
/// `~/.pwr/models` for both. Until 2026-09-24 the default was LM Studio's
/// folder, `~/.lmstudio/models`, from when LM Studio served the models.
pub fn models_root(kind: BackendKind) -> std::path::PathBuf {
    match kind {
        BackendKind::Mlx => MlxConfig::from_env().models_root,
        BackendKind::Llama => LlamaConfig::from_env().models_root,
    }
}

/// Whether an engine can run on this machine now, and what to do if not.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub id: &'static str,
    pub label: &'static str,
    pub format: &'static str,
    pub active: bool,
    pub available: bool,
    pub detail: String,
    pub models_root: String,
}

/// Each engine's readiness: MLX needs Apple Silicon and a Python with
/// `mlx-lm`; llama.cpp needs `llama-server`. Checked by running them, with a
/// timeout, never by guessing from paths.
pub async fn backend_status(active: BackendKind) -> Vec<BackendStatus> {
    let mlx = MlxConfig::from_env();
    let mlx_status = if !(cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")) {
        (false, "MLX runs only on Apple Silicon Macs.".to_owned())
    } else {
        match run_quietly(
            &mlx.python,
            &["-c", "import mlx_lm; print(mlx_lm.__version__)"],
        )
        .await
        {
            Some(version) => (
                true,
                format!("mlx-lm {version} via {}", mlx.python.display()),
            ),
            None => (
                false,
                format!(
                    "The MLX engine is not set up: {} cannot import mlx_lm. The app installs it \
                     when PWR_MLX_PYTHON is not set; from a checkout, run scripts/setup-mlx.sh, or \
                     set PWR_MLX_PYTHON to a Python with mlx-lm installed.",
                    mlx.python.display()
                ),
            ),
        }
    };
    let llama = LlamaConfig::from_env();
    let llama_status = match run_quietly(&llama.server, &["--version"]).await {
        Some(version) => (
            true,
            format!(
                "{} ({})",
                llama.server.display(),
                version.lines().next().unwrap_or_default()
            ),
        ),
        None => (
            false,
            format!(
                "llama.cpp backend is not configured: {} was not found. Install llama.cpp and put \
                 llama-server on PATH, or set PWR_LLAMA_SERVER.",
                llama.server.display()
            ),
        ),
    };
    vec![
        BackendStatus {
            id: "mlx",
            label: "MLX (PWR engine)",
            format: "mlx",
            active: active == BackendKind::Mlx,
            available: mlx_status.0,
            detail: mlx_status.1,
            models_root: mlx.models_root.display().to_string(),
        },
        BackendStatus {
            id: "llama",
            label: "llama.cpp",
            format: "gguf",
            active: active == BackendKind::Llama,
            available: llama_status.0,
            detail: llama_status.1,
            models_root: llama.models_root.display().to_string(),
        },
    ]
}

async fn run_quietly(program: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = tokio::time::timeout(
        Duration::from_secs(8),
        tokio::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let text = if text.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
    } else {
        text
    };
    Some(text)
}
