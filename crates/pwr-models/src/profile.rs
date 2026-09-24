//! What PWR knows about one model artifact on one backend, and how sure it
//! is.
//!
//! A model PWR has never seen is not an unsupported model. Five states keep
//! "not tested" apart from "not compatible":
//!
//! - **Verified** -- PWR's own controlled evaluation of this exact artifact
//!   and backend ships with PWR ([`VerifiedEntry`]).
//! - **Locally calibrated** -- Quick Calibration ran on this machine and the
//!   checks agent mode depends on passed.
//! - **Provisional** -- nothing has been measured; the model is used with
//!   conservative defaults. This is where every new model starts.
//! - **Limited** -- calibration found that something agent mode depends on
//!   (tool calls, continuing after a tool result) did not work.
//! - **Incompatible** -- concrete evidence that the artifact cannot be run
//!   here: it could not be read, it cannot be formatted as a chat, or it
//!   failed to generate at all.
//!
//! Evidence is tied to [`Provenance`], not to a name. [`compare`] decides,
//! deterministically, whether evidence gathered under one provenance still
//! applies to the artifact in front of it: in full, with reduced confidence,
//! or not at all (see its table).

use pwr_domain::{
    ModelInspection, ReasoningBudgets, ReasoningCapability, ReasoningEvidence, ReasoningProfile,
    TemplateReasoning,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The Quick Calibration suite's version. Evidence from another version of the
/// suite measured different things and is not reused (see [`compare`]).
pub const CALIBRATION_VERSION: &str = "quick-calibration-3";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    Verified,
    LocallyCalibrated,
    Provisional,
    Limited,
    Incompatible,
}

/// How far the evidence behind a status can be trusted.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// Controlled evaluation of this exact artifact and backend.
    Established,
    /// A quick local check: enough to operate the model, not a benchmark.
    Preliminary,
    /// Evidence gathered under a provenance that differs in a way that may
    /// matter (a backend update, another machine).
    Reduced,
    /// Nothing measured.
    Untested,
}

/// Everything that identifies what a measurement was a measurement of.
/// Unknown facts stay `None`; nothing is filled in by guess.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    /// The model as addressed: a folder below the models root, or a path.
    pub model: String,
    /// The Hub commit it was downloaded at, when PWR downloaded it.
    pub revision: Option<String>,
    /// Where the artifact is on this machine. Not part of its identity: a
    /// moved folder is the same model.
    pub artifact_path: Option<String>,
    /// The backend's identity for the artifact (MLX: config and weight
    /// index; GGUF: header and metadata).
    pub artifact_digest: Option<String>,
    /// Names and sizes of the weight files, hashed.
    pub weights_fingerprint: Option<String>,
    pub artifact_bytes: Option<u64>,
    pub parameter_count: Option<u64>,
    pub quantization: Option<String>,
    pub format: Option<String>,
    pub architecture: Option<String>,
    pub tokenizer_fingerprint: Option<String>,
    pub chat_template_fingerprint: Option<String>,
    pub backend: String,
    pub backend_version: Option<String>,
    pub pwr_version: String,
    pub calibration_version: String,
    /// Coarse: platform, architecture, chip family and memory. Behaviour is a
    /// property of the model and engine, so a different machine lowers
    /// confidence rather than voiding evidence.
    pub hardware_class: Option<String>,
    pub observed_at: String,
}

