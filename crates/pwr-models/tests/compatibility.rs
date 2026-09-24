//! Unknown models: provisional by default, calibrated on this machine, limited
//! or incompatible only on evidence, and evidence reused only while its
//! provenance still matches.
use async_trait::async_trait;
use pwr_domain::{
    BackendState, DeploymentDescriptor, GenerationMetrics, ModelChunk, ModelDefinition,
    ModelInspection, ModelRequest, ReasoningBudgets, ReasoningCapability, ToolCall,
};
use pwr_models::calibration::{CalibrationError, MAX_REQUESTS, quick_calibrate};
use pwr_models::profile::{
    AssessInput, Assessment, Confidence, EvidenceStore, LocalEvidence, ProfileStatus, Provenance,
    VerifiedEntry, assess, static_incompatibility, template_reasoning,
};
use pwr_provider::{Cancel, ModelProvider, ModelStream, ProviderError};
use serde_json::json;
use std::sync::Mutex;

fn inspection(metadata: serde_json::Value) -> ModelInspection {
    ModelInspection {
        definition: ModelDefinition {
            schema_version: 1,
            id: pwr_domain::new_id(),
            digest: "mlx:abc".into(),
            family: Some("qwen3".into()),
            quantization: Some("4bit".into()),
            capabilities: Default::default(),
            metadata,
            provenance: pwr_domain::Provenance {
                source: "test".into(),
                observed_at: chrono::Utc::now(),
                content_hash: "mlx:abc".into(),
            },
        },
        deployment: DeploymentDescriptor {
            schema_version: 1,
            id: pwr_domain::new_id(),
            provider: "mlx".into(),
            endpoint: String::new(),
            model_ref: "someone/New-Model-4bit".into(),
            backend_options: Default::default(),
            auth_ref: None,
        },
    }
}

/// A model with a Qwen-style template: `<think>` delimiters, switchable.
fn thinking_model() -> ModelInspection {
    inspection(json!({
        "format": "mlx",
        "has_chat_template": true,
        "tokenizer_fingerprint": "tok1",
        "chat_template_fingerprint": "tpl1",
        "weights_fingerprint": "w1",
        "weights_bytes": 5_000_000_000u64,
        "reasoning_template": {"delimiters": ["<think>", "</think>"], "switchable": true,
                               "native_budget": false, "effort_levels": false},
        "reasoning_capability": "explicit_thinking_stream",
    }))
}

fn provenance(inspection: &ModelInspection) -> Provenance {
    Provenance::of(
        inspection,
        "mlx",
        Some("mlx-lm 0.31.3; mlx 0.32.0; sidecar aaa".into()),
        Some("macos-arm64-apple-m2-64gb".into()),
    )
}

fn assessed(
    inspection: &ModelInspection,
    current: Provenance,
    local: Option<&LocalEvidence>,
    verified: &[VerifiedEntry],
) -> Assessment {
    assess(AssessInput {
        current,
        reasoning: template_reasoning(inspection, false, None),
        incompatible: static_incompatibility(inspection),
        verified,
        local,
    })
}

/// Answers each request from its content, the way a well-behaved model
/// would, and counts the requests.
struct Model {
    calls_tools: bool,
    uses_results: bool,
    fails: bool,
    finalizes: bool,
    requests: Mutex<Vec<ModelRequest>>,
}

impl Model {
    fn good() -> Self {
        Self {
            calls_tools: true,
            uses_results: true,
            fails: false,
            finalizes: true,
            requests: Mutex::new(Vec::new()),
        }
    }
}

fn answer(text: &str) -> ModelChunk {
    ModelChunk {
        content: text.into(),
        done: true,
        metrics: Some(GenerationMetrics {
            generated_tokens: Some(4),
            ..Default::default()
        }),
        ..Default::default()
    }
}

