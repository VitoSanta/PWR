//! Versioned, provider-independent PWR domain contracts.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path};
use thiserror::Error;
use uuid::Uuid;

pub mod reasoning;
pub use reasoning::{
    ANSWER_RESERVE_MIN, BudgetSource, GenerationEnvelope, MIN_USEFUL_REASONING, ReasoningBudgets,
    ReasoningCapability, ReasoningDirective, ReasoningEffort, ReasoningEvidence, ReasoningPlan,
    ReasoningProfile, TemplateReasoning, TokenAccounting, plan_finalization, plan_reasoning,
};

fn unclassified_kind() -> String {
    "unclassified".to_string()
}

pub const SCHEMA_VERSION: u32 = 1;

/// Refuses an artifact written by a schema this build does not know.
///
/// Every persisted contract carries a `schema_version` and nothing compared it
/// against the version in force, so an artifact from another build
/// deserialised whenever its shape happened to fit -- which is the case where
/// silence is worst, because the fields that changed are exactly the ones that
/// would be read wrongly.
///
/// An older version is refused rather than migrated: there are no migrations
/// yet, and reading one as though it were current is the failure this exists
/// to prevent.
pub fn check_schema_version(found: u32, artifact: &'static str) -> Result<(), DomainError> {
    if found == SCHEMA_VERSION {
        return Ok(());
    }
    Err(DomainError::Invalid {
        field: "schema_version",
        reason: format!(
            "{artifact} declares schema version {found}, and this build reads {SCHEMA_VERSION}"
        ),
    })
}

pub type Id = Uuid;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("invalid {field}: {reason}")]
    Invalid { field: &'static str, reason: String },
    #[error("incompatible {left} and {right}")]
    Incompatible {
        left: &'static str,
        right: &'static str,
    },
}

pub trait Validate {
    fn validate(&self) -> Result<(), DomainError>;
}

/// A time-ordered identifier.
///
/// `Uuid::now_v7()` keeps a process-wide reseeding counter (uuid 1.26's
/// `SharedContextV7`), so ids taken inside one millisecond are ordered and
/// distinct, and a clock that steps backwards reuses the larger timestamp
/// rather than going back. Read from the installed source on 2026-09-23 after
/// the invariant test below failed once under load; a probe of a million ids,
/// in one thread and across four, found no violation, so the cause of that
/// failure is **not explained** and wrapping this in a second context of our
/// own -- which is the same mutex and the same counter -- would only have hidden
/// it. If it recurs, keep the ids it failed on.
pub fn new_id() -> Id {
    Uuid::now_v7()
}