impl Provenance {
    /// The provenance of an inspected artifact. Every fact is read from the
    /// inspection; one the backend did not report stays unknown.
    pub fn of(
        inspection: &ModelInspection,
        backend: &str,
        backend_version: Option<String>,
        hardware_class: Option<String>,
    ) -> Self {
        let metadata = &inspection.definition.metadata;
        let text = |key: &str| {
            metadata
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        };
        Self {
            model: inspection.deployment.model_ref.clone(),
            revision: text("revision"),
            artifact_path: text("path"),
            artifact_digest: Some(inspection.definition.digest.clone())
                .filter(|digest| !digest.is_empty()),
            weights_fingerprint: text("weights_fingerprint"),
            artifact_bytes: metadata
                .get("weights_bytes")
                .and_then(serde_json::Value::as_u64),
            parameter_count: metadata
                .get("parameter_count")
                .and_then(serde_json::Value::as_u64),
            quantization: inspection.definition.quantization.clone(),
            format: text("format"),
            architecture: inspection.definition.family.clone(),
            tokenizer_fingerprint: text("tokenizer_fingerprint"),
            chat_template_fingerprint: text("chat_template_fingerprint"),
            backend: backend.to_owned(),
            backend_version,
            pwr_version: env!("CARGO_PKG_VERSION").to_owned(),
            calibration_version: CALIBRATION_VERSION.to_owned(),
            hardware_class,
            observed_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// A coarse machine class for provenance: `macos-arm64-apple-m2-64gb`.
pub fn hardware_class(host: &pwr_runtime::host::HostProfile) -> String {
    let chip = host
        .apple_chip
        .as_ref()
        .map(|chip| {
            chip.name
                .to_lowercase()
                .split_whitespace()
                .take(2)
                .collect::<Vec<_>>()
                .join("-")
        })
        .unwrap_or_else(|| "unknown-chip".into());
    let memory = host
        .memory
        .total_bytes
        .map(|bytes| format!("{}gb", (bytes + (1 << 29)) >> 30))
        .unwrap_or_else(|| "unknown-memory".into());
    format!(
        "{}-{}-{chip}-{memory}",
        std::env::consts::OS,
        host.architecture
    )
}

/// Whether evidence gathered under one provenance applies to another.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Reuse {
    /// Same artifact, backend and suite: reused as it is.
    Applies,
    /// Same artifact, but something around it moved in a way that may matter:
    /// reused, with confidence lowered and the reasons shown.
    Reduced,
    /// Something that determines behaviour changed: the evidence is kept for
    /// reference and not applied; recalibrate.
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReuseDecision {
    pub reuse: Reuse,
    pub reasons: Vec<String>,
}

/// The reuse rules. Deterministic; documented in `docs/model-compatibility.md`.
///
/// | Change | Outcome |
/// |---|---|
/// | backend, format, artifact digest, weight files, artifact size, quantization, revision, tokenizer, chat template | stale |
/// | calibration suite version | stale |
/// | backend major version (for 0.x: the minor) | stale |
/// | backend minor/patch version, engine script | reduced |
/// | hardware class | reduced |
/// | a behaviour-determining fact known on one side only | reduced |
/// | pwr version, artifact path, observation time | applies |
pub fn compare(evidence: &Provenance, current: &Provenance) -> ReuseDecision {
    let mut stale = Vec::new();
    let mut reduced = Vec::new();
    if evidence.model != current.model {
        stale.push(format!(
            "it was measured on {}, not {}",
            evidence.model, current.model
        ));
    }
    if evidence.backend != current.backend {
        stale.push(format!(
            "backend changed ({} → {})",
            evidence.backend, current.backend
        ));
    }
    let identity: [(&str, &Option<String>, &Option<String>); 8] = [
        ("format", &evidence.format, &current.format),
        (
            "artifact",
            &evidence.artifact_digest,
            &current.artifact_digest,
        ),
        (
            "weight files",
            &evidence.weights_fingerprint,
            &current.weights_fingerprint,
        ),
        (
            "quantization",
            &evidence.quantization,
            &current.quantization,
        ),
        ("revision", &evidence.revision, &current.revision),
        (
            "tokenizer",
            &evidence.tokenizer_fingerprint,
            &current.tokenizer_fingerprint,
        ),
        (
            "chat template",
            &evidence.chat_template_fingerprint,
            &current.chat_template_fingerprint,
        ),
        (
            "architecture",
            &evidence.architecture,
            &current.architecture,
        ),
    ];
    for (name, before, now) in identity {
        match (before, now) {
            (Some(before), Some(now)) if before != now => {
                stale.push(format!("{name} changed"));
            }
            (Some(_), None) | (None, Some(_)) => {
                reduced.push(format!(
                    "{name} cannot be compared (known on one side only)"
                ));
            }
            _ => {}
        }
    }
    match (evidence.artifact_bytes, current.artifact_bytes) {
        (Some(before), Some(now)) if before != now => stale.push("artifact size changed".into()),
        _ => {}
    }
    if evidence.calibration_version != current.calibration_version {
        stale.push(format!(
            "measured by {}, and this PWR calibrates with {}",
            evidence.calibration_version, current.calibration_version
        ));
    }
    match (&evidence.backend_version, &current.backend_version) {
        (Some(before), Some(now)) if before != now => match backend_version_change(before, now) {
            VersionChange::Major(detail) => {
                stale.push(format!("backend major version changed ({detail})"))
            }
            VersionChange::Minor(detail) => reduced.push(format!("backend updated ({detail})")),
        },
        (Some(_), None) | (None, Some(_)) => {
            reduced.push("backend version cannot be compared (known on one side only)".into())
        }
        _ => {}
    }
    match (&evidence.hardware_class, &current.hardware_class) {
        (Some(before), Some(now)) if before != now => {
            reduced.push(format!("measured on another machine ({before})"))
        }
        _ => {}
    }
    if !stale.is_empty() {
        ReuseDecision {
            reuse: Reuse::Stale,
            reasons: stale,
        }
    } else if !reduced.is_empty() {
        ReuseDecision {
            reuse: Reuse::Reduced,
            reasons: reduced,
        }
    } else {
        ReuseDecision {
            reuse: Reuse::Applies,
            reasons: Vec::new(),
        }
    }
}

enum VersionChange {
    Major(String),
    Minor(String),
}

/// Compares `name version; name version; ...` strings component by
/// component. A component's major is its first number, or `0.minor` for a
/// 0.x version (which semver treats as breaking). Anything that does not
/// parse as numbers -- a build hash, a commit -- is compared as text and
/// counts as minor.
fn backend_version_change(before: &str, now: &str) -> VersionChange {
    let parts = |text: &str| -> Vec<(String, String)> {
        text.split(';')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| match part.rsplit_once(' ') {
                Some((name, version)) => (name.trim().to_owned(), version.trim().to_owned()),
                None => (String::new(), part.to_owned()),
            })
            .collect()
    };
    let major = |version: &str| -> Option<(u64, u64)> {
        let mut numbers = version
            .trim_start_matches(['v', 'b'])
            .split('.')
            .map(|n| n.parse::<u64>());
        let first = numbers.next()?.ok()?;
        let second = numbers.next().and_then(Result::ok).unwrap_or(0);
        Some(if first == 0 { (0, second) } else { (first, 0) })
    };
    let (before, now) = (parts(before), parts(now));
    let mut minor = Vec::new();
    for (name, old) in &before {
        match now.iter().find(|(other, _)| other == name) {
            Some((_, new)) if new != old => {
                let label = if name.is_empty() { "version" } else { name };
                match (major(old), major(new)) {
                    (Some(a), Some(b)) if a != b => {
                        return VersionChange::Major(format!("{label} {old} → {new}"));
                    }
                    _ => minor.push(format!("{label} {old} → {new}")),
                }
            }
            Some(_) => {}
            None => minor.push(format!("{name} no longer reported")),
        }
    }
    if minor.is_empty() {
        minor.push("version string changed".into());
    }
    VersionChange::Minor(minor.join(", "))
}

/// One mechanical check of Quick Calibration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub name: String,
    /// `None`: not run (a precondition failed, or not applicable).
    pub passed: Option<bool>,
    /// Agent mode depends on it.
    pub critical: bool,
    pub detail: String,
}

/// What calibration saw of the model's reasoning.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningObservation {
    /// The model produced reasoning when allowed to.
    pub emitted: Option<bool>,
    /// The reasoning arrived separately from the answer.
    pub separated: Option<bool>,
    /// A thinking phase ended by itself, under a generous budget.
    pub terminated_naturally: Option<bool>,
    /// After the engine closed the phase at a tiny budget, an answer followed.
    pub finalization_after_budget: Option<bool>,
    pub detail: String,
}

