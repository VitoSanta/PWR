//! A context tier the backend will not serve must not be measured.
//!
//! Measured on LM Studio: the context window there is fixed when the model is
//! loaded, and the chat API has no per-request equivalent of it. A ladder run
//! against such a backend without checking what it granted produces a set of
//! stable points labelled 4096, 8192, 32768 -- every one of them measured at
//! whatever window the running instance happened to have. The numbers look
//! like a ladder and are one measurement repeated.

use async_trait::async_trait;
use pwr_domain::{
    BackendState, CalibrationThresholds, DeploymentDescriptor, HardwareProfile, ModelInspection,
    ModelRequest, Observation, Provenance, new_id, now,
};
use pwr_provider::{ModelProvider, ModelStream, ProviderError};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Serves one window whatever it is asked for, and counts generations.
struct FixedWindowBackend {
    serves: u32,
    generations: AtomicUsize,
}

#[async_trait]
impl ModelProvider for FixedWindowBackend {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("calibration does not inspect")
    }

    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        Ok(BackendState {
            observed_at: now(),
            loaded_models: vec![],
            state: serde_json::json!({}),
        })
    }

    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.generations.fetch_add(1, Ordering::SeqCst);
        Err(ProviderError::Unavailable {
            safe_context: "a tier that was never granted must not be generated at".into(),
        })
    }

    async fn prepare_context(
        &self,
        _: &DeploymentDescriptor,
        _: u32,
    ) -> Result<u32, ProviderError> {
        Ok(self.serves)
    }
}

struct QuietHost;

#[async_trait]
impl pwr_orchestrator::HostProbe for QuietHost {
    async fn memory_pressure(&self) -> Observation {
        Observation::Unknown {
            reason: "not probed in this fixture".into(),
        }
    }
}

fn deployment() -> DeploymentDescriptor {
    DeploymentDescriptor {
        schema_version: 1,
        id: new_id(),
        provider: "fixture".into(),
        endpoint: "http://127.0.0.1:1/".into(),
        model_ref: "fixture:1".into(),
        backend_options: Default::default(),
        auth_ref: None,
    }
}

fn hardware() -> HardwareProfile {
    HardwareProfile {
        schema_version: 1,
        id: new_id(),
        compatibility_key: "key".into(),
        os: "Darwin".into(),
        architecture: "arm64".into(),
        cpu: "fixture".into(),
        accelerators: vec![],
        total_memory_bytes: Some(64 * 1024 * 1024 * 1024),
        storage_free_bytes: Some(1024),
        unavailable_fields: vec![],
        probe_version: "fixture".into(),
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "hash".into(),
        },
    }
}

#[tokio::test]
async fn a_tier_the_backend_will_not_serve_is_refused_rather_than_measured() {
    let backend = FixedWindowBackend {
        serves: 262_144,
        generations: AtomicUsize::new(0),
    };
    let outcome = pwr_orchestrator::calibrate(
        &backend,
        &QuietHost,
        &deployment(),
        &hardware(),
        "digest".into(),
        &[4_096, 8_192],
        "rev",
        CalibrationThresholds::default(),
        1,
    )
    .await
    .expect("a refusal is a result, not an error");

    let report = serde_json::to_value(&outcome).unwrap();
    assert_eq!(report["outcome"], "refused");

    let rejected = report["rejected"].as_array().expect("rejected tiers");
    assert_eq!(rejected.len(), 2);
    for tier in rejected {
        assert_eq!(tier["reasons"][0], "context_window_not_granted");
        // The finding is the number it served instead. A rejection that only
        // says "not granted" cannot tell a clamp from an outage.
        assert_eq!(tier["granted_context_tokens"], 262_144);
        assert_eq!(tier["measured"]["samples"], 0);
    }

    // Nothing was generated. Sampling a tier that was never granted is how the
    // ladder came to hold one measurement under several labels.
    assert_eq!(backend.generations.load(Ordering::SeqCst), 0);
}
