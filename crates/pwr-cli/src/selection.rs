//! Explainable automatic model selection, and the certification evidence it
//! consumes.
//!
//! Everything here is assembly and explanation. The decision itself lives in
//! `pwr_domain::SelectionRequest::select`, which is deterministic and
//! refuses to admit an unknown: a candidate whose context, memory or tool
//! support was never observed is rejected with a reason rather than tried and
//! hoped for. This module's job is to make sure the facts handed to it were
//! observed rather than assumed.

use pwr_domain::{
    CalibrationProfile, CandidateEvidence, CertificationLevel, CertificationRegistry,
    CertificationReport, CertificationScope, DeploymentDescriptor, HardwareProfile,
    ModelInspection, ModelProfile, Observation, SelectionCandidate, check_schema_version,
};
use pwr_provider::DiscoveredModel;
use std::path::{Path, PathBuf};

/// Where certification records live, relative to the working directory.
pub const CERTIFICATION_FILE: &str = "strategies/certifications.json";

/// A registry that has never been written is an empty one, not an error: a
/// workspace with no certifications is the normal starting state, and the
/// selector's answer for it -- "nothing here is certified" -- is correct.
pub fn load_certification_registry(path: &Path) -> Result<CertificationRegistry, String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(CertificationRegistry {
            schema_version: 1,
            records: Vec::new(),
        });
    };
    let registry: CertificationRegistry = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "{} is not a certification registry: {error}",
            path.display()
        )
    })?;
    registry.validate().map_err(|error| error.to_string())?;
    Ok(registry)
}