/// Quick Calibration's result for one artifact on one backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalEvidence {
    pub schema_version: u32,
    pub status: ProfileStatus,
    pub provenance: Provenance,
    pub checks: Vec<Check>,
    pub reasoning: ReasoningObservation,
    pub duration_ms: u64,
    /// Why the status is what it is, when it is not a pass.
    #[serde(default)]
    pub reason: Option<String>,
}

pub const EVIDENCE_SCHEMA: u32 = 1;

/// A controlled evaluation shipped with PWR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedEntry {
    pub provenance: Provenance,
    /// The suite and where its report is.
    pub evaluation: String,
    pub checks: Vec<Check>,
    pub reasoning: ReasoningObservation,
    #[serde(default)]
    pub reasoning_budgets: Option<ReasoningBudgets>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VerifiedRegistry {
    schema_version: u32,
    entries: Vec<VerifiedEntry>,
}

/// The verified evidence this build carries (`verified-models.json`).
pub fn verified_entries() -> Vec<VerifiedEntry> {
    parse_verified(include_str!("../verified-models.json")).unwrap_or_default()
}

fn parse_verified(text: &str) -> Result<Vec<VerifiedEntry>, String> {
    let registry: VerifiedRegistry = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if registry.schema_version != EVIDENCE_SCHEMA {
        return Err(format!(
            "verified registry schema {} is not {EVIDENCE_SCHEMA}",
            registry.schema_version
        ));
    }
    Ok(registry.entries)
}