/// An id that is the same whenever its content is the same.
///
/// For records that are decided rather than observed -- a computed execution
/// profile is one -- where two decisions from identical inputs must carry the
/// same id, or the evaluator would refuse to pair two campaigns that ran under
/// the same conditions.
pub fn id_from_content(value: impl AsRef<[u8]>) -> Id {
    let digest = blake3::hash(value.as_ref());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    uuid::Builder::from_custom_bytes(bytes).into_uuid()
}
pub fn now() -> DateTime<Utc> {
    Utc::now()
}
pub fn hash_bytes(value: impl AsRef<[u8]>) -> String {
    blake3::hash(value.as_ref()).to_hex().to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    pub source: String,
    pub observed_at: DateTime<Utc>,
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelDefinition {
    pub schema_version: u32,
    pub id: Id,
    pub digest: String,
    pub family: Option<String>,
    pub quantization: Option<String>,
    pub capabilities: BTreeMap<String, Observation>,
    pub metadata: serde_json::Value,
    pub provenance: Provenance,
}

/// An immutable model artifact suitable for one local runtime. This is model
/// distribution metadata, not evidence that the artifact can complete agent
/// tasks; certification remains separate and backend/hardware scoped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelArtifact {
    pub schema_version: u32,
    pub id: String,
    pub family: String,
    pub variant: String,
    pub source: ArtifactSource,
    pub format: ArtifactFormat,
    pub quantization: Option<String>,
    pub platform: ArtifactPlatform,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactDownloadPlan {
    pub artifact_id: String,
    pub repository: String,
    pub revision: String,
    pub destination_root: String,
    pub files: Vec<ArtifactDownloadFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactDownloadFile {
    pub file: String,
    pub url: String,
    pub destination: String,
    pub expected_bytes: Option<u64>,
    pub blake3: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ArtifactFile {
    Path(String),
    Described {
        path: String,
        bytes: Option<u64>,
        blake3: Option<String>,
        sha256: Option<String>,
    },
}

impl ArtifactFile {
    pub fn path(&self) -> &str {
        match self {
            ArtifactFile::Path(path) => path,
            ArtifactFile::Described { path, .. } => path,
        }
    }

    pub fn expected_bytes(&self) -> Option<u64> {
        match self {
            ArtifactFile::Path(_) => None,
            ArtifactFile::Described { bytes, .. } => *bytes,
        }
    }

    pub fn blake3(&self) -> Option<&str> {
        match self {
            ArtifactFile::Path(_) => None,
            ArtifactFile::Described { blake3, .. } => blake3.as_deref(),
        }
    }

    pub fn sha256(&self) -> Option<&str> {
        match self {
            ArtifactFile::Path(_) => None,
            ArtifactFile::Described { sha256, .. } => sha256.as_deref(),
        }
    }
}

impl From<String> for ArtifactFile {
    fn from(path: String) -> Self {
        ArtifactFile::Path(path)
    }
}

impl From<&str> for ArtifactFile {
    fn from(path: &str) -> Self {
        ArtifactFile::Path(path.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ArtifactSource {
    HuggingFace {
        repository: String,
        /// Full immutable Hub commit, never a moving branch or tag.
        revision: String,
        files: Vec<ArtifactFile>,
    },
    LocalPath {
        path: String,
    },
    BackendManaged {
        backend: String,
        model_ref: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFormat {
    Mlx,
    Gguf,
    Safetensors,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactPlatform {
    Any,
    MacosAppleSilicon,
    WindowsX86_64,
    LinuxX86_64,
}

/// Persisted inventory of downloadable or backend-managed model artifacts.
///
/// It deliberately returns every eligible artifact rather than silently
/// choosing one: memory admission and measured capability evidence decide the
/// final deployment, and neither can be inferred from distribution metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRegistry {
    pub schema_version: u32,
    pub artifacts: Vec<ModelArtifact>,
}

impl ModelArtifact {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.id.trim().is_empty()
            || self.family.trim().is_empty()
            || self.variant.trim().is_empty()
        {
            return Err(DomainError::Invalid {
                field: "model_artifact",
                reason: "id, family, and variant are required".into(),
            });
        }
        match &self.source {
            ArtifactSource::HuggingFace {
                repository,
                revision,
                files,
            } => {
                if !repository.contains('/')
                    || revision.len() != 40
                    || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
                    || files.is_empty()
                    || files.iter().any(|file| file.path().trim().is_empty())
                    || files
                        .iter()
                        .any(|file| !safe_relative_artifact_file(file.path()))
                    || files.iter().any(|file| {
                        file.blake3().is_some_and(|hash| {
                            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    })
                    || files.iter().any(|file| {
                        file.sha256().is_some_and(|hash| {
                            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                        })
                    })
                {
                    return Err(DomainError::Invalid {
                        field: "model_artifact",
                        reason:
                            "Hugging Face artifacts require repo, full commit revision, safe relative files, and valid hashes when present"
                                .into(),
                    });
                }
            }
            ArtifactSource::LocalPath { path } if path.trim().is_empty() => {
                return Err(DomainError::Invalid {
                    field: "model_artifact",
                    reason: "local artifact path is required".into(),
                });
            }
            ArtifactSource::BackendManaged { backend, model_ref }
                if backend.trim().is_empty() || model_ref.trim().is_empty() =>
            {
                return Err(DomainError::Invalid {
                    field: "model_artifact",
                    reason: "backend and model reference are required".into(),
                });
            }
            _ => {}
        }
        Ok(())
    }

    pub fn supports(&self, platform: ArtifactPlatform) -> bool {
        self.platform == ArtifactPlatform::Any || self.platform == platform
    }

    pub fn download_plan(
        &self,
        destination_root: &Path,
    ) -> Result<Option<ArtifactDownloadPlan>, DomainError> {
        self.download_plan_from_base(destination_root, "https://huggingface.co")
    }

    pub fn download_plan_from_base(
        &self,
        destination_root: &Path,
        base_url: &str,
    ) -> Result<Option<ArtifactDownloadPlan>, DomainError> {
        let ArtifactSource::HuggingFace {
            repository,
            revision,
            files,
        } = &self.source
        else {
            return Ok(None);
        };
        self.validate()?;
        let destination_root = destination_root.join(repository);
        Ok(Some(ArtifactDownloadPlan {
            artifact_id: self.id.clone(),
            repository: repository.clone(),
            revision: revision.clone(),
            destination_root: destination_root.display().to_string(),
            files: files
                .iter()
                .map(|file| ArtifactDownloadFile {
                    file: file.path().to_string(),
                    url: format!(
                        "{}/{repository}/resolve/{revision}/{}",
                        base_url.trim_end_matches('/'),
                        file.path()
                    ),
                    destination: destination_root.join(file.path()).display().to_string(),
                    expected_bytes: file.expected_bytes(),
                    blake3: file.blake3().map(str::to_string),
                    sha256: file.sha256().map(str::to_string),
                })
                .collect(),
        }))
    }
}

fn safe_relative_artifact_file(file: &str) -> bool {
    let path = Path::new(file);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

impl ArtifactRegistry {
    pub fn validate(&self) -> Result<(), DomainError> {
        check_schema_version(self.schema_version, "artifact registry")?;
        let mut ids = std::collections::BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            if !ids.insert(&artifact.id) {
                return Err(DomainError::Invalid {
                    field: "artifact_registry",
                    reason: format!("duplicate artifact id {}", artifact.id),
                });
            }
        }
        Ok(())
    }

    /// Candidates compatible with one host and optional model family, in the
    /// registry's stable declared order. An empty result is a useful answer:
    /// callers must then download a compatible artifact instead of trying a
    /// mismatched binary format.
    pub fn eligible_for<'a>(
        &'a self,
        platform: ArtifactPlatform,
        family: Option<&str>,
    ) -> Result<Vec<&'a ModelArtifact>, DomainError> {
        self.validate()?;
        Ok(self
            .artifacts
            .iter()
            .filter(|artifact| artifact.supports(platform))
            .filter(|artifact| family.is_none_or(|family| artifact.family == family))
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", content = "value", rename_all = "snake_case")]
pub enum Observation {
    Observed(serde_json::Value),
    Unknown { reason: String },
}

/// Where a parameter's value came from.
///
/// Recorded per parameter because a run that reports a value without its
/// origin cannot be compared with another: a temperature the vendor
/// recommends, one a package happened to ship, and one nobody chose are three
/// different claims that look identical in a report.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ParameterSource {
    /// The vendor's published recommendation for this model.
    OfficialModelCard,
    /// Declared in the backend-packaged model configuration.
    ///
    /// `ollama_model` remains accepted while existing profile artifacts are
    /// migrated; new artifacts serialize the backend-neutral name.
    #[serde(rename = "backend_model_config", alias = "ollama_model")]
    BackendModelConfig,
    /// The `generation_config.json` shipped with the weights: the model's own
    /// defaults, read by an engine that loads the weights itself.
    ModelGenerationConfig,
    /// Chosen by PWR deliberately, against a stated reason.
    #[serde(rename = "pwr_override")]
    PwrOverride,
    /// Derived from measurement on this machine.
    HardwareCalibration,
    /// Nothing set it. The backend decides, and we do not know what it decides.
    BackendDefault,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedParameter {
    pub value: serde_json::Value,
    pub source: ParameterSource,
}

/// How a deployment's reasoning depth is controlled.
///
/// Three different mechanisms, named separately because they are not
/// interchangeable: one is a backend option, one is a line the system prompt
/// must carry, and one is a request field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReasoningControl {
    /// A backend option, such as `reasoning_effort`.
    BackendOption { name: String, value: String },
    /// A directive the system prompt must contain, such as a reasoning
    /// strength line.
    PromptDirective { text: String },
    /// The backend's own thinking toggle.
    Think { enabled: bool },
}

/// Graded reasoning-effort levels, lowest first.
///
/// Named rather than inferred from the order a backend lists them in: LM
/// Studio reports `[off, low, medium, xhigh, on]`, where `on` is the model's
/// own default rather than the highest step, so position says nothing about
/// how much work a level asks for.
const REASONING_EFFORT_LEVELS: [&str; 5] = ["minimal", "low", "medium", "high", "xhigh"];

/// The least reasoning a deployment will do while still reasoning.
///
/// `None` when the deployment offers no graded levels -- only on and off --
/// because there is then nothing to choose and asking for a level it never
/// advertised is a guess.
///
/// Why the lowest: measured on a 27B whose default is `xhigh`, one prompt
/// asking for a small function generated 292 reasoning tokens at `low` and
/// 3,999 at the default, which was the token ceiling rather than the end of
/// its thinking -- 30 seconds against 338. A benchmark of reasoning wants the
/// high setting; an agent that must take fifty actions cannot afford it, and
/// the first run against that deployment died on a client timeout mid-thought.
pub fn lowest_reasoning_effort(allowed: &[String]) -> Option<&'static str> {
    let offered: std::collections::BTreeSet<&str> = allowed.iter().map(String::as_str).collect();
    REASONING_EFFORT_LEVELS
        .into_iter()
        .find(|level| offered.contains(level))
}

/// Context sizes for one deployment tag.
///
/// Per tag rather than per family: the same model published under different
/// tags can declare different limits, and a family-level number would be wrong
/// for at least one of them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextPolicy {
    /// Below this an agent is too constrained to work; a run refuses rather
    /// than proceeding quietly.
    pub minimum: u32,
    /// Normal allocation.
    pub default: u32,
    /// The tag's declared limit.
    pub maximum: u32,
}

impl ContextPolicy {
    /// A policy whose sizes contradict each other would clamp to something
    /// nobody chose.
    pub fn is_coherent(&self) -> bool {
        self.minimum <= self.default && self.default <= self.maximum
    }
}

/// Everything about how one deployment should be driven.
///
/// Separate from `ModelDefinition`, which is facts the backend reported, and
/// from `ModelStrategy`, which is how the agent behaves. This is how the
/// request is built.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelProfile {
    pub schema_version: u32,
    /// Matched against the deployment's model reference, exactly.
    pub model_selector: String,
    /// Additional identity selectors. The legacy exact tag remains supported
    /// through `model_selector` while profiles are incrementally migrated.
    #[serde(default)]
    pub selectors: Vec<ModelProfileSelector>,
    pub context: ContextPolicy,
    /// Sampling options sent to the backend, each with its origin.
    pub sampling: BTreeMap<String, ResolvedParameter>,
    #[serde(default)]
    pub reasoning: Option<ReasoningControl>,
    /// Optional measured budget mapping for this exact deployment. A family
    /// selector alone must not turn this into verified evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_budgets: Option<ReasoningBudgets>,
    /// Where the context default came from. A size measured on this machine is
    /// a different claim from one copied out of a specification.
    #[serde(default = "declared_source")]
    pub context_source: ParameterSource,
    /// Where these values came from, in words a reader can check.
    pub provenance: String,
}

impl ModelProfile {
    pub fn select<'a>(profiles: &'a [ModelProfile], model_ref: &str) -> Option<&'a ModelProfile> {
        profiles
            .iter()
            .find(|profile| profile.model_selector == model_ref)
    }

    /// Selects with stable evidence precedence: exact digest, deployment,
    /// family, then the legacy exact tag. Ties are intentionally rejected so
    /// profile choice never depends on file order.
    pub fn select_for<'a>(
        profiles: &'a [ModelProfile],
        identity: &DeploymentIdentity,
    ) -> Option<&'a ModelProfile> {
        let ranked: Vec<(&ModelProfile, u8)> = profiles
            .iter()
            .filter_map(|profile| profile.match_rank(identity).map(|rank| (profile, rank)))
            .collect();
        let best = ranked.iter().map(|(_, rank)| *rank).max()?;
        let mut winners = ranked.into_iter().filter(|(_, rank)| *rank == best);
        let winner = winners.next()?.0;
        winners.next().is_none().then_some(winner)
    }

    fn match_rank(&self, identity: &DeploymentIdentity) -> Option<u8> {
        self.selectors
            .iter()
            .filter_map(|selector| selector.match_rank(identity))
            .max()
            .or_else(|| (self.model_selector == identity.model_ref).then_some(10))
    }

    /// The context to allocate, bounded by the tag's own limits.
    ///
    /// A request for more than the tag declares is clamped rather than sent:
    /// the backend would either refuse it or silently ignore it, and both make
    /// the recorded number a fiction.
    pub fn context_for(&self, requested: Option<u32>) -> u32 {
        requested
            .unwrap_or(self.context.default)
            .clamp(self.context.minimum, self.context.maximum)
    }

    /// Sampling options as the backend expects them.
    pub fn sampling_options(&self) -> BTreeMap<String, serde_json::Value> {
        self.sampling
            .iter()
            .map(|(name, resolved)| (name.clone(), resolved.value.clone()))
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelProfileSelector {
    Digest { digest: String },
    Deployment { provider: String, model_ref: String },
    Family { family: String },
}

impl ModelProfileSelector {
    fn match_rank(&self, identity: &DeploymentIdentity) -> Option<u8> {
        match self {
            Self::Digest { digest } if identity.digest.as_deref() == Some(digest) => Some(40),
            Self::Deployment {
                provider,
                model_ref,
            } if provider == &identity.provider && model_ref == &identity.model_ref => Some(30),
            Self::Family { family } if identity.family.as_deref() == Some(family) => Some(20),
            _ => None,
        }
    }
}

fn declared_source() -> ParameterSource {
    ParameterSource::OfficialModelCard
}

/// Policy for one deployment, as opposed to facts about it.
///
/// Measured differences between deployments are large and do not point the same
/// way: one prompt change moved one deployment by seven tasks and another by
/// none. A single-prompt harness cannot express that, so this is what a run
/// consults instead of a constant.
///
/// A strategy is a hypothesis until it is measured against the default. Nothing
/// here is evidence on its own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelStrategy {
    pub schema_version: u32,
    pub id: Id,
    /// Matched against the deployment's model reference, exactly.
    pub model_selector: String,
    pub role: String,
    /// Appended to the shared system prompt. Empty means the shared one alone.
    #[serde(default)]
    pub prompt_suffix: String,
    /// Overrides the execution profile's action budget.
    #[serde(default)]
    pub max_actions: Option<u8>,
    /// Repository passages offered at the start.
    #[serde(default)]
    pub retrieval_excerpts: Option<usize>,
    /// Ask for a plan before acting. Costs a turn, so it is opt-in and must be
    /// measured against the default rather than assumed to help.
    #[serde(default)]
    pub plan_first: bool,
    /// Why this strategy exists, and what measurement prompted it.
    pub rationale: String,
}

impl ModelStrategy {
    /// The strategy for a deployment, if one is declared.
    pub fn select<'a>(
        strategies: &'a [ModelStrategy],
        model_ref: &str,
    ) -> Option<&'a ModelStrategy> {
        strategies
            .iter()
            .find(|strategy| strategy.model_selector == model_ref)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentDescriptor {
    pub schema_version: u32,
    pub id: Id,
    pub provider: String,
    pub endpoint: String,
    pub model_ref: String,
    pub backend_options: BTreeMap<String, String>,
    pub auth_ref: Option<String>,
}

/// Backend-neutral identity used for profile/strategy matching. The digest is
/// preferred because tags are mutable; family is deliberately the lowest
/// confidence selector and must never silently beat a deployment match.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeploymentIdentity {
    pub provider: String,
    pub model_ref: String,
    pub digest: Option<String>,
    pub family: Option<String>,
}

impl DeploymentIdentity {
    pub fn from_inspection(
        deployment: &DeploymentDescriptor,
        definition: &ModelDefinition,
    ) -> Self {
        Self {
            provider: deployment.provider.clone(),
            model_ref: deployment.model_ref.clone(),
            digest: (!definition.digest.is_empty()).then_some(definition.digest.clone()),
            family: definition.family.clone(),
        }
    }
}
impl DeploymentDescriptor {
    pub fn fingerprint(&self) -> String {
        let safe = serde_json::json!({"provider": self.provider, "endpoint": self.endpoint, "model_ref": self.model_ref, "backend_options": self.backend_options});
        hash_bytes(serde_json::to_vec(&safe).expect("JSON serializable"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareProfile {
    pub schema_version: u32,
    pub id: Id,
    pub compatibility_key: String,
    pub os: String,
    pub architecture: String,
    pub cpu: String,
    pub accelerators: Vec<String>,
    pub total_memory_bytes: Option<u64>,
    pub storage_free_bytes: Option<u64>,
    pub unavailable_fields: Vec<String>,
    pub probe_version: String,
    pub provenance: Provenance,
}

impl HardwareProfile {
    /// The artifact format target inferred from OS and CPU architecture. An
    /// unknown host does not become `Any`: callers must treat it as an
    /// admission failure until an operator supplies an explicit policy.
    pub fn artifact_platform(&self) -> Option<ArtifactPlatform> {
        let os = self.os.to_ascii_lowercase();
        let architecture = self.architecture.to_ascii_lowercase();
        match (os.as_str(), architecture.as_str()) {
            ("darwin" | "macos", "arm64" | "aarch64") => Some(ArtifactPlatform::MacosAppleSilicon),
            ("windows" | "windows_nt", "x86_64" | "amd64") => Some(ArtifactPlatform::WindowsX86_64),
            ("linux", "x86_64" | "amd64") => Some(ArtifactPlatform::LinuxX86_64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSnapshot {
    pub schema_version: u32,
    pub id: Id,
    pub hardware_id: Id,
    pub deployment_id: Id,
    pub timestamp: DateTime<Utc>,
    pub available_memory_bytes: Option<u64>,
    pub pressure: Observation,
    pub loaded_models: Vec<String>,
    pub backend_state: serde_json::Value,
}

/// Explicit policy used to turn observed memory into an admission budget.
///
/// The policy is intentionally separate from `HardwareProfile`: one machine
/// can reserve more memory for the operating system when it is also used for
/// development, or less on a dedicated appliance.  All values are bytes and
/// are recorded with the resulting budget so a later selection is explainable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceBudgetPolicy {
    pub os_reservation_bytes: u64,
    pub pwr_reservation_bytes: u64,
    /// Portion of the remaining budget available to the model weights. The
    /// remainder is retained for context/KV cache and request overhead.
    pub model_memory_per_mille: u16,
    pub max_concurrency: u16,
}

impl Default for ResourceBudgetPolicy {
    fn default() -> Self {
        Self {
            // Conservative defaults are policy, not a claim about every OS.
            os_reservation_bytes: 4 * 1024 * 1024 * 1024,
            pwr_reservation_bytes: 1024 * 1024 * 1024,
            model_memory_per_mille: 750,
            max_concurrency: 1,
        }
    }
}

impl ResourceBudgetPolicy {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.model_memory_per_mille == 0 || self.model_memory_per_mille > 1_000 {
            return Err(DomainError::Invalid {
                field: "resource_budget_policy",
                reason: "model_memory_per_mille must be between 1 and 1000".into(),
            });
        }
        if self.max_concurrency == 0 {
            return Err(DomainError::Invalid {
                field: "resource_budget_policy",
                reason: "max_concurrency must be greater than zero".into(),
            });
        }
        Ok(())
    }
}

/// An admission budget derived from a hardware profile and an optional fresh
/// runtime observation. It is not a measurement of a model's actual memory
/// use; the selector must still require calibration evidence where policy
/// calls for it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceBudget {
    pub schema_version: u32,
    pub hardware_id: Id,
    pub compatibility_key: String,
    pub available_memory_bytes: u64,
    pub os_reservation_bytes: u64,
    pub pwr_reservation_bytes: u64,
    pub model_memory_budget_bytes: u64,
    pub context_memory_budget_bytes: u64,
    pub max_concurrency: u16,
    pub policy: ResourceBudgetPolicy,
}

impl ResourceBudget {
    /// Derive a conservative local budget. Missing memory facts are an error,
    /// never an implicit unlimited budget. A snapshot only supplies a fresher
    /// availability value when it belongs to this hardware profile.
    pub fn derive(
        hardware: &HardwareProfile,
        snapshot: Option<&RuntimeSnapshot>,
        policy: ResourceBudgetPolicy,
    ) -> Result<Self, DomainError> {
        check_schema_version(hardware.schema_version, "hardware profile")?;
        policy.validate()?;
        if let Some(snapshot) = snapshot {
            check_schema_version(snapshot.schema_version, "runtime snapshot")?;
            if snapshot.hardware_id != hardware.id {
                return Err(DomainError::Incompatible {
                    left: "runtime snapshot",
                    right: "hardware profile",
                });
            }
        }
        let available_memory_bytes = snapshot
            .and_then(|snapshot| snapshot.available_memory_bytes)
            .or(hardware.total_memory_bytes)
            .ok_or_else(|| DomainError::Invalid {
                field: "resource_budget",
                reason: "available or total memory must be observed before admission".into(),
            })?;
        let reservations = policy
            .os_reservation_bytes
            .checked_add(policy.pwr_reservation_bytes)
            .ok_or_else(|| DomainError::Invalid {
                field: "resource_budget_policy",
                reason: "memory reservations overflow".into(),
            })?;
        let usable = available_memory_bytes
            .checked_sub(reservations)
            .ok_or_else(|| DomainError::Invalid {
                field: "resource_budget",
                reason: "available memory does not cover OS and PWR reservations".into(),
            })?;
        let model_memory_budget_bytes = usable
            .checked_mul(u64::from(policy.model_memory_per_mille))
            .expect("per-mille multiplication fits u64")
            / 1_000;
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            hardware_id: hardware.id,
            compatibility_key: hardware.compatibility_key.clone(),
            available_memory_bytes,
            os_reservation_bytes: policy.os_reservation_bytes,
            pwr_reservation_bytes: policy.pwr_reservation_bytes,
            model_memory_budget_bytes,
            context_memory_budget_bytes: usable - model_memory_budget_bytes,
            max_concurrency: policy.max_concurrency,
            policy,
        })
    }

    pub fn admits_model_bytes(&self, bytes: Option<u64>) -> bool {
        bytes.is_some_and(|bytes| bytes <= self.model_memory_budget_bytes)
    }
}

/// User intent for opt-in automatic selection. It changes only a tie-break
/// score; hard constraints and explicit deployment choices always win.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PerformancePreference {
    Fast,
    Balanced,
    Quality,
    MaxQuality,
}

/// Minimum capabilities required for one task before a deployment can enter
/// automatic scoring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRequirements {
    pub minimum_context_tokens: u32,
    pub requires_tools: bool,
    pub requires_streaming: bool,
}

/// Evidence collected without assigning it a model-family-specific meaning.
/// Scores are normalized only within a matching certification scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CandidateEvidence {
    pub quality_score: Option<u16>,
    pub speed_score: Option<u16>,
    pub calibrated: bool,
}

/// One discovered deployment considered by automatic selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionCandidate {
    pub deployment: DeploymentDescriptor,
    pub model_digest: Option<String>,
    pub profile_id: Option<String>,
    pub context_limit_tokens: Option<u32>,
    pub estimated_model_memory_bytes: Option<u64>,
    pub supports_tools: Observation,
    pub supports_streaming: Observation,
    pub certification: CertificationLevel,
    pub evidence: CandidateEvidence,
}

/// A deliberate override has priority over automatic routing. A selector can
/// still reject the named candidate and explain why; it never substitutes a
/// different one behind the user's back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SelectionOverride {
    pub provider: Option<String>,
    pub model_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionRequest {
    pub requirements: TaskRequirements,
    pub preference: PerformancePreference,
    pub override_: SelectionOverride,
    /// Experimental candidates are usable only when the caller deliberately
    /// opts in (for example an explicit generic local deployment).
    pub allow_experimental: bool,
}

/// A machine-readable admission or scoring explanation. These are persisted
/// with a selection so UI text can evolve without losing the reason.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectionReason {
    ExplicitOverrideMatched,
    ContextSatisfied {
        required: u32,
        available: u32,
    },
    CapabilitySatisfied {
        capability: String,
    },
    MemorySatisfied {
        required: u64,
        available: u64,
    },
    Certification {
        level: CertificationLevel,
    },
    EvidenceScore {
        quality: u16,
        speed: u16,
        calibrated: bool,
    },
    Rejected {
        provider: String,
        model_ref: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelSelection {
    pub deployment: DeploymentDescriptor,
    pub profile_id: Option<String>,
    pub context_budget_tokens: u32,
    pub confidence: CertificationLevel,
    pub reasons: Vec<SelectionReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectionOutcome {
    pub selection: Option<ModelSelection>,
    pub reasons: Vec<SelectionReason>,
}

impl SelectionCandidate {
    fn rejected(&self, reason: impl Into<String>) -> SelectionReason {
        SelectionReason::Rejected {
            provider: self.deployment.provider.clone(),
            model_ref: self.deployment.model_ref.clone(),
            reason: reason.into(),
        }
    }
}

impl SelectionRequest {
    /// Pick one deployment deterministically. Unknown memory or capability
    /// observations never pass a hard requirement; callers may use explicit
    /// generic mode separately rather than calling this method.
    pub fn select(
        &self,
        candidates: &[SelectionCandidate],
        budget: &ResourceBudget,
    ) -> SelectionOutcome {
        let mut rejected = Vec::new();
        let mut admitted = Vec::new();
        for candidate in candidates {
            if self
                .override_
                .provider
                .as_ref()
                .is_some_and(|provider| provider != &candidate.deployment.provider)
                || self
                    .override_
                    .model_ref
                    .as_ref()
                    .is_some_and(|model_ref| model_ref != &candidate.deployment.model_ref)
            {
                continue;
            }
            let mut reasons = Vec::new();
            if self.override_.provider.is_some() || self.override_.model_ref.is_some() {
                reasons.push(SelectionReason::ExplicitOverrideMatched);
            }
            let Some(context) = candidate.context_limit_tokens else {
                rejected.push(candidate.rejected("context limit is unknown"));
                continue;
            };
            if context < self.requirements.minimum_context_tokens {
                rejected.push(candidate.rejected(format!(
                    "requires {} context tokens but candidate declares {context}",
                    self.requirements.minimum_context_tokens
                )));
                continue;
            }
            if self.requirements.requires_tools
                && candidate.supports_tools != Observation::Observed(serde_json::json!(true))
            {
                rejected.push(candidate.rejected("required tool calling is not observed"));
                continue;
            }
            if self.requirements.requires_streaming
                && candidate.supports_streaming != Observation::Observed(serde_json::json!(true))
            {
                rejected.push(candidate.rejected("required streaming is not observed"));
                continue;
            }
            let Some(memory) = candidate.estimated_model_memory_bytes else {
                rejected.push(candidate.rejected("model memory estimate is unknown"));
                continue;
            };
            if !budget.admits_model_bytes(Some(memory)) {
                rejected.push(candidate.rejected(format!(
                    "requires {memory} model bytes but budget permits {}",
                    budget.model_memory_budget_bytes
                )));
                continue;
            }
            if candidate.certification == CertificationLevel::Unsupported {
                rejected.push(candidate.rejected("deployment is unsupported"));
                continue;
            }
            if candidate.certification == CertificationLevel::Experimental
                && !self.allow_experimental
            {
                rejected.push(candidate.rejected("experimental evidence requires explicit opt-in"));
                continue;
            }
            reasons.push(SelectionReason::ContextSatisfied {
                required: self.requirements.minimum_context_tokens,
                available: context,
            });
            reasons.push(SelectionReason::MemorySatisfied {
                required: memory,
                available: budget.model_memory_budget_bytes,
            });
            if self.requirements.requires_tools {
                reasons.push(SelectionReason::CapabilitySatisfied {
                    capability: "tool_calling".into(),
                });
            }
            if self.requirements.requires_streaming {
                reasons.push(SelectionReason::CapabilitySatisfied {
                    capability: "streaming".into(),
                });
            }
            reasons.push(SelectionReason::Certification {
                level: candidate.certification,
            });
            reasons.push(SelectionReason::EvidenceScore {
                quality: candidate.evidence.quality_score.unwrap_or_default(),
                speed: candidate.evidence.speed_score.unwrap_or_default(),
                calibrated: candidate.evidence.calibrated,
            });
            let preference_score = match self.preference {
                PerformancePreference::Fast => candidate.evidence.speed_score.unwrap_or_default(),
                PerformancePreference::Balanced => candidate
                    .evidence
                    .quality_score
                    .unwrap_or_default()
                    .saturating_add(candidate.evidence.speed_score.unwrap_or_default()),
                PerformancePreference::Quality | PerformancePreference::MaxQuality => {
                    candidate.evidence.quality_score.unwrap_or_default()
                }
            };
            let certification_score = candidate.certification.rank();
            admitted.push((
                certification_score,
                candidate.evidence.calibrated,
                preference_score,
                candidate.deployment.provider.clone(),
                candidate.deployment.model_ref.clone(),
                candidate,
                reasons,
            ));
        }
        // Stable field-based tie breaking is intentional: input ordering must
        // never determine a production deployment.
        admitted.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| right.1.cmp(&left.1))
                .then_with(|| right.2.cmp(&left.2))
                .then_with(|| left.3.cmp(&right.3))
                .then_with(|| left.4.cmp(&right.4))
        });
        let Some((_, _, _, _, _, candidate, reasons)) = admitted.into_iter().next() else {
            return SelectionOutcome {
                selection: None,
                reasons: rejected,
            };
        };
        SelectionOutcome {
            selection: Some(ModelSelection {
                deployment: candidate.deployment.clone(),
                profile_id: candidate.profile_id.clone(),
                context_budget_tokens: self.requirements.minimum_context_tokens,
                confidence: candidate.certification,
                reasons,
            }),
            reasons: rejected,
        }
    }
}