#[async_trait]
impl ModelProvider for Model {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!()
    }
    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!()
    }
    async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
        self.requests.lock().unwrap().push(request.clone());
        if self.fails {
            return Err(ProviderError::Protocol {
                safe_context: "unsupported model_type".into(),
            });
        }
        let last = request.messages.last().unwrap();
        let prompt = request.messages[0].content.as_str();
        let budget = request
            .sampling
            .get("reasoning_budget")
            .and_then(serde_json::Value::as_u64);
        let chunk = if last.role == "tool" {
            answer(if self.uses_results {
                "parse() returns 1337."
            } else {
                "I cannot tell."
            })
        } else if prompt.contains("READY") {
            answer("READY")
        } else if prompt.contains("JSON") {
            answer(r#"{"file": "src/parser.rs", "line": 7}"#)
        } else if prompt.contains("double(21)") {
            answer("42")
        } else if prompt.contains("wrong order") {
            answer("src/parser.rs")
        } else if prompt.contains("Show me") {
            if self.calls_tools {
                ModelChunk {
                    tool_calls: vec![ToolCall {
                        name: "read_file".into(),
                        arguments: json!({"path": "src/parser.rs"}),
                        id: Some("c".into()),
                    }],
                    done: true,
                    ..Default::default()
                }
            } else {
                answer("Here is what I think the file contains.")
            }
        } else if prompt.contains("17 multiplied") {
            ModelChunk {
                content: "51".into(),
                thinking: Some("17*3=51".into()),
                done: true,
                metrics: Some(GenerationMetrics {
                    reasoning_tokens: Some(12),
                    reasoning_budget_reached: Some(false),
                    ..Default::default()
                }),
                ..Default::default()
            }
        } else if prompt.contains("prime") {
            assert_eq!(budget, Some(16), "the forced probe uses a tiny budget");
            if !self.finalizes {
                return Ok(Box::pin(futures_util::stream::iter([Err(
                    ProviderError::ReasoningUnfinished {
                        safe_context: "no answer followed".into(),
                    },
                )])));
            }
            ModelChunk {
                content: "97".into(),
                thinking: Some("91 = 7*13".into()),
                done: true,
                metrics: Some(GenerationMetrics {
                    reasoning_tokens: Some(16),
                    reasoning_budget_reached: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }
        } else {
            panic!("unexpected calibration prompt: {prompt}");
        };
        Ok(Box::pin(futures_util::stream::iter([Ok(chunk)])))
    }
}

async fn calibrate(model: &Model, inspection: &ModelInspection) -> LocalEvidence {
    let reasoning = template_reasoning(inspection, false, None);
    quick_calibrate(
        model,
        inspection,
        provenance(inspection),
        &reasoning,
        &Cancel::new(),
        &mut |_: usize, _: usize, _: &str| {},
    )
    .await
    .expect("not cancelled")
}

#[test]
fn an_unknown_model_is_provisional_and_usable_not_unsupported() {
    let model = thinking_model();
    let assessment = assessed(&model, provenance(&model), None, &[]);
    assert_eq!(assessment.status, ProfileStatus::Provisional);
    assert_eq!(assessment.confidence, Confidence::Untested);
    assert!(assessment.features.chat && assessment.features.agent);
    assert!(assessment.recalibrate);
    // Conservative reasoning: the template says the phase can be bounded, but
    // nothing has shown an answer follows a forced close.
    assert_eq!(
        assessment.reasoning.capability,
        ReasoningCapability::ExplicitThinkingStream
    );
    assert_eq!(assessment.reasoning.finalization_verified, None);
    assert_eq!(
        assessment.reasoning.budgets().0,
        ReasoningBudgets::CONSERVATIVE
    );
    // Nothing tested is claimed.
    for line in &assessment.capabilities {
        assert!(
            ["not_tested", "provisional"].contains(&line.result.as_str()),
            "{line:?}"
        );
    }
}

#[test]
fn a_model_with_no_template_markers_has_unknown_reasoning_not_none() {
    let model = inspection(json!({"format": "mlx", "has_chat_template": true}));
    let assessment = assessed(&model, provenance(&model), None, &[]);
    assert_eq!(
        assessment.reasoning.capability,
        ReasoningCapability::Unknown
    );
    assert!(!assessment.reasoning.effort_applies());
}

#[tokio::test]
async fn quick_calibration_of_a_capable_model_makes_it_locally_calibrated() {
    let model = thinking_model();
    let provider = Model::good();
    let evidence = calibrate(&provider, &model).await;
    assert_eq!(
        evidence.status,
        ProfileStatus::LocallyCalibrated,
        "{evidence:?}"
    );
    assert!(
        evidence
            .checks
            .iter()
            .all(|check| check.passed == Some(true))
    );
    assert!(provider.requests.lock().unwrap().len() <= MAX_REQUESTS);
    // Reproducible: temperature 0 and a fixed seed on every request.
    for request in provider.requests.lock().unwrap().iter() {
        assert_eq!(request.sampling["temperature"], 0);
        assert!(request.seed.is_some());
    }
    assert_eq!(evidence.reasoning.emitted, Some(true));
    assert_eq!(evidence.reasoning.finalization_after_budget, Some(true));

    let assessment = assessed(&model, provenance(&model), Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::LocallyCalibrated);
    assert_eq!(assessment.confidence, Confidence::Preliminary);
    assert_eq!(assessment.reasoning.finalization_verified, Some(true));
    assert_eq!(
        assessment.reasoning.budgets().0,
        ReasoningBudgets::CALIBRATED
    );
    let line = |label: &str| {
        assessment
            .capabilities
            .iter()
            .find(|line| line.label == label)
            .unwrap()
            .result
            .clone()
    };
    assert_eq!(line("Tool calling"), "supported");
    assert_eq!(line("Reasoning mode"), "detected");
    // Quick Calibration never measures long context.
    assert_eq!(line("Context"), "provisional");
}

#[tokio::test]
async fn a_model_that_cannot_call_tools_is_limited_and_keeps_chat() {
    let model = thinking_model();
    let provider = Model {
        calls_tools: false,
        ..Model::good()
    };
    let evidence = calibrate(&provider, &model).await;
    assert_eq!(evidence.status, ProfileStatus::Limited);
    let assessment = assessed(&model, provenance(&model), Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::Limited);
    assert!(assessment.features.chat);
    assert!(!assessment.features.agent);
    assert!(assessment.features.note.unwrap().contains("Tool calling"));
    let tools = assessment
        .capabilities
        .iter()
        .find(|line| line.label == "Tool calling")
        .unwrap();
    assert_eq!(tools.result, "not_reliable");
}

#[tokio::test]
async fn a_model_that_ignores_tool_results_is_limited() {
    let model = thinking_model();
    let provider = Model {
        uses_results: false,
        ..Model::good()
    };
    assert_eq!(
        calibrate(&provider, &model).await.status,
        ProfileStatus::Limited
    );
}

#[tokio::test]
async fn a_model_that_generates_nothing_is_incompatible() {
    let model = thinking_model();
    let provider = Model {
        fails: true,
        ..Model::good()
    };
    let evidence = calibrate(&provider, &model).await;
    assert_eq!(evidence.status, ProfileStatus::Incompatible);
    assert!(
        evidence
            .reason
            .as_deref()
            .unwrap()
            .contains("unsupported model_type")
    );
    let assessment = assessed(&model, provenance(&model), Some(&evidence), &[]);
    assert!(!assessment.features.chat && !assessment.features.agent);
}

#[tokio::test]
async fn a_forced_close_with_no_answer_disables_the_budget_rather_than_pretending() {
    let model = thinking_model();
    let provider = Model {
        finalizes: false,
        ..Model::good()
    };
    let evidence = calibrate(&provider, &model).await;
    assert_eq!(evidence.reasoning.finalization_after_budget, Some(false));
    // Reasoning is not an agent-critical check: the model stays usable.
    assert_eq!(evidence.status, ProfileStatus::LocallyCalibrated);
    let assessment = assessed(&model, provenance(&model), Some(&evidence), &[]);
    assert!(!assessment.reasoning.budget_enforceable());
    assert!(!assessment.reasoning.effort_applies());
}

#[test]
fn a_model_without_a_chat_template_is_incompatible_before_anything_runs() {
    let model = inspection(json!({"format": "mlx", "has_chat_template": false}));
    let assessment = assessed(&model, provenance(&model), None, &[]);
    assert_eq!(assessment.status, ProfileStatus::Incompatible);
    assert!(assessment.reasons[0].contains("chat template"));
    // GGUF: llama.cpp has a fallback template, so the same fact is not proof.
    let gguf = inspection(json!({"format": "gguf", "has_chat_template": false}));
    assert_eq!(static_incompatibility(&gguf), None);
}

#[tokio::test]
async fn calibration_is_cancellable_and_records_nothing() {
    let model = thinking_model();
    let cancel = Cancel::new();
    cancel.cancel();
    let reasoning = template_reasoning(&model, false, None);
    let outcome = quick_calibrate(
        &Model::good(),
        &model,
        provenance(&model),
        &reasoning,
        &cancel,
        &mut |_: usize, _: usize, _: &str| {},
    )
    .await;
    assert_eq!(outcome.unwrap_err(), CalibrationError::Cancelled);
}

/// Cancelling while a generation is in flight stops it without waiting for
/// its timeout.
#[tokio::test]
async fn cancelling_mid_generation_returns_promptly() {
    struct Hanging;
    #[async_trait]
    impl ModelProvider for Hanging {
        async fn inspect(
            &self,
            _: &DeploymentDescriptor,
        ) -> Result<ModelInspection, ProviderError> {
            unreachable!()
        }
        async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
            unreachable!()
        }
        async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
            Ok(Box::pin(futures_util::stream::pending()))
        }
    }
    let model = thinking_model();
    let cancel = Cancel::new();
    let trigger = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        trigger.cancel();
    });
    let reasoning = template_reasoning(&model, false, None);
    let started = std::time::Instant::now();
    let outcome = quick_calibrate(
        &Hanging,
        &model,
        provenance(&model),
        &reasoning,
        &cancel,
        &mut |_: usize, _: usize, _: &str| {},
    )
    .await;
    assert_eq!(outcome.unwrap_err(), CalibrationError::Cancelled);
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
}