/// Local calibration results, one file per backend and model, outside any
/// repository: `$POORAI_EVIDENCE_DIR`, else `~/.poorai/model-evidence`. An
/// application update leaves them in place; what applies is decided by
/// [`compare`], not by deleting files.
#[derive(Debug, Clone)]
pub struct EvidenceStore {
    dir: PathBuf,
}

impl EvidenceStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn default_location() -> Option<Self> {
        if let Some(dir) = std::env::var_os("POORAI_EVIDENCE_DIR") {
            return Some(Self::new(dir));
        }
        let home = std::env::var_os("HOME")?;
        Some(Self::new(
            Path::new(&home).join(".poorai").join("model-evidence"),
        ))
    }

    /// A file name derived from the key by hashing: a model reference is
    /// user input and never becomes a path.
    fn path(&self, backend: &str, model: &str) -> PathBuf {
        let key = pwr_domain::hash_bytes(format!("{backend}\n{model}"));
        self.dir.join(format!("{}.json", &key[..32.min(key.len())]))
    }

    pub fn load(&self, backend: &str, model: &str) -> Option<LocalEvidence> {
        let bytes = std::fs::read(self.path(backend, model)).ok()?;
        let evidence: LocalEvidence = serde_json::from_slice(&bytes).ok()?;
        (evidence.schema_version == EVIDENCE_SCHEMA
            && evidence.provenance.backend == backend
            && evidence.provenance.model == model)
            .then_some(evidence)
    }

    pub fn save(&self, evidence: &LocalEvidence) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.dir)?;
        let path = self.path(&evidence.provenance.backend, &evidence.provenance.model);
        let temporary = path.with_extension("tmp");
        std::fs::write(
            &temporary,
            serde_json::to_vec_pretty(evidence).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(temporary, path)
    }
}

/// What a person can use the model for, given its status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Features {
    /// A conversation without a workspace (chat mode).
    pub chat: bool,
    /// A conversation in a workspace: reading, editing and running things
    /// through tool calls.
    pub agent: bool,
    /// Why a feature is off, for the person.
    pub note: Option<String>,
}

/// One line of the succinct result the app shows.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityLine {
    pub label: String,
    /// `supported`, `not_reliable`, `detected`, `not_detected`,
    /// `provisional`, `not_tested`.
    pub result: String,
}

/// Where the status came from.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    Verified,
    LocalCalibration,
    Inspection,
    None,
}