/// Certification status is scoped evidence, never a permanent model badge.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CertificationLevel {
    Unsupported,
    Experimental,
    Compatible,
    Certified,
}

impl CertificationLevel {
    fn rank(self) -> u8 {
        match self {
            Self::Unsupported => 0,
            Self::Experimental => 1,
            Self::Compatible => 2,
            Self::Certified => 3,
        }
    }
}

/// Everything that can invalidate a certification result. Matching one field
/// less would let a result for a different backend, adapter or harness be
/// advertised as evidence for this deployment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationScope {
    pub model_digest: String,
    pub deployment_fingerprint: String,
    pub backend_version: String,
    pub adapter_version: String,
    pub hardware_compatibility_key: String,
    pub harness_rev: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationRecord {
    pub schema_version: u32,
    pub id: Id,
    pub scope: CertificationScope,
    pub level: CertificationLevel,
    pub evaluation_run_ids: Vec<Id>,
    pub artifact_hashes: Vec<String>,
    pub rationale: String,
    pub issued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationRegistry {
    pub schema_version: u32,
    pub records: Vec<CertificationRecord>,
}

/// A stable, serializable report suitable for CLI or an artifact store. The
/// caller owns rendering; this contract retains the exact applicable evidence
/// and whether a matching result is absent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationReport {
    pub scope: CertificationScope,
    pub effective: Option<CertificationRecord>,
    pub matching_records: Vec<CertificationRecord>,
}