async fn calibrated() -> (ModelInspection, LocalEvidence) {
    let model = thinking_model();
    let evidence = calibrate(&Model::good(), &model).await;
    (model, evidence)
}

#[tokio::test]
async fn a_changed_quantization_or_template_makes_the_calibration_stale() {
    let (model, evidence) = calibrated().await;
    for change in [
        |p: &mut Provenance| p.quantization = Some("8bit".into()),
        |p: &mut Provenance| p.chat_template_fingerprint = Some("tpl2".into()),
        |p: &mut Provenance| p.artifact_digest = Some("mlx:other".into()),
        |p: &mut Provenance| p.backend = "llama".into(),
        |p: &mut Provenance| p.calibration_version = "quick-calibration-9".into(),
    ] {
        let mut current = provenance(&model);
        change(&mut current);
        let assessment = assessed(&model, current, Some(&evidence), &[]);
        assert_eq!(assessment.status, ProfileStatus::Provisional);
        assert!(assessment.recalibrate);
        assert!(
            assessment.reasons[0].contains("no longer applies"),
            "{:?}",
            assessment.reasons
        );
        // The stale evidence is named, not silently dropped.
        assert!(assessment.evidence_provenance.is_some());
    }
}

#[tokio::test]
async fn a_backend_update_lowers_confidence_and_a_major_one_voids_it() {
    let (model, evidence) = calibrated().await;
    let mut patched = provenance(&model);
    patched.backend_version = Some("mlx-lm 0.31.4; mlx 0.32.0; sidecar aaa".into());
    let assessment = assessed(&model, patched, Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::LocallyCalibrated);
    assert_eq!(assessment.confidence, Confidence::Reduced);
    assert!(assessment.recalibrate);

    let mut major = provenance(&model);
    major.backend_version = Some("mlx-lm 0.40.0; mlx 0.32.0; sidecar aaa".into());
    let assessment = assessed(&model, major, Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::Provisional);
}