pub fn write_certification_registry(
    path: &Path,
    registry: &CertificationRegistry,
) -> Result<(), String> {
    registry.validate().map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut bytes = serde_json::to_vec_pretty(registry).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

/// Everything that can invalidate a result, assembled from live observation.
///
/// A backend that publishes no version cannot be part of a scope, and that is
/// deliberate: a certification that survives a backend upgrade is a badge for
/// a build nobody measured.
pub fn certification_scope(
    model_digest: &str,
    deployment: &DeploymentDescriptor,
    backend_version: Option<&str>,
    adapter_version: &str,
    hardware: &HardwareProfile,
    harness_rev: &str,
) -> Result<CertificationScope, String> {
    let backend_version = backend_version.ok_or_else(|| {
        format!(
            "the {} backend publishes no version, so a result measured here cannot be scoped to a \
             backend build; certification is refused rather than attributed to an unknown one",
            deployment.provider
        )
    })?;
    let scope = CertificationScope {
        model_digest: model_digest.to_owned(),
        deployment_fingerprint: deployment.fingerprint(),
        backend_version: backend_version.to_owned(),
        adapter_version: adapter_version.to_owned(),
        hardware_compatibility_key: hardware.compatibility_key.clone(),
        harness_rev: harness_rev.to_owned(),
    };
    scope.validate().map_err(|error| error.to_string())?;
    Ok(scope)
}

pub fn certification_report(
    registry: &CertificationRegistry,
    scope: &CertificationScope,
) -> Result<CertificationReport, String> {
    registry.report(scope).map_err(|error| error.to_string())
}

/// The level in force for a scope.
///
/// No matching record is `Experimental`, never `Compatible`: a deployment
/// nobody has measured on this backend, adapter, hardware and harness has
/// limited evidence by definition, and the selector then requires an explicit
/// opt-in before using it.
pub fn effective_level(report: Option<&CertificationReport>) -> CertificationLevel {
    report
        .and_then(|report| report.effective.as_ref())
        .map(|record| record.level)
        .unwrap_or(CertificationLevel::Experimental)
}

/// Facts about one discovered deployment, gathered from what is on disk.
pub struct CandidateFacts {
    pub discovered: DiscoveredModel,
    pub deployment: DeploymentDescriptor,
    pub inspection: Option<ModelInspection>,
    pub calibration: Option<CalibrationProfile>,
    pub certification: CertificationLevel,
    pub profile_id: Option<String>,
}

impl CandidateFacts {
    /// Turns observed facts into a candidate, leaving every unobserved one
    /// unknown.
    ///
    /// The temptation here is to read a backend's declaration -- a tag that
    /// says "tools" -- as evidence of tool support. It is not: only a probe
    /// that executed a tool call observed one, which is why capability comes
    /// from the persisted inspection and is `Unknown` without it.
    pub fn into_candidate(self) -> SelectionCandidate {
        let capability = |name: &str| match self
            .inspection
            .as_ref()
            .and_then(|inspection| inspection.definition.capabilities.get(name))
        {
            Some(Observation::Observed(value)) => Observation::Observed(match value {
                // A probe records what it saw; the selector asks a yes/no
                // question, and a recorded observation of any shape is a yes.
                serde_json::Value::Bool(answer) => serde_json::json!(answer),
                _ => serde_json::json!(true),
            }),
            Some(unknown @ Observation::Unknown { .. }) => unknown.clone(),
            None => Observation::Unknown {
                reason: format!("{name} was never probed for this deployment"),
            },
        };
        let evidence = match &self.calibration {
            Some(calibration) => {
                let best = calibration
                    .stable_points
                    .iter()
                    .filter(|point| calibration.thresholds.admits(point))
                    .map(|point| point.generation_tokens_per_second)
                    .fold(0.0_f64, f64::max);
                CandidateEvidence {
                    // Speed is measured; quality is not measured here and is
                    // left absent rather than defaulted, so a fast deployment
                    // never scores as a good one for want of a benchmark.
                    quality_score: None,
                    speed_score: Some(best.round().clamp(0.0, f64::from(u16::MAX)) as u16),
                    calibrated: true,
                }
            }
            None => CandidateEvidence::default(),
        };
        SelectionCandidate {
            model_digest: self.discovered.digest.clone().or_else(|| {
                self.inspection
                    .as_ref()
                    .map(|inspection| inspection.definition.digest.clone())
            }),
            profile_id: self.profile_id,
            // The backend's declared ceiling. A calibration measures where the
            // deployment is actually stable, which is narrower, and the run
            // path already prefers it; admission only needs to know the tier
            // is reachable at all.
            context_limit_tokens: self.discovered.context_limit,
            estimated_model_memory_bytes: self.discovered.size_bytes,
            supports_tools: capability("structured_tools"),
            supports_streaming: capability("streaming"),
            certification: self.certification,
            evidence,
            deployment: self.deployment,
        }
    }
}

/// Reads the newest capability evidence for one deployment, or nothing.
///
/// Deliberately quieter than the run path's loader: selection is allowed to
/// consider a deployment with no evidence, and then reject it saying so. The
/// run path must refuse outright, which is a different job.
pub fn stored_inspection(
    directories: &[PathBuf],
    deployment: &DeploymentDescriptor,
    digest: Option<&str>,
) -> Option<ModelInspection> {
    const MAX_ARTIFACT_BYTES: u64 = 64 * 1024 * 1024;
    let fingerprint = deployment.fingerprint();
    let mut found: Vec<ModelInspection> = Vec::new();
    for directory in directories {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if !metadata.is_file() || metadata.len() > MAX_ARTIFACT_BYTES {
                continue;
            }
            let Ok(bytes) = std::fs::read(entry.path()) else {
                continue;
            };
            let Ok(inspection) = serde_json::from_slice::<ModelInspection>(&bytes) else {
                continue;
            };
            if check_schema_version(inspection.definition.schema_version, "capability evidence")
                .is_err()
            {
                continue;
            }
            if inspection.deployment.fingerprint() != fingerprint {
                continue;
            }
            // A digest the backend did not report cannot be matched on, so the
            // fingerprint carries the identity alone. Where the backend did
            // report one, a mismatch means the tag now serves other weights.
            if digest.is_some_and(|digest| inspection.definition.digest != digest) {
                continue;
            }
            found.push(inspection);
        }
    }
    found.sort_by_key(|inspection| inspection.definition.provenance.observed_at);
    found.pop()
}