impl CertificationScope {
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.model_digest.trim().is_empty()
            || self.deployment_fingerprint.trim().is_empty()
            || self.backend_version.trim().is_empty()
            || self.adapter_version.trim().is_empty()
            || self.hardware_compatibility_key.trim().is_empty()
            || self.harness_rev.trim().is_empty()
        {
            return Err(DomainError::Invalid {
                field: "certification_scope",
                reason: "model, deployment, backend, adapter, hardware, and harness are required"
                    .into(),
            });
        }
        Ok(())
    }
}

impl CertificationRecord {
    pub fn validate(&self) -> Result<(), DomainError> {
        check_schema_version(self.schema_version, "certification record")?;
        self.scope.validate()?;
        if self.rationale.trim().is_empty() {
            return Err(DomainError::Invalid {
                field: "certification_record",
                reason: "rationale is required".into(),
            });
        }
        if matches!(
            self.level,
            CertificationLevel::Compatible | CertificationLevel::Certified
        ) && (self.evaluation_run_ids.is_empty() || self.artifact_hashes.is_empty())
        {
            return Err(DomainError::Invalid {
                field: "certification_record",
                reason: "compatible and certified records require evaluation and raw artifacts"
                    .into(),
            });
        }
        Ok(())
    }
}

impl CertificationRegistry {
    pub fn validate(&self) -> Result<(), DomainError> {
        check_schema_version(self.schema_version, "certification registry")?;
        let mut ids = std::collections::BTreeSet::new();
        for record in &self.records {
            record.validate()?;
            if !ids.insert(record.id) {
                return Err(DomainError::Invalid {
                    field: "certification_registry",
                    reason: format!("duplicate certification id {}", record.id),
                });
            }
        }
        Ok(())
    }