#[tokio::test]
async fn harmless_changes_do_not_invalidate() {
    let (model, evidence) = calibrated().await;
    let mut current = provenance(&model);
    current.pwr_version = "9.9.9".into();
    current.artifact_path = Some("/elsewhere".into());
    current.observed_at = "later".into();
    let assessment = assessed(&model, current, Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::LocallyCalibrated);
    assert_eq!(assessment.confidence, Confidence::Preliminary);

    let mut other_machine = provenance(&model);
    other_machine.hardware_class = Some("macos-arm64-apple-m4-16gb".into());
    let assessment = assessed(&model, other_machine, Some(&evidence), &[]);
    assert_eq!(assessment.status, ProfileStatus::LocallyCalibrated);
    assert_eq!(assessment.confidence, Confidence::Reduced);
}

#[tokio::test]
async fn verified_evidence_applies_only_to_its_artifact_and_a_local_failure_outranks_it() {
    let (model, evidence) = calibrated().await;
    let entry = VerifiedEntry {
        provenance: provenance(&model),
        evaluation: "fixture suite".into(),
        checks: evidence.checks.clone(),
        reasoning: evidence.reasoning.clone(),
        reasoning_budgets: None,
    };
    let assessment = assessed(
        &model,
        provenance(&model),
        None,
        std::slice::from_ref(&entry),
    );
    assert_eq!(assessment.status, ProfileStatus::Verified);
    assert_eq!(assessment.confidence, Confidence::Established);

    let mut other = provenance(&model);
    other.artifact_digest = Some("mlx:another-quant".into());
    let assessment = assessed(&model, other, None, std::slice::from_ref(&entry));
    assert_eq!(assessment.status, ProfileStatus::Provisional);

    let limited = LocalEvidence {
        status: ProfileStatus::Limited,
        reason: Some("tool calls failed here".into()),
        ..evidence
    };
    let assessment = assessed(
        &model,
        provenance(&model),
        Some(&limited),
        std::slice::from_ref(&entry),
    );
    assert_eq!(assessment.status, ProfileStatus::Limited);
}