/// Everything the app shows about a model's compatibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Assessment {
    pub status: ProfileStatus,
    pub confidence: Confidence,
    pub source: EvidenceSource,
    pub summary: String,
    pub reasons: Vec<String>,
    pub features: Features,
    pub checks: Vec<Check>,
    pub capabilities: Vec<CapabilityLine>,
    pub reasoning: ReasoningProfile,
    pub provenance: Provenance,
    /// The provenance of the evidence used, or of stale evidence not used.
    pub evidence_provenance: Option<Provenance>,
    /// Recalibration is recommended (stale or reduced evidence, or none).
    pub recalibrate: bool,
}

/// What the model's template and declared profile say about reasoning,
/// before anything is run.
pub fn template_reasoning(
    inspection: &ModelInspection,
    disabled_by_profile: bool,
    declared_budgets: Option<ReasoningBudgets>,
) -> ReasoningProfile {
    let metadata = &inspection.definition.metadata;
    let template: TemplateReasoning = metadata
        .get("reasoning_template")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    let capability = metadata
        .get("reasoning_capability")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or(ReasoningCapability::Unknown);
    // A template that takes native levels and cannot switch reasoning off
    // (harmony: gpt-oss's least is `low`) is not "off" because a profile says
    // so -- the engine would send `low` and the app would claim thinking was
    // off. Its three native levels are what the setting controls.
    let disabled_by_profile = disabled_by_profile
        && !(capability == ReasoningCapability::TemplateControlled && !template.switchable);
    ReasoningProfile {
        capability,
        evidence: if declared_budgets.is_some() {
            ReasoningEvidence::Profile
        } else {
            ReasoningEvidence::Template
        },
        switchable: template.switchable,
        disabled_by_profile,
        budgets: declared_budgets.filter(|budgets| budgets.valid()),
        finalization_verified: None,
    }
}

/// Folds calibration's observation into the template's picture. A model that
/// reasoned is not downgraded by one that did not on a trivial prompt: `None`
/// is only concluded where the template gave no sign of reasoning either.
fn observed_reasoning(
    mut profile: ReasoningProfile,
    observation: &ReasoningObservation,
) -> ReasoningProfile {
    if observation.emitted.is_none() && observation.finalization_after_budget.is_none() {
        return profile;
    }
    profile.evidence = ReasoningEvidence::Calibration;
    if profile.capability == ReasoningCapability::Unknown && observation.emitted == Some(false) {
        profile.capability = ReasoningCapability::None;
    }
    profile.finalization_verified = observation.finalization_after_budget;
    profile
}

/// Something about the artifact that makes it unusable before anything runs.
pub fn static_incompatibility(inspection: &ModelInspection) -> Option<String> {
    let metadata = &inspection.definition.metadata;
    let format = metadata.get("format").and_then(serde_json::Value::as_str);
    if format == Some("mlx")
        && metadata.get("has_chat_template") == Some(&serde_json::Value::Bool(false))
    {
        return Some(
            "This model ships no chat template, so PWR cannot format a conversation for it. \
             It is likely a base (non-chat) model."
                .into(),
        );
    }
    None
}

fn line(label: &str, result: &str) -> CapabilityLine {
    CapabilityLine {
        label: label.into(),
        result: result.into(),
    }
}

fn outcome(checks: &[Check], names: &[&str]) -> &'static str {
    let found: Vec<Option<bool>> = checks
        .iter()
        .filter(|check| names.contains(&check.name.as_str()))
        .map(|check| check.passed)
        .collect();
    if found.is_empty() || found.iter().all(Option::is_none) {
        "not_tested"
    } else if found.iter().all(|passed| *passed == Some(true)) {
        "supported"
    } else {
        "not_reliable"
    }
}