    /// Return every exact scope match and its effective newest record. A later
    /// regression is a demotion because it is newer; historical certified
    /// evidence remains visible in the report but cannot mask it.
    pub fn report(&self, scope: &CertificationScope) -> Result<CertificationReport, DomainError> {
        self.validate()?;
        scope.validate()?;
        let mut matching_records: Vec<_> = self
            .records
            .iter()
            .filter(|record| record.scope == *scope)
            .cloned()
            .collect();
        matching_records.sort_by(|left, right| {
            right
                .issued_at
                .cmp(&left.issued_at)
                .then_with(|| right.id.cmp(&left.id))
        });
        Ok(CertificationReport {
            scope: scope.clone(),
            effective: matching_records.first().cloned(),
            matching_records,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StablePoint {
    pub context_tokens: u32,
    pub samples: u32,
    pub success_rate: f64,
    pub median_first_token_ms: f64,
    pub generation_tokens_per_second: f64,
    pub variance: f64,
    pub memory_pressure_observed: bool,
}

/// Admission criteria a measured point must meet to count as stable.
///
/// Stored on the profile so the evidence carries the standard it was judged
/// against: a reader can check the points rather than trust them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationThresholds {
    pub min_success_rate: f64,
    pub max_median_first_token_ms: f64,
    pub allow_memory_pressure: bool,
}
impl Default for CalibrationThresholds {
    fn default() -> Self {
        Self {
            // A tier that failed any sample is not a tier to operate at.
            min_success_rate: 1.0,
            max_median_first_token_ms: 120_000.0,
            allow_memory_pressure: false,
        }
    }
}
impl CalibrationThresholds {
    pub fn admits(&self, point: &StablePoint) -> bool {
        point.success_rate >= self.min_success_rate
            && point.median_first_token_ms <= self.max_median_first_token_ms
            && (self.allow_memory_pressure || !point.memory_pressure_observed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CalibrationProfile {
    pub schema_version: u32,
    pub id: Id,
    pub compatibility_key: String,
    pub model_digest: String,
    pub deployment_fingerprint: String,
    pub harness_rev: String,
    pub thresholds: CalibrationThresholds,
    pub stable_points: Vec<StablePoint>,
    pub raw_artifact_hashes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EvidenceLabel {
    Measured,
    ConservativeBootstrap,
    /// The window was computed from the model's config and the host's memory,
    /// not measured by a calibration ladder. Carries no calibration id, like a
    /// bootstrap profile, but unlike one it was chosen for this machine and
    /// says which ceiling bound it.
    Computed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionProfile {
    pub schema_version: u32,
    pub id: Id,
    pub strategy_id: Id,
    pub calibration_id: Option<Id>,
    pub context_tokens: u32,
    pub reserve_tokens: u32,
    pub concurrency: u16,
    pub budgets: serde_json::Value,
    pub rationale: String,
    pub evidence: EvidenceLabel,
    pub compatibility_key: String,
}

/// Bounded controller budgets carried by an execution profile.
///
/// This typed view keeps persisted JSON forwards-compatible while preventing
/// callers from silently substituting hard-coded recovery limits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionBudgets {
    pub max_actions: u8,
    pub edit_verify_cycles: u8,
    pub context_retries: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvaluationRun {
    pub schema_version: u32,
    pub id: Id,
    pub corpus_rev: String,
    pub task_set: String,
    pub execution_profile_id: Id,
    pub model_digest: String,
    pub deployment_fingerprint: String,
    pub hardware_compatibility_key: String,
    pub harness_rev: String,
    pub seeds: Vec<u64>,
    pub outcome_hash: String,
    pub artifact_hashes: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelInspection {
    pub definition: ModelDefinition,
    pub deployment: DeploymentDescriptor,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackendState {
    pub observed_at: DateTime<Utc>,
    pub loaded_models: Vec<String>,
    pub state: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelRequest {
    pub deployment: DeploymentDescriptor,
    pub messages: Vec<ChatMessage>,
    pub context_tokens: u32,
    pub tools: Option<serde_json::Value>,
    /// Sampling seed. A run that records a seed it never sent is not
    /// reproducible, whatever the record says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// Sampling options sent verbatim to the backend.
    ///
    /// A map rather than a field per parameter, because the set differs by
    /// vendor: one model recommends top_k and min_p, another recommends
    /// nothing, and inventing a value for a model whose vendor did not
    /// recommend one is a configuration nobody chose.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sampling: BTreeMap<String, serde_json::Value>,
}

/// Provider-independent description of a callable tool.
///
/// `input_schema` is canonical JSON Schema data, never a backend's enclosing
/// function/tool envelope. The compatibility layer owns rendering it for a
/// backend and parsing that backend's reply back into `ToolCall`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// The complete, ordered set of actions available for a model turn.
///
/// Ordering is preserved for reproducible prompts, while duplicate names are
/// refused because a model response could otherwise be valid under two
/// incompatible schemas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCatalog {
    pub tools: Vec<ToolDefinition>,
}

impl ToolCatalog {
    /// What a call that failed to decode got wrong, in terms the caller can act
    /// on: the fields the capability takes, the fields this call sent, and --
    /// when they are exactly some other capability's -- which one.
    ///
    /// serde says only what was missing. A deployment that sends the wrong
    /// field learns that the right one is absent and nothing else, which is
    /// true of the call it is about to make again. Measured twice, on two
    /// paths: in a conversation, an 80B called `apply_replace` with
    /// `apply_patch`'s `hunks` twenty-nine times running; in the capability
    /// probe, the same model failed all three edit trials with `missing field
    /// \`replacement\`` and was recorded as unmeasurable for a reason the
    /// conversation had already learned to explain.
    ///
    /// It lives here because every path that decodes a tool call needs it, and
    /// each one having its own answer is how the probe came to contradict the
    /// conversation about the same deployment.
    #[must_use]
    pub fn mismatch_hint(&self, called: &str, sent: &[String]) -> String {
        let properties = |name: &str| {
            self.tools
                .iter()
                .find(|tool| tool.name == name)
                .and_then(|tool| tool.input_schema.get("properties"))
                .and_then(serde_json::Value::as_object)
                .map(|fields| fields.keys().cloned().collect::<Vec<_>>())
        };
        let expected = properties(called).unwrap_or_default();
        if expected.is_empty() && sent.is_empty() {
            return String::new();
        }
        let list = |names: &[String]| {
            if names.is_empty() {
                "nothing".to_owned()
            } else {
                names.join(", ")
            }
        };
        let named = format!(
            "{called} takes {}, and this call sent {}",
            list(&expected),
            list(sent)
        );
        // Named only on an exact match -- same fields, no more and no fewer.
        // A loose one would send the caller to a tool at random, which is
        // worse than saying nothing.
        let elsewhere = self.tools.iter().find(|tool| {
            tool.name != called
                && properties(&tool.name).is_some_and(|fields| {
                    fields.len() == sent.len() && sent.iter().all(|name| fields.contains(name))
                })
        });
        match elsewhere {
            Some(tool) => format!(
                "{named}. Those are the arguments {} takes; call {}.",
                tool.name, tool.name
            ),
            None => named,
        }
    }

    pub fn new(tools: Vec<ToolDefinition>) -> Result<Self, DomainError> {
        let mut names = std::collections::BTreeSet::new();
        for tool in &tools {
            if tool.name.trim().is_empty() || tool.description.trim().is_empty() {
                return Err(DomainError::Invalid {
                    field: "tool_catalog",
                    reason: "tool names and descriptions must be non-empty".into(),
                });
            }
            if !tool.input_schema.is_object() {
                return Err(DomainError::Invalid {
                    field: "tool_catalog",
                    reason: format!("{} input_schema must be a JSON object", tool.name),
                });
            }
            if !names.insert(&tool.name) {
                return Err(DomainError::Invalid {
                    field: "tool_catalog",
                    reason: format!("duplicate tool name {}", tool.name),
                });
            }
        }
        Ok(Self { tools })
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.iter().find(|tool| tool.name == name)
    }
}
#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Harness-owned provenance, never an instruction inferred from text.
    /// Providers serialize only their wire fields; this survives compaction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<MessagePurpose>,
    /// The tool calls this assistant turn made, kept structurally.
    ///
    /// A turn that proposed an action was recorded as a string, so the next
    /// request carried the deployment's own call back to it as prose it had to
    /// re-read rather than as the call it made. A backend whose protocol pairs
    /// a call with its result cannot do that pairing from text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Which call this tool message answers.
    ///
    /// Without it, a run of results answers a run of calls by position, and
    /// position is exactly what compaction changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Images the person attached to this message, as files PWR stored
    /// (`.pwr/images/<sha256>.<ext>`), shown before its text. Only a
    /// deployment that reads images is sent any (backlog C.25).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<std::path::PathBuf>,
}

impl ChatMessage {
    /// A message that carries only text, which is most of them.
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            purpose: None,
            tool_calls: Vec::new(),
            tool_call_id: None,
            images: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessagePurpose {
    Task,
    SessionLedger,
    RepositoryExcerpts,
    /// The record compaction leaves in place of the history it folded. Marked
    /// so a later compaction merges it instead of reading it as a request, and
    /// so a replayed transcript does not show it as something the person said.
    CompactedMemory,
}
/// Provider-neutral tool invocation requested by a model.
///
/// Kept structural: adapters never flatten a call into prose, and callers never
/// re-parse `content` to guess that a call happened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}
/// Exact counts and timings reported by the backend for one generation.
///
/// Token counting is otherwise an estimate; where a backend reports real
/// counts, calibration uses them instead of a local proxy.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GenerationMetrics {
    pub prompt_tokens: Option<u64>,
    pub generated_tokens: Option<u64>,
    /// Tokens generated inside the thinking phase. `None` when the backend
    /// does not separate them -- never zero for "not counted".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u64>,
    /// Tokens generated after the thinking phase: the answer and any tool
    /// calls written in it, which no backend separates from each other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_tokens: Option<u64>,
    /// Where the counts above came from, so an estimate is never read as a
    /// count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_accounting: Option<TokenAccounting>,
    /// The engine ended the thinking phase at its budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_budget_reached: Option<bool>,
    pub total_duration_ns: Option<u64>,
    /// Time spent loading the model. A warm deployment reports near zero, which
    /// is how a warm-up is verified rather than assumed.
    pub load_duration_ns: Option<u64>,
    pub prompt_eval_duration_ns: Option<u64>,
    pub generation_duration_ns: Option<u64>,
}
impl GenerationMetrics {
    /// Backend-reported generation rate, when it reported enough to compute one.
    pub fn tokens_per_second(&self) -> Option<f64> {
        let tokens = self.generated_tokens?;
        let nanos = self.generation_duration_ns?;
        (nanos > 0).then(|| tokens as f64 / (nanos as f64 / 1e9))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelChunk {
    pub content: String,
    /// Reasoning text when the deployment emits it on a separate channel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    /// Present only on the terminal chunk, and only where the backend reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metrics: Option<GenerationMetrics>,
    pub done: bool,
}

impl ModelChunk {
    /// A chunk carries no observable payload when every channel is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
            && self.thinking.as_ref().is_none_or(|t| t.is_empty())
            && self.tool_calls.is_empty()
    }
}

impl Validate for DeploymentDescriptor {
    fn validate(&self) -> Result<(), DomainError> {
        if self.provider.is_empty() || self.model_ref.is_empty() {
            return Err(DomainError::Invalid {
                field: "deployment",
                reason: "provider and model_ref are required".into(),
            });
        }
        if !self.endpoint.starts_with("http://") && !self.endpoint.starts_with("https://") {
            return Err(DomainError::Invalid {
                field: "endpoint",
                reason: "must be an HTTP(S) URL".into(),
            });
        }
        Ok(())
    }
}
impl Validate for CalibrationProfile {
    fn validate(&self) -> Result<(), DomainError> {
        if self.model_digest.is_empty()
            || self.compatibility_key.is_empty()
            || self.harness_rev.is_empty()
        {
            return Err(DomainError::Invalid {
                field: "calibration",
                reason: "model, compatibility key, and harness revision are required".into(),
            });
        }
        if self.stable_points.iter().any(|p| {
            p.context_tokens == 0 || p.samples < 3 || !(0.0..=1.0).contains(&p.success_rate)
        }) {
            return Err(DomainError::Invalid {
                field: "stable_points",
                reason: "must have >=3 samples and valid measured success rates".into(),
            });
        }
        // A point that did not meet the profile's own admission criteria is not
        // a stable point. Without this, a tier where every sample failed still
        // authorises execution at that context.
        if let Some(point) = self
            .stable_points
            .iter()
            .find(|point| !self.thresholds.admits(point))
        {
            return Err(DomainError::Invalid {
                field: "stable_points",
                reason: format!(
                    "point at {} tokens does not meet the profile thresholds",
                    point.context_tokens
                ),
            });
        }
        Ok(())
    }
}
impl ExecutionProfile {
    pub fn execution_budgets(&self) -> Result<ExecutionBudgets, DomainError> {
        let budgets: ExecutionBudgets =
            serde_json::from_value(self.budgets.clone()).map_err(|e| DomainError::Invalid {
                field: "budgets",
                reason: format!(
                    "expected max_actions, edit_verify_cycles, and context_retries: {e}"
                ),
            })?;
        if budgets.max_actions == 0 || budgets.edit_verify_cycles == 0 {
            return Err(DomainError::Invalid {
                field: "budgets",
                reason: "max_actions and edit_verify_cycles must be greater than zero".into(),
            });
        }
        Ok(budgets)
    }

    pub fn validate_against(
        &self,
        calibration: Option<&CalibrationProfile>,
    ) -> Result<(), DomainError> {
        if self.context_tokens == 0
            || self.reserve_tokens >= self.context_tokens
            || self.concurrency == 0
        {
            return Err(DomainError::Invalid {
                field: "execution_profile",
                reason: "invalid context, reserve, or concurrency".into(),
            });
        }
        self.execution_budgets()?;
        match (self.calibration_id, calibration, &self.evidence) {
            (Some(id), Some(profile), EvidenceLabel::Measured)
                if id == profile.id
                    && self.compatibility_key == profile.compatibility_key
                    // The covering point must itself be admissible: capacity
                    // comes from a measurement that succeeded, not merely from
                    // one that was attempted at that size.
                    && profile.stable_points.iter().any(|p| {
                        p.context_tokens >= self.context_tokens
                            && profile.thresholds.admits(p)
                    }) =>
            {
                Ok(())
            }
            (None, None, EvidenceLabel::ConservativeBootstrap | EvidenceLabel::Computed) => Ok(()),
            _ => Err(DomainError::Incompatible {
                left: "execution profile",
                right: "calibration profile",
            }),
        }
    }
}
impl Validate for EvaluationRun {
    fn validate(&self) -> Result<(), DomainError> {
        if self.corpus_rev.is_empty()
            || self.harness_rev.is_empty()
            || self.model_digest.is_empty()
            || self.outcome_hash.is_empty()
        {
            return Err(DomainError::Invalid {
                field: "evaluation",
                reason: "corpus, harness, model, and outcome provenance are required".into(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hardware_with_memory(memory: Option<u64>) -> HardwareProfile {
        HardwareProfile {
            schema_version: SCHEMA_VERSION,
            id: new_id(),
            compatibility_key: "macos-arm64-test".into(),
            os: "macos".into(),
            architecture: "arm64".into(),
            cpu: "test".into(),
            accelerators: vec!["metal".into()],
            total_memory_bytes: memory,
            storage_free_bytes: None,
            unavailable_fields: Vec::new(),
            probe_version: "fixture".into(),
            provenance: Provenance {
                source: "fixture".into(),
                observed_at: now(),
                content_hash: "fixture".into(),
            },
        }
    }

    fn selection_candidate(
        model_ref: &str,
        certification: CertificationLevel,
    ) -> SelectionCandidate {
        SelectionCandidate {
            deployment: DeploymentDescriptor {
                schema_version: SCHEMA_VERSION,
                id: new_id(),
                provider: "fixture".into(),
                endpoint: "http://127.0.0.1:1234".into(),
                model_ref: model_ref.into(),
                backend_options: BTreeMap::new(),
                auth_ref: None,
            },
            model_digest: Some(format!("digest-{model_ref}")),
            profile_id: Some(format!("profile-{model_ref}")),
            context_limit_tokens: Some(32_768),
            estimated_model_memory_bytes: Some(4 * 1024 * 1024 * 1024),
            supports_tools: Observation::Observed(serde_json::json!(true)),
            supports_streaming: Observation::Observed(serde_json::json!(true)),
            certification,
            evidence: CandidateEvidence {
                quality_score: Some(50),
                speed_score: Some(50),
                calibrated: true,
            },
        }
    }

    fn certification_scope() -> CertificationScope {
        CertificationScope {
            model_digest: "digest".into(),
            deployment_fingerprint: "deployment".into(),
            backend_version: "backend-v1".into(),
            adapter_version: "adapter-v1".into(),
            hardware_compatibility_key: "hardware".into(),
            harness_rev: "harness-v1".into(),
        }
    }

    #[test]
    fn ids_are_v7() {
        assert_eq!(new_id().get_version_num(), 7);
    }
    #[test]
    fn measured_profile_rejects_bad_compatibility() {
        let p = CalibrationProfile {
            schema_version: 1,
            id: new_id(),
            compatibility_key: "a".into(),
            model_digest: "x".into(),
            deployment_fingerprint: "d".into(),
            harness_rev: "h".into(),
            thresholds: CalibrationThresholds::default(),
            stable_points: vec![StablePoint {
                context_tokens: 512,
                samples: 3,
                success_rate: 1.0,
                median_first_token_ms: 1.,
                generation_tokens_per_second: 1.,
                variance: 0.,
                memory_pressure_observed: false,
            }],
            raw_artifact_hashes: vec![],
            created_at: now(),
        };
        let e = ExecutionProfile {
            schema_version: 1,
            id: new_id(),
            strategy_id: new_id(),
            calibration_id: Some(p.id),
            context_tokens: 512,
            reserve_tokens: 1,
            concurrency: 1,
            budgets: serde_json::json!({}),
            rationale: "test".into(),
            evidence: EvidenceLabel::Measured,
            compatibility_key: "b".into(),
        };
        assert!(e.validate_against(Some(&p)).is_err());
    }

    #[test]
    fn resource_budget_requires_observed_memory_and_reserves_it_before_admission() {
        assert!(
            ResourceBudget::derive(
                &hardware_with_memory(None),
                None,
                ResourceBudgetPolicy::default(),
            )
            .is_err()
        );
        let gib = 1024 * 1024 * 1024;
        let budget = ResourceBudget::derive(
            &hardware_with_memory(Some(16 * gib)),
            None,
            ResourceBudgetPolicy::default(),
        )
        .unwrap();
        assert_eq!(budget.model_memory_budget_bytes, 33 * gib / 4);
        assert_eq!(budget.context_memory_budget_bytes, 11 * gib / 4);
        assert!(budget.admits_model_bytes(Some(8 * gib)));
        // An unobserved footprint is refused rather than assumed to fit.
        assert!(!budget.admits_model_bytes(None));
        // Admission stops at the derived budget, not at the raw memory total.
        assert!(budget.admits_model_bytes(Some(budget.model_memory_budget_bytes)));
        assert!(!budget.admits_model_bytes(Some(budget.model_memory_budget_bytes + 1)));
    }

    #[test]
    fn automatic_selection_is_deterministic_and_never_ignores_hard_requirements() {
        let gib = 1024 * 1024 * 1024;
        let budget = ResourceBudget::derive(
            &hardware_with_memory(Some(16 * gib)),
            None,
            ResourceBudgetPolicy::default(),
        )
        .unwrap();
        let request = SelectionRequest {
            requirements: TaskRequirements {
                minimum_context_tokens: 8_192,
                requires_tools: true,
                requires_streaming: true,
            },
            preference: PerformancePreference::Balanced,
            override_: SelectionOverride::default(),
            allow_experimental: false,
        };
        let compatible = selection_candidate("compatible", CertificationLevel::Compatible);
        let mut unknown_tools = selection_candidate("unknown-tools", CertificationLevel::Certified);
        unknown_tools.supports_tools = Observation::Unknown {
            reason: "not probed".into(),
        };
        let mut no_memory = selection_candidate("unknown-memory", CertificationLevel::Certified);
        no_memory.estimated_model_memory_bytes = None;
        let outcome = request.select(
            &[no_memory.clone(), unknown_tools.clone(), compatible.clone()],
            &budget,
        );
        assert_eq!(
            outcome
                .selection
                .as_ref()
                .map(|selection| selection.deployment.model_ref.as_str()),
            Some("compatible")
        );
        assert_eq!(outcome.reasons.len(), 2);
        // Input order is not a routing policy.
        let reversed = request.select(&[compatible, unknown_tools, no_memory], &budget);
        assert_eq!(outcome.selection, reversed.selection);
    }

    #[test]
    fn explicit_selection_is_never_silently_substituted() {
        let gib = 1024 * 1024 * 1024;
        let budget = ResourceBudget::derive(
            &hardware_with_memory(Some(16 * gib)),
            None,
            ResourceBudgetPolicy::default(),
        )
        .unwrap();
        let request = SelectionRequest {
            requirements: TaskRequirements {
                minimum_context_tokens: 8_192,
                requires_tools: true,
                requires_streaming: true,
            },
            preference: PerformancePreference::Quality,
            override_: SelectionOverride {
                provider: Some("fixture".into()),
                model_ref: Some("unfit".into()),
            },
            allow_experimental: false,
        };
        let fit = selection_candidate("fit", CertificationLevel::Certified);
        let mut unfit = selection_candidate("unfit", CertificationLevel::Certified);
        unfit.context_limit_tokens = Some(1);
        let outcome = request.select(&[fit, unfit], &budget);
        assert!(outcome.selection.is_none());
        assert_eq!(outcome.reasons.len(), 1);
    }

    #[test]
    fn latest_matching_certification_demotes_older_evidence_and_scope_must_match() {
        let scope = certification_scope();
        let old = CertificationRecord {
            schema_version: SCHEMA_VERSION,
            id: new_id(),
            scope: scope.clone(),
            level: CertificationLevel::Certified,
            evaluation_run_ids: vec![new_id()],
            artifact_hashes: vec!["old-artifact".into()],
            rationale: "suite passed".into(),
            issued_at: now(),
        };
        let mut regression = old.clone();
        regression.id = new_id();
        regression.level = CertificationLevel::Experimental;
        regression.evaluation_run_ids.clear();
        regression.artifact_hashes.clear();
        regression.rationale = "regression awaiting investigation".into();
        regression.issued_at = old.issued_at + chrono::Duration::seconds(1);
        let registry = CertificationRegistry {
            schema_version: SCHEMA_VERSION,
            records: vec![old, regression.clone()],
        };
        let report = registry.report(&scope).unwrap();
        assert_eq!(report.effective, Some(regression));
        let mut other_scope = scope;
        other_scope.adapter_version = "adapter-v2".into();
        assert!(registry.report(&other_scope).unwrap().effective.is_none());
    }
}

// ---------------------------------------------------------------- run events

/// Everything a run records, as a closed set.
///
/// The log carried `(&str, serde_json::Value)`: the type was a string literal
/// at each call site and the payload was whatever that site happened to build.
/// Nothing checked that two sites writing the same event agreed on its shape,
/// and nothing reading the log could rely on a field being there -- which is
/// why every reader so far has been a `payload["x"]` lookup that silently
/// yields null.
///
/// Typing them is the prerequisite for three things this project has written
/// down and not built: resuming a run from its own log rather than from a
/// summary, verifying a per-run hash chain, and a replay report. A reducer over
/// `serde_json::Value` would have to re-derive the shape at every match arm and
/// would be wrong in exactly the cases that matter.
///
/// Variants that carry genuinely open data -- a provenance bag, a tool outcome
/// whose shape belongs to the tool -- keep a `Value` for that field and are
/// typed around it. That is the honest boundary: the run's own state is typed,
/// and evidence produced elsewhere is carried.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum RunEvent {
    /// Opening provenance: profile, digest, hardware, approvals, evidence.
    RunStarted(serde_json::Value),
    SessionOpened {
        session: String,
    },
    TaskStarted(serde_json::Value),
    TaskTransition(TaskCheckpointRecord),
    TaskPlan {
        steps: Vec<String>,
        #[serde(default)]
        note: Option<String>,
    },
    /// A claimed step was checked against its own command.
    ///
    /// The harness still never infers that a step is done. It checks a claim,
    /// which is what it already does for completion.
    SubgoalChecked {
        step: usize,
        passed: bool,
        command: String,
    },
    PlanReconciled {
        steps_total: usize,
        steps_recorded_done: usize,
        steps_outstanding: Vec<String>,
    },
    TurnGenerated {
        step: u8,
        turn: u32,
        metrics: Option<GenerationMetrics>,
        tokens_per_second: Option<f64>,
        thinking_chars: usize,
        content_chars: usize,
        #[serde(default)]
        prompt_delivery: Option<serde_json::Value>,
        /// What the family adapter had to repair in this turn.
        ///
        /// Normalization that leaves no trace is indistinguishable from a
        /// deployment that never needed it, and the two call for opposite
        /// work: one says the adapter is carrying the run, the other says it
        /// could be retired. Defaulted so an artifact written before the
        /// compatibility layer existed still reads, as no repairs.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        normalizations: Vec<String>,
    },
    /// A generation that produced nothing the turn could use: a reply the
    /// backend could not parse, one cut off at the reply cap, a backend fault,
    /// a prompt refused for length, or one stopped by the operator.
    ///
    /// Measured 2026-09-22 (D.E2E-22): twelve minutes of generation in a
    /// conversation left no record at all, because only a usable reply wrote
    /// `turn.generated`, so a stall could be seen only while it was streaming.
    GenerationFailed {
        step: u8,
        turn: u32,
        /// `unparsed_output`, `runaway_reply`, `backend_fault`,
        /// `context_limit` or `cancelled`.
        outcome: String,
        detail: String,
        /// What had streamed before it failed.
        thinking_chars: usize,
        content_chars: usize,
        elapsed_ms: u64,
    },
    ActionMalformed {
        step: u8,
        problem: String,
        /// Which fault it was, decided where the fault was found.
        ///
        /// A count of malformed calls says a fifth of turns were wasted; it
        /// does not say whether the deployment wrote prose, invented a tool,
        /// or filled in a real one wrongly, and those have different fixes.
        /// Defaulted so an artifact written before the kinds existed still
        /// reads, as `unclassified`.
        #[serde(default = "unclassified_kind")]
        kind: String,
        /// What the turn contained, bounded. The kind says which fault; this
        /// says what to do about it, and the two commonest kinds cannot be
        /// acted on without it.
        #[serde(default)]
        detail: Option<String>,
    },
    ApprovalDecision {
        step: u8,
        approval: String,
        description: String,
        decision: String,
    },
    /// One tool attempt, allowed or not. `outcome` carries whatever the tool
    /// returned; the fields around it are the run's own record of what
    /// happened, which is what the reducer and the metrics read.
    ToolAction {
        action: serde_json::Value,
        status: ToolActionStatus,
        outcome_class: String,
        #[serde(default)]
        outcome: Option<serde_json::Value>,
        #[serde(default)]
        denial: Option<String>,
        #[serde(default)]
        failure: Option<String>,
        #[serde(default)]
        failure_category: Option<String>,
    },
    VerifierAdopted {
        step: u8,
        executable: String,
        args: Vec<String>,
        source: String,
    },
    VerificationBaseline(serde_json::Value),
    VerificationDiagnostics {
        step: u8,
        passing: bool,
        diagnostics: serde_json::Value,
    },
    VerificationInterim {
        step: u8,
        passing: bool,
    },
    VerificationResult {
        /// No check that was passing before the run is failing now. This is
        /// what verification proves, and all it has ever proved.
        verified: bool,
        verifiable: bool,
        /// Whether every check passes, which is a different and stricter fact
        /// than `verified` -- and not one a task should be judged by in a
        /// repository that was already failing something.
        #[serde(default)]
        suite_green: bool,
        /// Checks still failing that were failing before the run started. Not
        /// the agent's work, and recorded so a reader can see they were
        /// excluded deliberately rather than missed.
        #[serde(default)]
        still_failing_from_before: Vec<String>,
        after: serde_json::Value,
        comparison: serde_json::Value,
        /// Checks already failing before the run touched anything, by
        /// command. A completion is judged against this rather than against a
        /// green suite.
        #[serde(default)]
        failing_before_the_run: Vec<String>,
    },
    TaskRecovery(serde_json::Value),
    ContextCompacted(serde_json::Value),
    /// The prompt's section-by-section accounting: what each cost, what was
    /// cut, and what was reserved for the reply.
    ContextCompiled(serde_json::Value),
    ContextTierChanged {
        previous_context_tokens: u32,
        context_tokens: u32,
        evidence: String,
        provider_error: String,
        attempt: u8,
    },
    ContextDeliveryDiverged {
        step: u8,
        turn: u32,
        concern: String,
        delivery: serde_json::Value,
    },
    /// The host's memory pressure, sampled after a turn.
    ///
    /// Pressure was read once, at admission, so a run that starts on a quiet
    /// machine and ends on a saturated one recorded nothing about the
    /// difference -- which is usually the difference that explains its timings.
    ResourceSampled {
        step: u8,
        turn: u32,
        pressure: Observation,
    },
    LoopDetected {
        step: u8,
        action: String,
    },
    NoProgressDetected {
        step: u8,
        window: usize,
        actions: Vec<String>,
    },
    /// The deployment refused the task, with its reason.
    TaskDeclined {
        rationale: String,
    },
    TaskComplete {
        step: u8,
        verified: bool,
    },
    TaskFailed {
        reason: String,
        /// What kind of failure it was.
        ///
        /// A caller had to search the error text -- `error.contains("provider
        /// ")` -- to tell a backend fault from a deployment that could not do
        /// the task. That is a classification made of prose, and it changes
        /// whenever a message is reworded.
        #[serde(default)]
        class: TerminalClass,
        #[serde(default)]
        detail: Option<serde_json::Value>,
    },
}

/// Why a run ended without completing.
///
/// The distinction that matters for a measurement: a backend fault says
/// nothing about whether the deployment could have done the task, while a
/// timeout does -- a deployment that cannot answer within the bound has failed
/// it, and excluding that would hide slowness behind an infrastructure label.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalClass {
    /// The backend was unavailable, spoke badly, or refused the request.
    Provider,
    /// The deployment did not answer within the bound. Its failure, not the
    /// backend's.
    Timeout,
    /// Actions or turns ran out.
    Budget,
    /// Nothing could verify the work, so completion was refused.
    NoVerifier,
    /// The deployment could not form a usable call.
    Protocol,
    /// Recovery ran out of ways forward.
    Recovery,
    /// The run was interrupted, killed, or cancelled.
    Interrupted,
    /// The deployment refused the task and said why.
    ///
    /// Not a failure and not a completion. On an attack task it is the correct
    /// outcome, and collapsing it into either would report the boundary working
    /// as the boundary breaking.
    Declined,
    /// Anything not yet classified, including a build that predates this
    /// field. Never silently one of the above.
    #[default]
    Unclassified,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolActionStatus {
    Allowed,
    Denied,
    Failed,
}

/// A persisted state transition, as the log records it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCheckpointRecord {
    pub id: Id,
    pub state: String,
    pub detail: String,
    pub at: DateTime<Utc>,
}

impl RunEvent {
    /// The stored `event_type`, unchanged from what each call site used to
    /// write as a literal. The column keeps its meaning and old artifacts keep
    /// reading.
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::RunStarted(_) => "run.started",
            Self::SessionOpened { .. } => "session.opened",
            Self::TaskStarted(_) => "task.started",
            Self::TaskTransition(_) => "task.transition",
            Self::TaskPlan { .. } => "task.plan",
            Self::SubgoalChecked { .. } => "subgoal.checked",
            Self::PlanReconciled { .. } => "plan.reconciled",
            Self::TurnGenerated { .. } => "turn.generated",
            Self::GenerationFailed { .. } => "turn.failed",
            Self::ActionMalformed { .. } => "action.malformed",
            Self::ApprovalDecision { .. } => "approval.decision",
            Self::ToolAction { .. } => "tool.action",
            Self::VerifierAdopted { .. } => "verifier.adopted",
            Self::VerificationBaseline(_) => "verification.baseline",
            Self::VerificationDiagnostics { .. } => "verification.diagnostics",
            Self::VerificationInterim { .. } => "verification.interim",
            Self::VerificationResult { .. } => "verification.result",
            Self::TaskRecovery(_) => "task.recovery",
            Self::ContextCompacted(_) => "context.compacted",
            Self::ContextCompiled(_) => "context.compiled",
            Self::ContextTierChanged { .. } => "context.tier_changed",
            Self::ContextDeliveryDiverged { .. } => "context.delivery_diverged",
            Self::ResourceSampled { .. } => "resource.sampled",
            Self::LoopDetected { .. } => "loop.detected",
            Self::NoProgressDetected { .. } => "no_progress.detected",
            Self::TaskDeclined { .. } => "task.declined",
            Self::TaskComplete { .. } => "task.complete",
            Self::TaskFailed { .. } => "task.failed",
        }
    }

    /// The payload column: the variant's own fields, without the tag.
    ///
    /// A variant wrapping a bare value writes that value, so a payload written
    /// before this type existed still deserialises into the same variant.
    pub fn payload(&self) -> serde_json::Value {
        match self {
            Self::RunStarted(value)
            | Self::TaskStarted(value)
            | Self::VerificationBaseline(value)
            | Self::TaskRecovery(value)
            | Self::ContextCompacted(value) => value.clone(),
            other => {
                let mut value = serde_json::to_value(other).unwrap_or_default();
                if let Some(object) = value.as_object_mut() {
                    object.remove("event");
                }
                value
            }
        }
    }

    /// Reads a stored record back into its variant.
    ///
    /// `None` for an event this build does not know, which is a record written
    /// by another version rather than a corrupt one -- a reducer skips it and
    /// says so instead of guessing.
    pub fn from_stored(event_type: &str, payload: &serde_json::Value) -> Option<Self> {
        let wrapped = match event_type {
            "run.started" => return Some(Self::RunStarted(payload.clone())),
            "task.started" => return Some(Self::TaskStarted(payload.clone())),
            "verification.baseline" => {
                return Some(Self::VerificationBaseline(payload.clone()));
            }
            "task.recovery" => return Some(Self::TaskRecovery(payload.clone())),
            "context.compacted" => return Some(Self::ContextCompacted(payload.clone())),
            "context.compiled" => return Some(Self::ContextCompiled(payload.clone())),
            other => other,
        };
        let mut value = payload.clone();
        value.as_object_mut()?.insert(
            "event".into(),
            serde_json::Value::String(tag_for(wrapped)?.to_string()),
        );
        serde_json::from_value(value).ok()
    }
}

/// The serde tag for a stored event type.
fn tag_for(event_type: &str) -> Option<&'static str> {
    Some(match event_type {
        "session.opened" => "session_opened",
        "task.transition" => "task_transition",
        "task.plan" => "task_plan",
        "subgoal.checked" => "subgoal_checked",
        "plan.reconciled" => "plan_reconciled",
        "turn.generated" => "turn_generated",
        "turn.failed" => "generation_failed",
        "action.malformed" => "action_malformed",
        "approval.decision" => "approval_decision",
        "tool.action" => "tool_action",
        "verifier.adopted" => "verifier_adopted",
        "verification.diagnostics" => "verification_diagnostics",
        "verification.interim" => "verification_interim",
        "verification.result" => "verification_result",
        "context.tier_changed" => "context_tier_changed",
        "context.delivery_diverged" => "context_delivery_diverged",
        "resource.sampled" => "resource_sampled",
        "loop.detected" => "loop_detected",
        "no_progress.detected" => "no_progress_detected",
        "task.declined" => "task_declined",
        "task.complete" => "task_complete",
        "task.failed" => "task_failed",
        _ => return None,
    })
}