#[tokio::test]
async fn evidence_is_stored_outside_the_repository_and_read_back_by_key() {
    let (_, evidence) = calibrated().await;
    let dir = tempfile::tempdir().unwrap();
    let store = EvidenceStore::new(dir.path());
    store.save(&evidence).unwrap();
    let loaded = store
        .load("mlx", "someone/New-Model-4bit")
        .expect("saved evidence reads back");
    assert_eq!(loaded, evidence);
    assert!(store.load("mlx", "someone/Other").is_none());
    assert!(store.load("llama", "someone/New-Model-4bit").is_none());
    // A model reference is never a path: the file name is a hash.
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 1);
    assert!(!names[0].contains("New-Model"));
}

/// A request that times out fails its check; the calibration goes on rather
/// than reading the timeout as the person cancelling.
#[tokio::test(start_paused = true)]
async fn a_timed_out_request_fails_its_check_and_the_rest_still_run() {
    struct SlowFirst {
        inner: Model,
        first: Mutex<bool>,
    }
    #[async_trait]
    impl ModelProvider for SlowFirst {
        async fn inspect(
            &self,
            _: &DeploymentDescriptor,
        ) -> Result<ModelInspection, ProviderError> {
            unreachable!()
        }
        async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
            unreachable!()
        }
        async fn chat(&self, request: ModelRequest) -> Result<ModelStream, ProviderError> {
            if std::mem::replace(&mut *self.first.lock().unwrap(), false) {
                return Ok(Box::pin(futures_util::stream::pending()));
            }
            self.inner.chat(request).await
        }
    }
    let model = thinking_model();
    let provider = SlowFirst {
        inner: Model::good(),
        first: Mutex::new(true),
    };
    let reasoning = template_reasoning(&model, false, None);
    let evidence = quick_calibrate(
        &provider,
        &model,
        provenance(&model),
        &reasoning,
        &Cancel::new(),
        &mut |_: usize, _: usize, _: &str| {},
    )
    .await
    .expect("a timeout is a result, not a cancellation");
    let termination = evidence
        .checks
        .iter()
        .find(|check| check.name == "termination")
        .unwrap();
    assert_eq!(termination.passed, Some(false));
    assert!(termination.detail.contains("no reply within"));
    assert!(
        evidence
            .checks
            .iter()
            .find(|check| check.name == "tool_selection")
            .unwrap()
            .passed
            == Some(true)
    );
    // Termination is agent-critical, so one hung request limits the model.
    assert_eq!(evidence.status, ProfileStatus::Limited);
}

/// gpt-oss (harmony) cannot switch reasoning off: its packaged profile's
/// `think: false` must not disable the native low/medium/high levels.
#[test]
fn a_profile_cannot_switch_off_a_model_whose_reasoning_only_takes_levels() {
    let harmony = inspection(json!({
        "format": "mlx",
        "has_chat_template": true,
        "reasoning_template": {"switchable": false, "native_budget": false, "effort_levels": true},
        "reasoning_capability": "template_controlled",
    }));
    let reasoning = template_reasoning(&harmony, true, None);
    assert!(!reasoning.disabled_by_profile);
    assert!(reasoning.effort_applies());
    // A template that can switch thinking off still honours the profile.
    let qwen = thinking_model();
    assert!(template_reasoning(&qwen, true, None).disabled_by_profile);
}