/// The succinct result lines. A capability that was not tested says so.
pub fn capability_lines(checks: &[Check], reasoning: &ReasoningProfile) -> Vec<CapabilityLine> {
    let reasoning_line = match (reasoning.evidence, reasoning.capability) {
        (_, ReasoningCapability::None) => "not_detected",
        (ReasoningEvidence::Calibration, _) if reasoning.finalization_verified == Some(false) => {
            "not_reliable"
        }
        (ReasoningEvidence::Calibration, ReasoningCapability::Unknown) => "not_tested",
        (ReasoningEvidence::Calibration, _) => "detected",
        (_, ReasoningCapability::Unknown) => "not_tested",
        _ => "provisional",
    };
    vec![
        line(
            "Tool calling",
            outcome(
                checks,
                &[
                    "tool_selection",
                    "tool_arguments",
                    "tool_result_continuation",
                ],
            ),
        ),
        line("Structured output", outcome(checks, &["structured_output"])),
        line(
            "Coding",
            outcome(checks, &["code_understanding", "repository_file_selection"]),
        ),
        line(
            "Instruction following",
            outcome(checks, &["instruction_following"]),
        ),
        line("Reasoning mode", reasoning_line),
        // Quick Calibration never measures long context.
        line("Context", "provisional"),
    ]
}

/// The inputs to [`assess`].
pub struct AssessInput<'a> {
    pub current: Provenance,
    /// From [`template_reasoning`].
    pub reasoning: ReasoningProfile,
    /// From [`static_incompatibility`], or an inspection that failed.
    pub incompatible: Option<String>,
    pub verified: &'a [VerifiedEntry],
    pub local: Option<&'a LocalEvidence>,
}

fn features_of(status: ProfileStatus, checks: &[Check]) -> Features {
    match status {
        ProfileStatus::Incompatible => Features {
            chat: false,
            agent: false,
            note: Some("PWR cannot run this model.".into()),
        },
        ProfileStatus::Limited => Features {
            chat: checks
                .iter()
                .any(|check| check.name == "termination" && check.passed == Some(true)),
            agent: false,
            note: Some(
                "Tool calling was not reliable during calibration, and agent tasks in a workspace \
                 depend on it. Chat without a workspace is still available; reading attached \
                 files there also uses a tool call and may fail."
                    .into(),
            ),
        },
        _ => Features {
            chat: true,
            agent: true,
            note: None,
        },
    }
}