/// The newest calibration matching this deployment exactly.
pub fn stored_calibration(
    directory: &Path,
    digest: Option<&str>,
    deployment: &DeploymentDescriptor,
    harness_rev: &str,
) -> Option<(PathBuf, CalibrationProfile)> {
    let fingerprint = deployment.fingerprint();
    let mut found: Vec<(PathBuf, CalibrationProfile)> = Vec::new();
    let entries = std::fs::read_dir(directory).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            continue;
        };
        // Calibration artifacts wrap the profile alongside their samples; a
        // bare profile is also accepted, as the run path accepts one.
        let candidate = value.get("profile").cloned().unwrap_or(value);
        let Ok(profile) = serde_json::from_value::<CalibrationProfile>(candidate) else {
            continue;
        };
        if profile.deployment_fingerprint == fingerprint
            && profile.harness_rev == harness_rev
            && digest.is_none_or(|digest| profile.model_digest == digest)
        {
            found.push((path, profile));
        }
    }
    found.sort_by_key(|(_, profile)| profile.created_at);
    found.pop()
}

/// The declared profile that applies to a deployment, by its own selector
/// precedence. Reported as an identifier so a selection says which policy it
/// would run under.
pub fn profile_id_for(
    profiles: &[ModelProfile],
    deployment: &DeploymentDescriptor,
    inspection: Option<&ModelInspection>,
) -> Option<String> {
    let identity = match inspection {
        Some(inspection) => {
            pwr_domain::DeploymentIdentity::from_inspection(deployment, &inspection.definition)
        }
        None => pwr_domain::DeploymentIdentity {
            provider: deployment.provider.clone(),
            model_ref: deployment.model_ref.clone(),
            digest: None,
            family: None,
        },
    };
    ModelProfile::select_for(profiles, &identity).map(|profile| profile.model_selector.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pwr_domain::{ModelDefinition, Provenance, new_id, now};
    use std::collections::BTreeMap;

    fn deployment() -> DeploymentDescriptor {
        DeploymentDescriptor {
            schema_version: 1,
            id: new_id(),
            provider: "ollama".into(),
            endpoint: "http://127.0.0.1:11434/".into(),
            model_ref: "qwen3:8b".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        }
    }

    fn inspection(capabilities: BTreeMap<String, Observation>) -> ModelInspection {
        ModelInspection {
            definition: ModelDefinition {
                schema_version: 1,
                id: new_id(),
                digest: "digest".into(),
                family: Some("qwen".into()),
                quantization: None,
                capabilities,
                metadata: serde_json::json!({}),
                provenance: Provenance {
                    source: "test".into(),
                    observed_at: now(),
                    content_hash: "hash".into(),
                },
            },
            deployment: deployment(),
        }
    }

    fn facts(inspection: Option<ModelInspection>) -> CandidateFacts {
        CandidateFacts {
            discovered: DiscoveredModel {
                model_ref: "qwen3:8b".into(),
                digest: Some("digest".into()),
                context_limit: Some(32_768),
                size_bytes: Some(5_000_000_000),
                accelerator_size_bytes: None,
            },
            deployment: deployment(),
            inspection,
            calibration: None,
            certification: CertificationLevel::Compatible,
            profile_id: None,
        }
    }

    #[test]
    fn a_capability_nobody_probed_is_unknown_and_not_a_no() {
        let candidate = facts(None).into_candidate();
        // Unknown, so the selector rejects it with a reason. Reported as
        // `false` it would look like a measured absence, and a deployment that
        // does support tools would be excluded on evidence nobody gathered.
        assert!(matches!(
            candidate.supports_tools,
            Observation::Unknown { .. }
        ));
        assert!(matches!(
            candidate.supports_streaming,
            Observation::Unknown { .. }
        ));
    }

    #[test]
    fn an_observed_capability_reaches_the_selector_as_observed() {
        let candidate = facts(Some(inspection(BTreeMap::from([
            (
                "structured_tools".to_string(),
                Observation::Observed(serde_json::json!({"trials": 3})),
            ),
            (
                "streaming".to_string(),
                Observation::Observed(serde_json::json!(true)),
            ),
        ]))))
        .into_candidate();
        assert_eq!(
            candidate.supports_tools,
            Observation::Observed(serde_json::json!(true))
        );
        assert_eq!(
            candidate.supports_streaming,
            Observation::Observed(serde_json::json!(true))
        );
    }

    #[test]
    fn a_probe_that_observed_a_no_is_carried_as_that_no() {
        let candidate = facts(Some(inspection(BTreeMap::from([(
            "structured_tools".to_string(),
            Observation::Observed(serde_json::json!(false)),
        )]))))
        .into_candidate();
        assert_eq!(
            candidate.supports_tools,
            Observation::Observed(serde_json::json!(false))
        );
    }

    #[test]
    fn measured_speed_never_becomes_a_quality_claim() {
        let mut candidate_facts = facts(None);
        candidate_facts.calibration = Some(CalibrationProfile {
            schema_version: 1,
            id: new_id(),
            compatibility_key: "key".into(),
            model_digest: "digest".into(),
            deployment_fingerprint: deployment().fingerprint(),
            harness_rev: "rev".into(),
            thresholds: Default::default(),
            stable_points: vec![pwr_domain::StablePoint {
                context_tokens: 32_768,
                samples: 3,
                success_rate: 1.0,
                median_first_token_ms: 900.0,
                generation_tokens_per_second: 27.3,
                variance: 1.0,
                memory_pressure_observed: false,
            }],
            raw_artifact_hashes: vec!["hash".into()],
            created_at: now(),
        });
        let candidate = candidate_facts.into_candidate();
        assert_eq!(candidate.evidence.speed_score, Some(27));
        assert!(candidate.evidence.calibrated);
        // Nothing measured quality, so nothing claims it.
        assert_eq!(candidate.evidence.quality_score, None);
    }

    #[test]
    fn a_scope_cannot_be_formed_without_a_backend_version() {
        let hardware = HardwareProfile {
            schema_version: 1,
            id: new_id(),
            compatibility_key: "key".into(),
            os: "Darwin".into(),
            architecture: "arm64".into(),
            cpu: "Apple M2 Max".into(),
            accelerators: vec![],
            total_memory_bytes: Some(64),
            storage_free_bytes: Some(64),
            unavailable_fields: vec![],
            probe_version: "test".into(),
            provenance: Provenance {
                source: "test".into(),
                observed_at: now(),
                content_hash: "hash".into(),
            },
        };
        assert!(
            certification_scope("digest", &deployment(), None, "qwen-v1", &hardware, "rev")
                .is_err()
        );
        assert!(
            certification_scope(
                "digest",
                &deployment(),
                Some("0.33.3"),
                "qwen-v1",
                &hardware,
                "rev"
            )
            .is_ok()
        );
    }

    #[test]
    fn a_deployment_with_no_record_is_experimental_rather_than_compatible() {
        assert_eq!(effective_level(None), CertificationLevel::Experimental);
    }

    #[test]
    fn an_absent_registry_reads_as_empty_rather_than_failing() {
        let registry =
            load_certification_registry(Path::new("no/such/certifications.json")).unwrap();
        assert!(registry.records.is_empty());
    }

    #[test]
    fn a_newer_regression_demotes_an_older_certification() {
        use pwr_domain::CertificationRecord;
        let scope = CertificationScope {
            model_digest: "digest".into(),
            deployment_fingerprint: deployment().fingerprint(),
            backend_version: "0.33.3".into(),
            adapter_version: "qwen-v1".into(),
            hardware_compatibility_key: "key".into(),
            harness_rev: "rev".into(),
        };
        let record = |level, issued_at| CertificationRecord {
            schema_version: 1,
            id: new_id(),
            scope: scope.clone(),
            level,
            evaluation_run_ids: vec![new_id()],
            artifact_hashes: vec!["hash".into()],
            rationale: "suite".into(),
            issued_at,
        };
        let earlier = now() - chrono::Duration::hours(1);
        let registry = CertificationRegistry {
            schema_version: 1,
            records: vec![
                record(CertificationLevel::Certified, earlier),
                record(CertificationLevel::Experimental, now()),
            ],
        };
        let report = certification_report(&registry, &scope).unwrap();
        // The badge does not survive the regression that invalidated it, and
        // the earlier evidence is still visible rather than deleted.
        assert_eq!(
            effective_level(Some(&report)),
            CertificationLevel::Experimental
        );
        assert_eq!(report.matching_records.len(), 2);
    }
}