/// The status of one artifact, from everything known about it.
///
/// Precedence: concrete incompatibility; then a local calibration that found
/// the model limited or incompatible (a failure on this machine outranks a
/// pass elsewhere); then verified evidence that applies; then local evidence
/// that applies; then Provisional. Stale evidence is never applied, and is
/// named in `reasons`.
pub fn assess(input: AssessInput<'_>) -> Assessment {
    let AssessInput {
        current,
        reasoning,
        incompatible,
        verified,
        local,
    } = input;
    let build = |status,
                 confidence,
                 source,
                 summary: String,
                 reasons: Vec<String>,
                 checks: Vec<Check>,
                 reasoning: ReasoningProfile,
                 evidence_provenance: Option<Provenance>,
                 recalibrate| Assessment {
        features: features_of(status, &checks),
        capabilities: capability_lines(&checks, &reasoning),
        status,
        confidence,
        source,
        summary,
        reasons,
        checks,
        reasoning,
        provenance: current.clone(),
        evidence_provenance,
        recalibrate,
    };
    if let Some(reason) = incompatible {
        return build(
            ProfileStatus::Incompatible,
            Confidence::Established,
            EvidenceSource::Inspection,
            "Incompatible".into(),
            vec![reason],
            Vec::new(),
            reasoning,
            None,
            false,
        );
    }
    let local_decision = local.map(|evidence| (evidence, compare(&evidence.provenance, &current)));
    // A local failure that still applies outranks everything but static
    // incompatibility.
    if let Some((evidence, decision)) = &local_decision
        && decision.reuse != Reuse::Stale
        && matches!(
            evidence.status,
            ProfileStatus::Limited | ProfileStatus::Incompatible
        )
    {
        let reasoning = observed_reasoning(reasoning, &evidence.reasoning);
        let mut reasons: Vec<String> = evidence.reason.iter().cloned().collect();
        reasons.extend(decision.reasons.iter().cloned());
        return build(
            evidence.status,
            if decision.reuse == Reuse::Reduced {
                Confidence::Reduced
            } else {
                Confidence::Preliminary
            },
            EvidenceSource::LocalCalibration,
            match evidence.status {
                ProfileStatus::Limited => "Limited compatibility".into(),
                _ => "Incompatible".into(),
            },
            reasons,
            evidence.checks.clone(),
            reasoning,
            Some(evidence.provenance.clone()),
            decision.reuse == Reuse::Reduced,
        );
    }
    let mut stale_reasons = Vec::new();
    for entry in verified {
        if entry.provenance.backend != current.backend
            || entry.provenance.artifact_digest.is_none()
            || entry.provenance.artifact_digest != current.artifact_digest
        {
            continue;
        }
        // Verified evidence is not a calibration: its suite version is its
        // own, so only the artifact and engine facts are compared.
        let comparable = Provenance {
            calibration_version: current.calibration_version.clone(),
            ..entry.provenance.clone()
        };
        let decision = compare(&comparable, &current);
        match decision.reuse {
            Reuse::Stale => stale_reasons.extend(
                decision
                    .reasons
                    .iter()
                    .map(|reason| format!("verified evidence no longer applies: {reason}")),
            ),
            reuse => {
                let reasoning = observed_reasoning(
                    ReasoningProfile {
                        budgets: entry.reasoning_budgets.or(reasoning.budgets),
                        ..reasoning
                    },
                    &entry.reasoning,
                );
                return build(
                    ProfileStatus::Verified,
                    if reuse == Reuse::Reduced {
                        Confidence::Reduced
                    } else {
                        Confidence::Established
                    },
                    EvidenceSource::Verified,
                    format!("Verified ({})", entry.evaluation),
                    decision.reasons,
                    entry.checks.clone(),
                    reasoning,
                    Some(entry.provenance.clone()),
                    reuse == Reuse::Reduced,
                );
            }
        }
    }
    if let Some((evidence, decision)) = &local_decision {
        match decision.reuse {
            Reuse::Stale => {
                stale_reasons.extend(
                    decision
                        .reasons
                        .iter()
                        .map(|reason| format!("the last calibration no longer applies: {reason}")),
                );
                return build(
                    ProfileStatus::Provisional,
                    Confidence::Untested,
                    EvidenceSource::None,
                    "Calibration out of date".into(),
                    stale_reasons,
                    Vec::new(),
                    reasoning,
                    Some(evidence.provenance.clone()),
                    true,
                );
            }
            reuse => {
                let reasoning = observed_reasoning(reasoning, &evidence.reasoning);
                return build(
                    evidence.status,
                    if reuse == Reuse::Reduced {
                        Confidence::Reduced
                    } else {
                        Confidence::Preliminary
                    },
                    EvidenceSource::LocalCalibration,
                    "Locally calibrated".into(),
                    decision.reasons.clone(),
                    evidence.checks.clone(),
                    reasoning,
                    Some(evidence.provenance.clone()),
                    reuse == Reuse::Reduced,
                );
            }
        }
    }
    build(
        ProfileStatus::Provisional,
        Confidence::Untested,
        EvidenceSource::None,
        "New model detected".into(),
        stale_reasons,
        Vec::new(),
        reasoning,
        None,
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_registry_parses() {
        assert!(parse_verified(include_str!("../verified-models.json")).is_ok());
    }

    #[test]
    fn backend_versions_compare_by_component() {
        assert!(matches!(
            backend_version_change("mlx-lm 0.31.3; mlx 0.32.0", "mlx-lm 0.31.4; mlx 0.32.0"),
            VersionChange::Minor(_)
        ));
        assert!(matches!(
            backend_version_change("mlx-lm 0.31.3; mlx 0.32.0", "mlx-lm 0.32.0; mlx 0.32.0"),
            VersionChange::Major(_)
        ));
        assert!(matches!(
            backend_version_change("llama 1.4.0", "llama 2.0.1"),
            VersionChange::Major(_)
        ));
        assert!(matches!(
            backend_version_change("mlx-lm 0.31.3; sidecar abc", "mlx-lm 0.31.3; sidecar def"),
            VersionChange::Minor(_)
        ));
    }
}
